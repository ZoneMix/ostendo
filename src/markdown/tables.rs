//! Pipe table rows: cell splitting and column alignment.

use crate::presentation::TableAlign;

/// A table under construction. Rows only count once the `| --- |` separator has been seen;
/// without it the lines are not a table and are discarded.
pub(crate) struct TableParseState {
    pub headers: Vec<String>,
    pub alignments: Vec<TableAlign>,
    pub rows: Vec<Vec<String>>,
    pub has_separator: bool,
}

/// Trimmed cells between the outer pipes. Empty cells are kept so later columns stay aligned.
pub(crate) fn parse_table_cells(row: &str) -> Vec<String> {
    let row = row.trim();
    let row = row.strip_prefix('|').unwrap_or(row);
    let row = row.strip_suffix('|').unwrap_or(row);
    row.split('|').map(|cell| cell.trim().to_string()).collect()
}

/// `:---` or `---` is left, `:---:` center, `---:` right.
pub(crate) fn parse_table_alignments(separator: &str) -> Vec<TableAlign> {
    parse_table_cells(separator)
        .iter()
        .map(|spec| match (spec.starts_with(':'), spec.ends_with(':')) {
            (true, true) => TableAlign::Center,
            (false, true) => TableAlign::Right,
            _ => TableAlign::Left,
        })
        .collect()
}
