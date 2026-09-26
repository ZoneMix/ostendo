//! Resolved colors for the active theme.

use crossterm::style::Color;

use crate::theme::colors::{
    color_to_rgb, contrast_ratio, hex_to_color, interpolate_color, relative_luminance,
};
use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Palette {
    pub bg: Color,
    pub text: Color,
    pub accent: Color,
    pub code_bg: Color,
    /// Secondary text (captions, labels): text blended toward the background.
    pub muted: Color,
    /// Chrome surfaces (status bar, notes, badges) that must stand off the page.
    pub surface: Color,
    pub gradient: Option<(Color, Color)>,
}

impl Palette {
    /// `accent` overrides the theme accent (front-matter `accent:`) when it
    /// meets the same 3:1 contrast every built-in theme accent does.
    pub fn new(theme: &Theme, accent: Option<Color>) -> Self {
        let color = |hex: &str, fallback| hex_to_color(hex).unwrap_or(fallback);
        let bg = color(&theme.colors.background, Color::Black);
        let text = color(&theme.colors.text, Color::White);
        let code_bg = color(&theme.colors.code_background, Color::DarkGrey);
        let gradient = theme
            .gradient
            .as_ref()
            .and_then(|g| Some((hex_to_color(&g.from)?, hex_to_color(&g.to)?)));
        Self {
            bg,
            text,
            accent: accent
                .filter(|&a| contrast_ratio(a, bg) >= 3.0)
                .unwrap_or_else(|| color(&theme.colors.accent, Color::Green)),
            code_bg,
            muted: interpolate_color(text, bg, 0.45),
            // A panel a notch off the page: the code background when it stands
            // apart, otherwise a light tint of the text color.
            surface: if contrast_ratio(code_bg, bg) >= 1.12 {
                code_bg
            } else {
                interpolate_color(bg, text, 0.09)
            },
            gradient,
        }
    }

    /// Background for `row` of `total`, following the theme gradient if any.
    pub fn row_bg(&self, row: usize, total: usize) -> Color {
        match self.gradient {
            Some((from, to)) if total > 1 => {
                interpolate_color(from, to, row as f64 / (total - 1) as f64)
            }
            Some((from, _)) => from,
            None => self.bg,
        }
    }

    /// Whether code sits on a dark background, which picks the syntax theme.
    pub fn is_dark(&self) -> bool {
        color_to_rgb(self.code_bg).is_none_or(|(r, g, b)| relative_luminance(r, g, b) < 0.18)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeRegistry;

    #[test]
    fn dark_and_light_themes_pick_matching_code_colors() {
        let registry = ThemeRegistry::load();
        let dark = Palette::new(&registry.get("terminal_green").unwrap(), None);
        let light = Palette::new(&registry.get("paper").unwrap(), None);
        assert!(dark.is_dark());
        assert!(!light.is_dark());
    }

    #[test]
    fn accent_override_must_stay_readable() {
        let registry = ThemeRegistry::load();
        let cyan = Color::Rgb {
            r: 0,
            g: 229,
            b: 255,
        };
        let dark = registry.get("terminal_green").unwrap();
        let light = registry.get("paper").unwrap();
        assert_eq!(Palette::new(&dark, Some(cyan)).accent, cyan);
        assert_ne!(Palette::new(&light, Some(cyan)).accent, cyan);
    }
}
