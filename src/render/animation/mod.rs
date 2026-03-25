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
//! - **`Vec<StyledLine>`**: The "virtual buffer" -- a list of styled text lines that represent
//!   one full screen of terminal content. Every animation function takes buffer(s) as input
//!   and returns a new buffer with the animation effect applied.

mod transitions;
mod entrance;
mod loops;

use std::time::Instant;

use crate::render::text::StyledLine;

// Re-export dispatch functions so callers don't change.
pub use transitions::render_transition_frame;
pub use entrance::render_entrance_frame;
pub use loops::render_loop_frame;

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
    /// A bouncing ball that moves across the screen in a triangle-wave pattern.
    Bounce,
    /// All content brightness oscillates via a sine wave (pulsing glow effect).
    Pulse,
    /// Random cells briefly become sparkle/star characters in bright colors.
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
    /// Matched in rendering.rs dispatch; constructed only in tests since loops
    /// bypass AnimationState in the live engine (managed via active_loop vec).
    #[allow(dead_code)]
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
///    Loop animations never complete.
/// 4. **Disposal**: When `is_done()` returns true, the engine discards this state.
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
    /// When true, the transition only fades/dissolves the old content out
    /// (to background) without revealing new content.
    pub exit_only: bool,
}

impl AnimationState {
    /// Creates a new transition animation state.
    ///
    /// Duration varies by type: Dissolve = 600ms, Fade = 400ms, SlideLeft = 300ms.
    pub fn new_transition(
        kind: TransitionType,
        old_buffer: Vec<StyledLine>,
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
            exit_only: false,
        }
    }

    /// Creates a new entrance animation state with a fixed 500ms duration.
    pub fn new_entrance(kind: EntranceAnimation) -> Self {
        Self {
            kind: AnimationKind::Entrance(kind),
            started: Instant::now(),
            duration_ms: 500,
            frame: 0,
            old_buffer: Vec::new(),
            exit_only: false,
        }
    }

    /// Creates a new loop animation state that runs indefinitely (`duration_ms = u64::MAX`).
    /// Only used in tests -- loop animations are managed directly by the render engine.
    #[cfg(test)]
    pub fn new_loop(kind: LoopAnimation) -> Self {
        Self {
            kind: AnimationKind::Loop(kind),
            started: Instant::now(),
            duration_ms: u64::MAX,
            frame: 0,
            old_buffer: Vec::new(),
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

// ── Helper functions (shared by submodules) ──

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
    use crossterm::style::Color;

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
        );
        // Just created -- progress should be near 0
        assert!(state.progress() < 0.5);
        assert!(!state.is_done());
    }

    #[test]
    fn test_loop_never_done() {
        let state = AnimationState::new_loop(
            LoopAnimation::Pulse,
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
