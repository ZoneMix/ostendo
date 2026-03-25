//! Multi-column layout renderer.
//!
//! Handles side-by-side column rendering with ratio-based widths,
//! syntax-highlighted code blocks, FIGlet titles, and ASCII art images
//! inside columns. Extracted from `content.rs` for maintainability.

use super::*;

/// A single row within a column during rendering.
///
/// Fields: `(spans, is_code, is_figlet, is_ascii_image)`.
/// - `spans`: The styled text fragments for this row.
/// - `is_code`: Whether the row belongs to a syntax-highlighted code block.
/// - `is_figlet`: Whether the row is part of a FIGlet ASCII art title.
/// - `is_ascii_image`: Whether the row is part of an ASCII art image.
type ColumnRow = (Vec<StyledSpan>, bool, bool, bool);

impl Presenter {
    /// Render a multi-column layout with side-by-side content.
    ///
    /// Columns are defined by ratio (e.g., `[1, 1]` for equal halves, `[2, 1]`
    /// for 2/3 + 1/3). Each column can contain bullets and code blocks.
    ///
    /// The rendering process:
    /// 1. Calculate column widths from ratios and available space.
    /// 2. Render each column's content independently into row vectors.
    /// 3. Merge columns side-by-side, padding shorter columns with empty rows.
    /// 4. Columns are separated by a dimmed `│` character.
    ///
    /// Code blocks within columns are syntax-highlighted and soft-wrapped to
    /// fit the column width (unlike slide-level code blocks which truncate).
    ///
    /// When `figlet_title` is `Some(title)`, the FIGlet ASCII art for the title
    /// is rendered inside column 0 (prepended before its body content) instead of
    /// appearing full-width above the columns. Lines originating from the FIGlet
    /// title are tagged with `LineContentType::FigletTitle` so animation targeting
    /// (e.g., `sparkle(figlet)`) continues to work.
    ///
    /// # Parameters
    ///
    /// - `cols` — The parsed column layout (ratios and per-column content).
    /// - `content_width` — Total available width for all columns combined.
    /// - `pad` — Left margin spaces.
    /// - `lines` — Output buffer.
    /// - `figlet_title` — Optional title to render as FIGlet inside column 0.
    pub(crate) fn render_columns(
        &self,
        cols: &crate::presentation::ColumnLayout,
        content_width: usize,
        pad: &str,
        lines: &mut Vec<StyledLine>,
        figlet_title: Option<&str>,
    ) {
        let total_ratio: u16 = cols.ratios.iter().map(|&r| r as u16).sum();
        if total_ratio == 0 || cols.contents.is_empty() { return; }

        // Calculate column widths
        let separator_width = 3; // " | "
        let usable = content_width.saturating_sub(separator_width * (cols.ratios.len() - 1));
        let col_widths: Vec<usize> = cols.ratios.iter()
            .map(|&r| (usable as f64 * r as f64 / total_ratio as f64).floor() as usize)
            .collect();

        // Each column row: (spans, is_code, is_figlet, is_ascii_image)
        let mut col_lines: Vec<Vec<ColumnRow>> = Vec::new();
        for (i, content) in cols.contents.iter().enumerate() {
            let cw = col_widths.get(i).copied().unwrap_or(20);
            let mut col_rows: Vec<ColumnRow> = Vec::new();

            // If this is column 0 and a FIGlet title was requested, render it here
            if i == 0 {
                if let Some(title) = figlet_title {
                    let mut figlet_lines: Vec<StyledLine> = Vec::new();
                    // Constrain FIGlet to column width so it doesn't overflow
                    self.render_ascii_title_constrained(title, "", &mut figlet_lines, Some(cw));
                    for fl in &figlet_lines {
                        let mut spans: Vec<StyledSpan> = fl.spans.clone();
                        // Apply column text_scale to FIGlet spans only if the
                        // scaled width fits within the column.
                        if let Some(scale) = cols.text_scale {
                            let figlet_width: usize = spans.iter()
                                .map(|s| unicode_width::UnicodeWidthStr::width(s.text.as_str()))
                                .sum();
                            if figlet_width * scale as usize <= cw {
                                for span in &mut spans {
                                    span.text_scale = scale;
                                }
                            }
                        }
                        col_rows.push((spans, false, true, false));
                    }
                    // Add a blank line after the FIGlet title
                    if !figlet_lines.is_empty() {
                        col_rows.push((vec![StyledSpan::new(&" ".repeat(cw))], false, false, false));
                    }
                }
            }

            // Bullets with inline formatting, themed markers, and word wrapping.
            // When column text_scale is set and this is not an image column,
            // wrap width is divided by the scale factor (each char takes scale columns)
            // and text_scale is applied to all bullet spans.
            let is_text_column = content.image.is_none();
            let scale = if is_text_column { cols.text_scale } else { None };
            let scale_factor = scale.map(|s| s as usize).unwrap_or(1);
            for bullet in &content.bullets {
                if bullet.text.is_empty() { continue; }
                let indent = bullet_indent(bullet.depth);
                // When text_scale is active, the indent also gets scaled.
                // Account for the scaled indent width when computing wrap width.
                let scaled_indent_width = indent.len() * scale_factor;
                let available = cw.saturating_sub(scaled_indent_width);
                if available == 0 { continue; }
                // Wrap width is in unscaled characters (each takes scale_factor cells)
                let wrap_width = available / scale_factor;
                let wrapped = textwrap_simple(&bullet.text, wrap_width);
                for (wi, wline) in wrapped.iter().enumerate() {
                    let mut spans = Vec::new();
                    if wi == 0 {
                        spans.push(StyledSpan::new(indent).with_fg(self.accent_color));
                    } else {
                        spans.push(StyledSpan::new(&" ".repeat(indent.len())));
                    }
                    let inline_spans = crate::markdown::parser::parse_inline_formatting(
                        wline, self.text_color, self.code_bg_color,
                    );
                    for span in inline_spans {
                        spans.push(span);
                    }
                    // Apply column text_scale to bullet text in non-image columns
                    if let Some(scale) = cols.text_scale {
                        if is_text_column {
                            for span in &mut spans {
                                span.text_scale = scale;
                            }
                        }
                    }
                    col_rows.push((spans, false, false, false));
                }
            }

            // Add spacing between bullets and code blocks
            if !content.bullets.is_empty() && !content.code_blocks.is_empty() && !col_rows.is_empty() {
                col_rows.push((vec![StyledSpan::new(&" ".repeat(cw))], false, false, false));
            }

            // Code blocks with syntax highlighting
            for cb in &content.code_blocks {
                let inner_pad = 4usize;
                let code_content_width = cw.saturating_sub(inner_pad);

                // Vertical padding top
                col_rows.push((vec![StyledSpan::new(&" ".repeat(cw)).with_bg(self.code_bg_color)], true, false, false));

                // Language label
                let label = if cb.label.is_empty() { cb.language.clone() } else { cb.label.clone() };
                if !label.is_empty() {
                    let comment_prefix = comment_prefix_for(&cb.language);
                    let label_text = format!("  {}{}", comment_prefix, label);
                    col_rows.push((vec![
                        StyledSpan::new(&label_text).with_fg(self.accent_color).with_bg(self.code_bg_color).dim(),
                    ], true, false, false));
                }

                // Highlighted code lines — soft-wrap in columns
                let highlighted = self.highlighter.highlight(&cb.code, &cb.language);
                for hline in &highlighted {
                    let mut spans = vec![StyledSpan::new("    ").with_bg(self.code_bg_color)];
                    let mut char_count = 0usize;
                    for span in hline {
                        let txt = span.text.trim_end_matches('\n');
                        let mut offset = 0usize;
                        let chars: Vec<char> = txt.chars().collect();
                        while offset < chars.len() {
                            let remaining = code_content_width.saturating_sub(char_count);
                            if remaining == 0 {
                                // Push current line and start a new wrapped line
                                col_rows.push((spans, true, false, false));
                                spans = Vec::new();
                                spans.push(StyledSpan::new("    ").with_bg(self.code_bg_color));
                                char_count = 0;
                                continue;
                            }
                            // Take as many chars as fit in remaining width
                            let mut chunk = String::new();
                            let mut chunk_w = 0usize;
                            while offset < chars.len() {
                                let cw = unicode_width::UnicodeWidthChar::width(chars[offset]).unwrap_or(0);
                                if chunk_w + cw > remaining { break; }
                                chunk.push(chars[offset]);
                                chunk_w += cw;
                                offset += 1;
                            }
                            if !chunk.is_empty() {
                                spans.push(StyledSpan::new(&chunk).with_fg(span.fg).with_bg(self.code_bg_color));
                                char_count += chunk_w;
                            }
                        }
                    }
                    col_rows.push((spans, true, false, false));
                }

                // Vertical padding bottom
                col_rows.push((vec![StyledSpan::new(&" ".repeat(cw)).with_bg(self.code_bg_color)], true, false, false));

                // Exec mode indicator for column code blocks (hidden when --no-exec)
                if cb.exec_mode.is_some() && self.allow_exec {
                    let mode_str = match cb.exec_mode {
                        Some(ExecMode::Exec) => "  [Ctrl+E to execute]",
                        Some(ExecMode::Pty) => "  [Ctrl+E to run in PTY]",
                        None => "",
                    };
                    col_rows.push((vec![
                        StyledSpan::new(mode_str).with_fg(self.accent_color).dim(),
                    ], false, false, false));
                }
            }

            // Render column image as ASCII art
            if let Some(ref col_img) = content.image {
                let img_path = std::path::Path::new(&col_img.path);
                if img_path.exists() {
                    if let Ok(img) = crate::image_util::load_image(img_path) {
                        // Calculate render width based on column width and scale
                        let scale_pct = col_img.scale.unwrap_or(100) as f64 / 100.0;
                        let render_width = (cw as f64 * scale_pct).max(10.0) as usize;

                        // Determine color override from the column image directive
                        let color_override = col_img.color.as_deref()
                            .and_then(crate::theme::colors::hex_to_color);

                        // Render as ASCII art
                        let ascii_rows = crate::terminal::ascii_art::render_ascii_art(
                            &img, render_width, color_override, Some(self.bg_color),
                        );

                        // Add a blank line before the image if there is preceding content
                        if !col_rows.is_empty() {
                            col_rows.push((
                                vec![StyledSpan::new(&" ".repeat(cw))],
                                false, false, false,
                            ));
                        }

                        // Push each ASCII art line into column rows
                        for row in &ascii_rows {
                            let spans: Vec<StyledSpan> = row.iter().map(|cell| {
                                let mut span = StyledSpan::new(&cell.ch.to_string())
                                    .with_fg(cell.fg);
                                if let Some(bg) = cell.bg {
                                    span = span.with_bg(bg);
                                }
                                span.animatable = true;
                                span
                            }).collect();
                            col_rows.push((spans, false, false, true));
                        }
                    }
                }
            }

            col_lines.push(col_rows);
        }

        // Find max height
        let max_height = col_lines.iter().map(|c| c.len()).max().unwrap_or(0);

        // Merge side-by-side
        for row in 0..max_height {
            let mut line = StyledLine::empty();
            line.push(StyledSpan::new(pad));
            let mut row_has_figlet = false;
            let mut row_has_ascii_image = false;
            for (i, col) in col_lines.iter().enumerate() {
                let cw = col_widths.get(i).copied().unwrap_or(20);
                if let Some((spans, is_code, is_figlet, is_ascii_image)) = col.get(row) {
                    if *is_figlet { row_has_figlet = true; }
                    if *is_ascii_image { row_has_ascii_image = true; }
                    // Calculate display width of spans — use .width() which
                    // accounts for OSC 66 text_scale (scaled chars occupy
                    // scale * base_width columns).
                    let span_width: usize = spans.iter()
                        .map(|s| s.width())
                        .sum();
                    // Push styled spans
                    for span in spans {
                        line.push(span.clone());
                    }
                    // Pad remaining width
                    let pad_needed = cw.saturating_sub(span_width);
                    if pad_needed > 0 {
                        if *is_code {
                            line.push(StyledSpan::new(&" ".repeat(pad_needed)).with_bg(self.code_bg_color));
                        } else {
                            line.push(StyledSpan::new(&" ".repeat(pad_needed)));
                        }
                    }
                } else {
                    // Empty row — just pad
                    line.push(StyledSpan::new(&" ".repeat(cw)));
                }

                if i < col_lines.len() - 1 {
                    if cols.separator {
                        line.push(StyledSpan::new(" │ ").with_fg(self.accent_color).dim());
                    } else {
                        line.push(StyledSpan::new("   "));
                    }
                }
            }
            // Tag lines for animation targeting.
            // Now that spin works per-span (only animatable spans are spun),
            // we can tag rows with AsciiImage even when they also contain text.
            if row_has_figlet {
                line.content_type = LineContentType::FigletTitle;
            } else if row_has_ascii_image {
                line.content_type = LineContentType::AsciiImage;
            }
            lines.push(line);
        }
    }
}
