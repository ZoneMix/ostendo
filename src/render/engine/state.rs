//! Themes, view toggles, and persisted state.

use std::time::{Duration, Instant};

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
        super::terminal::set_background(self.palette.bg);
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
        self.timer_start = match self.timer_start {
            Some(_) => None,
            None => Some(Instant::now()),
        };
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
        let _ = self.state.save();
    }
}

/// `m:ss`, or `h:mm:ss` past an hour.
pub(crate) fn clock(d: Duration) -> String {
    let secs = d.as_secs();
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}
