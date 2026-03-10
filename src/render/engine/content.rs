use super::*;

impl Presenter {
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
                let sanitized = strip_control_chars(ol);
                if wrap_width > 0 && unicode_width::UnicodeWidthStr::width(sanitized.as_str()) > wrap_width {
                    // Wrap long lines by character width
                    let chars: Vec<char> = sanitized.chars().collect();
                    let mut pos = 0;
                    while pos < chars.len() {
                        let mut line = StyledLine::empty();
                        line.push(StyledSpan::new(pad));
                        line.push(StyledSpan::new("  "));
                        let mut chunk = String::new();
                        let mut w = 0;
                        while pos < chars.len() {
                            let cw = unicode_width::UnicodeWidthChar::width(chars[pos]).unwrap_or(0);
                            if w + cw > wrap_width { break; }
                            chunk.push(chars[pos]);
                            w += cw;
                            pos += 1;
                        }
                        line.push(StyledSpan::new(&chunk).with_fg(self.text_color));
                        lines.push(line);
                    }
                } else {
                    let mut line = StyledLine::empty();
                    line.push(StyledSpan::new(pad));
                    line.push(StyledSpan::new("  "));
                    line.push(StyledSpan::new(&sanitized).with_fg(self.text_color));
                    lines.push(line);
                }
            }
        }
    }

    pub(crate) fn render_ascii_title(&self, title: &str, pad: &str, lines: &mut Vec<StyledLine>) {
        let font_data = include_str!("../../../fonts/slant.flf");
        let fig = match figlet_rs::FIGfont::from_content(font_data)
            .or_else(|_| figlet_rs::FIGfont::standard())
        {
            Ok(f) => f,
            Err(_) => {
                // Graceful fallback: render as plain bold title
                let mut line = StyledLine::empty();
                line.push(StyledSpan::new(pad));
                line.push(StyledSpan::new(title).with_fg(self.accent_color).bold());
                lines.push(line);
                return;
            }
        };
        let content_width = self.width as usize - pad.len();

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

    pub(crate) fn render_columns(
        &self,
        cols: &crate::presentation::ColumnLayout,
        content_width: usize,
        pad: &str,
        lines: &mut Vec<StyledLine>,
    ) {
        let total_ratio: u16 = cols.ratios.iter().map(|&r| r as u16).sum();
        if total_ratio == 0 || cols.contents.is_empty() { return; }

        // Calculate column widths
        let separator_width = 3; // " | "
        let usable = content_width.saturating_sub(separator_width * (cols.ratios.len() - 1));
        let col_widths: Vec<usize> = cols.ratios.iter()
            .map(|&r| (usable as f64 * r as f64 / total_ratio as f64).floor() as usize)
            .collect();

        // Each column row: (spans, is_code) — styled spans instead of plain text
        let mut col_lines: Vec<Vec<(Vec<StyledSpan>, bool)>> = Vec::new();
        for (i, content) in cols.contents.iter().enumerate() {
            let cw = col_widths.get(i).copied().unwrap_or(20);
            let mut col_rows: Vec<(Vec<StyledSpan>, bool)> = Vec::new();

            // Bullets with inline formatting, themed markers, and word wrapping
            for bullet in &content.bullets {
                if bullet.text.is_empty() { continue; }
                let indent = match bullet.depth {
                    0 => "  * ",
                    1 => "      - ",
                    _ => "          > ",
                };
                let text_width = cw.saturating_sub(indent.len());
                if text_width == 0 { continue; }
                let wrapped = textwrap_simple(&bullet.text, text_width);
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
                    col_rows.push((spans, false));
                }
            }

            // Add spacing between bullets and code blocks
            if !content.bullets.is_empty() && !content.code_blocks.is_empty() && !col_rows.is_empty() {
                col_rows.push((vec![StyledSpan::new(&" ".repeat(cw))], false));
            }

            // Code blocks with syntax highlighting
            for cb in &content.code_blocks {
                let inner_pad = 4usize;
                let code_content_width = cw.saturating_sub(inner_pad);

                // Vertical padding top
                col_rows.push((vec![StyledSpan::new(&" ".repeat(cw)).with_bg(self.code_bg_color)], true));

                // Language label
                let label = if cb.label.is_empty() { cb.language.clone() } else { cb.label.clone() };
                if !label.is_empty() {
                    let comment_prefix = comment_prefix_for(&cb.language);
                    let label_text = format!("  {}{}", comment_prefix, label);
                    col_rows.push((vec![
                        StyledSpan::new(&label_text).with_fg(self.accent_color).with_bg(self.code_bg_color).dim(),
                    ], true));
                }

                // Highlighted code lines — soft-wrap in columns
                let highlighted = self.highlighter.highlight(&cb.code, &cb.language);
                for hline in &highlighted {
                    let mut spans = Vec::new();
                    spans.push(StyledSpan::new("    ").with_bg(self.code_bg_color));
                    let mut char_count = 0usize;
                    for span in hline {
                        let txt = span.text.trim_end_matches('\n');
                        let mut offset = 0usize;
                        let chars: Vec<char> = txt.chars().collect();
                        while offset < chars.len() {
                            let remaining = code_content_width.saturating_sub(char_count);
                            if remaining == 0 {
                                // Push current line and start a new wrapped line
                                col_rows.push((spans, true));
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
                    col_rows.push((spans, true));
                }

                // Vertical padding bottom
                col_rows.push((vec![StyledSpan::new(&" ".repeat(cw)).with_bg(self.code_bg_color)], true));

                // Exec mode indicator for column code blocks (hidden when --no-exec)
                if cb.exec_mode.is_some() && self.allow_exec {
                    let mode_str = match cb.exec_mode {
                        Some(ExecMode::Exec) => "  [Ctrl+E to execute]",
                        Some(ExecMode::Pty) => "  [Ctrl+E to run in PTY]",
                        None => "",
                    };
                    col_rows.push((vec![
                        StyledSpan::new(mode_str).with_fg(self.accent_color).dim(),
                    ], false));
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
            for (i, col) in col_lines.iter().enumerate() {
                let cw = col_widths.get(i).copied().unwrap_or(20);
                if let Some((spans, is_code)) = col.get(row) {
                    // Calculate display width of spans
                    let span_width: usize = spans.iter()
                        .map(|s| unicode_width::UnicodeWidthStr::width(s.text.as_str()))
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
                    line.push(StyledSpan::new(" │ ").with_fg(self.accent_color).dim());
                }
            }
            lines.push(line);
        }
    }
}
