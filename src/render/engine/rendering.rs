//! Frame rendering pipeline.
//!
//! Contains the core `render_frame()` function that assembles slide content
//! into a virtual buffer and writes it to the terminal, plus the optimized
//! `render_status_bar_only()` for timer-only updates.
//!
//! # How `render_frame()` works
//!
//! 1. **Font change handling** — If a font size change is pending, apply it
//!    before rendering (optionally with a scatter-dissolve transition).
//! 2. **Mode dispatch** — Help and Overview modes have their own dedicated
//!    renderers and return early.
//! 3. **Smart redraw check** — If nothing changed since the last frame, only
//!    the status bar is redrawn (avoids expensive image re-emission).
//! 4. **Content assembly** — Builds a `Vec<StyledLine>` virtual buffer from
//!    the slide's section label, title, subtitle, bullets, code blocks,
//!    tables, block quotes, columns, images, and diagrams.
//! 5. **Alignment** — Applies vertical and horizontal centering if requested.
//! 6. **Animation overlays** — Applies transition, entrance, or loop animations
//!    to the buffer.
//! 7. **Viewport calculation** — Clamps scroll offset, determines visible lines.
//! 8. **Terminal output** — Writes status bar, content, footer, notes panel,
//!    and command bar within a synchronized update block.
//! 9. **Protocol images** — Emits Kitty/iTerm2/Sixel escape data after text.
//! 10. **Dissolve-in** — If a font-change transition just completed, runs a
//!     scatter-reveal animation to smoothly bring in the new content.

use super::*;

impl Presenter {
    /// Render a complete frame to the terminal.
    ///
    /// This is the heart of the rendering engine. It is called on every input
    /// event, animation tick, timer update, or hot-reload change. The function
    /// is designed to be fast for the common case (smart redraw skips most work
    /// when only the timer changed) and thorough for the full case (rebuilds
    /// the entire virtual buffer and flushes atomically).
    ///
    /// See the module-level docs for a step-by-step breakdown of the pipeline.
    pub(crate) fn render_frame(&mut self) -> Result<()> {
        // --- Font Change and Dissolve Transition Handling ---
        // If font size is changing, apply it before rendering and re-query
        // dimensions.  The ioctl(TIOCGWINSZ) returns the new size
        // synchronously after Kitty processes the font change, so we don't
        // need to sleep or wait for SIGWINCH.  We also drain any resize
        // events already queued so the event loop doesn't re-render again.
        let font_changing = if let Some(target) = self.pending_font_size.take() {
            if self.last_applied_font_size != Some(target) {
                Some(target)
            } else {
                None
            }
        } else {
            None
        };

        // Font change BEFORE the sync block — the terminal resize triggered
        // by font changes must settle before we query dimensions and render.
        if let Some(target) = font_changing {
            self.apply_font_change(target)?;
        }

        // --- Begin Synchronized Update Block ---
        // Everything from here until EndSynchronizedUpdate is buffered and
        // sent to the terminal atomically, preventing visible flicker.
        let stdout = io::stdout();
        let mut w = BufWriter::with_capacity(256 * 1024, stdout.lock());
        queue!(w, BeginSynchronizedUpdate)?;

        // --- Mode Dispatch (Help / Overview early return) ---
        match self.mode {
            Mode::Help => {
                self.last_rendered_mode = Mode::Help;
                // Reset font to base for help readability
                if self.font_capability.is_available() {
                    if let Some(ref orig) = self.original_font_size {
                        if let Ok(base) = orig.parse::<f64>() {
                            if self.last_applied_font_size != Some(base) {
                                self.kitty_font_size_absolute(base, true);
                                std::thread::sleep(std::time::Duration::from_millis(30));
                                while event::poll(std::time::Duration::from_millis(10))? {
                                    if let Event::Resize(w2, h2) = event::read()? {
                                        self.width = w2;
                                        self.height = h2;
                                    } else { break; }
                                }
                                self.window_size = WindowSize::query();
                                self.width = self.window_size.columns;
                                self.height = self.window_size.rows;
                                self.last_applied_font_size = Some(base);
                            }
                        }
                    }
                }
                return self.render_help_buf(&mut w);
            }
            Mode::Overview => {
                self.last_rendered_mode = Mode::Overview;
                return self.render_overview_buf(&mut w);
            }
            _ => {}
        }

        // --- Smart Redraw Check ---
        // Smart redraw: skip full re-render when nothing changed (timer-only ticks)
        let state_changed = self.needs_full_redraw
            || self.pending_dissolve_in
            || self.last_rendered_slide != Some(self.current)
            || self.last_rendered_scroll != self.scroll_offset
            || self.last_rendered_width != self.width
            || self.last_rendered_height != self.height
            || self.last_rendered_mode != self.mode
            || self.last_rendered_scale != self.global_scale
            || self.last_rendered_image_scale != self.image_scale_offset;

        if !state_changed && self.mode == Mode::Normal {
            // Only update status bar (timer display) without re-emitting images
            return self.render_status_bar_only(&mut w);
        }

        // Track whether we need to clear old Kitty images.  Only clear when
        // images actually need re-positioning/re-sizing — NOT on every
        // needs_full_redraw (which fires on animation ticks and would cause
        // visible flicker from the clear+re-emit cycle).
        let need_kitty_clear = self.image_protocol == ImageProtocol::Kitty
            && (self.last_rendered_slide != Some(self.current)
                || self.last_rendered_scroll != self.scroll_offset
                || self.last_rendered_width != self.width
                || self.last_rendered_height != self.height
                || self.last_rendered_scale != self.global_scale
                || self.last_rendered_image_scale != self.image_scale_offset
                || self.gif_current_frame != self.last_rendered_gif_frame);

        // --- Content Assembly ---
        // Build the virtual buffer line by line. The buffer is a Vec<StyledLine>
        // that represents the slide's content area (everything between the status
        // bar and the notes panel / footer).

        // Clone needed: slide fields are read throughout the frame while &mut self
        // methods (build_status_bar, queue_styled_line, etc.) borrow self mutably.
        let slide = self.slides[self.current].clone();
        let tw = self.width as usize;
        let th = self.height as usize;

        let content_width = scaled_content_width(tw, self.current_scale());
        let margin = tw.saturating_sub(content_width) / 2;
        let pad = " ".repeat(margin);

        // Build virtual buffer (status bar rendered separately to avoid flicker on scroll)
        let mut lines: Vec<StyledLine> = Vec::new();
        let status_bar_rows: usize = if !self.show_fullscreen { 2 } else { 0 };

        // Section (respects per-slide directive and global toggle)
        let show_section = slide.show_section.unwrap_or(self.show_sections);
        if show_section && !slide.section.is_empty() {
            let mut line = StyledLine::empty();
            line.push(StyledSpan::new(&pad));
            line.push(StyledSpan::new(&format!("Section: {}", slide.section)).with_fg(self.text_color).dim());
            lines.push(line);
            lines.push(StyledLine::empty());
        }

        // Title (with optional decoration)
        if !slide.title.is_empty() {
            let decoration = slide.title_decoration.as_deref()
                .or(self.theme.title_decoration.as_deref());
            if slide.ascii_title {
                self.render_ascii_title(&slide.title, &pad, &mut lines);
            } else if let Some(dec) = decoration {
                self.render_title_decorated(&slide.title, dec, content_width, &pad, &mut lines);
            } else {
                let mut line = StyledLine::empty();
                line.push(StyledSpan::new(&pad));
                // OSC 66 title scaling is disabled for now — multicell blocks get
                // destroyed during full redraws (e.g. fullscreen toggle). The data
                // model and parser support remain; re-enable by applying
                // slide.title_scale here when the rendering path is fixed.
                let title_span = StyledSpan::new(&slide.title).with_fg(self.accent_color).bold();
                line.push(title_span);
                lines.push(line);
            }
            lines.push(StyledLine::empty());
        }

        // Subtitle (wrapped to content width)
        if !slide.subtitle.is_empty() {
            let sub_width = content_width.saturating_sub(2);
            let wrapped_sub = textwrap_simple(&slide.subtitle, sub_width);
            // OSC 66 subtitle scaling disabled (same reason as title — see above).
            for wline in &wrapped_sub {
                let mut line = StyledLine::empty();
                line.push(StyledSpan::new(&pad));
                let subtitle_spans = crate::markdown::parser::parse_inline_formatting(
                    wline, self.text_color, self.code_bg_color,
                );
                for span in subtitle_spans {
                    line.push(span);
                }
                lines.push(line);
            }
            lines.push(StyledLine::empty());
        }

        // Bullets
        for bullet in &slide.bullets {
            let indent = bullet_indent(bullet.depth);
            let wrapped = textwrap_simple(&bullet.text, content_width.saturating_sub(indent.len() + 2));
            for (i, wline) in wrapped.iter().enumerate() {
                let mut line = StyledLine::empty();
                line.push(StyledSpan::new(&pad));
                if i == 0 {
                    line.push(StyledSpan::new(indent).with_fg(self.accent_color));
                } else {
                    line.push(StyledSpan::new(&" ".repeat(indent.len())));
                }
                let inline_spans = crate::markdown::parser::parse_inline_formatting(
                    wline, self.text_color, self.code_bg_color,
                );
                for span in inline_spans {
                    line.push(span);
                }
                lines.push(line);
            }
        }
        if !slide.bullets.is_empty() {
            lines.push(StyledLine::empty());
        }

        // Column layouts
        if let Some(ref cols) = slide.columns {
            self.render_columns(cols, content_width, &pad, &mut lines);
            // Show exec output if columns have executable code blocks
            // Column exec blocks come after slide-level exec blocks in index order
            let slide_exec_count = slide.code_blocks.iter()
                .filter(|cb| cb.exec_mode.is_some()).count();
            let col_exec_blocks: Vec<&crate::presentation::CodeBlock> = cols.contents.iter()
                .flat_map(|c| c.code_blocks.iter())
                .filter(|cb| cb.exec_mode.is_some())
                .collect();
            if !col_exec_blocks.is_empty() {
                // Column exec block index starts after slide-level exec blocks
                let col_local_idx = self.exec_block_index.saturating_sub(slide_exec_count);
                if self.exec_block_index >= slide_exec_count && col_local_idx < col_exec_blocks.len() {
                    self.render_exec_output(&pad, &mut lines);
                }
            }
            lines.push(StyledLine::empty());
        }

        // Code blocks (presenterm-style: background rect with padding, no borders)
        let mut exec_render_idx: usize = 0;
        for cb in slide.code_blocks.iter() {
            let label = if cb.label.is_empty() { cb.language.clone() } else { cb.label.clone() };
            let inner_pad = 4; // 2 left + 2 right padding inside block
            let block_width = content_width;

            // Vertical padding top (empty line with code_bg)
            let mut vpad_top = StyledLine::empty();
            vpad_top.push(StyledSpan::new(&pad));
            vpad_top.push(StyledSpan::new(&" ".repeat(block_width)).with_bg(self.code_bg_color));
            lines.push(vpad_top);

            // Language label line (dimmed, like a comment)
            if !label.is_empty() {
                let comment_prefix = comment_prefix_for(&cb.language);
                let label_text = format!("  {}{}", comment_prefix, label);
                let label_width = unicode_width::UnicodeWidthStr::width(label_text.as_str());
                let label_pad = block_width.saturating_sub(label_width);
                let mut ll = StyledLine::empty();
                ll.push(StyledSpan::new(&pad));
                ll.push(StyledSpan::new(&label_text).with_fg(self.accent_color).with_bg(self.code_bg_color).dim());
                if label_pad > 0 {
                    ll.push(StyledSpan::new(&" ".repeat(label_pad)).with_bg(self.code_bg_color));
                }
                lines.push(ll);
            }

            // Highlighted code lines (truncated to block_width)
            let code_content_width = block_width.saturating_sub(inner_pad);
            let highlighted = self.highlighter.highlight(&cb.code, &cb.language);
            for hline in &highlighted {
                let mut line = StyledLine::empty();
                line.push(StyledSpan::new(&pad));
                line.push(StyledSpan::new("    ").with_bg(self.code_bg_color)); // left padding
                let mut line_char_count = inner_pad;
                for span in hline {
                    let txt = span.text.trim_end_matches('\n');
                    let span_w = unicode_width::UnicodeWidthStr::width(txt);
                    let remaining = code_content_width.saturating_sub(line_char_count.saturating_sub(inner_pad));
                    if remaining == 0 {
                        break;
                    }
                    if span_w <= remaining {
                        line.push(StyledSpan::new(txt)
                            .with_fg(span.fg)
                            .with_bg(self.code_bg_color));
                        line_char_count += span_w;
                    } else {
                        let truncated = truncate_to_width(txt, remaining);
                        let tw = unicode_width::UnicodeWidthStr::width(truncated.as_str());
                        line.push(StyledSpan::new(&truncated)
                            .with_fg(span.fg)
                            .with_bg(self.code_bg_color));
                        line_char_count += tw;
                        break;
                    }
                }
                // Pad to block_width with code_bg
                let pad_needed = block_width.saturating_sub(line_char_count);
                if pad_needed > 0 {
                    line.push(StyledSpan::new(&" ".repeat(pad_needed)).with_bg(self.code_bg_color));
                }
                lines.push(line);
            }

            // Vertical padding bottom (empty line with code_bg)
            let mut vpad_bot = StyledLine::empty();
            vpad_bot.push(StyledSpan::new(&pad));
            vpad_bot.push(StyledSpan::new(&" ".repeat(block_width)).with_bg(self.code_bg_color));
            lines.push(vpad_bot);

            // Exec mode indicator (hidden when --no-exec)
            if cb.exec_mode.is_some() && self.allow_exec {
                let mut el = StyledLine::empty();
                el.push(StyledSpan::new(&pad));
                let mode_str = match cb.exec_mode {
                    Some(ExecMode::Exec) => "  [Ctrl+E to execute]",
                    Some(ExecMode::Pty) => "  [Ctrl+E to run in PTY]",
                    None => "",
                };
                el.push(StyledSpan::new(mode_str).with_fg(self.accent_color).dim());
                lines.push(el);
            }

            // Execution output (show only under the currently-executed block)
            if cb.exec_mode.is_some() && self.allow_exec {
                if exec_render_idx == self.exec_block_index {
                    self.render_exec_output(&pad, &mut lines);
                }
                exec_render_idx += 1;
            }
            lines.push(StyledLine::empty());
        }

        // Tables
        for table in &slide.tables {
            self.render_table(table, content_width, &pad, &mut lines);
            lines.push(StyledLine::empty());
        }

        // Block quotes (with text wrapping)
        for bq in &slide.block_quotes {
            let bq_prefix_width = 4; // "  │ "
            let bq_available = content_width.saturating_sub(bq_prefix_width + margin);
            for qline in &bq.lines {
                let wrapped = textwrap_simple(qline, bq_available);
                for wline in &wrapped {
                    let mut line = StyledLine::empty();
                    line.push(StyledSpan::new(&pad));
                    line.push(StyledSpan::new("  │ ").with_fg(self.accent_color).dim());
                    let inline_spans = crate::markdown::parser::parse_inline_formatting(
                        wline, self.text_color, self.code_bg_color,
                    );
                    for span in inline_spans {
                        line.push(span.italic());
                    }
                    lines.push(line);
                }
            }
            if !bq.lines.is_empty() {
                lines.push(StyledLine::empty());
            }
        }

        // --- Images and Diagrams ---
        // Protocol images (Kitty/iTerm2/Sixel) can't be mixed into the styled line
        // buffer — they need their raw escape data written after the text content.
        // We track their positions here and emit them at the end of the frame.
        let mut pending_protocol_images: Vec<(String, usize)> = Vec::new();

        // Mermaid diagrams
        for mermaid_block in &slide.mermaid_blocks {
            if let Some(ref mut renderer) = self.mermaid_renderer {
                // Use actual pixel width if available, else estimate at 2x for quality
                let pixel_width = if self.window_size.pixel_width > 0 {
                    self.window_size.pixel_width as usize
                } else {
                    content_width * 16
                };
                match renderer.render(&mermaid_block.source, pixel_width) {
                    Ok(png_path) => {
                        // Load and render the PNG as an image
                        let mermaid_img = crate::presentation::SlideImage {
                            path: png_path,
                            alt_text: String::from("Mermaid diagram"),
                            position: crate::presentation::ImagePosition::Below,
                            render_mode: crate::presentation::ImageRenderMode::Auto,
                            scale: 80,
                            color_override: String::new(),
                        };
                        let effective_protocol = self.image_protocol;
                        let img_max_height = th / 2;
                        let preloaded = self.preloaded_images.get(&mermaid_img.path);
                        let rendered = render_slide_image(
                            &mermaid_img, content_width, img_max_height, &pad,
                            self.accent_color, self.text_color,
                            effective_protocol, self.bg_color,
                            &self.window_size, preloaded,
                        );
                        match rendered {
                            RenderedImage::Lines(l) => lines.extend(l),
                            RenderedImage::Protocol { escape_data, placeholder_height } => {
                                let image_line_offset = lines.len();
                                for _ in 0..placeholder_height {
                                    lines.push(StyledLine::empty());
                                }
                                pending_protocol_images.push((escape_data, image_line_offset));
                            }
                            RenderedImage::KittyPlacement { cols, rows, transmit_escape: _, image_id: _ } => {
                                // TODO(v0.5.0 step 1.6): use placement command instead
                                let image_line_offset = lines.len();
                                for _ in 0..rows {
                                    lines.push(StyledLine::empty());
                                }
                                let _ = cols; // will be used for placement
                            }
                        }
                    }
                    Err(_) => {
                        // Fallback: show source as visible code block with warning
                        lines.push(StyledLine::empty());
                        let mut warn = StyledLine::empty();
                        warn.push(StyledSpan::new(&pad));
                        warn.push(StyledSpan::new("  ┌─ Mermaid Diagram (render failed) ─┐").with_fg(self.accent_color));
                        lines.push(warn);
                        lines.push(StyledLine::empty());
                        let code_fg = Color::Rgb { r: 130, g: 200, b: 130 };
                        for src_line in mermaid_block.source.lines() {
                            let mut line = StyledLine::empty();
                            line.push(StyledSpan::new(&pad));
                            line.push(StyledSpan::new("  │ "));
                            line.push(StyledSpan::new(src_line).with_fg(code_fg));
                            lines.push(line);
                        }
                    }
                }
            } else {
                // No renderer available — show source as a visible code-like block
                lines.push(StyledLine::empty());
                let mut header = StyledLine::empty();
                header.push(StyledSpan::new(&pad));
                header.push(StyledSpan::new("  ┌─ Mermaid Diagram (install mmdc to render) ─┐").with_fg(self.accent_color));
                lines.push(header);
                lines.push(StyledLine::empty());
                let code_fg = Color::Rgb { r: 130, g: 200, b: 130 }; // green-ish for diagram source
                for src_line in mermaid_block.source.lines() {
                    let mut line = StyledLine::empty();
                    line.push(StyledSpan::new(&pad));
                    line.push(StyledSpan::new("  │ "));
                    line.push(StyledSpan::new(src_line).with_fg(code_fg));
                    lines.push(line);
                }
                lines.push(StyledLine::empty());
                let mut footer = StyledLine::empty();
                footer.push(StyledSpan::new(&pad));
                footer.push(StyledSpan::new("  └─ npm install -g @mermaid-js/mermaid-cli ──┘").with_fg(self.accent_color).dim());
                lines.push(footer);
            }
            lines.push(StyledLine::empty());
        }

        // Native diagram blocks (adaptive: auto-downgrades style if too wide)
        for diagram_block in &slide.diagram_blocks {
            let graph = crate::diagram::parser::parse(&diagram_block.source);
            let dim = match self.text_color {
                Color::Rgb { r, g, b } => Color::Rgb {
                    r: r / 2,
                    g: g / 2,
                    b: b / 2,
                },
                _ => Color::Rgb { r: 128, g: 128, b: 128 },
            };
            let diagram_lines = crate::diagram::render_adaptive(
                &graph,
                diagram_block.style,
                content_width,
                tw,
                self.accent_color,
                self.text_color,
                dim,
                &pad,
            );
            lines.extend(diagram_lines);
        }

        // Image rendering (cached)
        if let Some(ref img) = slide.image {
            // Per-image render mode override from markdown directives
            let effective_protocol = resolve_image_protocol(img.render_mode, self.image_protocol);
            let proto_key = protocol_cache_key(effective_protocol);
            // Apply image_scale directive + runtime offset
            let effective_scale = (img.scale as i16 + self.image_scale_offset as i16).clamp(5, 100) as u8;
            let img_width = (content_width as f64 * effective_scale as f64 / 100.0).max(1.0) as usize;
            let img_max_height = (th as f64 * effective_scale as f64 / 100.0 / 2.0).max(1.0) as usize;
            // For animated GIFs, include the current frame index in the cache key
            let is_animated_gif = self.gif_frames.contains_key(&img.path);
            let frame_idx = if is_animated_gif { self.gif_current_frame } else { 0 };
            let cache_key = ImageCacheKey {
                path: img.path.clone(),
                render_width: img_width,
                protocol: proto_key,
                gif_frame_index: frame_idx,
                color_override: img.color_override.clone(),
            };
            let cached = self.image_cache.entry(cache_key).or_insert_with(|| {
                // For animated GIFs, use the current frame's image data
                let gif_frame_img = if is_animated_gif {
                    self.gif_frames.get(&img.path)
                        .and_then(|frames| frames.get(frame_idx))
                        .map(|f| &f.image)
                } else {
                    None
                };
                let preloaded = gif_frame_img.or_else(|| self.preloaded_images.get(&img.path));
                // Center image within content area (pad covers terminal→content margin,
                // img_extra_margin centers within content when img < content_width)
                let img_extra_margin = content_width.saturating_sub(img_width) / 2;
                let img_pad = " ".repeat(margin + img_extra_margin);
                let rendered = render_slide_image(
                    img, img_width, img_max_height, &img_pad,
                    self.accent_color, self.text_color,
                    effective_protocol, self.bg_color,
                    &self.window_size, preloaded,
                );
                match rendered {
                    RenderedImage::Lines(l) => CachedImage::Lines(l),
                    RenderedImage::Protocol { escape_data, placeholder_height } => {
                        CachedImage::Protocol { escape_data, placeholder_height }
                    }
                    RenderedImage::KittyPlacement { image_id, cols, rows, transmit_escape: _ } => {
                        // TODO(v0.5.0 step 1.5): transmit_escape written at prerender time
                        CachedImage::KittyRef { image_id, cols, rows }
                    }
                }
            });
            match cached {
                CachedImage::Lines(cached_lines) => {
                    lines.extend(cached_lines.clone());
                }
                CachedImage::KittyRef { image_id, cols, rows } => {
                    // Kitty v2: reserve placeholder rows, emit placement at frame end
                    let image_line_offset = lines.len();
                    for _ in 0..*rows {
                        lines.push(StyledLine::empty());
                    }
                    let placement = crate::image_util::kitty::placement_escape(*image_id, *cols, *rows);
                    pending_protocol_images.push((placement, image_line_offset));
                }
                CachedImage::Protocol { escape_data, placeholder_height } => {
                    // Record line offset where image should be drawn
                    let image_line_offset = lines.len();
                    for _ in 0..*placeholder_height {
                        lines.push(StyledLine::empty());
                    }
                    pending_protocol_images.push((escape_data.clone(), image_line_offset));
                }
            }
            lines.push(StyledLine::empty());
        }

        // --- Viewport Calculation and Alignment ---
        // Calculate available display area (excluding status bar rows)
        let has_slide_footer = slide.footer.is_some();
        let reserved_bottom =
            if self.show_notes && !slide.notes.is_empty() { 7 } else { 0 }
            + if self.mode == Mode::Command || self.mode == Mode::Goto { 1 } else { 0 }
            + if has_slide_footer { 1 } else { 0 };
        let content_area = th.saturating_sub(status_bar_rows + reserved_bottom);

        // Vertical centering: per-slide alignment overrides global default_alignment
        let effective_alignment = slide.alignment
            .or(self.meta.default_alignment)
            .unwrap_or(SlideAlignment::Top);
        let do_vcenter = matches!(effective_alignment, SlideAlignment::Center | SlideAlignment::VCenter);
        let do_hcenter = matches!(effective_alignment, SlideAlignment::Center | SlideAlignment::HCenter);

        if do_vcenter && lines.len() < content_area {
            let padding_rows = (content_area - lines.len()) / 2;
            if padding_rows > 0 {
                let mut padded = Vec::with_capacity(lines.len() + padding_rows);
                for _ in 0..padding_rows {
                    padded.push(StyledLine::empty());
                }
                padded.append(&mut lines);
                lines = padded;
                // Shift protocol image offsets to account for centering padding
                for (_, offset) in &mut pending_protocol_images {
                    *offset += padding_rows;
                }
            }
        }

        // Horizontal centering: center each line's content within the terminal width.
        // Lines already start with `margin` spaces (the pad), so we subtract that
        // to get the actual content width, then compute the correct left offset.
        if do_hcenter {
            for line in &mut lines {
                let line_width: usize = line.spans.iter()
                    .map(|s| unicode_width::UnicodeWidthStr::width(s.text.as_str()))
                    .sum();
                // Content width is the line minus the existing left margin
                let content_text_width = line_width.saturating_sub(margin);
                if content_text_width > 0 && content_text_width < tw {
                    let desired_left = (tw.saturating_sub(content_text_width)) / 2;
                    if desired_left > margin {
                        let extra = desired_left - margin;
                        let mut centered = StyledLine::empty();
                        centered.content_type = line.content_type;
                        centered.push(StyledSpan::new(&" ".repeat(extra)));
                        for span in &line.spans {
                            centered.push(span.clone());
                        }
                        *line = centered;
                    }
                }
            }
        }

        // --- Animation Overlays ---
        // Apply transition, entrance, or loop animations to the buffer.
        // Transitions blend old and new buffers; entrance effects reveal
        // content progressively; loops run continuously (sparkle, matrix, etc.).
        if let Some(ref mut anim) = self.active_animation {
            match anim.kind {
                AnimationKind::Transition(tt) => {
                    // Pass &lines directly — no need to clone into anim.new_buffer
                    let progress = anim.progress();
                    lines = render_transition_frame(
                        &anim.old_buffer, &lines,
                        progress, tt, self.bg_color, content_width,
                        anim.exit_only,
                    );
                }
                AnimationKind::Entrance(ea) => {
                    // Pass &lines directly — avoids per-frame clone
                    let progress = anim.progress();
                    lines = render_entrance_frame(&lines, progress, ea, self.bg_color);
                }
                AnimationKind::Loop(_) => {
                    // Loops are handled below (separate from active_animation)
                }
            }
        }

        // Apply loop animations (runs independently, only when no transition/entrance active)
        // Use full terminal width (tw) so matrix/bounce fill edge-to-edge
        if self.active_animation.is_none() && !self.active_loop.is_empty() {
            let slide_loops = &self.slides[self.current].loop_animations;
            for (i, &(la, frame)) in self.active_loop.iter().enumerate() {
                let target = slide_loops.get(i).and_then(|(_, t): &(_, _)| t.as_deref());
                lines = render_loop_frame(
                    &lines, la, frame,
                    self.accent_color, self.bg_color,
                    tw, content_area,
                    target,
                );
            }
        }

        // --- Scroll Clamping ---
        // Ensure scroll offset doesn't go past the end of content.
        if lines.len() > content_area {
            let max_scroll = lines.len().saturating_sub(content_area);
            self.scroll_offset = self.scroll_offset.min(max_scroll);
        } else {
            self.scroll_offset = 0;
        }

        let visible_start = self.scroll_offset;
        let visible_end = (visible_start + content_area).min(lines.len());

        // --- Terminal Output (Status Bar + Content + Gradient) ---

        // Render fixed status bar at rows 0-1 (only when not scroll-only change)
        let scroll_only = !self.needs_full_redraw
            && self.last_rendered_slide == Some(self.current)
            && self.last_rendered_width == self.width
            && self.last_rendered_height == self.height
            && self.last_rendered_mode == self.mode
            && self.last_rendered_scale == self.global_scale
            && self.last_rendered_image_scale == self.image_scale_offset;

        // Total rows below the status bar that should participate in the gradient.
        // This includes: separator row (1) + content_area + footer row (0 or 1).
        let has_gradient = self.gradient_from.is_some() && self.gradient_to.is_some();
        let gradient_span = if !self.show_fullscreen {
            1 + content_area + if has_slide_footer { 1 } else { 0 }
        } else {
            content_area + if has_slide_footer { 1 } else { 0 }
        };

        if !scroll_only && !self.show_fullscreen {
            let bar = self.build_status_bar(tw);
            queue!(w, cursor::MoveTo(0, 0))?;
            self.queue_styled_line(&mut w, &bar, tw)?;
            let sep_bg = if has_gradient {
                self.row_bg_color(0, gradient_span.max(1))
            } else {
                self.bg_color
            };
            queue!(w, cursor::MoveTo(0, 1), SetBackgroundColor(sep_bg))?;
            write!(w, "{:width$}", "", width = tw)?;
        }

        // Offset for gradient: content rows start after the separator row (unless fullscreen).
        let gradient_offset = if !self.show_fullscreen { 1 } else { 0 };

        // Render visible content lines (offset by status_bar_rows), with per-row gradient.
        // When dissolve-in is pending, render blank content (the dissolve loop
        // will progressively reveal it after this frame flushes).
        if self.pending_dissolve_in {
            for i in 0..content_area {
                let row = (status_bar_rows + i) as u16;
                let bg = if has_gradient {
                    self.row_bg_color(gradient_offset + visible_start + i, gradient_span.max(1))
                } else {
                    self.bg_color
                };
                queue!(w, cursor::MoveTo(0, row), SetBackgroundColor(bg))?;
                write!(w, "{:width$}", "", width = tw)?;
            }
        } else {
            for (i, line) in lines[visible_start..visible_end].iter().enumerate() {
                if line.is_scale_placeholder { continue; }
                let row = (status_bar_rows + i) as u16;
                queue!(w, cursor::MoveTo(0, row))?;
                if has_gradient {
                    let screen_row = gradient_offset + visible_start + i;
                    let row_bg = self.row_bg_color(screen_row, gradient_span.max(1));
                    self.queue_styled_line_with_bg(&mut w, line, tw, row_bg)?;
                } else {
                    self.queue_styled_line(&mut w, line, tw)?;
                }
            }
        }

        // Cache current buffer for transition source on next slide change.
        // Moved here (after the last read of `lines`) to avoid a full clone.
        self.last_rendered_buffer = lines;

        // Fill remaining rows below content
        let content_rows_drawn = visible_end - visible_start;
        for i in content_rows_drawn..content_area {
            let row = (status_bar_rows + i) as u16;
            let fill_bg = if has_gradient {
                self.row_bg_color(gradient_offset + visible_start + i, gradient_span.max(1))
            } else {
                self.bg_color
            };
            queue!(w, cursor::MoveTo(0, row), SetBackgroundColor(fill_bg))?;
            write!(w, "{:width$}", "", width = tw)?;
        }

        // --- Per-Slide Footer Bar ---
        if has_slide_footer {
            if let Some(ref footer_text) = slide.footer {
                use crate::presentation::FooterAlign;
                let footer_row = (status_bar_rows + content_area) as u16;
                let footer_bg = if has_gradient {
                    self.row_bg_color(gradient_span.saturating_sub(1), gradient_span.max(1))
                } else {
                    self.bg_color
                };
                queue!(w, cursor::MoveTo(0, footer_row), SetBackgroundColor(footer_bg))?;
                let text = footer_text.as_str();
                let text_width = unicode_width::UnicodeWidthStr::width(text);
                queue!(w, SetForegroundColor(self.accent_color))?;
                match slide.footer_align {
                    FooterAlign::Left => {
                        let pad_right = tw.saturating_sub(text_width + 1);
                        write!(w, " {}{:width$}", text, "", width = pad_right)?;
                    }
                    FooterAlign::Center => {
                        let pad_left = tw.saturating_sub(text_width) / 2;
                        let pad_right = tw.saturating_sub(pad_left + text_width);
                        write!(w, "{:wl$}{}{:wr$}", "", text, "", wl = pad_left, wr = pad_right)?;
                    }
                    FooterAlign::Right => {
                        let pad_left = tw.saturating_sub(text_width + 1);
                        write!(w, "{:width$}{} ", "", text, width = pad_left)?;
                    }
                }
            }
        }

        // --- Notes Panel ---
        // Speaker notes are shown in a fixed-height panel at the bottom
        // with scrolling support (N/P keys). The panel has its own background.
        if self.show_notes && !slide.notes.is_empty() {
            let notes_rows = 6usize; // 1 separator + 5 content rows = 6, +1 reserved
            let notes_y = (th as u16).saturating_sub(7);

            // Separator line
            queue!(w, cursor::MoveTo(0, notes_y), SetBackgroundColor(self.code_bg_color), SetForegroundColor(self.accent_color))?;
            let all_note_lines: Vec<&str> = slide.notes.lines().collect();
            let scroll_indicator = if all_note_lines.len() > notes_rows {
                let max_scroll = all_note_lines.len().saturating_sub(notes_rows);
                self.notes_scroll = self.notes_scroll.min(max_scroll);
                format!(" [{}/{}] N/P scroll", self.notes_scroll + 1, max_scroll + 1)
            } else {
                self.notes_scroll = 0;
                String::new()
            };
            let sep: String = format!("─── Notes{} {}", scroll_indicator, "─".repeat(tw))
                .chars().take(tw).collect();
            let sep_pad = tw.saturating_sub(sep.chars().count());
            write!(w, "{}{:width$}", sep, "", width = sep_pad)?;

            // Content rows (scrollable, fill all 6 remaining rows)
            let visible_notes: Vec<&str> = all_note_lines
                .iter()
                .skip(self.notes_scroll)
                .take(notes_rows)
                .copied()
                .collect();
            for i in 0..notes_rows {
                queue!(w, cursor::MoveTo(0, notes_y + 1 + i as u16), SetBackgroundColor(self.code_bg_color), SetForegroundColor(self.text_color))?;
                if let Some(note_line) = visible_notes.get(i) {
                    let truncated: String = note_line.chars().take(tw.saturating_sub(2)).collect();
                    let trunc_cols = truncated.chars().count();
                    write!(w, " {}{:width$}", truncated, "", width = tw.saturating_sub(trunc_cols + 2))?;
                } else {
                    write!(w, "{:width$}", "", width = tw)?;
                }
            }
        }

        // --- Command Bar / Goto Input ---
        if self.mode == Mode::Command {
            let y = th as u16 - 1;
            queue!(w, cursor::MoveTo(0, y), SetBackgroundColor(self.code_bg_color), SetForegroundColor(self.accent_color))?;
            write!(w, ":{}{:width$}", self.command_buf, "", width = tw.saturating_sub(self.command_buf.len() + 1))?;
        }

        // Goto indicator
        if self.mode == Mode::Goto {
            let y = th as u16 - 1;
            queue!(w, cursor::MoveTo(0, y), SetBackgroundColor(self.code_bg_color), SetForegroundColor(self.accent_color))?;
            write!(w, "goto: {}{:width$}", self.goto_buf, "", width = tw.saturating_sub(self.goto_buf.len() + 7))?;
        }

        // --- Protocol Image Emission ---
        // Clear old Kitty images right before placing new content, so the
        // delete and new frame appear atomically within the synchronized update.
        if need_kitty_clear {
            w.write_all(KITTY_CLEAR_IMAGES)?;
        }

        // Write protocol image data after line rendering (Kitty/iTerm2/Sixel).
        // Skip during transitions/entrance animations and dissolve-in — emitting
        // images on every animation frame causes visible flicker from rapid
        // re-placement.  Images appear cleanly once the animation completes.
        let animation_active = matches!(
            self.active_animation,
            Some(ref a) if matches!(a.kind, AnimationKind::Transition(_) | AnimationKind::Entrance(_))
        );
        if !self.pending_dissolve_in && !animation_active {
            for (escape_data, line_offset) in &pending_protocol_images {
                if *line_offset >= visible_start && *line_offset < visible_end {
                    let display_row = line_offset - visible_start;
                    let screen_row = (status_bar_rows + display_row) as u16;
                    queue!(w, cursor::MoveTo(0, screen_row))?;
                    write!(w, "{}", escape_data)?;
                }
            }
        }

        // --- End Synchronized Update ---
        queue!(w, EndSynchronizedUpdate, ResetColor)?;
        w.flush()?;

        // --- Update Smart Redraw Tracking ---
        self.last_rendered_slide = Some(self.current);
        self.last_rendered_scroll = self.scroll_offset;
        self.last_rendered_width = self.width;
        self.last_rendered_height = self.height;
        self.last_rendered_mode = self.mode;
        self.last_rendered_scale = self.global_scale;
        self.last_rendered_image_scale = self.image_scale_offset;
        self.last_rendered_gif_frame = self.gif_current_frame;
        self.needs_full_redraw = false;

        // --- Dissolve-In: Scatter-Reveal After Font Transition ---
        if self.pending_dissolve_in {
            self.render_dissolve_in(
                &pending_protocol_images, visible_start, visible_end, status_bar_rows,
            )?;
        }

        Ok(())
    }

    /// Redraw only the status bar line (for timer-only updates without re-emitting images).
    ///
    /// This is the "smart redraw" fast path. When the presentation timer is
    /// running but no other state has changed, we only need to repaint the
    /// status bar (row 0) to update the elapsed time display. This avoids
    /// the expensive full content rebuild and, crucially, avoids re-emitting
    /// protocol image escape sequences (which would cause visible flicker).
    pub(crate) fn render_status_bar_only(&self, w: &mut impl Write) -> Result<()> {
        let tw = self.width as usize;
        if !self.show_fullscreen {
            queue!(w, cursor::MoveTo(0, 0))?;
            let bar = self.build_status_bar(tw);
            self.queue_styled_line(w, &bar, tw)?;
        }
        queue!(w, EndSynchronizedUpdate, ResetColor)?;
        w.flush()?;
        Ok(())
    }

    /// Applies a pending font size change, including optional scatter-dissolve transition.
    ///
    /// When `font_change_is_slide_transition` is set, a scatter-dissolve animation
    /// plays during the font change so the screen is never blank. Font step batches
    /// are interleaved between dissolve frames so the zoom and dissolve overlap.
    /// Otherwise, a plain font stepping approach is used (smooth or instant depending
    /// on the `font_transition` directive).
    ///
    /// After the font change, terminal dimensions are re-queried and resize events drained.
    fn apply_font_change(&mut self, target: f64) -> Result<()> {
        let mut font_applied = false;

        // ── Slide transition: scatter-dissolve interleaved with font stepping ──
        if self.font_change_is_slide_transition {
            let old_buf = self.last_rendered_buffer.clone();
            if !old_buf.is_empty() {
                // Clear Kitty images before the transition
                if self.image_protocol == ImageProtocol::Kitty {
                    let stdout = io::stdout();
                    let mut pre = stdout.lock();
                    pre.write_all(KITTY_CLEAR_IMAGES)?;
                    pre.flush()?;
                }

                // For Ghostty, fire the keystrokes now — the dissolve
                // animation covers the processing delay.
                if matches!(self.font_capability, FontSizeCapability::GhosttyKeystroke) {
                    self.ghostty_set_font_size(target);
                }

                // Build a full-screen buffer: status bar + separator + content.
                let status_rows = if self.show_fullscreen { 0u16 } else { 2 };
                let mut screen_buf: Vec<StyledLine> = Vec::new();
                if status_rows > 0 {
                    let bar = self.build_status_bar(self.width as usize);
                    screen_buf.push(bar);
                    screen_buf.push(StyledLine::empty());
                }
                for line in &old_buf {
                    screen_buf.push(line.clone());
                }

                // Calculate Kitty font stepping parameters
                let current_font = self.last_applied_font_size.unwrap_or(target);
                let font_delta = target - current_font;
                let num_font_steps = if font_delta.abs() > 0.3 {
                    (font_delta.abs() / 0.2).round() as usize
                } else {
                    0
                };
                let font_dir = if font_delta >= 0.0 { 1.0_f64 } else { -1.0_f64 };

                // Scale dissolve to cover font stepping
                let font_step_time_ms = num_font_steps as u32 * 8;
                let target_duration_ms = font_step_time_ms.max(400);
                let dissolve_frames = (target_duration_ms / 30).clamp(12, 20);
                let mut font_steps_sent = 0usize;

                for frame in 1..=dissolve_frames {
                    let progress = frame as f64 / dissolve_frames as f64;

                    // Re-query terminal dimensions (font steps resize the terminal)
                    if frame > 1 && num_font_steps > 0 {
                        self.window_size = WindowSize::query();
                        self.width = self.window_size.columns;
                        self.height = self.window_size.rows;
                    }
                    let tw = self.width as usize;

                    // Render one dissolve frame
                    {
                        let stdout = io::stdout();
                        let mut fw = BufWriter::with_capacity(64 * 1024, stdout.lock());
                        let dis_has_grad = self.gradient_from.is_some() && self.gradient_to.is_some();
                        let grad_total = (self.height.saturating_sub(status_rows)) as usize;
                        queue!(fw, BeginSynchronizedUpdate)?;
                        for row in 0..self.height {
                            let row_bg = if dis_has_grad && row >= status_rows {
                                self.row_bg_color((row - status_rows) as usize, grad_total.max(1))
                            } else {
                                self.bg_color
                            };
                            queue!(fw, cursor::MoveTo(0, row), SetBackgroundColor(row_bg))?;
                            if let Some(line) = screen_buf.get(row as usize) {
                                let mut col = 0usize;
                                for span in &line.spans {
                                    if col >= tw { break; }
                                    let span_bg = span.bg.unwrap_or(row_bg);
                                    let fg = span.fg.unwrap_or(self.text_color);
                                    let dimmed_fg = interpolate_color(fg, span_bg, progress * 0.7);
                                    let dimmed_bg = interpolate_color(span_bg, row_bg, progress * 0.7);
                                    for ch in span.text.chars() {
                                        if col >= tw { break; }
                                        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
                                        let group = col / 2;
                                        let hash = (row as u64).wrapping_mul(31)
                                            .wrapping_add(group as u64)
                                            .wrapping_mul(7919) % 1000;
                                        let threshold = hash as f64 / 1000.0;
                                        if threshold < progress {
                                            queue!(fw, SetBackgroundColor(row_bg))?;
                                            for _ in 0..cw { write!(fw, " ")?; }
                                        } else {
                                            queue!(fw, SetBackgroundColor(dimmed_bg))?;
                                            queue!(fw, SetForegroundColor(dimmed_fg))?;
                                            write!(fw, "{}", ch)?;
                                        }
                                        col += cw;
                                    }
                                }
                                if col < tw {
                                    write!(fw, "{:width$}", "", width = tw - col)?;
                                }
                            } else {
                                write!(fw, "{:width$}", "", width = tw)?;
                            }
                        }
                        queue!(fw, EndSynchronizedUpdate, ResetColor)?;
                        fw.flush()?;
                    }

                    // Interleave Kitty font step batch + pace the frame
                    let frame_target_ms = target_duration_ms / dissolve_frames;
                    let frame_start = std::time::Instant::now();

                    if num_font_steps > 0 && matches!(self.font_capability, FontSizeCapability::KittyRemote) {
                        let target_steps = ((num_font_steps as f64 * progress).round() as usize)
                            .min(num_font_steps);
                        let batch = target_steps - font_steps_sent;
                        if batch > 0 {
                            let stdout = io::stdout();
                            let mut pre = stdout.lock();
                            for s in 0..batch {
                                let step_idx = font_steps_sent + s + 1;
                                let intermediate = current_font + font_dir * 0.2 * step_idx as f64;
                                pre.write_all(kitty_font_escape(intermediate).as_bytes())?;
                                pre.flush()?;
                                std::thread::sleep(std::time::Duration::from_millis(8));
                            }
                            font_steps_sent = target_steps;
                        }
                    }

                    // Pad remaining time so the dissolve isn't too fast
                    let elapsed = frame_start.elapsed().as_millis() as u32;
                    if elapsed < frame_target_ms {
                        std::thread::sleep(std::time::Duration::from_millis(
                            (frame_target_ms - elapsed) as u64,
                        ));
                    }
                }

                // Final font step — land exactly on target
                match self.font_capability {
                    FontSizeCapability::KittyRemote => {
                        let stdout = io::stdout();
                        let mut pre = stdout.lock();
                        pre.write_all(kitty_font_escape(target).as_bytes())?;
                        pre.flush()?;
                    }
                    _ => {} // Ghostty keystrokes already sent above
                }
                font_applied = true;
                self.pending_dissolve_in = true;
            }
            self.font_change_is_slide_transition = false;
        }

        // ── Plain font stepping (interactive ] / [ or slide with font_transition: none) ──
        if !font_applied {
            let skip_stepping = self.slides[self.current].font_transition.as_deref() == Some("none");
            match self.font_capability {
                FontSizeCapability::KittyRemote => {
                    let stdout = io::stdout();
                    let mut pre = stdout.lock();

                    if skip_stepping {
                        queue!(pre, BeginSynchronizedUpdate)?;
                        for row in 0..self.height {
                            queue!(pre, cursor::MoveTo(0, row), SetBackgroundColor(self.bg_color))?;
                            write!(pre, "{:width$}", "", width = self.width as usize)?;
                        }
                        queue!(pre, EndSynchronizedUpdate, ResetColor)?;
                        pre.flush()?;
                    }

                    if self.image_protocol == ImageProtocol::Kitty {
                        pre.write_all(KITTY_CLEAR_IMAGES)?;
                        pre.flush()?;
                    }

                    if !skip_stepping {
                        let current = self.last_applied_font_size.unwrap_or(target);
                        if (target - current).abs() > 0.3 {
                            let step = 0.2_f64;
                            let delta = target - current;
                            let dir = if delta >= 0.0 { 1.0 } else { -1.0 };
                            let num_steps = (delta.abs() / step).round() as usize;
                            for i in 1..num_steps {
                                let intermediate = current + dir * step * i as f64;
                                pre.write_all(kitty_font_escape(intermediate).as_bytes())?;
                                pre.flush()?;
                                std::thread::sleep(std::time::Duration::from_millis(8));
                            }
                        }
                    }

                    pre.write_all(kitty_font_escape(target).as_bytes())?;
                    pre.flush()?;
                    drop(pre);
                }
                FontSizeCapability::GhosttyKeystroke => {
                    self.ghostty_set_font_size(target);
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                FontSizeCapability::None => {}
            }
        }

        self.last_applied_font_size = Some(target);
        // Let the terminal settle after font change, then drain resize events
        std::thread::sleep(std::time::Duration::from_millis(30));
        while event::poll(std::time::Duration::from_millis(10))? {
            if let Event::Resize(w2, h2) = event::read()? {
                self.width = w2;
                self.height = h2;
            } else {
                break;
            }
        }
        self.window_size = WindowSize::query();
        self.width = self.window_size.columns;
        self.height = self.window_size.rows;
        self.needs_full_redraw = true;
        Ok(())
    }

    /// Runs the scatter-reveal dissolve-in animation after a font transition.
    ///
    /// Mirrors the dissolve-out so the transition feels symmetric. Protocol images
    /// are emitted on the final frame within the same synchronized update block so
    /// they appear atomically with the fully-revealed content.
    fn render_dissolve_in(
        &mut self,
        pending_protocol_images: &[(String, usize)],
        visible_start: usize,
        visible_end: usize,
        status_bar_rows: usize,
    ) -> Result<()> {
        self.pending_dissolve_in = false;
        let dissolve_lines = std::mem::take(&mut self.last_rendered_buffer);
        if dissolve_lines.is_empty() {
            self.active_animation = None;
            self.needs_full_redraw = true;
            return Ok(());
        }

        let dis_frames = 12u32;
        let dis_tw = self.width as usize;
        let dis_status = if self.show_fullscreen { 0u16 } else { 2 };
        let dis_content_rows = (self.height - dis_status) as usize;
        let dis_visible = dissolve_lines.len().min(dis_content_rows);
        let status_bar = if dis_status > 0 {
            self.build_status_bar(dis_tw)
        } else {
            StyledLine::empty()
        };

        for frame in 1..=dis_frames {
            let progress = frame as f64 / dis_frames as f64;
            let dim = (1.0 - progress) * 0.4;
            let is_last = frame == dis_frames;
            let stdout = io::stdout();
            let mut dw = BufWriter::with_capacity(64 * 1024, stdout.lock());
            queue!(dw, BeginSynchronizedUpdate)?;

            let din_has_grad = self.gradient_from.is_some() && self.gradient_to.is_some();
            let din_grad_total = dis_content_rows + if dis_status > 0 { 1 } else { 0 };

            // Status bar at full brightness
            if dis_status > 0 {
                queue!(dw, cursor::MoveTo(0, 0))?;
                self.queue_styled_line(&mut dw, &status_bar, dis_tw)?;
                let sep_bg = if din_has_grad {
                    self.row_bg_color(0, din_grad_total.max(1))
                } else {
                    self.bg_color
                };
                queue!(dw, cursor::MoveTo(0, 1), SetBackgroundColor(sep_bg))?;
                write!(dw, "{:width$}", "", width = dis_tw)?;
            }

            let din_grad_offset = if dis_status > 0 { 1 } else { 0 };

            // Content: per-cell scatter reveal
            for (i, line) in dissolve_lines[..dis_visible].iter().enumerate() {
                if line.is_scale_placeholder { continue; }
                let row = (dis_status as usize + i) as u16;
                let row_bg = if din_has_grad {
                    self.row_bg_color(din_grad_offset + i, din_grad_total.max(1))
                } else {
                    self.bg_color
                };
                queue!(dw, cursor::MoveTo(0, row), SetBackgroundColor(row_bg))?;
                let mut col = 0usize;
                for span in &line.spans {
                    if col >= dis_tw { break; }
                    let span_bg = span.bg.unwrap_or(row_bg);
                    let fg = span.fg.unwrap_or(self.text_color);
                    let dimmed_fg = interpolate_color(fg, span_bg, dim);
                    let dimmed_bg = interpolate_color(span_bg, row_bg, dim);
                    for ch in span.text.chars() {
                        if col >= dis_tw { break; }
                        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
                        let group = col / 2;
                        let hash = (row as u64).wrapping_mul(31)
                            .wrapping_add(group as u64)
                            .wrapping_mul(7919) % 1000;
                        let threshold = hash as f64 / 1000.0;
                        if threshold < progress {
                            queue!(dw, SetBackgroundColor(dimmed_bg),
                                       SetForegroundColor(dimmed_fg))?;
                            write!(dw, "{}", ch)?;
                        } else {
                            queue!(dw, SetBackgroundColor(row_bg))?;
                            write!(dw, "{:width$}", "", width = cw)?;
                        }
                        col += cw;
                    }
                }
                if col < dis_tw {
                    queue!(dw, SetBackgroundColor(row_bg))?;
                    write!(dw, "{:width$}", "", width = dis_tw - col)?;
                }
            }

            // Fill remaining rows
            for i in dis_visible..dis_content_rows {
                let row = (dis_status as usize + i) as u16;
                let row_bg = if din_has_grad {
                    self.row_bg_color(din_grad_offset + i, din_grad_total.max(1))
                } else {
                    self.bg_color
                };
                queue!(dw, cursor::MoveTo(0, row), SetBackgroundColor(row_bg))?;
                write!(dw, "{:width$}", "", width = dis_tw)?;
            }

            // Emit protocol images on the final frame
            if is_last {
                for (escape_data, line_offset) in pending_protocol_images {
                    if *line_offset >= visible_start && *line_offset < visible_end {
                        let display_row = line_offset - visible_start;
                        let screen_row = (status_bar_rows + display_row) as u16;
                        queue!(dw, cursor::MoveTo(0, screen_row))?;
                        write!(dw, "{}", escape_data)?;
                    }
                }
            }

            queue!(dw, EndSynchronizedUpdate, ResetColor)?;
            dw.flush()?;
            std::thread::sleep(std::time::Duration::from_millis(25));
        }

        // The dissolve-in already revealed content, so skip any remaining
        // transition/entrance animation to avoid double-reveal.
        self.active_animation = None;
        self.needs_full_redraw = true;
        Ok(())
    }
}
