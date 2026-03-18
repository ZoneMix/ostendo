//! Continuous loop animations that run while a slide is displayed.
//!
//! Loop animations modify the rendered buffer on every frame. They never complete
//! and are replaced only when the user navigates to a different slide.

use crossterm::style::Color;

use crate::render::text::{LineContentType, StyledLine, StyledSpan};
use crate::theme::colors::interpolate_color;

use super::{LoopAnimation, line_to_string};

/// Dispatch function: renders one frame of a loop animation, returning the modified buffer.
///
/// Called by the render engine on every tick. Unlike transitions and entrances, loop animations
/// use the `frame` counter (incrementing each tick) rather than a 0.0-1.0 progress value,
/// because they run indefinitely.
///
/// # Parameters
/// - `buffer`: The current slide's fully-rendered content.
/// - `animation`: Which loop effect to apply.
/// - `frame`: Monotonically increasing frame counter (drives animation timing).
/// - `accent`: The theme's accent color (used by bounce ball, pulse, and sparkle).
/// - `bg`: The theme's background color (used by pulse for interpolation).
/// - `width`: Terminal width in columns (used by matrix and bounce for positioning).
/// - `height`: Terminal height in rows (used by matrix and bounce for screen coverage).
/// - `target`: Optional animation target filter. When `Some("figlet")` or `Some("image")`,
///   only lines matching that content type are animated; other lines pass through unchanged.
#[allow(clippy::too_many_arguments)]
pub fn render_loop_frame(
    buffer: &[StyledLine],
    animation: LoopAnimation,
    frame: u64,
    accent: Color,
    bg: Color,
    width: usize,
    height: usize,
    target: Option<&str>,
) -> Vec<StyledLine> {
    match animation {
        LoopAnimation::Matrix => render_matrix(buffer, frame, width, height),
        LoopAnimation::Bounce => render_bounce(buffer, frame, accent, width, height),
        LoopAnimation::Pulse => render_pulse(buffer, frame, accent, bg),
        LoopAnimation::Sparkle => render_sparkle(buffer, frame, accent, target),
        LoopAnimation::Spin => render_spin(buffer, frame, target),
    }
}

/// Renders the Matrix rain loop animation: green cascading characters falling top-to-bottom.
///
/// Columns of random alphanumeric characters "rain" down the screen at different speeds.
/// Content rows are processed at character-level granularity to interleave rain and content spans.
fn render_matrix(
    buffer: &[StyledLine],
    frame: u64,
    width: usize,
    height: usize,
) -> Vec<StyledLine> {
    let bright_green = Color::Rgb { r: 0, g: 255, b: 0 };
    let green = Color::Rgb { r: 0, g: 180, b: 0 };
    let dim_green = Color::Rgb { r: 0, g: 80, b: 0 };
    let dark_green = Color::Rgb { r: 0, g: 40, b: 0 };
    let matrix_chars: &[u8] = b"0123456789abcdef:.<>+-=*/#@$%&";

    // Classify each column into a brightness level (0=space, 1-4=bright to dark)
    let classify = |col: usize, row: usize| -> (u8, u8) {
        let stream_speed = (col as u64 % 5) + 1;
        let stream_offset = col as u64 * 37 + 13;
        let drop_pos = ((frame * stream_speed + stream_offset) / 3) % (height as u64 * 2);
        let dist = (row as u64).wrapping_sub(drop_pos) % (height as u64 * 2);
        let ch_idx = ((col as u64 + row as u64 + frame) % matrix_chars.len() as u64) as usize;
        let ch = matrix_chars[ch_idx];
        let brightness = if dist == 0 { 4 } else if dist < 3 { 3 } else if dist < 6 { 2 } else if dist < 10 { 1 } else { 0 };
        (brightness, ch)
    };

    // Helper: append rain characters from col_start..col_end as batched spans
    let append_rain = |line: &mut StyledLine, row: usize, col_start: usize, col_end: usize| {
        let mut batch = String::with_capacity(col_end - col_start);
        let mut cur_brightness: u8 = 255; // sentinel
        for col in col_start..col_end {
            let (b, ch) = classify(col, row);
            if b != cur_brightness && !batch.is_empty() {
                line.push(styled_rain_span(&batch, cur_brightness, bright_green, green, dim_green, dark_green));
                batch.clear();
            }
            cur_brightness = b;
            if b == 0 {
                batch.push(' ');
            } else {
                batch.push(ch as char);
            }
        }
        if !batch.is_empty() {
            line.push(styled_rain_span(&batch, cur_brightness, bright_green, green, dim_green, dark_green));
        }
    };

    let mut result = Vec::with_capacity(height);

    for row in 0..height {
        let has_content = buffer.get(row)
            .map(|l| l.spans.iter().any(|s| s.text.chars().any(|c| !c.is_whitespace())))
            .unwrap_or(false);

        if has_content {
            let source_line = &buffer[row];
            let mut char_entries: Vec<(char, usize)> = Vec::new();
            for (span_idx, span) in source_line.spans.iter().enumerate() {
                for ch in span.text.chars() {
                    char_entries.push((ch, span_idx));
                }
            }
            let content_len = char_entries.len();

            let mut mixed_line = StyledLine::empty();
            let mut col = 0;

            while col < width {
                if col < content_len {
                    let (ch, _span_idx) = char_entries[col];
                    if ch.is_whitespace() {
                        let rain_start = col;
                        while col < content_len {
                            let (c, _) = char_entries[col];
                            if !c.is_whitespace() {
                                break;
                            }
                            col += 1;
                        }
                        append_rain(&mut mixed_line, row, rain_start, col);
                    } else {
                        let run_span_idx = char_entries[col].1;
                        let run_start = col;
                        while col < content_len {
                            let (c, si) = char_entries[col];
                            if c.is_whitespace() || si != run_span_idx {
                                break;
                            }
                            col += 1;
                        }
                        let text: String = char_entries[run_start..col].iter().map(|(c, _)| *c).collect();
                        mixed_line.push(StyledSpan {
                            text,
                            ..source_line.spans[run_span_idx].clone()
                        });
                    }
                } else {
                    append_rain(&mut mixed_line, row, col, width);
                    col = width;
                }
            }

            result.push(mixed_line);
        } else {
            let mut rain_line = StyledLine::empty();
            append_rain(&mut rain_line, row, 0, width);
            result.push(rain_line);
        }
    }
    result
}

/// Creates a styled span for matrix rain characters at the given brightness level.
///
/// Brightness levels: 4 = bright green + bold (leading drop), 3 = green, 2 = dim green,
/// 1 = dark green, 0 = unstyled (space).
fn styled_rain_span(text: &str, brightness: u8, bright: Color, green: Color, dim: Color, dark: Color) -> StyledSpan {
    match brightness {
        4 => StyledSpan::new(text).with_fg(bright).bold(),
        3 => StyledSpan::new(text).with_fg(green),
        2 => StyledSpan::new(text).with_fg(dim).dim(),
        1 => StyledSpan::new(text).with_fg(dark).dim(),
        _ => StyledSpan::new(text),
    }
}

/// Renders the bounce loop animation: a filled circle bouncing off the screen edges.
///
/// A single ball character in the theme's accent color moves across the screen, bouncing off
/// all four edges. The ball follows a triangle-wave pattern.
fn render_bounce(
    buffer: &[StyledLine],
    frame: u64,
    accent: Color,
    width: usize,
    height: usize,
) -> Vec<StyledLine> {
    let ball = "\u{25CF}";
    // Simple bounce physics -- triangle wave
    let period_x = (width.max(2) * 2) as u64;
    let period_y = (height.max(2) * 2) as u64;
    let x_pos = if period_x > 0 {
        let raw = frame % period_x;
        if raw < width as u64 { raw as usize } else { (period_x - raw) as usize }
    } else { 0 };
    let y_pos = if period_y > 0 {
        let raw = (frame * 2) % period_y;
        if raw < height as u64 { raw as usize } else { (period_y - raw) as usize }
    } else { 0 };

    let mut result: Vec<StyledLine> = buffer.to_vec();
    // Extend to fill screen if needed
    while result.len() < height {
        result.push(StyledLine::empty());
    }

    let target_row = y_pos.min(result.len().saturating_sub(1));
    let x_clamped = x_pos.min(width.saturating_sub(1));

    // Rebuild the target row with the ball inserted at x_pos
    let original = &result[target_row];
    let original_text = line_to_string(original);
    let original_chars: Vec<char> = original_text.chars().collect();

    let mut new_line = StyledLine::empty();
    // Part before ball
    if x_clamped > 0 {
        if x_clamped <= original_chars.len() {
            let prefix: String = original_chars[..x_clamped].iter().collect();
            let mut chars_emitted = 0;
            for span in &original.spans {
                let span_chars: Vec<char> = span.text.chars().collect();
                if chars_emitted >= x_clamped {
                    break;
                }
                let take = (x_clamped - chars_emitted).min(span_chars.len());
                let partial: String = span_chars[..take].iter().collect();
                new_line.push(StyledSpan { text: partial, ..span.clone() });
                chars_emitted += take;
            }
            if chars_emitted < x_clamped {
                new_line.push(StyledSpan::new(&" ".repeat(x_clamped - chars_emitted)));
            }
            let _ = prefix; // used via chars_emitted logic
        } else {
            let existing: String = original_chars.iter().collect();
            if !existing.is_empty() {
                for span in &original.spans {
                    new_line.push(span.clone());
                }
            }
            let pad = x_clamped - original_chars.len();
            if pad > 0 {
                new_line.push(StyledSpan::new(&" ".repeat(pad)));
            }
        }
    }

    // The ball itself
    new_line.push(StyledSpan::new(ball).with_fg(accent).bold());

    // Part after ball
    let after_pos = x_clamped + 1;
    if after_pos < original_chars.len() {
        let mut chars_emitted = 0;
        for span in &original.spans {
            let span_chars: Vec<char> = span.text.chars().collect();
            let span_end = chars_emitted + span_chars.len();
            if span_end <= after_pos {
                chars_emitted = span_end;
                continue;
            }
            let start_in_span = after_pos.saturating_sub(chars_emitted);
            let partial: String = span_chars[start_in_span..].iter().collect();
            if !partial.is_empty() {
                new_line.push(StyledSpan { text: partial, ..span.clone() });
            }
            chars_emitted = span_end;
        }
    }

    result[target_row] = new_line;
    result
}

/// Renders the pulse loop animation: all content brightness oscillates in a sine-wave pattern.
///
/// Every character on screen smoothly pulsates between dim and bright.
fn render_pulse(
    buffer: &[StyledLine],
    frame: u64,
    accent: Color,
    bg: Color,
) -> Vec<StyledLine> {
    // Sine wave oscillation: 0.3 to 1.0
    let t = 0.65 + 0.35 * (frame as f64 * 0.15).sin();
    let mut result = Vec::with_capacity(buffer.len());
    for line in buffer.iter() {
        let mut pulsed = StyledLine::empty();
        pulsed.is_scale_placeholder = line.is_scale_placeholder;
        pulsed.content_type = line.content_type;
        for span in &line.spans {
            let fg = span.fg.unwrap_or(accent);
            let pulsed_fg = interpolate_color(bg, fg, t);
            pulsed.push(StyledSpan {
                fg: Some(pulsed_fg),
                ..span.clone()
            });
        }
        result.push(pulsed);
    }
    result
}

/// Renders the sparkle loop animation: random cells twinkle as star/sparkle characters.
///
/// Non-whitespace cells periodically flash as sparkle characters in bright colors. The
/// `target` parameter can limit the effect to specific content types (`"figlet"` or `"image"`).
fn render_sparkle(
    buffer: &[StyledLine],
    frame: u64,
    accent: Color,
    target: Option<&str>,
) -> Vec<StyledLine> {
    let sparkle_chars: &[char] = &['\u{2726}', '\u{2727}', '\u{2605}', '\u{2606}', '\u{272B}', '\u{272C}', '\u{00B7}', '\u{207A}', '\u{2739}', '\u{2735}'];
    let bright_white = Color::Rgb { r: 255, g: 255, b: 255 };
    let bright_yellow = Color::Rgb { r: 255, g: 255, b: 100 };
    let bright_cyan = Color::Rgb { r: 100, g: 255, b: 255 };

    let mut result = Vec::with_capacity(buffer.len());

    for (row, line) in buffer.iter().enumerate() {
        let should_animate = match target {
            None => true,
            Some("figlet") => line.content_type == LineContentType::FigletTitle,
            Some("image") => line.content_type == LineContentType::AsciiImage,
            _ => true,
        };
        if !should_animate {
            result.push(line.clone());
            continue;
        }
        let chars: Vec<char> = line.spans.iter().flat_map(|s| s.text.chars()).collect();
        if chars.is_empty() || chars.iter().all(|c| c.is_whitespace()) {
            result.push(line.clone());
            continue;
        }

        // Determine which cells sparkle this frame
        let mut sparkle_map: Vec<Option<(char, Color)>> = vec![None; chars.len()];
        for col in 0..chars.len() {
            if chars[col].is_whitespace() { continue; }
            let cell_hash = (row as u64).wrapping_mul(7919).wrapping_add(col as u64 * 6271).wrapping_add(31);
            let sparkle_period = 40 + (cell_hash % 50);
            let phase = (frame.wrapping_add(cell_hash)) % sparkle_period;
            if phase < 3 {
                let ch_idx = ((cell_hash + frame) % sparkle_chars.len() as u64) as usize;
                let color_pick = (cell_hash + frame / 3) % 4;
                let color = match color_pick {
                    0 => bright_white,
                    1 => bright_yellow,
                    2 => bright_cyan,
                    _ => accent,
                };
                sparkle_map[col] = Some((sparkle_chars[ch_idx], color));
            }
        }

        if sparkle_map.iter().all(|s| s.is_none()) {
            result.push(line.clone());
            continue;
        }

        // Rebuild line with sparkles injected
        let mut new_line = StyledLine::empty();
        new_line.content_type = line.content_type;
        let mut char_pos = 0;
        for span in &line.spans {
            let span_chars: Vec<char> = span.text.chars().collect();
            let mut run_start = 0;
            while run_start < span_chars.len() {
                let global_pos = char_pos + run_start;
                if global_pos < sparkle_map.len() {
                    if let Some((sch, scolor)) = sparkle_map[global_pos] {
                        new_line.push(StyledSpan::new(&sch.to_string()).with_fg(scolor).bold());
                        run_start += 1;
                        continue;
                    }
                }
                let run_end = (run_start + 1..span_chars.len())
                    .find(|&i| {
                        let gp = char_pos + i;
                        gp < sparkle_map.len() && sparkle_map[gp].is_some()
                    })
                    .unwrap_or(span_chars.len());
                let chunk: String = span_chars[run_start..run_end].iter().collect();
                new_line.push(StyledSpan { text: chunk, ..span.clone() });
                run_start = run_end;
            }
            char_pos += span_chars.len();
        }
        result.push(new_line);
    }
    result
}

/// Renders the spin loop animation: ASCII art characters cycle through the brightness ramp.
///
/// Each ASCII art character is shifted along a brightness ramp by a sine-wave offset that
/// varies by position and frame, creating a shimmering wave effect. The `target` parameter
/// can limit the effect to `"figlet"` or `"image"` lines.
fn render_spin(
    buffer: &[StyledLine],
    frame: u64,
    target: Option<&str>,
) -> Vec<StyledLine> {
    const ASCII_RAMP: &[u8] = b" .'`^\",:;Il!i><~+_-?][}{1)(|/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$";

    // Build O(1) lookup table: byte value -> ramp index (255 = not in ramp)
    let mut ramp_lookup = [255u8; 128];
    for (i, &b) in ASCII_RAMP.iter().enumerate() {
        if (b as usize) < 128 {
            ramp_lookup[b as usize] = i as u8;
        }
    }

    let mut result = Vec::with_capacity(buffer.len());

    for (row, line) in buffer.iter().enumerate() {
        let should_animate = match target {
            None => true,
            Some("figlet") => line.content_type == LineContentType::FigletTitle,
            Some("image") => line.content_type == LineContentType::AsciiImage,
            _ => true,
        };
        if !should_animate {
            result.push(line.clone());
            continue;
        }
        let chars: Vec<char> = line.spans.iter().flat_map(|s| s.text.chars()).collect();
        if chars.is_empty() || chars.iter().all(|c| c.is_whitespace()) {
            result.push(line.clone());
            continue;
        }

        let has_art = line.spans.iter().any(|s| {
            !s.text.trim().is_empty() && s.fg.is_some() && s.text.chars().any(|c| !c.is_whitespace())
        });
        if !has_art {
            result.push(line.clone());
            continue;
        }

        // Rebuild with shifted ASCII ramp characters
        let mut new_line = StyledLine::empty();
        new_line.content_type = line.content_type;
        let mut char_pos: usize = 0;
        for span in &line.spans {
            let span_chars: Vec<char> = span.text.chars().collect();
            let mut new_text = String::with_capacity(span_chars.len());
            for (i, &ch) in span_chars.iter().enumerate() {
                let global_col = char_pos + i;
                if ch.is_whitespace() || !ch.is_ascii() {
                    new_text.push(ch);
                    continue;
                }
                let ramp_pos = if (ch as u32) < 128 {
                    let idx = ramp_lookup[ch as usize];
                    if idx < 255 { Some(idx as usize) } else { None }
                } else { None };
                if let Some(ramp_pos) = ramp_pos {
                    let cell_phase = (row as f64 * 0.3 + global_col as f64 * 0.2).sin();
                    let wave = (frame as f64 * 0.12 + cell_phase * 3.0).sin();
                    let shift = (wave * 4.0) as i32;
                    let new_pos = (ramp_pos as i32 + shift)
                        .clamp(1, ASCII_RAMP.len() as i32 - 1) as usize;
                    new_text.push(ASCII_RAMP[new_pos] as char);
                } else {
                    new_text.push(ch);
                }
            }
            new_line.push(StyledSpan { text: new_text, ..span.clone() });
            char_pos += span_chars.len();
        }
        result.push(new_line);
    }
    result
}
