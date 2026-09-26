//! Writes composed screens to the terminal, rewriting only rows that changed.
//!
//! Every frame is composed in full, but only rows whose encoded bytes differ
//! from the previous frame are sent. That keeps output tiny (timer ticks touch
//! one row), avoids flicker, and leaves protocol images alone unless the text
//! under them was rewritten.

use crossterm::cursor::MoveTo;
use crossterm::queue;
use crossterm::style::{Color, SetBackgroundColor, SetForegroundColor};
use crossterm::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{self, Write};
use std::rc::Rc;

use crate::image_util::kitty;
use crate::render::text::{StyledLine, StyledSpan};

/// One terminal row: its content plus the background used for unstyled cells.
pub(crate) struct Row {
    pub line: StyledLine,
    pub bg: Color,
}

/// An image to show at a fixed screen cell.
#[derive(Clone)]
pub(crate) struct PlacedImage {
    pub row: u16,
    pub col: u16,
    pub rows: u16,
    pub kind: ImageKind,
}

#[derive(Clone)]
pub(crate) enum ImageKind {
    /// Already transmitted to Kitty; placed by id.
    Kitty { id: u32, cols: u16 },
    /// iTerm2 / Sixel escape data drawn into the text layer.
    Inline(Rc<str>),
}

pub(crate) struct Screen {
    pub rows: Vec<Row>,
    pub images: Vec<PlacedImage>,
}

/// What was last written, used to skip unchanged rows and images.
#[derive(Default)]
pub(crate) struct Display {
    rows: Vec<u64>,
    kitty: Vec<(u32, u16, u16)>,
    inline: Vec<u64>,
    width: u16,
}

impl Display {
    /// Forces the next `present` to repaint every row and re-emit every image.
    pub fn invalidate(&mut self) {
        self.rows.clear();
        self.inline.clear();
    }

    pub fn present(
        &mut self,
        screen: &Screen,
        width: u16,
        default_fg: Color,
        out: &mut impl Write,
    ) -> io::Result<()> {
        if width != self.width || screen.rows.len() != self.rows.len() {
            self.invalidate();
            self.width = width;
            self.rows.resize(screen.rows.len(), 0);
            // A zero hash never matches an encoded row, so every row is written.
        }
        let mut buf = Vec::with_capacity(64 * 1024);
        queue!(buf, BeginSynchronizedUpdate)?;

        let mut rewritten = vec![false; screen.rows.len()];
        let mut scratch = Vec::with_capacity(1024);
        let mut deferred = Vec::new();
        for (i, row) in screen.rows.iter().enumerate() {
            scratch.clear();
            encode_row(&mut scratch, row, width as usize, default_fg)?;
            let hash = hash_bytes(&scratch);
            if self.rows[i] == hash {
                continue;
            }
            self.rows[i] = hash;
            rewritten[i] = true;
            // Scaled (OSC 66) text spans several rows; write it after the rows it
            // covers so their blank cells don't erase it.
            if row.line.spans.iter().any(|s| s.text_scale >= 2) {
                deferred.push((i, scratch.clone()));
                continue;
            }
            queue!(buf, MoveTo(0, i as u16))?;
            buf.extend_from_slice(&scratch);
        }
        for (i, bytes) in deferred {
            queue!(buf, MoveTo(0, i as u16))?;
            buf.extend_from_slice(&bytes);
        }

        self.place_kitty(&screen.images, &mut buf)?;
        self.place_inline(&screen.images, &rewritten, &mut buf)?;

        queue!(buf, EndSynchronizedUpdate)?;
        out.write_all(&buf)?;
        out.flush()
    }

    fn place_kitty(&mut self, images: &[PlacedImage], buf: &mut Vec<u8>) -> io::Result<()> {
        let wanted: Vec<(u32, u16, u16)> = images
            .iter()
            .filter_map(|img| match img.kind {
                ImageKind::Kitty { id, .. } => Some((id, img.row, img.col)),
                ImageKind::Inline(_) => None,
            })
            .collect();
        for gone in self.kitty.iter().filter(|k| !wanted.contains(k)) {
            buf.extend_from_slice(kitty::delete_placements(gone.0).as_bytes());
        }
        for img in images {
            if let ImageKind::Kitty { id, cols } = img.kind {
                if !self.kitty.contains(&(id, img.row, img.col)) {
                    queue!(buf, MoveTo(img.col, img.row))?;
                    let place = kitty::placement_escape(id, 1, cols as usize, img.rows as usize);
                    buf.extend_from_slice(place.as_bytes());
                }
            }
        }
        self.kitty = wanted;
        Ok(())
    }

    fn place_inline(
        &mut self,
        images: &[PlacedImage],
        rewritten: &[bool],
        buf: &mut Vec<u8>,
    ) -> io::Result<()> {
        let mut placed = Vec::new();
        for img in images {
            let ImageKind::Inline(ref data) = img.kind else {
                continue;
            };
            let key = {
                let mut h = DefaultHasher::new();
                (img.row, img.col, Rc::as_ptr(data)).hash(&mut h);
                h.finish()
            };
            let span = img.row as usize..(img.row + img.rows) as usize;
            let touched = span
                .clone()
                .any(|r| rewritten.get(r).copied().unwrap_or(false));
            if touched || !self.inline.contains(&key) {
                queue!(buf, MoveTo(img.col, img.row))?;
                buf.extend_from_slice(data.as_bytes());
            }
            placed.push(key);
        }
        self.inline = placed;
        Ok(())
    }
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    // Reserve 0 for "never written".
    h.finish().max(1)
}

#[derive(Clone, Copy, PartialEq, Default)]
struct Attrs {
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    strike: bool,
}

impl Attrs {
    fn of(span: &StyledSpan) -> Self {
        Self {
            bold: span.bold,
            dim: span.dim,
            italic: span.italic,
            underline: span.underline,
            strike: span.strikethrough,
        }
    }
}

/// Encodes a row as SGR + text, emitting only style changes between spans and
/// padding to `width` with the row background.
fn encode_row(buf: &mut Vec<u8>, row: &Row, width: usize, default_fg: Color) -> io::Result<()> {
    buf.extend_from_slice(b"\x1b[0m");
    queue!(
        buf,
        SetBackgroundColor(row.bg),
        SetForegroundColor(default_fg)
    )?;
    let (mut fg, mut bg, mut attrs) = (default_fg, row.bg, Attrs::default());
    let mut used = 0;
    for span in &row.line.spans {
        if used >= width {
            break;
        }
        let want = Attrs::of(span);
        write_attrs(buf, attrs, want);
        attrs = want;
        let span_fg = span.fg.unwrap_or(default_fg);
        if span_fg != fg {
            queue!(buf, SetForegroundColor(span_fg))?;
            fg = span_fg;
        }
        let span_bg = span.bg.unwrap_or(row.bg);
        if span_bg != bg {
            queue!(buf, SetBackgroundColor(span_bg))?;
            bg = span_bg;
        }
        let scale = usize::from(span.text_scale.max(1));
        let text = if span.width() + used > width {
            crate::render::text::truncate_to_width(&span.text, (width - used) / scale)
        } else {
            span.text.clone()
        };
        used += unicode_width::UnicodeWidthStr::width(text.as_str()) * scale;
        if scale >= 2 {
            write!(buf, "\x1b]66;s={scale};{text}\x07")?;
        } else {
            buf.extend_from_slice(text.as_bytes());
        }
    }
    if used < width {
        write_attrs(buf, attrs, Attrs::default());
        if bg != row.bg {
            queue!(buf, SetBackgroundColor(row.bg))?;
        }
        buf.resize(buf.len() + (width - used), b' ');
    }
    Ok(())
}

fn write_attrs(buf: &mut Vec<u8>, from: Attrs, to: Attrs) {
    if from == to {
        return;
    }
    // Bold and dim share one "off" code (22), so re-assert whichever survives.
    if (from.bold && !to.bold) || (from.dim && !to.dim) {
        buf.extend_from_slice(b"\x1b[22m");
        if to.bold {
            buf.extend_from_slice(b"\x1b[1m");
        }
        if to.dim {
            buf.extend_from_slice(b"\x1b[2m");
        }
    } else {
        if to.bold && !from.bold {
            buf.extend_from_slice(b"\x1b[1m");
        }
        if to.dim && !from.dim {
            buf.extend_from_slice(b"\x1b[2m");
        }
    }
    for (on, was, set, unset) in [
        (to.italic, from.italic, "3", "23"),
        (to.underline, from.underline, "4", "24"),
        (to.strike, from.strike, "9", "29"),
    ] {
        if on != was {
            write!(buf, "\x1b[{}m", if on { set } else { unset }).ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(spans: Vec<StyledSpan>) -> Row {
        Row {
            line: StyledLine {
                spans,
                ..StyledLine::default()
            },
            bg: Color::Black,
        }
    }

    fn screen(rows: Vec<Row>) -> Screen {
        Screen {
            rows,
            images: Vec::new(),
        }
    }

    fn present(d: &mut Display, s: &Screen) -> String {
        let mut out = Vec::new();
        d.present(s, 10, Color::White, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn only_changed_rows_are_rewritten() {
        let mut d = Display::default();
        let first = screen(vec![row(vec![StyledSpan::new("a")]), row(vec![])]);
        let out = present(&mut d, &first);
        assert!(out.contains("\x1b[1;1H") && out.contains("\x1b[2;1H"));

        let second = screen(vec![
            row(vec![StyledSpan::new("a")]),
            row(vec![StyledSpan::new("b")]),
        ]);
        let out = present(&mut d, &second);
        assert!(
            !out.contains("\x1b[1;1H"),
            "unchanged row rewritten: {out:?}"
        );
        assert!(out.contains("\x1b[2;1H"));

        d.invalidate();
        assert!(present(&mut d, &second).contains("\x1b[1;1H"));
    }

    #[test]
    fn rows_are_padded_and_clipped_to_width() {
        let mut buf = Vec::new();
        let r = row(vec![StyledSpan::new("日本語テキスト")]);
        encode_row(&mut buf, &r, 5, Color::White).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.ends_with("日本 "), "{text:?}");
    }

    #[test]
    fn attribute_changes_never_emit_sgr_21() {
        // SGR 21 is double-underline on most terminals, not "bold off".
        let mut buf = Vec::new();
        let r = row(vec![
            StyledSpan::new("b").bold(),
            StyledSpan::new("d").dim(),
            StyledSpan::new("p"),
        ]);
        encode_row(&mut buf, &r, 5, Color::White).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(!text.contains("[21m"));
        assert!(text.contains("\x1b[22m\x1b[2md"), "{text:?}");
    }

    #[test]
    fn kitty_placements_are_moved_and_removed_by_diff() {
        let mut d = Display::default();
        let img = |row| PlacedImage {
            row,
            col: 2,
            rows: 1,
            kind: ImageKind::Kitty { id: 7, cols: 3 },
        };
        let mut s = screen(vec![row(vec![]), row(vec![])]);
        s.images = vec![img(0)];
        assert!(present(&mut d, &s).contains("a=p,i=7,p=1"));
        assert!(
            !present(&mut d, &s).contains("a=p"),
            "unchanged image re-placed"
        );
        s.images = vec![img(1)];
        let out = present(&mut d, &s);
        assert!(out.contains("a=d,d=i,i=7") && out.contains("a=p,i=7,p=1"));
        s.images.clear();
        assert!(present(&mut d, &s).contains("a=d,d=i,i=7"));
    }
}
