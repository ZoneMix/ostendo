use anyhow::Result;
use regex::Regex;
use std::path::Path;
use std::sync::LazyLock;

use crate::presentation::{
    BlockQuote, Bullet, CodeBlock, ColumnContent, ColumnLayout, ExecMode, FooterAlign,
    ImagePosition, ImageRenderMode, MermaidBlock, PresentationMeta, Slide, SlideAlignment,
    SlideImage, Table, TableAlign,
};

static FENCE_OPEN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^```(\w*)\s*(\+exec|\+pty)?\s*(?:\{label:\s*"([^"]*)"\s*\})?\s*$"#).unwrap()
});
static FENCE_CLOSE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^```\s*$").unwrap());

static SECTION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*section:\s*(\S+)\s*-->").unwrap());
static TIMING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*timing:\s*([\d.]+)\s*-->").unwrap());
static ASCII_TITLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*ascii_title\s*-->").unwrap());
static IMAGE_POS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*image_position:\s*(left|right)\s*-->").unwrap());
static IMAGE_RENDER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*image_render:\s*(ascii|kitty|iterm2?|sixel)\s*-->").unwrap());
static IMAGE_SCALE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*image_scale:\s*(\d+)\s*-->").unwrap());
static IMAGE_COLOR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*image_color:\s*(\S+)\s*-->").unwrap());
static NOTES_MULTI_START_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*notes:\s*$").unwrap());
static NOTES_SINGLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*notes:\s*(.*?)\s*-->\s*$").unwrap());
static NOTES_END_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"-->\s*$").unwrap());
static HTML_COMMENT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--.*-->\s*$").unwrap());
static COLUMN_LAYOUT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*column_layout:\s*\[([^\]]+)\]\s*-->").unwrap());
static COLUMN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*column:\s*(\d+)\s*-->").unwrap());
static RESET_LAYOUT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*reset_layout\s*-->").unwrap());
static FONT_SIZE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*font_size:\s*(\d+)\s*-->").unwrap());
static TITLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^#\s+(.+)$").unwrap());
static IMAGE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^!\[([^\]]*)\]\(([^)]+)\)\s*$").unwrap());
static BULLET_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)[-*]\s*(.*)$").unwrap());
static SLIDE_SEPARATOR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^---\s*$").unwrap());
static TABLE_ROW_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\|(.+)\|\s*$").unwrap());
static TABLE_SEP_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\|[\s:]*-+[\s:]*(\|[\s:]*-+[\s:]*)*\|\s*$").unwrap());
static BLOCKQUOTE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^>\s?(.*)$").unwrap());
static FOOTER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*footer:\s*(.*?)\s*-->").unwrap());
static FOOTER_ALIGN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*footer_align:\s*(left|center|right)\s*-->").unwrap());
static ALIGN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*align:\s*(top|center|vcenter|hcenter)\s*-->").unwrap());
static TITLE_DECORATION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*title_decoration:\s*(underline|box|banner|none)\s*-->").unwrap());
static TRANSITION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*transition:\s*(fade|slide|dissolve)\s*-->").unwrap());
static ANIMATION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*animation:\s*(typewriter|fade_in|slide_down)\s*-->").unwrap());
static LOOP_ANIMATION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*loop_animation:\s*(matrix|bounce|pulse|sparkle|spin)\s*-->").unwrap());
static PREAMBLE_START_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*preamble_start:\s*(\w+)\s*-->").unwrap());
static PREAMBLE_END_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*preamble_end\s*-->").unwrap());

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

struct TableParseState {
    headers: Vec<String>,
    alignments: Vec<TableAlign>,
    rows: Vec<Vec<String>>,
    has_separator: bool,
}

fn parse_table_cells(row: &str) -> Vec<String> {
    row.split('|')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn parse_table_alignments(sep_row: &str) -> Vec<TableAlign> {
    sep_row
        .split('|')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| {
            let left = s.starts_with(':');
            let right = s.ends_with(':');
            match (left, right) {
                (true, true) => TableAlign::Center,
                (false, true) => TableAlign::Right,
                _ => TableAlign::Left,
            }
        })
        .collect()
}

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
    let mut font_size: Option<u8> = None;
    let mut footer: Option<String> = None;
    let mut footer_align = FooterAlign::Left;
    let mut alignment: Option<SlideAlignment> = None;
    let mut title_decoration: Option<String> = None;
    let mut transition: Option<String> = None;
    let mut entrance_animation: Option<String> = None;
    let mut loop_animation: Option<String> = None;
    let mut code_preambles: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut preamble_lang: Option<String> = None;
    let mut preamble_lines: Vec<String> = Vec::new();
    let mut mermaid_blocks: Vec<MermaidBlock> = Vec::new();

    let mut in_notes = false;
    let mut in_code = false;
    let mut code_lang = String::new();
    let mut code_label = String::new();
    let mut code_exec_mode: Option<ExecMode> = None;
    let mut code_lines: Vec<String> = Vec::new();
    let mut title_found = false;
    let mut subtitle_found = false;
    let mut column_ratios: Option<Vec<u8>> = None;
    let mut column_contents: Vec<ColumnContent> = Vec::new();
    let mut current_column: Option<usize> = None;
    let mut tables: Vec<Table> = Vec::new();
    let mut block_quotes: Vec<BlockQuote> = Vec::new();
    let mut table_state: Option<TableParseState> = None;
    let mut blockquote_lines: Vec<String> = Vec::new();

    for line in raw.lines() {
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

        // Font size directive
        if let Some(caps) = FONT_SIZE_RE.captures(line) {
            font_size = caps[1].parse::<u8>().ok().map(|s| s.clamp(1, 7));
            continue;
        }

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

        // Transition directive
        if let Some(caps) = TRANSITION_RE.captures(line) {
            transition = Some(caps[1].to_string());
            continue;
        }

        // Entrance animation directive
        if let Some(caps) = ANIMATION_RE.captures(line) {
            entrance_animation = Some(caps[1].to_string());
            continue;
        }

        // Loop animation directive
        if let Some(caps) = LOOP_ANIMATION_RE.captures(line) {
            loop_animation = Some(caps[1].to_string());
            continue;
        }

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
            if let Ok(s) = caps[1].parse::<u8>() {
                image_scale = s.clamp(1, 100);
            }
            continue;
        }

        // Image color directive
        if let Some(caps) = IMAGE_COLOR_RE.captures(line) {
            image_color = caps[1].to_string();
            continue;
        }

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
                }).collect();
                column_ratios = Some(ratios);
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

        // Skip other HTML comments
        if HTML_COMMENT_RE.is_match(line) {
            continue;
        }

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

        // Subtitle: first non-empty, non-directive line after title
        let stripped = line.trim();
        if !stripped.is_empty() && title_found && !subtitle_found {
            subtitle = stripped.to_string();
            subtitle_found = true;
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

    let columns = column_ratios.map(|ratios| ColumnLayout {
        ratios,
        contents: column_contents,
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
        footer,
        footer_align,
        alignment,
        title_decoration,
        transition,
        entrance_animation,
        loop_animation,
        code_preambles,
        mermaid_blocks,
    };

    (slide, current_section)
}

/// Maximum number of slides allowed in a presentation.
const MAX_SLIDES: usize = 10_000;

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

/// Parse inline markdown formatting into styled spans.
/// Handles **bold**, *italic*, ~~strikethrough~~, and `inline code`.
pub fn parse_inline_formatting(
    text: &str,
    base_fg: crossterm::style::Color,
    code_bg: crossterm::style::Color,
) -> Vec<crate::render::text::StyledSpan> {
    use crate::render::text::StyledSpan;

    let mut spans = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut pos = 0;
    let mut current = String::new();

    while pos < chars.len() {
        // Check for ** (bold) — recursively handles *italic* inside bold
        if pos + 1 < chars.len() && chars[pos] == '*' && chars[pos + 1] == '*' {
            if !current.is_empty() {
                spans.push(StyledSpan::new(&current).with_fg(base_fg));
                current.clear();
            }
            pos += 2;
            let mut inner = String::new();
            while pos + 1 < chars.len() && !(chars[pos] == '*' && chars[pos + 1] == '*') {
                inner.push(chars[pos]);
                pos += 1;
            }
            if pos + 1 < chars.len() { pos += 2; } // skip closing **
            if !inner.is_empty() {
                // Parse inner content for nested italic (*...*) within bold
                let inner_chars: Vec<char> = inner.chars().collect();
                let mut ipos = 0;
                let mut plain = String::new();
                while ipos < inner_chars.len() {
                    if inner_chars[ipos] == '*' {
                        if !plain.is_empty() {
                            spans.push(StyledSpan::new(&plain).with_fg(base_fg).bold());
                            plain.clear();
                        }
                        ipos += 1;
                        let mut italic_text = String::new();
                        while ipos < inner_chars.len() && inner_chars[ipos] != '*' {
                            italic_text.push(inner_chars[ipos]);
                            ipos += 1;
                        }
                        if ipos < inner_chars.len() { ipos += 1; } // skip closing *
                        if !italic_text.is_empty() {
                            spans.push(StyledSpan::new(&italic_text).with_fg(base_fg).bold().italic());
                        }
                    } else {
                        plain.push(inner_chars[ipos]);
                        ipos += 1;
                    }
                }
                if !plain.is_empty() {
                    spans.push(StyledSpan::new(&plain).with_fg(base_fg).bold());
                }
            }
            continue;
        }
        // Check for ~~ (strikethrough)
        if pos + 1 < chars.len() && chars[pos] == '~' && chars[pos + 1] == '~' {
            if !current.is_empty() {
                spans.push(StyledSpan::new(&current).with_fg(base_fg));
                current.clear();
            }
            pos += 2;
            let mut inner = String::new();
            while pos + 1 < chars.len() && !(chars[pos] == '~' && chars[pos + 1] == '~') {
                inner.push(chars[pos]);
                pos += 1;
            }
            if pos + 1 < chars.len() { pos += 2; }
            if !inner.is_empty() {
                spans.push(StyledSpan::new(&inner).with_fg(base_fg).strikethrough());
            }
            continue;
        }
        // Check for ` (inline code)
        if chars[pos] == '`' {
            if !current.is_empty() {
                spans.push(StyledSpan::new(&current).with_fg(base_fg));
                current.clear();
            }
            pos += 1;
            let mut inner = String::new();
            while pos < chars.len() && chars[pos] != '`' {
                inner.push(chars[pos]);
                pos += 1;
            }
            if pos < chars.len() { pos += 1; } // skip closing `
            if !inner.is_empty() {
                spans.push(StyledSpan::new(&format!(" {} ", inner)).with_fg(base_fg).with_bg(code_bg));
            }
            continue;
        }
        // Check for * or _ (italic) — single, not double
        if (chars[pos] == '*' || chars[pos] == '_')
            && (pos + 1 >= chars.len() || chars[pos + 1] != chars[pos])
        {
            let marker = chars[pos];
            if !current.is_empty() {
                spans.push(StyledSpan::new(&current).with_fg(base_fg));
                current.clear();
            }
            pos += 1;
            let mut inner = String::new();
            while pos < chars.len() && chars[pos] != marker {
                inner.push(chars[pos]);
                pos += 1;
            }
            if pos < chars.len() { pos += 1; } // skip closing marker
            if !inner.is_empty() {
                spans.push(StyledSpan::new(&inner).with_fg(base_fg).italic());
            }
            continue;
        }
        current.push(chars[pos]);
        pos += 1;
    }

    if !current.is_empty() {
        spans.push(StyledSpan::new(&current).with_fg(base_fg));
    }

    // If no formatting was found, return a single span
    if spans.is_empty() {
        spans.push(StyledSpan::new(text).with_fg(base_fg));
    }

    spans
}

#[cfg(test)]
mod tests {
    use super::*;

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
            assert!(quote_slides.len() >= 1, "Expected at least 1 slide with block quotes");
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
        assert_eq!(slides[0].transition.as_deref(), Some("dissolve"));
    }

    #[test]
    fn test_animation_directives() {
        let src = "<!-- animation: typewriter -->\n<!-- loop_animation: matrix -->\n# Animated";
        let slides = parse(src);
        assert_eq!(slides[0].entrance_animation.as_deref(), Some("typewriter"));
        assert_eq!(slides[0].loop_animation.as_deref(), Some("matrix"));
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
}
