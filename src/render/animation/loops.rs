//! Loop animations that run while a slide is shown.

use crossterm::style::Color;

use crate::render::text::{LineContentType, StyledLine, StyledSpan};

use super::{cells, from_cells, LoopAnimation};

/// One frame of `animation`. `target` limits it to `figlet` or `image` lines.
#[allow(clippy::too_many_arguments)]
pub fn render_loop_frame(
    buffer: &[StyledLine],
    animation: LoopAnimation,
    frame: u64,
    accent: Color,
    bg: Color,
    width: usize,
    height: usize,
    target: Option<&str>,
) -> Vec<StyledLine> {
    let targeted = |line: &StyledLine| match target {
        Some("figlet") => line.content_type == LineContentType::FigletTitle,
        Some("image") => {
            line.content_type == LineContentType::AsciiImage
                || line.spans.iter().any(|s| s.animatable)
        }
        _ => true,
    };
    match animation {
        LoopAnimation::Matrix => matrix(buffer, frame, width, height),
        LoopAnimation::Bounce => bounce(buffer, frame, accent, width, height),
        LoopAnimation::Pulse => {
            let amount = 0.65 + 0.35 * (frame as f64 * 0.15).sin();
            buffer
                .iter()
                .map(|l| {
                    if targeted(l) {
                        super::transitions::tint(l, bg, amount)
                    } else {
                        l.clone()
                    }
                })
                .collect()
        }
        LoopAnimation::Sparkle => buffer
            .iter()
            .enumerate()
            .map(|(row, l)| {
                if targeted(l) {
                    sparkle(l, row, frame, accent)
                } else {
                    l.clone()
                }
            })
            .collect(),
        LoopAnimation::Spin => buffer
            .iter()
            .enumerate()
            .map(|(row, l)| {
                if targeted(l) {
                    spin(l, row, frame, target == Some("image"))
                } else {
                    l.clone()
                }
            })
            .collect(),
    }
}

fn cell_hash(row: usize, col: usize) -> u64 {
    (row as u64)
        .wrapping_mul(7919)
        .wrapping_add(col as u64 * 6271)
        .wrapping_add(31)
}

/// Green rain falling behind the content: blank cells show rain, content stays.
fn matrix(buffer: &[StyledLine], frame: u64, width: usize, height: usize) -> Vec<StyledLine> {
    const GLYPHS: &[u8] = b"0123456789abcdef:.<>+-=*/#@$%&";
    let shades = [
        StyledSpan::new("").with_fg(Color::Rgb { r: 0, g: 40, b: 0 }),
        StyledSpan::new("").with_fg(Color::Rgb { r: 0, g: 90, b: 0 }),
        StyledSpan::new("").with_fg(Color::Rgb { r: 0, g: 180, b: 0 }),
        StyledSpan::new("")
            .with_fg(Color::Rgb {
                r: 180,
                g: 255,
                b: 180,
            })
            .bold(),
    ];
    let space = StyledSpan::new(" ");
    let cycle = (height.max(1) * 2) as u64;
    let rain = |row: usize, col: usize| -> Option<(char, usize)> {
        let speed = col as u64 % 5 + 1;
        let head = ((frame * speed + col as u64 * 37 + 13) / 3) % cycle;
        let dist = (row as u64 + cycle - head) % cycle;
        let shade = match dist {
            0 => 3,
            1..=2 => 2,
            3..=5 => 1,
            6..=9 => 0,
            _ => return None,
        };
        let glyph = GLYPHS[((col as u64 + row as u64 + frame) % GLYPHS.len() as u64) as usize];
        Some((glyph as char, shade))
    };
    let empty = StyledLine::empty();
    (0..buffer.len().max(height))
        .map(|row| {
            let line = buffer.get(row).unwrap_or(&empty);
            let content = cells(line);
            // Rain stays clear of the text on this row, with a one-cell margin.
            let inked: Vec<usize> = (0..content.len())
                .filter(|&c| !content[c].0.is_whitespace())
                .collect();
            let clear = match (inked.first(), inked.last()) {
                (Some(&a), Some(&b)) => a.saturating_sub(1)..b + 2,
                _ => 0..0,
            };
            let out: Vec<(char, &StyledSpan)> = (0..width.max(content.len()))
                .map(|col| match content.get(col) {
                    Some(&cell) if clear.contains(&col) => cell,
                    _ => rain(row, col).map_or((' ', &space), |(g, shade)| (g, &shades[shade])),
                })
                .collect();
            from_cells(out, line)
        })
        .collect()
}

/// A ball bouncing around the content area.
fn bounce(
    buffer: &[StyledLine],
    frame: u64,
    accent: Color,
    width: usize,
    height: usize,
) -> Vec<StyledLine> {
    let tri = |t: u64, span: usize| {
        let span = span.max(2) as u64 - 1;
        let p = t % (span * 2);
        (if p < span { p } else { span * 2 - p }) as usize
    };
    let (x, y) = (tri(frame, width), tri(frame * 2 / 3, height));
    let ball = StyledSpan::new("●").with_fg(accent).bold();
    let space = StyledSpan::new(" ");
    let mut out = buffer.to_vec();
    out.resize(out.len().max(height), StyledLine::empty());
    let mut row = cells(&out[y]);
    if row.len() <= x {
        row.resize(x + 1, (' ', &space));
    }
    row[x] = ('●', &ball);
    out[y] = from_cells(row, &buffer.get(y).cloned().unwrap_or_default());
    out
}

fn sparkle(line: &StyledLine, row: usize, frame: u64, accent: Color) -> StyledLine {
    const STARS: &[char] = &['✦', '✧', '★', '☆', '✫', '✬', '·', '⁺', '✹', '✵'];
    let colors = [
        StyledSpan::new("")
            .with_fg(Color::Rgb {
                r: 255,
                g: 255,
                b: 255,
            })
            .bold(),
        StyledSpan::new("")
            .with_fg(Color::Rgb {
                r: 255,
                g: 255,
                b: 100,
            })
            .bold(),
        StyledSpan::new("")
            .with_fg(Color::Rgb {
                r: 100,
                g: 255,
                b: 255,
            })
            .bold(),
        StyledSpan::new("").with_fg(accent).bold(),
    ];
    let out: Vec<(char, &StyledSpan)> = cells(line)
        .into_iter()
        .enumerate()
        .map(|(col, (c, s))| {
            let h = cell_hash(row, col);
            if c.is_whitespace() || frame.wrapping_add(h) % (40 + h % 50) >= 3 {
                return (c, s);
            }
            let star = STARS[((h + frame) % STARS.len() as u64) as usize];
            (star, &colors[((h + frame / 3) % 4) as usize])
        })
        .collect();
    from_cells(out, line)
}

/// Shifts ASCII-art characters along a brightness ramp in a moving wave.
fn spin(line: &StyledLine, row: usize, frame: u64, only_animatable: bool) -> StyledLine {
    const RAMP: &[u8] = b" .'`^\",:;Il!i><~+_-?][}{1)(|/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$";
    let out: Vec<(char, &StyledSpan)> = cells(line)
        .into_iter()
        .enumerate()
        .map(|(col, (c, s))| {
            let pos = (!only_animatable || s.animatable)
                .then(|| RAMP.iter().position(|&b| char::from(b) == c))
                .flatten();
            let Some(pos) = pos.filter(|_| !c.is_whitespace()) else {
                return (c, s);
            };
            let phase = (row as f64 * 0.3 + col as f64 * 0.2).sin();
            let shift = ((frame as f64 * 0.12 + phase * 3.0).sin() * 4.0) as i64;
            let next = (pos as i64 + shift).clamp(1, RAMP.len() as i64 - 1) as usize;
            (char::from(RAMP[next]), s)
        })
        .collect();
    from_cells(out, line)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACCENT: Color = Color::Cyan;
    const BG: Color = Color::Black;

    fn text(line: &StyledLine) -> String {
        line.spans.iter().map(|s| s.text.as_str()).collect()
    }

    fn tagged(text: &str, content_type: LineContentType) -> StyledLine {
        StyledLine {
            spans: vec![StyledSpan::new(text).with_fg(ACCENT)],
            content_type,
        }
    }

    #[test]
    fn every_loop_keeps_content_rows_and_their_tags() {
        let buffer: Vec<StyledLine> = (0..30)
            .map(|i| {
                tagged(
                    &format!("line {i}"),
                    if i == 0 {
                        LineContentType::FigletTitle
                    } else {
                        LineContentType::Text
                    },
                )
            })
            .collect();
        for animation in [
            LoopAnimation::Matrix,
            LoopAnimation::Bounce,
            LoopAnimation::Pulse,
            LoopAnimation::Sparkle,
            LoopAnimation::Spin,
        ] {
            let out = render_loop_frame(&buffer, animation, 7, ACCENT, BG, 40, 10, None);
            assert!(out.len() >= buffer.len(), "{animation:?} dropped rows");
            assert_eq!(
                out[0].content_type,
                LineContentType::FigletTitle,
                "{animation:?}"
            );
            if animation == LoopAnimation::Matrix {
                assert!(text(&out[29]).contains("line 29"), "matrix hid content");
            }
        }
    }

    #[test]
    fn targeted_loops_leave_other_lines_alone() {
        let buffer = vec![
            tagged("|||||||||||||||", LineContentType::FigletTitle),
            tagged("|||||||||||||||", LineContentType::Text),
        ];
        let changed = (0..200).any(|f| {
            let out = render_loop_frame(
                &buffer,
                LoopAnimation::Spin,
                f,
                ACCENT,
                BG,
                40,
                2,
                Some("figlet"),
            );
            assert_eq!(out[1], buffer[1]);
            out[0] != buffer[0]
        });
        assert!(changed, "the targeted line never animated");
    }
}
