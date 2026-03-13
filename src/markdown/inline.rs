//! Inline Markdown formatting parser.
//!
//! Converts raw inline text (e.g., bullet or subtitle strings) into a sequence of
//! `StyledSpan` values carrying bold, italic, code, and strikethrough flags. Called
//! during **rendering**, not during slide parsing.

use crate::render::text::StyledSpan;

/// Parses inline Markdown formatting into a vector of styled text spans.
///
/// # Supported formatting
/// - `**bold**` — applies the `bold` flag. Supports nested `*italic*` inside bold.
/// - `*italic*` or `_italic_` — applies the `italic` flag.
/// - `` `inline code` `` — applies the `code_bg` background color and pads with spaces.
/// - `~~strikethrough~~` — applies the `strikethrough` flag.
///
/// # Parameters
/// - `text`: The raw inline text to parse (e.g., a bullet or subtitle string).
/// - `base_fg`: The default foreground color for unstyled text.
/// - `code_bg`: The background color used for inline code spans.
///
/// # Returns
/// A `Vec<StyledSpan>` where each span covers a contiguous run of identically-formatted text.
/// If the input contains no formatting markers, a single span wrapping the entire text is returned.
///
/// # Algorithm
/// The function walks through the character array one position at a time, looking for opening
/// markers (`**`, `~~`, `` ` ``, `*`, `_`). When it finds one, it flushes any accumulated
/// plain text as a span, then scans forward for the matching closing marker. The text between
/// the markers becomes a new span with the appropriate formatting flag set.
pub fn parse_inline_formatting(
    text: &str,
    base_fg: crossterm::style::Color,
    code_bg: crossterm::style::Color,
) -> Vec<StyledSpan> {
    let mut spans = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut pos = 0;
    let mut current = String::new();

    while pos < chars.len() {
        // Check for ** (bold) — recursively handles *italic* inside bold
        if pos + 1 < chars.len() && chars[pos] == '*' && chars[pos + 1] == '*' {
            if !current.is_empty() {
                spans.push(StyledSpan::new(&current).with_fg(base_fg));
                current.clear();
            }
            pos += 2;
            let mut inner = String::new();
            while pos + 1 < chars.len() && !(chars[pos] == '*' && chars[pos + 1] == '*') {
                inner.push(chars[pos]);
                pos += 1;
            }
            if pos + 1 < chars.len() { pos += 2; } // skip closing **
            if !inner.is_empty() {
                // Parse inner content for nested italic (*...*) within bold
                let inner_chars: Vec<char> = inner.chars().collect();
                let mut ipos = 0;
                let mut plain = String::new();
                while ipos < inner_chars.len() {
                    if inner_chars[ipos] == '*' {
                        if !plain.is_empty() {
                            spans.push(StyledSpan::new(&plain).with_fg(base_fg).bold());
                            plain.clear();
                        }
                        ipos += 1;
                        let mut italic_text = String::new();
                        while ipos < inner_chars.len() && inner_chars[ipos] != '*' {
                            italic_text.push(inner_chars[ipos]);
                            ipos += 1;
                        }
                        if ipos < inner_chars.len() { ipos += 1; } // skip closing *
                        if !italic_text.is_empty() {
                            spans.push(StyledSpan::new(&italic_text).with_fg(base_fg).bold().italic());
                        }
                    } else {
                        plain.push(inner_chars[ipos]);
                        ipos += 1;
                    }
                }
                if !plain.is_empty() {
                    spans.push(StyledSpan::new(&plain).with_fg(base_fg).bold());
                }
            }
            continue;
        }
        // Check for ~~ (strikethrough)
        if pos + 1 < chars.len() && chars[pos] == '~' && chars[pos + 1] == '~' {
            if !current.is_empty() {
                spans.push(StyledSpan::new(&current).with_fg(base_fg));
                current.clear();
            }
            pos += 2;
            let mut inner = String::new();
            while pos + 1 < chars.len() && !(chars[pos] == '~' && chars[pos + 1] == '~') {
                inner.push(chars[pos]);
                pos += 1;
            }
            if pos + 1 < chars.len() { pos += 2; }
            if !inner.is_empty() {
                spans.push(StyledSpan::new(&inner).with_fg(base_fg).strikethrough());
            }
            continue;
        }
        // Check for ` (inline code)
        if chars[pos] == '`' {
            if !current.is_empty() {
                spans.push(StyledSpan::new(&current).with_fg(base_fg));
                current.clear();
            }
            pos += 1;
            let mut inner = String::new();
            while pos < chars.len() && chars[pos] != '`' {
                inner.push(chars[pos]);
                pos += 1;
            }
            if pos < chars.len() { pos += 1; } // skip closing `
            if !inner.is_empty() {
                spans.push(StyledSpan::new(&format!(" {} ", inner)).with_fg(base_fg).with_bg(code_bg));
            }
            continue;
        }
        // Check for * or _ (italic) — single, not double
        if (chars[pos] == '*' || chars[pos] == '_')
            && (pos + 1 >= chars.len() || chars[pos + 1] != chars[pos])
        {
            let marker = chars[pos];
            if !current.is_empty() {
                spans.push(StyledSpan::new(&current).with_fg(base_fg));
                current.clear();
            }
            pos += 1;
            let mut inner = String::new();
            while pos < chars.len() && chars[pos] != marker {
                inner.push(chars[pos]);
                pos += 1;
            }
            if pos < chars.len() { pos += 1; } // skip closing marker
            if !inner.is_empty() {
                spans.push(StyledSpan::new(&inner).with_fg(base_fg).italic());
            }
            continue;
        }
        current.push(chars[pos]);
        pos += 1;
    }

    if !current.is_empty() {
        spans.push(StyledSpan::new(&current).with_fg(base_fg));
    }

    // If no formatting was found, return a single span
    if spans.is_empty() {
        spans.push(StyledSpan::new(text).with_fg(base_fg));
    }

    spans
}
