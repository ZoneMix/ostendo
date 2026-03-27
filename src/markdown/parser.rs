//! Markdown-to-slide parser. Converts a markdown file with YAML front matter and HTML comment
//! directives into a vector of `Slide` structs. Supports 50+ directives for animations, layout,
//! code execution, images, themes, and more.
//!
//! # Architecture
//!
//! This module sits at the very beginning of the pipeline: raw Markdown text goes in, structured
//! `Slide` data comes out. The render engine (`src/render/`) consumes these slides to produce
//! terminal output.
//!
//! # Parsing flow
//!
//! 1. `parse_presentation()` reads the full source string.
//! 2. YAML-like front matter (between `---` fences at the top) is extracted via `parse_front_matter()`.
//! 3. The remaining text is split on `---` line separators into per-slide blocks.
//! 4. Each block is parsed by `parse_slide()`, which scans every line looking for HTML comment
//!    directives (`<!-- directive: value -->`), Markdown headings, bullets, code fences, tables,
//!    block quotes, and images.
//! 5. Inline text formatting (bold, italic, code, strikethrough) is handled by
//!    `parse_inline_formatting()` in the `inline` submodule, called later during rendering.
//!
//! # Submodules
//!
//! - `regex_patterns` — All `LazyLock<Regex>` statics used by the parser.
//! - `tables` — `TableParseState`, `parse_table_cells`, `parse_table_alignments`.
//! - `inline` — `parse_inline_formatting` for inline Markdown formatting (bold, italic, etc.).

use anyhow::Result;
use regex::Regex;
use std::path::Path;

use crate::presentation::{
    BlockQuote, Bullet, CodeBlock, ColumnContent, ColumnImage, ColumnLayout, DiagramBlock,
    DiagramStyle, ExecMode, FooterAlign, ImagePosition, ImageRenderMode, MermaidBlock,
    PresentationMeta, Slide, SlideAlignment, SlideImage, Table,
};

use super::regex_patterns::*;
use super::tables::{TableParseState, parse_table_cells, parse_table_alignments};

// Re-export parse_inline_formatting at this path for backward compatibility.
// External callers use `crate::markdown::parser::parse_inline_formatting`.
pub use super::inline::parse_inline_formatting;

/// Parses YAML-like front matter into a `PresentationMeta` struct.
///
/// Front matter sits between two `---` lines at the very top of the file and contains
/// key-value pairs like `title: My Deck`, `author: Alice`, `accent: "#FF5500"`, etc.
/// This function does **not** use a full YAML parser; it matches simple `key: value` lines
/// with a regex, which is sufficient for the small set of supported fields.
///
/// # Parameters
/// - `block`: The raw text between the opening and closing `---` fences (no fences included).
///
/// # Returns
/// A `PresentationMeta` with all recognized fields populated. Unknown keys are stored in
/// `meta.pairs` for potential downstream use but otherwise ignored.
fn parse_front_matter(block: &str) -> PresentationMeta {
    let mut meta = PresentationMeta::default();
    let kv_re = Regex::new(r"^(\w+)\s*:\s*(.+)$").unwrap();
    for line in block.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(caps) = kv_re.captures(line) {
            let key = caps[1].to_string();
            let val = caps[2].trim().trim_matches('"').to_string();
            match key.as_str() {
                "title" => meta.title = val.clone(),
                "author" => meta.author = val.clone(),
                "date" => meta.date = val.clone(),
                "accent" => meta.accent = val.clone(),
                "transition" => meta.transition = val.clone(),
                "align" | "alignment" => {
                    meta.default_alignment = match val.as_str() {
                        "center" => Some(SlideAlignment::Center),
                        "vcenter" => Some(SlideAlignment::VCenter),
                        "hcenter" => Some(SlideAlignment::HCenter),
                        "top" => Some(SlideAlignment::Top),
                        _ => None,
                    };
                }
                _ => {}
            }
            meta.pairs.push((key, val));
        }
    }
    meta
}

/// Parses a single slide block into a `Slide` struct.
///
/// This is the main per-slide parser. It scans every line of the raw slide text looking for:
///
/// 1. **Multi-line state** (notes, code blocks, diagram blocks, preamble blocks) — tracked
///    by boolean flags (`in_notes`, `in_code`, `in_diagram`). While inside one of these,
///    lines are accumulated until the closing delimiter is found.
/// 2. **HTML comment directives** — each `<!-- key: value -->` line is matched against the
///    static regex patterns defined in `regex_patterns`. Recognized directives set fields on the slide.
/// 3. **Markdown content** — headings (`#`), images (`![]()`), bullets (`- text`), tables
///    (`| ... |`), and block quotes (`> text`) are parsed into their respective structs.
/// 4. **Subtitle** — the first non-empty, non-directive line after the title heading.
///
/// # Parameters
/// - `raw`: The text of one slide (everything between two `---` separators).
/// - `number`: The 1-based slide number.
/// - `last_section`: The section name inherited from the previous slide.
/// - `base_dir`: Optional directory for resolving relative image paths.
///
/// # Returns
/// A tuple of `(Slide, current_section)` where `current_section` is passed forward so the
/// next slide can inherit it.
fn parse_slide(raw: &str, number: usize, last_section: &str, base_dir: Option<&Path>) -> (Slide, String) {
    let mut title = String::new();
    let mut subtitle = String::new();
    let mut section = String::new();
    let mut timing: Option<f64> = None;
    let mut notes_lines: Vec<String> = Vec::new();
    let mut bullets: Vec<Bullet> = Vec::new();
    let mut code_blocks: Vec<CodeBlock> = Vec::new();
    let mut image_alt = String::new();
    let mut image_path = String::new();
    let mut image_position = ImagePosition::Below;
    let mut image_render = ImageRenderMode::Auto;
    let mut image_scale: u8 = 100;
    let mut image_color = String::new();
    let mut ascii_title = false;
    let mut font_size: Option<i8> = None;
    let mut text_scale: Option<u8> = None;
    let mut title_scale: Option<u8> = None;
    let mut footer: Option<String> = None;
    let mut footer_align = FooterAlign::Left;
    let mut alignment: Option<SlideAlignment> = None;
    let mut title_decoration: Option<String> = None;
    let mut transition: Option<crate::render::animation::TransitionType> = None;
    let mut entrance_animation: Option<crate::render::animation::EntranceAnimation> = None;
    let mut loop_animations: Vec<(crate::render::animation::LoopAnimation, Option<String>)> = Vec::new();
    let mut fullscreen: Option<bool> = None;
    let mut show_section: Option<bool> = None;
    let mut code_preambles: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut preamble_lang: Option<String> = None;
    let mut preamble_lines: Vec<String> = Vec::new();
    let mut mermaid_blocks: Vec<MermaidBlock> = Vec::new();
    let mut diagram_blocks: Vec<DiagramBlock> = Vec::new();
    let mut theme_override: Option<String> = None;
    let mut font_transition: Option<String> = None;

    let mut in_notes = false;
    let mut in_code = false;
    let mut in_diagram = false;
    let mut diagram_style = DiagramStyle::Box;
    let mut diagram_lines: Vec<String> = Vec::new();
    let mut code_lang = String::new();
    let mut code_label = String::new();
    let mut code_exec_mode: Option<ExecMode> = None;
    let mut code_lines: Vec<String> = Vec::new();
    let mut title_found = false;
    let mut subtitle_found = false;
    let mut column_ratios: Option<Vec<u8>> = None;
    let mut column_contents: Vec<ColumnContent> = Vec::new();
    let mut current_column: Option<usize> = None;
    let mut column_separator: bool = true;
    let mut column_text_scale: Option<u8> = None;
    let mut tables: Vec<Table> = Vec::new();
    let mut block_quotes: Vec<BlockQuote> = Vec::new();
    let mut table_state: Option<TableParseState> = None;
    let mut blockquote_lines: Vec<String> = Vec::new();
    let mut trailing_text: Vec<String> = Vec::new();

    for line in raw.lines() {
        // ── Multi-line block state handling ──
        // These checks run first because when we're inside a multi-line block (notes, code,
        // or diagram), every line belongs to that block until the closing delimiter appears.

        // Multi-line notes continuation
        if in_notes {
            if let Some(m) = NOTES_END_RE.find(line) {
                let before = &line[..m.start()];
                if !before.trim().is_empty() {
                    notes_lines.push(before.trim_end().to_string());
                }
                in_notes = false;
            } else {
                notes_lines.push(line.to_string());
            }
            continue;
        }

        // Inside diagram block
        if in_diagram {
            if FENCE_CLOSE_RE.is_match(line) {
                diagram_blocks.push(DiagramBlock {
                    source: diagram_lines.join("\n"),
                    style: diagram_style,
                });
                in_diagram = false;
                diagram_lines.clear();
            } else {
                diagram_lines.push(line.to_string());
            }
            continue;
        }

        // Inside code block
        if in_code {
            if FENCE_CLOSE_RE.is_match(line) {
                if code_lang == "mermaid" {
                    // Store as MermaidBlock instead of CodeBlock
                    mermaid_blocks.push(MermaidBlock {
                        source: code_lines.join("\n"),
                    });
                    code_lang.clear();
                    code_label.clear();
                    code_exec_mode = None;
                } else {
                    let block = CodeBlock {
                        language: std::mem::take(&mut code_lang),
                        code: code_lines.join("\n"),
                        label: std::mem::take(&mut code_label),
                        exec_mode: code_exec_mode.take(),
                    };
                    if let Some(col_idx) = current_column {
                        if col_idx < column_contents.len() {
                            column_contents[col_idx].code_blocks.push(block);
                        }
                    } else {
                        code_blocks.push(block);
                    }
                }
                in_code = false;
                code_lines.clear();
            } else {
                code_lines.push(line.to_string());
            }
            continue;
        }

        // ── Fence openings (code blocks and diagrams) ──

        // Diagram fence opening (check before general code fence)
        if let Some(caps) = DIAGRAM_FENCE_RE.captures(line) {
            in_diagram = true;
            diagram_style = match caps.get(1).map(|m| m.as_str()) {
                Some("bracket") => DiagramStyle::Bracket,
                Some("vertical") => DiagramStyle::Vertical,
                _ => DiagramStyle::Box,
            };
            continue;
        }

        // Code fence opening
        if let Some(caps) = FENCE_OPEN_RE.captures(line) {
            in_code = true;
            code_lang = caps.get(1).map_or("", |m| m.as_str()).to_string();
            code_exec_mode = caps.get(2).map(|m| match m.as_str() {
                "+exec" => ExecMode::Exec,
                "+pty" => ExecMode::Pty,
                _ => ExecMode::Exec,
            });
            code_label = caps.get(3).map_or("", |m| m.as_str()).to_string();
            continue;
        }

        // ── Slide metadata directives (section, timing, title style) ──

        // Section directive
        if let Some(caps) = SECTION_RE.captures(line) {
            section = caps[1].to_string();
            continue;
        }

        // Timing directive
        if let Some(caps) = TIMING_RE.captures(line) {
            timing = caps[1].parse().ok();
            continue;
        }

        // ASCII title directive
        if ASCII_TITLE_RE.is_match(line) {
            ascii_title = true;
            continue;
        }

        // ── Font and scaling directives ──

        // Font size directive
        if let Some(caps) = FONT_SIZE_RE.captures(line) {
            font_size = caps[1].parse::<i8>().ok().map(|s| s.clamp(-20, 20));
            continue;
        }

        // Font transition directive
        if let Some(caps) = FONT_TRANSITION_RE.captures(line) {
            font_transition = Some(caps[1].to_string());
            continue;
        }

        // Text scale directive (OSC 66 — scales title + subtitle)
        if let Some(caps) = TEXT_SCALE_RE.captures(line) {
            text_scale = caps[1].parse::<u8>().ok().map(|s| s.clamp(1, 7));
            continue;
        }

        // Title scale directive (OSC 66 — scales title only)
        if let Some(caps) = TITLE_SCALE_RE.captures(line) {
            title_scale = caps[1].parse::<u8>().ok().map(|s| s.clamp(1, 7));
            continue;
        }

        // ── Layout and appearance directives (footer, alignment, decoration) ──

        // Footer directive
        if let Some(caps) = FOOTER_RE.captures(line) {
            footer = Some(caps[1].to_string());
            continue;
        }

        // Footer alignment directive
        if let Some(caps) = FOOTER_ALIGN_RE.captures(line) {
            footer_align = match &caps[1] {
                "center" => FooterAlign::Center,
                "right" => FooterAlign::Right,
                _ => FooterAlign::Left,
            };
            continue;
        }

        // Alignment directive
        if let Some(caps) = ALIGN_RE.captures(line) {
            alignment = match &caps[1] {
                "center" => Some(SlideAlignment::Center),
                "vcenter" => Some(SlideAlignment::VCenter),
                "hcenter" => Some(SlideAlignment::HCenter),
                "top" => Some(SlideAlignment::Top),
                _ => None,
            };
            continue;
        }

        // Title decoration directive
        if let Some(caps) = TITLE_DECORATION_RE.captures(line) {
            title_decoration = Some(caps[1].to_string());
            continue;
        }

        // ── Animation directives (transitions, entrance effects, loops) ──

        // Transition directive
        if let Some(caps) = TRANSITION_RE.captures(line) {
            transition = crate::render::animation::parse_transition(&caps[1]);
            continue;
        }

        // Entrance animation directive
        if let Some(caps) = ANIMATION_RE.captures(line) {
            entrance_animation = crate::render::animation::parse_entrance(&caps[1]);
            continue;
        }

        // Loop animation directive (multiple allowed per slide)
        if let Some(caps) = LOOP_ANIMATION_RE.captures(line) {
            if let Some(la) = crate::render::animation::parse_loop_animation(&caps[1]) {
                let target = caps.get(2).map(|m| m.as_str().to_string());
                loop_animations.push((la, target));
            }
            continue;
        }

        // ── Display mode directives (fullscreen, theme, section visibility) ──

        // Fullscreen directive (<!-- fullscreen --> or <!-- fullscreen: true/false -->)
        if let Some(caps) = FULLSCREEN_RE.captures(line) {
            fullscreen = Some(caps.get(1).is_none_or(|m| m.as_str() != "false"));
            continue;
        }

        // Theme override directive (<!-- theme: slug -->)
        if let Some(caps) = THEME_OVERRIDE_RE.captures(line) {
            theme_override = Some(caps[1].to_string());
            continue;
        }

        // Show section directive (<!-- show_section: true/false -->)
        if let Some(caps) = SHOW_SECTION_RE.captures(line) {
            show_section = Some(caps[1].as_bytes()[0] == b't');
            continue;
        }

        // ── Code preamble directives ──
        // Preambles let authors define reusable import/setup code that gets prepended to
        // executable code blocks of the same language.

        // Preamble start/end directives
        if let Some(caps) = PREAMBLE_START_RE.captures(line) {
            preamble_lang = Some(caps[1].to_string());
            preamble_lines.clear();
            continue;
        }
        if PREAMBLE_END_RE.is_match(line) {
            if let Some(lang) = preamble_lang.take() {
                code_preambles.insert(lang, preamble_lines.join("\n"));
                preamble_lines.clear();
            }
            continue;
        }
        // Accumulate preamble content
        if preamble_lang.is_some() {
            // Lines between preamble_start and preamble_end are raw content (not HTML comments)
            preamble_lines.push(line.to_string());
            continue;
        }

        // ── Image directives (position, render mode, scale, color) ──

        // Image position directive
        if let Some(caps) = IMAGE_POS_RE.captures(line) {
            image_position = match &caps[1] {
                "left" => ImagePosition::Left,
                "right" => ImagePosition::Right,
                _ => ImagePosition::Below,
            };
            continue;
        }

        // Image render mode directive
        if let Some(caps) = IMAGE_RENDER_RE.captures(line) {
            // When inside a column with an image, apply to the column image
            if let Some(col_idx) = current_column {
                if col_idx < column_contents.len() {
                    if let Some(ref mut img) = column_contents[col_idx].image {
                        img.render_mode = Some(caps[1].to_string());
                        continue;
                    }
                }
            }
            image_render = match &caps[1] {
                "ascii" => ImageRenderMode::Ascii,
                "kitty" => ImageRenderMode::Kitty,
                "iterm" | "iterm2" => ImageRenderMode::Iterm,
                "sixel" => ImageRenderMode::Sixel,
                _ => ImageRenderMode::Auto,
            };
            continue;
        }

        // Image scale directive
        if let Some(caps) = IMAGE_SCALE_RE.captures(line) {
            // When inside a column with an image, apply to the column image
            if let Some(col_idx) = current_column {
                if col_idx < column_contents.len() {
                    if let Some(ref mut img) = column_contents[col_idx].image {
                        if let Ok(s) = caps[1].parse::<u8>() {
                            img.scale = Some(s.clamp(1, 100));
                        }
                        continue;
                    }
                }
            }
            if let Ok(s) = caps[1].parse::<u8>() {
                image_scale = s.clamp(1, 100);
            }
            continue;
        }

        // Image color directive
        if let Some(caps) = IMAGE_COLOR_RE.captures(line) {
            // When inside a column with an image, apply to the column image
            if let Some(col_idx) = current_column {
                if col_idx < column_contents.len() {
                    if let Some(ref mut img) = column_contents[col_idx].image {
                        img.color = Some(caps[1].to_string());
                        continue;
                    }
                }
            }
            image_color = caps[1].to_string();
            continue;
        }

        // ── Speaker notes directives ──

        // Multi-line notes start
        if NOTES_MULTI_START_RE.is_match(line) {
            in_notes = true;
            notes_lines.clear();
            continue;
        }

        // Single-line notes
        if let Some(caps) = NOTES_SINGLE_RE.captures(line) {
            notes_lines = vec![caps[1].to_string()];
            continue;
        }

        // ── Column layout directives ──

        // Column layout directive
        if let Some(caps) = COLUMN_LAYOUT_RE.captures(line) {
            let ratios: Vec<u8> = caps[1]
                .split(',')
                .filter_map(|s| s.trim().parse::<u8>().ok())
                .collect();
            if !ratios.is_empty() {
                column_contents = ratios.iter().map(|_| ColumnContent {
                    bullets: Vec::new(),
                    code_blocks: Vec::new(),
                    image: None,
                    text_lines: Vec::new(),
                }).collect();
                column_ratios = Some(ratios);
            }
            continue;
        }

        // Column separator visibility directive
        if let Some(caps) = COLUMN_SEPARATOR_RE.captures(line) {
            if caps[1].eq_ignore_ascii_case("none") {
                column_separator = false;
            }
            continue;
        }

        // Column text scale directive (OSC 66 scaling for non-image columns)
        if let Some(caps) = COLUMN_TEXT_SCALE_RE.captures(line) {
            if let Ok(scale) = caps[1].parse::<u8>() {
                if (2..=7).contains(&scale) {
                    column_text_scale = Some(scale);
                }
            }
            continue;
        }

        // Column switch directive
        if let Some(caps) = COLUMN_RE.captures(line) {
            if let Ok(idx) = caps[1].parse::<usize>() {
                current_column = Some(idx);
            }
            continue;
        }

        // Reset layout directive
        if RESET_LAYOUT_RE.is_match(line) {
            current_column = None;
            continue;
        }

        // ── Catch-all: skip any unrecognized HTML comments ──
        if HTML_COMMENT_RE.is_match(line) {
            continue;
        }

        // ── Markdown content: block quotes, tables, headings, images, bullets ──

        // Block quotes
        if let Some(caps) = BLOCKQUOTE_RE.captures(line) {
            blockquote_lines.push(caps[1].to_string());
            continue;
        } else if !blockquote_lines.is_empty() {
            // End of blockquote block
            block_quotes.push(BlockQuote { lines: std::mem::take(&mut blockquote_lines) });
        }

        // Table parsing
        if let Some(_caps) = TABLE_ROW_RE.captures(line) {
            if TABLE_SEP_RE.is_match(line) {
                // This is the separator row
                if let Some(ref mut state) = table_state {
                    state.alignments = parse_table_alignments(line);
                    state.has_separator = true;
                }
                continue;
            }
            let cells = parse_table_cells(line);
            if let Some(ref mut state) = table_state {
                if state.has_separator {
                    state.rows.push(cells);
                }
                // If no separator yet and we already have headers, this line
                // might be a non-table pipe line — but we wait for separator
            } else {
                // First table row = headers
                table_state = Some(TableParseState {
                    headers: cells,
                    alignments: Vec::new(),
                    rows: Vec::new(),
                    has_separator: false,
                });
            }
            continue;
        } else if let Some(state) = table_state.take() {
            // End of table block
            if state.has_separator {
                tables.push(Table {
                    headers: state.headers,
                    alignments: state.alignments,
                    rows: state.rows,
                });
            }
        }

        // Title (# heading)
        if let Some(caps) = TITLE_RE.captures(line) {
            if !title_found {
                title = caps[1].trim().to_string();
                title_found = true;
                continue;
            }
        }

        // Image
        if let Some(caps) = IMAGE_RE.captures(line) {
            // When inside a column, store the image in the column content
            // instead of at slide level so it renders inline within the column.
            if let Some(col_idx) = current_column {
                if col_idx < column_contents.len() {
                    column_contents[col_idx].image = Some(ColumnImage {
                        path: caps[2].to_string(),
                        render_mode: None,
                        scale: None,
                        color: None,
                    });
                    continue;
                }
            }
            image_alt = caps[1].to_string();
            image_path = caps[2].to_string();
            continue;
        }

        // Bullets
        if let Some(caps) = BULLET_RE.captures(line) {
            let indent = caps[1].len();
            let text = caps[2].trim().to_string();
            // Skip empty bullets (bare `-` or `*` with no text)
            if text.is_empty() { continue; }
            let depth = if indent >= 4 {
                2
            } else if indent >= 2 {
                1
            } else {
                0
            };
            let bullet = Bullet { text, depth };
            if let Some(col_idx) = current_column {
                if col_idx < column_contents.len() {
                    column_contents[col_idx].bullets.push(bullet);
                }
            } else {
                bullets.push(bullet);
            }
            continue;
        }

        // Plain text: non-empty, non-directive lines after the title.
        // - First such line (before any bullets) becomes the subtitle.
        // - Inside a column context, pushed to that column's text_lines.
        // - After bullets have started (outside columns), pushed to trailing_text.
        let stripped = line.trim();
        if !stripped.is_empty() && title_found {
            if let Some(col_idx) = current_column {
                if col_idx < column_contents.len() {
                    column_contents[col_idx].text_lines.push(stripped.to_string());
                }
            } else if !bullets.is_empty() {
                trailing_text.push(stripped.to_string());
            } else if !subtitle_found {
                subtitle = stripped.to_string();
                subtitle_found = true;
            }
        }
    }

    // Flush remaining blockquote
    if !blockquote_lines.is_empty() {
        block_quotes.push(BlockQuote { lines: blockquote_lines });
    }
    // Flush remaining table
    if let Some(state) = table_state {
        if state.has_separator {
            tables.push(Table {
                headers: state.headers,
                alignments: state.alignments,
                rows: state.rows,
            });
        }
    }

    // Resolve section
    if section.is_empty() {
        section = last_section.to_string();
    }
    let current_section = section.clone();

    // Resolve timing
    let timing_minutes = timing.unwrap_or(1.0);

    // Resolve notes
    let notes = notes_lines.join("\n").trim().to_string();

    // Resolve image path
    let image = if !image_path.is_empty() {
        let path = if let Some(base) = base_dir {
            let p = std::path::Path::new(&image_path);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                base.join(p)
            }
        } else {
            std::path::PathBuf::from(&image_path)
        };

        Some(SlideImage {
            path,
            alt_text: image_alt,
            position: image_position,
            render_mode: image_render,
            scale: image_scale,
            color_override: image_color,
        })
    } else {
        None
    };

    // Resolve column image paths (same logic as slide-level image path resolution)
    for content in &mut column_contents {
        if let Some(ref mut col_img) = content.image {
            if !col_img.path.is_empty() {
                let resolved = if std::path::Path::new(&col_img.path).is_absolute() {
                    col_img.path.clone()
                } else if let Some(base) = base_dir {
                    base.join(&col_img.path).to_string_lossy().to_string()
                } else {
                    col_img.path.clone()
                };
                col_img.path = resolved;
            }
        }
    }

    let columns = column_ratios.map(|ratios| ColumnLayout {
        ratios,
        contents: column_contents,
        separator: column_separator,
        text_scale: column_text_scale,
    });

    let slide = Slide {
        number,
        title,
        section,
        subtitle,
        bullets,
        code_blocks,
        image,
        ascii_title,
        notes,
        timing_minutes,
        columns,
        tables,
        block_quotes,
        font_size,
        text_scale,
        title_scale,
        footer,
        footer_align,
        alignment,
        title_decoration,
        transition,
        entrance_animation,
        loop_animations,
        fullscreen,
        show_section,
        code_preambles,
        mermaid_blocks,
        diagram_blocks,
        theme_override,
        font_transition,
        trailing_text,
    };

    (slide, current_section)
}

/// Maximum number of slides allowed in a single presentation file.
/// This limit prevents accidental memory exhaustion from malformed or enormous files.
const MAX_SLIDES: usize = 10_000;

/// Public entry point: parses an entire Markdown presentation source string into metadata and slides.
///
/// # Parsing steps
/// 1. Split the source on `---` line separators.
/// 2. If the file starts with `---`, treat the first block as YAML-like front matter and parse
///    it via `parse_front_matter()`. Otherwise, use default metadata.
/// 3. Each remaining non-empty block is parsed by `parse_slide()` into a `Slide` struct.
///    Sections are inherited from the previous slide when not explicitly set.
///
/// # Parameters
/// - `source`: The full Markdown source text (as read from a `.md` file).
/// - `base_dir`: Optional directory used to resolve relative image paths. Typically the parent
///   directory of the Markdown file.
///
/// # Returns
/// `Ok((PresentationMeta, Vec<Slide>))` on success, or an error if the file exceeds `MAX_SLIDES`.
///
/// # Errors
/// Returns an error if the number of `---`-delimited blocks exceeds `MAX_SLIDES + 2`.
pub fn parse_presentation(source: &str, base_dir: Option<&Path>) -> Result<(PresentationMeta, Vec<Slide>)> {
    // Split on --- separators
    let blocks: Vec<&str> = SLIDE_SEPARATOR_RE.split(source).collect();

    if blocks.len() > MAX_SLIDES + 2 {
        anyhow::bail!("Presentation exceeds maximum of {} slides", MAX_SLIDES);
    }

    let (meta, slide_blocks) = if blocks.len() >= 3 && blocks[0].trim().is_empty() {
        // First block empty = file starts with ---, second is front matter
        let meta = parse_front_matter(blocks[1]);
        (meta, &blocks[2..])
    } else {
        (PresentationMeta::default(), &blocks[..])
    };

    let mut slides = Vec::new();
    let mut last_section = "opening".to_string();
    let mut number = 1;

    for block in slide_blocks {
        if block.trim().is_empty() {
            continue;
        }
        let (slide, section) = parse_slide(block, number, &last_section, base_dir);
        last_section = section;
        slides.push(slide);
        number += 1;
    }

    Ok((meta, slides))
}

#[cfg(test)]
#[path = "parser_tests.rs"]
mod tests;
