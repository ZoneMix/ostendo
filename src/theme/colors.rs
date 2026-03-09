use crossterm::style::Color;

/// Extract RGB components from a crossterm Color. Returns None for non-RGB colors.
pub fn color_to_rgb(color: Color) -> Option<(u8, u8, u8)> {
    match color {
        Color::Rgb { r, g, b } => Some((r, g, b)),
        _ => None,
    }
}

/// Lighten an RGB color by a percentage (0.0 to 1.0).
pub fn lighten_color(color: Color, amount: f64) -> Color {
    if let Some((r, g, b)) = color_to_rgb(color) {
        let r2 = (r as f64 + (255.0 - r as f64) * amount).min(255.0) as u8;
        let g2 = (g as f64 + (255.0 - g as f64) * amount).min(255.0) as u8;
        let b2 = (b as f64 + (255.0 - b as f64) * amount).min(255.0) as u8;
        Color::Rgb { r: r2, g: g2, b: b2 }
    } else {
        color
    }
}

/// WCAG 2.0 relative luminance for an sRGB color.
pub fn relative_luminance(r: u8, g: u8, b: u8) -> f64 {
    let to_linear = |c: u8| -> f64 {
        let s = c as f64 / 255.0;
        if s <= 0.03928 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * to_linear(r) + 0.7152 * to_linear(g) + 0.0722 * to_linear(b)
}

/// WCAG 2.0 contrast ratio between two colors (1.0 to 21.0).
pub fn contrast_ratio(c1: Color, c2: Color) -> f64 {
    let (r1, g1, b1) = color_to_rgb(c1).unwrap_or((128, 128, 128));
    let (r2, g2, b2) = color_to_rgb(c2).unwrap_or((128, 128, 128));
    let l1 = relative_luminance(r1, g1, b1);
    let l2 = relative_luminance(r2, g2, b2);
    let (lighter, darker) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (lighter + 0.05) / (darker + 0.05)
}

/// Ensure a badge background has sufficient contrast against the page background.
/// If contrast ratio < 1.5, lighten the badge_bg by 30%.
pub fn ensure_badge_contrast(badge_bg: Color, page_bg: Color) -> Color {
    if contrast_ratio(badge_bg, page_bg) < 1.5 {
        lighten_color(badge_bg, 0.30)
    } else {
        badge_bg
    }
}

pub fn hex_to_color(hex: &str) -> Option<Color> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
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
}
