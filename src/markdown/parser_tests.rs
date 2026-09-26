//! Unit tests for the Markdown-to-slide parser.

use super::*;
use crate::presentation::TableAlign;

fn parse(src: &str) -> Vec<Slide> {
    let (_meta, slides) = parse_presentation(src, None).unwrap();
    slides
}

#[test]
fn test_single_slide_title() {
    let slides = parse("# Hello World");
    assert_eq!(slides.len(), 1);
    assert_eq!(slides[0].title, "Hello World");
}

#[test]
fn test_multiple_slides() {
    let slides = parse("# Slide 1\n---\n# Slide 2\n---\n# Slide 3");
    assert_eq!(slides.len(), 3);
    assert_eq!(slides[0].title, "Slide 1");
    assert_eq!(slides[1].title, "Slide 2");
    assert_eq!(slides[2].title, "Slide 3");
}

#[test]
fn test_empty_slides_skipped() {
    let slides = parse("# Slide 1\n---\n\n---\n# Slide 3");
    assert_eq!(slides.len(), 2);
}

#[test]
fn list_items_need_a_marker_and_whitespace() {
    let cases = [
        ("- dash", Some(("dash", 0))),
        ("* star", Some(("star", 0))),
        ("+ plus", Some(("plus", 0))),
        ("  - nested", Some(("nested", 1))),
        ("    - deep", Some(("deep", 2))),
        ("1. first", Some(("1. first", 0))),
        ("   12) twelfth", Some(("12) twelfth", 1))),
        ("**Bold** text", None),
        ("*Note:* x", None),
        ("-5 degrees", None),
    ];
    for (line, bullet) in cases {
        let slide = &parse(&format!("# T\n{line}"))[0];
        let got: Vec<(&str, usize)> = slide
            .bullets
            .iter()
            .map(|b| (b.text.as_str(), b.depth))
            .collect();
        match bullet {
            Some(item) => assert_eq!(got, [item], "{line}"),
            None => {
                assert!(got.is_empty(), "{line}");
                assert_eq!(slide.subtitle, line);
            }
        }
    }
}

#[test]
fn headings_and_rules() {
    let slide = &parse("# T\n## Sub heading\ntext\n### Third\n***\n* * *\n___\n# Second")[0];
    assert_eq!(slide.title, "T");
    assert_eq!(slide.subtitle, "Sub heading");
    assert_eq!(slide.paragraphs, ["text", "Third", "Second"]);
    assert!(slide.bullets.is_empty());
}

#[test]
fn fence_info_string() {
    let cases = [
        ("```python", "python", None, ""),
        ("```bash +exec", "bash", Some(ExecMode::Exec), ""),
        ("```bash +pty", "bash", Some(ExecMode::Pty), ""),
        (
            "```rust {label: \"example.rs\"}",
            "rust",
            None,
            "example.rs",
        ),
        ("```c++ +exec", "c++", Some(ExecMode::Exec), ""),
        ("```rust,ignore", "rust,ignore", None, ""),
        ("```objective-c", "objective-c", None, ""),
        (
            "```shell-session +pty {label: \"s\"}",
            "shell-session",
            Some(ExecMode::Pty),
            "s",
        ),
        ("```+exec", "", Some(ExecMode::Exec), ""),
        ("~~~python", "python", None, ""),
    ];
    for (open, language, exec_mode, label) in cases {
        let close = &open[..3];
        let slides = parse(&format!("# T\n{open}\n- body\n{close}"));
        let block = &slides[0].code_blocks[0];
        assert_eq!(block.language, language, "{open}");
        assert_eq!(block.exec_mode, exec_mode, "{open}");
        assert_eq!(block.label, label, "{open}");
        assert_eq!(block.code, "- body", "{open}");
    }
}

#[test]
fn separator_inside_fence_or_comment_stays_in_slide() {
    for (body, code) in [
        ("```\na\n---\nb\n```", "a\n---\nb"),
        ("~~~ yaml\na\n---\n~~~", "a\n---"),
        ("````md\n```\n---\n```\n````", "```\n---\n```"),
    ] {
        let slides = parse(&format!("# T\n{body}\n---\n# Next"));
        assert_eq!(slides.len(), 2, "{body}");
        assert_eq!(slides[0].code_blocks[0].code, code, "{body}");
    }
    let slides =
        parse("# T\n<!-- notes:\nbefore\n---\nafter\n-->\n<!--\nTODO\n---\n-->\n---\n# Next");
    assert_eq!(slides.len(), 2);
    assert_eq!(slides[0].notes, "before\n---\nafter");
}

#[test]
fn unclosed_blocks_keep_content_and_end_with_the_slide() {
    let slides = parse(
        "# A\n```python\nprint(1)\n---\n# B\n```diagram\nX -> Y\n---\n# C\n<!-- preamble_start: python -->\nimport os",
    );
    let titles: Vec<&str> = slides.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(titles, ["A", "B", "C"]);
    assert_eq!(slides[0].code_blocks[0].code, "print(1)");
    assert_eq!(slides[1].diagram_blocks[0].source, "X -> Y");
    assert_eq!(slides[2].code_preambles["python"], "import os");
}

#[test]
fn multi_line_comments_do_not_leak() {
    let slides = parse("# T\n<!--\nTODO\n-->\n<!-- notes: first\nsecond\n-->");
    assert!(slides[0].subtitle.is_empty());
    assert!(slides[0].blocks.is_empty());
    assert_eq!(slides[0].notes, "first\nsecond");
}

#[test]
fn front_matter_only_for_leading_key_value_block() {
    let cases: [(&str, &[&str], &str); 4] = [
        ("---\ntitle: Deck\n---\n# One", &["One"], "Deck"),
        ("\u{feff}---\ntitle: Deck\n---\n# One", &["One"], "Deck"),
        ("---\n# One\n---\n# Two", &["One", "Two"], ""),
        ("---\n---\n# One", &["One"], ""),
    ];
    for (src, titles, deck_title) in cases {
        let (meta, slides) = parse_presentation(src, None).unwrap();
        let got: Vec<&str> = slides.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(got, titles, "{src:?}");
        assert_eq!(meta.title, deck_title, "{src:?}");
    }
}

#[test]
fn test_section_directive() {
    let src = "<!-- section: Code Execution -->\n# Welcome";
    let slides = parse(src);
    assert_eq!(slides[0].section, "Code Execution");
}

#[test]
fn out_of_range_column_falls_back_to_slide_level() {
    let slides =
        parse("# T\n<!-- column_layout: [1, 1] -->\n<!-- column: 5 -->\n- kept\n```sh\necho\n```");
    assert_eq!(slides[0].bullets[0].text, "kept");
    assert_eq!(slides[0].code_blocks.len(), 1);
}

#[test]
fn test_section_inherits() {
    let src = "<!-- section: intro -->\n# Slide 1\n---\n# Slide 2";
    let slides = parse(src);
    assert_eq!(slides[0].section, "intro");
    assert_eq!(slides[1].section, "intro");
}

#[test]
fn test_notes_single_line() {
    let src = "# Slide\n<!-- notes: Remember this -->";
    let slides = parse(src);
    assert_eq!(slides[0].notes, "Remember this");
}

#[test]
fn test_notes_multi_line() {
    let src = "# Slide\n<!-- notes:\nLine 1\nLine 2\n-->";
    let slides = parse(src);
    assert_eq!(slides[0].notes, "Line 1\nLine 2");
}

#[test]
fn test_image_parsing() {
    let src = "# Slide\n![alt text](image.png)\n";
    let slides = parse(src);
    assert!(slides[0].image.is_some());
    let img = slides[0].image.as_ref().unwrap();
    assert_eq!(img.alt_text, "alt text");
    assert_eq!(img.path.to_str().unwrap(), "image.png");
}

#[test]
fn test_ascii_title_directive() {
    let src = "<!-- ascii_title -->\n# Big Title";
    let slides = parse(src);
    assert!(slides[0].ascii_title);
}

#[test]
fn test_front_matter_skipped() {
    let src = "---\ntitle: My Deck\nauthor: Me\n---\n# First Slide";
    let slides = parse(src);
    assert_eq!(slides.len(), 1);
    assert_eq!(slides[0].title, "First Slide");
}

#[test]
fn test_subtitle_extraction() {
    let src = "# Title\nThis is a subtitle";
    let slides = parse(src);
    assert_eq!(slides[0].subtitle, "This is a subtitle");
}

#[test]
fn blocks_record_source_order() {
    let slides = parse(
        "# T\nSubtitle\n\nIntro line one\nline two\n- a\n- b\n```sh\nls\n```\n> quote\n\n| h |\n|---|\n| r |\n\n- c",
    );
    let s = &slides[0];
    assert_eq!(s.subtitle, "Subtitle");
    assert_eq!(s.paragraphs, ["Intro line one line two"]);
    assert_eq!(
        s.blocks,
        [
            Block::Paragraph(0),
            Block::Bullets(0),
            Block::Code(0),
            Block::Quote(0),
            Block::Table(0),
            Block::Bullets(1),
        ]
    );
    assert_eq!(s.bullet_groups, [0..2, 2..3]);
}

#[test]
fn text_without_title_is_kept() {
    let slides = parse("# Deck\n---\nThank you!");
    assert_eq!(slides[1].subtitle, "");
    assert_eq!(slides[1].paragraphs, ["Thank you!"]);
}

#[test]
fn test_html_comments_ignored() {
    let src = "# Slide\n<!-- some random comment -->\n- bullet";
    let slides = parse(src);
    assert_eq!(slides[0].bullets.len(), 1);
}

#[test]
fn test_presentation_file() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("presentation.md");
    if path.exists() {
        let source = std::fs::read_to_string(&path).unwrap();
        let (_meta, slides) = parse_presentation(&source, path.parent()).unwrap();
        assert!(
            slides.len() >= 20,
            "Expected at least 20 slides, got {}",
            slides.len()
        );
    }
}

#[test]
fn test_slide_numbering() {
    let slides = parse("# A\n---\n# B\n---\n# C");
    assert_eq!(slides[0].number, 1);
    assert_eq!(slides[1].number, 2);
    assert_eq!(slides[2].number, 3);
}

#[test]
fn test_test_presentation() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("presentations/examples/test_presentation.md");
    if path.exists() {
        let source = std::fs::read_to_string(&path).unwrap();
        let (_meta, slides) = parse_presentation(&source, path.parent()).unwrap();
        assert!(
            slides.len() >= 15,
            "Expected at least 15 slides, got {}",
            slides.len()
        );
        // Verify tables parsed
        let table_slides: Vec<_> = slides.iter().filter(|s| !s.tables.is_empty()).collect();
        assert!(
            table_slides.len() >= 2,
            "Expected at least 2 slides with tables"
        );
        // Verify block quotes parsed
        let quote_slides: Vec<_> = slides
            .iter()
            .filter(|s| !s.block_quotes.is_empty())
            .collect();
        assert!(
            !quote_slides.is_empty(),
            "Expected at least 1 slide with block quotes"
        );
        // Verify columns parsed
        let col_slides: Vec<_> = slides.iter().filter(|s| s.columns.is_some()).collect();
        assert!(
            col_slides.len() >= 2,
            "Expected at least 2 slides with columns"
        );
    }
}

#[test]
fn test_inline_bold() {
    use crossterm::style::Color;
    let spans = parse_inline_formatting("hello **world**", Color::White, Color::DarkGrey);
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].text, "hello ");
    assert!(!spans[0].bold);
    assert_eq!(spans[1].text, "world");
    assert!(spans[1].bold);
}

#[test]
fn test_inline_italic() {
    use crossterm::style::Color;
    let spans = parse_inline_formatting("hello *world*", Color::White, Color::DarkGrey);
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].text, "hello ");
    assert!(spans[1].italic);
}

#[test]
fn test_inline_bold_italic_nested() {
    use crossterm::style::Color;
    let spans =
        parse_inline_formatting("**Bold *and italic* mixed**", Color::White, Color::DarkGrey);
    // Should produce: "Bold " (bold), "and italic" (bold+italic), " mixed" (bold)
    assert!(
        spans.len() >= 3,
        "Expected at least 3 spans, got {}: {:?}",
        spans.len(),
        spans.iter().map(|s| &s.text).collect::<Vec<_>>()
    );
    assert!(spans[0].bold);
    assert!(!spans[0].italic);
    assert!(spans[1].bold);
    assert!(spans[1].italic);
    assert!(spans[2].bold);
    assert!(!spans[2].italic);
}

#[test]
fn test_inline_strikethrough() {
    use crossterm::style::Color;
    let spans = parse_inline_formatting("hello ~~world~~", Color::White, Color::DarkGrey);
    assert_eq!(spans.len(), 2);
    assert!(spans[1].strikethrough);
}

#[test]
fn test_inline_code() {
    use crossterm::style::Color;
    let spans = parse_inline_formatting("use `println!`", Color::White, Color::DarkGrey);
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].text, "use ");
    assert!(spans[1].text.contains("println!"));
    assert_eq!(spans[1].bg, Some(Color::DarkGrey));
}

#[test]
fn test_table_parsing() {
    let src = "# Slide\n| Name | Unit | Value |\n| --- | --- | --- |\n| foo | m | 1 |\n| bar |  | 2 |";
    let slides = parse(src);
    assert_eq!(slides[0].tables.len(), 1);
    let table = &slides[0].tables[0];
    assert_eq!(table.headers, ["Name", "Unit", "Value"]);
    assert_eq!(table.rows, [["foo", "m", "1"], ["bar", "", "2"]]);
}

#[test]
fn test_table_alignment() {
    let src = "# Slide\n| Left | Center | Right |\n| :--- | :---: | ---: |\n| a | b | c |";
    let slides = parse(src);
    let table = &slides[0].tables[0];
    assert_eq!(table.alignments[0], TableAlign::Left);
    assert_eq!(table.alignments[1], TableAlign::Center);
    assert_eq!(table.alignments[2], TableAlign::Right);
}

#[test]
fn test_blockquote_parsing() {
    let src = "# Slide\n> This is a quote\n> Second line";
    let slides = parse(src);
    assert_eq!(slides[0].block_quotes.len(), 1);
    assert_eq!(
        slides[0].block_quotes[0].lines,
        vec!["This is a quote", "Second line"]
    );
}

#[test]
fn test_inline_plain_text() {
    use crossterm::style::Color;
    let spans = parse_inline_formatting("no formatting here", Color::White, Color::DarkGrey);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].text, "no formatting here");
}

// ── Batch 1 tests ──

#[test]
fn test_front_matter_meta() {
    let src = "---\ntitle: My Deck\nauthor: Alice\ndate: 2026-03-09\naccent: \"#FF5500\"\nalign: center\ntransition: fade\ntheme: nord\n---\n# First Slide";
    let (meta, slides) = parse_presentation(src, None).unwrap();
    assert_eq!(meta.title, "My Deck");
    assert_eq!(meta.author, "Alice");
    assert_eq!(meta.date, "2026-03-09");
    assert_eq!(meta.accent, "#FF5500");
    assert_eq!(
        meta.default_alignment,
        Some(crate::presentation::SlideAlignment::Center)
    );
    assert_eq!(meta.transition, "fade");
    assert_eq!(meta.theme.as_deref(), Some("nord"));
    assert_eq!(slides.len(), 1);
    assert_eq!(slides[0].title, "First Slide");
}

#[test]
fn test_footer_directive() {
    let src = "# Slide\n<!-- footer: Custom Footer -->\n- bullet";
    let slides = parse(src);
    assert_eq!(slides[0].footer.as_deref(), Some("Custom Footer"));
}

#[test]
fn test_align_directive() {
    let src = "<!-- align: center -->\n# Centered Slide";
    let slides = parse(src);
    assert_eq!(
        slides[0].alignment,
        Some(crate::presentation::SlideAlignment::Center)
    );
}

#[test]
fn test_title_decoration_directive() {
    let src = "<!-- title_decoration: box -->\n# Boxed";
    let slides = parse(src);
    assert_eq!(slides[0].title_decoration.as_deref(), Some("box"));
}

#[test]
fn test_transition_directive() {
    let src = "<!-- transition: dissolve -->\n# Trans";
    let slides = parse(src);
    assert_eq!(
        slides[0].transition,
        Some(crate::render::animation::TransitionType::Dissolve)
    );
}

#[test]
fn test_animation_directives() {
    let src = "<!-- animation: typewriter -->\n<!-- loop_animation: matrix -->\n# Animated";
    let slides = parse(src);
    assert_eq!(
        slides[0].entrance_animation,
        Some(crate::render::animation::EntranceAnimation::Typewriter)
    );
    assert_eq!(
        slides[0].loop_animations,
        vec![(crate::render::animation::LoopAnimation::Matrix, None)]
    );
}

#[test]
fn test_preamble_directives() {
    let src = "# Code\n<!-- preamble_start: python -->\nimport math\n<!-- preamble_end -->\n```python +exec\nprint(math.pi)\n```";
    let slides = parse(src);
    assert_eq!(
        slides[0].code_preambles.get("python").unwrap(),
        "import math"
    );
}

#[test]
fn test_no_front_matter_default_meta() {
    let src = "# Just a slide";
    let (meta, slides) = parse_presentation(src, None).unwrap();
    assert!(meta.author.is_empty());
    assert!(meta.title.is_empty());
    assert_eq!(slides.len(), 1);
}

#[test]
fn test_text_scale_directive() {
    let src = "<!-- text_scale: 3 -->\n# Scaled Title";
    let slides = parse(src);
    assert_eq!(slides[0].text_scale, Some(3));
}

#[test]
fn test_text_scale_clamped() {
    let src = "<!-- text_scale: 99 -->\n# Clamped";
    let slides = parse(src);
    assert_eq!(slides[0].text_scale, Some(7));
}

#[test]
fn test_fullscreen_directive() {
    let src = "<!-- fullscreen -->\n# Full";
    let slides = parse(src);
    assert_eq!(slides[0].fullscreen, Some(true));
}

#[test]
fn test_fullscreen_directive_false() {
    let src = "<!-- fullscreen: false -->\n# Not Full";
    let slides = parse(src);
    assert_eq!(slides[0].fullscreen, Some(false));
}

#[test]
fn test_show_section_directive() {
    let src = "<!-- show_section: false -->\n# No Section";
    let slides = parse(src);
    assert_eq!(slides[0].show_section, Some(false));
}

#[test]
fn test_font_transition_directive() {
    let md = "---\n---\n# Slide\n<!-- font_transition: none -->\nHello";
    let (_, slides) = parse_presentation(md, None).unwrap();
    assert_eq!(slides[0].font_transition.as_deref(), Some("none"));
}

#[test]
fn test_theme_override_directive() {
    let src = "<!-- theme: cyber_red -->\n# Red Slide";
    let slides = parse(src);
    assert_eq!(slides[0].theme_override.as_deref(), Some("cyber_red"));
}

#[test]
fn test_theme_override_default_none() {
    let src = "# No Theme Override";
    let slides = parse(src);
    assert!(slides[0].theme_override.is_none());
}
