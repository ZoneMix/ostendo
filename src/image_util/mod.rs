//! Image loading and sizing.
//!
//! Anything the `image` crate decodes is loaded as an `RgbaImage`; SVG is
//! rasterized with `resvg` at 2x (capped at 2048 px) for high-DPI terminals.
//! Animated GIF frames are downscaled to at most 800 px as they are decoded,
//! so a long HD GIF never holds its full-size frames in memory at once.

pub mod kitty;
pub mod mermaid;
pub mod render;

use anyhow::Result;
use fast_image_resize as fir;
use image::RgbaImage;
use std::path::Path;

use crate::render::layout::WindowSize;

/// Resize with SIMD-accelerated `fast_image_resize`, which is far faster than
/// `image::imageops::resize`. Returns the source unchanged if resizing fails.
fn fast_resize(
    src: &RgbaImage,
    dst_width: u32,
    dst_height: u32,
    filter: fir::FilterType,
) -> RgbaImage {
    if dst_width == 0 || dst_height == 0 {
        return RgbaImage::new(dst_width.max(1), dst_height.max(1));
    }
    if src.dimensions() == (dst_width, dst_height) {
        return src.clone();
    }
    let resized = || {
        let (sw, sh) = src.dimensions();
        let view = fir::images::ImageRef::new(sw, sh, src.as_raw(), fir::PixelType::U8x4).ok()?;
        let mut dst = fir::images::Image::new(dst_width, dst_height, fir::PixelType::U8x4);
        let options = fir::ResizeOptions::new().resize_alg(fir::ResizeAlg::Convolution(filter));
        fir::Resizer::new().resize(&view, &mut dst, &options).ok()?;
        RgbaImage::from_raw(dst_width, dst_height, dst.into_vec())
    };
    resized().unwrap_or_else(|| src.clone())
}

/// One frame of an animated GIF, already downscaled.
#[derive(Clone)]
pub struct GifFrame {
    pub image: RgbaImage,
    /// Never zero: zero-delay frames are stored as 100 ms.
    pub delay_ms: u32,
}

/// Load any format the `image` crate decodes, or an SVG, as RGBA.
pub fn load_image(path: &Path) -> Result<RgbaImage> {
    let ext = path
        .extension()
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

/// Decode every frame of an animated GIF. Returns `None` for non-GIF paths
/// and single-frame GIFs, which load through [`load_image`].
pub fn load_gif_frames(path: &Path) -> Option<Vec<GifFrame>> {
    use image::AnimationDecoder;
    use std::io::BufReader;

    const MAX_DIM: u32 = 800;

    let is_gif = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gif"));
    if !is_gif {
        return None;
    }

    let file = std::fs::File::open(path).ok()?;
    let decoder = image::codecs::gif::GifDecoder::new(BufReader::new(file)).ok()?;
    let frames: Vec<GifFrame> = decoder
        .into_frames()
        .filter_map(Result::ok)
        .map(|frame| {
            let (numer, denom) = frame.delay().numer_denom_ms();
            // Browsers play zero-delay frames at 100 ms; match them.
            let delay_ms = match numer.checked_div(denom) {
                Some(0) | None => 100,
                Some(ms) => ms,
            };
            let raw = frame.into_buffer();
            let (w, h) = raw.dimensions();
            let image = if w > MAX_DIM || h > MAX_DIM {
                let scale = MAX_DIM as f64 / w.max(h) as f64;
                let nw = (w as f64 * scale).max(1.0) as u32;
                let nh = (h as f64 * scale).max(1.0) as u32;
                fast_resize(&raw, nw, nh, fir::FilterType::Bilinear)
            } else {
                raw
            };
            GifFrame { image, delay_ms }
        })
        .collect();

    (frames.len() > 1).then_some(frames)
}

/// Rasterize an SVG file to an RGBA image using the `resvg` library.
///
/// Renders at 2x the SVG's native size (capped at 2048 px on the longest side)
/// for crisp display on high-DPI terminals.  The `resvg` library uses
/// premultiplied alpha internally, so pixel values are un-premultiplied before
/// returning.
fn load_svg(path: &Path) -> Result<RgbaImage> {
    let tree =
        resvg::usvg::Tree::from_data(&std::fs::read(path)?, &resvg::usvg::Options::default())?;

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

/// Resize `img` to the largest pixel size that fits `max_cols` x `max_rows`
/// cells (less a 5% horizontal margin) at the terminal's cell size, keeping
/// its aspect ratio. Returns the image and the `(cols, rows)` it occupies.
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

    if height_px > available_height_px {
        height_px = available_height_px;
        width_px = height_px / aspect_ratio;
    }

    let width_px = width_px.max(1.0) as u32;
    let height_px = height_px.max(1.0) as u32;

    let cols = (width_px as f64 / ppc).ceil() as usize;
    let rows = (height_px as f64 / ppr).ceil() as usize;

    let scaled = fast_resize(img, width_px, height_px, fir::FilterType::Lanczos3);
    (scaled, cols, rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::gif::GifEncoder;
    use image::{Delay, Frame};

    fn write_gif(path: &Path, frames: usize, width: u32) {
        let mut encoder = GifEncoder::new(std::fs::File::create(path).unwrap());
        for _ in 0..frames {
            let frame = Frame::from_parts(
                RgbaImage::from_pixel(width, 10, image::Rgba([255, 0, 0, 255])),
                0,
                0,
                Delay::from_numer_denom_ms(0, 1),
            );
            encoder.encode_frame(frame).unwrap();
        }
    }

    #[test]
    fn gif_frames_are_downscaled_and_single_frames_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let animated = dir.path().join("animated.gif");
        write_gif(&animated, 2, 1600);
        let frames = load_gif_frames(&animated).unwrap();
        assert_eq!(frames.len(), 2);
        for frame in &frames {
            assert_eq!(frame.image.dimensions(), (800, 5));
            assert_eq!(frame.delay_ms, 100);
        }

        let still = dir.path().join("still.gif");
        write_gif(&still, 1, 20);
        assert!(load_gif_frames(&still).is_none());
    }
}
