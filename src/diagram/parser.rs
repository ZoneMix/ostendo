/// Diagram DSL parser.
///
/// Syntax:
///   # Optional title
///   Node A -> Node B -> Node C
///   : annotation A  : annotation B  : annotation C
///
/// Lines starting with `#` are titles (rendered dimmed above the diagram).
/// Lines starting with `:` are annotations (rendered dimmed below the previous row's nodes).
/// All other non-empty lines are node rows, split on ` -> `.

#[derive(Debug, Clone)]
pub struct DiagramGraph {
    pub title: Option<String>,
    pub rows: Vec<DiagramRow>,
}

#[derive(Debug, Clone)]
pub struct DiagramRow {
    pub nodes: Vec<DiagramNode>,
    pub annotations: Vec<Option<String>>,
}

#[derive(Debug, Clone)]
pub struct DiagramNode {
    pub label: String,
}

/// Parse diagram DSL source text into a `DiagramGraph`.
pub fn parse(source: &str) -> DiagramGraph {
    let mut title: Option<String> = None;
    let mut rows: Vec<DiagramRow> = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Title line
        if trimmed.starts_with('#') {
            title = Some(trimmed.trim_start_matches('#').trim().to_string());
            continue;
        }

        // Annotation line — applies to the most recent row
        if trimmed.starts_with(':') {
            if let Some(last_row) = rows.last_mut() {
                let annotations = parse_annotations(trimmed);
                // Merge: extend or replace
                last_row.annotations = annotations;
                // Clamp annotations to match node count
                last_row.annotations.truncate(last_row.nodes.len());
                while last_row.annotations.len() < last_row.nodes.len() {
                    last_row.annotations.push(None);
                }
            }
            continue;
        }

        // Node row: split on ` -> `
        let nodes: Vec<DiagramNode> = trimmed
            .split("->")
            .map(|s| DiagramNode {
                label: s.trim().to_string(),
            })
            .filter(|n| !n.label.is_empty())
            .collect();

        if !nodes.is_empty() {
            let node_count = nodes.len();
            rows.push(DiagramRow {
                nodes,
                annotations: vec![None; node_count],
            });
        }
    }

    DiagramGraph { title, rows }
}

/// Parse a colon-separated annotation line.
///
/// Format: `: annotation 1  : annotation 2  : annotation 3`
///
/// Each segment is separated by `:` at the start or preceded by whitespace.
fn parse_annotations(line: &str) -> Vec<Option<String>> {
    // Split on `: ` pattern (colon followed by space), treating leading `:` as first delimiter
    let stripped = line.trim_start_matches(':');
    stripped
        .split(':')
        .map(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_row() {
        let graph = parse("A -> B -> C");
        assert_eq!(graph.rows.len(), 1);
        assert_eq!(graph.rows[0].nodes.len(), 3);
        assert_eq!(graph.rows[0].nodes[0].label, "A");
        assert_eq!(graph.rows[0].nodes[1].label, "B");
        assert_eq!(graph.rows[0].nodes[2].label, "C");
    }

    #[test]
    fn test_title() {
        let graph = parse("# Attack Chain\nA -> B");
        assert_eq!(graph.title.as_deref(), Some("Attack Chain"));
        assert_eq!(graph.rows.len(), 1);
    }

    #[test]
    fn test_annotations() {
        let graph = parse("A -> B -> C\n: first  : second  : third");
        assert_eq!(graph.rows[0].annotations[0].as_deref(), Some("first"));
        assert_eq!(graph.rows[0].annotations[1].as_deref(), Some("second"));
        assert_eq!(graph.rows[0].annotations[2].as_deref(), Some("third"));
    }

    #[test]
    fn test_multiple_rows() {
        let graph = parse("A -> B\nB -> C -> D");
        assert_eq!(graph.rows.len(), 2);
        assert_eq!(graph.rows[0].nodes.len(), 2);
        assert_eq!(graph.rows[1].nodes.len(), 3);
    }

    #[test]
    fn test_empty_input() {
        let graph = parse("");
        assert!(graph.rows.is_empty());
        assert!(graph.title.is_none());
    }

    #[test]
    fn test_single_node() {
        let graph = parse("Alone");
        assert_eq!(graph.rows.len(), 1);
        assert_eq!(graph.rows[0].nodes.len(), 1);
        assert_eq!(graph.rows[0].nodes[0].label, "Alone");
    }

    #[test]
    fn test_annotations_fewer_than_nodes() {
        let graph = parse("A -> B -> C\n: only first");
        assert_eq!(graph.rows[0].annotations.len(), 3);
        assert_eq!(graph.rows[0].annotations[0].as_deref(), Some("only first"));
        assert!(graph.rows[0].annotations[1].is_none());
        assert!(graph.rows[0].annotations[2].is_none());
    }

    #[test]
    fn test_annotations_more_than_nodes() {
        // Annotation line has 3 entries but the row only has 2 nodes.
        // Excess annotations should be truncated, not panic renderers.
        let graph = parse("A -> B\n: one : two : three");
        assert_eq!(graph.rows[0].nodes.len(), 2);
        assert_eq!(graph.rows[0].annotations.len(), 2);
        assert_eq!(graph.rows[0].annotations[0].as_deref(), Some("one"));
        assert_eq!(graph.rows[0].annotations[1].as_deref(), Some("two"));
    }

    #[test]
    fn test_annotations_on_multirow_last_row_shorter() {
        // Reproduces the slide 7 panic: row 1 has 3 nodes, row 2 has 2 nodes,
        // annotation line with 3 entries gets attached to row 2.
        let graph = parse("A -> B -> C\nC -> D\n: x : y : z");
        assert_eq!(graph.rows[1].nodes.len(), 2);
        assert_eq!(graph.rows[1].annotations.len(), 2);
    }

    #[test]
    fn test_blank_lines_ignored() {
        let graph = parse("A -> B\n\n\nC -> D");
        assert_eq!(graph.rows.len(), 2);
    }

    #[test]
    fn test_long_labels() {
        let graph = parse("Very Long Node Label -> Another Long One");
        assert_eq!(graph.rows[0].nodes[0].label, "Very Long Node Label");
        assert_eq!(graph.rows[0].nodes[1].label, "Another Long One");
    }
}
