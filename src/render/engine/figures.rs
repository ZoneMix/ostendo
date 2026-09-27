//! Charts and QR codes drawn with block characters.

use crossterm::style::Color;
use unicode_width::UnicodeWidthStr;

use crate::presentation::Chart;
use crate::render::text::{ellipsize, StyledLine, StyledSpan};
use crate::theme::colors::interpolate_color;

use super::blocks::Ctx;

/// Rows a column chart's bars span.
const COLUMN_HEIGHT: usize = 8;

/// Horizontal bars (or vertical columns) scaled to the largest value.
pub(crate) fn chart(ctx: &Ctx, chart: &Chart) -> Vec<StyledLine> {
    let mut out = Vec::new();
    if let Some(title) = &chart.title {
        out.push(StyledLine::plain(title));
        out[0].spans[0].fg = Some(ctx.pal.muted);
        out.push(StyledLine::empty());
    }
    if chart.bars.is_empty() {
        return out;
    }
    let max = chart.bars.iter().map(|b| b.1).fold(0.0, f64::max);
    let share = |value: f64| if max > 0.0 { value / max } else { 0.0 };
    // Bars fade from the accent toward the text color down the list.
    let color = |i: usize| {
        let t = i as f64 / chart.bars.len().max(2).saturating_sub(1) as f64;
        interpolate_color(ctx.pal.accent, ctx.pal.text, 0.35 * t)
    };
    if chart.columns {
        out.extend(columns(ctx, chart, share, color));
        return out;
    }

    let label_w = chart
        .bars
        .iter()
        .map(|b| b.0.width())
        .max()
        .unwrap_or(0)
        .min(ctx.width / 3);
    let value_w = chart.bars.iter().map(|b| b.2.width()).max().unwrap_or(0);
    let track = ctx.width.saturating_sub(label_w + value_w + 3).max(1);
    for (i, (label, value, shown)) in chart.bars.iter().enumerate() {
        let eighths = (share(*value) * (track * 8) as f64).round() as usize;
        let bar = "█".repeat(eighths / 8) + ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"][eighths % 8];
        let label = ellipsize(label, label_w);
        let gap = track + 1 - bar.width();
        out.push(StyledLine {
            spans: vec![
                StyledSpan::new(&format!("{}{label} ", " ".repeat(label_w - label.width())))
                    .with_fg(ctx.pal.text),
                StyledSpan::new(&bar).with_fg(color(i)),
                StyledSpan::new(&" ".repeat(gap)),
                StyledSpan::new(shown).with_fg(ctx.pal.muted),
            ],
            ..StyledLine::default()
        });
    }
    out
}

fn columns(
    ctx: &Ctx,
    chart: &Chart,
    share: impl Fn(f64) -> f64,
    color: impl Fn(usize) -> Color,
) -> Vec<StyledLine> {
    let n = chart.bars.len();
    let widest = chart
        .bars
        .iter()
        .map(|b| b.0.width().max(b.2.width()))
        .max()
        .unwrap_or(1);
    // Columns share the width; each keeps a one-cell gap.
    let slot = (ctx.width / n).clamp(2, widest.max(3) + 2);
    let bar_w = (slot - 1).clamp(1, 6);
    let cell = |text: &str| {
        let text = ellipsize(text, slot - 1);
        let pad = slot.saturating_sub(text.width());
        format!("{}{text}{}", " ".repeat(pad / 2), " ".repeat(pad - pad / 2))
    };
    let heights: Vec<usize> = chart
        .bars
        .iter()
        .map(|b| (share(b.1) * (COLUMN_HEIGHT * 8) as f64).round() as usize)
        .collect();
    let mut out = Vec::new();
    for row in (0..COLUMN_HEIGHT).rev() {
        let mut line = StyledLine::empty();
        for (i, &h) in heights.iter().enumerate() {
            let fill = h.saturating_sub(row * 8).min(8);
            let glyph = ["", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"][fill];
            let left = (slot - bar_w) / 2;
            line.push(StyledSpan::new(&" ".repeat(left)));
            let body = if fill == 0 { " " } else { glyph };
            line.push(StyledSpan::new(&body.repeat(bar_w)).with_fg(color(i)));
            line.push(StyledSpan::new(&" ".repeat(slot - left - bar_w)));
        }
        out.push(line);
    }
    let baseline = "─".repeat((slot * n).min(ctx.width));
    out.push(StyledLine {
        spans: vec![StyledSpan::new(&baseline).with_fg(ctx.pal.muted)],
        ..StyledLine::default()
    });
    for (pick, fg) in [(0, ctx.pal.text), (2, ctx.pal.muted)] {
        let text: String = chart
            .bars
            .iter()
            .map(|b| cell(if pick == 0 { &b.0 } else { &b.2 }))
            .collect();
        out.push(StyledLine {
            spans: vec![StyledSpan::new(&text).with_fg(fg)],
            ..StyledLine::default()
        });
    }
    out
}

/// A QR code for `data`, two modules per cell, black on white whatever the
/// theme so phone cameras read it. `None` when it cannot be encoded or does
/// not fit `width`.
pub(crate) fn qr(data: &str, width: usize) -> Option<Vec<StyledLine>> {
    let code = qrcode::QrCode::new(data.as_bytes()).ok()?;
    let modules = code.width();
    // Scanners need a light margin around the code.
    let quiet = 2;
    let size = modules + 2 * quiet;
    if size > width {
        return None;
    }
    let dark = |x: usize, y: usize| {
        (quiet..quiet + modules).contains(&x)
            && (quiet..quiet + modules).contains(&y)
            && code[(x - quiet, y - quiet)] == qrcode::Color::Dark
    };
    let paint = |on: bool| {
        if on {
            Color::Rgb { r: 0, g: 0, b: 0 }
        } else {
            Color::Rgb {
                r: 255,
                g: 255,
                b: 255,
            }
        }
    };
    Some(
        (0..size.div_ceil(2))
            .map(|row| StyledLine {
                spans: (0..size)
                    .map(|x| {
                        StyledSpan::new("▀")
                            .with_fg(paint(dark(x, row * 2)))
                            .with_bg(paint(dark(x, row * 2 + 1)))
                    })
                    .collect(),
                ..StyledLine::default()
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_cells_match_the_encoded_modules() {
        let lines = qr("https://github.com/ZoneMix/ostendo", 80).expect("fits");
        let code = qrcode::QrCode::new(b"https://github.com/ZoneMix/ostendo").unwrap();
        let size = code.width() + 4;
        assert_eq!(lines.len(), size.div_ceil(2));
        let black = Color::Rgb { r: 0, g: 0, b: 0 };
        // Every module drawn in the cells matches the encoder's matrix.
        for y in 0..code.width() {
            for x in 0..code.width() {
                let (row, lower) = ((y + 2) / 2, (y + 2) % 2 == 1);
                let span = &lines[row].spans[x + 2];
                let drawn = if lower { span.bg } else { span.fg };
                assert_eq!(
                    drawn == Some(black),
                    code[(x, y)] == qrcode::Color::Dark,
                    "module {x},{y}"
                );
            }
        }
        assert!(
            qr("x", 10).is_none(),
            "a code wider than the slide is refused"
        );
    }
}
