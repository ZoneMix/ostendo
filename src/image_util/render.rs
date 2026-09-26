//! Protocol-specific image rendering for terminal display.
//!
//! - **Kitty graphics** -- PNG transmitted once by ID, then placed cheaply
//!   (see [`super::kitty`]). Kitty and Ghostty.
//! - **iTerm2 inline images** -- base64 PNG in an OSC 1337 escape. iTerm2 and
//!   WezTerm.
//! - **Sixel** -- DEC bitmap encoding via `icy_sixel`.
//! - **ASCII art** -- colored characters from [`crate::terminal::ascii_art`];
//!   works everywhere at low resolution.
//!
//! Protocol images are flattened onto the theme background before encoding
//! because Sixel has no alpha channel; a theme change must therefore
//! invalidate cached renders. Protocol escapes carry no cursor movement: the
//! caller positions the cursor at the image's top-left cell before writing.

use base64::Engine;
use crossterm::style::Color;
use image::RgbaImage;
use std::io::Cursor;

use crate::presentation::SlideImage;
use crate::render::layout::WindowSize;
use crate::render::text::{LineContentType, StyledLine, StyledSpan};
use crate::terminal::protocols::ImageProtocol;

/// Wrap an escape for tmux DCS passthrough so it reaches the outer terminal;
/// tmux requires every embedded ESC to be doubled.
fn tmux_wrap(escape: &str) -> String {
    if std::env::var("TMUX").is_err() {
        return escape.to_string();
    }
    let doubled = escape.replace('\x1b', "\x1b\x1b");
    format!("\x1bPtmux;{}\x1b\\", doubled)
}

fn composite_on_bg(img: &RgbaImage, bg_color: Color) -> RgbaImage {
    let (bg_r, bg_g, bg_b) = match bg_color {
        Color::Rgb { r, g, b } => (r, g, b),
        _ => (0, 0, 0),
    };
    let mut out = RgbaImage::new(img.width(), img.height());
    for (x, y, pixel) in img.enumerate_pixels() {
        let [r, g, b, a] = pixel.0;
        let alpha = a as f32 / 255.0;
        let blended_r = (r as f32 * alpha + bg_r as f32 * (1.0 - alpha)) as u8;
        let blended_g = (g as f32 * alpha + bg_g as f32 * (1.0 - alpha)) as u8;
        let blended_b = (b as f32 * alpha + bg_b as f32 * (1.0 - alpha)) as u8;
        out.put_pixel(x, y, image::Rgba([blended_r, blended_g, blended_b, 255]));
    }
    out
}

/// A rendered slide image, sized in terminal cells.
pub enum RenderedImage {
    /// ASCII art rows starting at column 0, mixed into the text buffer.
    Lines(Vec<StyledLine>),
    /// iTerm2 or Sixel escape, written after the text frame at the cursor
    /// position; reserve `rows` blank lines for it.
    Protocol {
        escape_data: String,
        cols: usize,
        rows: usize,
    },
    /// A Kitty image: `transmit_escape` must reach the terminal once before
    /// [`super::kitty::placement_escape`] can show `image_id`.
    KittyPlacement {
        image_id: u32,
        cols: usize,
        rows: usize,
        transmit_escape: String,
    },
}

/// Render `img` for `protocol`, fitting it within `max_cols` x `max_rows`
/// cells. `image` supplies the ASCII color override and caption.
#[allow(clippy::too_many_arguments)]
pub fn render_slide_image(
    img: &RgbaImage,
    image: &SlideImage,
    max_cols: usize,
    max_rows: usize,
    protocol: ImageProtocol,
    text_color: Color,
    bg_color: Color,
    window_size: &WindowSize,
) -> RenderedImage {
    let fit = || {
        let composited = composite_on_bg(img, bg_color);
        crate::image_util::scale_image_pixels(&composited, window_size, max_cols, max_rows)
    };
    let escape = |escape: Option<String>, cols, rows| match escape {
        Some(escape_data) => RenderedImage::Protocol {
            escape_data,
            cols,
            rows,
        },
        None => RenderedImage::Lines(Vec::new()),
    };
    match protocol {
        ImageProtocol::Ascii => render_ascii(img, image, max_cols, max_rows, text_color),
        ImageProtocol::Blocks => {
            render_blocks(img, image, max_cols, max_rows, text_color, bg_color)
        }
        ImageProtocol::Kitty => {
            let (scaled, cols, rows) = fit();
            let image_id = super::kitty::next_image_id();
            match super::kitty::transmit_escape(image_id, &scaled) {
                Some(transmit_escape) => RenderedImage::KittyPlacement {
                    image_id,
                    cols,
                    rows,
                    transmit_escape,
                },
                None => RenderedImage::Lines(Vec::new()),
            }
        }
        ImageProtocol::Iterm2 => {
            let (scaled, cols, rows) = fit();
            escape(iterm2_escape(&scaled, cols, rows), cols, rows)
        }
        ImageProtocol::Sixel => {
            let (scaled, cols, rows) = fit();
            escape(sixel_escape(&scaled), cols, rows)
        }
    }
}

fn render_ascii(
    img: &RgbaImage,
    image: &SlideImage,
    max_cols: usize,
    max_rows: usize,
    text_color: Color,
) -> RenderedImage {
    let color_override = crate::theme::colors::hex_to_color(&image.color_override);
    let rows = max_rows.saturating_sub(usize::from(!image.alt_text.is_empty()));
    let ascii_rows =
        crate::terminal::ascii_art::render_ascii_art(img, max_cols, rows, color_override);
    let mut lines = Vec::with_capacity(ascii_rows.len() + 1);
    for row in &ascii_rows {
        let mut line = StyledLine::empty();
        for cell in row {
            line.push(StyledSpan::new(&cell.ch.to_string()).with_fg(cell.fg));
        }
        line.content_type = LineContentType::AsciiImage;
        lines.push(line);
    }

    if !image.alt_text.is_empty() {
        let mut cap = StyledLine::empty();
        cap.push(
            StyledSpan::new(&format!("  {}", image.alt_text))
                .with_fg(text_color)
                .dim(),
        );
        lines.push(cap);
    }

    RenderedImage::Lines(lines)
}

fn render_blocks(
    img: &RgbaImage,
    image: &SlideImage,
    max_cols: usize,
    max_rows: usize,
    text_color: Color,
    bg_color: Color,
) -> RenderedImage {
    let bg = crate::theme::colors::color_to_rgb(bg_color).unwrap_or((0, 0, 0));
    let rows = max_rows.saturating_sub(usize::from(!image.alt_text.is_empty()));
    let mut lines: Vec<StyledLine> =
        crate::terminal::ascii_art::render_blocks(img, max_cols, rows, bg)
            .into_iter()
            .map(|cells| StyledLine {
                spans: cells
                    .into_iter()
                    .map(|(top, bottom)| StyledSpan::new("▀").with_fg(top).with_bg(bottom))
                    .collect(),
                content_type: LineContentType::AsciiImage,
            })
            .collect();
    if !image.alt_text.is_empty() {
        let width = lines.first().map_or(0, StyledLine::width);
        let caption = crate::render::text::ellipsize(&image.alt_text, width.max(1));
        let pad = " ".repeat(
            width.saturating_sub(unicode_width::UnicodeWidthStr::width(caption.as_str())) / 2,
        );
        lines.push(StyledLine::plain(&format!("{pad}{caption}")));
        lines.last_mut().unwrap().spans[0].fg = Some(text_color);
        lines.last_mut().unwrap().spans[0].dim = true;
    }
    RenderedImage::Lines(lines)
}

fn iterm2_escape(img: &RgbaImage, cols: usize, rows: usize) -> Option<String> {
    let mut png_bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(Cursor::new(&mut png_bytes));
    image::ImageEncoder::write_image(
        encoder,
        img.as_raw(),
        img.width(),
        img.height(),
        image::ExtendedColorType::Rgba8,
    )
    .ok()?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
    let osc = format!(
        "\x1b]1337;File=size={};inline=1;width={};height={};preserveAspectRatio=1:{}\x1b\\",
        png_bytes.len(),
        cols,
        rows,
        encoded
    );
    Some(tmux_wrap(&osc))
}

/// Stucki dithering gives the best result within Sixel's 256-color palette.
fn sixel_escape(img: &RgbaImage) -> Option<String> {
    let rgb: Vec<u8> = img.pixels().flat_map(|p| [p[0], p[1], p[2]]).collect();
    icy_sixel::sixel_string(
        &rgb,
        img.width() as i32,
        img.height() as i32,
        icy_sixel::PixelFormat::RGB888,
        icy_sixel::DiffusionMethod::Stucki,
        icy_sixel::MethodForLargest::Auto,
        icy_sixel::MethodForRep::Auto,
        icy_sixel::Quality::AUTO,
    )
    .ok()
}
