//! Vertical flow diagram style, the narrowest: one node per line, joined by
//! `↓`, with each node's annotation aligned to the right of the labels.
//!
//! ```text
//!   Parse Input       serde_json
//!    ↓
//!   Validate Schema   jsonschema
//! ```

use crossterm::style::Color;
use unicode_width::UnicodeWidthStr;

use crate::diagram::parser::DiagramGraph;
use crate::render::text::{LineContentType, StyledLine, StyledSpan};

pub fn render(
    graph: &DiagramGraph,
    accent: Color,
    text_color: Color,
    dim_color: Color,
    pad: &str,
) -> Vec<StyledLine> {
    let line = |spans: Vec<StyledSpan>| StyledLine {
        spans: std::iter::once(StyledSpan::new(pad)).chain(spans).collect(),
        content_type: LineContentType::Diagram,
    };
    let steps: Vec<(&str, Option<&str>)> = graph
        .rows
        .iter()
        .flat_map(|row| {
            row.nodes.iter().enumerate().map(|(i, node)| {
                let note = row.annotations.get(i).and_then(|a| a.as_deref()).filter(|a| !a.is_empty());
                (node.label.as_str(), note)
            })
        })
        .collect();
    let label_w = steps.iter().map(|(l, _)| l.width()).max().unwrap_or(0);

    let mut lines = Vec::new();
    if let Some(title) = &graph.title {
        lines.push(line(vec![StyledSpan::new(title).with_fg(dim_color)]));
    }
    for (i, (label, note)) in steps.iter().enumerate() {
        if i > 0 {
            lines.push(line(vec![StyledSpan::new("   "), StyledSpan::new("↓").with_fg(accent)]));
        }
        let mut spans = vec![StyledSpan::new("  "), StyledSpan::new(label).with_fg(text_color).bold()];
        if let Some(note) = note {
            spans.push(StyledSpan::new(&" ".repeat(label_w - label.width() + 3)));
            spans.push(StyledSpan::new(note).with_fg(dim_color));
        }
        lines.push(line(spans));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagram::parser::parse;

    fn texts(source: &str) -> Vec<String> {
        render(&parse(source), Color::Cyan, Color::White, Color::Grey, "")
            .iter()
            .map(|l| l.spans.iter().map(|s| s.text.as_str()).collect::<String>().trim_end().to_string())
            .collect()
    }

    #[test]
    fn every_node_gets_its_own_line_with_its_annotation() {
        assert_eq!(
            texts("A -> Longer B\n: first : second\nC"),
            ["  A          first", "   ↓", "  Longer B   second", "   ↓", "  C"]
        );
    }
}
