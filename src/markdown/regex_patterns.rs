//! Line patterns for the presentation parser, compiled once.

use regex::Regex;
use std::sync::LazyLock;

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("static regex")
}

/// Single-line `<!-- name -->` or `<!-- name: value -->`: (1) name, (2) value.
pub(crate) static DIRECTIVE_RE: LazyLock<Regex> =
    LazyLock::new(|| re(r"^\s*<!--\s*(\w+)\s*(?::\s*(.*?))?\s*-->"));

/// `key: value` line in the front matter block.
pub(crate) static FRONT_MATTER_KV_RE: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(\w+)\s*:\s*(.+)$"));

/// Code fence opener: (1) language, (2) `+exec`/`+pty`, (3) `{label: "..."}` text.
pub(crate) static FENCE_OPEN_RE: LazyLock<Regex> = LazyLock::new(|| {
    re(r#"^```(\w*)\s*(\+exec|\+pty)?\s*(?:\{label:\s*"([^"]*)"\s*\})?\s*$"#)
});

/// ```` ```diagram style=<name> ````: (1) style.
pub(crate) static DIAGRAM_FENCE_RE: LazyLock<Regex> =
    LazyLock::new(|| re(r"^```diagram\s*(?:style=(\w+))?\s*$"));

pub(crate) static FENCE_CLOSE_RE: LazyLock<Regex> = LazyLock::new(|| re(r"^```\s*$"));

/// `<!-- notes:` with the text continuing on the following lines.
pub(crate) static NOTES_MULTI_START_RE: LazyLock<Regex> =
    LazyLock::new(|| re(r"^\s*<!--\s*notes:\s*$"));

pub(crate) static NOTES_END_RE: LazyLock<Regex> = LazyLock::new(|| re(r"-->\s*$"));

pub(crate) static HTML_COMMENT_RE: LazyLock<Regex> = LazyLock::new(|| re(r"^\s*<!--.*-->\s*$"));

pub(crate) static TITLE_RE: LazyLock<Regex> = LazyLock::new(|| re(r"^#\s+(.+)$"));

/// `![alt](path)`: (1) alt, (2) path.
pub(crate) static IMAGE_RE: LazyLock<Regex> =
    LazyLock::new(|| re(r"^!\[([^\]]*)\]\(([^)]+)\)\s*$"));

/// (1) indentation, (2) item text.
pub(crate) static BULLET_RE: LazyLock<Regex> = LazyLock::new(|| re(r"^(\s*)[-*]\s*(.*)$"));

pub(crate) static SLIDE_SEPARATOR_RE: LazyLock<Regex> = LazyLock::new(|| re(r"(?m)^---\s*$"));

pub(crate) static TABLE_ROW_RE: LazyLock<Regex> = LazyLock::new(|| re(r"^\|(.+)\|\s*$"));

/// `| :--- | :---: | ---: |`
pub(crate) static TABLE_SEP_RE: LazyLock<Regex> =
    LazyLock::new(|| re(r"^\|[\s:]*-+[\s:]*(\|[\s:]*-+[\s:]*)*\|\s*$"));

/// (1) quoted text.
pub(crate) static BLOCKQUOTE_RE: LazyLock<Regex> = LazyLock::new(|| re(r"^>\s?(.*)$"));
