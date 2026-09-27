//! Assembles the full screen for the current state.

use crate::presentation::FooterAlign;
use crate::render::animation::{
    render_entrance_frame, render_loop_frame, render_transition_frame, AnimationKind,
};
use crate::render::text::{ellipsize, StyledLine, StyledSpan};

use super::display::{PlacedImage, Row, Screen};
use super::types::{Layout, Mode};
use super::Presenter;

/// Milliseconds per loop-animation frame (~30 fps).
const LOOP_FRAME_MS: u128 = 33;

impl Presenter {
    /// Rows taken at the bottom: (status/prompt bar, notes panel, footer).
    fn chrome_rows(&self) -> (usize, usize, usize) {
        let slide = &self.slides[self.current];
        let prompt = matches!(self.mode, Mode::Command | Mode::Goto | Mode::Search);
        let bar = usize::from(!self.fullscreen || prompt);
        let notes = if self.show_notes && !slide.notes.trim().is_empty() {
            (usize::from(self.height) / 3).clamp(4, 10)
        } else {
            0
        };
        (bar, notes, usize::from(slide.footer.is_some()))
    }

    pub(crate) fn layout(&self) -> Layout {
        let (width, height) = (usize::from(self.width), usize::from(self.height));
        let (bar, notes, footer) = self.chrome_rows();
        let content_top = 1;
        // One spare row above whatever sits at the bottom.
        let reserved = content_top + footer + notes + bar + 1;
        let content_width = (width * usize::from(self.scale) / 100).clamp(1, width.max(1));
        Layout {
            width,
            height,
            content_top,
            content_rows: height.saturating_sub(reserved).max(1),
            content_width,
            margin: (width - content_width) / 2,
        }
    }

    pub(crate) fn compose(&mut self) -> Screen {
        let (width, height) = (usize::from(self.width), usize::from(self.height));
        let blank_rows = |lines: Vec<StyledLine>, pal: super::palette::Palette| Screen {
            rows: lines
                .into_iter()
                .map(|line| Row { line, bg: pal.bg })
                .collect(),
            images: Vec::new(),
        };
        if self.blank {
            return blank_rows(vec![StyledLine::empty(); height], self.palette);
        }
        match self.mode {
            Mode::Help => return blank_rows(self.help_screen(width, height), self.palette),
            Mode::Overview => return blank_rows(self.overview_screen(width, height), self.palette),
            _ => {}
        }

        let layout = self.layout();
        let frame = self.slide_frame(&layout);
        let pad = StyledSpan::new(&" ".repeat(layout.margin));
        let mut lines: Vec<StyledLine> = frame
            .lines
            .iter()
            .map(|l| {
                let mut spans = Vec::with_capacity(l.spans.len() + 1);
                spans.push(pad.clone());
                spans.extend(l.spans.iter().cloned());
                StyledLine {
                    spans,
                    content_type: l.content_type,
                }
            })
            .collect();

        self.max_scroll = lines.len().saturating_sub(layout.content_rows);
        self.scroll = self.scroll.min(self.max_scroll);
        let visible = self.scroll..(self.scroll + layout.content_rows).min(lines.len());
        lines = lines.drain(visible.clone()).collect();
        let animating = self.animate(&mut lines, &layout);
        self.last_lines.clone_from(&lines);

        let pal = self.palette;
        let row_bg = |row: usize| pal.row_bg(row, height);
        let mut rows: Vec<Row> = (0..height)
            .map(|r| Row {
                line: StyledLine::empty(),
                bg: row_bg(r),
            })
            .collect();
        for (i, line) in lines.into_iter().enumerate() {
            if let Some(row) = rows.get_mut(layout.content_top + i) {
                row.line = line;
            }
        }
        let marker = |row: &mut Row, glyph: &str| {
            let at = layout.margin + layout.content_width;
            let used = row.line.width();
            if used <= at.saturating_sub(1) {
                row.line
                    .push(StyledSpan::new(&" ".repeat(at.saturating_sub(used + 1))));
                row.line.push(StyledSpan::new(glyph).with_fg(pal.muted));
            }
        };
        if self.scroll > 0 {
            marker(&mut rows[layout.content_top], "▲");
        }
        if self.scroll < self.max_scroll {
            marker(&mut rows[layout.content_top + layout.content_rows - 1], "▼");
        }

        let (bar, notes_rows, _) = self.chrome_rows();
        let mut bottom = height;
        if bar == 1 && height > 0 {
            bottom -= 1;
            rows[bottom].line = match self.mode {
                Mode::Command | Mode::Goto | Mode::Search => self.prompt_bar(width),
                _ => self.status_bar(width),
            };
        }
        if notes_rows > 0 && bottom > notes_rows {
            let panel = self.notes_panel(width, notes_rows);
            bottom -= panel.len();
            for (i, line) in panel.into_iter().enumerate() {
                rows[bottom + i].line = line;
            }
        }
        if let Some(text) = self.slides[self.current]
            .footer
            .as_ref()
            .filter(|_| bottom > 0)
        {
            let text = ellipsize(text, layout.content_width);
            let free = layout
                .content_width
                .saturating_sub(unicode_width::UnicodeWidthStr::width(text.as_str()));
            let left = layout.margin
                + match self.slides[self.current].footer_align {
                    FooterAlign::Left => 0,
                    FooterAlign::Center => free / 2,
                    FooterAlign::Right => free,
                };
            rows[bottom - 1].line = StyledLine {
                spans: vec![
                    StyledSpan::new(&" ".repeat(left)),
                    StyledSpan::new(&text).with_fg(pal.muted),
                ],
                ..StyledLine::default()
            };
        }

        let images = if animating {
            Vec::new()
        } else {
            self.place_images(&frame.images, &layout)
        };
        Screen { rows, images }
    }

    /// Maps frame images to screen cells, dropping any not fully visible.
    fn place_images(
        &self,
        images: &[super::types::FrameImage],
        layout: &Layout,
    ) -> Vec<PlacedImage> {
        images
            .iter()
            .filter_map(|img| {
                let line = if img.pinned_right {
                    img.line
                } else {
                    img.line.checked_sub(self.scroll)?
                };
                if line + img.rows > layout.content_rows {
                    return None;
                }
                Some(PlacedImage {
                    row: (layout.content_top + line) as u16,
                    col: (layout.margin + img.col) as u16,
                    rows: img.rows as u16,
                    kind: img.kind.clone(),
                })
            })
            .collect()
    }

    /// Applies the running transition/entrance, or the slide's loop animations.
    /// Returns whether a one-shot animation is in progress.
    fn animate(&mut self, lines: &mut Vec<StyledLine>, layout: &Layout) -> bool {
        let pal = self.palette;
        if let Some(anim) = &self.animation {
            let progress = anim.progress();
            *lines = match anim.kind {
                AnimationKind::Transition(t) => render_transition_frame(
                    &anim.old_buffer,
                    lines,
                    progress,
                    t,
                    pal.bg,
                    layout.width,
                    anim.exit_only,
                ),
                AnimationKind::Entrance(e) => render_entrance_frame(lines, progress, e, pal.bg),
            };
            return true;
        }
        let frame = (self.loop_started.elapsed().as_millis() / LOOP_FRAME_MS) as u64;
        for (animation, target) in &self.slides[self.current].loop_animations {
            *lines = render_loop_frame(
                lines,
                *animation,
                frame,
                pal.accent,
                pal.bg,
                layout.width,
                layout.content_rows,
                target.as_deref(),
            );
        }
        false
    }
}
