//! Screen furniture around the slide: status bar, notes, prompts, help, overview.

use crossterm::style::Color;

use crate::presentation::Callout;
use unicode_width::UnicodeWidthStr;

use crate::render::text::{ellipsize, wrap_text, StyledLine, StyledSpan};
use crate::theme::colors::interpolate_color;

use super::palette::Palette;
use super::Presenter;

fn padded(mut spans: Vec<StyledSpan>, width: usize) -> StyledLine {
    let used: usize = spans.iter().map(StyledSpan::width).sum();
    spans.push(StyledSpan::new(&" ".repeat(width.saturating_sub(used))));
    StyledLine {
        spans,
        ..StyledLine::default()
    }
}

/// Colors the first `fill` columns of `line` with `fill_bg` and the rest with
/// `bg`, splitting spans at the boundary. Spans with their own background keep it.
fn two_tone(line: StyledLine, fill: usize, fill_bg: Color, bg: Color) -> StyledLine {
    let mut out = StyledLine::empty();
    let mut col = 0;
    for span in line.spans {
        let w = span.width();
        if span.bg.is_some() || col >= fill || col + w <= fill {
            let tone = if col < fill { fill_bg } else { bg };
            let mut s = span;
            s.bg.get_or_insert(tone);
            out.push(s);
        } else {
            let head = crate::render::text::truncate_to_width(&span.text, fill - col);
            let tail = span.text[head.len()..].to_string();
            out.push(StyledSpan {
                text: head,
                bg: Some(fill_bg),
                ..span.clone()
            });
            out.push(StyledSpan {
                text: tail,
                bg: Some(bg),
                ..span
            });
        }
        col += w;
    }
    out
}

/// How long a notice replaces the deck title in the status bar.
const NOTICE_TIME: std::time::Duration = std::time::Duration::from_millis(2500);

impl Presenter {
    /// Bottom bar: deck title and section on the left; theme, timer, and slide
    /// number on the right. Its background fills left-to-right with progress.
    pub(crate) fn status_bar(&self, width: usize) -> StyledLine {
        let pal = &self.palette;
        let slide = &self.slides[self.current];
        let mut right = Vec::new();
        if self.show_theme_name {
            right.push(StyledSpan::new(&format!("{}   ", self.theme.name)).with_fg(pal.muted));
        }
        if let Some(t) = self.timer_text() {
            let (mut text, mut color) = (format!("◷ {t}"), pal.text);
            if let (Some((behind, over)), Some(total)) = (self.pace(), self.meta.duration) {
                text.push_str(&format!(" / {}", super::state::clock(total)));
                if over {
                    color = pal.callout(Callout::Caution);
                } else if let Some(late) = behind {
                    text.push_str(&format!(" · {} behind", super::state::clock(late)));
                    color = pal.callout(Callout::Warning);
                }
            }
            right.push(StyledSpan::new(&format!("{text}   ")).with_fg(color));
        }
        let steps = slide.steps.len();
        if steps > 0 {
            // One dot per build state; a count once dots would crowd the bar.
            if steps < 8 {
                right.push(StyledSpan::new(&"●".repeat(self.step + 1)).with_fg(pal.accent));
                right.push(StyledSpan::new(&"○".repeat(steps - self.step)).with_fg(pal.muted));
                right.push(StyledSpan::new("   "));
            } else {
                let text = format!("step {} / {}   ", self.step, steps);
                right.push(StyledSpan::new(&text).with_fg(pal.muted));
            }
        }
        right.push(
            StyledSpan::new(&format!("{}", self.current + 1))
                .with_fg(pal.accent)
                .bold(),
        );
        right.push(StyledSpan::new(&format!(" / {} ", self.slides.len())).with_fg(pal.muted));
        let right_w: usize = right.iter().map(StyledSpan::width).sum();

        let title = if self.meta.title.is_empty() {
            self.presentation_path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        } else {
            self.meta.title.clone()
        };
        let mut left_text = format!(" {title}");
        if !self.meta.author.is_empty() {
            left_text.push_str(&format!(" — {}", self.meta.author));
        }
        if !slide.section.is_empty() {
            left_text.push_str(&format!("  ·  {}", slide.section));
        }
        let mut left_fg = pal.text;
        if let Some((text, at)) = &self.notice {
            if at.elapsed() < NOTICE_TIME {
                left_text = format!(" {text}");
                left_fg = pal.accent;
            }
        }
        let left = ellipsize(&left_text, width.saturating_sub(right_w + 2));
        let gap = width.saturating_sub(left.width() + right_w);
        let mut spans = vec![StyledSpan::new(&left).with_fg(left_fg)];
        spans.push(StyledSpan::new(&" ".repeat(gap)));
        spans.extend(right);

        let progress = (self.current + 1) as f64 / self.slides.len().max(1) as f64;
        let fill = (progress * width as f64).round() as usize;
        let fill_bg = interpolate_color(pal.surface, pal.accent, 0.18);
        two_tone(padded(spans, width), fill, fill_bg, pal.surface)
    }

    /// The `:` or goto prompt, drawn in place of the status bar.
    pub(crate) fn prompt_bar(&self, width: usize) -> StyledLine {
        let pal = &self.palette;
        let (label, hint) = match self.mode {
            super::Mode::Goto => ("go to slide ", format!("   1–{}", self.slides.len())),
            super::Mode::Search if self.input.is_empty() && !self.last_search.is_empty() => {
                ("/", format!("   Enter: next “{}”", self.last_search))
            }
            super::Mode::Search => ("/", String::new()),
            _ => (":", String::new()),
        };
        let mut line = padded(
            vec![
                StyledSpan::new(&format!(" {label}"))
                    .with_fg(pal.accent)
                    .bold(),
                StyledSpan::new(&self.input).with_fg(pal.text),
                StyledSpan::new("▏").with_fg(pal.accent),
                StyledSpan::new(&hint).with_fg(pal.muted),
            ],
            width,
        );
        for s in &mut line.spans {
            s.bg = Some(pal.surface);
        }
        line
    }

    /// Speaker notes: a header row plus `rows - 1` wrapped, scrollable lines.
    pub(crate) fn notes_panel(&mut self, width: usize, rows: usize) -> Vec<StyledLine> {
        let pal = self.palette;
        let body: Vec<String> = self.slides[self.current]
            .notes
            .lines()
            .flat_map(|l| wrap_text(l, width.saturating_sub(4)))
            .collect();
        let visible = rows.saturating_sub(1);
        self.notes_scroll = self.notes_scroll.min(body.len().saturating_sub(visible));
        let position = if body.len() > visible {
            format!(
                " {}–{} of {} · N/P ",
                self.notes_scroll + 1,
                (self.notes_scroll + visible).min(body.len()),
                body.len()
            )
        } else {
            String::new()
        };
        let rule = "─".repeat(width.saturating_sub(10 + position.width()));
        let mut out = vec![padded(
            vec![
                StyledSpan::new(" ── ").with_fg(pal.muted),
                StyledSpan::new("Notes ").with_fg(pal.accent).bold(),
                StyledSpan::new(&rule).with_fg(pal.muted),
                StyledSpan::new(&position).with_fg(pal.muted),
            ],
            width,
        )];
        out.extend((0..visible).map(|i| {
            let text = body
                .get(self.notes_scroll + i)
                .map(String::as_str)
                .unwrap_or("");
            padded(
                vec![StyledSpan::new(&format!("  {text}")).with_fg(pal.text)],
                width,
            )
        }));
        for s in out.iter_mut().flat_map(|l| l.spans.iter_mut()) {
            s.bg = Some(pal.surface);
        }
        out
    }

    /// Full-screen key reference.
    pub(crate) fn help_screen(&self, width: usize, height: usize) -> Vec<StyledLine> {
        let pal = &self.palette;
        let font = if self.font.available() {
            Some(("] [  0", "font size bigger / smaller / reset"))
        } else {
            None
        };
        let sections: Vec<(&str, Vec<(&str, &str)>)> = vec![
            (
                "Navigate",
                vec![
                    ("→ l space", "next step or slide"),
                    ("← h ⌫", "previous step or slide"),
                    ("J K", "next / previous section"),
                    ("g", "go to slide number"),
                    ("Home End", "first / last slide"),
                    ("j k ↓ ↑", "scroll (Ctrl+D / Ctrl+U: half page)"),
                    ("o", "overview (J / K there move a slide)"),
                ],
            ),
            (
                "Present",
                vec![
                    ("n", "speaker notes (N / P scroll)"),
                    ("/", "search slides (Enter again: next)"),
                    ("b", "blank the screen"),
                    ("e", "edit this slide in $EDITOR"),
                    ("f", "fullscreen"),
                    ("Ctrl+E", "run code block (again: next block)"),
                    ("t", "start / reset timer"),
                ],
            ),
            (
                "Look",
                vec![
                    ("D", "light / dark variant"),
                    ("T", "show theme name"),
                    ("S", "show section labels"),
                    ("+ -", "content width"),
                    ("> <", "image size"),
                ],
            ),
            (
                "Commands",
                vec![
                    (":theme <slug>", "switch theme"),
                    (":goto <n>", "jump to slide"),
                    (":reload", "re-read the file"),
                    ("q  Ctrl+C", "quit"),
                ],
            ),
        ];
        let key_w = 15;
        let mut body: Vec<StyledLine> = Vec::new();
        for (name, keys) in &sections {
            if !body.is_empty() {
                body.push(StyledLine::empty());
            }
            body.push(padded(
                vec![StyledSpan::new(name).with_fg(pal.accent).bold()],
                0,
            ));
            let extra = if *name == "Look" { font } else { None };
            for (k, what) in keys.iter().copied().chain(extra) {
                body.push(padded(
                    vec![
                        StyledSpan::new(&format!("  {k:<key_w$}"))
                            .with_fg(pal.text)
                            .bold(),
                        StyledSpan::new(what).with_fg(pal.muted),
                    ],
                    0,
                ));
            }
        }
        let card_w = body.iter().map(StyledLine::width).max().unwrap_or(0);
        let footer = format!(
            "{} · {:?} images · press any key",
            self.theme.slug,
            self.images.protocol()
        );
        frame_card(pal, "Ostendo", body, &footer, card_w, width, height)
    }

    /// Paged grid of slide cards with the selection highlighted.
    pub(crate) fn overview_screen(&self, width: usize, height: usize) -> Vec<StyledLine> {
        let pal = &self.palette;
        let card_w = 34.min(width.saturating_sub(4)).max(12);
        let cols = ((width.saturating_sub(4) + 2) / (card_w + 2)).max(1);
        let rows = (height.saturating_sub(4) / 3).max(1);
        let per_page = cols * rows;
        let page = self.overview_sel / per_page;
        let pages = self.slides.len().div_ceil(per_page);
        let start = page * per_page;

        let mut out = vec![StyledLine::empty(); height];
        let byline: Vec<&str> = [
            self.meta.title.as_str(),
            self.meta.author.as_str(),
            self.meta.date.as_str(),
        ]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
        let header = format!(
            " Overview   {} / {}   {}",
            self.overview_sel + 1,
            self.slides.len(),
            byline.join(" · ")
        );
        out[0] = padded(
            vec![StyledSpan::new(&header).with_fg(pal.accent).bold()],
            width,
        );
        let left = (width.saturating_sub(cols * (card_w + 2) - 2)) / 2;
        for (slot, slide) in self.slides.iter().enumerate().skip(start).take(per_page) {
            let i = slot - start;
            let (r, c) = (i % rows, i / rows);
            let y = 2 + r * 3;
            if y + 1 >= height.saturating_sub(1) {
                continue;
            }
            let selected = slot == self.overview_sel;
            let (fg, bg) = if selected {
                (pal.bg, pal.accent)
            } else {
                (pal.text, pal.surface)
            };
            let title = if slide.title.is_empty() {
                "(untitled)"
            } else {
                slide.title.as_str()
            };
            let top = ellipsize(&format!(" {:>2}  {title}", slot + 1), card_w);
            let sub = ellipsize(&format!("     {}", slide.section), card_w);
            for (dy, text, color) in [
                (0, top, fg),
                (1, sub, if selected { fg } else { pal.muted }),
            ] {
                let row = &mut out[y + dy];
                let used = row.width();
                let x = left + c * (card_w + 2);
                row.push(StyledSpan::new(&" ".repeat(x.saturating_sub(used))));
                let mut span = StyledSpan::new(&format!("{text:<card_w$}"))
                    .with_fg(color)
                    .with_bg(bg);
                span.bold = selected && dy == 0;
                row.push(span);
            }
        }
        let hint = format!(
            " ←↑↓→ select   J K reorder   Enter open   Esc back{}",
            if pages > 1 {
                format!("   page {} / {pages}", page + 1)
            } else {
                String::new()
            }
        );
        out[height - 1] = padded(vec![StyledSpan::new(&hint).with_fg(pal.muted)], width);
        out
    }
}

/// Centers `body` in a titled card on a blank screen.
fn frame_card(
    pal: &Palette,
    title: &str,
    body: Vec<StyledLine>,
    footer: &str,
    body_w: usize,
    width: usize,
    height: usize,
) -> Vec<StyledLine> {
    let inner = (body_w.max(footer.width()) + 4).min(width.saturating_sub(2));
    let left = " ".repeat(width.saturating_sub(inner + 2) / 2);
    let border = |s: String| StyledSpan::new(&s).with_fg(pal.muted);
    let mut card = Vec::new();
    let title_rule = "─".repeat(inner.saturating_sub(title.width() + 3));
    card.push(StyledLine {
        spans: vec![
            StyledSpan::new(&left),
            border("╭─ ".into()),
            StyledSpan::new(title).with_fg(pal.accent).bold(),
            border(format!(" {title_rule}╮")),
        ],
        ..StyledLine::default()
    });
    let row = |spans: Vec<StyledSpan>| {
        let used: usize = spans.iter().map(StyledSpan::width).sum();
        let mut all = vec![StyledSpan::new(&left), border("│  ".into())];
        all.extend(spans);
        all.push(StyledSpan::new(&" ".repeat(inner.saturating_sub(used + 2))));
        all.push(border("│".into()));
        StyledLine {
            spans: all,
            ..StyledLine::default()
        }
    };
    card.push(row(Vec::new()));
    for line in body {
        card.push(row(line.spans));
    }
    card.push(row(Vec::new()));
    card.push(row(vec![StyledSpan::new(&ellipsize(
        footer,
        inner.saturating_sub(4),
    ))
    .with_fg(pal.muted)
    .italic()]));
    card.push(StyledLine {
        spans: vec![
            StyledSpan::new(&left),
            border(format!("╰{}╯", "─".repeat(inner))),
        ],
        ..StyledLine::default()
    });
    let top = height.saturating_sub(card.len()) / 2;
    let mut out = vec![StyledLine::empty(); top];
    out.extend(card.into_iter().take(height));
    out.resize(height, StyledLine::empty());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_tone_splits_spans_at_the_fill_column() {
        let line = padded(vec![StyledSpan::new("ab日c")], 6);
        let out = two_tone(line, 3, Color::Red, Color::Blue);
        let parts: Vec<(&str, Option<Color>)> =
            out.spans.iter().map(|s| (s.text.as_str(), s.bg)).collect();
        assert_eq!(parts[0], ("ab", Some(Color::Red)));
        assert_eq!(parts[1], ("日c", Some(Color::Blue)));
    }
}
