//! Animation system for slide transitions, entrance effects, and continuous loop animations.
//! Supports fade, slide-left, dissolve transitions; typewriter, fade-in, slide-down entrance
//! effects; and matrix, bounce, pulse, sparkle, spin loop animations with optional targeting
//! (FIGlet, images).
//!
//! # Architecture
//!
//! This module is consumed by the render engine (`src/render/engine/`). When the presenter
//! navigates to a new slide, the engine creates an `AnimationState` and calls the appropriate
//! `render_*_frame()` function on every tick until the animation completes (or indefinitely
//! for loop animations).
//!
//! # Three animation categories
//!
//! 1. **Transitions** play *between* two slides (old -> new). They blend two pre-rendered
//!    buffers over a short duration (300-600 ms). The `exit_only` flag lets a transition
//!    fade out without revealing the new content, deferring the reveal to an entrance animation.
//!
//! 2. **Entrance animations** play *on arrival* at a slide. They progressively reveal a single
//!    buffer (the new slide's content) over ~500 ms.
//!
//! 3. **Loop animations** run continuously while a slide is displayed. They modify the rendered
//!    buffer each frame to create ongoing visual effects. Loop animations never finish (`is_done`
//!    always returns false).
//!
//! # Key Rust concepts
//!
//! - **`Instant`**: A monotonic clock timestamp from `std::time`. Used to measure elapsed time
//!   since the animation started, which drives the `progress()` calculation.
//! - **`Vec<StyledLine>`**: The "virtual buffer" — a list of styled text lines that represent
//!   one full screen of terminal content. Every animation function takes buffer(s) as input
//!   and returns a new buffer with the animation effect applied.

use crossterm::style::Color;
use std::time::Instant;

use crate::render::text::{LineContentType, StyledLine, StyledSpan};
use crate::theme::colors::interpolate_color;

/// Available transition types that play when navigating between slides.
///
/// Transitions blend two buffers (old slide and new slide) over a short duration.
/// Set via the `<!-- transition: fade -->` directive in Markdown.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransitionType {
    /// Crossfade: old content fades to the background color, then new content fades in.
    Fade,
    /// Horizontal slide: old content slides left off-screen while new content enters from the right.
    SlideLeft,
    /// Per-character dissolve: cells jumble into random symbols and then resolve into new content.
    Dissolve,
}

/// Available entrance animations that play once when a slide first appears.
///
/// Entrance effects progressively reveal the new slide's content over ~500 ms.
/// Set via the `<!-- animation: typewriter -->` directive in Markdown.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EntranceAnimation {
    /// Characters appear one at a time from left to right, like a typewriter.
    Typewriter,
    /// All content fades in from the background color to full brightness.
    FadeIn,
    /// Lines are revealed top-to-bottom, one row at a time.
    SlideDown,
}

/// Available continuous loop animations that run while a slide is displayed.
///
/// Loop animations modify the rendered buffer on every frame. They never complete and
/// are replaced only when the user navigates to a different slide.
/// Set via the `<!-- loop_animation: matrix -->` directive in Markdown.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LoopAnimation {
    /// Green cascading characters (Matrix-style rain) that fill the background.
    Matrix,
    /// A bouncing ball (`●`) that moves across the screen in a triangle-wave pattern.
    Bounce,
    /// All content brightness oscillates via a sine wave (pulsing glow effect).
    Pulse,
    /// Random cells briefly become sparkle/star characters (`✦`, `★`, etc.) in bright colors.
    Sparkle,
    /// ASCII art characters cycle through the brightness ramp, creating a shimmering wave effect.
    Spin,
}

/// Wrapper enum that tags an animation with its category (transition, entrance, or loop).
///
/// The render engine stores this inside `AnimationState` to know which dispatch function
/// to call on each tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnimationKind {
    /// A transition blending old and new slide buffers.
    Transition(TransitionType),
    /// An entrance effect revealing the new slide buffer.
    Entrance(EntranceAnimation),
    /// A continuous loop effect modifying the current slide buffer.
    Loop(LoopAnimation),
}

/// State machine tracking an active animation's lifecycle.
///
/// # Lifecycle
///
/// 1. **Creation**: One of the `new_transition()`, `new_entrance()`, or `new_loop()` constructors
///    is called, which records the start time and stores the relevant buffer(s).
/// 2. **Ticking**: On every render tick, the engine calls `tick()` to increment the frame counter,
///    then calls `progress()` to get a 0.0-1.0 value that drives the animation.
/// 3. **Completion**: `is_done()` returns true when `progress() >= 1.0` (transitions and entrances).
///    Loop animations never complete — `is_done()` always returns false for them.
/// 4. **Disposal**: When `is_done()` returns true, the engine discards this state and either
///    chains the next animation (e.g., entrance after transition) or settles into static rendering.
pub struct AnimationState {
    /// Which kind of animation is running (transition, entrance, or loop).
    pub kind: AnimationKind,
    /// When the animation started, used to calculate elapsed time.
    pub started: Instant,
    /// Total duration in milliseconds. Set to `u64::MAX` for loop animations (effectively infinite).
    pub duration_ms: u64,
    /// Frame counter, incremented each render tick by `tick()`.
    pub frame: u64,
    /// The previous slide's rendered content (only used by transitions).
    pub old_buffer: Vec<StyledLine>,
    /// The current/new slide's rendered content.
    pub new_buffer: Vec<StyledLine>,
    /// When true, the transition only fades/dissolves the old content out
    /// (to background) without revealing new content.  The entrance animation
    /// that follows will handle revealing the new content.
    pub exit_only: bool,
}

impl AnimationState {
    /// Creates a new transition animation state.
    ///
    /// Duration varies by type: Dissolve = 600ms, Fade = 400ms, SlideLeft = 300ms.
    ///
    /// # Parameters
    /// - `kind`: Which transition effect to use.
    /// - `old_buffer`: The fully-rendered content of the slide being left.
    /// - `new_buffer`: The fully-rendered content of the slide being entered.
    pub fn new_transition(
        kind: TransitionType,
        old_buffer: Vec<StyledLine>,
        new_buffer: Vec<StyledLine>,
    ) -> Self {
        let duration_ms = match kind {
            TransitionType::Dissolve => 600,
            TransitionType::Fade => 400,
            _ => 300,
        };
        Self {
            kind: AnimationKind::Transition(kind),
            started: Instant::now(),
            duration_ms,
            frame: 0,
            old_buffer,
            new_buffer,
            exit_only: false,
        }
    }

    /// Creates a new entrance animation state with a fixed 500ms duration.
    ///
    /// # Parameters
    /// - `kind`: Which entrance effect to use.
    /// - `buffer`: The fully-rendered content of the new slide to reveal.
    pub fn new_entrance(kind: EntranceAnimation, buffer: Vec<StyledLine>) -> Self {
        Self {
            kind: AnimationKind::Entrance(kind),
            started: Instant::now(),
            duration_ms: 500,
            frame: 0,
            old_buffer: Vec::new(),
            new_buffer: buffer,
            exit_only: false,
        }
    }

    /// Creates a new loop animation state that runs indefinitely (`duration_ms = u64::MAX`).
    ///
    /// # Parameters
    /// - `kind`: Which loop effect to use.
    /// - `buffer`: The fully-rendered content of the current slide.
    pub fn new_loop(kind: LoopAnimation, buffer: Vec<StyledLine>) -> Self {
        Self {
            kind: AnimationKind::Loop(kind),
            started: Instant::now(),
            duration_ms: u64::MAX, // loops indefinitely
            frame: 0,
            old_buffer: Vec::new(),
            new_buffer: buffer,
            exit_only: false,
        }
    }

    /// Progress from 0.0 to 1.0 for finite animations.
    pub fn progress(&self) -> f64 {
        let elapsed = self.started.elapsed().as_millis() as f64;
        (elapsed / self.duration_ms as f64).min(1.0)
    }

    /// Whether the animation has completed (always false for loops).
    pub fn is_done(&self) -> bool {
        match self.kind {
            AnimationKind::Loop(_) => false,
            _ => self.progress() >= 1.0,
        }
    }

    /// Advance frame counter (called each render tick).
    pub fn tick(&mut self) {
        self.frame += 1;
    }
}

/// Parses a transition type from a directive string.
///
/// Called by the Markdown parser when it encounters `<!-- transition: <value> -->`.
/// Returns `None` if the string does not match any known transition name.
///
/// # Accepted values
/// - `"fade"` -> `TransitionType::Fade`
/// - `"slide"` -> `TransitionType::SlideLeft`
/// - `"dissolve"` -> `TransitionType::Dissolve`
pub fn parse_transition(s: &str) -> Option<TransitionType> {
    match s {
        "fade" => Some(TransitionType::Fade),
        "slide" => Some(TransitionType::SlideLeft),
        "dissolve" => Some(TransitionType::Dissolve),
        _ => None,
    }
}

/// Parses an entrance animation from a directive string.
///
/// Called by the Markdown parser when it encounters `<!-- animation: <value> -->`.
/// Returns `None` if the string does not match any known entrance animation name.
///
/// # Accepted values
/// - `"typewriter"` -> `EntranceAnimation::Typewriter`
/// - `"fade_in"` -> `EntranceAnimation::FadeIn`
/// - `"slide_down"` -> `EntranceAnimation::SlideDown`
pub fn parse_entrance(s: &str) -> Option<EntranceAnimation> {
    match s {
        "typewriter" => Some(EntranceAnimation::Typewriter),
        "fade_in" => Some(EntranceAnimation::FadeIn),
        "slide_down" => Some(EntranceAnimation::SlideDown),
        _ => None,
    }
}

/// Parses a loop animation from a directive string.
///
/// Called by the Markdown parser when it encounters `<!-- loop_animation: <value> -->`.
/// Returns `None` if the string does not match any known loop animation name.
///
/// # Accepted values
/// - `"matrix"` -> `LoopAnimation::Matrix`
/// - `"bounce"` -> `LoopAnimation::Bounce`
/// - `"pulse"` -> `LoopAnimation::Pulse`
/// - `"sparkle"` -> `LoopAnimation::Sparkle`
/// - `"spin"` -> `LoopAnimation::Spin`
pub fn parse_loop_animation(s: &str) -> Option<LoopAnimation> {
    match s {
        "matrix" => Some(LoopAnimation::Matrix),
        "bounce" => Some(LoopAnimation::Bounce),
        "pulse" => Some(LoopAnimation::Pulse),
        "sparkle" => Some(LoopAnimation::Sparkle),
        "spin" => Some(LoopAnimation::Spin),
        _ => None,
    }
}

/// Dispatch function: renders one frame of a transition animation, returning the blended buffer.
///
/// This is the main entry point for transition rendering. The render engine calls this on
/// every tick with the current `progress` (0.0 to 1.0) and it delegates to the appropriate
/// effect function (`render_fade`, `render_slide_left`, or `render_dissolve`).
///
/// When `exit_only` is true, the transition only fades/dissolves the old content out to the
/// background without revealing the new content — the entrance animation that follows will
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
/// Color interpolation is done per-span using `interpolate_color()`, which linearly blends
/// RGB components. This preserves the original text while smoothly transitioning its visibility.
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
            // Exit-only: fade old→bg over the full duration
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
            // Fade old toward bg (progress 0→0.5 maps to t 0→1)
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
            // Fade new from bg toward full color (progress 0.5→1 maps to t 0→1)
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
/// The shift amount is proportional to `progress * terminal_width`. At progress 0.0, the old
/// content is fully visible. At progress 1.0, the old content has completely slid off-screen
/// and the new content fills the view.
///
/// Uses character-based (not byte-based) slicing to avoid panics on multi-byte UTF-8 text
/// like emoji or CJK characters. When `exit_only`, old content slides left and is replaced
/// by blank space rather than new content.
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
/// 2. **Jumbling**: The cell displays a random symbol (box-drawing chars, shapes, punctuation)
///    that changes each frame, creating visual "noise".
/// 3. **Resolved**: The cell shows the final character from the new slide.
///
/// The timing of each cell's transition is determined by a deterministic hash of its (row, col)
/// position, so cells resolve in a pseudo-random spatial pattern rather than left-to-right.
/// When `exit_only`, cells resolve to spaces instead of new content.
fn render_dissolve(
    old: &[StyledLine],
    new: &[StyledLine],
    progress: f64,
    exit_only: bool,
) -> Vec<StyledLine> {
    let max_len = old.len().max(new.len());
    let jumble_chars: &[char] = &[
        '░', '▒', '▓', '█', '┃', '╋', '╳', '┫', '╬', '║', '╠',
        '◆', '◇', '○', '●', '□', '■', '△', '▲', '◌', '◍',
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
            // Deterministic hash per cell — each cell resolves at a different progress point
            let cell_hash = ((row as u64).wrapping_mul(7919).wrapping_add(col as u64 * 6271).wrapping_add(31)) % 1000;
            let resolve_at = cell_hash as f64 / 1000.0;

            if progress > resolve_at {
                // This cell has resolved — show the new character (or space if exit-only)
                if exit_only {
                    out.push(' ');
                } else {
                    out.push(*new_chars.get(col).unwrap_or(&' '));
                }
            } else if progress > resolve_at * 0.5 {
                // This cell is jumbling — show random character
                let jumble_idx = ((cell_hash + (progress * 1000.0) as u64) % jumble_chars.len() as u64) as usize;
                out.push(jumble_chars[jumble_idx]);
            } else {
                // This cell still shows old content
                out.push(*old_chars.get(col).unwrap_or(&' '));
            }
        }

        // Use new line's styling but with the jumbled text
        // This preserves colors from the new slide
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
/// swapping in jumbled or resolved characters. Each span's character count determines how
/// many characters from `new_text` it receives. Any leftover characters beyond the original
/// spans are appended as unstyled text.
fn rebuild_line_with_text(source: &StyledLine, new_text: &str, _max_cols: usize) -> StyledLine {
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
/// count is exhausted, remaining lines appear as empty. Partially-revealed lines show only
/// their first N characters.
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
/// its target color. At progress 0.0 all text is the background color (hidden); at progress 1.0
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
/// are replaced with empty lines. This creates a "curtain dropping" visual effect.
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

/// Dispatch function: renders one frame of a loop animation, returning the modified buffer.
///
/// Called by the render engine on every tick. Unlike transitions and entrances, loop animations
/// use the `frame` counter (incrementing each tick) rather than a 0.0-1.0 progress value,
/// because they run indefinitely.
///
/// # Parameters
/// - `buffer`: The current slide's fully-rendered content.
/// - `animation`: Which loop effect to apply.
/// - `frame`: Monotonically increasing frame counter (drives animation timing).
/// - `accent`: The theme's accent color (used by bounce ball, pulse, and sparkle).
/// - `bg`: The theme's background color (used by pulse for interpolation).
/// - `width`: Terminal width in columns (used by matrix and bounce for positioning).
/// - `height`: Terminal height in rows (used by matrix and bounce for screen coverage).
/// - `target`: Optional animation target filter. When `Some("figlet")` or `Some("image")`,
///   only lines matching that content type are animated; other lines pass through unchanged.
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
    match animation {
        LoopAnimation::Matrix => render_matrix(buffer, frame, width, height),
        LoopAnimation::Bounce => render_bounce(buffer, frame, accent, width, height),
        LoopAnimation::Pulse => render_pulse(buffer, frame, accent, bg),
        LoopAnimation::Sparkle => render_sparkle(buffer, frame, accent, target),
        LoopAnimation::Spin => render_spin(buffer, frame, target),
    }
}

/// Renders the Matrix rain loop animation: green cascading characters falling top-to-bottom.
///
/// # Visual effect
/// Columns of random alphanumeric characters "rain" down the screen at different speeds.
/// The leading character of each rain stream is bright green; trailing characters fade through
/// green, dim green, and dark green. Actual slide content is overlaid on top of the rain
/// effect — non-whitespace content cells are preserved, while whitespace cells (even within
/// content lines) are filled with rain characters. This creates the appearance of content
/// floating in a Matrix-style digital rain.
///
/// # Performance
/// Rain characters are batched into spans grouped by brightness level, reducing the number
/// of terminal style-change escape sequences emitted per frame.
///
/// # Algorithm
/// A `classify(col, row)` closure computes each cell's brightness (0=space, 1-4=dark to bright)
/// based on a deterministic formula involving frame number, column speed, and column offset.
/// Content rows are processed at character-level granularity using a flat `(char, span_index)`
/// map to interleave rain and content spans.
fn render_matrix(
    buffer: &[StyledLine],
    frame: u64,
    width: usize,
    height: usize,
) -> Vec<StyledLine> {
    let bright_green = Color::Rgb { r: 0, g: 255, b: 0 };
    let green = Color::Rgb { r: 0, g: 180, b: 0 };
    let dim_green = Color::Rgb { r: 0, g: 80, b: 0 };
    let dark_green = Color::Rgb { r: 0, g: 40, b: 0 };
    let matrix_chars: &[u8] = b"0123456789abcdef:.<>+-=*/#@$%&";

    // Classify each column into a brightness level (0=space, 1-4=bright to dark)
    // Returns (brightness, char) for a given col,row,frame
    let classify = |col: usize, row: usize| -> (u8, u8) {
        let stream_speed = (col as u64 % 5) + 1;
        let stream_offset = col as u64 * 37 + 13;
        let drop_pos = ((frame * stream_speed + stream_offset) / 3) % (height as u64 * 2);
        let dist = (row as u64).wrapping_sub(drop_pos) % (height as u64 * 2);
        let ch_idx = ((col as u64 + row as u64 + frame) % matrix_chars.len() as u64) as usize;
        let ch = matrix_chars[ch_idx];
        let brightness = if dist == 0 { 4 } else if dist < 3 { 3 } else if dist < 6 { 2 } else if dist < 10 { 1 } else { 0 };
        (brightness, ch)
    };

    // Helper: append rain characters from col_start..col_end as batched spans
    let append_rain = |line: &mut StyledLine, row: usize, col_start: usize, col_end: usize| {
        let mut batch = String::with_capacity(col_end - col_start);
        let mut cur_brightness: u8 = 255; // sentinel
        for col in col_start..col_end {
            let (b, ch) = classify(col, row);
            if b != cur_brightness && !batch.is_empty() {
                line.push(styled_rain_span(&batch, cur_brightness, bright_green, green, dim_green, dark_green));
                batch.clear();
            }
            cur_brightness = b;
            if b == 0 {
                batch.push(' ');
            } else {
                batch.push(ch as char);
            }
        }
        if !batch.is_empty() {
            line.push(styled_rain_span(&batch, cur_brightness, bright_green, green, dim_green, dark_green));
        }
    };

    let mut result = Vec::with_capacity(height);

    for row in 0..height {
        let has_content = buffer.get(row)
            .map(|l| l.spans.iter().any(|s| s.text.chars().any(|c| !c.is_whitespace())))
            .unwrap_or(false);

        if has_content {
            // Character-level: preserve non-whitespace content, fill whitespace with rain.
            // First, expand the line's spans into per-character entries so we know
            // which character position is whitespace vs content and can preserve styling.
            let source_line = &buffer[row];
            // Build a flat list: (char, span_index) for each character position
            let mut char_entries: Vec<(char, usize)> = Vec::new();
            for (span_idx, span) in source_line.spans.iter().enumerate() {
                for ch in span.text.chars() {
                    char_entries.push((ch, span_idx));
                }
            }
            let content_len = char_entries.len();

            let mut mixed_line = StyledLine::empty();
            let mut col = 0;

            while col < width {
                if col < content_len {
                    let (ch, _span_idx) = char_entries[col];
                    if ch.is_whitespace() {
                        // Whitespace cell in content row: accumulate consecutive
                        // whitespace cells and render them as rain.
                        let rain_start = col;
                        while col < content_len {
                            let (c, _) = char_entries[col];
                            if !c.is_whitespace() {
                                break;
                            }
                            col += 1;
                        }
                        // Also include any columns beyond content_len if we
                        // reached the end of content during this whitespace run
                        // (they'll be handled by the trailing rain below).
                        append_rain(&mut mixed_line, row, rain_start, col);
                    } else {
                        // Non-whitespace: accumulate a run of consecutive
                        // non-whitespace chars from the same span and emit
                        // them as a single styled span preserving original styling.
                        let run_span_idx = char_entries[col].1;
                        let run_start = col;
                        while col < content_len {
                            let (c, si) = char_entries[col];
                            if c.is_whitespace() || si != run_span_idx {
                                break;
                            }
                            col += 1;
                        }
                        let text: String = char_entries[run_start..col].iter().map(|(c, _)| *c).collect();
                        mixed_line.push(StyledSpan {
                            text,
                            ..source_line.spans[run_span_idx].clone()
                        });
                    }
                } else {
                    // Past the content line's length: fill remaining with rain
                    append_rain(&mut mixed_line, row, col, width);
                    col = width; // done
                }
            }

            result.push(mixed_line);
        } else {
            // Empty row: build full rain line with batched spans
            let mut rain_line = StyledLine::empty();
            append_rain(&mut rain_line, row, 0, width);
            result.push(rain_line);
        }
    }
    result
}

/// Creates a styled span for matrix rain characters at the given brightness level.
///
/// Brightness levels: 4 = bright green + bold (leading drop), 3 = green, 2 = dim green,
/// 1 = dark green, 0 = unstyled (space). This maps each level to the appropriate color
/// and weight to create the depth/distance illusion in the rain effect.
fn styled_rain_span(text: &str, brightness: u8, bright: Color, green: Color, dim: Color, dark: Color) -> StyledSpan {
    match brightness {
        4 => StyledSpan::new(text).with_fg(bright).bold(),
        3 => StyledSpan::new(text).with_fg(green),
        2 => StyledSpan::new(text).with_fg(dim).dim(),
        1 => StyledSpan::new(text).with_fg(dark).dim(),
        _ => StyledSpan::new(text),
    }
}

/// Renders the bounce loop animation: a filled circle (`●`) bouncing off the screen edges.
///
/// # Visual effect
/// A single ball character in the theme's accent color moves across the screen, bouncing off
/// all four edges. The ball is overlaid on top of the existing slide content — it replaces
/// one character at its current position while preserving all surrounding content and styling.
///
/// # Physics
/// The ball follows a triangle-wave pattern (not a sine wave), creating linear motion with
/// instant direction reversal at the edges. The X and Y axes use different periods so the
/// ball traces a Lissajous-like path rather than a simple diagonal.
fn render_bounce(
    buffer: &[StyledLine],
    frame: u64,
    accent: Color,
    width: usize,
    height: usize,
) -> Vec<StyledLine> {
    let ball = "●";
    // Simple bounce physics — triangle wave
    let period_x = (width.max(2) * 2) as u64;
    let period_y = (height.max(2) * 2) as u64;
    let x_pos = if period_x > 0 {
        let raw = frame % period_x;
        if raw < width as u64 { raw as usize } else { (period_x - raw) as usize }
    } else { 0 };
    let y_pos = if period_y > 0 {
        let raw = (frame * 2) % period_y;
        if raw < height as u64 { raw as usize } else { (period_y - raw) as usize }
    } else { 0 };

    let mut result: Vec<StyledLine> = buffer.to_vec();
    // Extend to fill screen if needed
    while result.len() < height {
        result.push(StyledLine::empty());
    }

    let target_row = y_pos.min(result.len().saturating_sub(1));
    let x_clamped = x_pos.min(width.saturating_sub(1));

    // Rebuild the target row with the ball inserted at x_pos
    let original = &result[target_row];
    let original_text = line_to_string(original);
    let original_chars: Vec<char> = original_text.chars().collect();

    let mut new_line = StyledLine::empty();
    // Part before ball
    if x_clamped > 0 {
        if x_clamped <= original_chars.len() {
            // Preserve original content before ball position
            let prefix: String = original_chars[..x_clamped].iter().collect();
            // Try to preserve styling from original spans
            let mut chars_emitted = 0;
            for span in &original.spans {
                let span_chars: Vec<char> = span.text.chars().collect();
                if chars_emitted >= x_clamped {
                    break;
                }
                let take = (x_clamped - chars_emitted).min(span_chars.len());
                let partial: String = span_chars[..take].iter().collect();
                new_line.push(StyledSpan { text: partial, ..span.clone() });
                chars_emitted += take;
            }
            // Pad if original was shorter
            if chars_emitted < x_clamped {
                new_line.push(StyledSpan::new(&" ".repeat(x_clamped - chars_emitted)));
            }
            let _ = prefix; // used via chars_emitted logic
        } else {
            // Original line is shorter than x_pos — pad with spaces, then ball
            let existing: String = original_chars.iter().collect();
            if !existing.is_empty() {
                for span in &original.spans {
                    new_line.push(span.clone());
                }
            }
            let pad = x_clamped - original_chars.len();
            if pad > 0 {
                new_line.push(StyledSpan::new(&" ".repeat(pad)));
            }
        }
    }

    // The ball itself
    new_line.push(StyledSpan::new(ball).with_fg(accent).bold());

    // Part after ball
    let after_pos = x_clamped + 1;
    if after_pos < original_chars.len() {
        let mut chars_emitted = 0;
        for span in &original.spans {
            let span_chars: Vec<char> = span.text.chars().collect();
            let span_end = chars_emitted + span_chars.len();
            if span_end <= after_pos {
                chars_emitted = span_end;
                continue;
            }
            let start_in_span = after_pos.saturating_sub(chars_emitted);
            let partial: String = span_chars[start_in_span..].iter().collect();
            if !partial.is_empty() {
                new_line.push(StyledSpan { text: partial, ..span.clone() });
            }
            chars_emitted = span_end;
        }
    }

    result[target_row] = new_line;
    result
}

/// Renders the pulse loop animation: all content brightness oscillates in a sine-wave pattern.
///
/// # Visual effect
/// Every character on screen smoothly pulsates between dim and bright. The brightness factor
/// oscillates between 0.3 and 1.0 using a sine wave based on the frame counter. All content
/// (including different colors) is affected uniformly — each span's foreground is interpolated
/// between the background color and its original color by the current brightness factor.
fn render_pulse(
    buffer: &[StyledLine],
    frame: u64,
    accent: Color,
    bg: Color,
) -> Vec<StyledLine> {
    // Sine wave oscillation: 0.3 to 1.0
    let t = 0.65 + 0.35 * (frame as f64 * 0.15).sin();
    let mut result = Vec::with_capacity(buffer.len());
    for line in buffer.iter() {
        let mut pulsed = StyledLine::empty();
        pulsed.is_scale_placeholder = line.is_scale_placeholder;
        pulsed.content_type = line.content_type;
        for span in &line.spans {
            let fg = span.fg.unwrap_or(accent);
            let pulsed_fg = interpolate_color(bg, fg, t);
            pulsed.push(StyledSpan {
                fg: Some(pulsed_fg),
                ..span.clone()
            });
        }
        result.push(pulsed);
    }
    result
}

/// Renders the sparkle loop animation: random cells twinkle as star/sparkle characters.
///
/// # Visual effect
/// Non-whitespace cells periodically flash as sparkle characters (`✦`, `✧`, `★`, `☆`, etc.)
/// in bright white, yellow, cyan, or the theme's accent color. Each cell has its own
/// deterministic "sparkle period" (40-89 frames) based on a hash of its position, so different
/// cells sparkle at different times, creating a natural twinkling starfield appearance.
/// A cell sparkles for about 3 frames, then returns to its original content.
///
/// # Targeting
/// The `target` parameter can limit the effect to specific content types:
/// - `Some("figlet")` — only animate FIGlet ASCII title lines.
/// - `Some("image")` — only animate ASCII art image lines.
/// - `None` — animate all non-whitespace content.
fn render_sparkle(
    buffer: &[StyledLine],
    frame: u64,
    accent: Color,
    target: Option<&str>,
) -> Vec<StyledLine> {
    let sparkle_chars: &[char] = &['✦', '✧', '★', '☆', '✫', '✬', '·', '⁺', '✹', '✵'];
    let bright_white = Color::Rgb { r: 255, g: 255, b: 255 };
    let bright_yellow = Color::Rgb { r: 255, g: 255, b: 100 };
    let bright_cyan = Color::Rgb { r: 100, g: 255, b: 255 };

    let mut result = Vec::with_capacity(buffer.len());

    for (row, line) in buffer.iter().enumerate() {
        let should_animate = match target {
            None => true,
            Some("figlet") => line.content_type == LineContentType::FigletTitle,
            Some("image") => line.content_type == LineContentType::AsciiImage,
            _ => true,
        };
        if !should_animate {
            result.push(line.clone());
            continue;
        }
        let chars: Vec<char> = line.spans.iter().flat_map(|s| s.text.chars()).collect();
        if chars.is_empty() || chars.iter().all(|c| c.is_whitespace()) {
            result.push(line.clone());
            continue;
        }

        // Determine which cells sparkle this frame
        let mut sparkle_map: Vec<Option<(char, Color)>> = vec![None; chars.len()];
        for col in 0..chars.len() {
            if chars[col].is_whitespace() { continue; }
            // Each cell has a "sparkle phase" — it sparkles for 2-3 frames then goes dark
            let cell_hash = (row as u64).wrapping_mul(7919).wrapping_add(col as u64 * 6271).wrapping_add(31);
            let sparkle_period = 40 + (cell_hash % 50); // 40-89 frames between sparkles
            let phase = (frame.wrapping_add(cell_hash)) % sparkle_period;
            if phase < 3 {
                // This cell is sparkling
                let ch_idx = ((cell_hash + frame) % sparkle_chars.len() as u64) as usize;
                let color_pick = (cell_hash + frame / 3) % 4;
                let color = match color_pick {
                    0 => bright_white,
                    1 => bright_yellow,
                    2 => bright_cyan,
                    _ => accent,
                };
                sparkle_map[col] = Some((sparkle_chars[ch_idx], color));
            }
        }

        // Check if any cell sparkles on this line
        if sparkle_map.iter().all(|s| s.is_none()) {
            result.push(line.clone());
            continue;
        }

        // Rebuild line with sparkles injected
        let mut new_line = StyledLine::empty();
        new_line.content_type = line.content_type;
        let mut char_pos = 0;
        for span in &line.spans {
            let span_chars: Vec<char> = span.text.chars().collect();
            // Split span into runs of sparkle vs non-sparkle
            let mut run_start = 0;
            while run_start < span_chars.len() {
                let global_pos = char_pos + run_start;
                if global_pos < sparkle_map.len() {
                    if let Some((sch, scolor)) = sparkle_map[global_pos] {
                        // Sparkle cell
                        new_line.push(StyledSpan::new(&sch.to_string()).with_fg(scolor).bold());
                        run_start += 1;
                        continue;
                    }
                }
                // Find run of non-sparkle chars
                let run_end = (run_start + 1..span_chars.len())
                    .find(|&i| {
                        let gp = char_pos + i;
                        gp < sparkle_map.len() && sparkle_map[gp].is_some()
                    })
                    .unwrap_or(span_chars.len());
                let chunk: String = span_chars[run_start..run_end].iter().collect();
                new_line.push(StyledSpan { text: chunk, ..span.clone() });
                run_start = run_end;
            }
            char_pos += span_chars.len();
        }
        result.push(new_line);
    }
    result
}

/// Renders the spin loop animation: ASCII art characters cycle through the brightness ramp.
///
/// # Visual effect
/// Each ASCII art character is shifted along a brightness ramp (from space to `$`) by a
/// sine-wave offset that varies by position and frame. This creates a shimmering, morphing
/// wave that flows across the image. Characters near the top of the ramp (dark) shift toward
/// lighter characters and vice versa, producing a liquid/metallic surface appearance.
///
/// # Algorithm
/// 1. A 128-element lookup table maps ASCII byte values to their position in the ramp for O(1) access.
/// 2. For each non-whitespace ASCII character, the ramp position is found and shifted by
///    `±4` positions based on `sin(frame * speed + cell_phase)`.
/// 3. The shifted position is clamped to [1, ramp_len-1] to avoid producing spaces (index 0).
///
/// # Targeting
/// Like sparkle, the `target` parameter can limit the effect to `"figlet"` or `"image"` lines.
fn render_spin(
    buffer: &[StyledLine],
    frame: u64,
    target: Option<&str>,
) -> Vec<StyledLine> {
    // Same ramp as ascii_art.rs for consistency
    const ASCII_RAMP: &[u8] = b" .'`^\",:;Il!i><~+_-?][}{1)(|/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$";

    // Build O(1) lookup table: byte value -> ramp index (255 = not in ramp)
    let mut ramp_lookup = [255u8; 128];
    for (i, &b) in ASCII_RAMP.iter().enumerate() {
        if (b as usize) < 128 {
            ramp_lookup[b as usize] = i as u8;
        }
    }

    let mut result = Vec::with_capacity(buffer.len());

    for (row, line) in buffer.iter().enumerate() {
        let should_animate = match target {
            None => true,
            Some("figlet") => line.content_type == LineContentType::FigletTitle,
            Some("image") => line.content_type == LineContentType::AsciiImage,
            _ => true,
        };
        if !should_animate {
            result.push(line.clone());
            continue;
        }
        let chars: Vec<char> = line.spans.iter().flat_map(|s| s.text.chars()).collect();
        if chars.is_empty() || chars.iter().all(|c| c.is_whitespace()) {
            result.push(line.clone());
            continue;
        }

        // Check if this line has ASCII art characters (colored, non-whitespace cells)
        let has_art = line.spans.iter().any(|s| {
            !s.text.trim().is_empty() && s.fg.is_some() && s.text.chars().any(|c| !c.is_whitespace())
        });
        if !has_art {
            result.push(line.clone());
            continue;
        }

        // Rebuild with shifted ASCII ramp characters
        let mut new_line = StyledLine::empty();
        new_line.content_type = line.content_type;
        let mut char_pos: usize = 0;
        for span in &line.spans {
            let span_chars: Vec<char> = span.text.chars().collect();
            let mut new_text = String::with_capacity(span_chars.len());
            for (i, &ch) in span_chars.iter().enumerate() {
                let global_col = char_pos + i;
                if ch.is_whitespace() || !ch.is_ascii() {
                    new_text.push(ch);
                    continue;
                }
                // Find current position in the ASCII ramp (O(1) lookup)
                let ramp_pos = if (ch as u32) < 128 {
                    let idx = ramp_lookup[ch as usize];
                    if idx < 255 { Some(idx as usize) } else { None }
                } else { None };
                if let Some(ramp_pos) = ramp_pos {
                    // Wave: each cell shifts by a sine-based offset depending on position and frame
                    let cell_phase = (row as f64 * 0.3 + global_col as f64 * 0.2).sin();
                    let wave = (frame as f64 * 0.12 + cell_phase * 3.0).sin();
                    let shift = (wave * 4.0) as i32; // ±4 positions in ramp
                    let new_pos = (ramp_pos as i32 + shift)
                        .clamp(1, ASCII_RAMP.len() as i32 - 1) as usize; // skip space at 0
                    new_text.push(ASCII_RAMP[new_pos] as char);
                } else {
                    new_text.push(ch);
                }
            }
            new_line.push(StyledSpan { text: new_text, ..span.clone() });
            char_pos += span_chars.len();
        }
        result.push(new_line);
    }
    result
}

// ── Helper functions ──

/// Concatenates all span texts in a `StyledLine` into a single plain `String`,
/// discarding all styling information. Useful for character-level manipulation.
fn line_to_string(line: &StyledLine) -> String {
    line.spans.iter().map(|s| s.text.as_str()).collect()
}

/// Counts the total number of Unicode characters (not bytes) across all spans in a line.
/// This is used by the typewriter animation to calculate how many characters to reveal.
fn line_char_count(line: &StyledLine) -> usize {
    line.spans.iter().map(|s| s.text.chars().count()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_transition() {
        assert_eq!(parse_transition("fade"), Some(TransitionType::Fade));
        assert_eq!(parse_transition("slide"), Some(TransitionType::SlideLeft));
        assert_eq!(parse_transition("dissolve"), Some(TransitionType::Dissolve));
        assert_eq!(parse_transition("unknown"), None);
    }

    #[test]
    fn test_parse_entrance() {
        assert_eq!(parse_entrance("typewriter"), Some(EntranceAnimation::Typewriter));
        assert_eq!(parse_entrance("fade_in"), Some(EntranceAnimation::FadeIn));
        assert_eq!(parse_entrance("slide_down"), Some(EntranceAnimation::SlideDown));
        assert_eq!(parse_entrance("unknown"), None);
    }

    #[test]
    fn test_parse_loop() {
        assert_eq!(parse_loop_animation("matrix"), Some(LoopAnimation::Matrix));
        assert_eq!(parse_loop_animation("bounce"), Some(LoopAnimation::Bounce));
        assert_eq!(parse_loop_animation("pulse"), Some(LoopAnimation::Pulse));
        assert_eq!(parse_loop_animation("unknown"), None);
    }

    #[test]
    fn test_animation_state_progress() {
        let state = AnimationState::new_transition(
            TransitionType::Fade,
            vec![StyledLine::plain("old")],
            vec![StyledLine::plain("new")],
        );
        // Just created — progress should be near 0
        assert!(state.progress() < 0.5);
        assert!(!state.is_done());
    }

    #[test]
    fn test_loop_never_done() {
        let state = AnimationState::new_loop(
            LoopAnimation::Pulse,
            vec![StyledLine::plain("test")],
        );
        assert!(!state.is_done());
    }

    #[test]
    fn test_fade_transition() {
        let bg = Color::Rgb { r: 0, g: 0, b: 0 };
        let old = vec![StyledLine::plain("old content")];
        let new = vec![StyledLine::plain("new content")];
        let result = render_transition_frame(&old, &new, 0.0, TransitionType::Fade, bg, 80, false);
        assert_eq!(result.len(), 1);
        let result_end = render_transition_frame(&old, &new, 1.0, TransitionType::Fade, bg, 80, false);
        assert_eq!(result_end.len(), 1);
    }

    #[test]
    fn test_dissolve_transition() {
        let bg = Color::Rgb { r: 0, g: 0, b: 0 };
        let old = vec![StyledLine::plain("AAAA")];
        let new = vec![StyledLine::plain("BBBB")];
        // At progress 0, all old
        let result_0 = render_transition_frame(&old, &new, 0.0, TransitionType::Dissolve, bg, 80, false);
        let text_0 = line_to_string(&result_0[0]);
        assert!(text_0.contains('A'));
        // At progress 1, all new
        let result_1 = render_transition_frame(&old, &new, 1.0, TransitionType::Dissolve, bg, 80, false);
        let text_1 = line_to_string(&result_1[0]);
        assert!(text_1.contains('B'));
    }

    #[test]
    fn test_typewriter_entrance() {
        let buffer = vec![StyledLine::plain("Hello World")];
        let bg = Color::Rgb { r: 0, g: 0, b: 0 };
        let half = render_entrance_frame(&buffer, 0.5, EntranceAnimation::Typewriter, bg);
        let text = line_to_string(&half[0]);
        assert!(text.len() < "Hello World".len());
    }
}
