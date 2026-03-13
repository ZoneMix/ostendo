//! Theme registry and management.
//!
//! Loads 29 built-in YAML themes at startup (compiled into the binary by `build.rs`),
//! validates WCAG 2.0 contrast ratios in tests, and supports runtime theme switching
//! via the `:theme <slug>` command or the `D` key for light/dark variant toggling.
//!
//! # Submodules
//! - [`schema`] — The `Theme` struct and its nested types (colors, fonts, gradients).
//! - [`builtin`] — Auto-generated theme list populated at compile time from `themes/*.yaml`.
//! - [`colors`] — Hex-to-color conversion and WCAG contrast ratio computation.
//!
//! # Contrast Requirements (enforced in tests)
//! - `text:background` >= 4.5:1 (WCAG AA normal text)
//! - `accent:background` >= 3.0:1 (WCAG AA large text / UI components)
//! - `code_background:background` >= 1.2:1 (subtle distinction)

pub mod schema;
pub mod builtin;
pub mod colors;

pub use schema::Theme;

/// In-memory registry of all available presentation themes.
///
/// Created once at startup via [`ThemeRegistry::load`] and then queried by slug
/// throughout the application lifetime. The registry is immutable after creation —
/// themes cannot be added or removed at runtime.
pub struct ThemeRegistry {
    /// The full list of parsed themes, loaded from the compiled-in YAML sources.
    themes: Vec<Theme>,
}

impl ThemeRegistry {
    /// Load all built-in themes from the compiled YAML sources.
    ///
    /// This calls [`builtin::load_builtin_themes`] which deserializes each YAML
    /// string into a `Theme` struct. Themes with invalid YAML are silently skipped
    /// (via `filter_map`).
    pub fn load() -> Self {
        let themes = builtin::load_builtin_themes();
        Self { themes }
    }

    /// Look up a theme by its unique slug identifier (e.g., `"dracula"`, `"nord"`).
    ///
    /// Returns `None` if no theme with that slug exists. The returned `Theme` is
    /// a clone — callers get their own copy and cannot affect the registry.
    pub fn get(&self, slug: &str) -> Option<Theme> {
        self.themes.iter().find(|t| t.slug == slug).cloned()
    }

    /// List all available theme slugs, in the order they were loaded.
    ///
    /// Used by the CLI `--list-themes` flag and the remote control theme dropdown.
    pub fn list(&self) -> Vec<String> {
        self.themes.iter().map(|t| t.slug.clone()).collect()
    }

    /// Get the light or dark variant of a theme, if it has one.
    ///
    /// Themes can declare `light_variant` or `dark_variant` slugs in their YAML.
    /// When the user presses `D` to toggle dark/light mode, this method finds the
    /// companion theme. Returns `None` if the theme has no variant in the
    /// requested direction.
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
