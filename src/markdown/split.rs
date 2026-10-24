//! Deck-level structure: the front matter block and `---` slide boundaries.

use super::regex_patterns::FRONT_MATTER_KV_RE;

/// A ```` ``` ```` or `~~~` fence marker (3+ characters at column 0).
#[derive(Clone, Copy)]
pub(super) struct Fence {
    tilde: bool,
    len: usize,
}

impl Fence {
    /// Returns the fence and its trimmed info string.
    pub(super) fn parse(line: &str) -> Option<(Fence, &str)> {
        let ch = *line
            .as_bytes()
            .first()
            .filter(|&&b| b == b'`' || b == b'~')?;
        let len = line.bytes().take_while(|&b| b == ch).count();
        let info = &line[len..];
        if len < 3 || (ch == b'`' && info.contains('`')) {
            return None;
        }
        Some((
            Fence {
                tilde: ch == b'~',
                len,
            },
            info.trim(),
        ))
    }

    pub(super) fn is_closed_by(self, line: &str) -> bool {
        matches!(Fence::parse(line), Some((close, "")) if close.tilde == self.tilde && close.len >= self.len)
    }
}

/// A `<!--` line that does not close on the same line.
pub(super) fn opens_comment(line: &str) -> bool {
    line.trim_start().starts_with("<!--") && !line.contains("-->")
}

/// Splits off a leading `---` block made of `key: value` (or indented) lines. Anything else
/// after an opening `---` is a slide, so decks that start with a separator keep slide 1.
pub(super) fn split_front_matter<'a>(
    lines: &'a [&'a str],
) -> (Option<&'a [&'a str]>, &'a [&'a str]) {
    let start = lines.iter().take_while(|l| l.trim().is_empty()).count();
    let is_separator = |l: &&str| l.trim_end() == "---";
    if lines.get(start).is_some_and(is_separator) {
        let rest = &lines[start + 1..];
        if let Some(len) = rest.iter().position(is_separator) {
            let block = &rest[..len];
            let is_meta = |l: &&str| {
                l.trim().is_empty()
                    || l.starts_with(char::is_whitespace)
                    || FRONT_MATTER_KV_RE.is_match(l)
            };
            if block.iter().all(is_meta) {
                return (Some(block), &rest[len + 1..]);
            }
        }
    }
    (None, lines)
}

/// Splits on `---` lines outside fences and multi-line comments, returning the line
/// range of each slide. An opener with no closer anywhere below is an ordinary line, so
/// one unclosed fence cannot swallow the deck.
pub(super) fn split_slides(lines: &[&str]) -> Vec<std::ops::Range<usize>> {
    let last_comment_end = lines.iter().rposition(|l| l.contains("-->"));
    // longest_close[i][tilde]: longest bare fence of that kind at or after line i.
    let mut longest_close = vec![[0usize; 2]; lines.len() + 1];
    for (i, line) in lines.iter().enumerate().rev() {
        longest_close[i] = longest_close[i + 1];
        if let Some((fence, "")) = Fence::parse(line) {
            let slot = &mut longest_close[i][usize::from(fence.tilde)];
            *slot = (*slot).max(fence.len);
        }
    }

    let mut blocks = Vec::new();
    let mut start = 0;
    let mut fence: Option<Fence> = None;
    let mut in_comment = false;
    for (i, line) in lines.iter().enumerate() {
        if let Some(open) = fence {
            if open.is_closed_by(line) {
                fence = None;
            }
        } else if in_comment {
            in_comment = !line.contains("-->");
        } else if line.trim_end() == "---" {
            blocks.push(start..i);
            start = i + 1;
        } else if let Some((open, _)) = Fence::parse(line) {
            if longest_close[i + 1][usize::from(open.tilde)] >= open.len {
                fence = Some(open);
            }
        } else {
            in_comment = opens_comment(line) && last_comment_end.is_some_and(|end| end > i);
        }
    }
    blocks.push(start..lines.len());
    blocks
}

/// `source` with slides `first` and `first + 1` (counted as the parser counts
/// them, skipping blank ones) trading places. Everything else, front matter
/// and separators included, is left as written. `None` when there is no such
/// pair.
pub fn swap_adjacent_slides(source: &str, first: usize) -> Option<String> {
    let raw: Vec<&str> = source.split_inclusive('\n').collect();
    let lines: Vec<&str> = raw
        .iter()
        .map(|l| l.trim_end_matches(['\n', '\r']))
        .collect();
    let (_, body) = split_front_matter(&lines);
    let offset = lines.len() - body.len();
    let slides: Vec<std::ops::Range<usize>> = split_slides(body)
        .into_iter()
        .filter(|r| body[r.clone()].iter().any(|l| !l.trim().is_empty()))
        .map(|r| r.start + offset..r.end + offset)
        .collect();
    let (a, b) = (slides.get(first)?.clone(), slides.get(first + 1)?.clone());
    // Each moved slide must end its own line, wherever it lands.
    let text = |r: &std::ops::Range<usize>| {
        let mut t = raw[r.clone()].concat();
        if !t.ends_with('\n') {
            t.push('\n');
        }
        t
    };
    let mut out = raw[..a.start].concat();
    out.push_str(&text(&b));
    out.push_str(&raw[a.end..b.start].concat());
    out.push_str(&text(&a));
    out.push_str(&raw[b.end..].concat());
    if !source.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swapping_slides_moves_only_their_text() {
        let deck = "---\ntitle: T\n---\n# A\n- a\n\n---\n\n---\n# B\n---  \n# C";
        assert_eq!(
            swap_adjacent_slides(deck, 0).unwrap(),
            "---\ntitle: T\n---\n# B\n---\n\n---\n# A\n- a\n\n---  \n# C"
        );
        assert_eq!(
            swap_adjacent_slides(deck, 1).unwrap(),
            "---\ntitle: T\n---\n# A\n- a\n\n---\n\n---\n# C\n---  \n# B",
            "the last slide keeps the file's missing final newline"
        );
        assert!(swap_adjacent_slides(deck, 2).is_none());
        let crlf = "# A\r\n---\r\n# B\r\n";
        assert_eq!(
            swap_adjacent_slides(crlf, 0).unwrap(),
            "# B\r\n---\r\n# A\r\n"
        );
    }
}
