//! Markdown table parsing helpers.
//!
//! Contains the temporary state machine and cell/alignment parsers used while scanning
//! pipe-delimited table rows inside a slide block.

use crate::presentation::TableAlign;

/// Temporary state accumulated while parsing a Markdown table.
///
/// A valid Markdown table consists of:
/// 1. A header row: `| Col A | Col B |`
/// 2. A separator row: `| --- | :---: |`  (sets alignment; must appear to finalize the table)
/// 3. Zero or more data rows: `| val1 | val2 |`
///
/// The parser collects lines into this struct. Only once `has_separator` is true are subsequent
/// rows treated as data rows. If the separator never appears, the "table" is discarded.
pub(crate) struct TableParseState {
    /// Column header labels from the first row.
    pub headers: Vec<String>,
    /// Per-column alignment (left, center, right) inferred from the separator row's colons.
    pub alignments: Vec<TableAlign>,
    /// Data rows collected after the separator.
    pub rows: Vec<Vec<String>>,
    /// Whether the mandatory separator row (`| --- | --- |`) has been seen.
    pub has_separator: bool,
}

/// Splits a pipe-delimited table row into individual cell strings.
///
/// Leading and trailing pipes produce empty strings which are filtered out, so
/// `"| A | B |"` yields `["A", "B"]`.
///
/// # Parameters
/// - `row`: A single line of a Markdown table, e.g. `"| Name | Value |"`.
///
/// # Returns
/// A `Vec<String>` of trimmed cell values.
pub(crate) fn parse_table_cells(row: &str) -> Vec<String> {
    row.split('|')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Determines per-column alignment from a Markdown table separator row.
///
/// The separator row uses colons to indicate alignment:
/// - `:---` or `---` = left-aligned (default)
/// - `:---:` = center-aligned
/// - `---:` = right-aligned
///
/// # Parameters
/// - `sep_row`: The separator line, e.g. `"| :--- | :---: | ---: |"`.
///
/// # Returns
/// A `Vec<TableAlign>` with one entry per column.
pub(crate) fn parse_table_alignments(sep_row: &str) -> Vec<TableAlign> {
    sep_row
        .split('|')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| {
            let left = s.starts_with(':');
            let right = s.ends_with(':');
            match (left, right) {
                (true, true) => TableAlign::Center,
                (false, true) => TableAlign::Right,
                _ => TableAlign::Left,
            }
        })
        .collect()
}
