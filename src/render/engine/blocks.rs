//! Builders for slide elements. Each returns lines starting at column 0 of the
//! content area; the caller places them on screen.

use std::sync::LazyLock;

use regex::Regex;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::code::highlight::Highlighter;
use crate::presentation::{BlockQuote, Bullet, Callout, CodeBlock, ExecMode, Table, TableAlign};
use crate::render::text::{ellipsize, wrap_text, LineContentType, StyledLine, StyledSpan};

use crate::theme::colors::interpolate_color;

use super::ansi::parse_ansi_line;
use super::palette::Palette;

/// The running or finished output of the Ctrl+E block with index `block`.
#[derive(Clone, Copy)]
pub(crate) struct ExecView<'a> {
    pub block: usize,
    pub output: &'a str,
    pub running: bool,
}

pub(crate) struct Ctx<'a> {
    pub pal: &'a Palette,
    pub width: usize,
    pub highlighter: &'a Highlighter,
    pub figfont: &'a figlet_rs::FIGfont,
    pub osc66: bool,
    pub allow_exec: bool,
    pub exec: Option<ExecView<'a>>,
}

impl Ctx<'_> {
    pub fn with_width(&self, width: usize) -> Ctx<'_> {
        Ctx { width, ..*self }
    }

    fn inline(&self, text: &str) -> Vec<StyledSpan> {
        let mut spans =
            crate::markdown::parser::parse_inline_formatting(text, self.pal.text, self.pal.code_bg);
        for span in spans.iter_mut().filter(|s| s.link.is_some()) {
            span.fg = Some(self.pal.accent);
        }
        spans
    }
}

fn line(spans: Vec<StyledSpan>) -> StyledLine {
    StyledLine {
        spans,
        content_type: LineContentType::Text,
    }
}

/// Wraps styled spans at word boundaries, keeping each piece's style.
pub(crate) fn wrap_spans(spans: &[StyledSpan], width: usize) -> Vec<Vec<StyledSpan>> {
    // Tokens alternate between whitespace and words; a word may cross spans.
    let mut tokens: Vec<(bool, Vec<StyledSpan>)> = Vec::new();
    for span in spans {
        let mut chunk = String::new();
        let mut chunk_ws = None;
        let push = |tokens: &mut Vec<(bool, Vec<StyledSpan>)>, ws: bool, text: String| {
            let piece = StyledSpan {
                text,
                ..span.clone()
            };
            match tokens.last_mut() {
                Some((last_ws, pieces)) if *last_ws == ws => pieces.push(piece),
                _ => tokens.push((ws, vec![piece])),
            }
        };
        for ch in span.text.chars() {
            let ws = ch.is_whitespace();
            if chunk_ws.is_some_and(|w| w != ws) {
                push(&mut tokens, !ws, std::mem::take(&mut chunk));
            }
            chunk_ws = Some(ws);
            chunk.push(ch);
        }
        if let Some(ws) = chunk_ws {
            push(&mut tokens, ws, chunk);
        }
    }

    let width = width.max(1);
    let piece_width = |p: &[StyledSpan]| p.iter().map(StyledSpan::width).sum::<usize>();
    let mut lines = Vec::new();
    let mut cur: Vec<StyledSpan> = Vec::new();
    let mut used = 0;
    let mut gap: Option<Vec<StyledSpan>> = None;
    for (ws, pieces) in tokens {
        if ws {
            if used > 0 {
                gap = Some(pieces);
            }
            continue;
        }
        let word_w = piece_width(&pieces);
        let gap_w = gap.as_deref().map_or(0, piece_width);
        if used > 0 && used + gap_w + word_w > width {
            lines.push(std::mem::take(&mut cur));
            used = 0;
        } else if let Some(g) = gap.take() {
            cur.extend(g);
            used += gap_w;
        }
        gap = None;
        if used + word_w <= width {
            cur.extend(pieces);
            used += word_w;
            continue;
        }
        for piece in pieces {
            for ch in piece.text.chars() {
                let cw = ch.width().unwrap_or(0);
                if used > 0 && used + cw > width {
                    lines.push(std::mem::take(&mut cur));
                    used = 0;
                }
                match cur.last_mut() {
                    Some(last) if same_style(last, &piece) => last.text.push(ch),
                    _ => cur.push(StyledSpan {
                        text: ch.to_string(),
                        ..piece.clone()
                    }),
                }
                used += cw;
            }
        }
    }
    if !cur.is_empty() || lines.is_empty() {
        lines.push(cur);
    }
    lines.into_iter().map(merge_runs).collect()
}

/// Joins neighboring spans that share a style.
fn merge_runs(spans: Vec<StyledSpan>) -> Vec<StyledSpan> {
    let mut out: Vec<StyledSpan> = Vec::with_capacity(spans.len());
    for span in spans {
        match out.last_mut() {
            Some(last) if same_style(last, &span) => last.text.push_str(&span.text),
            _ => out.push(span),
        }
    }
    out
}

fn same_style(a: &StyledSpan, b: &StyledSpan) -> bool {
    StyledSpan {
        text: String::new(),
        ..a.clone()
    } == StyledSpan {
        text: String::new(),
        ..b.clone()
    }
}

/// Inline-formatted text wrapped under a prefix: `first` leads the first row,
/// `rest` every following row (both must be the same width).
pub(crate) fn rich_text(
    ctx: &Ctx,
    text: &str,
    first: &[StyledSpan],
    rest: &[StyledSpan],
) -> Vec<StyledLine> {
    let prefix_w: usize = first.iter().map(StyledSpan::width).sum();
    wrap_spans(&ctx.inline(text), ctx.width.saturating_sub(prefix_w))
        .into_iter()
        .enumerate()
        .map(|(i, spans)| {
            let mut out = if i == 0 {
                first.to_vec()
            } else {
                rest.to_vec()
            };
            out.extend(spans);
            line(out)
        })
        .collect()
}

pub(crate) fn paragraph(ctx: &Ctx, text: &str) -> Vec<StyledLine> {
    rich_text(ctx, text, &[], &[])
}

static ORDERED_MARKER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d+[.)])\s+").unwrap());

/// A bullet list. `spaced` puts a blank line between top-level items.
pub(crate) fn bullets(ctx: &Ctx, items: &[Bullet], spaced: bool) -> Vec<StyledLine> {
    let mut out = Vec::new();
    for (i, b) in items.iter().enumerate() {
        if spaced && i > 0 && b.depth == 0 {
            out.push(StyledLine::empty());
        }
        let indent = "  ".repeat(b.depth.min(3));
        let (marker, text, color) = match (b.task(), ORDERED_MARKER.find(&b.text)) {
            (Some((true, text)), _) => ("✓".to_string(), text, ctx.pal.accent),
            (Some((false, text)), _) => ("☐".to_string(), text, ctx.pal.muted),
            (None, Some(m)) => (
                m.as_str().trim_end().to_string(),
                &b.text[m.end()..],
                ctx.pal.accent,
            ),
            (None, None) => (
                ["•", "◦", "▪"][b.depth.min(2)].to_string(),
                b.text.as_str(),
                ctx.pal.accent,
            ),
        };
        let hang = [StyledSpan::new(
            &" ".repeat(indent.len() + marker.width() + 1),
        )];
        let first = [
            StyledSpan::new(&indent),
            StyledSpan::new(&marker).with_fg(color).bold(),
            StyledSpan::new(" "),
        ];
        out.extend(rich_text(ctx, text, &first, &hang));
    }
    out
}

pub(crate) fn section_label(ctx: &Ctx, section: &str) -> StyledLine {
    line(vec![
        StyledSpan::new(&section.to_uppercase()).with_fg(ctx.pal.muted)
    ])
}

/// The slide title, honoring FIGlet (`ascii_title`), OSC 66 scaling, and decoration.
pub(crate) fn title(
    ctx: &Ctx,
    text: &str,
    ascii: bool,
    scale: Option<u8>,
    decoration: Option<&str>,
) -> Vec<StyledLine> {
    let accent = |t: &str| StyledSpan::new(t).with_fg(ctx.pal.accent).bold();
    if ascii {
        if let Some(lines) = figlet(ctx, text) {
            return lines;
        }
    }
    let width = text.width();
    if let Some(s) = scale.filter(|&s| ctx.osc66 && s >= 2 && width * s as usize <= ctx.width) {
        let mut span = accent(text);
        span.text_scale = s;
        let mut out = vec![line(vec![span])];
        out.extend((1..s).map(|_| StyledLine::empty()));
        return out;
    }
    let wrapped = wrap_text(text, ctx.width);
    let widest = wrapped.iter().map(|l| l.width()).max().unwrap_or(0);
    match decoration {
        Some("underline") => {
            let mut out: Vec<_> = wrapped.iter().map(|l| line(vec![accent(l)])).collect();
            out.push(line(vec![
                StyledSpan::new(&"━".repeat(widest)).with_fg(ctx.pal.accent)
            ]));
            out
        }
        Some("box") if widest + 4 <= ctx.width => {
            let edge = |l: &str, r: &str| {
                line(vec![StyledSpan::new(&format!(
                    "{l}{}{r}",
                    "─".repeat(widest + 2)
                ))
                .with_fg(ctx.pal.accent)])
            };
            let mut out = vec![edge("╭", "╮")];
            for l in &wrapped {
                let pad = " ".repeat(widest - l.width());
                out.push(line(vec![
                    StyledSpan::new("│ ").with_fg(ctx.pal.accent),
                    accent(l),
                    StyledSpan::new(&format!("{pad} │")).with_fg(ctx.pal.accent),
                ]));
            }
            out.push(edge("╰", "╯"));
            out
        }
        Some("banner") => wrapped
            .iter()
            .map(|l| {
                let left = (ctx.width.saturating_sub(l.width())) / 2;
                let right = ctx.width.saturating_sub(left + l.width());
                let text = format!("{}{l}{}", " ".repeat(left), " ".repeat(right));
                line(vec![StyledSpan::new(&text)
                    .with_fg(ctx.pal.bg)
                    .with_bg(ctx.pal.accent)
                    .bold()])
            })
            .collect(),
        _ => wrapped.iter().map(|l| line(vec![accent(l)])).collect(),
    }
}

/// FIGlet art for `text`, splitting words across rows when too wide; `None`
/// when even a single word does not fit.
pub(crate) fn figlet(ctx: &Ctx, text: &str) -> Option<Vec<StyledLine>> {
    let render = |t: &str| -> Option<Vec<String>> {
        let art = ctx.figfont.convert(t)?.to_string();
        let mut rows: Vec<String> = art.lines().map(|l| l.trim_end().to_string()).collect();
        while rows.last().is_some_and(|r| r.is_empty()) {
            rows.pop();
        }
        let w = rows.iter().map(|r| r.width()).max().unwrap_or(0);
        (w <= ctx.width).then_some(rows)
    };
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut rows = Vec::new();
    let mut i = 0;
    while i < words.len() {
        // Greedily take the most words that still fit on one FIGlet row.
        let mut j = words.len();
        let art = loop {
            if j == i {
                return None;
            }
            if let Some(art) = render(&words[i..j].join(" ")) {
                break art;
            }
            j -= 1;
        };
        rows.extend(art);
        i = j;
    }
    Some(
        rows.into_iter()
            .map(|r| StyledLine {
                spans: vec![StyledSpan::new(&r).with_fg(ctx.pal.accent).bold()],
                content_type: LineContentType::FigletTitle,
            })
            .collect(),
    )
}

/// A code block on a tinted panel with a header showing the label and language.
/// `emphasis` lists 1-based line ranges to stand out; the other lines dim.
pub(crate) fn code_block(
    ctx: &Ctx,
    cb: &CodeBlock,
    emphasis: Option<&Vec<(usize, usize)>>,
) -> Vec<StyledLine> {
    let (pal, width) = (ctx.pal, ctx.width);
    let emphasis = emphasis.filter(|ranges| !ranges.is_empty());
    let lifted = interpolate_color(pal.code_bg, pal.accent, 0.12);
    let row_panel = |mut spans: Vec<StyledSpan>, bg| {
        let used: usize = spans.iter().map(StyledSpan::width).sum();
        spans.push(StyledSpan::new(&" ".repeat(width.saturating_sub(used))));
        for s in &mut spans {
            s.bg.get_or_insert(bg);
        }
        line(spans)
    };
    let panel = |spans| row_panel(spans, pal.code_bg);
    let lang = cb.language.to_lowercase();
    let label = if cb.label.is_empty() {
        ""
    } else {
        cb.label.as_str()
    };
    let header_right = format!("{lang}  ");
    let header_left = ellipsize(
        &format!("  {label}"),
        width.saturating_sub(header_right.width() + 1),
    );
    let gap = width.saturating_sub(header_left.width() + header_right.width());
    let mut out = vec![panel(vec![
        StyledSpan::new(&header_left).with_fg(pal.accent).bold(),
        StyledSpan::new(&" ".repeat(gap)),
        StyledSpan::new(&header_right).with_fg(pal.muted),
    ])];

    let body_width = width.saturating_sub(4).max(1);
    for (n, hl) in ctx
        .highlighter
        .highlight(&cb.code, &cb.language, pal.is_dark())
        .into_iter()
        .enumerate()
    {
        let focus = emphasis.map(|r| r.iter().any(|&(a, b)| (a..=b).contains(&(n + 1))));
        let spans: Vec<StyledSpan> = hl
            .iter()
            .map(|s| match focus {
                Some(false) => {
                    StyledSpan::new(&s.text).with_fg(interpolate_color(s.fg, pal.code_bg, 0.6))
                }
                _ => StyledSpan::new(&s.text).with_fg(s.fg),
            })
            .collect();
        for (i, chunk) in wrap_code(&spans, body_width).into_iter().enumerate() {
            let (bar, bg) = match focus {
                Some(true) => (StyledSpan::new("▌").with_fg(pal.accent), lifted),
                _ => (StyledSpan::new(" "), pal.code_bg),
            };
            let lead = if i == 0 { " " } else { " ↪ " };
            let mut row = vec![bar, StyledSpan::new(lead).with_fg(pal.muted)];
            row.extend(chunk);
            out.push(row_panel(row, bg));
        }
    }
    out.push(panel(Vec::new()));

    if ctx.allow_exec {
        if let Some(mode) = cb.exec_mode {
            let what = if mode == ExecMode::Pty {
                " run in terminal"
            } else {
                " run"
            };
            let mut badge = line(vec![
                StyledSpan::new("  ▶ ").with_fg(pal.accent),
                StyledSpan::new("Ctrl+E").with_fg(pal.accent).bold(),
                StyledSpan::new(what).with_fg(pal.muted),
            ]);
            if badge.width() > width {
                badge.spans.pop();
            }
            out.push(badge);
        }
    }
    out
}

/// Splits highlighted code into rows of at most `width` columns, preserving
/// indentation on the first row and never stalling on wide characters.
fn wrap_code(spans: &[StyledSpan], width: usize) -> Vec<Vec<StyledSpan>> {
    let mut rows = vec![Vec::new()];
    let mut used = 0;
    let mut limit = width;
    for span in spans {
        let mut buf = String::new();
        for ch in span.text.chars() {
            let cw = ch.width().unwrap_or(0);
            if used > 0 && used + cw > limit {
                if !buf.is_empty() {
                    rows.last_mut().unwrap().push(StyledSpan {
                        text: std::mem::take(&mut buf),
                        ..span.clone()
                    });
                }
                rows.push(Vec::new());
                used = 0;
                // Continuation rows carry a two-column marker.
                limit = width.saturating_sub(2).max(1);
            }
            buf.push(ch);
            used += cw;
        }
        if !buf.is_empty() {
            rows.last_mut().unwrap().push(StyledSpan {
                text: buf,
                ..span.clone()
            });
        }
    }
    rows
}

/// Program output under the block that produced it.
pub(crate) fn exec_output(ctx: &Ctx, output: &str, running: bool) -> Vec<StyledLine> {
    let pal = ctx.pal;
    let bar = || StyledSpan::new("  │ ").with_fg(pal.accent);
    let mut out = Vec::new();
    for raw in output.lines() {
        for spans in wrap_spans(&parse_ansi_line(raw), ctx.width.saturating_sub(4)) {
            let mut row = vec![bar()];
            row.extend(spans);
            out.push(line(row));
        }
    }
    if running {
        out.push(line(vec![
            bar(),
            StyledSpan::new("running…").with_fg(pal.muted).italic(),
        ]));
    }
    out
}

pub(crate) fn quote(ctx: &Ctx, q: &BlockQuote) -> Vec<StyledLine> {
    if let Some((kind, heading)) = &q.callout {
        return callout(ctx, &q.lines, *kind, heading);
    }
    let bar = [StyledSpan::new("┃ ").with_fg(ctx.pal.accent)];
    q.lines
        .iter()
        .flat_map(|l| {
            let attribution = l.trim_start().starts_with("— ") || l.trim_start().starts_with("-- ");
            let mut lines = rich_text(ctx, l, &bar, &bar);
            for s in lines.iter_mut().flat_map(|ln| ln.spans.iter_mut().skip(1)) {
                s.italic = true;
                if attribution {
                    s.fg = Some(ctx.pal.muted);
                }
            }
            lines
        })
        .collect()
}

/// A GitHub-style alert: a tinted panel with a colored bar and heading.
fn callout(ctx: &Ctx, body: &[String], kind: Callout, heading: &str) -> Vec<StyledLine> {
    let color = ctx.pal.callout(kind);
    let tint = interpolate_color(ctx.pal.bg, color, 0.1);
    // Text-presentation symbols only: emoji-capable ones (ℹ ⚠) are drawn two
    // cells wide by some terminals, which would break the panel's edge.
    let icon = match kind {
        Callout::Note => "◉",
        Callout::Tip => "✦",
        Callout::Important => "◆",
        Callout::Warning => "▲",
        Callout::Caution => "⬣",
    };
    let bar = [StyledSpan::new("▌ ").with_fg(color)];
    let heading = ellipsize(&format!("{icon} {heading}"), ctx.width.saturating_sub(2));
    let mut out = vec![line(vec![
        bar[0].clone(),
        StyledSpan::new(&heading).with_fg(color).bold(),
    ])];
    for text in body {
        out.extend(rich_text(ctx, text, &bar, &bar));
    }
    for l in &mut out {
        let free = ctx.width.saturating_sub(l.width());
        l.push(StyledSpan::new(&" ".repeat(free)));
        for s in &mut l.spans {
            s.bg.get_or_insert(tint);
        }
    }
    out
}

/// A table with rounded borders; columns shrink and cells wrap to fit.
pub(crate) fn table(ctx: &Ctx, t: &Table) -> Vec<StyledLine> {
    let cols = t
        .headers
        .len()
        .max(t.rows.iter().map(Vec::len).max().unwrap_or(0));
    if cols == 0 {
        return Vec::new();
    }
    let pal = ctx.pal;
    let parse = |row: &[String], header: bool| -> Vec<Vec<StyledSpan>> {
        (0..cols)
            .map(|i| {
                let mut spans = ctx.inline(row.get(i).map(String::as_str).unwrap_or(""));
                if header {
                    for s in &mut spans {
                        s.bold = true;
                        if s.bg.is_none() {
                            s.fg = Some(pal.accent);
                        }
                    }
                }
                spans
            })
            .collect()
    };
    let header = parse(&t.headers, true);
    let body: Vec<Vec<Vec<StyledSpan>>> = t.rows.iter().map(|r| parse(r, false)).collect();
    let span_width = |spans: &[StyledSpan]| spans.iter().map(StyledSpan::width).sum::<usize>();
    let mut widths: Vec<usize> = (0..cols)
        .map(|i| {
            std::iter::once(&header)
                .chain(&body)
                .map(|r| span_width(&r[i]))
                .max()
                .unwrap_or(0)
                .max(1)
        })
        .collect();
    // Borders and padding take 3 columns per cell plus one.
    let budget = ctx.width.saturating_sub(3 * cols + 1);
    while widths.iter().sum::<usize>() > budget {
        let (widest, &w) = widths.iter().enumerate().max_by_key(|(_, w)| **w).unwrap();
        if w <= 3 {
            break;
        }
        widths[widest] = w - 1;
    }

    let border = |s: &str| StyledSpan::new(s).with_fg(pal.muted);
    let rule = |l: &str, m: &str, r: &str| {
        let body: Vec<String> = widths.iter().map(|w| "─".repeat(w + 2)).collect();
        line(vec![border(&format!("{l}{}{r}", body.join(m)))])
    };
    let align = |i: usize| t.alignments.get(i).copied().unwrap_or(TableAlign::Left);
    let row_lines = |cells: &[Vec<StyledSpan>]| -> Vec<StyledLine> {
        let wrapped: Vec<Vec<Vec<StyledSpan>>> = cells
            .iter()
            .enumerate()
            .map(|(i, c)| wrap_spans(c, widths[i]))
            .collect();
        let height = wrapped.iter().map(Vec::len).max().unwrap_or(1);
        (0..height)
            .map(|r| {
                let mut spans = vec![border("│")];
                for (i, lines) in wrapped.iter().enumerate() {
                    let content = lines.get(r).cloned().unwrap_or_default();
                    let free = widths[i].saturating_sub(span_width(&content));
                    let left = match align(i) {
                        TableAlign::Left => 0,
                        TableAlign::Center => free / 2,
                        TableAlign::Right => free,
                    };
                    spans.push(StyledSpan::new(&" ".repeat(left + 1)));
                    spans.extend(content);
                    spans.push(StyledSpan::new(&" ".repeat(free - left + 1)));
                    spans.push(border("│"));
                }
                line(spans)
            })
            .collect()
    };

    let mut out = vec![rule("╭", "┬", "╮")];
    out.extend(row_lines(&header));
    out.push(rule("├", "┼", "┤"));
    for row in &body {
        out.extend(row_lines(row));
    }
    out.push(rule("╰", "┴", "╯"));
    out
}

/// Source listing shown when a Mermaid diagram cannot be rendered.
/// Display math, centered; its one-line form, wrapped, when the layout is
/// too wide.
pub(crate) fn math(ctx: &Ctx, tex: &str) -> Vec<StyledLine> {
    let mut rows = crate::math::display(tex);
    let mut widest = rows.iter().map(|r| r.width()).max().unwrap_or(0);
    if widest > ctx.width {
        rows = wrap_text(&crate::math::inline(tex), ctx.width);
        widest = rows.iter().map(|r| r.width()).max().unwrap_or(0);
    }
    let pad = " ".repeat(ctx.width.saturating_sub(widest) / 2);
    rows.iter()
        .map(|r| {
            line(vec![
                StyledSpan::new(&format!("{pad}{r}")).with_fg(ctx.pal.text)
            ])
        })
        .collect()
}

pub(crate) fn mermaid_fallback(ctx: &Ctx, source: &str, reason: &str) -> Vec<StyledLine> {
    let mut out = vec![line(vec![
        StyledSpan::new("◇ mermaid ").with_fg(ctx.pal.accent).bold(),
        StyledSpan::new(reason).with_fg(ctx.pal.muted).italic(),
    ])];
    out.extend(source.lines().map(|l| {
        line(vec![
            StyledSpan::new("│ ").with_fg(ctx.pal.muted),
            StyledSpan::new(&ellipsize(l, ctx.width.saturating_sub(2))).with_fg(ctx.pal.text),
        ])
    }));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::style::Color;

    fn ctx_with<R>(width: usize, f: impl FnOnce(&Ctx) -> R) -> R {
        let pal = Palette {
            bg: Color::Black,
            text: Color::White,
            accent: Color::Cyan,
            code_bg: Color::DarkGrey,
            muted: Color::Grey,
            surface: Color::DarkGrey,
            gradient: None,
        };
        let hl = Highlighter::new();
        let font = figlet_rs::FIGfont::standard().unwrap();
        f(&Ctx {
            pal: &pal,
            width,
            highlighter: &hl,
            figfont: &font,
            osc66: false,
            allow_exec: true,
            exec: None,
        })
    }

    fn texts(lines: &[StyledLine]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.text.as_str()).collect::<String>())
            .collect()
    }

    #[test]
    fn wrapping_keeps_inline_styles_across_line_breaks() {
        let spans = vec![
            StyledSpan::new("plain "),
            StyledSpan::new("very bold words").bold(),
            StyledSpan::new(" end"),
        ];
        let rows = wrap_spans(&spans, 10);
        let flat: Vec<Vec<(String, bool)>> = rows
            .iter()
            .map(|r| r.iter().map(|s| (s.text.clone(), s.bold)).collect())
            .collect();
        assert_eq!(
            flat,
            [
                vec![("plain ".into(), false), ("very".into(), true)],
                vec![("bold words".into(), true)],
                vec![("end".into(), false)],
            ]
        );
    }

    #[test]
    fn every_block_fits_its_width() {
        let long = "x".repeat(50) + " 日本語テキスト " + &"y".repeat(30);
        for width in [20, 33, 61] {
            ctx_with(width, |ctx| {
                let cb = CodeBlock {
                    language: "go".into(),
                    code: format!("\tfmt.Println(\"{long}\")\n\n界界界"),
                    label: "demo".into(),
                    exec_mode: Some(ExecMode::Exec),
                    highlights: vec![vec![(1, 2)]],
                };
                let t = Table {
                    headers: vec!["Feature".into(), "".into(), "Notes".into()],
                    alignments: vec![TableAlign::Center],
                    rows: vec![vec![long.clone(), "".into(), "café".into()]],
                };
                let blocks: [Vec<StyledLine>; 8] = [
                    paragraph(ctx, &long),
                    bullets(
                        ctx,
                        &[Bullet {
                            text: long.clone(),
                            depth: 2,
                        }],
                        false,
                    ),
                    code_block(ctx, &cb, cb.highlights.first()),
                    exec_output(ctx, &format!("\x1b[31m{long}"), true),
                    table(ctx, &t),
                    quote(
                        ctx,
                        &BlockQuote {
                            lines: vec![long.clone()],
                            callout: None,
                        },
                    ),
                    quote(
                        ctx,
                        &BlockQuote {
                            lines: vec![long.clone()],
                            callout: Some((Callout::Warning, long.clone())),
                        },
                    ),
                    math(
                        ctx,
                        r"\frac{a + b + c + d}{2} = \sum_{i=1}^{n} \sqrt{x_i^2 + y_i^2}",
                    ),
                ];
                for lines in blocks {
                    for l in &lines {
                        assert!(l.width() <= width, "width {width}: {:?}", texts(&lines));
                    }
                }
            });
        }
    }

    #[test]
    fn table_rows_line_up_with_wide_and_empty_cells() {
        ctx_with(60, |ctx| {
            let t = Table {
                headers: vec!["A".into(), "B".into(), "C".into()],
                alignments: vec![TableAlign::Left, TableAlign::Center, TableAlign::Right],
                rows: vec![vec!["日本語".into(), "".into(), "café".into()]],
            };
            let lines = table(ctx, &t);
            let widths: Vec<usize> = lines.iter().map(StyledLine::width).collect();
            assert!(
                widths.iter().all(|&w| w == widths[0]),
                "{:?}",
                texts(&lines)
            );
        });
    }

    #[test]
    fn bullets_use_depth_glyphs_and_keep_ordered_markers() {
        ctx_with(40, |ctx| {
            let items = [
                Bullet {
                    text: "top".into(),
                    depth: 0,
                },
                Bullet {
                    text: "nested".into(),
                    depth: 1,
                },
                Bullet {
                    text: "2. second".into(),
                    depth: 0,
                },
            ];
            assert_eq!(
                texts(&bullets(ctx, &items, true)),
                ["• top", "  ◦ nested", "", "2. second"]
            );
        });
    }

    #[test]
    fn figlet_titles_split_words_to_fit_and_tag_lines() {
        ctx_with(40, |ctx| {
            let lines = figlet(ctx, "Hello World").unwrap();
            assert!(lines.iter().all(|l| l.width() <= 40));
            assert!(lines
                .iter()
                .all(|l| l.content_type == LineContentType::FigletTitle));
            assert!(figlet(&ctx.with_width(3), "Hello").is_none());
        });
    }
}
