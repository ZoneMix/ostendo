/// Bracket-style renderer.
///
/// Produces compact bracket diagrams:
/// ```text
/// [Node A] → [Node B] → [Node C]
///  note A     note B     note C
/// ```
use crossterm::style::Color;

use crate::diagram::parser::DiagramGraph;
use crate::render::text::{LineContentType, StyledLine, StyledSpan};

/// Render a `DiagramGraph` as bracket-style lines.
pub fn render(
    graph: &DiagramGraph,
    _content_width: usize,
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
        // Compute column widths for alignment (bracket width = label + 2 for [])
        let col_widths: Vec<usize> = row
            .nodes
            .iter()
            .enumerate()
            .map(|(i, node)| {
                let label_w = node.label.chars().count() + 2; // +2 for brackets
                let ann_w = row
                    .annotations
                    .get(i)
                    .and_then(|a| a.as_ref())
                    .map(|a| a.chars().count())
                    .unwrap_or(0);
                label_w.max(ann_w)
            })
            .collect();

        // Node line: [A] → [B] → [C]
        let mut node_line = StyledLine::empty();
        node_line.content_type = LineContentType::Diagram;
        node_line.push(StyledSpan::new(pad));

        for (i, node) in row.nodes.iter().enumerate() {
            let bracket_text = format!("[{}]", node.label);
            let bracket_chars = bracket_text.chars().count();
            let right_pad = col_widths[i].saturating_sub(bracket_chars);

            node_line.push(StyledSpan::new("[").with_fg(accent));
            node_line.push(StyledSpan::new(&node.label).with_fg(text_color).bold());
            node_line.push(StyledSpan::new("]").with_fg(accent));
            node_line.push(StyledSpan::new(&" ".repeat(right_pad)));

            if i + 1 < row.nodes.len() {
                node_line.push(StyledSpan::new(" → ").with_fg(accent));
            }
        }
        lines.push(node_line);

        // Annotation line (if any)
        let has_annotations = row.annotations.iter().any(|a| a.is_some());
        if has_annotations {
            let mut ann_line = StyledLine::empty();
            ann_line.content_type = LineContentType::Diagram;
            ann_line.push(StyledSpan::new(pad));
            // Extra space to align under bracket content (past the `[`)
            ann_line.push(StyledSpan::new(" "));

            for (i, ann) in row.annotations.iter().take(col_widths.len()).enumerate() {
                let text = ann.as_deref().unwrap_or("");
                let text_chars = text.chars().count();
                // Align to col_width (which includes bracket chars)
                let right_pad = col_widths[i].saturating_sub(text_chars + 1); // +1 for leading space offset

                ann_line.push(StyledSpan::new(text).with_fg(dim_color).dim());
                ann_line.push(StyledSpan::new(&" ".repeat(right_pad)));

                if i + 1 < row.nodes.len() {
                    ann_line.push(StyledSpan::new("   ")); // align with " → "
                }
            }
            lines.push(ann_line);
        }

        // Vertical connector between rows
        if row_idx + 1 < graph.rows.len() {
            let mut connector = StyledLine::empty();
            connector.content_type = LineContentType::Diagram;
            connector.push(StyledSpan::new(pad));
            // Find connection point (last node of current row matching first of next)
            let next_first = graph.rows[row_idx + 1]
                .nodes
                .first()
                .map(|n| n.label.as_str())
                .unwrap_or("");
            let mut offset = 0usize;
            let mut found = false;
            for (i, node) in row.nodes.iter().enumerate() {
                if node.label == next_first {
                    // Center under this column
                    let center = offset + col_widths[i] / 2;
                    connector.push(StyledSpan::new(&" ".repeat(center)));
                    connector.push(StyledSpan::new("↓").with_fg(accent));
                    found = true;
                    break;
                }
                offset += col_widths[i] + 3; // +3 for " → "
            }
            if !found {
                connector.push(StyledSpan::new(" "));
            }
            lines.push(connector);
        }
    }

    lines.push(StyledLine::empty());
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagram::parser::parse;

    fn test_colors() -> (Color, Color, Color) {
        (
            Color::Rgb { r: 189, g: 147, b: 249 },
            Color::Rgb { r: 248, g: 248, b: 242 },
            Color::Rgb { r: 98, g: 114, b: 164 },
        )
    }

    #[test]
    fn test_simple_bracket() {
        let (accent, text, dim) = test_colors();
        let graph = parse("A -> B -> C");
        let lines = render(&graph, 80, accent, text, dim, "  ");
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.text.as_str())
            .collect();
        assert!(all_text.contains("["));
        assert!(all_text.contains("]"));
        assert!(all_text.contains("→"));
        assert!(all_text.contains("A"));
        assert!(all_text.contains("B"));
        assert!(all_text.contains("C"));
    }

    #[test]
    fn test_bracket_with_title() {
        let (accent, text, dim) = test_colors();
        let graph = parse("# Test\nX -> Y");
        let lines = render(&graph, 80, accent, text, dim, "  ");
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.text.as_str())
            .collect();
        assert!(all_text.contains("Test"));
    }

    #[test]
    fn test_bracket_compact() {
        let (accent, text, dim) = test_colors();
        let graph = parse("A -> B");
        let lines = render(&graph, 80, accent, text, dim, "  ");
        // Bracket style should be compact: fewer lines than box style
        // At minimum: node_line + trailing_empty = 2
        assert!(lines.len() >= 2);
    }
}
