//! Text renderings of images for terminals without an image protocol.
//!
//! - [`render_blocks`]: true-color half blocks, two pixels per cell; the
//!   faithful fallback.
//! - [`render_ascii_art`]: a character-ramp look (brighter blocks get denser
//!   glyphs) chosen explicitly with `image_render: ascii`; loop animations
//!   like `spin(image)` operate on its glyphs.

use crossterm::style::Color;

pub struct AsciiCell {
    pub ch: char,
    pub fg: Color,
}

/// Ordered from least to most ink.
const ASCII_RAMP: &[u8] = b" .'`^\",:;Il!i><~+_-?][}{1)(|/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$";

/// Convert `img` to rows of exactly `width` cells (narrower if needed to stay
/// within `max_rows`). Cells are twice as tall as wide, so each row covers
/// twice the pixel height of a column's width. When `color_override` is set
/// every visible cell uses it instead of the sampled color.
pub fn render_ascii_art(
    img: &image::RgbaImage,
    width: usize,
    max_rows: usize,
    color_override: Option<Color>,
) -> Vec<Vec<AsciiCell>> {
    let (iw, ih) = img.dimensions();
    if iw == 0 || ih == 0 || width == 0 || max_rows == 0 {
        return Vec::new();
    }
    // rows ≈ width * ih / (2 * iw); shrink the width until the rows fit.
    let fit = (max_rows as f64 * 2.0 * iw as f64 / ih as f64).floor() as usize;
    let width = width.min(fit.max(1));

    let x_scale = iw as f64 / width as f64;
    let row_scale = x_scale * 2.0;
    let height = (ih as f64 / row_scale).ceil() as usize;

    // Every cell spans at least one source pixel; when upscaling, adjacent
    // cells repeat a pixel rather than covering an empty range.
    let span = |index: usize, scale: f64, limit: u32| {
        let start = ((index as f64 * scale) as u32).min(limit - 1);
        let end = (((index + 1) as f64 * scale) as u32).clamp(start + 1, limit);
        (start, end)
    };

    (0..height)
        .map(|row| {
            let (y0, y1) = span(row, row_scale, ih);
            (0..width)
                .map(|col| {
                    let (x0, x1) = span(col, x_scale, iw);
                    match block_average(img, x0..x1, y0..y1) {
                        None => AsciiCell {
                            ch: ' ',
                            fg: Color::Reset,
                        },
                        Some((r, g, b)) => AsciiCell {
                            ch: ramp_char(r, g, b),
                            fg: color_override.unwrap_or_else(|| vivid(r, g, b)),
                        },
                    }
                })
                .collect()
        })
        .collect()
}

/// Upper/lower pixel colors for each cell of a half-block rendering at most
/// `max_cols` × `max_rows` cells, transparency blended onto `bg`. Draw each
/// cell as `▀` with the upper color as foreground and the lower as background.
pub fn render_blocks(
    img: &image::RgbaImage,
    max_cols: usize,
    max_rows: usize,
    bg: (u8, u8, u8),
) -> Vec<Vec<(Color, Color)>> {
    let (iw, ih) = img.dimensions();
    if iw == 0 || ih == 0 || max_cols == 0 || max_rows == 0 {
        return Vec::new();
    }
    // A half cell is roughly square, so sample one pixel per column and two per row.
    let scale = (max_cols as f64 / iw as f64).min(max_rows as f64 * 2.0 / ih as f64);
    let w = ((iw as f64 * scale).round() as u32).clamp(1, max_cols as u32);
    let h = ((ih as f64 * scale).round() as u32).clamp(1, max_rows as u32 * 2);
    let small = image::imageops::resize(img, w, h + h % 2, image::imageops::FilterType::Triangle);
    let blend = |x: u32, y: u32| {
        let p = small.get_pixel(x, y);
        let a = u16::from(p[3]);
        let mix = |c: u8, b: u8| ((u16::from(c) * a + u16::from(b) * (255 - a)) / 255) as u8;
        Color::Rgb {
            r: mix(p[0], bg.0),
            g: mix(p[1], bg.1),
            b: mix(p[2], bg.2),
        }
    };
    (0..small.height() / 2)
        .map(|row| {
            (0..w)
                .map(|x| (blend(x, row * 2), blend(x, row * 2 + 1)))
                .collect()
        })
        .collect()
}

/// BT.601 luma picks the glyph.
fn ramp_char(r: u8, g: u8, b: u8) -> char {
    let lum = 0.299 * r as f64 + 0.587 * g as f64 + 0.114 * b as f64;
    let idx = ((lum / 255.0) * (ASCII_RAMP.len() - 1) as f64) as usize;
    ASCII_RAMP[idx.min(ASCII_RAMP.len() - 1)] as char
}

/// Boost saturation and lift dark values so sampled colors stay legible on a
/// dark terminal background.
fn vivid(r: u8, g: u8, b: u8) -> Color {
    let (h, s, v) = rgb_to_hsv(r, g, b);
    let (r, g, b) = hsv_to_rgb(h, (s * 1.3).min(1.0), v.max(0.5));
    Color::Rgb { r, g, b }
}

/// Alpha-weighted mean color of the block, or `None` if it is fully
/// transparent.
fn block_average(
    img: &image::RgbaImage,
    xs: std::ops::Range<u32>,
    ys: std::ops::Range<u32>,
) -> Option<(u8, u8, u8)> {
    let (mut r_sum, mut g_sum, mut b_sum, mut count) = (0u64, 0u64, 0u64, 0u64);
    for y in ys {
        for x in xs.clone() {
            let p = img.get_pixel(x, y);
            let a = p[3] as u64;
            r_sum += p[0] as u64 * a;
            g_sum += p[1] as u64 * a;
            b_sum += p[2] as u64 * a;
            count += a;
        }
    }
    if count == 0 {
        return None;
    }
    Some((
        (r_sum / count) as u8,
        (g_sum / count) as u8,
        (b_sum / count) as u8,
    ))
}

/// Returns `(hue in degrees, saturation 0..=1, value 0..=1)`.
fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let r = r as f64 / 255.0;
    let g = g as f64 / 255.0;
    let b = b as f64 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let s = if max == 0.0 { 0.0 } else { d / max };
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / d) % 6.0)
    } else if max == g {
        60.0 * (((b - r) / d) + 2.0)
    } else {
        60.0 * (((r - g) / d) + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    (h, s, max)
}

/// Inverse of [`rgb_to_hsv`].
fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn solid_image(w: u32, h: u32, r: u8, g: u8, b: u8) -> RgbaImage {
        RgbaImage::from_pixel(w, h, Rgba([r, g, b, 255]))
    }

    #[test]
    fn zero_width_returns_empty() {
        let img = solid_image(10, 10, 200, 100, 50);
        assert!(render_ascii_art(&img, 0, 10, None).is_empty());
    }

    #[test]
    fn zero_dimension_image_returns_empty() {
        let img = RgbaImage::new(0, 0);
        assert!(render_ascii_art(&img, 20, 10, None).is_empty());
    }

    #[test]
    fn output_width_matches_requested_width() {
        let img = solid_image(100, 100, 128, 128, 128);
        let result = render_ascii_art(&img, 40, 100, None);
        assert!(!result.is_empty());
        for row in &result {
            assert_eq!(row.len(), 40, "every row must be exactly 40 cells wide");
        }
    }

    #[test]
    fn every_cell_of_an_opaque_image_is_drawn() {
        let img = solid_image(5, 5, 200, 50, 50);
        for width in [3, 20] {
            let rows = render_ascii_art(&img, width, 100, None);
            assert!(!rows.is_empty());
            assert!(
                rows.iter().flatten().all(|cell| cell.ch != ' '),
                "blank cell at width {width}"
            );
        }
    }

    #[test]
    fn fully_transparent_image_renders_spaces() {
        let img = RgbaImage::new(20, 20);
        let result = render_ascii_art(&img, 10, 100, None);
        assert!(result.iter().flatten().all(|cell| cell.ch == ' '));
    }

    #[test]
    fn color_override_is_applied_to_all_opaque_cells() {
        let img = solid_image(20, 20, 200, 100, 50);
        let override_color = Color::Rgb { r: 255, g: 0, b: 0 };
        let result = render_ascii_art(&img, 10, 100, Some(override_color));
        for cell in result.iter().flatten() {
            assert_eq!(cell.fg, override_color);
        }
    }

    #[test]
    fn dark_image_uses_sparse_ascii_characters() {
        let img = solid_image(20, 20, 10, 10, 10);
        let result = render_ascii_art(&img, 10, 100, None);
        let sparse = [' ', '.', '\'', '`', '^'];
        assert!(result.iter().flatten().all(|c| sparse.contains(&c.ch)));
    }

    #[test]
    fn bright_image_uses_dense_ascii_characters() {
        let img = solid_image(20, 20, 250, 250, 250);
        let result = render_ascii_art(&img, 10, 100, None);
        let dense = ['@', '$', '#', 'B', 'M', 'W', '%', '8', '&'];
        assert!(result.iter().flatten().all(|c| dense.contains(&c.ch)));
    }

    #[test]
    fn blocks_fit_the_box_and_blend_transparency_onto_the_page() {
        let mut img = solid_image(40, 20, 255, 0, 0);
        img.put_pixel(0, 0, Rgba([0, 0, 0, 0]));
        let rows = render_blocks(&img, 10, 3, (0, 0, 255));
        assert!(rows.len() <= 3 && !rows.is_empty());
        assert!(rows.iter().all(|r| r.len() <= 10));
        let (top, bottom) = rows[1][5];
        assert_eq!(top, Color::Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(bottom, top);
        assert!(
            matches!(rows[0][0].0, Color::Rgb { b, .. } if b > 0),
            "transparent corner not blended"
        );
    }

    #[test]
    fn ramp_art_shrinks_to_the_row_limit() {
        let img = solid_image(10, 100, 200, 200, 200);
        let rows = render_ascii_art(&img, 40, 5, None);
        assert!(rows.len() <= 5, "{} rows", rows.len());
    }
}
