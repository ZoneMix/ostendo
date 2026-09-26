//! Adaptive ASCII diagrams for ```` ```diagram ```` blocks.
//!
//! [`parser`] turns the `A -> B -> C` DSL into a [`DiagramGraph`]; the box,
//! bracket and vertical renderers draw it. When the requested style is wider
//! than the terminal, [`render_adaptive`] first shortens labels in the same
//! style, then falls back `Box` -> `Bracket` -> `Vertical`, and finally
//! truncates labels in `Vertical`.

pub mod parser;
pub mod render_box;
pub mod render_bracket;
pub mod render_vertical;

use crossterm::style::Color;

use crate::presentation::DiagramStyle;
use crate::render::text::StyledLine;
use parser::DiagramGraph;

/// Render `graph` no wider than `max_width` columns, degrading the style and
/// truncating labels as needed (see the module docs).
pub fn render_adaptive(
    graph: &DiagramGraph,
    style: DiagramStyle,
    max_width: usize,
    accent: Color,
    text_color: Color,
    dim_color: Color,
    pad: &str,
) -> Vec<StyledLine> {
    let render = |graph: &DiagramGraph, style| match style {
        DiagramStyle::Box => render_box::render(graph, accent, text_color, dim_color, pad),
        DiagramStyle::Bracket => render_bracket::render(graph, accent, text_color, dim_color, pad),
        DiagramStyle::Vertical => {
            render_vertical::render(graph, accent, text_color, dim_color, pad)
        }
    };
    let fits = |lines: &[StyledLine]| lines.iter().all(|l| l.width() <= max_width);

    let fallback_chain: &[DiagramStyle] = match style {
        DiagramStyle::Box => &[
            DiagramStyle::Box,
            DiagramStyle::Bracket,
            DiagramStyle::Vertical,
        ],
        DiagramStyle::Bracket => &[DiagramStyle::Bracket, DiagramStyle::Vertical],
        DiagramStyle::Vertical => &[DiagramStyle::Vertical],
    };
    for &try_style in fallback_chain {
        let lines = render(graph, try_style);
        if fits(&lines) {
            return lines;
        }
        // Shorter labels in the same style beat switching to a plainer style.
        let truncated = truncate_graph_labels_for_style(graph, max_width, pad, try_style);
        let lines = render(&truncated, try_style);
        if fits(&lines) {
            return lines;
        }
    }

    let truncated = truncate_graph_labels_for_style(graph, max_width, pad, DiagramStyle::Vertical);
    render(&truncated, DiagramStyle::Vertical)
}

/// Copy of `graph` whose labels and annotations are cut (with `…`) so each
/// row's single-line layout fits `max_width` in `style`, never below 3 chars.
fn truncate_graph_labels_for_style(
    graph: &DiagramGraph,
    max_width: usize,
    pad: &str,
    style: DiagramStyle,
) -> DiagramGraph {
    // Per node, per arrow, and leading indent, as drawn by each renderer.
    let (per_node_overhead, arrow_width, indent) = match style {
        DiagramStyle::Box => (4usize, 4usize, 2usize),
        DiagramStyle::Bracket => (2, 3, 0),
        DiagramStyle::Vertical => (0, 3, 2),
    };

    let rows = graph
        .rows
        .iter()
        .map(|row| {
            let n = row.nodes.len();
            if n == 0 {
                return row.clone();
            }
            let overhead = pad.len() + indent + n * per_node_overhead + (n - 1) * arrow_width;
            let max_label = (max_width.saturating_sub(overhead) / n).max(3);
            parser::DiagramRow {
                nodes: row
                    .nodes
                    .iter()
                    .map(|node| parser::DiagramNode {
                        label: truncate(&node.label, max_label),
                    })
                    .collect(),
                annotations: row
                    .annotations
                    .iter()
                    .take(n)
                    .map(|ann| ann.as_deref().map(|text| truncate(text, max_label)))
                    .collect(),
            }
        })
        .collect();

    DiagramGraph {
        title: graph.title.clone(),
        rows,
    }
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_colors() -> (Color, Color, Color) {
        (
            Color::Rgb {
                r: 189,
                g: 147,
                b: 249,
            },
            Color::Rgb {
                r: 248,
                g: 248,
                b: 242,
            },
            Color::Rgb {
                r: 98,
                g: 114,
                b: 164,
            },
        )
    }

    fn text_of(lines: &[StyledLine]) -> String {
        lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.text.as_str())
            .collect()
    }

    fn widest(lines: &[StyledLine]) -> usize {
        lines.iter().map(|l| l.width()).max().unwrap_or(0)
    }

    #[test]
    fn test_adaptive_fits_returns_requested_style() {
        let graph = parser::parse("A -> B");
        let (accent, text, dim) = test_colors();
        let lines = render_adaptive(&graph, DiagramStyle::Box, 120, accent, text, dim, "  ");
        assert!(text_of(&lines).contains('┌'), "Expected box-style output");
    }

    #[test]
    fn test_adaptive_truncates_box_before_bracket() {
        let graph =
            parser::parse("Very Long Node Name -> Another Very Long Name -> Third Long One");
        let (accent, text, dim) = test_colors();
        let full_box = render_box::render(&graph, accent, text, dim, "  ");
        assert!(
            widest(&full_box) > 70,
            "fixture must overflow as a full box"
        );

        let lines = render_adaptive(&graph, DiagramStyle::Box, 70, accent, text, dim, "  ");
        let all_text = text_of(&lines);
        assert!(all_text.contains('┌'), "fell back from Box: {all_text}");
        assert!(all_text.contains('…'), "labels were not truncated");
        assert!(widest(&lines) <= 70);
    }

    #[test]
    fn test_adaptive_truncates_labels_as_last_resort() {
        let graph = parser::parse("Extremely Long Node Label Here -> Another Extremely Long Label");
        let (accent, text, dim) = test_colors();
        let lines = render_adaptive(&graph, DiagramStyle::Box, 30, accent, text, dim, "");
        assert!(
            widest(&lines) <= 30,
            "Widest line {} exceeds 30",
            widest(&lines)
        );
    }

    #[test]
    fn test_truncate_preserves_short_labels() {
        let graph = parser::parse("A -> B -> C");
        let truncated = truncate_graph_labels_for_style(&graph, 80, "  ", DiagramStyle::Vertical);
        assert_eq!(truncated.rows[0].nodes[0].label, "A");
        assert_eq!(truncated.rows[0].nodes[1].label, "B");
        assert_eq!(truncated.rows[0].nodes[2].label, "C");
    }

    #[test]
    fn test_truncate_clips_long_labels() {
        let graph = parser::parse("VeryLongLabel -> AnotherLongOne");
        // pad="  " (2), indent=2, arrow=3, 2 nodes => overhead=7, available=13, max_label=6
        let truncated = truncate_graph_labels_for_style(&graph, 20, "  ", DiagramStyle::Vertical);
        for node in &truncated.rows[0].nodes {
            assert!(
                node.label.chars().count() <= 6,
                "Label '{}' exceeds max",
                node.label
            );
        }
    }
}
