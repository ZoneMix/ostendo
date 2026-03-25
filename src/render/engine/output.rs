//! Output helper functions for terminal rendering.
//!
//! Standalone utility functions that format and write text to the terminal
//! output buffer. These do not require `&self` access to `Presenter`.

use anyhow::Result;
use std::io::Write;

use crate::render::text::StyledSpan;

/// Parse a line containing ANSI color escape sequences into styled spans.
/// Converts SGR codes (e.g., \x1B[31m for red) into StyledSpan foreground colors.
/// Falls back to `default_fg` for unrecognized codes or when reset (\x1B[0m) is used.
pub(crate) fn parse_ansi_styled_spans(s: &str, default_fg: crossterm::style::Color) -> Vec<StyledSpan> {
    use crossterm::style::Color;

    let mut spans: Vec<StyledSpan> = Vec::new();
    let mut current_fg = default_fg;
    let mut is_bold = false;
    let mut text = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1B' {
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                // Flush accumulated text
                if !text.is_empty() {
                    let mut span = StyledSpan::new(&text).with_fg(current_fg);
                    if is_bold { span = span.bold(); }
                    spans.push(span);
                    text.clear();
                }
                // Parse SGR parameters
                let mut params = String::new();
                while let Some(&next) = chars.peek() {
                    if next.is_ascii_alphabetic() || next == '~' {
                        chars.next();
                        if next == 'm' {
                            // Process SGR codes
                            for code in params.split(';') {
                                match code.trim() {
                                    "0" | "" => { current_fg = default_fg; is_bold = false; }
                                    "1" => { is_bold = true; }
                                    "22" => { is_bold = false; }
                                    "30" => current_fg = Color::Black,
                                    "31" => current_fg = Color::Red,
                                    "32" => current_fg = Color::Green,
                                    "33" => current_fg = Color::Yellow,
                                    "34" => current_fg = Color::Blue,
                                    "35" => current_fg = Color::Magenta,
                                    "36" => current_fg = Color::Cyan,
                                    "37" => current_fg = Color::White,
                                    "90" => current_fg = Color::DarkGrey,
                                    "91" => current_fg = Color::DarkRed,
                                    "92" => current_fg = Color::DarkGreen,
                                    "93" => current_fg = Color::DarkYellow,
                                    "94" => current_fg = Color::DarkBlue,
                                    "95" => current_fg = Color::DarkMagenta,
                                    "96" => current_fg = Color::DarkCyan,
                                    "97" => current_fg = Color::Grey,
                                    _ => {} // ignore unknown codes
                                }
                            }
                        }
                        break;
                    }
                    chars.next();
                    params.push(next);
                }
            } else if chars.peek() == Some(&']') {
                // Skip OSC sequences
                chars.next();
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next == '\x07' { break; }
                    if next == '\x1B' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            continue;
        }
        if c == '\t' || (c >= ' ' && c != '\x7F') {
            text.push(c);
        }
    }
    // Flush remaining text
    if !text.is_empty() {
        let mut span = StyledSpan::new(&text).with_fg(current_fg);
        if is_bold { span = span.bold(); }
        spans.push(span);
    }
    spans
}

/// Get comment prefix for a programming language (used in code block labels).
pub(crate) fn comment_prefix_for(lang: &str) -> &'static str {
    match lang {
        "python" | "python3" | "py" | "bash" | "sh" | "ruby" | "rb" | "yaml" | "toml" | "r" => "# ",
        "html" | "xml" => "<!-- ",
        "css" => "/* ",
        "sql" | "lua" | "haskell" => "-- ",
        "c" | "cpp" | "c++" | "java" | "javascript" | "js" | "typescript" | "go" | "golang" | "rust"
        | "swift" | "kotlin" | "scala" | "php" | "dart" | "zig" => "// ",
        _ => "// ",
    }
}

/// Write text to the output buffer, applying OSC 66 text scaling when `scale >= 2`.
///
/// OSC 66 is a Kitty-specific escape sequence that tells the terminal to
/// render text at a larger size (2x, 3x, etc.) within a single cell span.
/// When scale is 1 (or 0), the text is written directly without any escape.
pub(crate) fn write_span_text(w: &mut impl Write, scale: u8, text: &str) -> Result<()> {
    if scale >= 2 {
        write!(w, "\x1b]66;s={};{}\x07", scale, text)?;
    } else {
        write!(w, "{}", text)?;
    }
    Ok(())
}

/// Truncate a string to fit within `max_cols` display columns.
///
/// Uses Unicode character widths (not byte count) so that wide characters
/// (CJK, emoji) are measured correctly. For example, a full-width character
/// counts as 2 columns.
pub(crate) fn truncate_to_width(s: &str, max_cols: usize) -> String {
    use unicode_width::UnicodeWidthChar;
    let mut result = String::new();
    let mut w = 0;
    for ch in s.chars() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
        if w + cw > max_cols {
            break;
        }
        result.push(ch);
        w += cw;
    }
    result
}

/// Simple word-wrapping: splits text at word boundaries to fit within `width` display columns.
///
/// Words that exceed the width on their own are placed on a single line without
/// breaking mid-word. Returns a `Vec<String>` where each entry is one wrapped line.
pub(crate) fn textwrap_simple(text: &str, width: usize) -> Vec<String> {
    if width == 0 || text.is_empty() {
        return vec![text.to_string()];
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in words {
        let test = if current.is_empty() { word.to_string() } else { format!("{} {}", current, word) };
        if unicode_width::UnicodeWidthStr::width(test.as_str()) > width && !current.is_empty() {
            lines.push(current);
            current = word.to_string();
        } else {
            current = test;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}
