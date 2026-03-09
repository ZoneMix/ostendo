use crossterm::style::Color;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct StyledSpan {
    pub text: String,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    pub dim: bool,
    pub strikethrough: bool,
    pub underline: bool,
}

#[allow(dead_code)]
impl StyledSpan {
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            fg: None,
            bg: None,
            bold: false,
            italic: false,
            dim: false,
            strikethrough: false,
            underline: false,
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

    pub fn underline(mut self) -> Self {
        self.underline = true;
        self
    }

    pub fn width(&self) -> usize {
        UnicodeWidthStr::width(self.text.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct StyledLine {
    pub spans: Vec<StyledSpan>,
}

#[allow(dead_code)]
impl StyledLine {
    pub fn empty() -> Self {
        Self { spans: Vec::new() }
    }

    pub fn plain(text: &str) -> Self {
        Self {
            spans: vec![StyledSpan::new(text)],
        }
    }

    pub fn styled(text: &str, fg: Color) -> Self {
        Self {
            spans: vec![StyledSpan::new(text).with_fg(fg)],
        }
    }

    pub fn width(&self) -> usize {
        self.spans.iter().map(|s| s.width()).sum()
    }

    pub fn push(&mut self, span: StyledSpan) {
        self.spans.push(span);
    }
}

/// Wrap a styled line into multiple lines that fit within `max_width`.
#[allow(dead_code)]
pub fn wrap_styled_lines(lines: &[StyledLine], max_width: usize) -> Vec<StyledLine> {
    let mut result = Vec::new();
    for line in lines {
        if line.width() <= max_width {
            result.push(line.clone());
            continue;
        }
        // Simple word-boundary wrapping
        let full_text: String = line.spans.iter().map(|s| s.text.as_str()).collect();
        let words: Vec<&str> = full_text.split_whitespace().collect();
        if words.is_empty() {
            result.push(StyledLine::empty());
            continue;
        }

        // Use first span's style for wrapped continuation
        let style_ref = &line.spans[0];
        let mut current = String::new();
        for word in &words {
            let test = if current.is_empty() {
                word.to_string()
            } else {
                format!("{} {}", current, word)
            };
            if UnicodeWidthStr::width(test.as_str()) > max_width && !current.is_empty() {
                result.push(StyledLine {
                    spans: vec![StyledSpan {
                        text: current,
                        ..style_ref.clone()
                    }],
                });
                current = word.to_string();
            } else {
                current = test;
            }
        }
        if !current.is_empty() {
            result.push(StyledLine {
                spans: vec![StyledSpan {
                    text: current,
                    ..style_ref.clone()
                }],
            });
        }
    }
    result
}

/// Pad or truncate a line to exactly `width` characters.
#[allow(dead_code)]
pub fn pad_line(line: &StyledLine, width: usize) -> StyledLine {
    let current = line.width();
    if current == width {
        return line.clone();
    }
    if current < width {
        let mut padded = line.clone();
        padded.push(StyledSpan::new(&" ".repeat(width - current)));
        return padded;
    }
    // Truncate: walk spans and cut at width
    let mut result = StyledLine::empty();
    let mut remaining = width;
    for span in &line.spans {
        if remaining == 0 {
            break;
        }
        let sw = span.width();
        if sw <= remaining {
            result.push(span.clone());
            remaining -= sw;
        } else {
            // Truncate this span
            let truncated: String = span.text.chars().take(remaining).collect();
            result.push(StyledSpan {
                text: truncated,
                ..span.clone()
            });
            remaining = 0;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_width() {
        assert_eq!(StyledSpan::new("hello").width(), 5);
        assert_eq!(StyledSpan::new("").width(), 0);
    }

    #[test]
    fn test_line_width() {
        let mut line = StyledLine::empty();
        assert_eq!(line.width(), 0);
        line.push(StyledSpan::new("abc"));
        line.push(StyledSpan::new("de"));
        assert_eq!(line.width(), 5);
    }

    #[test]
    fn test_plain_line() {
        let line = StyledLine::plain("test");
        assert_eq!(line.width(), 4);
        assert_eq!(line.spans.len(), 1);
    }

    #[test]
    fn test_wrap_short_line() {
        let line = StyledLine::plain("short");
        let wrapped = wrap_styled_lines(&[line], 80);
        assert_eq!(wrapped.len(), 1);
    }

    #[test]
    fn test_wrap_long_line() {
        let line = StyledLine::plain("this is a long line that should be wrapped at some point");
        let wrapped = wrap_styled_lines(&[line], 20);
        assert!(wrapped.len() > 1);
        for w in &wrapped {
            assert!(w.width() <= 20);
        }
    }

    #[test]
    fn test_pad_line_shorter() {
        let line = StyledLine::plain("hi");
        let padded = pad_line(&line, 10);
        assert_eq!(padded.width(), 10);
    }

    #[test]
    fn test_pad_line_exact() {
        let line = StyledLine::plain("exact");
        let padded = pad_line(&line, 5);
        assert_eq!(padded.width(), 5);
    }

    #[test]
    fn test_empty_line() {
        let line = StyledLine::empty();
        assert_eq!(line.width(), 0);
        assert!(line.spans.is_empty());
    }
}
