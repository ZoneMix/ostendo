//! In-memory styled text: frames are built as `Vec<StyledLine>` and written to
//! the terminal in one pass.

use crossterm::style::Color;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// A run of text with uniform styling. `None` colors inherit the row's defaults.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StyledSpan {
    pub text: String,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    pub dim: bool,
    pub strikethrough: bool,
    pub underline: bool,
    /// OSC 66 scale factor (Kitty); 0 or 1 means normal size. Each character
    /// then occupies `text_scale` columns and rows.
    pub text_scale: u8,
    /// Whether per-span loop animations (spin) may alter this span.
    pub animatable: bool,
}

impl StyledSpan {
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            ..Self::default()
        }
    }

    pub fn with_fg(mut self, color: Color) -> Self {
        self.fg = Some(color);
        self
    }

    pub fn with_bg(mut self, color: Color) -> Self {
        self.bg = Some(color);
        self
    }

    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    pub fn dim(mut self) -> Self {
        self.dim = true;
        self
    }

    pub fn strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }

    /// Display width in columns, accounting for wide characters and OSC 66 scaling.
    pub fn width(&self) -> usize {
        self.text.width() * usize::from(self.text_scale.max(1))
    }
}

/// What a line holds, so loop animations can target it (`sparkle(figlet)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineContentType {
    #[default]
    Text,
    FigletTitle,
    AsciiImage,
    Diagram,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StyledLine {
    pub spans: Vec<StyledSpan>,
    pub content_type: LineContentType,
}

impl StyledLine {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn plain(text: &str) -> Self {
        Self {
            spans: vec![StyledSpan::new(text)],
            content_type: LineContentType::Text,
        }
    }

    pub fn width(&self) -> usize {
        self.spans.iter().map(StyledSpan::width).sum()
    }

    pub fn push(&mut self, span: StyledSpan) {
        self.spans.push(span);
    }

    pub fn is_blank(&self) -> bool {
        self.spans
            .iter()
            .all(|s| s.bg.is_none() && s.text.chars().all(char::is_whitespace))
    }
}

/// Truncates `s` to at most `max_cols` display columns.
pub fn truncate_to_width(s: &str, max_cols: usize) -> String {
    let mut out = String::new();
    let mut w = 0;
    for ch in s.chars() {
        let cw = ch.width().unwrap_or(0);
        if w + cw > max_cols {
            break;
        }
        out.push(ch);
        w += cw;
    }
    out
}

/// Truncates to `max_cols`, ending with `…` when something was cut.
pub fn ellipsize(s: &str, max_cols: usize) -> String {
    if s.width() <= max_cols {
        return s.to_string();
    }
    if max_cols == 0 {
        return String::new();
    }
    let mut out = truncate_to_width(s, max_cols - 1);
    out.push('…');
    out
}

/// Word-wraps `text` to `width` display columns. Words longer than a line are
/// split so no line exceeds `width`.
pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_w = 0;
    for word in text.split_whitespace() {
        let mut word = word;
        loop {
            let word_w = word.width();
            let sep = usize::from(current_w > 0);
            if current_w + sep + word_w <= width {
                if sep == 1 {
                    current.push(' ');
                }
                current.push_str(word);
                current_w += sep + word_w;
                break;
            }
            if current_w > 0 {
                lines.push(std::mem::take(&mut current));
                current_w = 0;
                continue;
            }
            // A word wider than the line is hard-split; always take at least
            // one character so a glyph wider than `width` cannot stall.
            let mut head_len = truncate_to_width(word, width).len();
            if head_len == 0 {
                head_len = word.chars().next().map_or(word.len(), char::len_utf8);
            }
            lines.push(word[..head_len].to_string());
            word = &word[head_len..];
            if word.is_empty() {
                break;
            }
        }
    }
    if current_w > 0 || lines.is_empty() {
        lines.push(current);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_text_measures_display_width_and_splits_long_words() {
        let cases: &[(&str, usize, &[&str])] = &[
            ("hello world foo", 11, &["hello world", "foo"]),
            ("", 10, &[""]),
            ("日本語 テキスト", 6, &["日本語", "テキス", "ト"]),
            ("abcdefghij", 4, &["abcd", "efgh", "ij"]),
            ("界", 1, &["界"]),
        ];
        for (text, width, expected) in cases {
            assert_eq!(wrap_text(text, *width), *expected, "{text:?} @ {width}");
        }
    }

    #[test]
    fn truncation_respects_wide_characters() {
        assert_eq!(truncate_to_width("a→b", 2), "a→");
        assert_eq!(truncate_to_width("日本", 3), "日");
        assert_eq!(ellipsize("presentation", 6), "prese…");
        assert_eq!(ellipsize("short", 6), "short");
    }
}
