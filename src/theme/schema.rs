//! YAML schema for `themes/*.yaml`. Keys not listed here are ignored.

use serde::Deserialize;

/// Background gradient, drawn top to bottom behind slide content.
#[derive(Debug, Clone, Deserialize)]
pub struct ThemeGradient {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Theme {
    pub name: String,
    /// Defaults to the YAML file stem.
    #[serde(default)]
    pub slug: String,
    pub colors: ThemeColors,
    pub gradient: Option<ThemeGradient>,
    /// Decoration for slides without a `title_decoration` directive.
    pub title_decoration: Option<String>,
    /// Companion slugs for the `D` dark/light toggle.
    pub dark_variant: Option<String>,
    pub light_variant: Option<String>,
}

/// `#RRGGBB` colors.
#[derive(Debug, Clone, Deserialize)]
pub struct ThemeColors {
    pub background: String,
    pub accent: String,
    pub text: String,
    #[serde(default = "default_code_bg")]
    pub code_background: String,
}

fn default_code_bg() -> String {
    "#1A1A1A".to_string()
}
