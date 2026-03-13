//! ASCII art image rendering.
//!
//! Converts images to text-based representations using Unicode block characters
//! for terminals without native image protocol support (e.g., no Kitty, iTerm2,
//! or Sixel capability). Each pixel block is mapped to an ASCII character from a
//! luminance ramp, where darker pixels get denser characters like `@` and lighter
//! pixels get sparse characters like `.`.
//!
//! This module lives in `terminal/` because it is a terminal-specific fallback
//! rendering strategy, used when protocol-based image display is unavailable.

use crossterm::style::Color;

/// A single character cell in the ASCII art output grid.
///
/// Each cell holds the character to display, a foreground color derived from
/// the source image (or an override color), and an optional background color.
/// The renderer collects these into rows (`Vec<Vec<AsciiCell>>`) that map
/// directly to terminal lines.
pub struct AsciiCell {
    /// The ASCII character representing this pixel block's brightness.
    pub ch: char,
    /// Foreground color — either sampled from the image or a user-specified override.
    pub fg: Color,
    /// Optional background color. `None` means the terminal's default background is used.
    pub bg: Option<Color>,
}

/// Brightness-to-character lookup table, ordered from lightest (space) to darkest (`$`).
/// The index into this array is computed from the luminance of each pixel block.
/// Characters with more ink coverage represent darker regions of the image.
const ASCII_RAMP: &[u8] = b" .'`^\",:;Il!i><~+_-?][}{1)(|/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$";

/// Convert an RGBA image to a grid of colored ASCII characters.
///
/// The image is scaled down to fit within `width` terminal columns. Each output
/// cell spans multiple source pixels — their colors are averaged to produce a
/// single luminance value that selects the ASCII character, and a single RGB
/// color for the foreground.
///
/// # Parameters
/// - `img`: The source RGBA image (from the `image` crate).
/// - `width`: Target width in terminal columns.
/// - `color_override`: If `Some`, all cells use this color instead of sampling from the image.
/// - `bg_color`: Background color for transparent regions (defaults to white).
///
/// # Returns
/// A 2D grid of `AsciiCell` values — one inner `Vec` per terminal row.
pub fn render_ascii_art(
    img: &image::RgbaImage,
    width: usize,
    color_override: Option<Color>,
    bg_color: Option<Color>,
) -> Vec<Vec<AsciiCell>> {
    let (iw, ih) = img.dimensions();
    if iw == 0 || ih == 0 || width == 0 {
        return Vec::new();
    }

    // Calculate how many source pixels each output column covers.
    let x_scale = iw as f64 / width as f64;
    // Terminal characters are roughly twice as tall as they are wide,
    // so each output row covers 2x the vertical pixels of one column.
    let row_scale = x_scale * 2.0;
    let height = (ih as f64 / row_scale).ceil() as usize;

    let default_bg = bg_color.unwrap_or(Color::White);
    let mut result = Vec::with_capacity(height);

    for row in 0..height {
        let mut line = Vec::with_capacity(width);
        let y_start = (row as f64 * row_scale) as u32;
        let y_end = (((row + 1) as f64 * row_scale) as u32).min(ih);

        for col in 0..width {
            let x_start = (col as f64 * x_scale) as u32;
            let x_end = (((col + 1) as f64 * x_scale) as u32).min(iw);

            // Average all pixels in this rectangular block to get one color.
            let avg = block_average(img, x_start, y_start, x_end, y_end);
            match avg {
                // Fully transparent block — render as a space.
                None => {
                    line.push(AsciiCell { ch: ' ', fg: default_bg, bg: None });
                }
                Some((r, g, b)) => {
                    // Convert to perceived luminance using BT.601 weights.
                    // This determines which ASCII character to use from the ramp.
                    let lum = 0.299 * r as f64 + 0.587 * g as f64 + 0.114 * b as f64;
                    let idx = ((lum / 255.0) * (ASCII_RAMP.len() - 1) as f64) as usize;
                    let ch = ASCII_RAMP[idx.min(ASCII_RAMP.len() - 1)] as char;

                    // Use the override color if provided; otherwise boost the
                    // sampled color's saturation and brightness so it looks
                    // vivid on a dark terminal background.
                    let color = color_override.unwrap_or_else(|| {
                        let (h, s, v) = rgb_to_hsv(r, g, b);
                        let s_boosted = (s * 1.3).min(1.0);
                        let v_boosted = v.max(0.5);
                        let (cr, cg, cb) = hsv_to_rgb(h, s_boosted, v_boosted);
                        Color::Rgb { r: cr, g: cg, b: cb }
                    });

                    line.push(AsciiCell { ch, fg: color, bg: None });
                }
            }
        }
        result.push(line);
    }
    result
}

/// Average all pixels in a rectangular block. Returns None if out of bounds or fully transparent.
fn block_average(img: &image::RgbaImage, x0: u32, y0: u32, x1: u32, y1: u32) -> Option<(u8, u8, u8)> {
    let (iw, ih) = img.dimensions();
    let x0 = x0.min(iw);
    let y0 = y0.min(ih);
    let x1 = x1.min(iw);
    let y1 = y1.min(ih);

    if x0 >= x1 || y0 >= y1 {
        return None;
    }

    let mut r_sum: u64 = 0;
    let mut g_sum: u64 = 0;
    let mut b_sum: u64 = 0;
    let mut count: u64 = 0;

    for y in y0..y1 {
        for x in x0..x1 {
            let p = img.get_pixel(x, y);
            let a = p[3] as u64;
            if a > 0 {
                r_sum += p[0] as u64 * a;
                g_sum += p[1] as u64 * a;
                b_sum += p[2] as u64 * a;
                count += a;
            }
        }
    }

    if count == 0 {
        return None;
    }

    Some(((r_sum / count) as u8, (g_sum / count) as u8, (b_sum / count) as u8))
}

/// Convert an RGB color to HSV (Hue, Saturation, Value) representation.
///
/// HSV is used here because it makes it easy to boost saturation and brightness
/// independently — something that is awkward in RGB space.
///
/// Returns `(hue_degrees, saturation_0_to_1, value_0_to_1)`.
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

/// Convert an HSV color back to RGB.
///
/// This is the inverse of [`rgb_to_hsv`]. The result is clamped to `u8` range
/// (0-255) for direct use as terminal color components.
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
