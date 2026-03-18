use super::*;

impl Presenter {
    pub(crate) fn toggle_fullscreen(&mut self) {
        self.show_fullscreen = !self.show_fullscreen;
        self.user_fullscreen_override = Some(self.show_fullscreen);
        // Delete old Kitty placements — viewport size changes, image positions shift
        self.clear_kitty_placements();
        self.needs_full_redraw = true;
    }

    pub(crate) fn toggle_notes(&mut self) {
        self.show_notes = !self.show_notes;
        self.notes_scroll = 0;
        // Delete old Kitty placements — viewport size changes when notes panel shows/hides
        self.clear_kitty_placements();
        self.needs_full_redraw = true;
    }

    /// Delete all Kitty image placements (but keep data in terminal memory).
    /// Called when viewport layout changes (notes toggle, fullscreen toggle)
    /// so images are re-placed at correct positions on next render_frame().
    fn clear_kitty_placements(&self) {
        if self.image_protocol == ImageProtocol::Kitty {
            // d=a: delete all visible placements, keep image data for re-placement
            let clear = "\x1b_Ga=d,d=a,q=2;AAAA\x1b\\";
            let _ = std::io::Write::write_all(&mut std::io::stdout(), clear.as_bytes());
            let _ = std::io::Write::flush(&mut std::io::stdout());
        }
    }

    pub(crate) fn toggle_theme_name(&mut self) {
        self.show_theme_name = !self.show_theme_name;
        self.needs_full_redraw = true;
    }

    pub(crate) fn toggle_sections(&mut self) {
        self.show_sections = !self.show_sections;
        self.needs_full_redraw = true;
    }

    pub(crate) fn toggle_dark_mode(&mut self) {
        let registry = crate::theme::ThemeRegistry::load();
        if let Some(variant) = registry.get_variant(&self.theme, !self.is_light_variant) {
            self.is_light_variant = !self.is_light_variant;
            self.base_theme = variant.clone();
            self.apply_theme(variant);
        }
    }

    pub(crate) fn scale_up(&mut self) {
        self.global_scale = (self.global_scale + 5).min(200);
        self.needs_full_redraw = true;
    }

    pub(crate) fn scale_down(&mut self) {
        self.global_scale = self.global_scale.saturating_sub(5).max(50);
        self.needs_full_redraw = true;
    }

    pub(crate) fn image_scale_up(&mut self) {
        self.image_scale_offset = (self.image_scale_offset + 10).min(100);
        self.needs_full_redraw = true;
    }

    pub(crate) fn image_scale_down(&mut self) {
        self.image_scale_offset = (self.image_scale_offset - 10).max(-90);
        self.needs_full_redraw = true;
    }

    pub(crate) fn adjust_font_offset(&mut self, delta: i8) {
        if self.font_capability.is_available() {
            let cur = self.slide_font_offsets.get(&self.current).copied().unwrap_or(0);
            let new = cur + delta;
            if (-20..=20).contains(&new) {
                self.slide_font_offsets.insert(self.current, new);
                self.font_change_is_slide_transition = false;
                self.apply_slide_font();
                self.needs_full_redraw = true;
                self.save_state();
            }
        }
    }

    pub(crate) fn reset_font_offset(&mut self) {
        if self.font_capability.is_available() {
            self.slide_font_offsets.remove(&self.current);
            self.font_change_is_slide_transition = false;
            self.apply_slide_font();
            self.needs_full_redraw = true;
            self.save_state();
        }
    }

    pub(crate) fn current_scale(&self) -> u8 {
        self.global_scale
    }

    /// Apply a theme, updating all cached color fields.
    pub(crate) fn apply_theme(&mut self, new_theme: Theme) {
        self.bg_color = hex_to_color(&new_theme.colors.background).unwrap_or(Color::Black);
        self.accent_color = hex_to_color(&new_theme.colors.accent).unwrap_or(Color::Green);
        self.text_color = hex_to_color(&new_theme.colors.text).unwrap_or(Color::White);
        self.code_bg_color = hex_to_color(&new_theme.colors.code_background).unwrap_or(Color::DarkGrey);
        // Parse gradient
        if let Some(ref grad) = new_theme.gradient {
            self.gradient_from = hex_to_color(&grad.from);
            self.gradient_to = hex_to_color(&grad.to);
            self.gradient_vertical = grad.direction != "horizontal";
        } else {
            self.gradient_from = None;
            self.gradient_to = None;
        }
        self.help_badge_bg = ensure_badge_contrast(self.code_bg_color, self.bg_color);
        Self::set_terminal_bg(self.bg_color);
        self.theme = new_theme;
        self.image_cache.clear();
        self.needs_full_redraw = true;
    }

    /// Compute the background color for a given row, applying gradient if configured.
    pub(crate) fn row_bg_color(&self, row: usize, total_rows: usize) -> Color {
        if let (Some(from), Some(to)) = (self.gradient_from, self.gradient_to) {
            let t = if total_rows <= 1 { 0.0 } else { row as f64 / (total_rows - 1) as f64 };
            interpolate_color(from, to, t)
        } else {
            self.bg_color
        }
    }

    /// Persist current state (slide position, font offsets, theme) to disk.
    pub(crate) fn save_state(&mut self) {
        self.state.set_current_slide(self.current);
        for (&slide, &offset) in &self.slide_font_offsets {
            self.state.set_font_offset(slide, offset);
        }
        self.state.set_theme_slug(&self.theme.slug);
        let _ = self.state.save();
    }

    pub(crate) fn format_timer(&self) -> String {
        match self.timer_start {
            Some(start) => {
                let elapsed = start.elapsed().as_secs();
                let h = elapsed / 3600;
                let m = (elapsed % 3600) / 60;
                let s = elapsed % 60;
                format!("{:02}:{:02}:{:02}", h, m, s)
            }
            None => "00:00:00".to_string(),
        }
    }
}
