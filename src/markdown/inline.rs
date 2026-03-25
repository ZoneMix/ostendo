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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::style::Color;

    const FG: Color = Color::White;
    const BG: Color = Color::DarkGrey;

    // --- helpers ---

    fn text_of(spans: &[StyledSpan]) -> String {
        spans.iter().map(|s| s.text.as_str()).collect()
    }

    fn find_span<'a>(spans: &'a [StyledSpan], fragment: &str) -> Option<&'a StyledSpan> {
        spans.iter().find(|s| s.text.contains(fragment))
    }

    // --- plain text ---

    #[test]
    fn plain_text_returns_single_span() {
        let spans = parse_inline_formatting("plain text", FG, BG);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "plain text");
        assert!(!spans[0].bold);
        assert!(!spans[0].italic);
        assert!(!spans[0].strikethrough);
        assert!(spans[0].bg.is_none());
    }

    #[test]
    fn empty_string_returns_single_empty_span() {
        let spans = parse_inline_formatting("", FG, BG);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "");
    }

    // --- bold ---

    #[test]
    fn bold_marker_produces_bold_span() {
        let spans = parse_inline_formatting("**bold**", FG, BG);
        let bold_span = find_span(&spans, "bold").expect("bold span not found");
        assert!(bold_span.bold);
        assert!(!bold_span.italic);
    }

    #[test]
    fn bold_preserves_surrounding_text() {
        let spans = parse_inline_formatting("pre **bold** post", FG, BG);
        assert!(find_span(&spans, "pre ").is_some());
        assert!(find_span(&spans, " post").is_some());
        let bold = find_span(&spans, "bold").unwrap();
        assert!(bold.bold);
    }

    // --- italic ---

    #[test]
    fn italic_star_produces_italic_span() {
        let spans = parse_inline_formatting("*italic*", FG, BG);
        let span = find_span(&spans, "italic").expect("italic span not found");
        assert!(span.italic);
        assert!(!span.bold);
    }

    #[test]
    fn italic_underscore_produces_italic_span() {
        let spans = parse_inline_formatting("_italic_", FG, BG);
        let span = find_span(&spans, "italic").expect("italic span not found");
        assert!(span.italic);
    }

    // --- code ---

    #[test]
    fn backtick_code_uses_code_bg_and_padding() {
        let spans = parse_inline_formatting("`code`", FG, BG);
        let code_span = spans.iter().find(|s| s.bg.is_some()).expect("code span with bg not found");
        assert_eq!(code_span.bg, Some(BG));
        // inline code is wrapped with a leading and trailing space
        assert!(code_span.text.contains("code"));
        assert!(code_span.text.starts_with(' '));
        assert!(code_span.text.ends_with(' '));
    }

    #[test]
    fn backtick_code_does_not_set_bold() {
        let spans = parse_inline_formatting("`snippet`", FG, BG);
        let code_span = spans.iter().find(|s| s.bg.is_some()).unwrap();
        assert!(!code_span.bold);
    }

    // --- strikethrough ---

    #[test]
    fn strikethrough_marker_sets_flag() {
        let spans = parse_inline_formatting("~~strike~~", FG, BG);
        let span = find_span(&spans, "strike").expect("strikethrough span not found");
        assert!(span.strikethrough);
        assert!(!span.bold);
        assert!(!span.italic);
    }

    // --- mixed ---

    #[test]
    fn mixed_bold_and_italic_produces_multiple_spans() {
        let spans = parse_inline_formatting("**bold** and *italic*", FG, BG);
        let bold = find_span(&spans, "bold").expect("bold span missing");
        let italic = find_span(&spans, "italic").expect("italic span missing");
        assert!(bold.bold);
        assert!(italic.italic);
        assert!(!italic.bold);
    }

    #[test]
    fn full_text_is_preserved_across_spans() {
        let input = "**bold** and *italic*";
        let spans = parse_inline_formatting(input, FG, BG);
        assert_eq!(text_of(&spans), input.replace("**", "").replace('*', ""));
    }

    // --- nested bold+italic ---

    #[test]
    fn italic_nested_inside_bold_sets_both_flags() {
        let spans = parse_inline_formatting("**outer *inner* end**", FG, BG);
        let nested = find_span(&spans, "inner").expect("nested italic not found");
        assert!(nested.bold, "nested span should be bold");
        assert!(nested.italic, "nested span should be italic");
        let outer = find_span(&spans, "outer").or_else(|| find_span(&spans, "end"));
        if let Some(o) = outer {
            assert!(o.bold);
            assert!(!o.italic);
        }
    }

    // --- fg color propagation ---

    #[test]
    fn all_spans_use_base_fg() {
        let spans = parse_inline_formatting("**a** b *c*", FG, BG);
        for s in &spans {
            if s.bg.is_none() {
                // non-code spans use base_fg
                assert_eq!(s.fg, Some(FG));
            }
        }
    }
}
