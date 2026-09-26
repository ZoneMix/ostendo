//! Moving between slides and scrolling within one.

use std::time::Instant;

use crate::render::animation::{parse_transition, AnimationState};

use super::Presenter;

impl Presenter {
    /// Shows slide `idx` (clamped), resetting per-slide state and starting its animations.
    pub(crate) fn goto_slide(&mut self, idx: usize) {
        let idx = idx.min(self.slides.len() - 1);
        if idx == self.current {
            return;
        }
        if self.timer_start.is_none() {
            self.timer_start = Some(Instant::now());
        }
        let old = std::mem::take(&mut self.last_lines);
        self.current = idx;
        self.scroll = 0;
        self.notes_scroll = 0;
        self.exec = None;
        self.exec_output = None;
        self.exec_block = 0;
        self.images.reset_gif();
        self.loop_started = Instant::now();
        let slide = &self.slides[idx];
        self.fullscreen = slide.fullscreen.unwrap_or(self.fullscreen_default);
        let transition = slide
            .transition
            .or_else(|| parse_transition(&self.meta.transition));
        let entrance = slide.entrance_animation;
        self.animation = match (transition, entrance) {
            (Some(t), e) => {
                let mut anim = AnimationState::new_transition(t, old);
                // The entrance that follows reveals the new slide.
                anim.exit_only = e.is_some();
                Some(anim)
            }
            (None, Some(e)) => Some(AnimationState::new_entrance(e)),
            (None, None) => None,
        };
        self.apply_slide_theme();
        self.font.request(Some(idx));
    }

    pub(crate) fn next_slide(&mut self) {
        self.goto_slide(self.current + 1);
    }

    pub(crate) fn prev_slide(&mut self) {
        self.goto_slide(self.current.saturating_sub(1));
    }

    /// First slide of the next section.
    pub(crate) fn next_section(&mut self) {
        let section = &self.slides[self.current].section;
        if let Some(i) =
            (self.current + 1..self.slides.len()).find(|&i| self.slides[i].section != *section)
        {
            self.goto_slide(i);
        }
    }

    /// First slide of the current section, or of the previous one when already there.
    pub(crate) fn prev_section(&mut self) {
        let start_of = |mut i: usize| {
            while i > 0 && self.slides[i - 1].section == self.slides[i].section {
                i -= 1;
            }
            i
        };
        let start = start_of(self.current);
        let target = if start < self.current {
            start
        } else {
            start_of(start.saturating_sub(1))
        };
        self.goto_slide(target);
    }

    pub(crate) fn scroll_by(&mut self, delta: isize) {
        self.scroll = self
            .scroll
            .saturating_add_signed(delta)
            .min(self.max_scroll);
    }
}
