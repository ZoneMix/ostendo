//! Hex color parsing, blending, and WCAG 2.0 contrast.

use crossterm::style::Color;

fn color_to_rgb(color: Color) -> Option<(u8, u8, u8)> {
    match color {
        Color::Rgb { r, g, b } => Some((r, g, b)),
        _ => None,
    }
}

/// Moves each channel `amount` (0.0-1.0) of the way to white; non-RGB colors are unchanged.
fn lighten_color(color: Color, amount: f64) -> Color {
    let Some((r, g, b)) = color_to_rgb(color) else {
        return color;
    };
    let lighten = |c: u8| (c as f64 + (255.0 - c as f64) * amount).min(255.0) as u8;
    Color::Rgb {
        r: lighten(r),
        g: lighten(g),
        b: lighten(b),
    }
}

/// WCAG 2.0 relative luminance: sRGB channels linearized, then weighted per ITU-R BT.709.
fn relative_luminance(r: u8, g: u8, b: u8) -> f64 {
    let to_linear = |c: u8| {
        let s = c as f64 / 255.0;
        if s <= 0.03928 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * to_linear(r) + 0.7152 * to_linear(g) + 0.0722 * to_linear(b)
}

/// WCAG 2.0 contrast ratio, from 1.0 (identical) to 21.0 (black on white). Non-RGB colors
/// count as mid grey.
pub fn contrast_ratio(c1: Color, c2: Color) -> f64 {
    let (r1, g1, b1) = color_to_rgb(c1).unwrap_or((128, 128, 128));
    let (r2, g2, b2) = color_to_rgb(c2).unwrap_or((128, 128, 128));
    let l1 = relative_luminance(r1, g1, b1);
    let l2 = relative_luminance(r2, g2, b2);
    let (lighter, darker) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (lighter + 0.05) / (darker + 0.05)
}

/// Lightens a badge background by 30% when it would blend into the page (contrast < 1.5).
pub fn ensure_badge_contrast(badge_bg: Color, page_bg: Color) -> Color {
    if contrast_ratio(badge_bg, page_bg) < 1.5 {
        lighten_color(badge_bg, 0.30)
    } else {
        badge_bg
    }
}

/// Linear blend from `from` (t = 0.0) to `to` (t = 1.0); `t` is clamped and non-RGB colors
/// count as black.
pub fn interpolate_color(from: Color, to: Color, t: f64) -> Color {
    let (r1, g1, b1) = color_to_rgb(from).unwrap_or((0, 0, 0));
    let (r2, g2, b2) = color_to_rgb(to).unwrap_or((0, 0, 0));
    let t = t.clamp(0.0, 1.0);
    let mix = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * t) as u8;
    Color::Rgb {
        r: mix(r1, r2),
        g: mix(g1, g2),
        b: mix(b1, b2),
    }
}

/// `#rrggbb`; non-RGB colors become `#000000`.
pub fn color_to_hex(color: Color) -> String {
    let (r, g, b) = color_to_rgb(color).unwrap_or((0, 0, 0));
    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

/// Parses `#RRGGBB` or `RRGGBB`. Input comes from decks and themes, so anything else is `None`.
pub fn hex_to_color(hex: &str) -> Option<Color> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some(Color::Rgb {
        r: channel(0)?,
        g: channel(2)?,
        b: channel(4)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_to_color_accepts_only_six_hex_digits() {
        let cases = [
            ("#00ff00", Some((0, 255, 0))),
            ("FF0000", Some((255, 0, 0))),
            ("#fff", None),
            ("#gggggg", None),
            ("#+f+f+f", None),
            // Multi-byte characters used to panic when sliced at byte offsets.
            ("#0é000", None),
            ("€000", None),
        ];
        for (hex, rgb) in cases {
            let expected = rgb.map(|(r, g, b)| Color::Rgb { r, g, b });
            assert_eq!(hex_to_color(hex), expected, "{hex}");
        }
    }

    #[test]
    fn interpolate_color_endpoints_and_midpoint() {
        let black = Color::Rgb { r: 0, g: 0, b: 0 };
        let white = Color::Rgb {
            r: 254,
            g: 254,
            b: 254,
        };
        assert_eq!(interpolate_color(black, white, 0.0), black);
        assert_eq!(interpolate_color(black, white, 1.0), white);
        assert_eq!(
            interpolate_color(black, white, 0.5),
            Color::Rgb {
                r: 127,
                g: 127,
                b: 127
            }
        );
    }
}
