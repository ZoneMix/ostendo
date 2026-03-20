use super::*;

impl Presenter {
    pub(crate) fn start_slide_animations(&mut self) {
        let slide = &self.slides[self.current];
        let old_buffer = self.last_rendered_buffer.clone();

        // Determine transition: per-slide directive overrides global meta
        let transition_type = slide.transition
            .or_else(|| if self.meta.transition.is_empty() { None } else { parse_transition(&self.meta.transition) });

        if let Some(tt) = transition_type {
            let has_entrance = slide.entrance_animation.is_some();
            let mut anim = AnimationState::new_transition(tt, old_buffer, Vec::new());
            // When an entrance animation follows, the transition only fades out
            // old content — the entrance handles revealing new content.
            anim.exit_only = has_entrance;
            self.active_animation = Some(anim);
        } else if let Some(ea) = slide.entrance_animation {
            self.active_animation = Some(AnimationState::new_entrance(ea, Vec::new()));
        }

        // Set up loop animation (runs independently after transition/entrance complete)
        self.active_loop = slide.loop_animations.iter().map(|(la, _)| (*la, 0)).collect();

        self.needs_full_redraw = true;
    }

    /// Reset transient state after changing slides.
    pub(crate) fn on_slide_changed(&mut self) {
        self.scroll_offset = 0;
        self.notes_scroll = 0;
        self.exec_output = None;
        self.exec_rx = None;
        self.exec_block_index = 0;

        // Kitty image cleanup is handled in render_frame() before emitting
        // new placements — NOT here, because the font fade-out animation needs
        // the old image to remain visible until the fade completes.

        // Reset GIF animation to first frame on slide change
        self.gif_current_frame = 0;
        self.gif_last_advance = std::time::Instant::now();
        // Font transition animation: parse per-slide directive
        self.font_change_is_slide_transition = match self.slides[self.current].font_transition.as_deref() {
            Some("none") => FontTransitionMode::None,
            Some("fade") => FontTransitionMode::Fade,
            Some("dissolve") => FontTransitionMode::Dissolve,
            _ => FontTransitionMode::Fade, // Default: smooth fade (not scatter dissolve)
        };
        // Apply per-slide theme override (or restore base theme)
        self.apply_slide_theme();
        // Apply per-slide fullscreen directive. User toggle (f key) is sticky
        // until the next slide change, then directives take control again.
        self.user_fullscreen_override = None;
        if let Some(fs) = self.slides[self.current].fullscreen {
            self.show_fullscreen = fs;
            self.needs_full_redraw = true;
        } else {
            // No directive: revert to non-fullscreen (default)
            if self.show_fullscreen {
                self.show_fullscreen = false;
                self.needs_full_redraw = true;
            }
        }
        self.apply_slide_font();
        self.start_slide_animations();
    }

    /// Apply per-slide theme override or restore the base theme.
    pub(crate) fn apply_slide_theme(&mut self) {
        let desired_slug = self.slides[self.current]
            .theme_override
            .as_deref()
            .unwrap_or(&self.base_theme.slug);
        if self.theme.slug != desired_slug {
            if desired_slug == self.base_theme.slug {
                self.apply_theme(self.base_theme.clone());
            } else {
                let registry = crate::theme::ThemeRegistry::load();
                if let Some(override_theme) = registry.get(desired_slug) {
                    self.apply_theme(override_theme);
                }
            }
        }
    }

    pub(crate) fn next_slide(&mut self) {
        if self.timer_start.is_none() {
            self.start_timer();
        }
        if self.current < self.slides.len() - 1 {
            self.current += 1;
            self.on_slide_changed();
        }
    }

    pub(crate) fn prev_slide(&mut self) {
        if self.current > 0 {
            self.current -= 1;
            self.on_slide_changed();
        }
    }

    pub(crate) fn goto_slide(&mut self, idx: usize) {
        if idx < self.slides.len() {
            self.current = idx;
            self.on_slide_changed();
        }
    }

    pub(crate) fn next_section(&mut self) {
        let current_section = &self.slides[self.current].section;
        for i in (self.current + 1)..self.slides.len() {
            if self.slides[i].section != *current_section {
                self.current = i;
                self.on_slide_changed();
                return;
            }
        }
    }

    pub(crate) fn prev_section(&mut self) {
        let current_section = &self.slides[self.current].section;
        let mut section_start = self.current;
        while section_start > 0 && self.slides[section_start - 1].section == *current_section {
            section_start -= 1;
        }
        if section_start == 0 { return; }
        let prev_section = &self.slides[section_start - 1].section;
        let mut target = section_start - 1;
        while target > 0 && self.slides[target - 1].section == *prev_section {
            target -= 1;
        }
        self.current = target;
        self.on_slide_changed();
    }

    pub(crate) fn scroll_down(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(n);
    }

    pub(crate) fn scroll_up(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
    }
}
