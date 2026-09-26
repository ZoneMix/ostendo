//! Hex parsing, blending, and WCAG 2.0 contrast for theme colors.

use crossterm::style::Color;

/// Extract the red, green, and blue components from a crossterm [`Color`].
///
/// Returns `Some((r, g, b))` for `Color::Rgb` variants, or `None` for
/// named/indexed colors that don't carry explicit RGB values.
///
/// # Parameters
/// - `color` — the crossterm color to decompose.
pub fn color_to_rgb(color: Color) -> Option<(u8, u8, u8)> {
    match color {
        Color::Rgb { r, g, b } => Some((r, g, b)),
        _ => None,
    }
}

/// Compute the WCAG 2.0 relative luminance for an sRGB color.
///
/// Relative luminance is a measure of perceived brightness on a `0.0` (black)
/// to `1.0` (white) scale. It accounts for the human eye's different
/// sensitivity to red, green, and blue light.
///
/// The calculation first linearizes each sRGB channel (reversing the gamma
/// curve), then applies the ITU-R BT.709 luminance coefficients.
///
/// # Parameters
/// - `r`, `g`, `b` — sRGB channel values (0-255).
///
/// # Returns
/// A `f64` in the range `[0.0, 1.0]`.
pub fn relative_luminance(r: u8, g: u8, b: u8) -> f64 {
    // Convert 8-bit sRGB to linear RGB. The threshold 0.03928 and exponent 2.4
    // come from the sRGB specification (IEC 61966-2-1).
    let to_linear = |c: u8| -> f64 {
        let s = c as f64 / 255.0;
        if s <= 0.03928 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    // Weighted sum per ITU-R BT.709: green contributes most to perceived brightness.
    0.2126 * to_linear(r) + 0.7152 * to_linear(g) + 0.0722 * to_linear(b)
}

/// Compute the WCAG 2.0 contrast ratio between two colors.
///
/// The result ranges from `1.0` (identical colors) to `21.0` (black vs. white).
/// WCAG 2.0 requires:
/// - Normal text: >= 4.5:1
/// - Large text: >= 3.0:1
///
/// Non-RGB colors fall back to `(128, 128, 128)` (medium gray).
///
/// # Parameters
/// - `c1`, `c2` — the two crossterm colors to compare.
///
/// # Returns
/// The contrast ratio as a `f64` in the range `[1.0, 21.0]`.
pub fn contrast_ratio(c1: Color, c2: Color) -> f64 {
    let (r1, g1, b1) = color_to_rgb(c1).unwrap_or((128, 128, 128));
    let (r2, g2, b2) = color_to_rgb(c2).unwrap_or((128, 128, 128));
    let l1 = relative_luminance(r1, g1, b1);
    let l2 = relative_luminance(r2, g2, b2);
    // The lighter luminance goes in the numerator per the WCAG formula.
    let (lighter, darker) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (lighter + 0.05) / (darker + 0.05)
}

/// Linearly interpolate between two RGB colors.
///
/// Produces a smooth blend useful for rendering background gradients row by
/// row. The parameter `t` controls the mix:
/// - `t = 0.0` returns `from` exactly.
/// - `t = 1.0` returns `to` exactly.
/// - `t = 0.5` returns the midpoint.
///
/// Values outside `[0.0, 1.0]` are clamped. Non-RGB colors fall back to black.
///
/// # Parameters
/// - `from` — the starting color.
/// - `to` — the ending color.
/// - `t` — interpolation factor (`0.0` to `1.0`).
pub fn interpolate_color(from: Color, to: Color, t: f64) -> Color {
    let (r1, g1, b1) = color_to_rgb(from).unwrap_or((0, 0, 0));
    let (r2, g2, b2) = color_to_rgb(to).unwrap_or((0, 0, 0));
    let t = t.clamp(0.0, 1.0);
    Color::Rgb {
        r: (r1 as f64 + (r2 as f64 - r1 as f64) * t) as u8,
        g: (g1 as f64 + (g2 as f64 - g1 as f64) * t) as u8,
        b: (b1 as f64 + (b2 as f64 - b1 as f64) * t) as u8,
    }
}

/// Convert a crossterm [`Color`] to a CSS hex string (e.g., `"#ff5733"`).
///
/// Non-RGB colors are returned as `"#000000"` (black).
///
/// # Parameters
/// - `color` — the crossterm color to convert.
pub fn color_to_hex(color: Color) -> String {
    if let Some((r, g, b)) = color_to_rgb(color) {
        format!("#{:02x}{:02x}{:02x}", r, g, b)
    } else {
        "#000000".to_string()
    }
}

/// Parse a CSS hex color string into a crossterm [`Color`].
///
/// Accepts both `"#RRGGBB"` and `"RRGGBB"` formats (the leading `#` is
/// optional). Returns `None` if the string is not exactly 6 hex digits
/// (after stripping the `#`).
///
/// # Parameters
/// - `hex` — the hex color string to parse.
///
/// # Examples
/// ```
/// use crossterm::style::Color;
/// // Both formats work:
/// assert_eq!(hex_to_color("#ff0000"), Some(Color::Rgb { r: 255, g: 0, b: 0 }));
/// assert_eq!(hex_to_color("00ff00"), Some(Color::Rgb { r: 0, g: 255, b: 0 }));
/// // Invalid input returns None:
/// assert_eq!(hex_to_color("#fff"), None);
/// ```
pub fn hex_to_color(hex: &str) -> Option<Color> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    // Parse each pair of hex digits into a u8 channel value.
    // `u8::from_str_radix` converts a string in the given base (16 = hex) to a number.
    // The `?` operator returns None early if any pair contains invalid hex characters.
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color::Rgb { r, g, b })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_to_color_valid() {
        let c = hex_to_color("#00ff00").unwrap();
        assert_eq!(c, Color::Rgb { r: 0, g: 255, b: 0 });
    }

    #[test]
    fn test_hex_to_color_no_hash() {
        let c = hex_to_color("ff0000").unwrap();
        assert_eq!(c, Color::Rgb { r: 255, g: 0, b: 0 });
    }

    #[test]
    fn test_hex_to_color_invalid_short() {
        assert!(hex_to_color("#fff").is_none());
    }

    #[test]
    fn test_hex_to_color_invalid_chars() {
        assert!(hex_to_color("#gggggg").is_none());
    }

    #[test]
    fn test_interpolate_color_endpoints() {
        let black = Color::Rgb { r: 0, g: 0, b: 0 };
        let white = Color::Rgb {
            r: 255,
            g: 255,
            b: 255,
        };
        assert_eq!(interpolate_color(black, white, 0.0), black);
        assert_eq!(interpolate_color(black, white, 1.0), white);
    }

    #[test]
    fn test_interpolate_color_midpoint() {
        let black = Color::Rgb { r: 0, g: 0, b: 0 };
        let white = Color::Rgb {
            r: 254,
            g: 254,
            b: 254,
        };
        let mid = interpolate_color(black, white, 0.5);
        assert_eq!(
            mid,
            Color::Rgb {
                r: 127,
                g: 127,
                b: 127
            }
        );
    }
}
