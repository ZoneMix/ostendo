//! Themes, view toggles, and persisted state.

use std::time::{Duration, Instant};

use crate::presentation::rehearsal::{self, clock};
use crate::theme::Theme;

use super::palette::Palette;
use super::Presenter;

impl Presenter {
    /// Marks the slide frame stale (content, colors, or toggles changed).
    pub(crate) fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    /// Sets the user's theme (`:theme`, `D`, remote); slide overrides still apply.
    pub(crate) fn set_base_theme(&mut self, theme: Theme) {
        self.base_theme = theme;
        self.apply_slide_theme();
    }

    /// Shows the current slide's `<!-- theme -->` override, or the base theme.
    pub(crate) fn apply_slide_theme(&mut self) {
        let wanted = self.slides[self.current]
            .theme_override
            .as_deref()
            .and_then(|slug| self.registry.get(slug))
            .unwrap_or_else(|| self.base_theme.clone());
        if wanted.slug == self.theme.slug && self.palette == self.palette_for(&wanted) {
            return;
        }
        self.palette = self.palette_for(&wanted);
        self.theme = wanted;
        self.images.clear();
        self.display.invalidate();
        self.invalidate();
    }

    /// The front-matter accent applies to the deck's own theme, not to
    /// slide-level overrides that bring their own look.
    fn palette_for(&self, theme: &Theme) -> Palette {
        let accent = self
            .accent_override
            .filter(|_| theme.slug == self.base_theme.slug);
        Palette::new(theme, accent)
    }

    /// Switches the base theme to its light or dark counterpart.
    pub(crate) fn toggle_dark_mode(&mut self) {
        let counterpart = self
            .base_theme
            .light_variant
            .as_deref()
            .or(self.base_theme.dark_variant.as_deref())
            .and_then(|slug| self.registry.get(slug));
        if let Some(theme) = counterpart {
            self.set_base_theme(theme);
            self.save_state();
        }
    }

    pub(crate) fn toggle_notes(&mut self) {
        self.show_notes = !self.show_notes;
        self.notes_scroll = 0;
    }

    /// Puts the notes beside the slide, or back below it, showing them.
    pub(crate) fn move_notes(&mut self) {
        self.show_notes = true;
        self.notes_side = !self.notes_side;
        self.save_state();
    }

    pub(crate) fn resize_notes(&mut self, delta: i16) {
        self.show_notes = true;
        self.notes_share = (i16::from(self.notes_share) + delta).clamp(15, 60) as u8;
        self.save_state();
    }

    pub(crate) fn toggle_fullscreen(&mut self) {
        self.fullscreen = !self.fullscreen;
    }

    pub(crate) fn toggle_sections(&mut self) {
        self.show_sections = !self.show_sections;
    }

    pub(crate) fn adjust_scale(&mut self, delta: i16) {
        self.scale = (i16::from(self.scale) + delta).clamp(40, 100) as u8;
    }

    pub(crate) fn adjust_image_scale(&mut self, delta: i8) {
        self.image_scale_offset = self.image_scale_offset.saturating_add(delta).clamp(-90, 90);
    }

    /// Starts the timer, or resets it when already running.
    pub(crate) fn toggle_timer(&mut self) {
        match self.timer_start {
            Some(_) => self.reset_timer(),
            None => self.start_timer(),
        }
    }

    pub(crate) fn start_timer(&mut self) {
        if self.timer_start.is_none() {
            self.timer_start = Some(Instant::now());
            self.entered = Instant::now();
        }
    }

    /// Stops the timer; a reset starts the rehearsal over, so the time per
    /// slide goes too.
    pub(crate) fn reset_timer(&mut self) {
        self.timer_start = None;
        self.slide_time.fill(Duration::ZERO);
    }

    /// Credits the time since the last call to the current slide while the
    /// timer runs; called before the current slide changes.
    pub(crate) fn clock_slide(&mut self) {
        let now = Instant::now();
        if self.timer_start.is_some() {
            if let Some(time) = self.slide_time.get_mut(self.current) {
                *time += now - self.entered;
            }
        }
        self.entered = now;
    }

    /// Saves this run's time per slide for `--report`; runs under a minute
    /// are a quick look, not a rehearsal.
    pub(crate) fn record_rehearsal(&mut self) {
        self.clock_slide();
        if self.slide_time.iter().sum::<Duration>() < Duration::from_secs(60) {
            return;
        }
        let slides = self
            .slides
            .iter()
            .zip(&self.slide_time)
            .map(|(s, t)| (s.title.clone(), t.as_secs_f64()))
            .collect();
        let _ = rehearsal::record(&self.presentation_path, rehearsal::Run::new(slides));
    }

    /// Elapsed time as `m:ss` or `h:mm:ss`, when the timer is running.
    pub(crate) fn timer_text(&self) -> Option<String> {
        Some(clock(self.timer_start?.elapsed()))
    }

    /// With a front-matter `duration`: how far the talk is behind the pace
    /// that ends on time (`None` when on pace), and whether it ran over.
    pub(crate) fn pace(&self) -> Option<(Option<Duration>, bool)> {
        let (total, start) = (self.meta.duration?, self.timer_start?);
        let elapsed = start.elapsed();
        // The share of the talk the slides so far should have taken.
        let planned = total.mul_f64((self.current + 1) as f64 / self.slides.len() as f64);
        let behind = elapsed
            .checked_sub(planned)
            .filter(|late| *late > Duration::from_secs(30));
        Some((behind, elapsed > total))
    }

    pub(crate) fn save_state(&mut self) {
        self.state.set_current_slide(self.current);
        self.state.set_font_offsets(self.font.user_offsets());
        self.state.set_theme_slug(&self.base_theme.slug);
        self.state.set_image_scale_offset(self.image_scale_offset);
        self.state
            .set_notes_layout(self.notes_side, self.notes_share);
        let _ = self.state.save();
    }
}
