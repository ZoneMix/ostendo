//! Built-in themes and color utilities.

mod builtin;
pub mod colors;
pub mod schema;

use std::sync::LazyLock;

pub use schema::Theme;

static BUILTIN: LazyLock<Vec<Theme>> = LazyLock::new(builtin::load_builtin_themes);

/// Handle to the built-in themes, which are parsed once per process.
pub struct ThemeRegistry {
    themes: &'static [Theme],
}

impl ThemeRegistry {
    pub fn load() -> Self {
        Self { themes: &BUILTIN }
    }

    pub fn get(&self, slug: &str) -> Option<Theme> {
        self.themes.iter().find(|t| t.slug == slug).cloned()
    }

    /// Slugs in file name order.
    pub fn list(&self) -> Vec<String> {
        self.themes.iter().map(|t| t.slug.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn color(slug: &str, hex: &str) -> crossterm::style::Color {
        colors::hex_to_color(hex).unwrap_or_else(|| panic!("{slug}: invalid color {hex:?}"))
    }

    /// WCAG 2.0 AA: text:background >= 4.5 (normal text), accent:background >= 3.0 (large
    /// text and UI); code blocks need a visible 1.2 against the page.
    #[test]
    fn builtin_themes_meet_contrast_minimums() {
        let registry = ThemeRegistry::load();
        for theme in registry.themes {
            let slug = &theme.slug;
            let c = &theme.colors;
            let bg = color(slug, &c.background);
            for (name, hex, min) in [
                ("text", &c.text, 4.5),
                ("accent", &c.accent, 3.0),
                ("code_background", &c.code_background, 1.2),
            ] {
                let ratio = colors::contrast_ratio(color(slug, hex), bg);
                assert!(
                    ratio >= min,
                    "{slug}: {name}:background contrast {ratio:.2} < {min}"
                );
            }
            if let Some(g) = &theme.gradient {
                color(slug, &g.from);
                color(slug, &g.to);
            }
        }
    }

    #[test]
    fn default_and_variant_slugs_resolve() {
        let registry = ThemeRegistry::load();
        // main.rs falls back to it when --theme names an unknown slug.
        assert!(registry.get("terminal_green").is_some());
        for theme in registry.themes {
            for variant in [&theme.light_variant, &theme.dark_variant]
                .into_iter()
                .flatten()
            {
                assert!(
                    registry.get(variant).is_some(),
                    "{}: unknown variant {variant}",
                    theme.slug
                );
            }
        }
    }
}
