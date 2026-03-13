//! YAML schema definitions for Ostendo themes.
//!
//! Each theme is stored as a YAML file in the `themes/` directory and
//! deserialized into the [`Theme`] struct defined here. Themes control
//! the visual appearance of a presentation — colors, fonts, layout style,
//! optional gradients, and title decorations.
//!
//! The structs use `serde` derive macros for automatic serialization and
//! deserialization. The `#[serde(default)]` attribute tells serde to use the
//! field type's `Default` value when the key is absent from the YAML file,
//! while `#[serde(default = "function_name")]` calls a specific function to
//! produce the default. This makes theme files forward-compatible — adding a
//! new field with a default function means old theme files still load fine.

use serde::{Deserialize, Serialize};

/// A background color gradient that transitions between two colors.
///
/// Applied per terminal row to create a smooth gradient effect behind
/// slide content. Configured in the theme YAML under the `gradient:` key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeGradient {
    /// Starting color as a CSS hex string (e.g., `"#1a1a2e"`).
    pub from: String,
    /// Ending color as a CSS hex string (e.g., `"#16213e"`).
    pub to: String,
    /// Gradient direction. Currently only `"vertical"` (top-to-bottom) is
    /// supported. Defaults to `"vertical"` if omitted from the YAML.
    #[serde(default = "default_gradient_direction")]
    pub direction: String,
}

/// Provides the default gradient direction when the field is missing from YAML.
fn default_gradient_direction() -> String { "vertical".to_string() }

/// A complete theme definition, loaded from a YAML file in `themes/`.
///
/// Themes are identified by their `slug` (a URL-safe lowercase identifier
/// like `"dracula"` or `"solarized-light"`) and can reference light/dark
/// variant slugs for toggling with the `D` keybinding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    /// Human-readable theme name displayed in the UI (e.g., `"Dracula"`).
    pub name: String,
    /// URL-safe identifier used in commands and state persistence (e.g., `"dracula"`).
    /// Defaults to an empty string and is typically populated by the theme registry
    /// from the YAML filename.
    #[serde(default)]
    pub slug: String,
    /// Color palette for background, accent, text, and code blocks.
    pub colors: ThemeColors,
    /// Font family hints for headings, body text, and code.
    /// These are advisory — actual rendering depends on the terminal's font.
    #[serde(default)]
    pub fonts: ThemeFonts,
    /// Content alignment style: `"left"` or `"center"`.
    /// Defaults to `"left"` if omitted.
    #[serde(default = "default_layout")]
    pub layout: String,
    /// Visual weight for emphasis elements: `"bold"`, `"minimal"`, etc.
    /// Defaults to `"bold"` if omitted.
    #[serde(default = "default_visual_style")]
    pub visual_style: String,
    /// Optional background gradient. When present, the solid `background` color
    /// is replaced with a smooth gradient.
    #[serde(default)]
    pub gradient: Option<ThemeGradient>,
    /// Default title decoration style for all slides (e.g., `"underline"`, `"box"`,
    /// `"banner"`). Individual slides can override this with a directive.
    #[serde(default)]
    pub title_decoration: Option<String>,
    /// Slug of the dark variant theme. Used by the `D` key toggle when the
    /// current theme is the light variant.
    #[serde(default)]
    pub dark_variant: Option<String>,
    /// Slug of the light variant theme. Used by the `D` key toggle when the
    /// current theme is the dark variant.
    #[serde(default)]
    pub light_variant: Option<String>,
}

/// Color palette for a theme.
///
/// All colors are stored as CSS hex strings (e.g., `"#282A36"`). The theme
/// system validates that text/accent colors have sufficient contrast against
/// the background using WCAG 2.0 ratios (see `crate::theme::colors`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeColors {
    /// Page background color (e.g., `"#282A36"`).
    pub background: String,
    /// Accent color used for bullets, borders, highlights (e.g., `"#BD93F9"`).
    pub accent: String,
    /// Primary text color (e.g., `"#F8F8F2"`).
    pub text: String,
    /// Background color for fenced code blocks. Defaults to `"#1A1A1A"` if omitted.
    #[serde(default = "default_code_bg")]
    pub code_background: String,
}

/// Font family hints for different content types within a theme.
///
/// These are advisory metadata — the terminal's configured font ultimately
/// determines what is rendered. All fields default to empty strings when
/// omitted from the YAML, meaning the terminal's default font is used.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThemeFonts {
    /// Preferred font family for headings / titles.
    #[serde(default)]
    pub heading: String,
    /// Preferred font family for body text / bullets.
    #[serde(default)]
    pub body: String,
    /// Preferred font family for code blocks.
    #[serde(default)]
    pub code: String,
}

/// Provides the default layout value when the field is missing from YAML.
fn default_layout() -> String { "left".to_string() }
/// Provides the default visual style when the field is missing from YAML.
fn default_visual_style() -> String { "bold".to_string() }
/// Provides the default code background color when the field is missing from YAML.
fn default_code_bg() -> String { "#1A1A1A".to_string() }
