//! Presenter behavior checked on the composed screen.

use super::*;
use crate::terminal::protocols::ImageProtocol;

pub(super) fn presenter(md: &str) -> Presenter {
    // State writes fail quietly here, so tests never touch the disk.
    presenter_at(md, PathBuf::from("/nonexistent/ostendo-test/deck.md"))
}

fn presenter_at(md: &str, presentation_path: PathBuf) -> Presenter {
    let (meta, slides) = crate::markdown::parse_presentation(md, None).unwrap();
    let mut p = Presenter::new(PresenterConfig {
        slides,
        meta,
        theme: ThemeRegistry::load().get("terminal_green").unwrap(),
        theme_explicit: true,
        start: Some(0),
        presentation_path,
        image_protocol: Some(ImageProtocol::Blocks),
        remote: None,
        allow_exec: true,
        allow_remote_exec: false,
        fullscreen: false,
        timer: false,
        scale: 100,
    });
    p.watcher = None;
    p.width = 60;
    p.height = 20;
    p
}

/// Screen rows as plain text, trailing spaces trimmed.
pub(super) fn screen(p: &mut Presenter) -> Vec<String> {
    p.compose()
        .rows
        .iter()
        .map(|r| {
            let text: String = r.line.spans.iter().map(|s| s.text.as_str()).collect();
            text.trim_end().to_string()
        })
        .collect()
}

fn row_of(rows: &[String], needle: &str) -> Option<usize> {
    rows.iter().position(|r| r.contains(needle))
}

#[test]
fn pauses_build_the_slide_without_moving_it() {
    let mut p = presenter(
        "# Plan\n<!-- align: center -->\n- one\n<!-- pause -->\n- two, a longer item\n<!-- pause -->\nDone.\n---\n# Next",
    );
    let first = screen(&mut p);
    assert!(row_of(&first, "two").is_none() && row_of(&first, "Done").is_none());
    p.next_slide();
    let second = screen(&mut p);
    assert!(row_of(&second, "two").is_some() && row_of(&second, "Done").is_none());
    p.next_slide();
    let built = screen(&mut p);
    assert!(row_of(&built, "Done").is_some());
    assert_eq!(
        row_of(&first, "one"),
        row_of(&built, "one"),
        "content shifted"
    );
    let bullet_col = |needle: &str| built[row_of(&built, needle).unwrap()].find('•');
    assert_eq!(
        bullet_col("one"),
        bullet_col("two"),
        "a list split by pauses stays aligned"
    );

    p.next_slide();
    assert_eq!(p.current, 1);
    p.prev_slide();
    assert!(
        row_of(&screen(&mut p), "Done").is_some(),
        "going back shows it built"
    );
}

#[test]
fn code_highlights_step_through_their_groups() {
    let mut p = presenter("# Code\n```rust {1|2|all}\nlet a = 1;\nlet b = 2;\n```");
    let bars = |p: &mut Presenter| -> Vec<bool> {
        let screen = p.compose();
        ["let a", "let b"]
            .iter()
            .map(|needle| {
                let row = screen.rows.iter().find(|r| {
                    r.line
                        .spans
                        .iter()
                        .map(|s| s.text.as_str())
                        .collect::<String>()
                        .contains(needle)
                });
                row.is_some_and(|r| r.line.spans.iter().any(|s| s.text == "▌"))
            })
            .collect()
    };
    assert_eq!(bars(&mut p), [true, false]);
    p.next_slide();
    assert_eq!(bars(&mut p), [false, true]);
    p.next_slide();
    assert_eq!(bars(&mut p), [false, false]);
}

#[test]
fn reload_jumps_to_the_edited_slide_fully_built() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("deck.md");
    let deck =
        |last: &str| format!("# One\n---\n# Two\n---\n# Three\n- a\n<!-- pause -->\n- {last}");
    std::fs::write(&path, deck("b")).unwrap();
    let mut p = presenter_at(&deck("b"), path.clone());

    std::fs::write(&path, deck("changed")).unwrap();
    p.reload();
    assert_eq!((p.current, p.step), (2, 1));
    assert!(row_of(&screen(&mut p), "changed").is_some());

    p.goto_slide(0);
    std::fs::write(&path, format!("---\ntitle: New\n---\n{}", deck("changed"))).unwrap();
    p.reload();
    assert_eq!(p.current, 0, "front matter edits keep the position");
}

#[test]
fn chart_bars_scale_to_the_largest_value() {
    let mut p = presenter("# Chart\n```chart\nFull: 10\nHalf: 5 ms\n```");
    let rows = screen(&mut p);
    let row = |label: &str| rows[row_of(&rows, label).unwrap()].clone();
    let (full, half) = (
        row("Full").matches('█').count(),
        row("Half").matches('█').count(),
    );
    assert!(
        full > 20 && full.abs_diff(half * 2) <= 1,
        "{full} vs {half}"
    );
    let value_col = |label: &str, value: &str| {
        let r = row(label);
        assert!(r.ends_with(value), "{r:?}");
        r.chars().count() - value.chars().count()
    };
    assert_eq!(
        value_col("Full", "10"),
        value_col("Half", "5 ms"),
        "values line up"
    );
}

#[test]
fn the_timer_shows_how_far_behind_pace_the_talk_is() {
    let mut p = presenter("---\nduration: 8\n---\n# A\n---\n# B\n---\n# C\n---\n# D");
    // Two minutes is the plan for the first of four slides; five have passed.
    p.timer_start = Instant::now().checked_sub(std::time::Duration::from_secs(300));
    let bar = screen(&mut p).pop().unwrap();
    assert!(bar.contains("/ 8:00 · 3:00 behind"), "{bar}");

    p.goto_slide(3);
    let bar = screen(&mut p).pop().unwrap();
    assert!(bar.contains("/ 8:00") && !bar.contains("behind"), "{bar}");
}
