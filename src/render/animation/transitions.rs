//! Transition animations that play between two slides.
//!
//! Each transition blends an old buffer (the slide being left) with a new buffer (the slide
//! being entered) over a short duration.

use crossterm::style::Color;

use crate::render::text::{StyledLine, StyledSpan};
use crate::theme::colors::interpolate_color;

use super::{TransitionType, line_to_string};

/// Dispatch function: renders one frame of a transition animation, returning the blended buffer.
///
/// This is the main entry point for transition rendering. The render engine calls this on
/// every tick with the current `progress` (0.0 to 1.0) and it delegates to the appropriate
/// effect function (`render_fade`, `render_slide_left`, or `render_dissolve`).
///
/// When `exit_only` is true, the transition only fades/dissolves the old content out to the
/// background without revealing the new content -- the entrance animation that follows will
/// handle the reveal. This creates a two-phase effect: exit transition -> entrance animation.
///
/// # Parameters
/// - `old`: The previous slide's rendered buffer.
/// - `new`: The new slide's rendered buffer.
/// - `progress`: Animation progress from 0.0 (start) to 1.0 (complete).
/// - `transition`: Which transition effect to apply.
/// - `bg`: The theme's background color (used as the "fade to" target).
/// - `width`: Terminal width in columns (used by slide-left for shift calculation).
/// - `exit_only`: If true, only fade/dissolve the old content out without showing new content.
///
/// # Returns
/// A new buffer representing the blended frame to display.
pub fn render_transition_frame(
    old: &[StyledLine],
    new: &[StyledLine],
    progress: f64,
    transition: TransitionType,
    bg: Color,
    width: usize,
    exit_only: bool,
) -> Vec<StyledLine> {
    match transition {
        TransitionType::Fade => render_fade(old, new, progress, bg, exit_only),
        TransitionType::SlideLeft => render_slide_left(old, new, progress, width, exit_only),
        TransitionType::Dissolve => render_dissolve(old, new, progress, exit_only),
    }
}

/// Renders a crossfade transition between old and new slide content.
///
/// The effect works in two halves (unless `exit_only` is set):
/// - Progress 0.0-0.5: Old content's foreground colors are interpolated toward the background
///   color, making the old text "dissolve" into the background.
/// - Progress 0.5-1.0: New content's foreground colors are interpolated from the background
///   color toward their actual colors, making the new text "materialize".
///
/// When `exit_only` is true, the full progress range (0.0-1.0) fades old content to background
/// without ever showing new content.
fn render_fade(
    old: &[StyledLine],
    new: &[StyledLine],
    progress: f64,
    bg: Color,
    exit_only: bool,
) -> Vec<StyledLine> {
    let max_len = old.len().max(new.len());
    let mut result = Vec::with_capacity(max_len);

    for i in 0..max_len {
        if exit_only {
            // Exit-only: fade old->bg over the full duration
            let source = old.get(i).cloned().unwrap_or_else(StyledLine::empty);
            let mut faded = StyledLine::empty();
            for span in &source.spans {
                let fg = span.fg.unwrap_or(Color::White);
                let new_fg = interpolate_color(fg, bg, progress);
                faded.push(StyledSpan {
                    fg: Some(new_fg),
                    ..span.clone()
                });
            }
            result.push(faded);
        } else if progress < 0.5 {
            // Fade old toward bg (progress 0->0.5 maps to t 0->1)
            let t = progress * 2.0;
            let source = old.get(i).cloned().unwrap_or_else(StyledLine::empty);
            let mut faded = StyledLine::empty();
            for span in &source.spans {
                let fg = span.fg.unwrap_or(Color::White);
                let new_fg = interpolate_color(fg, bg, t);
                faded.push(StyledSpan {
                    fg: Some(new_fg),
                    ..span.clone()
                });
            }
            result.push(faded);
        } else {
            // Fade new from bg toward full color (progress 0.5->1 maps to t 0->1)
            let t = (progress - 0.5) * 2.0;
            let source = new.get(i).cloned().unwrap_or_else(StyledLine::empty);
            let mut faded = StyledLine::empty();
            for span in &source.spans {
                let fg = span.fg.unwrap_or(Color::White);
                let new_fg = interpolate_color(bg, fg, t);
                faded.push(StyledSpan {
                    fg: Some(new_fg),
                    ..span.clone()
                });
            }
            result.push(faded);
        }
    }
    result
}

/// Renders a horizontal sliding transition where old content exits left and new content enters
/// from the right.
///
/// The shift amount is proportional to `progress * terminal_width`. When `exit_only`, old
/// content slides left and is replaced by blank space rather than new content.
fn render_slide_left(
    old: &[StyledLine],
    new: &[StyledLine],
    progress: f64,
    width: usize,
    exit_only: bool,
) -> Vec<StyledLine> {
    let max_len = old.len().max(new.len());
    let shift = (width as f64 * progress) as usize;
    let mut result = Vec::with_capacity(max_len);

    for i in 0..max_len {
        let old_chars: Vec<char> = old.get(i).map(line_to_string).unwrap_or_default().chars().collect();

        // Shift old left
        let old_visible: String = old_chars.iter().skip(shift).collect();

        if exit_only {
            // Pad with spaces instead of bringing in new content
            let pad = " ".repeat(shift.min(width));
            let combined = format!("{}{}", old_visible, pad);
            result.push(StyledLine::plain(&combined));
        } else {
            let new_chars: Vec<char> = new.get(i).map(line_to_string).unwrap_or_default().chars().collect();
            let new_visible: String = new_chars.iter().take(shift).collect();
            let combined = format!("{}{}", old_visible, new_visible);
            result.push(StyledLine::plain(&combined));
        }
    }
    result
}

/// Renders a per-character dissolve transition with random symbol jumbling.
///
/// Each character cell on screen transitions through three phases at a different rate:
/// 1. **Old content**: The cell still shows its original character from the old slide.
/// 2. **Jumbling**: The cell displays a random symbol that changes each frame.
/// 3. **Resolved**: The cell shows the final character from the new slide.
///
/// When `exit_only`, cells resolve to spaces instead of new content.
fn render_dissolve(
    old: &[StyledLine],
    new: &[StyledLine],
    progress: f64,
    exit_only: bool,
) -> Vec<StyledLine> {
    let max_len = old.len().max(new.len());
    let jumble_chars: &[char] = &[
        '\u{2591}', '\u{2592}', '\u{2593}', '\u{2588}', '\u{2503}', '\u{254B}', '\u{2573}', '\u{252B}', '\u{256C}', '\u{2551}', '\u{2560}',
        '\u{25C6}', '\u{25C7}', '\u{25CB}', '\u{25CF}', '\u{25A1}', '\u{25A0}', '\u{25B3}', '\u{25B2}', '\u{25CC}', '\u{25CD}',
        '#', '@', '%', '&', '*', '~', '/', '\\', '|',
    ];
    let mut result = Vec::with_capacity(max_len);

    for row in 0..max_len {
        let new_line = new.get(row).cloned().unwrap_or_else(StyledLine::empty);
        let old_line = old.get(row).cloned().unwrap_or_else(StyledLine::empty);

        if progress >= 1.0 {
            if exit_only {
                result.push(StyledLine::empty());
            } else {
                result.push(new_line);
            }
            continue;
        }
        if progress <= 0.0 {
            result.push(old_line);
            continue;
        }

        let old_text = line_to_string(&old_line);
        let old_chars: Vec<char> = old_text.chars().collect();
        let (new_chars, new_text, max_cols) = if exit_only {
            let mc = old_chars.len();
            (Vec::new(), String::new(), mc)
        } else {
            let nt = line_to_string(&new_line);
            let nc: Vec<char> = nt.chars().collect();
            let mc = nc.len().max(old_chars.len());
            (nc, nt, mc)
        };
        let _ = &new_text; // suppress unused warning

        if max_cols == 0 {
            result.push(StyledLine::empty());
            continue;
        }

        // Build the dissolved line character by character
        let mut out = String::with_capacity(max_cols);
        for col in 0..max_cols {
            // Deterministic hash per cell
            let cell_hash = ((row as u64).wrapping_mul(7919).wrapping_add(col as u64 * 6271).wrapping_add(31)) % 1000;
            let resolve_at = cell_hash as f64 / 1000.0;

            if progress > resolve_at {
                if exit_only {
                    out.push(' ');
                } else {
                    out.push(*new_chars.get(col).unwrap_or(&' '));
                }
            } else if progress > resolve_at * 0.5 {
                let jumble_idx = ((cell_hash + (progress * 1000.0) as u64) % jumble_chars.len() as u64) as usize;
                out.push(jumble_chars[jumble_idx]);
            } else {
                out.push(*old_chars.get(col).unwrap_or(&' '));
            }
        }

        if exit_only {
            result.push(rebuild_line_with_text(&old_line, &out, max_cols));
        } else if progress > 0.5 {
            result.push(rebuild_line_with_text(&new_line, &out, max_cols));
        } else {
            result.push(rebuild_line_with_text(&old_line, &out, max_cols));
        }
    }
    result
}

/// Rebuilds a `StyledLine` by replacing its text characters while preserving each span's
/// formatting (foreground color, bold, italic, etc.).
///
/// This is used by the dissolve transition to keep the original slide's colors while
/// swapping in jumbled or resolved characters.
pub(super) fn rebuild_line_with_text(source: &StyledLine, new_text: &str, _max_cols: usize) -> StyledLine {
    let chars: Vec<char> = new_text.chars().collect();
    let mut line = StyledLine::empty();
    let mut char_pos = 0;

    if source.spans.is_empty() {
        return StyledLine::plain(new_text);
    }

    for span in &source.spans {
        let span_len = span.text.chars().count();
        let take = span_len.min(chars.len().saturating_sub(char_pos));
        if take == 0 {
            char_pos += span_len;
            continue;
        }
        let replacement: String = chars[char_pos..char_pos + take].iter().collect();
        line.push(StyledSpan {
            text: replacement,
            ..span.clone()
        });
        char_pos += take;
    }
    // Any remaining characters not covered by original spans
    if char_pos < chars.len() {
        let rest: String = chars[char_pos..].iter().collect();
        line.push(StyledSpan::new(&rest));
    }
    line
}
