//! Presenter behavior checked on the composed screen.

use super::*;
use crate::terminal::protocols::ImageProtocol;

pub(super) fn presenter(md: &str) -> Presenter {
    // State writes fail quietly here, so tests never touch the disk.
    presenter_at(md, PathBuf::from("/nonexistent/ostendo-test/deck.md"))
}

pub(super) fn presenter_at(md: &str, presentation_path: PathBuf) -> Presenter {
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
        record: None,
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

#[test]
fn overflow_report_names_slides_that_would_scroll() {
    let long: String = (1..=30).map(|i| format!("- item {i}\n")).collect();
    let md = format!("# Fits\n- one\n---\n# Long\n{long}---\n# Built\n- a\n<!-- pause -->\n{long}");
    let (meta, slides) = crate::markdown::parse_presentation(&md, None).unwrap();
    let mut p = presenter(&md);
    let config = PresenterConfig {
        slides,
        meta,
        theme: p.base_theme.clone(),
        theme_explicit: true,
        start: Some(0),
        presentation_path: p.presentation_path.clone(),
        image_protocol: Some(ImageProtocol::Blocks),
        remote: None,
        allow_exec: true,
        allow_remote_exec: false,
        fullscreen: false,
        timer: false,
        scale: 100,
        record: None,
    };
    let report = overflowing_slides(config, 60, 20);
    let slides: Vec<usize> = report.iter().map(|r| r.0).collect();
    assert_eq!(slides, [2, 3], "hidden steps count too");
    // The compact layout drops list spacing: 30 items plus the title fill 32 rows.
    let layout = {
        p.current = 1;
        p.layout()
    };
    assert_eq!(report[0].1, 32 - layout.content_rows);
}

#[test]
fn the_remote_learns_what_comes_next() {
    let mut p = presenter("# Build\n- a\n<!-- pause -->\n- b\n---\n# Summary");
    let (tx, mut rx) = tokio::sync::broadcast::channel(8);
    p.state_broadcast = Some(tx);
    let mut up_next = |p: &mut Presenter| {
        p.broadcast_state();
        let json: serde_json::Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        json["up_next"].as_str().unwrap().to_string()
    };
    assert_eq!(up_next(&mut p), "1 more step on this slide");
    p.next_slide();
    assert_eq!(up_next(&mut p), "Summary");
    p.next_slide();
    assert_eq!(up_next(&mut p), "End of deck");
}

#[test]
fn columns_build_one_part_at_a_time() {
    let mut p = presenter(concat!(
        "# Cols\n<!-- column_layout: [1, 1] -->\n<!-- column: 0 -->\n- left one\n<!-- pause -->\n",
        "- left two\n<!-- column: 1 -->\n<!-- pause -->\n- right\n<!-- reset_layout -->\nAfter",
    ));
    let visible = |p: &mut Presenter| -> Vec<bool> {
        let rows = screen(p);
        ["left one", "left two", "right", "After"]
            .iter()
            .map(|t| row_of(&rows, t).is_some())
            .collect()
    };
    let first_row = row_of(&screen(&mut p), "left one");
    assert_eq!(visible(&mut p), [true, false, false, false]);
    p.next_slide();
    assert_eq!(visible(&mut p), [true, true, false, false]);
    p.next_slide();
    assert_eq!(visible(&mut p), [true, true, true, true]);
    assert_eq!(row_of(&screen(&mut p), "left one"), first_row);
}

#[test]
fn notes_move_beside_the_slide_and_resize() {
    let mut p = presenter("# Talk\n- point\n<!-- notes: say the thing -->");
    p.width = 100;
    p.show_notes = true;
    let below = screen(&mut p);
    assert!(row_of(&below, "say the thing").unwrap() > row_of(&below, "point").unwrap());

    p.move_notes();
    let beside = screen(&mut p);
    let row = row_of(&beside, "Notes").unwrap();
    assert_eq!(row, 0, "the panel starts at the top");
    let panel_col = beside[row].find("Notes").unwrap();
    assert!(panel_col > 60, "the panel sits on the right: {panel_col}");
    let slide_col = beside[row_of(&beside, "point").unwrap()]
        .find("point")
        .unwrap();
    assert!(slide_col < panel_col - 20, "the slide keeps the left side");

    let width_of = |p: &mut Presenter| 100 - screen(p)[0].find("──").unwrap();
    let before = width_of(&mut p);
    p.resize_notes(10);
    assert!(width_of(&mut p) > before);
}

#[test]
fn time_per_slide_is_recorded_for_the_pacing_report() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("talk.md");
    let mut p = presenter_at("# One\n---\n# Two\n---\n# Three", path.clone());
    let ago = |secs| {
        Instant::now()
            .checked_sub(Duration::from_secs(secs))
            .unwrap()
    };
    p.goto_slide(1); // Starts the timer.
    p.entered = ago(40);
    p.goto_slide(0);
    p.entered = ago(30);
    p.goto_slide(1);
    p.entered = ago(20);
    p.record_rehearsal();

    let runs = crate::presentation::rehearsal::load(&path);
    let secs: Vec<(&str, u64)> = runs[0]
        .slides
        .iter()
        .map(|(t, s)| (t.as_str(), s.round() as u64))
        .collect();
    assert_eq!(secs, [("One", 30), ("Two", 60), ("Three", 0)]);

    // A reset starts over, and a run under a minute is not a rehearsal.
    p.entered = ago(50);
    p.toggle_timer();
    p.toggle_timer();
    p.record_rehearsal();
    assert_eq!(crate::presentation::rehearsal::load(&path).len(), 1);
}
