use super::*;
use crate::presentation::{Callout, FooterAlign, Step, TableAlign};
use crate::render::animation::{EntranceAnimation, LoopAnimation, TransitionType};

fn parse(src: &str) -> Vec<Slide> {
    parse_presentation(src, None).unwrap().1
}

fn titles(slides: &[Slide]) -> Vec<&str> {
    slides.iter().map(|s| s.title.as_str()).collect()
}

#[test]
fn slides_split_on_separator_lines() {
    let slides = parse("# Slide 1\n---\n\n---\n# Slide 2\n---  \n# Slide 3");
    assert_eq!(titles(&slides), ["Slide 1", "Slide 2", "Slide 3"]);
    let numbers: Vec<usize> = slides.iter().map(|s| s.number).collect();
    assert_eq!(numbers, [1, 2, 3]);
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
    assert_eq!(titles(&slides), ["A", "B", "C"]);
    assert_eq!(slides[0].code_blocks[0].code, "print(1)");
    assert_eq!(slides[1].diagram_blocks[0].source, "X -> Y");
    assert_eq!(slides[2].code_preambles["python"], "import os");
}

#[test]
fn front_matter_only_for_leading_key_value_block() {
    let cases: [(&str, &[&str], &str); 4] = [
        ("---\ntitle: Deck\n---\n# One", &["One"], "Deck"),
        ("\u{feff}---\ntitle: Deck\n---\n# One", &["One"], "Deck"),
        ("---\n# One\n---\n# Two", &["One", "Two"], ""),
        ("---\n---\n# One", &["One"], ""),
    ];
    for (src, expected, deck_title) in cases {
        let (meta, slides) = parse_presentation(src, None).unwrap();
        assert_eq!(titles(&slides), expected, "{src:?}");
        assert_eq!(meta.title, deck_title, "{src:?}");
    }
}

#[test]
fn front_matter_fields() {
    let src = "---\ntitle: My Deck\nauthor: Alice\ndate: 2026-03-09\naccent: \"#FF5500\"\nalign: center\ntransition: fade\ntheme: nord\n---\n# First Slide";
    let (meta, slides) = parse_presentation(src, None).unwrap();
    assert_eq!(meta.title, "My Deck");
    assert_eq!(meta.author, "Alice");
    assert_eq!(meta.date, "2026-03-09");
    assert_eq!(meta.accent, "#FF5500");
    assert_eq!(meta.default_alignment, Some(SlideAlignment::Center));
    assert_eq!(meta.transition, "fade");
    assert_eq!(meta.theme.as_deref(), Some("nord"));
    assert_eq!(titles(&slides), ["First Slide"]);
}

#[test]
fn slide_directives() {
    type Check = fn(&Slide) -> bool;
    let cases: &[(&str, Check)] = &[
        ("<!-- section: Code Execution -->", |s| {
            s.section == "Code Execution"
        }),
        ("<!-- ascii_title -->", |s| s.ascii_title),
        ("<!-- font_size: -3 -->", |s| s.font_size == Some(-3)),
        ("<!-- font_size: 99 -->", |s| s.font_size == Some(20)),
        ("<!-- text_scale: 3 -->", |s| s.text_scale == Some(3)),
        ("<!-- text_scale: 99 -->", |s| s.text_scale == Some(7)),
        ("<!-- footer: Custom Footer -->", |s| {
            s.footer.as_deref() == Some("Custom Footer")
        }),
        ("<!-- footer_align: right -->", |s| {
            s.footer_align == FooterAlign::Right
        }),
        ("<!-- align: vcenter -->", |s| {
            s.alignment == Some(SlideAlignment::VCenter)
        }),
        ("<!-- title_decoration: box -->", |s| {
            s.title_decoration.as_deref() == Some("box")
        }),
        ("<!-- transition: dissolve -->", |s| {
            s.transition == Some(TransitionType::Dissolve)
        }),
        ("<!-- animation: typewriter -->", |s| {
            s.entrance_animation == Some(EntranceAnimation::Typewriter)
        }),
        ("<!-- loop_animation: sparkle(figlet) -->", |s| {
            s.loop_animations == [(LoopAnimation::Sparkle, Some("figlet".to_string()))]
        }),
        ("<!-- fullscreen -->", |s| s.fullscreen == Some(true)),
        ("<!-- fullscreen: false -->", |s| {
            s.fullscreen == Some(false)
        }),
        ("<!-- show_section: false -->", |s| {
            s.show_section == Some(false)
        }),
        ("<!-- theme: cyber_red -->", |s| {
            s.theme_override.as_deref() == Some("cyber_red")
        }),
        ("<!-- notes: Remember this -->", |s| {
            s.notes == "Remember this"
        }),
    ];
    for (directive, check) in cases {
        let slide = &parse(&format!("{directive}\n# Title"))[0];
        assert!(check(slide), "{directive}: {slide:?}");
        assert_eq!(slide.title, "Title", "{directive}");
    }
}

#[test]
fn sections_carry_over_to_later_slides() {
    let slides = parse("<!-- section: intro -->\n# A\n---\n# B\n---\n<!-- section: next -->\n# C");
    let sections: Vec<&str> = slides.iter().map(|s| s.section.as_str()).collect();
    assert_eq!(sections, ["intro", "intro", "next"]);
}

#[test]
fn comments_and_legacy_directives_are_hidden() {
    let slides = parse(
        "# T\n<!-- some random comment -->\n<!-- timing: 2.0 -->\n<!-- title_scale: 3 -->\n<!-- font_transition: none -->\n<!--\nTODO\n-->\n<!-- notes: first\nsecond\n-->\n- bullet",
    );
    let s = &slides[0];
    assert!(s.subtitle.is_empty());
    assert!(s.paragraphs.is_empty());
    assert_eq!(s.bullets.len(), 1);
    assert_eq!(s.notes, "first\nsecond");
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
    assert_eq!(slide.subtitle, "**Sub heading**");
    assert_eq!(slide.paragraphs, ["text", "**Third**", "**Second**"]);
    assert!(slide.bullets.is_empty());
}

#[test]
fn blocks_record_source_order() {
    let slides = parse(
        "# T\nSubtitle\n\nIntro line one\nline two\n- a\n- b\n```sh\nls\n```\n> quote\n> more\n\n| h |\n|---|\n| r |\n\n- c",
    );
    let s = &slides[0];
    assert_eq!(s.subtitle, "Subtitle");
    assert_eq!(s.paragraphs, ["Intro line one line two"]);
    assert_eq!(s.block_quotes[0].lines, ["quote", "more"]);
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
fn fence_info_string() {
    let cases = [
        ("```python", "python", None, ""),
        ("```bash +exec", "bash", Some(ExecMode::Exec), ""),
        ("```bash +pty", "bash", Some(ExecMode::Pty), ""),
        ("```rust {label: \"main.rs\"}", "rust", None, "main.rs"),
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
fn preamble_is_stored_per_language() {
    let src = "# Code\n<!-- preamble_start: python -->\nimport math\n<!-- preamble_end -->\n```python +exec\nprint(math.pi)\n```";
    let slide = &parse(src)[0];
    assert_eq!(slide.code_preambles["python"], "import math");
    assert_eq!(slide.code_blocks[0].code, "print(math.pi)");
}

#[test]
fn image_directives_and_path_resolution() {
    let src = "# S\n<!-- image_scale: 50 -->\n<!-- image_render: ascii -->\n<!-- image_position: left -->\n<!-- image_color: #FF0000 -->\n![alt text](img/a.png)";
    let (_, slides) = parse_presentation(src, Some(Path::new("/deck"))).unwrap();
    let img = slides[0].image.as_ref().unwrap();
    assert_eq!(img.path, Path::new("/deck/img/a.png"));
    assert_eq!(img.alt_text, "alt text");
    assert_eq!(img.scale, 50);
    assert_eq!(img.render_mode, ImageRenderMode::Ascii);
    assert_eq!(img.position, ImagePosition::Left);
    assert_eq!(img.color_override, "#FF0000");
}

#[test]
fn columns_route_content_by_column() {
    let slides = parse(
        "# T\n<!-- column_layout: [2, 1] -->\n<!-- column_separator: none -->\n<!-- column: 0 -->\n**Header**\n- a\n**Later**\n- b\n<!-- column: 1 -->\n![i](c.png)\n<!-- image_scale: 40 -->\n<!-- column: 5 -->\n- out of range\n<!-- reset_layout -->\n- after",
    );
    let s = &slides[0];
    let cols = s.columns.as_ref().unwrap();
    assert_eq!(cols.ratios, [2, 1]);
    assert!(!cols.separator);
    assert_eq!(cols.contents[0].text_lines, ["**Header**", "**Later**"]);
    assert_eq!(cols.contents[0].bullets[1].text, "b");
    assert_eq!(
        cols.contents[0].items,
        [
            ColumnItem::Text(0),
            ColumnItem::Bullet(0),
            ColumnItem::Text(1),
            ColumnItem::Bullet(1)
        ]
    );
    let img = cols.contents[1].image.as_ref().unwrap();
    assert_eq!((img.path.as_str(), img.scale), ("c.png", Some(40)));
    let slide_bullets: Vec<&str> = s.bullets.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(slide_bullets, ["out of range", "after"]);
    assert_eq!(s.blocks, [Block::Columns, Block::Bullets(0)]);
}

#[test]
fn tables_keep_alignment_and_empty_cells() {
    let src = "# Slide\n| Name | Unit | Value |\n| :--- | :---: | ---: |\n| foo | m | 1 |\n| bar |  | 2 |";
    let table = &parse(src)[0].tables[0];
    assert_eq!(table.headers, ["Name", "Unit", "Value"]);
    assert_eq!(
        table.alignments,
        [TableAlign::Left, TableAlign::Center, TableAlign::Right]
    );
    assert_eq!(table.rows, [["foo", "m", "1"], ["bar", "", "2"]]);
}

#[test]
fn bundled_decks_parse_into_titled_slides() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("presentations/examples");
    let mut decks = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "md") {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        let (meta, slides) = parse_presentation(&source, path.parent()).unwrap();
        assert!(!meta.title.is_empty(), "{path:?}: front matter");
        assert!(!slides.is_empty(), "{path:?}");
        for slide in &slides {
            assert!(
                !slide.title.is_empty(),
                "{path:?}: slide {} has no title",
                slide.number
            );
        }
        decks += 1;
    }
    assert!(decks > 0, "no decks in {dir:?}");

    let source = std::fs::read_to_string(dir.join("test_presentation.md")).unwrap();
    let (_, slides) = parse_presentation(&source, None).unwrap();
    assert!(slides.iter().any(|s| !s.tables.is_empty()));
    assert!(slides.iter().any(|s| !s.block_quotes.is_empty()));
    assert!(slides.iter().any(|s| s.columns.is_some()));
}

#[test]
fn pauses_and_highlight_groups_become_build_steps() {
    let s = &parse(concat!(
        "# T\n<!-- pause -->\nIntro\n- a\n<!-- pause -->\n- b\n",
        "```rust {label: \"x.rs\"} {1,3-4|all}\nfn main() {}\n```\n",
        "```py {label: \"bad\"} {2-1}\npass\n```",
    ))[0];
    assert_eq!(
        s.subtitle, "",
        "text after a pause is not the always-visible subtitle"
    );
    assert_eq!(
        s.blocks,
        [
            Block::Paragraph(0),
            Block::Bullets(0),
            Block::Bullets(1),
            Block::Code(0),
            Block::Code(1)
        ]
    );
    assert_eq!(
        s.steps,
        [
            Step::Pause(0),
            Step::Pause(2),
            Step::Highlight { code: 0, group: 1 }
        ]
    );
    assert_eq!(s.code_blocks[0].label, "x.rs");
    assert_eq!(s.code_blocks[0].highlights, [vec![(1, 1), (3, 4)], vec![]]);
    assert!(s.code_blocks[1].highlights.is_empty());
}

#[test]
fn github_alerts_become_callouts() {
    let s = &parse("# T\n> [!WARNING]\n> Careful\n\n> [!tip] Pro move\n> x\n\n> [!bogus]\n> y")[0];
    let callouts: Vec<_> = s.block_quotes.iter().map(|q| q.callout.clone()).collect();
    assert_eq!(
        callouts,
        [
            Some((Callout::Warning, "Warning".to_string())),
            Some((Callout::Tip, "Pro move".to_string())),
            None,
        ]
    );
    assert_eq!(s.block_quotes[0].lines, ["Careful"]);
    assert_eq!(s.block_quotes[2].lines, ["[!bogus]", "y"]);
}
