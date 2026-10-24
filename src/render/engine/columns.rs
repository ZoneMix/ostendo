//! Side-by-side column layouts.

use crate::presentation::{ColumnItem, ColumnLayout};
use crate::render::text::{LineContentType, StyledLine, StyledSpan};

use super::blocks::{self, Ctx};

/// Renders `layout` into merged rows. `title` puts a FIGlet title at the top of
/// column 0; `image` draws a column image at the given width; `first_exec` is
/// the Ctrl+E index of the first executable block in the columns; items after
/// more than `shown_pauses` pauses are left blank.
pub(crate) fn columns(
    ctx: &Ctx,
    layout: &ColumnLayout,
    title: Option<&str>,
    first_exec: usize,
    shown_pauses: usize,
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
            let pauses = |i: usize| content.pauses_before.get(i).copied().unwrap_or(0);
            let mut items = content.items.iter().copied().enumerate().peekable();
            let mut previous: Option<ColumnItem> = None;
            while let Some((i, item)) = items.next() {
                let consecutive_text = matches!(
                    (previous, item),
                    (Some(ColumnItem::Text(_)), ColumnItem::Text(_))
                );
                if !consecutive_text {
                    gap(&mut rows);
                }
                previous = Some(item);
                let full = ctx.with_width(width);
                let lines = match item {
                    ColumnItem::Text(t) => {
                        scaled(blocks::paragraph(&inner, &content.text_lines[t]), scale)
                    }
                    ColumnItem::Bullet(first) => {
                        // One list, unless a pause falls between its items.
                        let mut last = first;
                        while let Some(&(j, ColumnItem::Bullet(next))) = items.peek() {
                            if pauses(j) != pauses(i) {
                                break;
                            }
                            last = next;
                            items.next();
                        }
                        scaled(
                            blocks::bullets(&inner, &content.bullets[first..=last], false),
                            scale,
                        )
                    }
                    ColumnItem::Code(c) => {
                        let cb = &content.code_blocks[c];
                        let mut lines = blocks::code_block(&full, cb, cb.highlights.first());
                        if cb.exec_mode.is_some() {
                            if let Some(view) = ctx.exec.as_ref().filter(|v| v.block == exec_index)
                            {
                                lines.extend(blocks::exec_output(&full, view.output, view.running));
                            }
                            exec_index += 1;
                        }
                        lines
                    }
                    ColumnItem::Table(t) => blocks::table(&full, &content.tables[t]),
                    ColumnItem::Quote(q) => blocks::quote(&full, &content.quotes[q]),
                    ColumnItem::Math(m) => blocks::math(&full, &content.math[m]),
                    ColumnItem::Image => match &content.image {
                        Some(img) => {
                            let scale = usize::from(img.scale.unwrap_or(100).clamp(10, 100));
                            image(img, (width * scale / 100).max(1))
                        }
                        None => Vec::new(),
                    },
                };
                if pauses(i) > shown_pauses {
                    // Not built yet: keep its rows so nothing moves when it appears.
                    rows.extend(lines.iter().map(|_| StyledLine::empty()));
                } else {
                    rows.extend(lines);
                }
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

/// Applies OSC 66 scaling: each row grows to `scale` rows, so blank rows follow it.
fn scaled(lines: Vec<StyledLine>, scale: Option<u8>) -> Vec<StyledLine> {
    let Some(s) = scale else { return lines };
    lines
        .into_iter()
        .flat_map(|mut l| {
            for span in &mut l.spans {
                span.text_scale = s;
            }
            std::iter::once(l).chain((1..s).map(|_| StyledLine::empty()))
        })
        .collect()
}
