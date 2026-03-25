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
fn test_bullet_depths() {
    let slides = parse("# Test\n- top\n  - mid\n    - deep");
    assert_eq!(slides[0].bullets.len(), 3);
    assert_eq!(slides[0].bullets[0].depth, 0);
    assert_eq!(slides[0].bullets[0].text, "top");
    assert_eq!(slides[0].bullets[1].depth, 1);
    assert_eq!(slides[0].bullets[2].depth, 2);
}

#[test]
fn test_code_block_parsing() {
    let src = "# Code\n```python\nprint('hello')\n```";
    let slides = parse(src);
    assert_eq!(slides[0].code_blocks.len(), 1);
    assert_eq!(slides[0].code_blocks[0].language, "python");
    assert_eq!(slides[0].code_blocks[0].code, "print('hello')");
}

#[test]
fn test_code_block_exec_mode() {
    let src = "# Code\n```bash +exec\necho hi\n```";
    let slides = parse(src);
    assert_eq!(slides[0].code_blocks[0].exec_mode, Some(ExecMode::Exec));
}

#[test]
fn test_code_block_pty_mode() {
    let src = "# Code\n```bash +pty\nhtop\n```";
    let slides = parse(src);
    assert_eq!(slides[0].code_blocks[0].exec_mode, Some(ExecMode::Pty));
}

#[test]
fn test_code_block_label() {
    let src = "# Code\n```rust {label: \"example.rs\"}\nfn main() {}\n```";
    let slides = parse(src);
    assert_eq!(slides[0].code_blocks[0].label, "example.rs");
}

#[test]
fn test_section_directive() {
    let src = "<!-- section: intro -->\n# Welcome";
    let slides = parse(src);
    assert_eq!(slides[0].section, "intro");
}

#[test]
fn test_section_inherits() {
    let src = "<!-- section: intro -->\n# Slide 1\n---\n# Slide 2";
    let slides = parse(src);
    assert_eq!(slides[0].section, "intro");
    assert_eq!(slides[1].section, "intro");
}

#[test]
fn test_timing_directive() {
    let src = "<!-- timing: 2.5 -->\n# Timed";
    let slides = parse(src);
    assert!((slides[0].timing_minutes - 2.5).abs() < f64::EPSILON);
}

#[test]
fn test_default_timing() {
    let slides = parse("# No Timing");
    assert!((slides[0].timing_minutes - 1.0).abs() < f64::EPSILON);
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
        assert!(slides.len() >= 20, "Expected at least 20 slides, got {}", slides.len());
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
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("presentations/examples/test_presentation.md");
    if path.exists() {
        let source = std::fs::read_to_string(&path).unwrap();
        let (_meta, slides) = parse_presentation(&source, path.parent()).unwrap();
        assert!(slides.len() >= 15, "Expected at least 15 slides, got {}", slides.len());
        // Verify tables parsed
        let table_slides: Vec<_> = slides.iter().filter(|s| !s.tables.is_empty()).collect();
        assert!(table_slides.len() >= 2, "Expected at least 2 slides with tables");
        // Verify block quotes parsed
        let quote_slides: Vec<_> = slides.iter().filter(|s| !s.block_quotes.is_empty()).collect();
        assert!(!quote_slides.is_empty(), "Expected at least 1 slide with block quotes");
        // Verify columns parsed
        let col_slides: Vec<_> = slides.iter().filter(|s| s.columns.is_some()).collect();
        assert!(col_slides.len() >= 2, "Expected at least 2 slides with columns");
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
    let spans = parse_inline_formatting("**Bold *and italic* mixed**", Color::White, Color::DarkGrey);
    // Should produce: "Bold " (bold), "and italic" (bold+italic), " mixed" (bold)
    assert!(spans.len() >= 3, "Expected at least 3 spans, got {}: {:?}", spans.len(), spans.iter().map(|s| &s.text).collect::<Vec<_>>());
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
    let src = "# Slide\n| Name | Value |\n| --- | --- |\n| foo | 1 |\n| bar | 2 |";
    let slides = parse(src);
    assert_eq!(slides[0].tables.len(), 1);
    let table = &slides[0].tables[0];
    assert_eq!(table.headers, vec!["Name", "Value"]);
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0], vec!["foo", "1"]);
    assert_eq!(table.rows[1], vec!["bar", "2"]);
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
    assert_eq!(slides[0].block_quotes[0].lines, vec!["This is a quote", "Second line"]);
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
    let src = "---\ntitle: My Deck\nauthor: Alice\ndate: 2026-03-09\naccent: \"#FF5500\"\nalign: center\ntransition: fade\n---\n# First Slide";
    let (meta, slides) = parse_presentation(src, None).unwrap();
    assert_eq!(meta.title, "My Deck");
    assert_eq!(meta.author, "Alice");
    assert_eq!(meta.date, "2026-03-09");
    assert_eq!(meta.accent, "#FF5500");
    assert_eq!(meta.default_alignment, Some(crate::presentation::SlideAlignment::Center));
    assert_eq!(meta.transition, "fade");
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
    assert_eq!(slides[0].alignment, Some(crate::presentation::SlideAlignment::Center));
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
    assert_eq!(slides[0].transition, Some(crate::render::animation::TransitionType::Dissolve));
}

#[test]
fn test_animation_directives() {
    let src = "<!-- animation: typewriter -->\n<!-- loop_animation: matrix -->\n# Animated";
    let slides = parse(src);
    assert_eq!(slides[0].entrance_animation, Some(crate::render::animation::EntranceAnimation::Typewriter));
    assert_eq!(slides[0].loop_animations, vec![(crate::render::animation::LoopAnimation::Matrix, None)]);
}

#[test]
fn test_preamble_directives() {
    let src = "# Code\n<!-- preamble_start: python -->\nimport math\n<!-- preamble_end -->\n```python +exec\nprint(math.pi)\n```";
    let slides = parse(src);
    assert_eq!(slides[0].code_preambles.get("python").unwrap(), "import math");
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
    assert_eq!(slides[0].title_scale, None);
}

#[test]
fn test_title_scale_directive() {
    let src = "<!-- title_scale: 5 -->\n# Big Title";
    let slides = parse(src);
    assert_eq!(slides[0].title_scale, Some(5));
    assert_eq!(slides[0].text_scale, None);
}

#[test]
fn test_text_scale_clamped() {
    let src = "<!-- text_scale: 99 -->\n# Clamped";
    let slides = parse(src);
    assert_eq!(slides[0].text_scale, Some(7));
}

#[test]
fn test_title_scale_clamped_min() {
    let src = "<!-- title_scale: 0 -->\n# Zero";
    let slides = parse(src);
    assert_eq!(slides[0].title_scale, Some(1));
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
