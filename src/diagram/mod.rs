//! Adaptive ASCII diagram rendering.
//!
//! Supports box, bracket, and vertical layout styles with automatic fallback
//! for narrow terminals. When the requested style produces output wider than the
//! terminal, the renderer progressively tries more compact styles:
//! `Box` -> `Bracket` -> `Vertical`. If even `Vertical` overflows, node labels
//! are truncated with an ellipsis to fit.
//!
//! # Submodules
//! - [`parser`] — Parses the `diagram` code block DSL (`A -> B -> C`) into a [`DiagramGraph`].
//! - [`render_box`] — Box-style rendering with Unicode box-drawing characters (`+--+`).
//! - [`render_bracket`] — Bracket-style rendering with `[Node]` notation.
//! - [`render_vertical`] — Most compact style, one row per path with arrow separators.

pub mod parser;
pub mod render_box;
pub mod render_bracket;
pub mod render_vertical;

use crossterm::style::Color;

use crate::presentation::DiagramStyle;
use crate::render::text::StyledLine;
use parser::DiagramGraph;

/// Render a diagram with automatic style fallback when the output is too wide.
///
/// Tries the requested style first. If any rendered line exceeds `max_width`,
/// falls back to progressively more compact styles: Box → Bracket → Vertical.
/// If even Vertical overflows, truncates node labels so it fits.
pub fn render_adaptive(
    graph: &DiagramGraph,
    style: DiagramStyle,
    content_width: usize,
    max_width: usize,
    accent: Color,
    text_color: Color,
    dim_color: Color,
    pad: &str,
) -> Vec<StyledLine> {
    let fallback_chain: &[DiagramStyle] = match style {
        DiagramStyle::Box => &[DiagramStyle::Box, DiagramStyle::Bracket, DiagramStyle::Vertical],
        DiagramStyle::Bracket => &[DiagramStyle::Bracket, DiagramStyle::Vertical],
        DiagramStyle::Vertical => &[DiagramStyle::Vertical],
    };

    for &try_style in fallback_chain {
        let lines = render_with_style(graph, try_style, content_width, accent, text_color, dim_color, pad);
        let widest = lines.iter().map(|l| l.width()).max().unwrap_or(0);
        if widest <= max_width {
            return lines;
        }
    }

    // All styles overflow — truncate labels to fit in Vertical (most compact) style.
    let truncated = truncate_graph_labels(graph, max_width, pad);
    render_with_style(&truncated, DiagramStyle::Vertical, content_width, accent, text_color, dim_color, pad)
}

/// Render a diagram using a specific style (no fallback).
fn render_with_style(
    graph: &DiagramGraph,
    style: DiagramStyle,
    content_width: usize,
    accent: Color,
    text_color: Color,
    dim_color: Color,
    pad: &str,
) -> Vec<StyledLine> {
    match style {
        DiagramStyle::Box => render_box::render(graph, content_width, accent, text_color, dim_color, pad),
        DiagramStyle::Bracket => render_bracket::render(graph, content_width, accent, text_color, dim_color, pad),
        DiagramStyle::Vertical => render_vertical::render(graph, content_width, accent, text_color, dim_color, pad),
    }
}

/// Create a copy of the graph with node labels truncated so the widest
/// single-row horizontal layout fits within `max_width`.
///
/// The vertical renderer formats each row as:
///   `{pad}  {label1} → {label2} → ...`
/// We compute the overhead per row and divide remaining space among nodes.
fn truncate_graph_labels(graph: &DiagramGraph, max_width: usize, pad: &str) -> DiagramGraph {
    let pad_len = pad.len();
    // Vertical renderer indent: "  " (2 chars)
    let indent = 2;
    // Arrow: " → " (3 display chars)
    let arrow_width = 3;

    let mut new_rows = Vec::with_capacity(graph.rows.len());
    for row in &graph.rows {
        let n = row.nodes.len();
        if n == 0 {
            new_rows.push(row.clone());
            continue;
        }
        let overhead = pad_len + indent + (n.saturating_sub(1)) * arrow_width;
        let available = max_width.saturating_sub(overhead);
        let max_label = if n > 0 { available / n } else { available };
        // Minimum 3 chars so truncated labels are still readable (e.g. "Ab…")
        let max_label = max_label.max(3);

        let new_nodes: Vec<parser::DiagramNode> = row
            .nodes
            .iter()
            .map(|node| {
                let chars: Vec<char> = node.label.chars().collect();
                let label = if chars.len() > max_label {
                    let mut s: String = chars[..max_label.saturating_sub(1)].iter().collect();
                    s.push('…');
                    s
                } else {
                    node.label.clone()
                };
                parser::DiagramNode { label }
            })
            .collect();

        // Truncate annotations to match
        let new_annotations: Vec<Option<String>> = row
            .annotations
            .iter()
            .take(new_nodes.len())
            .map(|ann| {
                ann.as_ref().map(|text| {
                    let chars: Vec<char> = text.chars().collect();
                    if chars.len() > max_label {
                        let mut s: String = chars[..max_label.saturating_sub(1)].iter().collect();
                        s.push('…');
                        s
                    } else {
                        text.clone()
                    }
                })
            })
            .collect();

        new_rows.push(parser::DiagramRow {
            nodes: new_nodes,
            annotations: new_annotations,
        });
    }

    DiagramGraph {
        title: graph.title.clone(),
        rows: new_rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_colors() -> (Color, Color, Color) {
        (
            Color::Rgb { r: 189, g: 147, b: 249 },
            Color::Rgb { r: 248, g: 248, b: 242 },
            Color::Rgb { r: 98, g: 114, b: 164 },
        )
    }

    #[test]
    fn test_adaptive_fits_returns_requested_style() {
        let graph = parser::parse("A -> B");
        let (accent, text, dim) = test_colors();
        // Wide terminal — should keep Box style
        let lines = render_adaptive(&graph, DiagramStyle::Box, 80, 120, accent, text, dim, "  ");
        // Box style has "┌" characters
        let all_text: String = lines.iter().flat_map(|l| l.spans.iter()).map(|s| s.text.as_str()).collect();
        assert!(all_text.contains("┌"), "Expected box-style output");
    }

    #[test]
    fn test_adaptive_falls_back_to_bracket() {
        // Long labels that overflow at Box style but fit in Bracket
        let graph = parser::parse("Very Long Node Name -> Another Very Long Name -> Third Long One");
        let (accent, text, dim) = test_colors();
        // Box adds 4 chars of border per node + 4 char arrows — make terminal just tight enough
        let box_lines = render_with_style(&graph, DiagramStyle::Box, 60, accent, text, dim, "  ");
        let box_max = box_lines.iter().map(|l| l.width()).max().unwrap_or(0);
        // Use a max_width smaller than box but enough for bracket
        let bracket_lines = render_with_style(&graph, DiagramStyle::Bracket, 60, accent, text, dim, "  ");
        let bracket_max = bracket_lines.iter().map(|l| l.width()).max().unwrap_or(0);

        if box_max > bracket_max {
            // Set max_width between bracket_max and box_max — should fall back to bracket
            let lines = render_adaptive(&graph, DiagramStyle::Box, 60, bracket_max, accent, text, dim, "  ");
            let all_text: String = lines.iter().flat_map(|l| l.spans.iter()).map(|s| s.text.as_str()).collect();
            assert!(all_text.contains("["), "Expected bracket-style fallback");
            assert!(!all_text.contains("┌"), "Should not contain box borders");
        }
    }

    #[test]
    fn test_adaptive_truncates_labels_as_last_resort() {
        // Very long labels with narrow terminal
        let graph = parser::parse("Extremely Long Node Label Here -> Another Extremely Long Label");
        let (accent, text, dim) = test_colors();
        let lines = render_adaptive(&graph, DiagramStyle::Box, 30, 30, accent, text, dim, "");
        let widest = lines.iter().map(|l| l.width()).max().unwrap_or(0);
        // Should not exceed max_width after truncation
        assert!(widest <= 30, "Widest line {} exceeds max_width 30", widest);
    }

    #[test]
    fn test_truncate_preserves_short_labels() {
        let graph = parser::parse("A -> B -> C");
        let truncated = truncate_graph_labels(&graph, 80, "  ");
        assert_eq!(truncated.rows[0].nodes[0].label, "A");
        assert_eq!(truncated.rows[0].nodes[1].label, "B");
        assert_eq!(truncated.rows[0].nodes[2].label, "C");
    }

    #[test]
    fn test_truncate_clips_long_labels() {
        let graph = parser::parse("VeryLongLabel -> AnotherLongOne");
        // pad="  " (2), indent=2, arrow=3, 2 nodes => overhead=7, available=13, max_label=6
        let truncated = truncate_graph_labels(&graph, 20, "  ");
        for node in &truncated.rows[0].nodes {
            assert!(node.label.chars().count() <= 6, "Label '{}' exceeds max", node.label);
        }
    }
}
