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

pub(crate) static TITLE_RE: LazyLock<Regex> = LazyLock::new(|| re(r"^#\s+(.+)$"));

/// `![alt](path)`: (1) alt, (2) path.
pub(crate) static IMAGE_RE: LazyLock<Regex> =
    LazyLock::new(|| re(r"^!\[([^\]]*)\]\(([^)]+)\)\s*$"));

/// (1) indentation, (2) item text.
pub(crate) static BULLET_RE: LazyLock<Regex> = LazyLock::new(|| re(r"^(\s*)[-*]\s*(.*)$"));

pub(crate) static TABLE_ROW_RE: LazyLock<Regex> = LazyLock::new(|| re(r"^\|(.+)\|\s*$"));

/// `| :--- | :---: | ---: |`
pub(crate) static TABLE_SEP_RE: LazyLock<Regex> =
    LazyLock::new(|| re(r"^\|[\s:]*-+[\s:]*(\|[\s:]*-+[\s:]*)*\|\s*$"));

/// (1) quoted text.
pub(crate) static BLOCKQUOTE_RE: LazyLock<Regex> = LazyLock::new(|| re(r"^>\s?(.*)$"));
