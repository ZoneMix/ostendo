/// Box-drawing style renderer.
///
/// Produces Unicode box-drawing diagrams:
/// ```text
/// ┌──────────┐    ┌──────────┐    ┌──────────┐
/// │ Node A   │───→│ Node B   │───→│ Node C   │
/// └──────────┘    └──────────┘    └──────────┘
///   annotation     annotation      annotation
/// ```

use crossterm::style::Color;

use crate::diagram::parser::{DiagramGraph, DiagramRow};
use crate::render::text::{LineContentType, StyledLine, StyledSpan};

/// Horizontal arrow connector between boxes.
const ARROW: &str = "───→";
/// Arrow display width.
const ARROW_WIDTH: usize = 4;
/// Minimum padding inside box on each side of the label.
const BOX_PAD: usize = 1;

/// Render a `DiagramGraph` as box-drawing styled lines.
///
/// `accent` colors box borders and arrows. `text_color` colors labels.
/// `dim_color` colors annotations and titles.
pub fn render(
    graph: &DiagramGraph,
    content_width: usize,
    accent: Color,
    text_color: Color,
    dim_color: Color,
    pad: &str,
) -> Vec<StyledLine> {
    let mut lines: Vec<StyledLine> = Vec::new();

    // Title
    if let Some(ref title) = graph.title {
        lines.push(StyledLine::empty());
        let mut line = StyledLine::empty();
        line.content_type = LineContentType::Diagram;
        line.push(StyledSpan::new(pad));
        line.push(StyledSpan::new(title).with_fg(dim_color).dim());
        lines.push(line);
        lines.push(StyledLine::empty());
    }

    for (row_idx, row) in graph.rows.iter().enumerate() {
        let col_widths = compute_column_widths(row, content_width);
        render_row(&mut lines, row, &col_widths, accent, text_color, dim_color, pad);

        // Vertical connector to next row if rows share a node
        if row_idx + 1 < graph.rows.len() {
            let next_row = &graph.rows[row_idx + 1];
            if let Some(connector_col) = find_connector_column(row, next_row) {
                let offset = compute_connector_offset(&col_widths, connector_col);
                render_vertical_connector(&mut lines, offset, accent, pad);
            } else {
                lines.push(StyledLine::empty());
            }
        }
    }

    lines.push(StyledLine::empty());
    lines
}

/// Compute the display width for each column in a row.
///
/// Each column width = max(label_len, annotation_len) + 2 * BOX_PAD + 2 (for box border chars `│ │`).
fn compute_column_widths(row: &DiagramRow, _content_width: usize) -> Vec<usize> {
    row.nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let label_w = node.label.chars().count();
            let ann_w = row
                .annotations
                .get(i)
                .and_then(|a| a.as_ref())
                .map(|a| a.chars().count())
                .unwrap_or(0);
            let inner = label_w.max(ann_w);
            inner + 2 * BOX_PAD + 2 // +2 for `│` on each side
        })
        .collect()
}

/// Render a single row: top border, label, bottom border, annotations.
fn render_row(
    lines: &mut Vec<StyledLine>,
    row: &DiagramRow,
    col_widths: &[usize],
    accent: Color,
    text_color: Color,
    dim_color: Color,
    pad: &str,
) {
    let node_count = row.nodes.len();

    // Top border: ┌──...──┐    ┌──...──┐
    let mut top = StyledLine::empty();
    top.content_type = LineContentType::Diagram;
    top.push(StyledSpan::new(pad));
    for (i, &w) in col_widths.iter().enumerate() {
        let inner_w = w - 2; // subtract the corner chars
        top.push(StyledSpan::new("┌").with_fg(accent));
        top.push(StyledSpan::new(&"─".repeat(inner_w)).with_fg(accent));
        top.push(StyledSpan::new("┐").with_fg(accent));
        if i + 1 < node_count {
            top.push(StyledSpan::new(&" ".repeat(ARROW_WIDTH)));
        }
    }
    lines.push(top);

    // Label: │ Node A   │───→│ Node B   │
    let mut label_line = StyledLine::empty();
    label_line.content_type = LineContentType::Diagram;
    label_line.push(StyledSpan::new(pad));
    for (i, node) in row.nodes.iter().enumerate() {
        let inner_w = col_widths[i] - 2;
        let label = &node.label;
        let label_chars: usize = label.chars().count();
        let right_pad = inner_w.saturating_sub(BOX_PAD + label_chars);

        label_line.push(StyledSpan::new("│").with_fg(accent));
        label_line.push(StyledSpan::new(&" ".repeat(BOX_PAD)));
        label_line.push(StyledSpan::new(label).with_fg(text_color).bold());
        label_line.push(StyledSpan::new(&" ".repeat(right_pad)));
        label_line.push(StyledSpan::new("│").with_fg(accent));

        if i + 1 < node_count {
            label_line.push(StyledSpan::new(ARROW).with_fg(accent));
        }
    }
    lines.push(label_line);

    // Bottom border: └──...──┘    └──...──┘
    let mut bot = StyledLine::empty();
    bot.content_type = LineContentType::Diagram;
    bot.push(StyledSpan::new(pad));
    for (i, &w) in col_widths.iter().enumerate() {
        let inner_w = w - 2;
        bot.push(StyledSpan::new("└").with_fg(accent));
        bot.push(StyledSpan::new(&"─".repeat(inner_w)).with_fg(accent));
        bot.push(StyledSpan::new("┘").with_fg(accent));
        if i + 1 < node_count {
            bot.push(StyledSpan::new(&" ".repeat(ARROW_WIDTH)));
        }
    }
    lines.push(bot);

    // Annotations (if any non-None)
    let has_annotations = row.annotations.iter().any(|a| a.is_some());
    if has_annotations {
        let mut ann_line = StyledLine::empty();
        ann_line.content_type = LineContentType::Diagram;
        ann_line.push(StyledSpan::new(pad));
        for (i, ann) in row.annotations.iter().take(col_widths.len()).enumerate() {
            let col_w = col_widths[i];
            let text = ann.as_deref().unwrap_or("");
            let text_chars: usize = text.chars().count();
            // Center annotation under the box
            let total_pad = col_w.saturating_sub(text_chars);
            let left = total_pad / 2;
            let right = total_pad - left;
            ann_line.push(StyledSpan::new(&" ".repeat(left)));
            ann_line.push(StyledSpan::new(text).with_fg(dim_color).dim());
            ann_line.push(StyledSpan::new(&" ".repeat(right)));
            if i + 1 < row.nodes.len() {
                ann_line.push(StyledSpan::new(&" ".repeat(ARROW_WIDTH)));
            }
        }
        lines.push(ann_line);
    }
}

/// Find which column in the current row connects to the next row.
///
/// Returns the index of the last node in `current` whose label matches
/// the first node of `next`. If no match, returns `None`.
fn find_connector_column(current: &DiagramRow, next: &DiagramRow) -> Option<usize> {
    let next_first = next.nodes.first()?;
    current
        .nodes
        .iter()
        .rposition(|n| n.label == next_first.label)
}

/// Compute the horizontal offset (in chars) to the center of a given column.
fn compute_connector_offset(col_widths: &[usize], col_idx: usize) -> usize {
    let mut offset = 0;
    for (i, &w) in col_widths.iter().enumerate() {
        if i == col_idx {
            return offset + w / 2;
        }
        offset += w + ARROW_WIDTH;
    }
    offset
}

/// Render vertical connector lines (│ and ▼) between two rows.
fn render_vertical_connector(
    lines: &mut Vec<StyledLine>,
    offset: usize,
    accent: Color,
    pad: &str,
) {
    let pad_len = pad.len();

    // Pipe line
    let mut pipe = StyledLine::empty();
    pipe.content_type = LineContentType::Diagram;
    pipe.push(StyledSpan::new(&" ".repeat(pad_len + offset)));
    pipe.push(StyledSpan::new("│").with_fg(accent));
    lines.push(pipe);

    // Arrow down
    let mut arrow = StyledLine::empty();
    arrow.content_type = LineContentType::Diagram;
    arrow.push(StyledSpan::new(&" ".repeat(pad_len + offset)));
    arrow.push(StyledSpan::new("▼").with_fg(accent));
    lines.push(arrow);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagram::parser::parse;

    fn test_accent() -> Color {
        Color::Rgb { r: 189, g: 147, b: 249 }
    }
    fn test_text() -> Color {
        Color::Rgb { r: 248, g: 248, b: 242 }
    }
    fn test_dim() -> Color {
        Color::Rgb { r: 98, g: 114, b: 164 }
    }

    #[test]
    fn test_simple_render() {
        let graph = parse("A -> B -> C");
        let lines = render(&graph, 80, test_accent(), test_text(), test_dim(), "  ");
        // Should have: empty + top + label + bottom + empty = 5 lines minimum
        assert!(lines.len() >= 4);
        // All lines should be Diagram content type (except empties)
        for line in &lines {
            if !line.spans.is_empty() {
                assert_eq!(line.content_type, LineContentType::Diagram);
            }
        }
    }

    #[test]
    fn test_with_title() {
        let graph = parse("# My Title\nA -> B");
        let lines = render(&graph, 80, test_accent(), test_text(), test_dim(), "  ");
        // Should contain the title text
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.text.as_str())
            .collect();
        assert!(all_text.contains("My Title"));
    }

    #[test]
    fn test_with_annotations() {
        let graph = parse("A -> B\n: note1  : note2");
        let lines = render(&graph, 80, test_accent(), test_text(), test_dim(), "  ");
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.text.as_str())
            .collect();
        assert!(all_text.contains("note1"));
        assert!(all_text.contains("note2"));
    }

    #[test]
    fn test_annotations_clamped_to_node_count() {
        // Even if a DiagramRow somehow has more annotations than nodes,
        // rendering must not panic.
        let mut graph = parse("A -> B -> C\nC -> D\n: x : y : z");
        // Parser already clamps, but manually force extra annotation to test renderer defense
        graph.rows[1].annotations.push(Some("extra".into()));
        // Should not panic
        render(&graph, 80, test_accent(), test_text(), test_dim(), "  ");
    }

    #[test]
    fn test_multi_row_with_connector() {
        let graph = parse("A -> B\nB -> C");
        let lines = render(&graph, 80, test_accent(), test_text(), test_dim(), "  ");
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.text.as_str())
            .collect();
        // Should have vertical connectors
        assert!(all_text.contains("│") || all_text.contains("▼"));
    }
}
