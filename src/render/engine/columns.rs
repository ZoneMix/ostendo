//! Side-by-side column layouts.

use crate::presentation::ColumnLayout;
use crate::render::text::{LineContentType, StyledLine, StyledSpan};

use super::blocks::{self, Ctx};

/// Renders `layout` into merged rows. `title` puts a FIGlet title at the top of
/// column 0; `image` draws a column image at the given width; `first_exec` is
/// the Ctrl+E index of the first executable block in the columns.
pub(crate) fn columns(
    ctx: &Ctx,
    layout: &ColumnLayout,
    title: Option<&str>,
    first_exec: usize,
    image: &mut dyn FnMut(&crate::presentation::ColumnImage, usize) -> Vec<StyledLine>,
) -> Vec<StyledLine> {
    let n = layout.contents.len().min(layout.ratios.len().max(1));
    let total: usize = layout
        .ratios
        .iter()
        .take(n)
        .map(|&r| usize::from(r.max(1)))
        .sum();
    if n == 0 || total == 0 {
        return Vec::new();
    }
    let gutter = 3;
    let usable = ctx.width.saturating_sub(gutter * (n - 1));
    let widths: Vec<usize> = (0..n)
        .map(|i| usable * usize::from(layout.ratios.get(i).copied().unwrap_or(1).max(1)) / total)
        .collect();

    let mut exec_index = first_exec;
    let cols: Vec<Vec<StyledLine>> = layout
        .contents
        .iter()
        .take(n)
        .enumerate()
        .map(|(i, content)| {
            let width = widths[i];
            let scale = layout
                .text_scale
                .filter(|&s| ctx.osc66 && s >= 2 && content.image.is_none());
            let inner = ctx.with_width(width / usize::from(scale.unwrap_or(1)));
            let mut rows: Vec<StyledLine> = Vec::new();
            let gap = |rows: &mut Vec<StyledLine>| {
                if !rows.is_empty() {
                    rows.push(StyledLine::empty());
                }
            };
            if i == 0 {
                if let Some(t) = title {
                    rows.extend(
                        blocks::figlet(&ctx.with_width(width), t).unwrap_or_else(|| {
                            blocks::title(&ctx.with_width(width), t, false, None, None)
                        }),
                    );
                }
            }
            for text in &content.text_lines {
                gap(&mut rows);
                let mut lines = blocks::paragraph(&inner, text);
                for s in lines.iter_mut().flat_map(|l| l.spans.iter_mut()) {
                    s.bold = true;
                }
                rows.extend(lines);
            }
            if !content.bullets.is_empty() {
                gap(&mut rows);
                rows.extend(blocks::bullets(&inner, &content.bullets, false));
            }
            if let Some(s) = scale {
                rows = rows
                    .into_iter()
                    .flat_map(|mut l| {
                        for span in &mut l.spans {
                            span.text_scale = s;
                        }
                        std::iter::once(l).chain((1..s).map(|_| StyledLine::empty()))
                    })
                    .collect();
            }
            for cb in &content.code_blocks {
                gap(&mut rows);
                let cctx = ctx.with_width(width);
                rows.extend(blocks::code_block(&cctx, cb));
                if cb.exec_mode.is_some() {
                    if let Some(view) = ctx.exec.as_ref().filter(|v| v.block == exec_index) {
                        rows.extend(blocks::exec_output(&cctx, view.output, view.running));
                    }
                    exec_index += 1;
                }
            }
            if let Some(img) = &content.image {
                gap(&mut rows);
                let img_width = width * usize::from(img.scale.unwrap_or(100).clamp(10, 100)) / 100;
                rows.extend(image(img, img_width.max(1)));
            }
            rows
        })
        .collect();

    let height = cols.iter().map(Vec::len).max().unwrap_or(0);
    let separator = if layout.separator {
        StyledSpan::new(" │ ").with_fg(ctx.pal.muted)
    } else {
        StyledSpan::new("   ")
    };
    (0..height)
        .map(|r| {
            let mut out = StyledLine::empty();
            for (i, col) in cols.iter().enumerate() {
                if i > 0 {
                    out.push(separator.clone());
                }
                let row = col.get(r);
                let used = row.map_or(0, StyledLine::width);
                if let Some(row) = row {
                    out.spans.extend(row.spans.iter().cloned());
                    if row.content_type != LineContentType::Text {
                        out.content_type = row.content_type;
                    }
                }
                out.push(StyledSpan::new(&" ".repeat(widths[i].saturating_sub(used))));
            }
            out
        })
        .collect()
}
