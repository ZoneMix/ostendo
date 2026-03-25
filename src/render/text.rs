//! Virtual buffer system for terminal rendering.
//!
//! [`StyledLine`] and [`StyledSpan`] represent formatted text in memory before it is
//! written to the terminal.  This separation allows the rendering engine to build
//! complete frames in memory and write them atomically -- preventing flicker and
//! enabling features like word wrapping, padding, and animation without touching
//! the terminal until the frame is ready.
//!
//! # Key types
//!
//! - [`StyledSpan`] -- a single run of text with uniform formatting (color, bold, etc.)
//! - [`StyledLine`] -- a row of one or more spans, plus metadata like content type
//! - [`LineContentType`] -- tag that tells animations which lines to target
//!
//! # Typical flow
//!
//! 1. The parser converts Markdown into slides.
//! 2. The renderer builds `Vec<StyledLine>` for each slide.
//! 3. Wrapping and padding helpers adjust lines to fit the terminal width.
//! 4. The engine writes the final lines to stdout inside a synchronized update block.

use crossterm::style::Color;
use unicode_width::UnicodeWidthStr;

/// A contiguous run of text that shares the same visual formatting.
///
/// Think of it like a `<span>` in HTML: it holds a piece of text plus style
/// attributes (foreground color, bold, italic, etc.).  Multiple `StyledSpan`s
/// are combined into a [`StyledLine`] to represent a full row of output.
///
/// # Builder pattern
///
/// `StyledSpan` uses the *builder pattern* (common in Rust) where each method
/// consumes `self` and returns a modified copy.  This lets you chain calls:
///
/// ```ignore
/// StyledSpan::new("hello").with_fg(Color::Red).bold().italic()
/// ```
#[derive(Debug, Clone)]
pub struct StyledSpan {
    /// The raw text content of this span (no ANSI codes -- styling is applied at render time).
    pub text: String,
    /// Foreground (text) color.  `None` means use the terminal's default.
    pub fg: Option<Color>,
    /// Background color.  `None` means use the terminal's default.
    pub bg: Option<Color>,
    /// Whether the text is rendered in bold weight.
    pub bold: bool,
    /// Whether the text is rendered in italic style.
    pub italic: bool,
    /// Whether the text is rendered with reduced brightness.
    pub dim: bool,
    /// Whether the text is rendered with a horizontal line through the middle.
    pub strikethrough: bool,
    /// Whether the text is rendered with an underline.
    pub underline: bool,
    /// OSC 66 text scale (0 = normal, 2-7 = scaled). Only effective when
    /// the terminal supports TextScaleCapability::Osc66.
    /// Kitty terminal only -- other terminals ignore this field.
    pub text_scale: u8,
    /// Whether this span should participate in per-span animations (e.g., spin).
    /// When `true`, animation functions can selectively animate this span while
    /// leaving non-animatable spans on the same line unchanged.
    pub animatable: bool,
}

impl StyledSpan {
    /// Create a new span with the given text and no styling applied.
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
            text_scale: 0,
            animatable: false,
        }
    }

    /// Set the foreground (text) color.  Builder method -- returns `self` for chaining.
    pub fn with_fg(mut self, color: Color) -> Self {
        self.fg = Some(color);
        self
    }

    /// Set the background color.  Builder method -- returns `self` for chaining.
    pub fn with_bg(mut self, color: Color) -> Self {
        self.bg = Some(color);
        self
    }

    /// Enable bold weight.  Builder method -- returns `self` for chaining.
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    /// Enable italic style.  Builder method -- returns `self` for chaining.
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    /// Enable dim (reduced brightness).  Builder method -- returns `self` for chaining.
    pub fn dim(mut self) -> Self {
        self.dim = true;
        self
    }

    /// Enable strikethrough decoration.  Builder method -- returns `self` for chaining.
    pub fn strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }

    /// Enable underline decoration.  Builder method -- returns `self` for chaining.
    /// Part of the StyledSpan builder API for completeness.
    #[allow(dead_code)]
    pub fn underline(mut self) -> Self {
        self.underline = true;
        self
    }

    /// Set the OSC 66 text scale factor (Kitty terminal only).
    /// Values 2-7 enlarge the text; 0 or 1 means normal size.
    /// Used in tests; production code sets the field directly.
    #[allow(dead_code)]
    pub fn text_scale(mut self, scale: u8) -> Self {
        self.text_scale = scale;
        self
    }

    /// Calculate the display width of this span in terminal columns.
    ///
    /// Uses Unicode width rules (e.g. CJK characters count as 2 columns).
    /// If OSC 66 text scaling is active (scale >= 2), the width is multiplied
    /// by the scale factor because each character occupies more columns.
    pub fn width(&self) -> usize {
        let base = UnicodeWidthStr::width(self.text.as_str());
        if self.text_scale >= 2 {
            base * self.text_scale as usize
        } else {
            base
        }
    }

    /// Number of terminal rows this span occupies (1 for normal, scale for scaled).
    /// Used by StyledLine::height() which is exercised in tests.
    #[allow(dead_code)]
    pub fn height(&self) -> usize {
        if self.text_scale >= 2 {
            self.text_scale as usize
        } else {
            1
        }
    }
}

/// Tag indicating what kind of content a [`StyledLine`] represents.
///
/// The animation system uses this to apply effects selectively.  For example,
/// `sparkle(figlet)` only targets lines tagged as [`FigletTitle`], leaving
/// code blocks and regular text untouched.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum LineContentType {
    /// Normal body text, bullet points, blockquotes, etc.
    #[default]
    Text,
    /// Large ASCII art title rendered with the FIGlet library.
    FigletTitle,
    /// Image rendered as colored ASCII characters (fallback mode).
    AsciiImage,
    /// Syntax-highlighted code block.
    /// Reserved for future animation targeting (e.g., `sparkle(code)`).
    #[allow(dead_code)]
    CodeBlock,
    /// Mermaid or other diagram rendered as ASCII.
    Diagram,
    /// Empty spacing line used for vertical alignment or scaled-text placeholders.
    /// Used by scale_placeholder() for OSC 66 text scaling rows.
    #[allow(dead_code)]
    Padding,
}

/// A single row of terminal output, composed of one or more [`StyledSpan`]s.
///
/// This is the main unit of the virtual buffer.  The rendering engine builds a
/// `Vec<StyledLine>` for each slide, then writes the whole vector to stdout in
/// one pass.
#[derive(Debug, Clone)]
pub struct StyledLine {
    /// The formatted text segments that make up this line, rendered left to right.
    pub spans: Vec<StyledSpan>,
    /// When true, this line is a placeholder row for a scaled multicell block
    /// above it and should be skipped during terminal output (not overwritten).
    pub is_scale_placeholder: bool,
    /// What kind of content this line represents (for targeted animations).
    pub content_type: LineContentType,
}

impl StyledLine {
    /// Create a blank line with no spans (renders as an empty row).
    pub fn empty() -> Self {
        Self { spans: Vec::new(), is_scale_placeholder: false, content_type: LineContentType::default() }
    }

    /// Create a line containing a single unstyled text span.
    pub fn plain(text: &str) -> Self {
        Self {
            spans: vec![StyledSpan::new(text)],
            is_scale_placeholder: false,
            content_type: LineContentType::default(),
        }
    }

    /// Create a line containing a single span with the given foreground color.
    /// Part of the StyledLine builder API for completeness.
    #[allow(dead_code)]
    pub fn styled(text: &str, fg: Color) -> Self {
        Self {
            spans: vec![StyledSpan::new(text).with_fg(fg)],
            is_scale_placeholder: false,
            content_type: LineContentType::default(),
        }
    }

    /// Create a placeholder line for scaled text (skipped during rendering).
    /// Reserved for OSC 66 multicell text support.
    #[allow(dead_code)]
    pub fn scale_placeholder() -> Self {
        Self { spans: Vec::new(), is_scale_placeholder: true, content_type: LineContentType::Padding }
    }

    /// Total display width of this line in terminal columns (sum of all span widths).
    pub fn width(&self) -> usize {
        self.spans.iter().map(|s| s.width()).sum()
    }

    /// Maximum height among all spans (for OSC 66 scaled text).
    /// Used in tests; reserved for OSC 66 multicell rendering.
    #[allow(dead_code)]
    pub fn height(&self) -> usize {
        self.spans.iter().map(|s| s.height()).max().unwrap_or(1)
    }

    /// Append a span to the end of this line.
    pub fn push(&mut self, span: StyledSpan) {
        self.spans.push(span);
    }
}

/// Word-wrap a slice of styled lines so every line fits within `max_width` columns.
///
/// Lines that already fit (or contain OSC 66 scaled spans) are passed through
/// unchanged.  Long lines are split at word boundaries, inheriting the style of
/// the first span in the original line.
///
/// # Parameters
///
/// - `lines` -- the input lines to wrap.
/// - `max_width` -- the maximum allowed width in terminal columns.
///
/// # Returns
///
/// A new `Vec<StyledLine>` where every entry is at most `max_width` columns wide.
/// Used in tests; available as a utility for future callers.
#[allow(dead_code)]
pub fn wrap_styled_lines(lines: &[StyledLine], max_width: usize) -> Vec<StyledLine> {
    let mut result = Vec::new();
    for line in lines {
        // Skip wrapping for lines containing scaled spans (OSC 66) — they use
        // virtual width that doesn't correspond to actual terminal columns.
        let has_scaled = line.spans.iter().any(|s| s.text_scale >= 2);
        if has_scaled || line.width() <= max_width {
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
                    is_scale_placeholder: false,
                    content_type: LineContentType::default(),
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
                is_scale_placeholder: false,
                content_type: LineContentType::default(),
            });
        }
    }
    result
}

/// Pad or truncate a line to exactly `width` terminal columns.
///
/// - If the line is shorter than `width`, spaces are appended.
/// - If the line is longer, spans are truncated character-by-character.
/// - If the line is already the right width, it is cloned unchanged.
///
/// This is used to produce fixed-width rows so the terminal background fills
/// the entire window evenly (important for gradient themes).
/// Used in tests; available as a utility for future callers.
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

    #[test]
    fn test_text_scale_width() {
        let span = StyledSpan::new("Hi").text_scale(3);
        // "Hi" is 2 chars wide, at 3x scale = 6
        assert_eq!(span.width(), 6);
        assert_eq!(span.height(), 3);
    }

    #[test]
    fn test_text_scale_zero_is_normal() {
        let span = StyledSpan::new("abc").text_scale(0);
        assert_eq!(span.width(), 3);
        assert_eq!(span.height(), 1);
    }

    #[test]
    fn test_line_height_with_scaled_span() {
        let mut line = StyledLine::empty();
        line.push(StyledSpan::new("pad"));
        line.push(StyledSpan::new("Title").text_scale(3));
        assert_eq!(line.height(), 3);
    }

    #[test]
    fn test_line_height_normal() {
        let line = StyledLine::plain("normal");
        assert_eq!(line.height(), 1);
    }
}
