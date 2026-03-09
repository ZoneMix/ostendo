use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    #[serde(default)]
    pub slug: String,
    pub colors: ThemeColors,
    #[serde(default)]
    pub fonts: ThemeFonts,
    #[serde(default = "default_layout")]
    pub layout: String,
    #[serde(default = "default_visual_style")]
    pub visual_style: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeColors {
    pub background: String,
    pub accent: String,
    pub text: String,
    #[serde(default = "default_code_bg")]
    pub code_background: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThemeFonts {
    #[serde(default)]
    pub heading: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub code: String,
}

fn default_layout() -> String { "left".to_string() }
fn default_visual_style() -> String { "bold".to_string() }
fn default_code_bg() -> String { "#1A1A1A".to_string() }
