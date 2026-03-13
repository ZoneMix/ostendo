//! Entrance animations that play once when a slide first appears.
//!
//! Each entrance effect progressively reveals a single buffer (the new slide's content)
//! over ~500 ms.

use crossterm::style::Color;

use crate::render::text::{StyledLine, StyledSpan};
use crate::theme::colors::interpolate_color;

use super::{EntranceAnimation, line_to_string, line_char_count};

/// Dispatch function: renders one frame of an entrance animation, returning the partially-revealed buffer.
///
/// Called by the render engine on every tick with `progress` (0.0 to 1.0). At progress 0.0
/// the slide is fully hidden; at 1.0 it is fully revealed. Delegates to the specific effect
/// function based on `animation`.
///
/// # Parameters
/// - `buffer`: The new slide's fully-rendered content.
/// - `progress`: Animation progress from 0.0 (hidden) to 1.0 (fully revealed).
/// - `animation`: Which entrance effect to apply.
/// - `bg`: The theme's background color (used by `FadeIn` for color interpolation).
pub fn render_entrance_frame(
    buffer: &[StyledLine],
    progress: f64,
    animation: EntranceAnimation,
    bg: Color,
) -> Vec<StyledLine> {
    match animation {
        EntranceAnimation::Typewriter => render_typewriter(buffer, progress),
        EntranceAnimation::FadeIn => render_fade_in(buffer, progress, bg),
        EntranceAnimation::SlideDown => render_slide_down(buffer, progress),
    }
}

/// Renders a typewriter entrance effect: characters appear one at a time from left to right.
///
/// The total character count across all lines is multiplied by `progress` to determine how
/// many characters should be visible. Lines are processed top-to-bottom; once the reveal
/// count is exhausted, remaining lines appear as empty.
fn render_typewriter(buffer: &[StyledLine], progress: f64) -> Vec<StyledLine> {
    let total_chars: usize = buffer.iter().map(line_char_count).sum();
    let reveal_count = (total_chars as f64 * progress) as usize;
    let mut chars_shown = 0;
    let mut result = Vec::with_capacity(buffer.len());

    for line in buffer {
        let line_len = line_char_count(line);
        if chars_shown >= reveal_count {
            result.push(StyledLine::empty());
        } else if chars_shown + line_len <= reveal_count {
            result.push(line.clone());
            chars_shown += line_len;
        } else {
            let remaining = reveal_count - chars_shown;
            let text = line_to_string(line);
            let visible: String = text.chars().take(remaining).collect();
            result.push(StyledLine::plain(&visible));
            chars_shown += line_len;
        }
    }
    result
}

/// Renders a fade-in entrance effect: all content gradually becomes visible.
///
/// Every span's foreground color is interpolated from the background color (invisible) toward
/// its target color. At progress 0.0 all text is the background color (hidden); at 1.0
/// all text shows its original color (fully visible).
fn render_fade_in(buffer: &[StyledLine], progress: f64, bg: Color) -> Vec<StyledLine> {
    let mut result = Vec::with_capacity(buffer.len());
    for line in buffer {
        let mut faded = StyledLine::empty();
        for span in &line.spans {
            let target_fg = span.fg.unwrap_or(Color::White);
            let current_fg = interpolate_color(bg, target_fg, progress);
            faded.push(StyledSpan {
                fg: Some(current_fg),
                ..span.clone()
            });
        }
        result.push(faded);
    }
    result
}

/// Renders a slide-down entrance effect: lines are revealed one row at a time from top to bottom.
///
/// The number of visible rows equals `total_lines * progress`. Lines below the reveal point
/// are replaced with empty lines.
fn render_slide_down(buffer: &[StyledLine], progress: f64) -> Vec<StyledLine> {
    let total = buffer.len();
    let reveal_rows = (total as f64 * progress) as usize;
    let mut result = Vec::with_capacity(total);
    for (i, line) in buffer.iter().enumerate() {
        if i < reveal_rows {
            result.push(line.clone());
        } else {
            result.push(StyledLine::empty());
        }
    }
    result
}
