/// Vertical flow renderer.
///
/// Produces top-to-bottom pipe diagrams:
/// ```text
///   Node A
///     │
///     ▼
///   Node B
///     annotation
///     │
///     ▼
///   Node C
/// ```
use crossterm::style::Color;

use crate::diagram::parser::DiagramGraph;
use crate::render::text::{LineContentType, StyledLine, StyledSpan};

/// Indentation for node labels.
const INDENT: &str = "  ";

/// Render a `DiagramGraph` as vertical top-to-bottom flow.
///
/// In vertical mode, each row's nodes are laid out horizontally with ` → ` arrows,
/// and rows are connected vertically with `│` and `▼`.
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
        line.push(StyledSpan::new(title).with_fg(dim_color));
        lines.push(line);
    }

    for (row_idx, row) in graph.rows.iter().enumerate() {
        // Node line: each row's nodes connected with →
        let mut node_line = StyledLine::empty();
        node_line.content_type = LineContentType::Diagram;
        node_line.push(StyledSpan::new(pad));
        node_line.push(StyledSpan::new(INDENT));

        for (i, node) in row.nodes.iter().enumerate() {
            node_line.push(StyledSpan::new(&node.label).with_fg(text_color).bold());
            if i + 1 < row.nodes.len() {
                node_line.push(StyledSpan::new(" → ").with_fg(accent));
            }
        }
        lines.push(node_line);

        // Annotations (if any non-None)
        let has_annotations = row.annotations.iter().any(|a| a.is_some());
        if has_annotations {
            for ann in &row.annotations {
                if let Some(text) = ann.as_deref() {
                    if !text.is_empty() {
                        let mut ann_line = StyledLine::empty();
                        ann_line.content_type = LineContentType::Diagram;
                        ann_line.push(StyledSpan::new(pad));
                        ann_line.push(StyledSpan::new(INDENT));
                        ann_line.push(StyledSpan::new("  ")); // extra indent under node
                        ann_line.push(StyledSpan::new(text).with_fg(dim_color));
                        lines.push(ann_line);
                    }
                }
            }
        }

        // Vertical connector to next row
        if row_idx + 1 < graph.rows.len() {
            let connector_pad = format!("{}{}", pad, INDENT);
            let connector_x = 2; // center under short labels

            let mut pipe = StyledLine::empty();
            pipe.content_type = LineContentType::Diagram;
            pipe.push(StyledSpan::new(&connector_pad));
            pipe.push(StyledSpan::new(&" ".repeat(connector_x)));
            pipe.push(StyledSpan::new("│").with_fg(accent));
            lines.push(pipe);

            let mut arrow = StyledLine::empty();
            arrow.content_type = LineContentType::Diagram;
            arrow.push(StyledSpan::new(&connector_pad));
            arrow.push(StyledSpan::new(&" ".repeat(connector_x)));
            arrow.push(StyledSpan::new("▼").with_fg(accent));
            lines.push(arrow);
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
    fn test_vertical_simple() {
        let (accent, text, dim) = test_colors();
        let graph = parse("A -> B\nB -> C");
        let lines = render(&graph, 80, accent, text, dim, "  ");
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.text.as_str())
            .collect();
        assert!(all_text.contains("A"));
        assert!(all_text.contains("B"));
        assert!(all_text.contains("C"));
        assert!(all_text.contains("│"));
        assert!(all_text.contains("▼"));
    }

    #[test]
    fn test_vertical_with_annotations() {
        let (accent, text, dim) = test_colors();
        let graph = parse("Step 1\n: details here\nStep 2");
        let lines = render(&graph, 80, accent, text, dim, "  ");
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.text.as_str())
            .collect();
        assert!(all_text.contains("details here"));
    }

    #[test]
    fn test_vertical_single_row() {
        let (accent, text, dim) = test_colors();
        let graph = parse("Just One");
        let lines = render(&graph, 80, accent, text, dim, "  ");
        // Single row, no connectors
        let has_connector = lines
            .iter()
            .any(|l| l.spans.iter().any(|s| s.text.contains("│")));
        assert!(!has_connector);
    }
}
