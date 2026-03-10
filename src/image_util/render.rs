use base64::Engine;
use crossterm::style::Color;
use std::io::Cursor;

use crate::presentation::SlideImage;
use crate::render::layout::WindowSize;
use crate::render::text::{LineContentType, StyledLine, StyledSpan};
use crate::terminal::protocols::ImageProtocol;

/// Check if we're running inside tmux.
fn in_tmux() -> bool {
    std::env::var("TMUX").is_ok()
}

/// Wrap an escape sequence for tmux passthrough.
/// Inside tmux, escape sequences to the outer terminal need DCS wrapping.
fn tmux_wrap(escape: &str) -> String {
    if !in_tmux() {
        return escape.to_string();
    }
    // tmux passthrough: \ePtmux; followed by the escape with \e doubled, then \e\\
    let doubled = escape.replace('\x1b', "\x1b\x1b");
    format!("\x1bPtmux;{}\x1b\\", doubled)
}

/// Composite an RGBA image onto a solid background color, producing an opaque result.
/// This ensures protocol images (Kitty/iTerm2/Sixel) blend with the theme background.
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
pub enum RenderedImage {
    /// Styled lines for ascii (can be mixed into the line buffer).
    Lines(Vec<StyledLine>),
    /// Protocol escape data to write directly to stdout after frame flush.
    /// Includes placeholder height (number of blank lines to reserve in the buffer).
    Protocol {
        escape_data: String,
        placeholder_height: usize,
    },
}

/// Render a slide image using the specified protocol.
/// If `preloaded` is Some, use that image data instead of loading from disk.
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

fn render_kitty(
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

    // Build Kitty escape sequence (chunked at 4096 bytes like presenterm)
    let mut kitty_escape = String::new();

    let chunk_size = 4096;
    let chunks: Vec<&[u8]> = encoded.as_bytes().chunks(chunk_size).collect();

    for (i, chunk) in chunks.iter().enumerate() {
        let m = if i == chunks.len() - 1 { 0 } else { 1 };
        let chunk_str = std::str::from_utf8(chunk).unwrap_or("");
        if i == 0 {
            // a=T: transmit+display, f=100: PNG format, t=d: direct data
            // c/r: columns/rows to occupy, q=2: suppress responses
            kitty_escape.push_str(&format!(
                "\x1b_Ga=T,f=100,t=d,c={},r={},q=2,m={};{}\x1b\\",
                cols, rows, m, chunk_str
            ));
        } else {
            kitty_escape.push_str(&format!("\x1b_Gm={};{}\x1b\\", m, chunk_str));
        }
    }

    let wrapped = tmux_wrap(&kitty_escape);
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
