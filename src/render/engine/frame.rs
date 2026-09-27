//! Lays out the current slide into a cached [`SlideFrame`].

use std::rc::Rc;

use crate::presentation::{Block, ImagePosition, SlideAlignment, Step};
use crate::render::text::{ellipsize, StyledLine, StyledSpan};
use crate::theme::colors::interpolate_color;

use super::blocks::{self, Ctx, ExecView};
use super::columns::columns;
use super::display::ImageKind;
use super::images::Rendered;
use super::types::{FrameImage, Layout, SlideFrame};
use super::Presenter;

/// Everything a slide frame depends on besides the slide itself.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FrameKey {
    slide: usize,
    step: usize,
    layout: Layout,
    image_scale: i8,
    gif_frame: usize,
    show_sections: bool,
    generation: u64,
}

impl Presenter {
    /// Returns the current slide's frame, rebuilding it only when an input changed.
    pub(crate) fn slide_frame(&mut self, layout: &Layout) -> Rc<SlideFrame> {
        let key = FrameKey {
            slide: self.current,
            step: self.step,
            layout: *layout,
            image_scale: self.image_scale_offset,
            gif_frame: self.images.gif_frame(),
            show_sections: self.show_sections,
            generation: self.generation,
        };
        if let Some((cached, frame)) = &self.frame_cache {
            if *cached == key {
                return Rc::clone(frame);
            }
        }
        let mut frame = self.build_frame(layout, false, 0);
        if frame.lines.len() > layout.content_rows {
            frame = self.build_frame(layout, true, 0);
        }
        // Images and diagrams give up rows so the text after them fits too.
        let slide = &self.slides[self.current];
        let shrinkable = slide.image.is_some() || !slide.mermaid_blocks.is_empty();
        let extra = frame.lines.len().saturating_sub(layout.content_rows);
        if extra > 0 && shrinkable {
            frame = self.build_frame(layout, true, extra);
        }
        let frame = Rc::new(frame);
        self.frame_cache = Some((key, Rc::clone(&frame)));
        frame
    }

    /// `shrink` takes that many rows from each image's budget.
    fn build_frame(&mut self, layout: &Layout, compact: bool, shrink: usize) -> SlideFrame {
        let slide = &self.slides[self.current];
        let pal = self.palette;
        let exec = self.exec_output.as_deref().map(|output| ExecView {
            block: self.exec_block,
            output,
            running: self.exec.is_some(),
        });
        let ctx = Ctx {
            pal: &pal,
            width: layout.content_width,
            highlighter: &self.highlighter,
            figfont: &self.figfont,
            osc66: self.osc66,
            allow_exec: self.allow_exec,
            exec,
        };
        let colors = (pal.text, pal.bg);
        let mut out = SlideFrame::default();
        let blank = |out: &mut SlideFrame| {
            if out.lines.last().is_some_and(|l| !l.is_blank()) {
                out.lines.push(StyledLine::empty());
            }
        };

        // A right-pinned image narrows the text column beside it.
        let mut text_width = layout.content_width;
        if let Some(img) = slide
            .image
            .as_ref()
            .filter(|i| i.position == ImagePosition::Right)
        {
            let scale = self.image_scale(img.scale);
            let cols = layout.content_width * scale / 200;
            if let Some(Rendered::Cells { kind, cols, rows }) = self.images.slide_image(
                img,
                cols,
                layout.content_rows,
                self.images.protocol(),
                colors,
                &self.window,
            ) {
                text_width = layout.content_width.saturating_sub(cols + 2);
                out.images.push(FrameImage {
                    line: 0,
                    col: layout.content_width.saturating_sub(cols),
                    rows,
                    kind,
                    pinned_right: true,
                });
            }
        }
        let ctx = ctx.with_width(text_width);

        if slide.show_section.unwrap_or(self.show_sections) && !slide.section.is_empty() {
            out.push_unit([blocks::section_label(&ctx, &slide.section)]);
        }
        let title_in_columns = slide.ascii_title && slide.columns.is_some();
        if !slide.title.is_empty() && !title_in_columns {
            let decoration = slide
                .title_decoration
                .as_deref()
                .or(self.theme.title_decoration.as_deref());
            out.push_unit(blocks::title(
                &ctx,
                &slide.title,
                slide.ascii_title,
                slide.text_scale,
                decoration,
            ));
            blank(&mut out);
        }
        if !slide.subtitle.is_empty() {
            let mut sub = blocks::paragraph(&ctx, &slide.subtitle);
            for s in sub.iter_mut().flat_map(|l| l.spans.iter_mut()) {
                if s.fg == Some(pal.text) {
                    s.fg = Some(interpolate_color(pal.text, pal.accent, 0.25));
                }
            }
            out.push_unit(sub);
            blank(&mut out);
        }

        let exec_before = |i: usize| {
            slide.code_blocks[..i]
                .iter()
                .filter(|c| c.exec_mode.is_some())
                .count()
        };
        let (shown, pending) = slide.steps.split_at(self.step.min(slide.steps.len()));
        let hidden_block = pending.iter().find_map(|s| match s {
            Step::Pause(n) => Some(*n),
            Step::Highlight { .. } => None,
        });
        let mut hidden_from = None;
        for (index, block) in slide.blocks.iter().enumerate() {
            if Some(index) == hidden_block {
                hidden_from = Some(out.lines.len());
            }
            let mut lines = Vec::new();
            match *block {
                Block::Paragraph(i) => lines = blocks::paragraph(&ctx, &slide.paragraphs[i]),
                Block::Bullets(i) => {
                    let items = &slide.bullets[slide.bullet_groups[i].clone()];
                    lines = blocks::bullets(&ctx, items, !compact);
                }
                Block::Code(i) => {
                    let cb = &slide.code_blocks[i];
                    let group = shown
                        .iter()
                        .filter(|s| matches!(s, Step::Highlight { code, .. } if *code == i))
                        .count();
                    lines = blocks::code_block(&ctx, cb, cb.highlights.get(group));
                    let index = exec_before(i);
                    if let Some(view) = exec.filter(|v| cb.exec_mode.is_some() && v.block == index)
                    {
                        lines.extend(blocks::exec_output(&ctx, view.output, view.running));
                    }
                }
                Block::Table(i) => lines = blocks::table(&ctx, &slide.tables[i]),
                Block::Quote(i) => lines = blocks::quote(&ctx, &slide.block_quotes[i]),
                Block::Diagram(i) => {
                    let d = &slide.diagram_blocks[i];
                    let graph = crate::diagram::parser::parse(&d.source);
                    let dim = interpolate_color(pal.text, pal.bg, 0.5);
                    lines = crate::diagram::render_adaptive(
                        &graph, d.style, text_width, pal.accent, pal.text, dim, "",
                    );
                }
                Block::Chart(i) => lines = super::figures::chart(&ctx, &slide.charts[i]),
                Block::Math(i) => lines = blocks::math(&ctx, &slide.math[i]),
                Block::Qr(i) => match super::figures::qr(&slide.qr_codes[i], text_width) {
                    Some(code) => place(&mut out, Rendered::Lines(code), text_width),
                    None => {
                        let note =
                            format!("[QR code too large at this width: {}]", slide.qr_codes[i]);
                        lines.push(StyledLine::plain(&ellipsize(&note, text_width)));
                        lines[0].spans[0].fg = Some(pal.muted);
                    }
                },
                Block::Mermaid(i) => {
                    let source = &slide.mermaid_blocks[i].source;
                    let rows = layout
                        .content_rows
                        .saturating_sub(out.lines.len() + 1)
                        .max(layout.content_rows / 2)
                        .saturating_sub(shrink)
                        .max(3);
                    match self
                        .images
                        .mermaid(source, text_width, rows, colors, &self.window)
                    {
                        Ok(r) => place(&mut out, r, text_width),
                        Err(reason) => lines = blocks::mermaid_fallback(&ctx, source, &reason),
                    }
                }
                Block::Image => {
                    let Some(img) = slide
                        .image
                        .as_ref()
                        .filter(|i| i.position != ImagePosition::Right)
                    else {
                        continue;
                    };
                    let cols = text_width * self.image_scale(img.scale) / 100;
                    let remaining = layout.content_rows.saturating_sub(out.lines.len() + 1);
                    let rows = remaining
                        .max(layout.content_rows / 3)
                        .saturating_sub(shrink)
                        .max(3);
                    let protocol = self.images.protocol();
                    match self.images.slide_image(
                        img,
                        cols.max(1),
                        rows,
                        protocol,
                        colors,
                        &self.window,
                    ) {
                        Some(r) => place(&mut out, r, text_width),
                        None => {
                            let missing = format!("[image not found: {}]", img.path.display());
                            lines.push(StyledLine::plain(&missing));
                            lines[0].spans[0].fg = Some(pal.muted);
                        }
                    }
                }
                Block::Columns => {
                    let Some(layout_cols) = &slide.columns else {
                        continue;
                    };
                    let title = title_in_columns.then_some(slide.title.as_str());
                    let images = &mut self.images;
                    let mut col_image = |img: &crate::presentation::ColumnImage, w: usize| {
                        images.column_image(&img.path, img.color.as_deref(), w, colors)
                    };
                    let shown_pauses = shown.iter().filter(|s| matches!(s, Step::Pause(_))).count();
                    lines = columns(
                        &ctx,
                        layout_cols,
                        title,
                        exec_before(slide.code_blocks.len()),
                        shown_pauses,
                        &mut col_image,
                    );
                }
            }
            out.push_unit(lines);
            // A list split by `<!-- pause -->` still centers as one list.
            let continued = index > 0
                && matches!(block, Block::Bullets(_))
                && matches!(slide.blocks[index - 1], Block::Bullets(_));
            if continued && out.units.len() >= 2 {
                let last = out.units.pop().map_or(0, |u| u.end);
                if let Some(list) = out.units.last_mut() {
                    list.end = last;
                }
            }
            blank(&mut out);
        }
        while out.lines.last().is_some_and(StyledLine::is_blank) {
            out.lines.pop();
        }
        if let Some(from) = hidden_from {
            conceal(
                &mut out,
                from,
                slide.blocks[hidden_block.unwrap_or(0)..].contains(&Block::Image),
            );
        }
        align(
            &mut out,
            slide.alignment.or(self.meta.default_alignment),
            layout,
        );
        out
    }

    /// Effective image scale percentage after the runtime `<` / `>` offset.
    fn image_scale(&self, directive: u8) -> usize {
        (i16::from(directive) + i16::from(self.image_scale_offset)).clamp(5, 100) as usize
    }
}

/// Appends a rendered image to the frame, centered in `width`.
fn place(out: &mut SlideFrame, image: Rendered, width: usize) {
    match image {
        Rendered::Lines(lines) => {
            let widest = lines.iter().map(StyledLine::width).max().unwrap_or(0);
            let pad = " ".repeat(width.saturating_sub(widest) / 2);
            out.lines.extend(lines.into_iter().map(|mut l| {
                l.spans.insert(0, StyledSpan::new(&pad));
                l
            }));
        }
        Rendered::Cells { kind, cols, rows } => {
            out.images.push(FrameImage {
                line: out.lines.len(),
                col: width.saturating_sub(cols) / 2,
                rows,
                kind: match kind {
                    ImageKind::Kitty { id, .. } => ImageKind::Kitty {
                        id,
                        cols: cols as u16,
                    },
                    inline => inline,
                },
                pinned_right: false,
            });
            out.lines.extend((0..rows).map(|_| StyledLine::empty()));
        }
    }
}

/// Blanks lines from `from` on, keeping their space so the visible part does
/// not move as the slide builds. A right-pinned image goes too when its
/// `![]()` line is among the hidden blocks.
fn conceal(out: &mut SlideFrame, from: usize, image_hidden: bool) {
    for line in out.lines.iter_mut().skip(from) {
        *line = StyledLine::empty();
    }
    out.images.retain(|img| {
        if img.pinned_right {
            !image_hidden
        } else {
            img.line < from
        }
    });
}

/// Applies horizontal centering (per element) and vertical centering.
fn align(out: &mut SlideFrame, alignment: Option<SlideAlignment>, layout: &Layout) {
    let alignment = alignment.unwrap_or(SlideAlignment::Top);
    let vcenter = matches!(alignment, SlideAlignment::Center | SlideAlignment::VCenter);
    let hcenter = matches!(alignment, SlideAlignment::Center | SlideAlignment::HCenter);
    if hcenter {
        for unit in &out.units {
            // Trailing blank rows may have been trimmed off the last element.
            let end = unit.end.min(out.lines.len());
            let Some(lines) = out.lines.get_mut(unit.start..end) else {
                continue;
            };
            let widest = lines.iter().map(StyledLine::width).max().unwrap_or(0);
            let free = layout.content_width.saturating_sub(widest);
            if free < 2 {
                continue;
            }
            let pad = StyledSpan::new(&" ".repeat(free / 2));
            // content_type is kept so targeted loop animations still apply.
            for line in lines.iter_mut().filter(|l| !l.is_blank()) {
                line.spans.insert(0, pad.clone());
            }
        }
    }
    if vcenter && out.lines.len() < layout.content_rows {
        let top = (layout.content_rows - out.lines.len()) / 2;
        out.lines
            .splice(0..0, (0..top).map(|_| StyledLine::empty()));
        for img in out.images.iter_mut().filter(|i| !i.pinned_right) {
            img.line += top;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::text::LineContentType;

    fn layout(rows: usize, width: usize) -> Layout {
        Layout {
            width,
            height: rows + 2,
            content_top: 1,
            content_rows: rows,
            content_width: width,
            margin: 0,
        }
    }

    #[test]
    fn centering_moves_elements_whole_and_keeps_content_type() {
        let art = |text: &str| StyledLine {
            spans: vec![StyledSpan::new(text)],
            content_type: LineContentType::FigletTitle,
        };
        let mut frame = SlideFrame {
            images: vec![FrameImage {
                line: 0,
                col: 0,
                rows: 1,
                kind: ImageKind::Kitty { id: 1, cols: 1 },
                pinned_right: false,
            }],
            ..SlideFrame::default()
        };
        frame.push_unit([art("wide art"), art("art")]);
        align(&mut frame, Some(SlideAlignment::Center), &layout(10, 12));
        assert_eq!(frame.lines.len(), 6);
        for line in &frame.lines[4..] {
            assert_eq!(line.content_type, LineContentType::FigletTitle);
            assert_eq!(
                line.spans[0].text, "  ",
                "rows of one element shift together"
            );
        }
        assert_eq!(frame.images[0].line, 4);
    }
}
