//! Syntax highlighting wrapper using the `syntect` library.
//!
//! Highlights code blocks for 200+ languages using TextMate-compatible grammars
//! bundled inside the `syntect` crate.  The highlighter converts source code into
//! a grid of [`HighlightedSpan`]s (text + foreground color) that the rendering
//! engine maps to [`StyledSpan`](crate::render::text::StyledSpan)s for display.
//!
//! # Theme selection
//!
//! A default theme (`base16-eighties.dark`) is used unless overridden via
//! [`Highlighter::with_theme`].  The list of bundled theme names is in
//! [`HIGHLIGHT_THEMES`].
//!
//! # Color brightening
//!
//! Foreground colors from `syntect` are slightly brightened (shifted toward white
//! by ~12%) to improve readability on the dark terminal backgrounds that most
//! Ostendo themes use.

use crossterm::style::Color;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

/// Names of the bundled syntect highlight themes that can be passed to
/// [`Highlighter::with_theme`].
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

/// Stateful syntax highlighter that holds loaded grammar and theme sets.
///
/// Create once with [`Highlighter::new`] and reuse across slides -- loading
/// the syntax and theme sets is expensive, so this avoids repeated work.
pub struct Highlighter {
    /// The collection of language grammars (loaded from syntect's built-in defaults).
    syntax_set: SyntaxSet,
    /// The collection of color themes (loaded from syntect's built-in defaults).
    theme_set: ThemeSet,
    /// The currently active theme name (must be a key in `theme_set.themes`).
    theme_name: String,
}

/// A single piece of syntax-highlighted text with its foreground color.
///
/// The rendering engine converts these into [`StyledSpan`](crate::render::text::StyledSpan)s
/// when building the virtual buffer for a code block.
#[derive(Debug, Clone)]
pub struct HighlightedSpan {
    /// The text content (may include trailing newline from the source line).
    pub text: String,
    /// The foreground color determined by the syntax theme.
    pub fg: Color,
}

impl Highlighter {
    /// Create a new highlighter with default grammars and the `base16-eighties.dark` theme.
    pub fn new() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
            theme_name: "base16-eighties.dark".to_string(),
        }
    }

    /// Switch to a different bundled theme.  If `theme_name` is not found in the
    /// theme set, the current theme is kept (no error).  Builder method.
    #[allow(dead_code)]
    pub fn with_theme(mut self, theme_name: &str) -> Self {
        if self.theme_set.themes.contains_key(theme_name) {
            self.theme_name = theme_name.to_string();
        }
        self
    }

    /// Highlight a block of source code, returning one `Vec<HighlightedSpan>` per line.
    ///
    /// # Parameters
    ///
    /// - `code` -- the full source code string (may contain multiple lines).
    /// - `language` -- the language token used to select the grammar (e.g. `"rust"`,
    ///   `"python"`, `"js"`).  Falls back to plain text if the language is unknown.
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
