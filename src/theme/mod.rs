pub mod schema;
pub mod builtin;
pub mod colors;

pub use schema::Theme;

pub struct ThemeRegistry {
    themes: Vec<Theme>,
}

impl ThemeRegistry {
    pub fn load() -> Self {
        let themes = builtin::load_builtin_themes();
        Self { themes }
    }

    pub fn get(&self, slug: &str) -> Option<Theme> {
        self.themes.iter().find(|t| t.slug == slug).cloned()
    }

    pub fn list(&self) -> Vec<String> {
        self.themes.iter().map(|t| t.slug.clone()).collect()
    }

    /// Get the light or dark variant of a theme, if it has one.
    pub fn get_variant(&self, theme: &Theme, want_light: bool) -> Option<Theme> {
        let variant_slug = if want_light {
            theme.light_variant.as_deref()
        } else {
            theme.dark_variant.as_deref()
        };
        variant_slug.and_then(|slug| self.get(slug))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_themes_load() {
        let registry = ThemeRegistry::load();
        let themes = registry.list();
        assert!(themes.len() >= 20, "Expected at least 20 themes, got {}", themes.len());
    }

    #[test]
    fn test_theme_has_colors() {
        let registry = ThemeRegistry::load();
        for slug in registry.list() {
            let theme = registry.get(&slug).unwrap();
            assert!(!theme.colors.background.is_empty(), "{} missing background", slug);
            assert!(!theme.colors.accent.is_empty(), "{} missing accent", slug);
            assert!(!theme.colors.text.is_empty(), "{} missing text", slug);
            assert!(!theme.colors.code_background.is_empty(), "{} missing code_background", slug);
        }
    }

    #[test]
    fn test_get_existing_theme() {
        let registry = ThemeRegistry::load();
        assert!(registry.get("terminal_green").is_some());
        assert!(registry.get("dracula").is_some());
        assert!(registry.get("solarized").is_some());
    }

    #[test]
    fn test_get_nonexistent_theme() {
        let registry = ThemeRegistry::load();
        assert!(registry.get("nonexistent_theme").is_none());
    }

    #[test]
    fn test_theme_contrast_ratios() {
        let registry = ThemeRegistry::load();
        for slug in registry.list() {
            let theme = registry.get(&slug).unwrap();
            let bg = colors::hex_to_color(&theme.colors.background).unwrap();
            let text = colors::hex_to_color(&theme.colors.text).unwrap();
            let accent = colors::hex_to_color(&theme.colors.accent).unwrap();
            let code_bg = colors::hex_to_color(&theme.colors.code_background).unwrap();

            let text_ratio = colors::contrast_ratio(text, bg);
            let accent_ratio = colors::contrast_ratio(accent, bg);
            let code_ratio = colors::contrast_ratio(code_bg, bg);

            assert!(
                text_ratio >= 4.5,
                "{}: text:bg contrast {:.2} < 4.5 (text={}, bg={})",
                slug, text_ratio, theme.colors.text, theme.colors.background
            );
            assert!(
                accent_ratio >= 3.0,
                "{}: accent:bg contrast {:.2} < 3.0 (accent={}, bg={})",
                slug, accent_ratio, theme.colors.accent, theme.colors.background
            );
            assert!(
                code_ratio >= 1.2,
                "{}: code_bg:bg contrast {:.2} < 1.2 (code_bg={}, bg={})",
                slug, code_ratio, theme.colors.code_background, theme.colors.background
            );
        }
    }

    #[test]
    fn test_theme_colors_are_valid_hex() {
        let registry = ThemeRegistry::load();
        for slug in registry.list() {
            let theme = registry.get(&slug).unwrap();
            assert!(
                colors::hex_to_color(&theme.colors.background).is_some(),
                "{}: invalid background color '{}'", slug, theme.colors.background
            );
            assert!(
                colors::hex_to_color(&theme.colors.accent).is_some(),
                "{}: invalid accent color '{}'", slug, theme.colors.accent
            );
        }
    }
}
