//! Converts program output containing ANSI escapes into styled spans.
//!
//! SGR (colors and attributes) is honored; every other escape sequence and
//! control character is dropped so program output cannot move the cursor or
//! otherwise tamper with the presentation.

use crossterm::style::Color;

use crate::render::text::StyledSpan;

/// Parses one line of output. A carriage return discards what came before it,
/// so progress bars that redraw in place show their final state.
pub(crate) fn parse_ansi_line(line: &str) -> Vec<StyledSpan> {
    let mut spans: Vec<StyledSpan> = Vec::new();
    let mut style = StyledSpan::default();
    let mut text = String::new();
    let mut chars = line.chars().peekable();

    let flush = |spans: &mut Vec<StyledSpan>, text: &mut String, style: &StyledSpan| {
        if !text.is_empty() {
            spans.push(StyledSpan {
                text: std::mem::take(text),
                ..style.clone()
            });
        }
    };

    while let Some(c) = chars.next() {
        match c {
            '\x1b' => match chars.next() {
                Some('[') => {
                    let mut params = String::new();
                    let mut end = None;
                    for next in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&next) {
                            end = Some(next);
                            break;
                        }
                        params.push(next);
                    }
                    if end == Some('m') {
                        flush(&mut spans, &mut text, &style);
                        apply_sgr(&mut style, &params);
                    }
                }
                Some(']') => {
                    // OSC: skip to BEL or ST.
                    while let Some(next) = chars.next() {
                        if next == '\x07' || (next == '\x1b' && chars.next_if_eq(&'\\').is_some()) {
                            break;
                        }
                    }
                }
                _ => {}
            },
            '\r' => {
                spans.clear();
                text.clear();
            }
            '\t' => text.push_str("    "),
            c if c.is_control() => {}
            c => text.push(c),
        }
    }
    flush(&mut spans, &mut text, &style);
    spans
}

const NORMAL: [Color; 8] = [
    Color::Black,
    Color::DarkRed,
    Color::DarkGreen,
    Color::DarkYellow,
    Color::DarkBlue,
    Color::DarkMagenta,
    Color::DarkCyan,
    Color::Grey,
];
const BRIGHT: [Color; 8] = [
    Color::DarkGrey,
    Color::Red,
    Color::Green,
    Color::Yellow,
    Color::Blue,
    Color::Magenta,
    Color::Cyan,
    Color::White,
];

fn apply_sgr(style: &mut StyledSpan, params: &str) {
    let codes: Vec<u16> = params
        .split([';', ':'])
        .map(|p| p.parse().unwrap_or(0))
        .collect();
    let mut i = 0;
    while i < codes.len() {
        match codes[i] {
            0 => *style = StyledSpan::default(),
            1 => style.bold = true,
            2 => style.dim = true,
            3 => style.italic = true,
            4 => style.underline = true,
            9 => style.strikethrough = true,
            22 => (style.bold, style.dim) = (false, false),
            23 => style.italic = false,
            24 => style.underline = false,
            29 => style.strikethrough = false,
            c @ 30..=37 => style.fg = Some(NORMAL[usize::from(c - 30)]),
            c @ 90..=97 => style.fg = Some(BRIGHT[usize::from(c - 90)]),
            39 => style.fg = None,
            c @ 40..=47 => style.bg = Some(NORMAL[usize::from(c - 40)]),
            c @ 100..=107 => style.bg = Some(BRIGHT[usize::from(c - 100)]),
            49 => style.bg = None,
            c @ (38 | 48) => {
                let (color, used) = extended_color(&codes[i + 1..]);
                i += used;
                if c == 38 {
                    style.fg = color.or(style.fg);
                } else {
                    style.bg = color.or(style.bg);
                }
            }
            _ => {}
        }
        i += 1;
    }
}

/// Parses the arguments after 38/48; returns the color and how many were consumed.
fn extended_color(args: &[u16]) -> (Option<Color>, usize) {
    let byte = |i: usize| args.get(i).map(|&v| v.min(255) as u8);
    match args.first() {
        Some(5) => (byte(1).map(Color::AnsiValue), 2.min(args.len())),
        Some(2) => match (byte(1), byte(2), byte(3)) {
            (Some(r), Some(g), Some(b)) => (Some(Color::Rgb { r, g, b }), 4),
            _ => (None, args.len()),
        },
        _ => (None, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fg_of(input: &str) -> Vec<(String, Option<Color>)> {
        parse_ansi_line(input)
            .into_iter()
            .map(|s| (s.text, s.fg))
            .collect()
    }

    #[test]
    fn sgr_colors_map_to_the_right_palette_entries() {
        let rgb = |r, g, b| Some(Color::Rgb { r, g, b });
        let cases: &[(&str, Option<Color>)] = &[
            ("\x1b[31mx", Some(Color::DarkRed)),
            ("\x1b[91mx", Some(Color::Red)),
            ("\x1b[37mx", Some(Color::Grey)),
            ("\x1b[97mx", Some(Color::White)),
            ("\x1b[38;5;32mx", Some(Color::AnsiValue(32))),
            ("\x1b[38;2;0;200;1mx", rgb(0, 200, 1)),
            ("\x1b[1;38;2;10;20;30mx", rgb(10, 20, 30)),
            ("\x1b[31m\x1b[39mx", None),
            ("\x1b[31m\x1b[0mx", None),
        ];
        for (input, want) in cases {
            assert_eq!(fg_of(input), [("x".to_string(), *want)], "{input:?}");
        }
    }

    #[test]
    fn truecolor_components_do_not_toggle_attributes() {
        let span = &parse_ansi_line("\x1b[38;2;1;2;9mx")[0];
        assert!(!span.bold && !span.dim && !span.strikethrough);
    }

    #[test]
    fn non_sgr_escapes_and_controls_are_stripped() {
        let text: String = parse_ansi_line("a\x1b[2Jb\x1b]0;title\x07c\x08d\x1b[10;5He")
            .iter()
            .map(|s| s.text.as_str())
            .collect();
        assert_eq!(text, "abcde");
    }

    #[test]
    fn carriage_return_keeps_the_last_redraw() {
        let text: String = parse_ansi_line("10%\r50%\r100% done")
            .iter()
            .map(|s| s.text.as_str())
            .collect();
        assert_eq!(text, "100% done");
    }
}
