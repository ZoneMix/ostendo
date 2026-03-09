use crossterm::style::Color;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

/// Available syntect highlight themes (bundled).
#[allow(dead_code)]
pub const HIGHLIGHT_THEMES: &[&str] = &[
    "base16-ocean.dark",
    "base16-eighties.dark",
    "base16-mocha.dark",
    "base16-ocean.light",
    "InspiredGitHub",
    "Solarized (dark)",
    "Solarized (light)",
];

pub struct Highlighter {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    theme_name: String,
}

#[derive(Debug, Clone)]
pub struct HighlightedSpan {
    pub text: String,
    pub fg: Color,
}

impl Highlighter {
    pub fn new() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
            theme_name: "base16-eighties.dark".to_string(),
        }
    }

    #[allow(dead_code)]
    pub fn with_theme(mut self, theme_name: &str) -> Self {
        if self.theme_set.themes.contains_key(theme_name) {
            self.theme_name = theme_name.to_string();
        }
        self
    }

    pub fn highlight(&self, code: &str, language: &str) -> Vec<Vec<HighlightedSpan>> {
        let syntax = self
            .syntax_set
            .find_syntax_by_token(language)
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let theme = &self.theme_set.themes[&self.theme_name];
        let mut h = HighlightLines::new(syntax, theme);

        let mut lines = Vec::new();
        for line in LinesWithEndings::from(code) {
            let ranges: Vec<(Style, &str)> = h
                .highlight_line(line, &self.syntax_set)
                .unwrap_or_default();
            let spans: Vec<HighlightedSpan> = ranges
                .into_iter()
                .map(|(style, text)| {
                    // Brighten colors slightly for better visibility on dark backgrounds
                    let r = style.foreground.r;
                    let g = style.foreground.g;
                    let b = style.foreground.b;
                    let fg = Color::Rgb {
                        r: r.saturating_add((255 - r) / 8),
                        g: g.saturating_add((255 - g) / 8),
                        b: b.saturating_add((255 - b) / 8),
                    };
                    HighlightedSpan {
                        text: text.to_string(),
                        fg,
                    }
                })
                .collect();
            lines.push(spans);
        }
        lines
    }
}
