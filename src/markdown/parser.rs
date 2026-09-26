//! Markdown presentation parser: front matter, `---` slide separators, `<!-- name: value -->`
//! directives, and the markdown subset the renderer draws.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::presentation::{
    BlockQuote, Bullet, CodeBlock, ColumnContent, ColumnImage, ColumnLayout, DiagramBlock,
    DiagramStyle, ExecMode, FooterAlign, ImagePosition, ImageRenderMode, MermaidBlock,
    PresentationMeta, Slide, SlideAlignment, SlideImage, Table,
};
use crate::render::animation::{
    parse_entrance, parse_loop_animation, parse_transition, LoopAnimation,
};

use super::regex_patterns::*;
use super::tables::{parse_table_alignments, parse_table_cells, TableParseState};

pub use super::inline::parse_inline_formatting;

/// Guards against runaway memory use on malformed or generated input.
const MAX_SLIDES: usize = 10_000;

/// Parses a whole deck. `base_dir` resolves relative image paths.
pub fn parse_presentation(
    source: &str,
    base_dir: Option<&Path>,
) -> Result<(PresentationMeta, Vec<Slide>)> {
    let blocks: Vec<&str> = SLIDE_SEPARATOR_RE.split(source).collect();

    if blocks.len() > MAX_SLIDES + 2 {
        anyhow::bail!("Presentation exceeds maximum of {} slides", MAX_SLIDES);
    }

    let (meta, slide_blocks) = if blocks.len() >= 3 && blocks[0].trim().is_empty() {
        (parse_front_matter(blocks[1]), &blocks[2..])
    } else {
        (PresentationMeta::default(), &blocks[..])
    };

    let mut slides: Vec<Slide> = Vec::new();
    let mut section = "opening".to_string();
    for block in slide_blocks.iter().filter(|b| !b.trim().is_empty()) {
        let slide = parse_slide(block, slides.len() + 1, &section, base_dir);
        section.clone_from(&slide.section);
        slides.push(slide);
    }
    Ok((meta, slides))
}

fn parse_front_matter(block: &str) -> PresentationMeta {
    let mut meta = PresentationMeta::default();
    for caps in block
        .lines()
        .filter_map(|l| FRONT_MATTER_KV_RE.captures(l.trim()))
    {
        let val = caps[2].trim().trim_matches('"').to_string();
        match &caps[1] {
            "title" => meta.title = val,
            "author" => meta.author = val,
            "date" => meta.date = val,
            "accent" => meta.accent = val,
            "transition" => meta.transition = val,
            "align" | "alignment" => meta.default_alignment = parse_alignment(&val),
            _ => {}
        }
    }
    meta
}

fn parse_alignment(value: &str) -> Option<SlideAlignment> {
    match value {
        "top" => Some(SlideAlignment::Top),
        "center" => Some(SlideAlignment::Center),
        "vcenter" => Some(SlideAlignment::VCenter),
        "hcenter" => Some(SlideAlignment::HCenter),
        _ => None,
    }
}

fn parse_clamped(value: &str, min: i64, max: i64) -> Option<i64> {
    value.parse::<i64>().ok().map(|n| n.clamp(min, max))
}

/// `sparkle` or `sparkle(figlet)`.
fn parse_loop_directive(value: &str) -> Option<(LoopAnimation, Option<String>)> {
    let (name, target) = match value.split_once('(') {
        Some((name, rest)) => (name, Some(rest.strip_suffix(')')?.to_string())),
        None => (value, None),
    };
    Some((parse_loop_animation(name)?, target))
}

/// `Path::join` keeps absolute paths as they are.
fn resolve_path(base_dir: Option<&Path>, path: &str) -> PathBuf {
    base_dir.map_or_else(|| PathBuf::from(path), |base| base.join(path))
}

fn parse_slide(
    raw: &str,
    number: usize,
    inherited_section: &str,
    base_dir: Option<&Path>,
) -> Slide {
    let mut builder = SlideBuilder::new(number, base_dir);
    for line in raw.lines() {
        builder.line(line);
    }
    let mut slide = builder.finish();
    if slide.section.is_empty() {
        slide.section = inherited_section.to_string();
    }
    slide
}

/// Multi-line constructs that consume lines until their closing marker.
enum OpenBlock {
    Notes,
    Fence { kind: FenceKind, lines: Vec<String> },
    Preamble { lang: String, lines: Vec<String> },
}

enum FenceKind {
    Code {
        language: String,
        label: String,
        exec_mode: Option<ExecMode>,
    },
    Diagram(DiagramStyle),
}

struct SlideBuilder<'a> {
    slide: Slide,
    base_dir: Option<&'a Path>,
    open: Option<OpenBlock>,
    title_found: bool,
    subtitle_found: bool,
    /// Image directives may precede the `![]()` line; kept only once a path is set.
    image: SlideImage,
    /// Column directives may precede `column_layout`; kept only once ratios are set.
    columns: ColumnLayout,
    column: Option<usize>,
    notes: Vec<String>,
    quote: Vec<String>,
    table: Option<TableParseState>,
}

impl<'a> SlideBuilder<'a> {
    fn new(number: usize, base_dir: Option<&'a Path>) -> Self {
        Self {
            slide: Slide {
                number,
                ..Slide::default()
            },
            base_dir,
            open: None,
            title_found: false,
            subtitle_found: false,
            image: SlideImage {
                path: PathBuf::new(),
                alt_text: String::new(),
                position: ImagePosition::Below,
                render_mode: ImageRenderMode::Auto,
                scale: 100,
                color_override: String::new(),
            },
            columns: ColumnLayout {
                ratios: Vec::new(),
                contents: Vec::new(),
                separator: true,
                text_scale: None,
            },
            column: None,
            notes: Vec::new(),
            quote: Vec::new(),
            table: None,
        }
    }

    fn line(&mut self, line: &str) {
        if self.open.is_some() {
            self.continue_open_block(line);
        } else if !self.open_fence(line) {
            if let Some(caps) = DIRECTIVE_RE.captures(line) {
                self.directive(&caps[1], caps.get(2).map(|m| m.as_str()));
            } else if NOTES_MULTI_START_RE.is_match(line) {
                self.notes.clear();
                self.open = Some(OpenBlock::Notes);
            } else if !HTML_COMMENT_RE.is_match(line) {
                self.content(line);
            }
        }
    }

    fn continue_open_block(&mut self, line: &str) {
        let closed = match &mut self.open {
            Some(OpenBlock::Notes) => {
                let end = NOTES_END_RE.find(line);
                let text = end.map_or(line, |m| line[..m.start()].trim_end());
                if end.is_none() || !text.trim().is_empty() {
                    self.notes.push(text.to_string());
                }
                end.is_some()
            }
            Some(OpenBlock::Fence { lines, .. }) => {
                let closed = FENCE_CLOSE_RE.is_match(line);
                if !closed {
                    lines.push(line.to_string());
                }
                closed
            }
            Some(OpenBlock::Preamble { lines, .. }) => {
                let closed = DIRECTIVE_RE
                    .captures(line)
                    .is_some_and(|c| &c[1] == "preamble_end");
                if !closed {
                    lines.push(line.to_string());
                }
                closed
            }
            None => false,
        };
        if closed {
            self.close_open_block();
        }
    }

    fn close_open_block(&mut self) {
        match self.open.take() {
            Some(OpenBlock::Fence { kind, lines }) => {
                let source = lines.join("\n");
                match kind {
                    FenceKind::Diagram(style) => {
                        self.slide
                            .diagram_blocks
                            .push(DiagramBlock { source, style });
                    }
                    FenceKind::Code { language, .. } if language == "mermaid" => {
                        self.slide.mermaid_blocks.push(MermaidBlock { source });
                    }
                    FenceKind::Code {
                        language,
                        label,
                        exec_mode,
                    } => {
                        let block = CodeBlock {
                            language,
                            code: source,
                            label,
                            exec_mode,
                        };
                        match self.column_mut() {
                            Some(col) => col.code_blocks.push(block),
                            None => self.slide.code_blocks.push(block),
                        }
                    }
                }
            }
            Some(OpenBlock::Preamble { lang, lines }) => {
                self.slide.code_preambles.insert(lang, lines.join("\n"));
            }
            Some(OpenBlock::Notes) | None => {}
        }
    }

    fn open_fence(&mut self, line: &str) -> bool {
        let kind = if let Some(caps) = DIAGRAM_FENCE_RE.captures(line) {
            FenceKind::Diagram(match caps.get(1).map(|m| m.as_str()) {
                Some("bracket") => DiagramStyle::Bracket,
                Some("vertical") => DiagramStyle::Vertical,
                _ => DiagramStyle::Box,
            })
        } else if let Some(caps) = FENCE_OPEN_RE.captures(line) {
            FenceKind::Code {
                language: caps[1].to_string(),
                label: caps.get(3).map_or("", |m| m.as_str()).to_string(),
                exec_mode: caps.get(2).map(|m| match m.as_str() {
                    "+pty" => ExecMode::Pty,
                    _ => ExecMode::Exec,
                }),
            }
        } else {
            return false;
        };
        self.open = Some(OpenBlock::Fence {
            kind,
            lines: Vec::new(),
        });
        true
    }

    fn directive(&mut self, name: &str, value: Option<&str>) {
        let v = value.unwrap_or("");
        let s = &mut self.slide;
        match name {
            "section" if !v.is_empty() => s.section = v.to_string(),
            "ascii_title" => s.ascii_title = true,
            "font_size" => {
                if let Some(n) = parse_clamped(v, -20, 20) {
                    s.font_size = Some(n as i8);
                }
            }
            "font_transition" if !v.is_empty() => s.font_transition = Some(v.to_string()),
            "text_scale" => {
                if let Some(n) = parse_clamped(v, 1, 7) {
                    s.text_scale = Some(n as u8);
                }
            }
            "footer" => s.footer = Some(v.to_string()),
            "footer_align" => match v {
                "left" => s.footer_align = FooterAlign::Left,
                "center" => s.footer_align = FooterAlign::Center,
                "right" => s.footer_align = FooterAlign::Right,
                _ => {}
            },
            "align" => {
                if let Some(a) = parse_alignment(v) {
                    s.alignment = Some(a);
                }
            }
            "title_decoration" if matches!(v, "underline" | "box" | "banner" | "none") => {
                s.title_decoration = Some(v.to_string());
            }
            "transition" => {
                if let Some(t) = parse_transition(v) {
                    s.transition = Some(t);
                }
            }
            "animation" => {
                if let Some(a) = parse_entrance(v) {
                    s.entrance_animation = Some(a);
                }
            }
            "loop_animation" => {
                if let Some(la) = parse_loop_directive(v) {
                    s.loop_animations.push(la);
                }
            }
            "fullscreen" => match value {
                None | Some("true") => s.fullscreen = Some(true),
                Some("false") => s.fullscreen = Some(false),
                _ => {}
            },
            "show_section" => match v {
                "true" => s.show_section = Some(true),
                "false" => s.show_section = Some(false),
                _ => {}
            },
            "theme" if !v.is_empty() => s.theme_override = Some(v.to_string()),
            "notes" => self.notes = vec![v.to_string()],
            "preamble_start" if !v.is_empty() => {
                self.open = Some(OpenBlock::Preamble {
                    lang: v.to_string(),
                    lines: Vec::new(),
                });
            }
            "image_position" => match v {
                "left" => self.image.position = ImagePosition::Left,
                "right" => self.image.position = ImagePosition::Right,
                _ => {}
            },
            "image_render" | "image_scale" | "image_color" if !v.is_empty() => {
                self.image_directive(name, v);
            }
            "column_layout" => self.column_layout(v),
            "column_separator" if v.eq_ignore_ascii_case("none") => self.columns.separator = false,
            "column_text_scale" => {
                if let Some(n) = v.parse::<u8>().ok().filter(|n| (2..=7).contains(n)) {
                    self.columns.text_scale = Some(n);
                }
            }
            "column" => {
                if let Ok(i) = v.parse() {
                    self.column = Some(i);
                }
            }
            "reset_layout" => self.column = None,
            _ => {}
        }
    }

    /// Image directives after a column image apply to that image.
    fn image_directive(&mut self, name: &str, v: &str) {
        let scale = parse_clamped(v, 1, 100).map(|n| n as u8);
        if let Some(img) = self.column_mut().and_then(|c| c.image.as_mut()) {
            match name {
                "image_scale" if scale.is_some() => img.scale = scale,
                "image_color" => img.color = Some(v.to_string()),
                // Column images always render as ASCII.
                _ => {}
            }
            return;
        }
        match name {
            "image_render" => {
                self.image.render_mode = match v {
                    "ascii" => ImageRenderMode::Ascii,
                    "kitty" => ImageRenderMode::Kitty,
                    "iterm" | "iterm2" => ImageRenderMode::Iterm,
                    "sixel" => ImageRenderMode::Sixel,
                    _ => return,
                };
            }
            "image_scale" => {
                if let Some(scale) = scale {
                    self.image.scale = scale;
                }
            }
            _ => self.image.color_override = v.to_string(),
        }
    }

    fn column_layout(&mut self, v: &str) {
        let ratios: Vec<u8> = v
            .trim_start_matches('[')
            .trim_end_matches(']')
            .split(',')
            .filter_map(|r| r.trim().parse().ok())
            .collect();
        if !ratios.is_empty() {
            self.columns.contents = vec![ColumnContent::default(); ratios.len()];
            self.columns.ratios = ratios;
        }
    }

    /// The selected column; `None` (slide level) also covers indexes past the layout.
    fn column_mut(&mut self) -> Option<&mut ColumnContent> {
        self.columns.contents.get_mut(self.column?)
    }

    fn content(&mut self, line: &str) {
        if let Some(caps) = BLOCKQUOTE_RE.captures(line) {
            self.quote.push(caps[1].to_string());
            return;
        }
        self.flush_quote();
        if TABLE_ROW_RE.is_match(line) {
            self.table_row(line);
            return;
        }
        self.flush_table();

        if let Some(caps) = TITLE_RE.captures(line).filter(|_| !self.title_found) {
            self.slide.title = caps[1].trim().to_string();
            self.title_found = true;
        } else if let Some(caps) = IMAGE_RE.captures(line) {
            self.image_line(&caps[1], &caps[2]);
        } else if let Some(caps) = BULLET_RE.captures(line) {
            let text = caps[2].trim();
            if !text.is_empty() {
                self.bullet(caps[1].len(), text);
            }
        } else {
            self.text(line.trim());
        }
    }

    fn image_line(&mut self, alt: &str, path: &str) {
        let path = resolve_path(self.base_dir, path);
        match self.column_mut() {
            Some(col) => {
                col.image = Some(ColumnImage {
                    path: path.to_string_lossy().into_owned(),
                    scale: None,
                    color: None,
                });
            }
            None => {
                self.image.alt_text = alt.to_string();
                self.image.path = path;
            }
        }
    }

    fn bullet(&mut self, indent: usize, text: &str) {
        let depth = match indent {
            0..=1 => 0,
            2..=3 => 1,
            _ => 2,
        };
        let bullet = Bullet {
            text: text.to_string(),
            depth,
        };
        match self.column_mut() {
            Some(col) => col.bullets.push(bullet),
            None => self.slide.bullets.push(bullet),
        }
    }

    fn text(&mut self, text: &str) {
        if text.is_empty() || !self.title_found {
            return;
        }
        if let Some(col) = self.column_mut() {
            col.text_lines.push(text.to_string());
        } else if !self.slide.bullets.is_empty() {
            self.slide.trailing_text.push(text.to_string());
        } else if !self.subtitle_found {
            self.slide.subtitle = text.to_string();
            self.subtitle_found = true;
        }
    }

    fn table_row(&mut self, line: &str) {
        let is_separator = TABLE_SEP_RE.is_match(line);
        match &mut self.table {
            Some(table) if is_separator => {
                table.alignments = parse_table_alignments(line);
                table.has_separator = true;
            }
            Some(table) if table.has_separator => table.rows.push(parse_table_cells(line)),
            None if !is_separator => {
                self.table = Some(TableParseState {
                    headers: parse_table_cells(line),
                    alignments: Vec::new(),
                    rows: Vec::new(),
                    has_separator: false,
                });
            }
            _ => {}
        }
    }

    fn flush_quote(&mut self) {
        if !self.quote.is_empty() {
            let lines = std::mem::take(&mut self.quote);
            self.slide.block_quotes.push(BlockQuote { lines });
        }
    }

    fn flush_table(&mut self) {
        if let Some(t) = self.table.take().filter(|t| t.has_separator) {
            self.slide.tables.push(Table {
                headers: t.headers,
                alignments: t.alignments,
                rows: t.rows,
            });
        }
    }

    fn finish(mut self) -> Slide {
        self.flush_quote();
        self.flush_table();
        self.slide.notes = self.notes.join("\n").trim().to_string();
        if !self.image.path.as_os_str().is_empty() {
            self.slide.image = Some(self.image);
        }
        if !self.columns.ratios.is_empty() {
            self.slide.columns = Some(self.columns);
        }
        self.slide
    }
}

#[cfg(test)]
#[path = "parser_tests.rs"]
mod tests;
