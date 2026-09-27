//! Markdown presentation parser: front matter, `---` slide separators, `<!-- name: value -->`
//! directives, and the markdown subset the renderer draws.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::presentation::{
    Block, BlockQuote, Bullet, Callout, Chart, CodeBlock, ColumnContent, ColumnImage, ColumnItem,
    ColumnLayout, DiagramBlock, DiagramStyle, ExecMode, FooterAlign, ImagePosition,
    ImageRenderMode, MermaidBlock, PresentationMeta, Slide, SlideAlignment, SlideImage, Step,
    Table,
};
use crate::render::animation::{
    parse_entrance, parse_loop_animation, parse_transition, LoopAnimation,
};

use super::regex_patterns::*;
use super::split::{opens_comment, split_front_matter, split_slides, Fence};
use super::tables::{parse_table_alignments, parse_table_cells, TableParseState};

pub use super::inline::parse_inline_formatting;

/// Guards against runaway memory use on malformed or generated input.
const MAX_SLIDES: usize = 10_000;

/// Parses a whole deck. `base_dir` resolves relative image paths.
pub fn parse_presentation(
    source: &str,
    base_dir: Option<&Path>,
) -> Result<(PresentationMeta, Vec<Slide>)> {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let lines: Vec<&str> = source.lines().collect();
    let (front_matter, body) = split_front_matter(&lines);
    let meta = front_matter.map(parse_front_matter).unwrap_or_default();

    let mut slides: Vec<Slide> = Vec::new();
    let mut section = String::new();
    for block in split_slides(body) {
        if block.iter().all(|l| l.trim().is_empty()) {
            continue;
        }
        if slides.len() == MAX_SLIDES {
            anyhow::bail!("Presentation exceeds maximum of {} slides", MAX_SLIDES);
        }
        let slide = parse_slide(block, slides.len() + 1, &section, base_dir);
        section.clone_from(&slide.section);
        slides.push(slide);
    }
    Ok((meta, slides))
}

fn parse_front_matter(lines: &[&str]) -> PresentationMeta {
    let mut meta = PresentationMeta::default();
    for caps in lines
        .iter()
        .filter_map(|l| FRONT_MATTER_KV_RE.captures(l.trim()))
    {
        let val = caps[2].trim().trim_matches('"').to_string();
        match &caps[1] {
            "title" => meta.title = val,
            "author" => meta.author = val,
            "date" => meta.date = val,
            "accent" => meta.accent = val,
            "transition" => meta.transition = val,
            "theme" if !val.is_empty() => meta.theme = Some(val),
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

/// Directives that only set a `Slide` field. Unknown names and invalid values are ignored.
fn slide_directive(s: &mut Slide, name: &str, value: Option<&str>) {
    let v = value.unwrap_or("");
    let text = || (!v.is_empty()).then(|| v.to_string());
    match name {
        "section" if !v.is_empty() => s.section = v.to_string(),
        "ascii_title" => s.ascii_title = true,
        "font_size" => set(&mut s.font_size, parse_clamped(v, -20, 20).map(|n| n as i8)),
        "text_scale" => set(&mut s.text_scale, parse_clamped(v, 1, 7).map(|n| n as u8)),
        "footer" => s.footer = Some(v.to_string()),
        "footer_align" => match v {
            "left" => s.footer_align = FooterAlign::Left,
            "center" => s.footer_align = FooterAlign::Center,
            "right" => s.footer_align = FooterAlign::Right,
            _ => {}
        },
        "align" => set(&mut s.alignment, parse_alignment(v)),
        "title_decoration" if matches!(v, "underline" | "box" | "banner" | "none") => {
            s.title_decoration = text();
        }
        "transition" => set(&mut s.transition, parse_transition(v)),
        "animation" => set(&mut s.entrance_animation, parse_entrance(v)),
        "loop_animation" => s.loop_animations.extend(parse_loop_directive(v)),
        "fullscreen" => set(&mut s.fullscreen, value.map_or(Some(true), parse_bool)),
        "show_section" => set(&mut s.show_section, parse_bool(v)),
        "theme" => set(&mut s.theme_override, text()),
        _ => {}
    }
}

/// Overwrites `field` only when the directive value parsed.
fn set<T>(field: &mut Option<T>, value: Option<T>) {
    if value.is_some() {
        *field = value;
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// `# Title` plus `label: value` lines; the value is the leading number of
/// what follows the colon, so units and notes may follow it.
fn parse_chart(source: &str, columns: bool) -> Chart {
    let mut chart = Chart {
        title: None,
        bars: Vec::new(),
        columns,
    };
    for line in source.lines().map(str::trim) {
        if let Some(title) = line.strip_prefix("# ") {
            chart.title = Some(title.trim().to_string());
        } else if let Some((label, rest)) = line.rsplit_once(':') {
            let shown = rest.trim();
            let number: String = shown
                .chars()
                .take_while(|c| c.is_ascii_digit() || matches!(c, '.' | ','))
                .filter(|&c| c != ',')
                .collect();
            if let Ok(value) = number.parse::<f64>() {
                chart
                    .bars
                    .push((label.trim().to_string(), value, shown.to_string()));
            }
        }
    }
    chart
}

/// `[!TIP]` or `[!tip] Custom heading` opening a quote.
fn callout_marker(line: &str) -> Option<(Callout, String)> {
    let rest = line.trim().strip_prefix("[!")?;
    let (kind, heading) = rest.split_once(']')?;
    let kind = match kind.to_ascii_lowercase().as_str() {
        "note" => Callout::Note,
        "tip" => Callout::Tip,
        "important" => Callout::Important,
        "warning" => Callout::Warning,
        "caution" => Callout::Caution,
        _ => return None,
    };
    let heading = heading.trim();
    let heading = if heading.is_empty() {
        kind.name()
    } else {
        heading
    };
    Some((kind, heading.to_string()))
}

/// `Path::join` keeps absolute paths as they are.
fn resolve_path(base_dir: Option<&Path>, path: &str) -> PathBuf {
    base_dir.map_or_else(|| PathBuf::from(path), |base| base.join(path))
}

/// Info string: first word is the language (`diagram` selects the diagram engine), plus
/// optional `+exec`/`+pty`, `style=<name>`, `{label: "..."}` and `{1,3-5|all}` highlights.
fn fence_kind(info: &str) -> FenceKind {
    let mut language = "";
    let mut exec_mode = None;
    let mut style = DiagramStyle::Box;
    for word in info.split('{').next().unwrap_or("").split_whitespace() {
        match word {
            "+exec" => exec_mode = Some(ExecMode::Exec),
            "+pty" => exec_mode = Some(ExecMode::Pty),
            "style=bracket" => style = DiagramStyle::Bracket,
            "style=vertical" => style = DiagramStyle::Vertical,
            _ if language.is_empty() => language = word,
            _ => {}
        }
    }
    match language {
        "diagram" => return FenceKind::Diagram(style),
        "chart" => {
            return FenceKind::Chart {
                columns: info.contains("style=columns"),
            }
        }
        "qr" => return FenceKind::Qr,
        _ => {}
    }
    FenceKind::Code {
        language: language.to_string(),
        label: FENCE_LABEL_RE
            .captures(info)
            .map_or(String::new(), |c| c[1].to_string()),
        exec_mode,
        highlights: info
            .split('{')
            .skip(1)
            .filter_map(|group| parse_highlights(group.split('}').next()?))
            .next()
            .unwrap_or_default(),
    }
}

/// `1,3-5|7|all`: groups separated by `|`, each `all` or comma-separated lines
/// and ranges. Anything else (such as `label: ...`) is not a highlight spec.
fn parse_highlights(spec: &str) -> Option<Vec<Vec<(usize, usize)>>> {
    spec.split('|')
        .map(|group| {
            let group = group.trim();
            if group == "all" {
                return Some(Vec::new());
            }
            group
                .split(',')
                .map(|item| {
                    let (a, b) = item.split_once('-').unwrap_or((item, item));
                    let (a, b) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
                    (1 <= a && a <= b).then_some((a, b))
                })
                .collect()
        })
        .collect()
}

fn parse_slide(
    lines: &[&str],
    number: usize,
    inherited_section: &str,
    base_dir: Option<&Path>,
) -> Slide {
    let mut builder = SlideBuilder::new(number, base_dir);
    for line in lines {
        builder.line(line);
    }
    let mut slide = builder.finish();
    if slide.section.is_empty() {
        slide.section = inherited_section.to_string();
    }
    let mut hasher = std::hash::DefaultHasher::new();
    std::hash::Hash::hash(lines, &mut hasher);
    slide.fingerprint = std::hash::Hasher::finish(&hasher);
    slide
}

/// Multi-line constructs that consume lines until their closing marker. One left open at the
/// end of the slide is closed there.
enum OpenBlock {
    Notes,
    Comment,
    Fence {
        fence: Fence,
        kind: FenceKind,
        lines: Vec<String>,
    },
    Preamble {
        lang: String,
        lines: Vec<String>,
    },
}

enum FenceKind {
    Code {
        language: String,
        label: String,
        exec_mode: Option<ExecMode>,
        highlights: Vec<Vec<(usize, usize)>>,
    },
    Diagram(DiagramStyle),
    Chart {
        columns: bool,
    },
    Qr,
}

struct SlideBuilder<'a> {
    slide: Slide,
    base_dir: Option<&'a Path>,
    open: Option<OpenBlock>,
    title_found: bool,
    /// The previous line was paragraph text, so the next text line continues it.
    paragraph_open: bool,
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
            paragraph_open: false,
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
        let continues_paragraph = std::mem::take(&mut self.paragraph_open);
        if self.open.is_some() {
            self.continue_open_block(line);
        } else if let Some((fence, info)) = Fence::parse(line) {
            self.flush_quote();
            self.flush_table();
            let kind = fence_kind(info);
            let lines = Vec::new();
            self.open = Some(OpenBlock::Fence { fence, kind, lines });
        } else if opens_comment(line) {
            self.open_comment(line);
        } else if line.trim_start().starts_with("<!--") {
            if let Some(caps) = DIRECTIVE_RE.captures(line) {
                self.directive(&caps[1], caps.get(2).map(|m| m.as_str()));
            }
        } else {
            self.content(line, continues_paragraph);
        }
    }

    /// `<!-- notes:` starts multi-line notes (text may begin on the same line); any other
    /// unclosed `<!--` hides lines up to `-->`.
    fn open_comment(&mut self, line: &str) {
        let body = line.trim_start()["<!--".len()..].trim_start();
        match body.strip_prefix("notes:") {
            Some(first) => {
                self.notes.clear();
                if !first.trim().is_empty() {
                    self.notes.push(first.trim().to_string());
                }
                self.open = Some(OpenBlock::Notes);
            }
            None => self.open = Some(OpenBlock::Comment),
        }
    }

    fn continue_open_block(&mut self, line: &str) {
        let closed = match &mut self.open {
            Some(OpenBlock::Notes) => {
                let end = line.find("-->");
                let text = end.map_or(line, |i| line[..i].trim_end());
                if end.is_none() || !text.trim().is_empty() {
                    self.notes.push(text.to_string());
                }
                end.is_some()
            }
            Some(OpenBlock::Comment) => line.contains("-->"),
            Some(OpenBlock::Fence { fence, lines, .. }) => {
                let closed = fence.is_closed_by(line);
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
            Some(OpenBlock::Fence { kind, lines, .. }) => {
                let source = lines.join("\n");
                match kind {
                    FenceKind::Diagram(style) => {
                        self.push_block(Block::Diagram(self.slide.diagram_blocks.len()));
                        self.slide
                            .diagram_blocks
                            .push(DiagramBlock { source, style });
                    }
                    FenceKind::Chart { columns } => {
                        self.push_block(Block::Chart(self.slide.charts.len()));
                        self.slide.charts.push(parse_chart(&source, columns));
                    }
                    FenceKind::Qr => {
                        self.push_block(Block::Qr(self.slide.qr_codes.len()));
                        self.slide.qr_codes.push(source.trim().to_string());
                    }
                    FenceKind::Code { language, .. } if language == "mermaid" => {
                        self.push_block(Block::Mermaid(self.slide.mermaid_blocks.len()));
                        self.slide.mermaid_blocks.push(MermaidBlock { source });
                    }
                    FenceKind::Code {
                        language,
                        label,
                        exec_mode,
                        highlights,
                    } => {
                        let block = CodeBlock {
                            language,
                            code: source,
                            label,
                            exec_mode,
                            highlights,
                        };
                        match self.column_mut() {
                            Some(col) => {
                                col.items.push(ColumnItem::Code(col.code_blocks.len()));
                                col.code_blocks.push(block);
                            }
                            None => {
                                let code = self.slide.code_blocks.len();
                                self.push_block(Block::Code(code));
                                let groups = 1..block.highlights.len();
                                self.slide.code_blocks.push(block);
                                self.slide
                                    .steps
                                    .extend(groups.map(|group| Step::Highlight { code, group }));
                            }
                        }
                    }
                }
            }
            Some(OpenBlock::Preamble { lang, lines }) => {
                self.slide.code_preambles.insert(lang, lines.join("\n"));
            }
            Some(OpenBlock::Notes | OpenBlock::Comment) | None => {}
        }
    }

    fn directive(&mut self, name: &str, value: Option<&str>) {
        let v = value.unwrap_or("");
        match name {
            "notes" => self.notes = vec![v.to_string()],
            "preamble_start" if !v.is_empty() => {
                let lang = v.to_string();
                self.open = Some(OpenBlock::Preamble {
                    lang,
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
                let scale = v.parse().ok().filter(|n| (2..=7).contains(n));
                set(&mut self.columns.text_scale, scale);
            }
            "column" => set(&mut self.column, v.parse().ok()),
            "reset_layout" => self.column = None,
            "pause" if self.column.is_none() => {
                // A quote continues after the pause as a separate block.
                self.flush_quote();
                self.slide.steps.push(Step::Pause(self.slide.blocks.len()));
            }
            _ => slide_directive(&mut self.slide, name, value),
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
            "image_scale" => self.image.scale = scale.unwrap_or(self.image.scale),
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
            if !self.slide.blocks.contains(&Block::Columns) {
                self.push_block(Block::Columns);
            }
            self.columns.contents = vec![ColumnContent::default(); ratios.len()];
            self.columns.ratios = ratios;
        }
    }

    /// The selected column; `None` (slide level) also covers indexes past the layout.
    fn column_mut(&mut self) -> Option<&mut ColumnContent> {
        self.columns.contents.get_mut(self.column?)
    }

    fn content(&mut self, line: &str, continues_paragraph: bool) {
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
        if THEMATIC_BREAK_RE.is_match(line) {
            return;
        }

        if let Some(caps) = HEADING_RE.captures(line) {
            if &caps[1] == "#" && !self.title_found {
                self.slide.title = caps[2].to_string();
                self.title_found = true;
            } else {
                // Other headings stand alone: shown without markers, never merged with text.
                self.text(&format!("**{}**", &caps[2]), false);
                self.paragraph_open = false;
            }
        } else if let Some(caps) = IMAGE_RE.captures(line) {
            self.image_line(&caps[1], &caps[2]);
        } else if let Some(caps) = LIST_ITEM_RE.captures(line) {
            let rest = caps.get(3).map_or("", |m| m.as_str().trim());
            if !rest.is_empty() {
                let marker = &caps[2];
                let ordered = marker.ends_with(['.', ')']);
                let text = if ordered {
                    format!("{marker} {rest}")
                } else {
                    rest.to_string()
                };
                self.bullet(caps[1].len(), text);
            }
        } else {
            self.text(line.trim(), continues_paragraph);
        }
    }

    fn image_line(&mut self, alt: &str, path: &str) {
        let path = resolve_path(self.base_dir, path);
        match self.column_mut() {
            Some(col) => {
                if col.image.is_none() {
                    col.items.push(ColumnItem::Image);
                }
                col.image = Some(ColumnImage {
                    path: path.to_string_lossy().into_owned(),
                    scale: None,
                    color: None,
                });
            }
            None => {
                if !self.slide.blocks.contains(&Block::Image) {
                    self.push_block(Block::Image);
                }
                self.image.alt_text = alt.to_string();
                self.image.path = path;
            }
        }
    }

    fn bullet(&mut self, indent: usize, text: String) {
        let depth = match indent {
            0..=1 => 0,
            2..=3 => 1,
            _ => 2,
        };
        let bullet = Bullet { text, depth };
        if let Some(col) = self.column_mut() {
            col.items.push(ColumnItem::Bullet(col.bullets.len()));
            col.bullets.push(bullet);
            return;
        }
        let s = &mut self.slide;
        let paused_here = s.steps.contains(&Step::Pause(s.blocks.len()));
        match (s.blocks.last(), s.bullet_groups.last_mut()) {
            (Some(Block::Bullets(_)), Some(group)) if !paused_here => group.end += 1,
            _ => {
                let start = s.bullets.len();
                s.blocks.push(Block::Bullets(s.bullet_groups.len()));
                s.bullet_groups.push(start..start + 1);
            }
        }
        s.bullets.push(bullet);
    }

    fn text(&mut self, text: &str, continues_paragraph: bool) {
        if text.is_empty() {
            return;
        }
        if let Some(col) = self.column_mut() {
            col.items.push(ColumnItem::Text(col.text_lines.len()));
            col.text_lines.push(text.to_string());
            return;
        }
        let s = &mut self.slide;
        if continues_paragraph {
            if let Some(paragraph) = s.paragraphs.last_mut() {
                paragraph.push(' ');
                paragraph.push_str(text);
            }
        } else if self.title_found
            && s.subtitle.is_empty()
            && s.blocks.is_empty()
            && s.steps.is_empty()
        {
            s.subtitle = text.to_string();
            return;
        } else {
            s.blocks.push(Block::Paragraph(s.paragraphs.len()));
            s.paragraphs.push(text.to_string());
        }
        self.paragraph_open = true;
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
            let mut lines = std::mem::take(&mut self.quote);
            let callout = callout_marker(&lines[0]);
            if callout.is_some() {
                lines.remove(0);
            }
            let s = &mut self.slide;
            s.blocks.push(Block::Quote(s.block_quotes.len()));
            s.block_quotes.push(BlockQuote { lines, callout });
        }
    }

    fn flush_table(&mut self) {
        if let Some(t) = self.table.take().filter(|t| t.has_separator) {
            let s = &mut self.slide;
            s.blocks.push(Block::Table(s.tables.len()));
            s.tables.push(Table {
                headers: t.headers,
                alignments: t.alignments,
                rows: t.rows,
            });
        }
    }

    /// Quotes and tables end at the next block, so they are recorded first.
    fn push_block(&mut self, block: Block) {
        self.flush_quote();
        self.flush_table();
        self.slide.blocks.push(block);
    }

    fn finish(mut self) -> Slide {
        self.close_open_block();
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
