//! PowerPoint export: one slide per slide, in the theme's colors, with
//! speaker notes. Text stays editable; charts become shapes, and images, QR
//! codes, and SVGs become pictures. Content that would not fit shrinks,
//! images first.

use std::fmt::Write as _;
use std::path::Path;

use anyhow::Result;
use crossterm::style::Color;

use super::zip::Zip;
use crate::presentation::{
    Block, BlockQuote, Bullet, Callout, Chart, ColumnContent, ColumnItem, PresentationMeta, Slide,
    Table,
};
use crate::theme::colors::{color_to_hex, hex_to_color, interpolate_color};
use crate::theme::Theme;

/// English Metric Units: 914,400 per inch, 12,700 per point.
const PT: f64 = 12_700.0;
const INCH: f64 = 914_400.0;
const SLIDE_W: f64 = 12_192_000.0;
const SLIDE_H: f64 = 7.5 * INCH;
const MARGIN: f64 = 0.5 * INCH;
const GAP: f64 = 0.12 * INCH;
const MONO: &str = "Consolas";

pub fn export_pptx(
    slides: &[Slide],
    meta: &PresentationMeta,
    theme: &Theme,
    title: &str,
    output: &Path,
) -> Result<()> {
    let colors = Colors::new(theme);
    let mut zip = Zip::new();
    let mut media = Vec::new();
    let mut parts = Vec::new();
    for (i, slide) in slides.iter().enumerate() {
        let n = i + 1;
        let mut page = Page::new(&colors, &mut media);
        let (xml, rels) = page.slide(slide);
        parts.push((
            format!("ppt/slides/slide{n}.xml"),
            xml,
            format!("ppt/slides/_rels/slide{n}.xml.rels"),
            rels,
        ));
        parts.push((
            format!("ppt/notesSlides/notesSlide{n}.xml"),
            notes_slide(&slide.notes),
            format!("ppt/notesSlides/_rels/notesSlide{n}.xml.rels"),
            relationships(&[
                (
                    NOTES_MASTER,
                    "../notesMasters/notesMaster1.xml".into(),
                    false,
                ),
                (SLIDE, format!("../slides/slide{n}.xml"), false),
            ]),
        ));
    }

    zip.add(
        "[Content_Types].xml",
        content_types(slides.len()).as_bytes(),
    )?;
    zip.add("_rels/.rels", ROOT_RELS.as_bytes())?;
    zip.add(
        "docProps/core.xml",
        core_props(title, &meta.author).as_bytes(),
    )?;
    zip.add("docProps/app.xml", APP_PROPS.as_bytes())?;
    zip.add(
        "ppt/presentation.xml",
        presentation(slides.len()).as_bytes(),
    )?;
    let mut rels = vec![
        (
            SLIDE_MASTER,
            "slideMasters/slideMaster1.xml".to_string(),
            false,
        ),
        (NOTES_MASTER, "notesMasters/notesMaster1.xml".into(), false),
        (THEME, "theme/theme1.xml".into(), false),
        (PRES_PROPS, "presProps.xml".into(), false),
        (VIEW_PROPS, "viewProps.xml".into(), false),
        (TABLE_STYLES, "tableStyles.xml".into(), false),
    ];
    rels.extend((1..=slides.len()).map(|n| (SLIDE, format!("slides/slide{n}.xml"), false)));
    zip.add(
        "ppt/_rels/presentation.xml.rels",
        relationships(&rels).as_bytes(),
    )?;
    zip.add("ppt/presProps.xml", PRES_PROPS_XML.as_bytes())?;
    zip.add("ppt/viewProps.xml", VIEW_PROPS_XML.as_bytes())?;
    zip.add("ppt/tableStyles.xml", TABLE_STYLES_XML.as_bytes())?;
    zip.add(
        "ppt/theme/theme1.xml",
        theme_xml(&colors.bg, &colors.text, &colors.accent).as_bytes(),
    )?;
    zip.add(
        "ppt/theme/theme2.xml",
        theme_xml("FFFFFF", "000000", &colors.accent).as_bytes(),
    )?;
    zip.add(
        "ppt/slideMasters/slideMaster1.xml",
        slide_master(&colors).as_bytes(),
    )?;
    zip.add(
        "ppt/slideMasters/_rels/slideMaster1.xml.rels",
        relationships(&[
            (
                SLIDE_LAYOUT,
                "../slideLayouts/slideLayout1.xml".into(),
                false,
            ),
            (THEME, "../theme/theme1.xml".into(), false),
        ])
        .as_bytes(),
    )?;
    zip.add(
        "ppt/slideLayouts/slideLayout1.xml",
        SLIDE_LAYOUT_XML.as_bytes(),
    )?;
    zip.add(
        "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
        relationships(&[(
            SLIDE_MASTER,
            "../slideMasters/slideMaster1.xml".into(),
            false,
        )])
        .as_bytes(),
    )?;
    zip.add(
        "ppt/notesMasters/notesMaster1.xml",
        NOTES_MASTER_XML.as_bytes(),
    )?;
    zip.add(
        "ppt/notesMasters/_rels/notesMaster1.xml.rels",
        relationships(&[(THEME, "../theme/theme2.xml".into(), false)]).as_bytes(),
    )?;
    for (xml_path, xml, rels_path, rels) in &parts {
        zip.add(xml_path, xml.as_bytes())?;
        zip.add(rels_path, rels.as_bytes())?;
    }
    for (name, bytes) in &media {
        zip.add(&format!("ppt/media/{name}"), bytes)?;
    }
    std::fs::write(output, zip.finish()?)?;
    Ok(())
}

/// Theme colors as `RRGGBB`.
struct Colors {
    bg: String,
    text: String,
    accent: String,
    muted: String,
    code_bg: String,
}

impl Colors {
    fn new(theme: &Theme) -> Self {
        let parse = |hex: &str, fallback| hex_to_color(hex).unwrap_or(fallback);
        let bg = parse(&theme.colors.background, Color::Black);
        let text = parse(&theme.colors.text, Color::White);
        let hex = |c: Color| color_to_hex(c).trim_start_matches('#').to_uppercase();
        Self {
            bg: hex(bg),
            text: hex(text),
            accent: hex(parse(&theme.colors.accent, text)),
            muted: hex(interpolate_color(text, bg, 0.4)),
            code_bg: hex(parse(&theme.colors.code_background, bg)),
        }
    }
}

/// A paragraph of runs; sizes are points before any shrinking.
struct Para {
    runs: Vec<Run>,
    size: f64,
    /// `l`, `ctr`, or `r`; `None` follows the text box.
    align: Option<&'static str>,
    /// Nesting depth and bullet glyph; `Some((depth, None))` indents without one.
    bullet: Option<(usize, Option<char>)>,
    color: String,
    mono: bool,
    space_before: f64,
}

struct Run {
    text: String,
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    link: Option<String>,
}

impl Run {
    fn plain(text: &str) -> Self {
        Run {
            text: text.to_string(),
            bold: false,
            italic: false,
            strike: false,
            code: false,
            link: None,
        }
    }
}

struct Picture {
    file: String,
    width: f64,
    height: f64,
    alt: String,
    /// Tallest it may be drawn, before shrinking.
    max_height: f64,
}

enum Item {
    Text(Vec<Para>),
    /// Code, on the code background.
    Code(Vec<Para>),
    /// Display math: monospaced rows centered as one block.
    Math(Vec<Para>),
    Table(Table),
    Chart(Chart),
    Picture(Picture),
    Columns(Vec<(f64, Vec<Item>)>),
}

/// Builds one slide's shapes, and the files and links they refer to.
struct Page<'a> {
    colors: &'a Colors,
    media: &'a mut Vec<(String, Vec<u8>)>,
    /// Relationship type, target, external; `rId` is the index + 1.
    rels: Vec<(&'static str, String, bool)>,
    shapes: String,
    next_id: usize,
}

impl<'a> Page<'a> {
    fn new(colors: &'a Colors, media: &'a mut Vec<(String, Vec<u8>)>) -> Self {
        Self {
            colors,
            media,
            rels: Vec::new(),
            shapes: String::new(),
            next_id: 2,
        }
    }

    fn rel(&mut self, kind: &'static str, target: String, external: bool) -> String {
        self.rels.push((kind, target, external));
        format!("rId{}", self.rels.len())
    }

    fn id(&mut self) -> usize {
        self.next_id += 1;
        self.next_id - 1
    }

    fn slide(&mut self, slide: &Slide) -> (String, String) {
        let n = slide.number;
        self.rel(
            SLIDE_LAYOUT,
            "../slideLayouts/slideLayout1.xml".into(),
            false,
        );
        self.rel(
            NOTES_SLIDE,
            format!("../notesSlides/notesSlide{n}.xml"),
            false,
        );

        let mut top = MARGIN;
        if !slide.title.is_empty() {
            let title = escape(&slide.title);
            let id = self.id();
            let _ = write!(
                self.shapes,
                "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"Title\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"title\"/></p:nvPr></p:nvSpPr>\
                 <p:spPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm></p:spPr>\
                 <p:txBody><a:bodyPr anchor=\"b\" lIns=\"0\" rIns=\"0\"><a:normAutofit/></a:bodyPr><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\" sz=\"3200\" b=\"1\"><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill></a:rPr><a:t>{title}</a:t></a:r></a:p></p:txBody></p:sp>",
                emu(MARGIN),
                emu(0.35 * INCH),
                emu(SLIDE_W - 2.0 * MARGIN),
                emu(0.95 * INCH),
                self.colors.accent,
            );
            top = 1.45 * INCH;
        }

        let mut items = Vec::new();
        if !slide.subtitle.is_empty() {
            let mut p = self.para(&slide.subtitle, 22.0);
            p.color = self.colors.muted.clone();
            items.push(Item::Text(vec![p]));
        }
        for block in &slide.blocks {
            if let Some(item) = self.block(slide, *block) {
                push_item(&mut items, item);
            }
        }
        let area = (MARGIN, top, SLIDE_W - 2.0 * MARGIN, SLIDE_H - MARGIN - top);
        self.place(&items, area);

        let xml = format!(
            "{XML_HEAD}<p:sld {NS}><p:cSld><p:spTree>{GROUP}{}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>",
            self.shapes
        );
        (xml, relationships(&self.rels))
    }

    fn block(&mut self, slide: &Slide, block: Block) -> Option<Item> {
        Some(match block {
            Block::Paragraph(i) => Item::Text(vec![self.para(&slide.paragraphs[i], 20.0)]),
            Block::Bullets(i) => {
                Item::Text(self.bullets(&slide.bullets[slide.bullet_groups[i].clone()]))
            }
            Block::Code(i) => {
                let cb = &slide.code_blocks[i];
                self.code(&cb.label, &cb.code)
            }
            Block::Table(i) => Item::Table(slide.tables[i].clone()),
            Block::Quote(i) => Item::Text(self.quote(&slide.block_quotes[i])),
            Block::Diagram(i) => {
                let d = &slide.diagram_blocks[i];
                let graph = crate::diagram::parser::parse(&d.source);
                let c = Color::Reset;
                let lines = crate::diagram::render_adaptive(&graph, d.style, 110, c, c, c, "");
                let rows: Vec<String> = lines
                    .iter()
                    .map(|l| l.spans.iter().map(|s| s.text.as_str()).collect())
                    .collect();
                Item::Math(rows.iter().map(|r| self.mono(r, 14.0)).collect())
            }
            Block::Mermaid(i) => self.code("mermaid", &slide.mermaid_blocks[i].source),
            Block::Chart(i) => Item::Chart(slide.charts[i].clone()),
            Block::Qr(i) => {
                let file = self.qr(&slide.qr_codes[i])?;
                Item::Picture(Picture {
                    width: 1.0,
                    height: 1.0,
                    alt: slide.qr_codes[i].clone(),
                    max_height: 2.6 * INCH,
                    file,
                })
            }
            Block::Math(i) => self.math(&slide.math[i]),
            Block::Poll(i) => {
                let poll = &slide.polls[i];
                let mut paras = vec![self.para(&format!("**{}**", poll.question), 22.0)];
                for (n, option) in poll.options.iter().enumerate() {
                    let mut p = self.para(&format!("{}. {option}", n + 1), 20.0);
                    p.bullet = Some((0, None));
                    paras.push(p);
                }
                Item::Text(paras)
            }
            Block::Image => {
                let img = slide.image.as_ref()?;
                Item::Picture(self.picture(&img.path, &img.alt_text)?)
            }
            Block::Columns => {
                let layout = slide.columns.as_ref()?;
                let columns = layout
                    .contents
                    .iter()
                    .zip(&layout.ratios)
                    .map(|(content, &ratio)| (f64::from(ratio.max(1)), self.column(content)))
                    .collect();
                Item::Columns(columns)
            }
        })
    }

    fn column(&mut self, content: &ColumnContent) -> Vec<Item> {
        let mut items = Vec::new();
        for item in &content.items {
            let item = match *item {
                ColumnItem::Text(t) => Item::Text(vec![self.para(&content.text_lines[t], 18.0)]),
                ColumnItem::Bullet(b) => Item::Text(self.bullets(&content.bullets[b..=b])),
                ColumnItem::Code(c) => {
                    let cb = &content.code_blocks[c];
                    self.code(&cb.label, &cb.code)
                }
                ColumnItem::Table(t) => Item::Table(content.tables[t].clone()),
                ColumnItem::Quote(q) => Item::Text(self.quote(&content.quotes[q])),
                ColumnItem::Math(m) => self.math(&content.math[m]),
                ColumnItem::Image => {
                    let Some(img) = &content.image else { continue };
                    match self.picture(Path::new(&img.path), "") {
                        Some(p) => Item::Picture(p),
                        None => continue,
                    }
                }
            };
            push_item(&mut items, item);
        }
        items
    }

    /// Markdown text as a paragraph of styled runs.
    fn para(&self, text: &str, size: f64) -> Para {
        let marker = Color::AnsiValue(1);
        let runs = crate::markdown::parser::parse_inline_formatting(text, Color::Reset, marker)
            .into_iter()
            .filter(|s| !s.text.is_empty())
            .map(|span| {
                let code = span.bg == Some(marker);
                let text = match code {
                    // The terminal pads code spans with a space on each side.
                    true => span.text.trim_matches(' ').to_string(),
                    false => span.text,
                };
                Run {
                    text,
                    bold: span.bold,
                    italic: span.italic,
                    strike: span.strikethrough,
                    code,
                    link: span.link.map(|l| l.to_string()),
                }
            })
            .collect();
        Para {
            runs,
            size,
            align: None,
            bullet: None,
            color: self.colors.text.clone(),
            mono: false,
            space_before: 6.0,
        }
    }

    fn mono(&self, line: &str, size: f64) -> Para {
        Para {
            runs: vec![Run::plain(line)],
            size,
            align: None,
            bullet: None,
            color: self.colors.text.clone(),
            mono: true,
            space_before: 0.0,
        }
    }

    fn bullets(&self, items: &[Bullet]) -> Vec<Para> {
        items
            .iter()
            .map(|b| {
                let (glyph, text) = match b.task() {
                    Some((done, text)) => (Some(if done { '☑' } else { '☐' }), text),
                    None if b.is_ordered() => (None, b.text.as_str()),
                    None => (Some(['•', '◦', '▪'][b.depth.min(2)]), b.text.as_str()),
                };
                let mut p = self.para(text, 20.0 - 2.0 * b.depth.min(2) as f64);
                p.bullet = Some((b.depth.min(2), glyph));
                p
            })
            .collect()
    }

    fn quote(&self, q: &BlockQuote) -> Vec<Para> {
        let mut paras = Vec::new();
        if let Some((kind, heading)) = &q.callout {
            let mut p = self.para(&format!("**{heading}**"), 18.0);
            p.color = callout_color(*kind).to_string();
            paras.push(p);
        }
        for line in &q.lines {
            let mut p = self.para(line, 18.0);
            p.bullet = Some((0, None));
            if q.callout.is_none() {
                p.color = self.colors.muted.clone();
                p.runs.iter_mut().for_each(|r| r.italic = true);
            }
            paras.push(p);
        }
        paras
    }

    fn code(&self, label: &str, code: &str) -> Item {
        let mut paras = Vec::new();
        if !label.is_empty() {
            let mut p = self.mono(label, 12.0);
            p.color = self.colors.muted.clone();
            paras.push(p);
        }
        paras.extend(
            code.lines()
                .map(|l| self.mono(&l.replace('\t', "    "), 14.0)),
        );
        Item::Code(paras)
    }

    fn math(&self, tex: &str) -> Item {
        Item::Math(
            crate::math::display(tex)
                .iter()
                .map(|r| self.mono(r, 18.0))
                .collect(),
        )
    }

    fn picture(&mut self, path: &Path, alt: &str) -> Option<Picture> {
        let data = std::fs::read(path).ok()?;
        let native = image::guess_format(&data).ok().and_then(|f| match f {
            image::ImageFormat::Png => Some("png"),
            image::ImageFormat::Jpeg => Some("jpeg"),
            image::ImageFormat::Gif => Some("gif"),
            _ => None,
        });
        let (ext, bytes, (w, h)) = match native {
            Some(ext) => {
                let dims = image::ImageReader::new(std::io::Cursor::new(&data))
                    .with_guessed_format()
                    .ok()?
                    .into_dimensions()
                    .ok()?;
                (ext, data, dims)
            }
            // SVG, WebP, and BMP are not safe bets in every Office app.
            None => {
                let img = crate::image_util::load_image(path).ok()?;
                ("png", encode_png(&img)?, img.dimensions())
            }
        };
        let file = format!("image{}.{ext}", self.media.len() + 1);
        self.media.push((file.clone(), bytes));
        Some(Picture {
            file,
            width: f64::from(w),
            height: f64::from(h),
            alt: alt.to_string(),
            max_height: SLIDE_H,
        })
    }

    /// The QR code as a PNG with its quiet zone.
    fn qr(&mut self, data: &str) -> Option<String> {
        let code = qrcode::QrCode::new(data.as_bytes()).ok()?;
        let (n, scale) = (code.width(), 12);
        let size = u32::try_from((n + 8) * scale).ok()?;
        let img = image::RgbaImage::from_fn(size, size, |x, y| {
            let (mx, my) = (x as usize / scale, y as usize / scale);
            let dark = (4..n + 4).contains(&mx)
                && (4..n + 4).contains(&my)
                && code[(mx - 4, my - 4)] == qrcode::Color::Dark;
            image::Rgba(if dark { [0, 0, 0, 255] } else { [255; 4] })
        });
        let file = format!("image{}.png", self.media.len() + 1);
        self.media.push((file.clone(), encode_png(&img)?));
        Some(file)
    }

    /// Stacks `items` down `area` (x, y, width, height), shrinking pictures
    /// and then text until they fit.
    fn place(&mut self, items: &[Item], (x, y, w, h): (f64, f64, f64, f64)) {
        let (mut text, mut pics) = (1.0, 1.0);
        for _ in 0..40 {
            if stack_height(items, w, text, pics) <= h {
                break;
            }
            // Pictures give way first, but text starts shrinking before
            // they turn into thumbnails.
            if pics > 0.3 && pics >= text - 0.35 {
                pics -= 0.05;
            } else if text > 0.5 {
                text -= 0.05;
            } else {
                break;
            }
        }
        let mut top = y;
        for item in items {
            let height = item_height(item, w, text, pics);
            self.item(item, (x, top, w, height), text, pics);
            top += height + GAP;
        }
    }

    fn item(&mut self, item: &Item, (x, y, w, h): (f64, f64, f64, f64), text: f64, pics: f64) {
        match item {
            Item::Text(paras) => self.text_box(paras, (x, y, w, h), text, None, false),
            Item::Code(paras) => {
                let fill = self.colors.code_bg.clone();
                self.text_box(paras, (x, y, w, h), text, Some(&fill), false);
            }
            Item::Math(paras) => {
                let width = mono_width(paras, text).min(w);
                let area = (x + (w - width) / 2.0, y, width, h);
                self.text_box(paras, area, text, None, false);
            }
            Item::Table(table) => self.table(table, (x, y, w, h), text),
            Item::Chart(chart) => self.chart(chart, (x, y, w, h), text),
            Item::Picture(p) => {
                let height = picture_height(p, w, pics);
                let width = height * p.width / p.height;
                let rel = self.rel(IMAGE, format!("../media/{}", p.file), false);
                let id = self.id();
                let _ = write!(
                    self.shapes,
                    "<p:pic><p:nvPicPr><p:cNvPr id=\"{id}\" name=\"Picture {id}\" descr=\"{}\"/><p:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></p:cNvPicPr><p:nvPr/></p:nvPicPr>\
                     <p:blipFill><a:blip r:embed=\"{rel}\"/><a:stretch><a:fillRect/></a:stretch></p:blipFill>\
                     <p:spPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr></p:pic>",
                    escape(&p.alt),
                    emu(x + (w - width) / 2.0),
                    emu(y),
                    emu(width),
                    emu(height),
                );
            }
            Item::Columns(columns) => {
                let gutter = 0.3 * INCH;
                let total: f64 = columns.iter().map(|c| c.0).sum();
                let usable = w - gutter * (columns.len().saturating_sub(1)) as f64;
                let mut left = x;
                for (ratio, items) in columns {
                    let width = usable * ratio / total;
                    let mut top = y;
                    for item in items {
                        let height = item_height(item, width, text, pics);
                        self.item(item, (left, top, width, height), text, pics);
                        top += height + GAP;
                    }
                    left += width + gutter;
                }
            }
        }
    }

    /// A text box; a `label` stays on one line, centered vertically.
    fn text_box(
        &mut self,
        paras: &[Para],
        (x, y, w, h): (f64, f64, f64, f64),
        scale: f64,
        fill: Option<&str>,
        label: bool,
    ) {
        let body: String = paras.iter().map(|p| self.para_xml(p, scale)).collect();
        let fill = fill.map_or_else(
            || "<a:noFill/>".to_string(),
            |f| format!("<a:solidFill><a:srgbClr val=\"{f}\"/></a:solidFill>"),
        );
        let inset = if fill.contains("solidFill") {
            91_440
        } else {
            0
        };
        let wrap = if label {
            "wrap=\"none\" anchor=\"ctr\""
        } else {
            "wrap=\"square\""
        };
        let id = self.id();
        let _ = write!(
            self.shapes,
            "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"Text {id}\"/><p:cNvSpPr txBox=\"1\"/><p:nvPr/></p:nvSpPr>\
             <p:spPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom>{fill}</p:spPr>\
             <p:txBody><a:bodyPr {wrap} lIns=\"{inset}\" tIns=\"{inset}\" rIns=\"{inset}\" bIns=\"{inset}\"/><a:lstStyle/>{body}</p:txBody></p:sp>",
            emu(x),
            emu(y),
            emu(w),
            emu(h),
        );
    }

    fn para_xml(&mut self, p: &Para, scale: f64) -> String {
        let size = p.size * scale;
        let mut props = format!(
            "<a:spcBef><a:spcPts val=\"{}\"/></a:spcBef>",
            (p.space_before * scale * 100.0).round()
        );
        let mut attrs = p.align.map_or(String::new(), |a| format!(" algn=\"{a}\""));
        match p.bullet {
            Some((depth, glyph)) => {
                let indent = 0.3 * INCH;
                let _ = write!(
                    attrs,
                    " marL=\"{}\" indent=\"{}\"",
                    emu(indent * (depth + 1) as f64),
                    emu(-indent)
                );
                match glyph {
                    Some(c) => {
                        let _ = write!(
                            props,
                            "<a:buClr><a:srgbClr val=\"{}\"/></a:buClr><a:buFont typeface=\"Arial\"/><a:buChar char=\"{c}\"/>",
                            self.colors.accent
                        );
                    }
                    None => props.push_str("<a:buNone/>"),
                }
            }
            None => props.push_str("<a:buNone/>"),
        }
        let mut runs = String::new();
        for run in &p.runs {
            let mut rpr = format!(" lang=\"en-US\" sz=\"{}\"", (size * 100.0).round());
            for (on, attr) in [
                (run.bold, " b=\"1\""),
                (run.italic, " i=\"1\""),
                (run.strike, " strike=\"sngStrike\""),
                (run.link.is_some(), " u=\"sng\""),
            ] {
                if on {
                    rpr.push_str(attr);
                }
            }
            let color = if run.code || run.link.is_some() {
                &self.colors.accent
            } else {
                &p.color
            };
            // Every run names its font: some readers carry the previous
            // run's font over to one that does not.
            let font = if p.mono || run.code { MONO } else { "+mn-lt" };
            let mut inner = format!(
                "<a:solidFill><a:srgbClr val=\"{color}\"/></a:solidFill><a:latin typeface=\"{font}\"/><a:cs typeface=\"{font}\"/>"
            );
            // Only web and mail links: a `javascript:` target must not become live.
            if let Some(url) = run.link.as_deref().filter(|u| {
                ["http://", "https://", "mailto:"]
                    .iter()
                    .any(|s| u.starts_with(s))
            }) {
                let rel = self.rel(HYPERLINK, url.to_string(), true);
                let _ = write!(inner, "<a:hlinkClick r:id=\"{rel}\"/>");
            }
            let _ = write!(
                runs,
                "<a:r><a:rPr{rpr}>{inner}</a:rPr><a:t>{}</a:t></a:r>",
                escape(&run.text)
            );
        }
        format!(
            "<a:p><a:pPr{attrs}>{props}</a:pPr>{runs}<a:endParaRPr lang=\"en-US\" sz=\"{}\"/></a:p>",
            (size * 100.0).round()
        )
    }

    fn table(&mut self, t: &Table, (x, y, w, _): (f64, f64, f64, f64), scale: f64) {
        let columns = t
            .headers
            .len()
            .max(t.rows.iter().map(Vec::len).max().unwrap_or(0));
        if columns == 0 {
            return;
        }
        let widths = column_widths(t, columns, w);
        let size = 16.0 * scale;
        let mut xml = String::new();
        for (r, row) in std::iter::once(&t.headers).chain(&t.rows).enumerate() {
            let height = row_height(row, &widths, size);
            let _ = write!(xml, "<a:tr h=\"{}\">", emu(height));
            for c in 0..columns {
                let cell = row.get(c).map_or("", String::as_str);
                let mut p = self.para(cell, 16.0);
                p.space_before = 0.0;
                if r == 0 {
                    p.color = self.colors.accent.clone();
                    p.runs.iter_mut().for_each(|run| run.bold = true);
                }
                let body = self.para_xml(&p, scale);
                let line = |side: &str, color: &str| {
                    format!("<a:{side} w=\"9525\"><a:solidFill><a:srgbClr val=\"{color}\"/></a:solidFill></a:{side}>")
                };
                let border = &self.colors.muted;
                let bottom = if r == 0 { &self.colors.accent } else { border };
                let _ = write!(
                    xml,
                    "<a:tc><a:txBody><a:bodyPr/><a:lstStyle/>{body}</a:txBody><a:tcPr>{}{}{}{}<a:noFill/></a:tcPr></a:tc>",
                    line("lnL", border),
                    line("lnR", border),
                    line("lnT", border),
                    line("lnB", bottom),
                );
            }
            xml.push_str("</a:tr>");
        }
        let grid: String = widths
            .iter()
            .map(|w| format!("<a:gridCol w=\"{}\"/>", emu(*w)))
            .collect();
        let height: f64 = std::iter::once(&t.headers)
            .chain(&t.rows)
            .map(|row| row_height(row, &widths, size))
            .sum();
        let id = self.id();
        let _ = write!(
            self.shapes,
            "<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id=\"{id}\" name=\"Table {id}\"/><p:cNvGraphicFramePr><a:graphicFrameLocks noGrp=\"1\"/></p:cNvGraphicFramePr><p:nvPr/></p:nvGraphicFramePr>\
             <p:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></p:xfrm>\
             <a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/table\"><a:tbl><a:tblPr firstRow=\"1\"/><a:tblGrid>{grid}</a:tblGrid>{xml}</a:tbl></a:graphicData></a:graphic></p:graphicFrame>",
            emu(x),
            emu(y),
            emu(widths.iter().sum()),
            emu(height),
        );
    }

    /// Bars as rectangles, labels and values as text.
    fn chart(&mut self, chart: &Chart, (x, y, w, h): (f64, f64, f64, f64), scale: f64) {
        let mut top = y;
        if let Some(title) = &chart.title {
            let mut p = self.para(title, 16.0);
            p.color = self.colors.muted.clone();
            p.space_before = 0.0;
            let line = 16.0 * scale * 1.3 * PT;
            self.text_box(&[p], (x, top, w, line), scale, None, false);
            top += line;
        }
        let max = chart.bars.iter().map(|b| b.1).fold(0.0, f64::max);
        let share = |v: f64| if max > 0.0 { v / max } else { 0.0 };
        let n = chart.bars.len().max(1) as f64;
        let size = 16.0 * scale;
        let label = |page: &Self, text: &str| {
            let mut p = page.para(text, 16.0);
            p.space_before = 0.0;
            p
        };
        let accent = self.colors.accent.clone();
        if chart.columns {
            let text_row = size * 1.4 * PT;
            let track = (y + h - top - 2.0 * text_row).max(text_row);
            let slot = w / n;
            for (i, (name, value, shown)) in chart.bars.iter().enumerate() {
                let left = x + slot * i as f64;
                let bar = track * share(*value);
                let base = top + text_row + track;
                self.rect((left + slot * 0.2, base - bar, slot * 0.6, bar), &accent);
                let mut v = label(self, shown);
                v.color = self.colors.muted.clone();
                self.label(
                    v,
                    (left, base - bar - text_row, slot, text_row),
                    scale,
                    "ctr",
                );
                let name = label(self, name);
                self.label(name, (left, base, slot, text_row), scale, "ctr");
            }
            return;
        }
        let row = size * 1.6 * PT;
        let label_w = (w * 0.3).min(
            chart
                .bars
                .iter()
                .map(|b| b.0.chars().count())
                .max()
                .unwrap_or(1) as f64
                * size
                * 0.55
                * PT
                + 0.2 * INCH,
        );
        let value_w = chart
            .bars
            .iter()
            .map(|b| b.2.chars().count())
            .max()
            .unwrap_or(1) as f64
            * size
            * 0.55
            * PT
            + 0.3 * INCH;
        let track = (w - label_w - value_w).max(0.5 * INCH);
        for (i, (name, value, shown)) in chart.bars.iter().enumerate() {
            let top = top + row * i as f64;
            let mut l = label(self, name);
            l.runs.iter_mut().for_each(|r| r.bold = false);
            self.label(l, (x, top, label_w - 0.1 * INCH, row), scale, "r");
            let bar = (track * share(*value)).max(1.0);
            self.rect((x + label_w, top + row * 0.2, bar, row * 0.6), &accent);
            let mut v = label(self, shown);
            v.color = self.colors.muted.clone();
            self.label(
                v,
                (x + label_w + bar + 0.1 * INCH, top, value_w, row),
                scale,
                "l",
            );
        }
    }

    fn label(&mut self, mut p: Para, area: (f64, f64, f64, f64), scale: f64, align: &'static str) {
        p.align = Some(align);
        self.text_box(&[p], area, scale, None, true);
    }

    fn rect(&mut self, (x, y, w, h): (f64, f64, f64, f64), color: &str) {
        let id = self.id();
        let _ = write!(
            self.shapes,
            "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"Bar {id}\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>\
             <p:spPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val=\"{color}\"/></a:solidFill><a:ln><a:noFill/></a:ln></p:spPr></p:sp>",
            emu(x),
            emu(y),
            emu(w),
            emu(h),
        );
    }
}

/// Consecutive text merges into one text box.
fn push_item(items: &mut Vec<Item>, item: Item) {
    if let (Some(Item::Text(last)), Item::Text(paras)) = (items.last_mut(), &item) {
        if !paras.is_empty() {
            let Item::Text(paras) = item else { return };
            last.extend(paras);
            return;
        }
    }
    items.push(item);
}

fn picture_height(p: &Picture, w: f64, pics: f64) -> f64 {
    let natural = (w * p.height / p.width.max(1.0))
        .min(p.max_height)
        .min(SLIDE_H * 0.7);
    natural * pics
}

fn para_height(p: &Para, w: f64, scale: f64) -> f64 {
    let size = p.size * scale;
    let indent = p.bullet.map_or(0.0, |(d, _)| 0.3 * INCH * (d + 1) as f64);
    // Monospaced text is wider, and word wrap leaves the ends of lines empty.
    let width: f64 = p
        .runs
        .iter()
        .map(|r| {
            let em = if p.mono || r.code { 0.6 } else { 0.55 };
            r.text.chars().count() as f64 * em * size * PT
        })
        .sum();
    let lines = (width / ((w - indent) * 0.92).max(1.0)).ceil().max(1.0);
    lines * size * 1.2 * PT + p.space_before * scale * PT
}

fn mono_width(paras: &[Para], scale: f64) -> f64 {
    let widest = paras
        .iter()
        .map(|p| p.runs.iter().map(|r| r.text.chars().count()).sum::<usize>() as f64 * p.size)
        .fold(0.0, f64::max);
    widest * scale * 0.62 * PT + 0.1 * INCH
}

fn item_height(item: &Item, w: f64, text: f64, pics: f64) -> f64 {
    match item {
        Item::Text(paras) | Item::Math(paras) => {
            paras.iter().map(|p| para_height(p, w, text)).sum()
        }
        Item::Code(paras) => {
            paras
                .iter()
                .map(|p| para_height(p, w - 0.2 * INCH, text))
                .sum::<f64>()
                + 0.2 * INCH
        }
        Item::Table(t) => {
            let columns = t.headers.len().max(1);
            let widths = column_widths(t, columns, w);
            std::iter::once(&t.headers)
                .chain(&t.rows)
                .map(|row| row_height(row, &widths, 16.0 * text))
                .sum()
        }
        Item::Chart(chart) => {
            let title = chart.title.as_ref().map_or(0.0, |_| 16.0 * text * 1.3 * PT);
            let rows = if chart.columns {
                2.6 * INCH * text
            } else {
                chart.bars.len() as f64 * 16.0 * text * 1.6 * PT
            };
            title + rows
        }
        Item::Picture(p) => picture_height(p, w, pics),
        Item::Columns(columns) => {
            let total: f64 = columns.iter().map(|c| c.0).sum();
            let usable = w - 0.3 * INCH * columns.len().saturating_sub(1) as f64;
            columns
                .iter()
                .map(|(ratio, items)| stack_height(items, usable * ratio / total, text, pics))
                .fold(0.0, f64::max)
        }
    }
}

fn stack_height(items: &[Item], w: f64, text: f64, pics: f64) -> f64 {
    let gaps = GAP * items.len().saturating_sub(1) as f64;
    items
        .iter()
        .map(|i| item_height(i, w, text, pics))
        .sum::<f64>()
        + gaps
}

/// Column widths in proportion to their longest cell, none under a tenth.
fn column_widths(t: &Table, columns: usize, w: f64) -> Vec<f64> {
    let longest: Vec<f64> = (0..columns)
        .map(|c| {
            std::iter::once(&t.headers)
                .chain(&t.rows)
                .filter_map(|r| r.get(c))
                .map(|s| s.chars().count())
                .max()
                .unwrap_or(1)
                .max(3) as f64
        })
        .collect();
    let total: f64 = longest.iter().sum();
    let natural: f64 = total * 16.0 * 0.7 * PT + columns as f64 * 0.4 * INCH;
    let width = natural.min(w);
    let shares: Vec<f64> = longest.iter().map(|l| (l / total).max(0.1)).collect();
    let sum: f64 = shares.iter().sum();
    shares.iter().map(|s| width * s / sum).collect()
}

fn row_height(row: &[String], widths: &[f64], size: f64) -> f64 {
    let lines = row
        .iter()
        .zip(widths)
        .map(|(cell, w)| {
            let per_line = ((w - 0.2 * INCH) / (size * 0.7 * PT)).max(1.0);
            (cell.chars().count() as f64 / per_line).ceil().max(1.0)
        })
        .fold(1.0, f64::max);
    lines * size * 1.25 * PT + 0.14 * INCH
}

fn callout_color(kind: Callout) -> &'static str {
    match kind {
        Callout::Note => "4493F8",
        Callout::Tip => "3FB950",
        Callout::Important => "AB7DF8",
        Callout::Warning => "D29922",
        Callout::Caution => "F85149",
    }
}

fn encode_png(img: &image::RgbaImage) -> Option<Vec<u8>> {
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

fn emu(v: f64) -> i64 {
    v.round() as i64
}

/// Text for XML: escaped, without the control characters XML 1.0 forbids.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if c < ' ' || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

fn notes_slide(notes: &str) -> String {
    let paras: String = if notes.is_empty() {
        "<a:p><a:endParaRPr lang=\"en-US\"/></a:p>".to_string()
    } else {
        notes
            .lines()
            .map(|l| {
                format!(
                    "<a:p><a:r><a:rPr lang=\"en-US\"/><a:t>{}</a:t></a:r></a:p>",
                    escape(l)
                )
            })
            .collect()
    };
    format!(
        "{XML_HEAD}<p:notes {NS}><p:cSld><p:spTree>{GROUP}\
         <p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"Slide Image\"/><p:cNvSpPr><a:spLocks noGrp=\"1\" noRot=\"1\" noChangeAspect=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"sldImg\" idx=\"2\"/></p:nvPr></p:nvSpPr><p:spPr/></p:sp>\
         <p:sp><p:nvSpPr><p:cNvPr id=\"3\" name=\"Notes\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"body\" idx=\"3\"/></p:nvPr></p:nvSpPr><p:spPr/>\
         <p:txBody><a:bodyPr/><a:lstStyle/>{paras}</p:txBody></p:sp>\
         </p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:notes>"
    )
}

fn relationships(rels: &[(&str, String, bool)]) -> String {
    let mut xml = format!(
        "{XML_HEAD}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">"
    );
    for (i, (kind, target, external)) in rels.iter().enumerate() {
        let mode = if *external {
            " TargetMode=\"External\""
        } else {
            ""
        };
        let _ = write!(
            xml,
            "<Relationship Id=\"rId{}\" Type=\"{REL}/{kind}\" Target=\"{}\"{mode}/>",
            i + 1,
            escape(target)
        );
    }
    xml.push_str("</Relationships>");
    xml
}

fn content_types(slides: usize) -> String {
    let mut xml = format!(
        "{XML_HEAD}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
         <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
         <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
         <Default Extension=\"png\" ContentType=\"image/png\"/>\
         <Default Extension=\"jpeg\" ContentType=\"image/jpeg\"/>\
         <Default Extension=\"gif\" ContentType=\"image/gif\"/>"
    );
    let pml = "application/vnd.openxmlformats-officedocument.presentationml";
    for (part, kind) in [
        (
            "/ppt/presentation.xml",
            format!("{pml}.presentation.main+xml"),
        ),
        (
            "/ppt/slideMasters/slideMaster1.xml",
            format!("{pml}.slideMaster+xml"),
        ),
        (
            "/ppt/slideLayouts/slideLayout1.xml",
            format!("{pml}.slideLayout+xml"),
        ),
        (
            "/ppt/notesMasters/notesMaster1.xml",
            format!("{pml}.notesMaster+xml"),
        ),
        ("/ppt/presProps.xml", format!("{pml}.presProps+xml")),
        ("/ppt/viewProps.xml", format!("{pml}.viewProps+xml")),
        ("/ppt/tableStyles.xml", format!("{pml}.tableStyles+xml")),
        (
            "/ppt/theme/theme1.xml",
            "application/vnd.openxmlformats-officedocument.theme+xml".into(),
        ),
        (
            "/ppt/theme/theme2.xml",
            "application/vnd.openxmlformats-officedocument.theme+xml".into(),
        ),
        (
            "/docProps/core.xml",
            "application/vnd.openxmlformats-package.core-properties+xml".into(),
        ),
        (
            "/docProps/app.xml",
            "application/vnd.openxmlformats-officedocument.extended-properties+xml".into(),
        ),
    ] {
        let _ = write!(
            xml,
            "<Override PartName=\"{part}\" ContentType=\"{kind}\"/>"
        );
    }
    for n in 1..=slides {
        let _ = write!(
            xml,
            "<Override PartName=\"/ppt/slides/slide{n}.xml\" ContentType=\"{pml}.slide+xml\"/>\
             <Override PartName=\"/ppt/notesSlides/notesSlide{n}.xml\" ContentType=\"{pml}.notesSlide+xml\"/>"
        );
    }
    xml.push_str("</Types>");
    xml
}

fn presentation(slides: usize) -> String {
    let ids: String = (1..=slides)
        .map(|n| format!("<p:sldId id=\"{}\" r:id=\"rId{}\"/>", 255 + n, 6 + n))
        .collect();
    format!(
        "{XML_HEAD}<p:presentation {NS}>\
         <p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"rId1\"/></p:sldMasterIdLst>\
         <p:notesMasterIdLst><p:notesMasterId r:id=\"rId2\"/></p:notesMasterIdLst>\
         <p:sldIdLst>{ids}</p:sldIdLst>\
         <p:sldSz cx=\"{}\" cy=\"{}\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/>\
         </p:presentation>",
        emu(SLIDE_W),
        emu(SLIDE_H)
    )
}

fn core_props(title: &str, author: &str) -> String {
    format!(
        "{XML_HEAD}<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
         <dc:title>{}</dc:title><dc:creator>{}</dc:creator></cp:coreProperties>",
        escape(title),
        escape(author)
    )
}

fn slide_master(colors: &Colors) -> String {
    format!(
        "{XML_HEAD}<p:sldMaster {NS}><p:cSld>\
         <p:bg><p:bgPr><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill><a:effectLst/></p:bgPr></p:bg>\
         <p:spTree>{GROUP}\
         <p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"Title\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"title\"/></p:nvPr></p:nvSpPr>\
         <p:spPr><a:xfrm><a:off x=\"457200\" y=\"320040\"/><a:ext cx=\"11277600\" cy=\"868680\"/></a:xfrm></p:spPr>\
         <p:txBody><a:bodyPr anchor=\"b\"/><a:lstStyle/><a:p><a:endParaRPr lang=\"en-US\"/></a:p></p:txBody></p:sp>\
         </p:spTree></p:cSld>{CLR_MAP}\
         <p:sldLayoutIdLst><p:sldLayoutId id=\"2147483649\" r:id=\"rId1\"/></p:sldLayoutIdLst>\
         <p:txStyles>\
         <p:titleStyle><a:lvl1pPr><a:defRPr sz=\"3200\" b=\"1\"><a:solidFill><a:schemeClr val=\"accent1\"/></a:solidFill><a:latin typeface=\"+mj-lt\"/></a:defRPr></a:lvl1pPr></p:titleStyle>\
         <p:bodyStyle><a:lvl1pPr><a:defRPr sz=\"2000\"><a:solidFill><a:schemeClr val=\"tx1\"/></a:solidFill><a:latin typeface=\"+mn-lt\"/></a:defRPr></a:lvl1pPr></p:bodyStyle>\
         <p:otherStyle><a:lvl1pPr><a:defRPr sz=\"1800\"><a:solidFill><a:schemeClr val=\"tx1\"/></a:solidFill><a:latin typeface=\"+mn-lt\"/></a:defRPr></a:lvl1pPr></p:otherStyle>\
         </p:txStyles></p:sldMaster>",
        colors.bg
    )
}

/// `dk1` and `lt1` hold the text and background whether the theme is dark
/// or light, so `tx1` is always the text color.
fn theme_xml(bg: &str, text: &str, accent: &str) -> String {
    let fill = "<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>";
    let line = |w: u32| format!("<a:ln w=\"{w}\">{fill}</a:ln>");
    let effect = "<a:effectStyle><a:effectLst/></a:effectStyle>";
    format!(
        "{XML_HEAD}<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"Ostendo\"><a:themeElements>\
         <a:clrScheme name=\"Ostendo\">\
         <a:dk1><a:srgbClr val=\"{text}\"/></a:dk1><a:lt1><a:srgbClr val=\"{bg}\"/></a:lt1>\
         <a:dk2><a:srgbClr val=\"{text}\"/></a:dk2><a:lt2><a:srgbClr val=\"{bg}\"/></a:lt2>\
         <a:accent1><a:srgbClr val=\"{accent}\"/></a:accent1><a:accent2><a:srgbClr val=\"{accent}\"/></a:accent2>\
         <a:accent3><a:srgbClr val=\"{accent}\"/></a:accent3><a:accent4><a:srgbClr val=\"{accent}\"/></a:accent4>\
         <a:accent5><a:srgbClr val=\"{accent}\"/></a:accent5><a:accent6><a:srgbClr val=\"{accent}\"/></a:accent6>\
         <a:hlink><a:srgbClr val=\"{accent}\"/></a:hlink><a:folHlink><a:srgbClr val=\"{accent}\"/></a:folHlink>\
         </a:clrScheme>\
         <a:fontScheme name=\"Ostendo\">\
         <a:majorFont><a:latin typeface=\"Calibri\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:majorFont>\
         <a:minorFont><a:latin typeface=\"Calibri\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:minorFont>\
         </a:fontScheme>\
         <a:fmtScheme name=\"Ostendo\">\
         <a:fillStyleLst>{fill}{fill}{fill}</a:fillStyleLst>\
         <a:lnStyleLst>{}{}{}</a:lnStyleLst>\
         <a:effectStyleLst>{effect}{effect}{effect}</a:effectStyleLst>\
         <a:bgFillStyleLst>{fill}{fill}{fill}</a:bgFillStyleLst>\
         </a:fmtScheme></a:themeElements><a:objectDefaults/><a:extraClrSchemeLst/></a:theme>",
        line(6350),
        line(12700),
        line(19050),
    )
}

const XML_HEAD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";
const NS: &str = "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"";
const GROUP: &str =
    "<p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>";
const CLR_MAP: &str = "<p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/>";

const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const SLIDE: &str = "slide";
const SLIDE_MASTER: &str = "slideMaster";
const SLIDE_LAYOUT: &str = "slideLayout";
const NOTES_MASTER: &str = "notesMaster";
const NOTES_SLIDE: &str = "notesSlide";
const THEME: &str = "theme";
const PRES_PROPS: &str = "presProps";
const VIEW_PROPS: &str = "viewProps";
const TABLE_STYLES: &str = "tableStyles";
const IMAGE: &str = "image";
const HYPERLINK: &str = "hyperlink";

const ROOT_RELS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"ppt/presentation.xml\"/>\
<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/>\
<Relationship Id=\"rId3\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties\" Target=\"docProps/app.xml\"/>\
</Relationships>";

const APP_PROPS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\"><Application>Ostendo</Application></Properties>";

const PRES_PROPS_XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<p:presentationPr xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"/>";

const VIEW_PROPS_XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<p:viewPr xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"/>";

const TABLE_STYLES_XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<a:tblStyleLst xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" def=\"{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}\"/>";

const SLIDE_LAYOUT_XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<p:sldLayout xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\" type=\"titleOnly\" preserve=\"1\">\
<p:cSld name=\"Title Only\"><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>\
<p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"Title\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"title\"/></p:nvPr></p:nvSpPr><p:spPr/>\
<p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr lang=\"en-US\"/></a:p></p:txBody></p:sp>\
</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>";

const NOTES_MASTER_XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<p:notesMaster xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\">\
<p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>\
<p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"Slide Image\"/><p:cNvSpPr><a:spLocks noGrp=\"1\" noRot=\"1\" noChangeAspect=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"sldImg\" idx=\"2\"/></p:nvPr></p:nvSpPr>\
<p:spPr><a:xfrm><a:off x=\"381000\" y=\"685800\"/><a:ext cx=\"6096000\" cy=\"3429000\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:noFill/><a:ln w=\"12700\"><a:solidFill><a:srgbClr val=\"000000\"/></a:solidFill></a:ln></p:spPr></p:sp>\
<p:sp><p:nvSpPr><p:cNvPr id=\"3\" name=\"Notes\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"body\" sz=\"quarter\" idx=\"3\"/></p:nvPr></p:nvSpPr>\
<p:spPr><a:xfrm><a:off x=\"685800\" y=\"4343400\"/><a:ext cx=\"5486400\" cy=\"4114800\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr>\
<p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr lang=\"en-US\"/></a:p></p:txBody></p:sp>\
</p:spTree></p:cSld>\
<p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/>\
<p:notesStyle><a:lvl1pPr marL=\"0\" algn=\"l\"><a:defRPr sz=\"1200\"><a:solidFill><a:schemeClr val=\"tx1\"/></a:solidFill><a:latin typeface=\"+mn-lt\"/></a:defRPr></a:lvl1pPr></p:notesStyle>\
</p:notesMaster>";

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::Read;

    /// Every entry of a package this module wrote, checked against its CRC.
    fn unzip(bytes: &[u8]) -> HashMap<String, String> {
        let (mut files, mut at) = (HashMap::new(), 0);
        let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]) as usize;
        let u32_at = |i: usize| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
        while u32_at(at) == 0x0403_4b50 {
            let (crc, packed) = (u32_at(at + 14), u32_at(at + 18) as usize);
            let name_len = u16_at(at + 26);
            let name = String::from_utf8(bytes[at + 30..at + 30 + name_len].to_vec()).unwrap();
            let start = at + 30 + name_len;
            let mut data = Vec::new();
            flate2::read::DeflateDecoder::new(&bytes[start..start + packed])
                .read_to_end(&mut data)
                .unwrap();
            assert_eq!(crc32fast::hash(&data), crc, "{name}");
            files.insert(name, String::from_utf8_lossy(&data).into_owned());
            at = start + packed;
        }
        assert_eq!(
            u32_at(at),
            0x0201_4b50,
            "central directory follows the entries"
        );
        files
    }

    #[test]
    fn exports_an_office_package_with_slides_notes_and_media() {
        let image = concat!(env!("CARGO_MANIFEST_DIR"), "/images/opus.png");
        let md = format!(
            "# Deck & title\n- see [docs](https://x.dev) or [bad](javascript:alert)\n\
             - bell\x07 rings\n![logo]({image})\n<!-- notes: Say hi -->\n---\n# Two\n\
             | A | B |\n|---|---|\n| 1 | 2 |"
        );
        let (meta, slides) = crate::markdown::parse_presentation(&md, None).unwrap();
        let theme = crate::theme::ThemeRegistry::load().get("paper").unwrap();
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("deck.pptx");
        export_pptx(&slides, &meta, &theme, "Deck", &out).unwrap();

        let files = unzip(&std::fs::read(&out).unwrap());
        let types = &files["[Content_Types].xml"];
        for part in ["slide1", "slide2", "notesSlide1", "notesSlide2"] {
            assert!(
                types.contains(&format!("/{part}.xml\"")),
                "{part} not declared"
            );
        }
        assert_eq!(
            files["ppt/presentation.xml"].matches("<p:sldId ").count(),
            2
        );
        let first = &files["ppt/slides/slide1.xml"];
        assert!(first.contains("<a:t>Deck &amp; title</a:t>"), "{first}");
        assert!(
            first.contains("<a:t>bell rings</a:t>"),
            "control characters dropped"
        );
        assert!(first.contains("<p:pic>") && files.contains_key("ppt/media/image1.png"));
        let rels = &files["ppt/slides/_rels/slide1.xml.rels"];
        assert!(rels.contains("Target=\"https://x.dev\" TargetMode=\"External\""));
        assert!(!rels.contains("javascript"), "{rels}");
        assert!(files["ppt/notesSlides/notesSlide1.xml"].contains("<a:t>Say hi</a:t>"));
        assert!(files["ppt/slides/slide2.xml"].contains("<a:tbl>"));
    }
}
