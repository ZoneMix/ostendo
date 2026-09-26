//! Slide transitions, entrance effects, and loop animations.
//!
//! Every effect is a pure function from buffer(s) to a new buffer, applied to
//! the visible content each frame: transitions blend the previous and next
//! slide, entrances reveal the next slide, and loops run while it is shown.

mod entrance;
mod loops;
mod transitions;

use std::time::Instant;

use crate::render::text::{StyledLine, StyledSpan};

pub use entrance::render_entrance_frame;
pub use loops::render_loop_frame;
pub use transitions::render_transition_frame;

/// `<!-- transition: fade|slide|dissolve -->`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionType {
    Fade,
    SlideLeft,
    Dissolve,
}

/// `<!-- animation: typewriter|fade_in|slide_down -->`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntranceAnimation {
    Typewriter,
    FadeIn,
    SlideDown,
}

/// `<!-- loop_animation: matrix|bounce|pulse|sparkle|spin -->`, optionally
/// targeted: `sparkle(figlet)`, `spin(image)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopAnimation {
    Matrix,
    Bounce,
    Pulse,
    Sparkle,
    Spin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationKind {
    Transition(TransitionType),
    Entrance(EntranceAnimation),
}

/// A running one-shot animation.
pub struct AnimationState {
    pub kind: AnimationKind,
    started: Instant,
    duration_ms: u64,
    /// The previous slide's visible lines (transitions only).
    pub old_buffer: Vec<StyledLine>,
    /// Only take the old slide away; an entrance animation reveals the new one.
    pub exit_only: bool,
}

impl AnimationState {
    pub fn new_transition(kind: TransitionType, old_buffer: Vec<StyledLine>) -> Self {
        let duration_ms = match kind {
            TransitionType::Dissolve => 600,
            TransitionType::Fade => 400,
            TransitionType::SlideLeft => 300,
        };
        Self {
            kind: AnimationKind::Transition(kind),
            started: Instant::now(),
            duration_ms,
            old_buffer,
            exit_only: false,
        }
    }

    pub fn new_entrance(kind: EntranceAnimation) -> Self {
        Self {
            kind: AnimationKind::Entrance(kind),
            started: Instant::now(),
            duration_ms: 500,
            old_buffer: Vec::new(),
            exit_only: false,
        }
    }

    /// 0.0 at the start, 1.0 when finished; eased so motion settles gently.
    pub fn progress(&self) -> f64 {
        let t = (self.started.elapsed().as_millis() as f64 / self.duration_ms as f64).min(1.0);
        1.0 - (1.0 - t).powi(3)
    }

    pub fn is_done(&self) -> bool {
        self.started.elapsed().as_millis() >= u128::from(self.duration_ms)
    }
}

pub fn parse_transition(s: &str) -> Option<TransitionType> {
    match s {
        "fade" => Some(TransitionType::Fade),
        "slide" => Some(TransitionType::SlideLeft),
        "dissolve" => Some(TransitionType::Dissolve),
        _ => None,
    }
}

pub fn parse_entrance(s: &str) -> Option<EntranceAnimation> {
    match s {
        "typewriter" => Some(EntranceAnimation::Typewriter),
        "fade_in" => Some(EntranceAnimation::FadeIn),
        "slide_down" => Some(EntranceAnimation::SlideDown),
        _ => None,
    }
}

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

/// A line as individual characters, each with its span's style.
fn cells(line: &StyledLine) -> Vec<(char, &StyledSpan)> {
    line.spans
        .iter()
        .flat_map(|s| s.text.chars().map(move |c| (c, s)))
        .collect()
}

/// Rebuilds a line from styled characters, merging runs of the same style.
fn from_cells<'a>(
    cells: impl IntoIterator<Item = (char, &'a StyledSpan)>,
    like: &StyledLine,
) -> StyledLine {
    let mut out = StyledLine {
        spans: Vec::new(),
        content_type: like.content_type,
    };
    let mut last: Option<*const StyledSpan> = None;
    for (c, style) in cells {
        match out.spans.last_mut() {
            Some(span) if last == Some(style as *const _) => span.text.push(c),
            _ => out.spans.push(StyledSpan {
                text: c.to_string(),
                ..style.clone()
            }),
        }
        last = Some(style as *const _);
    }
    out
}
