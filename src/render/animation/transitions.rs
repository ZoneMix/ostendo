//! Transitions between the previous and the next slide.

use crossterm::style::Color;

use crate::render::text::{StyledLine, StyledSpan};
use crate::theme::colors::interpolate_color;

use super::{cells, from_cells, TransitionType};

/// One frame of `transition` at `progress` (0.0–1.0). With `exit_only`, the
/// old slide is taken away without revealing the new one.
pub fn render_transition_frame(
    old: &[StyledLine],
    new: &[StyledLine],
    progress: f64,
    transition: TransitionType,
    bg: Color,
    width: usize,
    exit_only: bool,
) -> Vec<StyledLine> {
    let blank = StyledLine::empty();
    let rows = old.len().max(new.len());
    let pick = |lines: &[StyledLine], i: usize| lines.get(i).cloned().unwrap_or_default();
    match transition {
        TransitionType::Fade => {
            // First half fades the old slide out; second half fades the new one in.
            let (source, amount) = if exit_only {
                (old, 1.0 - progress)
            } else if progress < 0.5 {
                (old, 1.0 - progress * 2.0)
            } else {
                (new, progress * 2.0 - 1.0)
            };
            (0..rows)
                .map(|i| tint(&pick(source, i), bg, amount))
                .collect()
        }
        TransitionType::SlideLeft => {
            let shift = (width as f64 * progress) as usize;
            (0..rows)
                .map(|i| {
                    let o = old.get(i).unwrap_or(&blank);
                    let n = if exit_only {
                        &blank
                    } else {
                        new.get(i).unwrap_or(&blank)
                    };
                    let pad = StyledSpan::new(" ");
                    let old_cells = cells(o);
                    let mut row: Vec<(char, &StyledSpan)> =
                        old_cells.into_iter().skip(shift).collect();
                    row.resize(width.saturating_sub(shift), (' ', &pad));
                    row.extend(cells(n).into_iter().take(shift));
                    from_cells(row, if progress < 0.5 { o } else { n })
                })
                .collect()
        }
        TransitionType::Dissolve => (0..rows)
            .map(|i| {
                dissolve_row(
                    old.get(i).unwrap_or(&blank),
                    if exit_only {
                        &blank
                    } else {
                        new.get(i).unwrap_or(&blank)
                    },
                    i,
                    progress,
                )
            })
            .collect(),
    }
}

/// Blends every color in `line` toward `bg`; `amount` 1.0 keeps the original.
pub(super) fn tint(line: &StyledLine, bg: Color, amount: f64) -> StyledLine {
    StyledLine {
        spans: line
            .spans
            .iter()
            .map(|s| StyledSpan {
                fg: Some(interpolate_color(bg, s.fg.unwrap_or(Color::White), amount)),
                bg: s.bg.map(|b| interpolate_color(bg, b, amount)),
                ..s.clone()
            })
            .collect(),
        content_type: line.content_type,
    }
}

/// Each cell turns into noise at its own moment, then resolves to the new slide.
fn dissolve_row(old: &StyledLine, new: &StyledLine, row: usize, progress: f64) -> StyledLine {
    const NOISE: &[char] = &[
        '░', '▒', '▓', '╳', '◆', '◇', '●', '○', '■', '□', '#', '%', '&', '*',
    ];
    let noise = StyledSpan::default();
    let blank = StyledSpan::new(" ");
    let (o, n) = (cells(old), cells(new));
    let cols = o.len().max(n.len());
    let out: Vec<(char, &StyledSpan)> = (0..cols)
        .map(|col| {
            let hash = (row as u64)
                .wrapping_mul(7919)
                .wrapping_add(col as u64 * 6271)
                .wrapping_add(31)
                % 1000;
            let resolve_at = hash as f64 / 1000.0;
            if progress >= resolve_at {
                n.get(col).copied().unwrap_or((' ', &blank))
            } else if progress >= resolve_at * 0.5 {
                let glyph =
                    NOISE[((hash + (progress * 997.0) as u64) % NOISE.len() as u64) as usize];
                (glyph, o.get(col).or(n.get(col)).map_or(&noise, |c| c.1))
            } else {
                o.get(col).copied().unwrap_or((' ', &blank))
            }
        })
        .collect();
    from_cells(out, if progress > 0.5 { new } else { old })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(line: &StyledLine) -> String {
        line.spans.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn transitions_start_on_the_old_slide_and_end_on_the_new_one() {
        let old = vec![StyledLine::plain("AAAA")];
        let new = vec![StyledLine {
            spans: vec![StyledSpan::new("BBBB").bold()],
            ..Default::default()
        }];
        for t in [
            TransitionType::Fade,
            TransitionType::SlideLeft,
            TransitionType::Dissolve,
        ] {
            let start = render_transition_frame(&old, &new, 0.0, t, Color::Black, 4, false);
            let end = render_transition_frame(&old, &new, 1.0, t, Color::Black, 4, false);
            assert_eq!(text(&start[0]), "AAAA", "{t:?}");
            assert_eq!(text(&end[0]), "BBBB", "{t:?}");
            assert!(end[0].spans.iter().all(|s| s.bold), "{t:?} lost styling");
        }
    }
}
