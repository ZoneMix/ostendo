//! Line patterns for the presentation parser, compiled once.

use regex::Regex;
use std::sync::LazyLock;

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("static regex")
}

/// Single-line `<!-- name -->` or `<!-- name: value -->`: (1) name, (2) value.
pub(crate) static DIRECTIVE_RE: LazyLock<Regex> =
    LazyLock::new(|| re(r"^\s*<!--\s*(\w+)\s*(?::\s*(.*?))?\s*-->"));

/// `key: value` line in the front matter block: (1) key, (2) value.
pub(crate) static FRONT_MATTER_KV_RE: LazyLock<Regex> =
    LazyLock::new(|| re(r"^([\w-]+)\s*:\s*(.*)$"));

/// `{label: "name"}` in a fence info string: (1) name.
pub(crate) static FENCE_LABEL_RE: LazyLock<Regex> =
    LazyLock::new(|| re(r#"\{label:\s*"([^"]*)"\s*\}"#));

/// ATX heading: (1) `#` markers, (2) text.
pub(crate) static HEADING_RE: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(#{1,6})\s+(.*?)\s*$"));

/// `![alt](path)`: (1) alt, (2) path.
pub(crate) static IMAGE_RE: LazyLock<Regex> =
    LazyLock::new(|| re(r"^!\[([^\]]*)\]\(([^)]+)\)\s*$"));

/// `-`, `*`, `+` or `1.`/`1)` followed by whitespace: (1) indentation, (2) marker, (3) text.
/// The whitespace keeps `**bold**`, `*emphasis*` and `-5` out of lists.
pub(crate) static LIST_ITEM_RE: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(\s*)([-*+]|\d{1,9}[.)])(?:\s+(.*))?$"));

/// `***`, `---`, `___` (optionally spaced) horizontal rules, which slides do not draw.
pub(crate) static THEMATIC_BREAK_RE: LazyLock<Regex> =
    LazyLock::new(|| re(r"^ {0,3}(?:(?:\*[ \t]*){3,}|(?:-[ \t]*){3,}|(?:_[ \t]*){3,})$"));

pub(crate) static TABLE_ROW_RE: LazyLock<Regex> = LazyLock::new(|| re(r"^\|(.+)\|\s*$"));

/// `| :--- | :---: | ---: |`
pub(crate) static TABLE_SEP_RE: LazyLock<Regex> =
    LazyLock::new(|| re(r"^\|[\s:]*-+[\s:]*(\|[\s:]*-+[\s:]*)*\|\s*$"));

/// (1) quoted text.
pub(crate) static BLOCKQUOTE_RE: LazyLock<Regex> = LazyLock::new(|| re(r"^>\s?(.*)$"));
