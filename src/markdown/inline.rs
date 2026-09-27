//! Inline markdown (`**bold**`, `*italic*` / `_italic_`, `~~strike~~`, `` `code` ``, links) to
//! styled spans, applied by the renderer to each wrapped line.
//!
//! Links are `[text](url)`, `<https://…>`, and bare `http(s)://` URLs; a target containing
//! whitespace or control characters stays literal text, so it can never reach the terminal
//! inside an escape sequence.
//!
//! Emphasis follows simplified CommonMark flanking rules: a delimiter run opens only before
//! non-whitespace and closes only after non-whitespace, `_` never opens or closes inside a
//! word, and a run without a matching closer is literal text. Styles nest, so code inside bold
//! keeps the bold flag.

use crate::render::text::StyledSpan;
use crossterm::style::Color;

#[derive(Clone, Copy, Default)]
struct Style {
    bold: bool,
    italic: bool,
    strike: bool,
    /// Character range of the link target.
    link: Option<(usize, usize)>,
}

impl Style {
    /// The style inside a delimiter run of `len` copies of `ch`, if that run is a delimiter.
    fn inside(self, ch: char, len: usize) -> Option<Style> {
        match (ch, len) {
            ('*' | '_', 1) => Some(Style {
                italic: true,
                ..self
            }),
            ('*' | '_', 2) => Some(Style { bold: true, ..self }),
            ('*' | '_', 3) => Some(Style {
                bold: true,
                italic: true,
                ..self
            }),
            ('~', 2) => Some(Style {
                strike: true,
                ..self
            }),
            _ => None,
        }
    }
}

/// (text range, or `None` to show the target; target range; index after the link).
type Link = (Option<(usize, usize)>, (usize, usize), usize);

struct Inline<'a> {
    chars: &'a [char],
    fg: Color,
    code_bg: Color,
    spans: Vec<StyledSpan>,
}

pub fn parse_inline_formatting(text: &str, base_fg: Color, code_bg: Color) -> Vec<StyledSpan> {
    let chars: Vec<char> = text.chars().collect();
    let mut inline = Inline {
        chars: &chars,
        fg: base_fg,
        code_bg,
        spans: Vec::new(),
    };
    inline.parse(0, chars.len(), Style::default());
    if inline.spans.is_empty() {
        inline.spans.push(StyledSpan::new("").with_fg(base_fg));
    }
    inline.spans
}

fn is_delimiter(ch: char) -> bool {
    matches!(ch, '*' | '_' | '~' | '`')
}

impl Inline<'_> {
    fn parse(&mut self, start: usize, end: usize, style: Style) {
        let mut plain = String::new();
        let mut i = start;
        while i < end {
            let ch = self.chars[i];
            if let Some((text, target, next)) = self.link_at(i, end, style) {
                self.push(std::mem::take(&mut plain), style, false);
                let linked = Style {
                    link: Some(target),
                    ..style
                };
                match text {
                    Some((a, b)) => self.parse(a, b, linked),
                    None => self.push(
                        self.chars[target.0..target.1].iter().collect(),
                        linked,
                        false,
                    ),
                }
                i = next;
                continue;
            }
            if !is_delimiter(ch) {
                plain.push(ch);
                i += 1;
                continue;
            }
            let len = self.run_len(i, end);
            let inner = if ch == '`' {
                Some(style)
            } else {
                style.inside(ch, len)
            };
            let close = inner.and_then(|_| self.find_close(i, len, end));
            match (inner, close) {
                (Some(inner), Some(close)) => {
                    self.push(std::mem::take(&mut plain), style, false);
                    if ch == '`' {
                        let code: String = self.chars[i + len..close].iter().collect();
                        // CommonMark: one space on each side lets code start
                        // or end with a backtick (`` `` ``` `` ``).
                        let code = match code.strip_prefix(' ').and_then(|c| c.strip_suffix(' ')) {
                            Some(inner) if !inner.trim().is_empty() => inner.to_string(),
                            _ => code,
                        };
                        self.push(format!(" {code} "), style, true);
                    } else {
                        self.parse(i + len, close, inner);
                    }
                    i = close + len;
                }
                _ => {
                    plain.extend(&self.chars[i..i + len]);
                    i += len;
                }
            }
        }
        self.push(plain, style, false);
    }

    /// A link starting at `i`.
    fn link_at(&self, i: usize, end: usize, style: Style) -> Option<Link> {
        if style.link.is_some() {
            return None;
        }
        let chars = &self.chars[..end];
        let find = |from: usize, open: char, close: char| {
            let mut depth = 0usize;
            for (j, &c) in chars.iter().enumerate().skip(from) {
                if c == open {
                    depth += 1;
                } else if c == close {
                    if depth == 0 {
                        return Some(j);
                    }
                    depth -= 1;
                }
            }
            None
        };
        let valid = |(a, b): (usize, usize)| {
            a < b
                && chars[a..b]
                    .iter()
                    .all(|c| !c.is_whitespace() && !c.is_control())
        };
        let starts = |at: usize, prefix: &str| {
            prefix
                .chars()
                .enumerate()
                .all(|(k, p)| chars.get(at + k) == Some(&p))
        };
        match chars[i] {
            '[' => {
                let close = find(i + 1, '[', ']')?;
                if chars.get(close + 1) != Some(&'(') {
                    return None;
                }
                let paren = find(close + 2, '(', ')')?;
                let target_end = (close + 2..paren)
                    .find(|&j| chars[j].is_whitespace())
                    .unwrap_or(paren);
                // Anything after the target must be a quoted title, which is dropped.
                let title: String = chars[target_end..paren].iter().collect();
                let title = title.trim();
                let quoted = title.is_empty()
                    || (title.len() >= 2
                        && [('"', '"'), ('\'', '\'')]
                            .iter()
                            .any(|&(a, b)| title.starts_with(a) && title.ends_with(b)));
                let target = (close + 2, target_end);
                (quoted && valid(target)).then_some((Some((i + 1, close)), target, paren + 1))
            }
            '<' => {
                let close = find(i + 1, '<', '>')?;
                let target = (i + 1, close);
                let scheme = ["http://", "https://", "mailto:"]
                    .iter()
                    .any(|p| starts(i + 1, p));
                (scheme && valid(target)).then_some((None, target, close + 1))
            }
            'h' => {
                let boundary = i
                    .checked_sub(1)
                    .is_none_or(|p| chars[p].is_whitespace() || chars[p] == '(');
                let scheme = ["https://", "http://"]
                    .into_iter()
                    .find(|p| starts(i, p))?
                    .len();
                if !boundary {
                    return None;
                }
                let mut stop = (i..end).find(|&j| chars[j].is_whitespace()).unwrap_or(end);
                // Sentence punctuation after a URL is not part of it.
                while stop > i
                    && matches!(
                        chars[stop - 1],
                        '.' | ',' | ';' | ':' | '!' | '?' | ')' | '\'' | '"'
                    )
                {
                    stop -= 1;
                }
                let target = (i, stop);
                (stop > i + scheme && valid(target)).then_some((None, target, stop))
            }
            _ => None,
        }
    }

    fn run_len(&self, i: usize, end: usize) -> usize {
        self.chars[i..end]
            .iter()
            .take_while(|&&c| c == self.chars[i])
            .count()
    }

    /// Start of the run that closes the `len`-long run at `open`. Code spans close on the next
    /// run of equal length; emphasis skips over code spans and needs flanking on both ends.
    fn find_close(&self, open: usize, len: usize, end: usize) -> Option<usize> {
        let ch = self.chars[open];
        if ch != '`' && !self.can_open(open, len, end) {
            return None;
        }
        let mut j = open + len;
        while j < end {
            let c = self.chars[j];
            if !is_delimiter(c) {
                j += 1;
                continue;
            }
            let run = self.run_len(j, end);
            if c == ch && run == len && (ch == '`' || self.can_close(j, len)) {
                return Some(j);
            }
            let code_end = (c == '`' && ch != '`')
                .then(|| self.find_close(j, run, end))
                .flatten();
            j = code_end.unwrap_or(j) + run;
        }
        None
    }

    fn can_open(&self, i: usize, len: usize, end: usize) -> bool {
        let next = self.chars[i + len..end].first();
        let prev = i.checked_sub(1).map(|p| self.chars[p]);
        next.is_some_and(|c| !c.is_whitespace())
            && (self.chars[i] != '_' || !prev.is_some_and(char::is_alphanumeric))
    }

    fn can_close(&self, i: usize, len: usize) -> bool {
        let prev = i.checked_sub(1).map(|p| self.chars[p]);
        let next = self.chars.get(i + len);
        prev.is_some_and(|c| !c.is_whitespace())
            && (self.chars[i] != '_' || !next.is_some_and(|c| c.is_alphanumeric()))
    }

    fn push(&mut self, text: String, style: Style, code: bool) {
        if text.is_empty() {
            return;
        }
        let mut span = StyledSpan::new(&text).with_fg(self.fg);
        if style.bold {
            span = span.bold();
        }
        if style.italic {
            span = span.italic();
        }
        if style.strike {
            span = span.strikethrough();
        }
        if code {
            span = span.with_bg(self.code_bg);
        }
        if let Some((a, b)) = style.link {
            span.link = Some(self.chars[a..b].iter().collect::<String>().into());
            span.underline = true;
        }
        self.spans.push(span);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FG: Color = Color::White;
    const BG: Color = Color::DarkGrey;

    /// Each span as (text, flags) with b = bold, i = italic, s = strikethrough, c = code.
    fn spans(input: &str) -> Vec<(String, String)> {
        parse_inline_formatting(input, FG, BG)
            .into_iter()
            .map(|s| {
                assert_eq!(s.fg, Some(FG), "{input}");
                let flags = [
                    (s.bold, 'b'),
                    (s.italic, 'i'),
                    (s.strikethrough, 's'),
                    (s.bg == Some(BG), 'c'),
                ];
                (s.text, flags.iter().filter(|f| f.0).map(|f| f.1).collect())
            })
            .collect()
    }

    #[test]
    fn inline_spans() {
        let cases: &[(&str, &[(&str, &str)])] = &[
            ("plain text", &[("plain text", "")]),
            ("", &[("", "")]),
            ("hello **world**", &[("hello ", ""), ("world", "b")]),
            (
                "*one* and _two_",
                &[("one", "i"), (" and ", ""), ("two", "i")],
            ),
            ("***both***", &[("both", "bi")]),
            ("~~gone~~ now", &[("gone", "s"), (" now", "")]),
            ("use `println!`", &[("use ", ""), (" println! ", "c")]),
            ("`a *b* c`", &[(" a *b* c ", "c")]),
            ("`` ```rust ``", &[(" ```rust ", "c")]),
            (
                "**Bold *and italic* mixed**",
                &[("Bold ", "b"), ("and italic", "bi"), (" mixed", "b")],
            ),
            ("*a **b** c*", &[("a ", "i"), ("b", "bi"), (" c", "i")]),
            (
                "**Bold with `code` inside**",
                &[("Bold with ", "b"), (" code ", "bc"), (" inside", "b")],
            ),
            (
                "*italic with ~~struck~~ words*",
                &[("italic with ", "i"), ("struck", "is"), (" words", "i")],
            ),
            // Literal: intraword underscores, spaced or unmatched stars, unclosed runs.
            (
                "my_var_name in config_file.rs",
                &[("my_var_name in config_file.rs", "")],
            ),
            ("a_b_c _ok_", &[("a_b_c ", ""), ("ok", "i")]),
            ("5 * 3 = 15, see *.rs", &[("5 * 3 = 15, see *.rs", "")]),
            ("**unclosed bold", &[("**unclosed bold", "")]),
            ("`unclosed code", &[("`unclosed code", "")]),
        ];
        for (input, expected) in cases {
            let expected: Vec<(String, String)> = expected
                .iter()
                .map(|(t, f)| (t.to_string(), f.to_string()))
                .collect();
            assert_eq!(spans(input), expected, "{input}");
        }
    }

    #[test]
    fn links_carry_their_target() {
        let links = |input: &str| -> Vec<(String, Option<String>)> {
            parse_inline_formatting(input, FG, BG)
                .into_iter()
                .map(|s| (s.text, s.link.map(|l| l.to_string())))
                .filter(|(_, l)| l.is_some())
                .collect()
        };
        let link = |t: &str, u: &str| (t.to_string(), Some(u.to_string()));
        assert_eq!(
            links("see [the **docs**](https://x.dev \"Title\") or <https://a.b>."),
            [
                link("the ", "https://x.dev"),
                link("docs", "https://x.dev"),
                link("https://a.b", "https://a.b"),
            ]
        );
        assert_eq!(
            links("(via https://c.d/e?q=1), then http://f.g."),
            [
                link("https://c.d/e?q=1", "https://c.d/e?q=1"),
                link("http://f.g", "http://f.g")
            ]
        );
        for literal in [
            "[a](has space)",
            "[esc](http://x\x1b]0;pwned\x07)",
            "<ftp://old>",
            "xhttps://glued",
            "https://",
            "[checkbox] text",
        ] {
            assert!(links(literal).is_empty(), "{literal:?}");
        }
    }
}
