use crossterm::style::Color;

pub struct AsciiCell {
    pub ch: char,
    pub fg: Color,
    pub bg: Option<Color>,
}

const ASCII_RAMP: &[u8] = b" .'`^\",:;Il!i><~+_-?][}{1)(|/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$";

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

    let x_scale = iw as f64 / width as f64;
    let row_scale = x_scale * 2.0; // chars are ~2:1 aspect
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

            let avg = block_average(img, x_start, y_start, x_end, y_end);
            match avg {
                None => {
                    line.push(AsciiCell { ch: ' ', fg: default_bg, bg: None });
                }
                Some((r, g, b)) => {
                    let lum = 0.299 * r as f64 + 0.587 * g as f64 + 0.114 * b as f64;
                    let idx = ((lum / 255.0) * (ASCII_RAMP.len() - 1) as f64) as usize;
                    let ch = ASCII_RAMP[idx.min(ASCII_RAMP.len() - 1)] as char;

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
