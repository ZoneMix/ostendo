//! Protocol-specific image rendering for terminal display.
//!
//! Supports four image protocols, each with different encoding and escape
//! sequence requirements:
//!
//! - **Kitty Graphics Protocol** (native) -- PNG data sent via chunked APC
//!   escape sequences.  Best quality and performance; supports Kitty and Ghostty.
//! - **iTerm2 Inline Images** -- base64-encoded PNG wrapped in an OSC 1337
//!   escape.  Supported by iTerm2, WezTerm, and many modern terminals.
//! - **Sixel** (VT340 legacy) -- a bitmap encoding from the 1980s DEC terminals,
//!   still supported by xterm, mlterm, and others.  Uses the `icy_sixel` crate.
//! - **ASCII Art Fallback** -- converts the image to colored Unicode half-block
//!   characters.  Works in any terminal but at much lower resolution.
//!
//! # tmux passthrough
//!
//! When running inside tmux, escape sequences destined for the outer terminal
//! must be wrapped in DCS passthrough (`\ePtmux;...\e\\`).  The [`tmux_wrap`]
//! helper handles this transparently.
//!
//! # Alpha compositing
//!
//! Images with transparency are composited onto the current theme's background
//! color before encoding, because most terminal image protocols do not support
//! alpha channels natively.

use base64::Engine;
use crossterm::style::Color;
use std::io::Cursor;

use crate::presentation::SlideImage;
use crate::render::layout::WindowSize;
use crate::render::text::{LineContentType, StyledLine, StyledSpan};
use crate::terminal::protocols::ImageProtocol;

/// Check if we are running inside tmux by looking for the `TMUX` env var.
fn in_tmux() -> bool {
    std::env::var("TMUX").is_ok()
}

/// Wrap an escape sequence for tmux DCS passthrough.
///
/// Inside tmux, escape sequences intended for the *outer* terminal (e.g.
/// Kitty graphics or iTerm2 inline images) must be wrapped in a DCS
/// passthrough envelope.  All embedded `\x1b` bytes are doubled so tmux
/// does not interpret them.
///
/// Outside tmux this is a no-op -- the escape is returned unchanged.
fn tmux_wrap(escape: &str) -> String {
    if !in_tmux() {
        return escape.to_string();
    }
    // tmux passthrough: \ePtmux; followed by the escape with \e doubled, then \e\\
    let doubled = escape.replace('\x1b', "\x1b\x1b");
    format!("\x1bPtmux;{}\x1b\\", doubled)
}

/// Composite an RGBA image onto a solid background color, producing a fully
/// opaque result (alpha = 255 for every pixel).
///
/// This is necessary because Kitty, iTerm2, and Sixel protocols render
/// transparent pixels as black (or undefined) rather than blending with the
/// terminal background.  By pre-compositing, images seamlessly match the
/// current theme's background color.
/// Public wrapper for alpha compositing (used by Kitty GIF animation upload).
#[allow(dead_code)]
pub fn composite_on_bg_pub(img: &image::RgbaImage, bg_color: Color) -> image::RgbaImage {
    composite_on_bg(img, bg_color)
}

fn composite_on_bg(img: &image::RgbaImage, bg_color: Color) -> image::RgbaImage {
    let (bg_r, bg_g, bg_b) = match bg_color {
        Color::Rgb { r, g, b } => (r, g, b),
        _ => (0, 0, 0),
    };
    let mut out = image::RgbaImage::new(img.width(), img.height());
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

/// Result of rendering a slide image.
///
/// The two variants reflect a fundamental difference in how image data reaches
/// the terminal:
///
/// - **Lines** -- ASCII art rows that are mixed into the normal virtual buffer
///   and written character-by-character like any other styled text.
/// - **Protocol** -- a raw escape sequence blob that is written to stdout
///   *after* the text frame has been flushed, because protocol images bypass
///   the character grid entirely.
pub enum RenderedImage {
    /// Styled lines for ASCII art rendering (mixed into the virtual line buffer).
    Lines(Vec<StyledLine>),
    /// Raw escape sequence data to write directly to stdout after the text frame.
    /// `placeholder_height` is the number of blank lines to reserve in the virtual
    /// buffer so subsequent content is positioned below the image.
    Protocol {
        escape_data: String,
        placeholder_height: usize,
    },
    /// Kitty v2: image data transmitted separately via `a=t,i=<id>`.
    /// At render time, only a ~50-byte `a=p` placement command is needed.
    /// The `transmit_escape` must be written to stdout once (at prerender time)
    /// before any placement commands reference this ID.
    KittyPlacement {
        /// The Kitty image ID assigned during transmission.
        image_id: u32,
        /// Display width in terminal columns.
        cols: usize,
        /// Display height in terminal rows.
        rows: usize,
        /// The full `a=t` transmit escape sequence (written once at prerender).
        transmit_escape: String,
    },
}

/// Render a slide image using the specified protocol.
/// If `preloaded` is Some, use that image data instead of loading from disk.
#[allow(clippy::too_many_arguments)]
pub fn render_slide_image(
    image: &SlideImage,
    content_width: usize,
    max_height: usize,
    pad: &str,
    accent_color: Color,
    text_color: Color,
    protocol: ImageProtocol,
    bg_color: Color,
    window_size: &WindowSize,
    preloaded: Option<&image::RgbaImage>,
) -> RenderedImage {
    let img = if let Some(pre) = preloaded {
        pre.clone()
    } else {
        let path = &image.path;
        if !path.exists() {
            let mut line = StyledLine::empty();
            line.push(StyledSpan::new(pad));
            line.push(
                StyledSpan::new(&format!("[Image not found: {}]", path.display()))
                    .with_fg(accent_color),
            );
            return RenderedImage::Lines(vec![line]);
        }
        match crate::image_util::load_image(path) {
            Ok(img) => img,
            Err(_) => {
                let mut line = StyledLine::empty();
                line.push(StyledSpan::new(pad));
                line.push(
                    StyledSpan::new(&format!("[Failed to load image: {}]", path.display()))
                        .with_fg(accent_color),
                );
                return RenderedImage::Lines(vec![line]);
            }
        }
    };

    let display_width = content_width.saturating_sub(pad.len());

    match protocol {
        ImageProtocol::Kitty => {
            let composited = composite_on_bg(&img, bg_color);
            render_kitty(&composited, display_width, max_height, pad.len(), window_size)
        }
        ImageProtocol::Iterm2 => {
            let composited = composite_on_bg(&img, bg_color);
            render_iterm2(&composited, display_width, max_height, pad.len(), window_size)
        }
        ImageProtocol::Sixel => {
            let composited = composite_on_bg(&img, bg_color);
            render_sixel(&composited, display_width, max_height, pad.len(), window_size)
        }
        ImageProtocol::Ascii => {
            render_ascii(&img, display_width, pad, text_color, image, bg_color)
        }
    }
}

/// Render an image as colored ASCII art using Unicode half-block characters.
///
/// This is the universal fallback that works in every terminal.  Each output
/// row represents two pixel rows (upper half-block + background color), so
/// the effective vertical resolution is doubled compared to simple character art.
fn render_ascii(
    img: &image::RgbaImage,
    display_width: usize,
    pad: &str,
    text_color: Color,
    image: &SlideImage,
    bg_color: Color,
) -> RenderedImage {
    use crate::terminal::ascii_art;

    let color_override = if image.color_override.is_empty() {
        None
    } else {
        crate::theme::colors::hex_to_color(&image.color_override)
    };
    let ascii_rows = ascii_art::render_ascii_art(img, display_width, color_override, Some(bg_color));
    let mut lines = Vec::with_capacity(ascii_rows.len() + 1);
    for row in &ascii_rows {
        let mut line = StyledLine::empty();
        line.push(StyledSpan::new(pad));
        for cell in row {
            let mut span = StyledSpan::new(&cell.ch.to_string())
                .with_fg(cell.fg);
            if let Some(bg) = cell.bg {
                span = span.with_bg(bg);
            }
            line.push(span);
        }
        line.content_type = LineContentType::AsciiImage;
        lines.push(line);
    }

    if !image.alt_text.is_empty() {
        let mut cap = StyledLine::empty();
        cap.push(StyledSpan::new(pad));
        cap.push(StyledSpan::new(&format!("  {}", image.alt_text)).with_fg(text_color).dim());
        lines.push(cap);
    }

    RenderedImage::Lines(lines)
}

/// Render an image using the Kitty Graphics Protocol.
///
/// The image is PNG-encoded, base64-encoded, and split into 4096-byte chunks
/// (following the protocol spec).  Each chunk is sent as an APC escape sequence.
/// The first chunk includes metadata (format, columns, rows, display mode).
fn render_kitty(
    img: &image::RgbaImage,
    display_width: usize,
    max_height: usize,
    _pad_cols: usize,
    window_size: &WindowSize,
) -> RenderedImage {
    use crate::image_util::kitty;

    let (scaled, cols, rows) = crate::image_util::scale_image_pixels(
        img, window_size, display_width, max_height,
    );

    // Kitty v2: transmit once (a=t), display by ID (a=p).
    // Centering is handled by cursor positioning at emit time, not by pad_cols.
    let image_id = kitty::next_image_id();
    match kitty::transmit_escape(image_id, &scaled) {
        Some(transmit) => RenderedImage::KittyPlacement {
            image_id,
            cols,
            rows,
            transmit_escape: transmit,
        },
        None => RenderedImage::Lines(vec![]),
    }
}

/// Render an image using the iTerm2 inline image protocol (OSC 1337).
///
/// The image is PNG-encoded and sent as a single base64-encoded blob inside
/// an OSC 1337 escape sequence.  The terminal is told the desired column/row
/// dimensions and handles scaling internally.
fn render_iterm2(
    img: &image::RgbaImage,
    display_width: usize,
    max_height: usize,
    pad_cols: usize,
    window_size: &WindowSize,
) -> RenderedImage {
    let (scaled, cols, rows) = crate::image_util::scale_image_pixels(
        img, window_size, display_width, max_height,
    );
    let (sw, sh) = scaled.dimensions();

    // Encode as PNG
    let mut png_bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(Cursor::new(&mut png_bytes));
    if image::ImageEncoder::write_image(
        encoder,
        scaled.as_raw(),
        sw,
        sh,
        image::ExtendedColorType::Rgba8,
    ).is_err() {
        return RenderedImage::Lines(vec![]);
    }

    let encoded = base64::engine::general_purpose::STANDARD.encode(&png_bytes);

    let osc = format!(
        "\x1b]1337;File=size={};inline=1;width={};height={};preserveAspectRatio=1:{}\x1b\\",
        png_bytes.len(), cols, rows, encoded
    );
    let wrapped = tmux_wrap(&osc);

    let mut escape = String::new();
    if pad_cols > 0 {
        escape.push_str(&format!("\x1b[{}C", pad_cols));
    }
    escape.push_str(&wrapped);

    RenderedImage::Protocol {
        escape_data: escape,
        placeholder_height: rows,
    }
}

/// Render an image using the Sixel graphics format (DEC VT340 protocol).
///
/// The image is converted to RGB bytes and encoded using the `icy_sixel` crate
/// with Stucki dithering for the best visual quality within Sixel's 256-color
/// palette limitation.
fn render_sixel(
    img: &image::RgbaImage,
    display_width: usize,
    max_height: usize,
    pad_cols: usize,
    window_size: &WindowSize,
) -> RenderedImage {
    let (scaled, _cols, rows) = crate::image_util::scale_image_pixels(
        img, window_size, display_width, max_height,
    );
    let (sw, sh) = scaled.dimensions();

    let rgb: Vec<u8> = scaled.pixels().flat_map(|p| [p[0], p[1], p[2]]).collect();

    let output = match icy_sixel::sixel_string(
        &rgb,
        sw as i32,
        sh as i32,
        icy_sixel::PixelFormat::RGB888,
        icy_sixel::DiffusionMethod::Stucki,
        icy_sixel::MethodForLargest::Auto,
        icy_sixel::MethodForRep::Auto,
        icy_sixel::Quality::AUTO,
    ) {
        Ok(s) => s,
        Err(_) => return RenderedImage::Lines(vec![]),
    };

    let mut escape = String::new();
    if pad_cols > 0 {
        escape.push_str(&format!("\x1b[{}C", pad_cols));
    }
    escape.push_str(&output);

    RenderedImage::Protocol {
        escape_data: escape,
        placeholder_height: rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::{ImagePosition, ImageRenderMode, SlideImage};
    use std::path::PathBuf;

    #[test]
    fn test_missing_image_returns_placeholder() {
        let img = SlideImage {
            path: PathBuf::from("/nonexistent/image.png"),
            alt_text: String::new(),
            position: ImagePosition::Below,
            render_mode: ImageRenderMode::Auto,
            scale: 100,
            color_override: String::new(),
        };
        let ws = WindowSize { columns: 80, rows: 24, pixel_width: 640, pixel_height: 384 };
        let result = render_slide_image(
            &img, 80, 20, "", Color::Green, Color::White, ImageProtocol::Ascii, Color::Black, &ws, None,
        );
        match result {
            RenderedImage::Lines(lines) => {
                assert!(!lines.is_empty());
                let text: String = lines[0].spans.iter().map(|s| s.text.as_str()).collect();
                assert!(text.contains("Image not found"));
            }
            _ => panic!("Expected Lines variant"),
        }
    }

    #[test]
    fn test_protocol_detection_returns_valid() {
        // In test environment, no KITTY_WINDOW_ID or iTerm, so should default to Iterm2
        let protocol = crate::terminal::protocols::detect_protocol();
        assert!(
            protocol == ImageProtocol::Kitty
                || protocol == ImageProtocol::Iterm2
                || protocol == ImageProtocol::Sixel
                || protocol == ImageProtocol::Ascii,
            "Expected a valid protocol"
        );
    }
}
