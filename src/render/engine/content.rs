//! Content element renderers for ASCII art titles, code execution output,
//! and decorated titles.
//!
//! Table rendering lives in `table_render.rs` and column layout rendering
//! lives in `columns.rs`.
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

    /// Render a FIGlet ASCII art title with an optional maximum width constraint.
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

}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::style::Color;

    const DEFAULT_FG: Color = Color::White;

    // --- parse_ansi_styled_spans ---

    #[test]
    fn no_ansi_codes_returns_single_span() {
        let spans = parse_ansi_styled_spans("hello world", DEFAULT_FG);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "hello world");
        assert_eq!(spans[0].fg, Some(DEFAULT_FG));
        assert!(!spans[0].bold);
    }

    #[test]
    fn empty_string_returns_no_spans() {
        let spans = parse_ansi_styled_spans("", DEFAULT_FG);
        assert!(spans.is_empty());
    }

    #[test]
    fn red_color_code_produces_red_span() {
        // \x1B[31m = red foreground
        let input = "\x1B[31mred text\x1B[0m";
        let spans = parse_ansi_styled_spans(input, DEFAULT_FG);
        let red_span = spans.iter().find(|s| s.text == "red text").expect("red span not found");
        assert_eq!(red_span.fg, Some(Color::Red));
    }

    #[test]
    fn green_color_code_produces_green_span() {
        let input = "\x1B[32mgreen\x1B[0m";
        let spans = parse_ansi_styled_spans(input, DEFAULT_FG);
        let span = spans.iter().find(|s| s.text == "green").expect("green span not found");
        assert_eq!(span.fg, Some(Color::Green));
    }

    #[test]
    fn blue_color_code_produces_blue_span() {
        let input = "\x1B[34mblue\x1B[0m";
        let spans = parse_ansi_styled_spans(input, DEFAULT_FG);
        let span = spans.iter().find(|s| s.text == "blue").expect("blue span not found");
        assert_eq!(span.fg, Some(Color::Blue));
    }

    #[test]
    fn reset_code_restores_default_fg() {
        // text1 in red, reset, text2 in default
        let input = "\x1B[31mred\x1B[0mdefault";
        let spans = parse_ansi_styled_spans(input, DEFAULT_FG);
        let default_span = spans.iter().find(|s| s.text == "default").expect("default span missing");
        assert_eq!(default_span.fg, Some(DEFAULT_FG));
    }

    #[test]
    fn bold_code_sets_bold_flag() {
        let input = "\x1B[1mbold text\x1B[0m";
        let spans = parse_ansi_styled_spans(input, DEFAULT_FG);
        let bold_span = spans.iter().find(|s| s.text == "bold text").expect("bold span missing");
        assert!(bold_span.bold);
    }

    #[test]
    fn bold_then_reset_clears_bold() {
        let input = "\x1B[1mbold\x1B[0mplain";
        let spans = parse_ansi_styled_spans(input, DEFAULT_FG);
        let plain_span = spans.iter().find(|s| s.text == "plain").expect("plain span missing");
        assert!(!plain_span.bold);
    }

    #[test]
    fn multiple_color_codes_on_one_line_produce_multiple_spans() {
        // red text followed by blue text
        let input = "\x1B[31mred\x1B[34mblue";
        let spans = parse_ansi_styled_spans(input, DEFAULT_FG);
        assert!(spans.len() >= 2, "expected at least 2 spans for red+blue input");
        let red = spans.iter().find(|s| s.text == "red").expect("red span missing");
        let blue = spans.iter().find(|s| s.text == "blue").expect("blue span missing");
        assert_eq!(red.fg, Some(Color::Red));
        assert_eq!(blue.fg, Some(Color::Blue));
    }

    #[test]
    fn yellow_color_code_produces_yellow_span() {
        let input = "\x1B[33myellow\x1B[0m";
        let spans = parse_ansi_styled_spans(input, DEFAULT_FG);
        let span = spans.iter().find(|s| s.text == "yellow").unwrap();
        assert_eq!(span.fg, Some(Color::Yellow));
    }

    #[test]
    fn cyan_color_code_produces_cyan_span() {
        let input = "\x1B[36mcyan\x1B[0m";
        let spans = parse_ansi_styled_spans(input, DEFAULT_FG);
        let span = spans.iter().find(|s| s.text == "cyan").unwrap();
        assert_eq!(span.fg, Some(Color::Cyan));
    }

    #[test]
    fn text_before_first_escape_uses_default_fg() {
        let input = "prefix\x1B[31mred";
        let spans = parse_ansi_styled_spans(input, DEFAULT_FG);
        let prefix = spans.iter().find(|s| s.text == "prefix").expect("prefix span missing");
        assert_eq!(prefix.fg, Some(DEFAULT_FG));
    }
}
