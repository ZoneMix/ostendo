//! Syntax highlighting for code blocks via syntect's bundled grammars.

use crossterm::style::Color;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

pub struct Highlighter {
    syntaxes: SyntaxSet,
    dark: Theme,
    light: Theme,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HighlightedSpan {
    pub text: String,
    pub fg: Color,
}

impl Highlighter {
    pub fn new() -> Self {
        let mut themes = ThemeSet::load_defaults().themes;
        Self {
            syntaxes: SyntaxSet::load_defaults_newlines(),
            dark: themes.remove("base16-eighties.dark").unwrap_or_default(),
            light: themes.remove("InspiredGitHub").unwrap_or_default(),
        }
    }

    /// Highlights `code` line by line (without line endings). `dark` selects
    /// colors readable on a dark code background.
    pub fn highlight(&self, code: &str, language: &str, dark: bool) -> Vec<Vec<HighlightedSpan>> {
        let theme = if dark { &self.dark } else { &self.light };
        let mut h = HighlightLines::new(self.syntax(language), theme);
        LinesWithEndings::from(code)
            .map(|line| {
                let ranges = h.highlight_line(line, &self.syntaxes).unwrap_or_default();
                ranges
                    .into_iter()
                    .map(|(style, text)| HighlightedSpan {
                        text: text.trim_end_matches(['\n', '\r']).replace('\t', "    "),
                        fg: adjust(style.foreground, dark),
                    })
                    .filter(|span| !span.text.is_empty())
                    .collect()
            })
            .collect()
    }

    fn syntax(&self, language: &str) -> &SyntaxReference {
        // Info strings like `rust,ignore` name the language first.
        let language = language.split([',', ' ']).next().unwrap_or_default();
        let token = match language.to_lowercase().as_str() {
            "c++" | "cxx" => "cpp".to_string(),
            "shell" | "zsh" | "console" | "shell-session" => "bash".to_string(),
            "golang" => "go".to_string(),
            other => other.to_string(),
        };
        self.syntaxes
            .find_syntax_by_token(&token)
            .unwrap_or_else(|| self.syntaxes.find_syntax_plain_text())
    }
}

/// Nudges dark-theme colors brighter so they read on projected backgrounds.
fn adjust(c: syntect::highlighting::Color, dark: bool) -> Color {
    let lift = |v: u8| {
        if dark {
            v.saturating_add((255 - v) / 8)
        } else {
            v
        }
    };
    Color::Rgb {
        r: lift(c.r),
        g: lift(c.g),
        b: lift(c.b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_expand_and_line_endings_are_stripped() {
        let h = Highlighter::new();
        let lines = h.highlight("\tx = 1\r\ny\n", "python", true);
        let text: Vec<String> = lines
            .iter()
            .map(|l| l.iter().map(|s| s.text.as_str()).collect())
            .collect();
        assert_eq!(text, ["    x = 1", "y"]);
    }

    #[test]
    fn light_pages_get_a_light_syntax_theme() {
        let h = Highlighter::new();
        let dark = h.highlight("fn main() {}", "rust", true);
        let light = h.highlight("fn main() {}", "rust", false);
        assert_ne!(dark, light);
    }
}
