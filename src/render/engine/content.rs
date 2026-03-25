//! Content element renderers for tables, columns, ASCII art titles, code
//! execution output, and decorated titles.
//!
//! Each function in this module appends styled lines to a `Vec<StyledLine>`
//! buffer. They are called by `render_frame()` in `rendering.rs` during the
//! content assembly phase.
//!
//! # Rendering Pattern
//!
//! All renderers follow the same pattern:
//! 1. Accept a reference to `self` (for theme colors, highlighter, etc.).
//! 2. Accept `pad` — a string of spaces for the left margin (content centering).
//! 3. Accept `lines` — the mutable output buffer to append to.
//! 4. Build `StyledLine` objects from `StyledSpan` parts and push them.

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
    /// Render the output of the most recent code execution.
    ///
    /// Displays an "Output:" header in accent color followed by the captured
    /// stdout/stderr. Long lines are word-wrapped to fit within the content area.
    /// If no execution output exists (`exec_output` is `None`), this is a no-op.
    ///
    /// # Parameters
    ///
    /// - `pad` — Left margin spaces for centering content within the terminal.
    /// - `lines` — The virtual buffer to append output lines to.
    pub(crate) fn render_exec_output(&self, pad: &str, lines: &mut Vec<StyledLine>) {
        if let Some(ref output) = self.exec_output {
            let prefix_width = pad.len() + 2; // pad + "  "
            let wrap_width = (self.width as usize).saturating_sub(prefix_width + 1);
            lines.push(StyledLine::empty());
            let mut oh = StyledLine::empty();
            oh.push(StyledSpan::new(pad));
            oh.push(StyledSpan::new("  Output:").with_fg(self.accent_color).bold());
            lines.push(oh);
            for ol in output.lines() {
                // Parse ANSI color codes into styled spans (preserves colors from scripts)
                let styled_spans = parse_ansi_styled_spans(ol, self.text_color);
                // Calculate total display width of the styled spans
                let total_width: usize = styled_spans.iter()
                    .map(|s| unicode_width::UnicodeWidthStr::width(s.text.as_str()))
                    .sum();

                if wrap_width > 0 && total_width > wrap_width {
                    // Wrap: flatten spans into (char, color, bold) tuples, then re-chunk
                    let mut flat: Vec<(char, crossterm::style::Color, bool)> = Vec::new();
                    for span in &styled_spans {
                        let fg = span.fg.unwrap_or(self.text_color);
                        let bold = span.bold;
                        for c in span.text.chars() {
                            flat.push((c, fg, bold));
                        }
                    }
                    let mut pos = 0;
                    while pos < flat.len() {
                        let mut line = StyledLine::empty();
                        line.push(StyledSpan::new(pad));
                        line.push(StyledSpan::new("  "));
                        let mut w = 0;
                        let mut current_text = String::new();
                        let mut current_fg = flat[pos].1;
                        let mut current_bold = flat[pos].2;
                        while pos < flat.len() {
                            let (ch, fg, bold) = flat[pos];
                            let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
                            if w + cw > wrap_width { break; }
                            // Flush if color/bold changes
                            if fg != current_fg || bold != current_bold {
                                if !current_text.is_empty() {
                                    let mut span = StyledSpan::new(&current_text).with_fg(current_fg);
                                    if current_bold { span = span.bold(); }
                                    line.push(span);
                                    current_text.clear();
                                }
                                current_fg = fg;
                                current_bold = bold;
                            }
                            current_text.push(ch);
                            w += cw;
                            pos += 1;
                        }
                        if !current_text.is_empty() {
                            let mut span = StyledSpan::new(&current_text).with_fg(current_fg);
                            if current_bold { span = span.bold(); }
                            line.push(span);
                        }
                        lines.push(line);
                    }
                } else {
                    let mut line = StyledLine::empty();
                    line.push(StyledSpan::new(pad));
                    line.push(StyledSpan::new("  "));
                    for span in styled_spans {
                        line.push(span);
                    }
                    lines.push(line);
                }
            }
        }
    }

    /// Render a slide title as large text.
    ///
    /// Uses one of three strategies:
    /// 1. **OSC 66** (Kitty only): Native text scaling at 2x-3x — crisp, uses
    ///    the terminal font, and renders multi-line titles correctly.
    /// 2. **FIGlet**: ASCII art rendering using the loaded FIGlet font.
    /// 3. **Plain bold**: Fallback when neither OSC 66 nor FIGlet fits.
    ///
    /// Each line is tagged with the appropriate `LineContentType` for
    /// targeted animations.
    /// Render a title as FIGlet ASCII art.
    ///
    /// When `max_width` is `Some(w)`, constrain the FIGlet to fit within `w`
    /// columns (used for rendering inside column layouts). When `None`, use
    /// the full terminal width.
    pub(crate) fn render_ascii_title(&self, title: &str, pad: &str, lines: &mut Vec<StyledLine>) {
        self.render_ascii_title_constrained(title, pad, lines, None);
    }

    pub(crate) fn render_ascii_title_constrained(
        &self, title: &str, pad: &str, lines: &mut Vec<StyledLine>, max_width: Option<usize>,
    ) {
        // OSC 66 path: only used as fallback when FIGlet font is NOT loaded.
        // FIGlet is the preferred path because it supports sparkle/spin animations.
        // OSC 66 is the fallback for terminals where FIGlet can't render.
        if self.figfont.is_none()
            && self.text_scale_cap == crate::terminal::protocols::TextScaleCapability::Osc66
        {
            let content_width = max_width.unwrap_or(self.width as usize - pad.len());
            let title_width = unicode_width::UnicodeWidthStr::width(title);

            let scale = if title_width * 3 <= content_width { 3u8 }
                        else if title_width * 2 <= content_width { 2u8 }
                        else { 0u8 };

            if scale >= 2 {
                let mut line = StyledLine::empty();
                line.push(StyledSpan::new(pad));
                let mut span = StyledSpan::new(title)
                    .with_fg(self.accent_color)
                    .bold();
                span.text_scale = scale;
                line.push(span);
                line.content_type = LineContentType::FigletTitle;
                lines.push(line);
                for _ in 1..scale {
                    lines.push(StyledLine::empty());
                }
                return;
            }
        }

        let fig = match self.figfont.as_ref() {
            Some(f) => f,
            None => {
                // Graceful fallback: render as plain bold title
                let mut line = StyledLine::empty();
                line.push(StyledSpan::new(pad));
                line.push(StyledSpan::new(title).with_fg(self.accent_color).bold());
                lines.push(line);
                return;
            }
        };
        let content_width = max_width.unwrap_or(self.width as usize - pad.len());

        // Helper: check if rendered FIGlet fits within content_width
        let fits = |text: &str| -> Option<String> {
            fig.convert(text).and_then(|rendered| {
                let s = rendered.to_string();
                let max_w = s.lines().map(|l| l.chars().count()).max().unwrap_or(0);
                if max_w <= content_width { Some(s) } else { None }
            })
        };

        // Try full title first
        if let Some(rendered_str) = fits(title) {
            for fig_line in rendered_str.lines() {
                let mut line = StyledLine::empty();
                line.push(StyledSpan::new(pad));
                line.push(StyledSpan::new(fig_line).with_fg(self.accent_color).bold());
                line.content_type = LineContentType::FigletTitle;
                lines.push(line);
            }
            return;
        }

        // Try splitting into words, rendering each word on its own FIGlet line
        let words: Vec<&str> = title.split_whitespace().collect();
        if words.len() > 1 {
            let mut all_fit = true;
            let mut word_renders: Vec<String> = Vec::new();
            for word in &words {
                if let Some(rendered_str) = fits(word) {
                    word_renders.push(rendered_str);
                } else {
                    all_fit = false;
                    break;
                }
            }
            if all_fit {
                for rendered_str in &word_renders {
                    for fig_line in rendered_str.lines() {
                        let mut line = StyledLine::empty();
                        line.push(StyledSpan::new(pad));
                        line.push(StyledSpan::new(fig_line).with_fg(self.accent_color).bold());
                        line.content_type = LineContentType::FigletTitle;
                        lines.push(line);
                    }
                }
                return;
            }
        }

        // Fallback: plain bold title when FIGlet doesn't fit
        let mut line = StyledLine::empty();
        line.push(StyledSpan::new(pad));
        line.push(StyledSpan::new(title).with_fg(self.accent_color).bold());
        lines.push(line);
    }

    /// Render a slide title with a decorative style.
    ///
    /// Supports four decoration modes (set via `<!-- title_decoration: ... -->`
    /// or in the theme YAML):
    ///
    /// - `"underline"` — Title text followed by a line of `─` characters.
    /// - `"box"` — Title enclosed in a Unicode box-drawing border (`┌─┐│└─┘`).
    /// - `"banner"` — Full-width inverted bar (accent background, bg foreground).
    /// - `"none"` or unrecognized — Plain bold title (same as no decoration).
    ///
    /// # Parameters
    ///
    /// - `title` — The title text to render.
    /// - `decoration` — The decoration style name.
    /// - `content_width` — Available width in columns (for banner full-width).
    /// - `pad` — Left margin spaces.
    /// - `lines` — Output buffer.
    pub(crate) fn render_title_decorated(
        &self,
        title: &str,
        decoration: &str,
        content_width: usize,
        pad: &str,
        lines: &mut Vec<StyledLine>,
    ) {
        let title_width = unicode_width::UnicodeWidthStr::width(title);
        match decoration {
            "underline" => {
                let mut tl = StyledLine::empty();
                tl.push(StyledSpan::new(pad));
                tl.push(StyledSpan::new(title).with_fg(self.accent_color).bold());
                lines.push(tl);
                let mut ul = StyledLine::empty();
                ul.push(StyledSpan::new(pad));
                ul.push(StyledSpan::new(&"─".repeat(title_width)).with_fg(self.accent_color));
                lines.push(ul);
            }
            "box" => {
                let box_w = title_width + 4; // 2 padding each side
                let top = format!("┌{}┐", "─".repeat(box_w.saturating_sub(2)));
                let mid = format!("│ {} │", title);
                let bot = format!("└{}┘", "─".repeat(box_w.saturating_sub(2)));
                for s in [&top, &mid, &bot] {
                    let mut line = StyledLine::empty();
                    line.push(StyledSpan::new(pad));
                    line.push(StyledSpan::new(s).with_fg(self.accent_color).bold());
                    lines.push(line);
                }
            }
            "banner" => {
                let banner_w = content_width;
                let text_pad = banner_w.saturating_sub(title_width + 2);
                let left = text_pad / 2;
                let right = text_pad - left;
                let banner_text = format!("{}{}{}", " ".repeat(left + 1), title, " ".repeat(right + 1));
                let mut line = StyledLine::empty();
                line.push(StyledSpan::new(pad));
                line.push(StyledSpan::new(&banner_text).with_fg(self.bg_color).with_bg(self.accent_color).bold());
                lines.push(line);
            }
            _ => {
                // "none" or unknown — plain title
                let mut line = StyledLine::empty();
                line.push(StyledSpan::new(pad));
                line.push(StyledSpan::new(title).with_fg(self.accent_color).bold());
                lines.push(line);
            }
        }
    }

    /// Render a markdown table with Unicode box-drawing borders.
    ///
    /// Builds a bordered table with:
    /// - Top border: `┌───┬───┐`
    /// - Header row: `│ Name │ Value │` (bold accent)
    /// - Separator:  `├───┼───┤`
    /// - Data rows:  `│ data │ data  │` (text color)
    /// - Bottom:     `└───┴───┘`
    ///
    /// Column widths are auto-calculated from content. Each column respects
    /// the alignment specified in the markdown separator row (`:---`, `:---:`, `---:`).
    ///
    /// # Parameters
    ///
    /// - `table` — The parsed table data (headers, rows, alignments).
    /// - `content_width` — Maximum available width for the table.
    /// - `pad` — Left margin spaces.
    /// - `lines` — Output buffer.
    pub(crate) fn render_table(
        &self,
        table: &crate::presentation::Table,
        content_width: usize,
        pad: &str,
        lines: &mut Vec<StyledLine>,
    ) {
        use crate::presentation::TableAlign;

        let num_cols = table.headers.len();
        if num_cols == 0 { return; }

        // Calculate column widths based on content
        let mut col_widths: Vec<usize> = table.headers.iter().map(|h| h.len()).collect();
        for row in &table.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < col_widths.len() {
                    col_widths[i] = col_widths[i].max(cell.len());
                }
            }
        }
        // Add padding (1 space each side)
        let col_widths: Vec<usize> = col_widths.iter().map(|w| w + 2).collect();
        let total_w: usize = col_widths.iter().sum::<usize>() + num_cols + 1; // +1 for borders
        // Ensure we don't exceed content_width
        let _ = total_w.min(content_width);

        // Helper to format a cell with alignment
        let fmt_cell = |text: &str, width: usize, align: TableAlign| -> String {
            let inner_w = width.saturating_sub(2); // minus padding
            let truncated = if text.len() > inner_w { &text[..inner_w] } else { text };
            let pad_total = inner_w.saturating_sub(truncated.len());
            match align {
                TableAlign::Right => format!(" {:>width$} ", truncated, width = inner_w),
                TableAlign::Center => {
                    let left_pad = pad_total / 2;
                    let right_pad = pad_total - left_pad;
                    format!(" {}{}{} ", " ".repeat(left_pad), truncated, " ".repeat(right_pad))
                }
                TableAlign::Left => format!(" {:<width$} ", truncated, width = inner_w),
            }
        };

        let get_align = |i: usize| -> TableAlign {
            table.alignments.get(i).copied().unwrap_or(TableAlign::Left)
        };

        // Top border: ┌───┬───┐
        let mut top = String::from("┌");
        for (i, w) in col_widths.iter().enumerate() {
            top.push_str(&"─".repeat(*w));
            if i < num_cols - 1 { top.push('┬'); } else { top.push('┐'); }
        }
        let mut tl = StyledLine::empty();
        tl.push(StyledSpan::new(pad));
        tl.push(StyledSpan::new("  "));
        tl.push(StyledSpan::new(&top).with_fg(self.accent_color).dim());
        lines.push(tl);

        // Header row: │ Name │ Value │
        let mut hl = StyledLine::empty();
        hl.push(StyledSpan::new(pad));
        hl.push(StyledSpan::new("  "));
        hl.push(StyledSpan::new("│").with_fg(self.accent_color).dim());
        for (i, header) in table.headers.iter().enumerate() {
            let cell = fmt_cell(header, col_widths[i], get_align(i));
            hl.push(StyledSpan::new(&cell).with_fg(self.accent_color).bold());
            hl.push(StyledSpan::new("│").with_fg(self.accent_color).dim());
        }
        lines.push(hl);

        // Header separator: ├───┼───┤
        let mut sep = String::from("├");
        for (i, w) in col_widths.iter().enumerate() {
            sep.push_str(&"─".repeat(*w));
            if i < num_cols - 1 { sep.push('┼'); } else { sep.push('┤'); }
        }
        let mut sl = StyledLine::empty();
        sl.push(StyledSpan::new(pad));
        sl.push(StyledSpan::new("  "));
        sl.push(StyledSpan::new(&sep).with_fg(self.accent_color).dim());
        lines.push(sl);

        // Data rows
        for row in &table.rows {
            let mut rl = StyledLine::empty();
            rl.push(StyledSpan::new(pad));
            rl.push(StyledSpan::new("  "));
            rl.push(StyledSpan::new("│").with_fg(self.accent_color).dim());
            for (i, cell) in row.iter().enumerate() {
                let w = if i < col_widths.len() { col_widths[i] } else { cell.len() + 2 };
                let formatted = fmt_cell(cell, w, get_align(i));
                rl.push(StyledSpan::new(&formatted).with_fg(self.text_color));
                rl.push(StyledSpan::new("│").with_fg(self.accent_color).dim());
            }
            lines.push(rl);
        }

        // Bottom border: └───┴───┘
        let mut bot = String::from("└");
        for (i, w) in col_widths.iter().enumerate() {
            bot.push_str(&"─".repeat(*w));
            if i < num_cols - 1 { bot.push('┴'); } else { bot.push('┘'); }
        }
        let mut bl = StyledLine::empty();
        bl.push(StyledSpan::new(pad));
        bl.push(StyledSpan::new("  "));
        bl.push(StyledSpan::new(&bot).with_fg(self.accent_color).dim());
        lines.push(bl);
    }

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
