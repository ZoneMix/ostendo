//! Image loading and protocol detection.
//!
//! Handles PNG, JPG, GIF, BMP, WEBP, and SVG image formats.  Static images are
//! loaded into an `RgbaImage` (from the `image` crate) which downstream code can
//! then render using the appropriate terminal protocol.
//!
//! Animated GIFs receive special treatment: all frames are decoded eagerly in
//! [`load_gif_frames`] and downscaled to a maximum of 800 px on the longest side
//! to keep memory usage reasonable (a 60-frame 1080p GIF would otherwise consume
//! hundreds of megabytes of uncompressed pixel data).
//!
//! SVG files are rasterized at 2x scale (capped at 2048 px) using the `resvg`
//! library, which provides high-quality rendering without an external tool.
//!
//! # Submodules
//!
//! - [`render`] -- protocol-specific image rendering (Kitty, iTerm2, Sixel, ASCII)
//! - [`mermaid`] -- Mermaid diagram rendering via external `mmdc` CLI

pub mod render;
pub mod mermaid;

use anyhow::Result;
use image::RgbaImage;
use std::path::Path;

use crate::render::layout::WindowSize;

/// A single decoded frame from an animated GIF.
///
/// The rendering engine cycles through these frames on a timer, re-emitting the
/// image escape sequence for each frame to produce animation in the terminal.
#[derive(Clone)]
pub struct GifFrame {
    /// The RGBA pixel data for this frame (already downscaled if needed).
    pub image: RgbaImage,
    /// How long this frame should be displayed before advancing, in milliseconds.
    /// The GIF spec uses centisecond precision; a value of 0 defaults to 100 ms.
    pub delay_ms: u32,
}

/// Load a static image from disk and convert it to RGBA pixel format.
///
/// Supports all formats handled by the `image` crate (PNG, JPG, BMP, WEBP, GIF,
/// etc.) plus SVG via `resvg`.  The returned `RgbaImage` is ready for protocol
/// rendering or ASCII art conversion.
pub fn load_image(path: &Path) -> Result<RgbaImage> {
    let ext = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if ext == "svg" {
        load_svg(path)
    } else {
        let img = image::open(path)?;
        Ok(img.to_rgba8())
    }
}

/// Load all frames from a GIF file. Returns None for non-GIF or single-frame images.
/// Frames are downscaled to max 800px on the longest side to avoid excessive memory usage.
pub fn load_gif_frames(path: &Path) -> Option<Vec<GifFrame>> {
    use image::AnimationDecoder;
    use std::io::BufReader;

    let ext = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if ext != "gif" {
        return None;
    }

    let file = std::fs::File::open(path).ok()?;
    let reader = BufReader::new(file);
    let decoder = image::codecs::gif::GifDecoder::new(reader).ok()?;
    let frames: Vec<image::Frame> = decoder.into_frames().filter_map(|f| f.ok()).collect();

    if frames.len() <= 1 {
        return None; // Static GIF, use normal load path
    }

    const MAX_DIM: u32 = 800;

    let gif_frames: Vec<GifFrame> = frames.into_iter().map(|f| {
        let (numer, denom) = f.delay().numer_denom_ms();
        let delay_ms = if denom > 0 { numer / denom } else { 100 };
        // GIF spec: delay of 0 means "as fast as possible", default to 100ms
        let delay_ms = if delay_ms == 0 { 100 } else { delay_ms };
        let raw = f.into_buffer();
        // Downscale large frames to keep memory usage reasonable
        let (w, h) = raw.dimensions();
        let image = if w > MAX_DIM || h > MAX_DIM {
            let scale = MAX_DIM as f64 / w.max(h) as f64;
            let nw = (w as f64 * scale).max(1.0) as u32;
            let nh = (h as f64 * scale).max(1.0) as u32;
            image::imageops::resize(&raw, nw, nh, image::imageops::FilterType::Triangle)
        } else {
            raw
        };
        GifFrame { image, delay_ms }
    }).collect();

    Some(gif_frames)
}

/// Rasterize an SVG file to an RGBA image using the `resvg` library.
///
/// Renders at 2x the SVG's native size (capped at 2048 px on the longest side)
/// for crisp display on high-DPI terminals.  The `resvg` library uses
/// premultiplied alpha internally, so pixel values are un-premultiplied before
/// returning.
fn load_svg(path: &Path) -> Result<RgbaImage> {
    let tree = resvg::usvg::Tree::from_data(
        &std::fs::read(path)?,
        &resvg::usvg::Options::default(),
    )?;

    let size = tree.size();
    // Render at 2x for quality, capped at 2048px
    let scale = (2048.0 / size.width().max(size.height())).min(2.0);
    let width = (size.width() * scale) as u32;
    let height = (size.height() * scale) as u32;

    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| anyhow::anyhow!("Failed to create pixmap for SVG"))?;

    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let mut img = RgbaImage::new(width, height);
    for (x, y, pixel) in img.enumerate_pixels_mut() {
        let idx = (y * width + x) as usize * 4;
        let data = pixmap.data();
        // tiny-skia uses premultiplied alpha, undo it
        let a = data[idx + 3] as f32 / 255.0;
        if a > 0.0 {
            *pixel = image::Rgba([
                (data[idx] as f32 / a).min(255.0) as u8,
                (data[idx + 1] as f32 / a).min(255.0) as u8,
                (data[idx + 2] as f32 / a).min(255.0) as u8,
                data[idx + 3],
            ]);
        } else {
            *pixel = image::Rgba([0, 0, 0, 0]);
        }
    }

    Ok(img)
}

/// Scale an image using pixel-accurate dimensions for protocol rendering.
///
/// Uses the terminal's pixel-per-cell ratios (from [`WindowSize`]) to compute
/// the exact pixel size the image should be, then resizes it with Lanczos3
/// filtering for high quality.
///
/// A 5% horizontal margin is reserved so images do not touch the window edge.
///
/// # Returns
///
/// A tuple of `(scaled_image, columns, rows)` where `columns` and `rows` are
/// the number of terminal cells the image will occupy.  The rendering code uses
/// these to emit the correct escape sequence parameters and to reserve
/// placeholder lines in the virtual buffer.
pub fn scale_image_pixels(
    img: &RgbaImage,
    window: &WindowSize,
    max_cols: usize,
    max_rows: usize,
) -> (RgbaImage, usize, usize) {
    let (iw, ih) = img.dimensions();
    if iw == 0 || ih == 0 {
        return (RgbaImage::new(1, 1), 1, 1);
    }
    let aspect_ratio = ih as f64 / iw as f64;

    let ppc = window.pixels_per_column();
    let ppr = window.pixels_per_row();

    // Available space in pixels (with 5% horizontal margin)
    let col_margin = (max_cols as f64 * 0.95).floor() as usize;
    let available_width_px = col_margin as f64 * ppc;
    let available_height_px = max_rows as f64 * ppr;

    // Scale to fit available space (allows both up and down scaling)
    let mut width_px = available_width_px;
    let mut height_px = width_px * aspect_ratio;

    // If too tall, scale down to fit height
    if height_px > available_height_px {
        height_px = available_height_px;
        width_px = height_px / aspect_ratio;
    }

    let width_px = width_px.max(1.0) as u32;
    let height_px = height_px.max(1.0) as u32;

    // Convert back to terminal cells
    let cols = (width_px as f64 / ppc).ceil() as usize;
    let rows = (height_px as f64 / ppr).ceil() as usize;

    let scaled = image::imageops::resize(img, width_px, height_px, image::imageops::FilterType::Lanczos3);
    (scaled, cols, rows)
}
