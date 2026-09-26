//! Entrance effects that reveal a slide as it appears.

use crossterm::style::Color;

use crate::render::text::StyledLine;

use super::transitions::tint;
use super::{cells, from_cells, EntranceAnimation};

pub fn render_entrance_frame(
    buffer: &[StyledLine],
    progress: f64,
    animation: EntranceAnimation,
    bg: Color,
) -> Vec<StyledLine> {
    match animation {
        EntranceAnimation::FadeIn => buffer.iter().map(|l| tint(l, bg, progress)).collect(),
        EntranceAnimation::SlideDown => {
            let shown = (buffer.len() as f64 * progress).ceil() as usize;
            buffer
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    if i < shown {
                        l.clone()
                    } else {
                        StyledLine::empty()
                    }
                })
                .collect()
        }
        EntranceAnimation::Typewriter => {
            let total: usize = buffer.iter().map(|l| cells(l).len()).sum();
            let mut budget = (total as f64 * progress) as usize;
            buffer
                .iter()
                .map(|l| {
                    let c = cells(l);
                    let take = budget.min(c.len());
                    budget -= take;
                    from_cells(c.into_iter().take(take), l)
                })
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::text::StyledSpan;

    #[test]
    fn typewriter_reveals_in_reading_order_and_keeps_styles() {
        let buffer = vec![
            StyledLine {
                spans: vec![StyledSpan::new("abcd").bold()],
                ..Default::default()
            },
            StyledLine::plain("efgh"),
        ];
        let half =
            render_entrance_frame(&buffer, 0.25, EntranceAnimation::Typewriter, Color::Black);
        assert_eq!(half[0].spans[0].text, "ab");
        assert!(half[0].spans[0].bold);
        assert!(half[1].spans.is_empty());
    }
}
