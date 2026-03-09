use crossterm::style::Color;
use std::time::Instant;

use crate::render::text::{StyledLine, StyledSpan};
use crate::theme::colors::interpolate_color;

/// Types of transition animations between slides.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransitionType {
    Fade,
    SlideLeft,
    Dissolve,
}

/// Types of entrance animations (play once when slide appears).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EntranceAnimation {
    Typewriter,
    FadeIn,
    SlideDown,
}

/// Types of looping animations (continuous).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LoopAnimation {
    Matrix,
    Bounce,
    Pulse,
    Sparkle,
    Spin,
}

/// The kind of animation currently active.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnimationKind {
    Transition(TransitionType),
    Entrance(EntranceAnimation),
    Loop(LoopAnimation),
}

/// State machine tracking an active animation.
pub struct AnimationState {
    pub kind: AnimationKind,
    pub started: Instant,
    pub duration_ms: u64,
    pub frame: u64,
    pub old_buffer: Vec<StyledLine>,
    pub new_buffer: Vec<StyledLine>,
}

impl AnimationState {
    pub fn new_transition(
        kind: TransitionType,
        old_buffer: Vec<StyledLine>,
        new_buffer: Vec<StyledLine>,
    ) -> Self {
        let duration_ms = match kind {
            TransitionType::Dissolve => 600,
            TransitionType::Fade => 400,
            _ => 300,
        };
        Self {
            kind: AnimationKind::Transition(kind),
            started: Instant::now(),
            duration_ms,
            frame: 0,
            old_buffer,
            new_buffer,
        }
    }

    pub fn new_entrance(kind: EntranceAnimation, buffer: Vec<StyledLine>) -> Self {
        Self {
            kind: AnimationKind::Entrance(kind),
            started: Instant::now(),
            duration_ms: 500,
            frame: 0,
            old_buffer: Vec::new(),
            new_buffer: buffer,
        }
    }

    pub fn new_loop(kind: LoopAnimation, buffer: Vec<StyledLine>) -> Self {
        Self {
            kind: AnimationKind::Loop(kind),
            started: Instant::now(),
            duration_ms: u64::MAX, // loops indefinitely
            frame: 0,
            old_buffer: Vec::new(),
            new_buffer: buffer,
        }
    }

    /// Progress from 0.0 to 1.0 for finite animations.
    pub fn progress(&self) -> f64 {
        let elapsed = self.started.elapsed().as_millis() as f64;
        (elapsed / self.duration_ms as f64).min(1.0)
    }

    /// Whether the animation has completed (always false for loops).
    pub fn is_done(&self) -> bool {
        match self.kind {
            AnimationKind::Loop(_) => false,
            _ => self.progress() >= 1.0,
        }
    }

    /// Advance frame counter (called each render tick).
    pub fn tick(&mut self) {
        self.frame += 1;
    }
}

/// Parse a transition type from a directive string.
pub fn parse_transition(s: &str) -> Option<TransitionType> {
    match s {
        "fade" => Some(TransitionType::Fade),
        "slide" => Some(TransitionType::SlideLeft),
        "dissolve" => Some(TransitionType::Dissolve),
        _ => None,
    }
}

/// Parse an entrance animation from a directive string.
pub fn parse_entrance(s: &str) -> Option<EntranceAnimation> {
    match s {
        "typewriter" => Some(EntranceAnimation::Typewriter),
        "fade_in" => Some(EntranceAnimation::FadeIn),
        "slide_down" => Some(EntranceAnimation::SlideDown),
        _ => None,
    }
}

/// Parse a loop animation from a directive string.
pub fn parse_loop_animation(s: &str) -> Option<LoopAnimation> {
    match s {
        "matrix" => Some(LoopAnimation::Matrix),
        "bounce" => Some(LoopAnimation::Bounce),
        "pulse" => Some(LoopAnimation::Pulse),
        "sparkle" => Some(LoopAnimation::Sparkle),
        "spin" => Some(LoopAnimation::Spin),
        _ => None,
    }
}

/// Render a transition animation frame, returning the blended buffer.
pub fn render_transition_frame(
    old: &[StyledLine],
    new: &[StyledLine],
    progress: f64,
    transition: TransitionType,
    bg: Color,
    width: usize,
) -> Vec<StyledLine> {
    match transition {
        TransitionType::Fade => render_fade(old, new, progress, bg),
        TransitionType::SlideLeft => render_slide_left(old, new, progress, width),
        TransitionType::Dissolve => render_dissolve(old, new, progress),
    }
}

/// Fade: interpolate fg colors old→bg→new over progress 0.0→1.0.
fn render_fade(
    old: &[StyledLine],
    new: &[StyledLine],
    progress: f64,
    bg: Color,
) -> Vec<StyledLine> {
    let max_len = old.len().max(new.len());
    let mut result = Vec::with_capacity(max_len);

    for i in 0..max_len {
        if progress < 0.5 {
            // Fade old toward bg (progress 0→0.5 maps to t 0→1)
            let t = progress * 2.0;
            let source = old.get(i).cloned().unwrap_or_else(StyledLine::empty);
            let mut faded = StyledLine::empty();
            for span in &source.spans {
                let fg = span.fg.unwrap_or(Color::White);
                let new_fg = interpolate_color(fg, bg, t);
                faded.push(StyledSpan {
                    fg: Some(new_fg),
                    ..span.clone()
                });
            }
            result.push(faded);
        } else {
            // Fade new from bg toward full color (progress 0.5→1 maps to t 0→1)
            let t = (progress - 0.5) * 2.0;
            let source = new.get(i).cloned().unwrap_or_else(StyledLine::empty);
            let mut faded = StyledLine::empty();
            for span in &source.spans {
                let fg = span.fg.unwrap_or(Color::White);
                let new_fg = interpolate_color(bg, fg, t);
                faded.push(StyledSpan {
                    fg: Some(new_fg),
                    ..span.clone()
                });
            }
            result.push(faded);
        }
    }
    result
}

/// SlideLeft: old content slides left, new enters from right.
/// Uses character-based slicing to avoid panics on multi-byte UTF-8.
fn render_slide_left(
    old: &[StyledLine],
    new: &[StyledLine],
    progress: f64,
    width: usize,
) -> Vec<StyledLine> {
    let max_len = old.len().max(new.len());
    let shift = (width as f64 * progress) as usize;
    let mut result = Vec::with_capacity(max_len);

    for i in 0..max_len {
        let old_chars: Vec<char> = old.get(i).map(line_to_string).unwrap_or_default().chars().collect();
        let new_chars: Vec<char> = new.get(i).map(line_to_string).unwrap_or_default().chars().collect();

        // Shift old left, new enters from right
        let old_visible: String = old_chars.iter().skip(shift).collect();
        let new_visible: String = new_chars.iter().take(shift).collect();

        let combined = format!("{}{}", old_visible, new_visible);
        result.push(StyledLine::plain(&combined));
    }
    result
}

/// Dissolve: characters jumble randomly and gradually resolve into the new content.
/// Early phase: each cell shows a random character, progressively replaced by target text.
fn render_dissolve(
    old: &[StyledLine],
    new: &[StyledLine],
    progress: f64,
) -> Vec<StyledLine> {
    let max_len = old.len().max(new.len());
    let jumble_chars: &[char] = &[
        '░', '▒', '▓', '█', '┃', '╋', '╳', '┫', '╬', '║', '╠',
        '◆', '◇', '○', '●', '□', '■', '△', '▲', '◌', '◍',
        '#', '@', '%', '&', '*', '~', '/', '\\', '|',
    ];
    let mut result = Vec::with_capacity(max_len);

    for row in 0..max_len {
        let new_line = new.get(row).cloned().unwrap_or_else(StyledLine::empty);
        let old_line = old.get(row).cloned().unwrap_or_else(StyledLine::empty);

        if progress >= 1.0 {
            result.push(new_line);
            continue;
        }
        if progress <= 0.0 {
            result.push(old_line);
            continue;
        }

        let new_text = line_to_string(&new_line);
        let old_text = line_to_string(&old_line);
        let new_chars: Vec<char> = new_text.chars().collect();
        let old_chars: Vec<char> = old_text.chars().collect();
        let max_cols = new_chars.len().max(old_chars.len());

        if max_cols == 0 {
            result.push(StyledLine::empty());
            continue;
        }

        // Build the dissolved line character by character
        let mut out = String::with_capacity(max_cols);
        for col in 0..max_cols {
            // Deterministic hash per cell — each cell resolves at a different progress point
            let cell_hash = ((row as u64).wrapping_mul(7919).wrapping_add(col as u64 * 6271).wrapping_add(31)) % 1000;
            let resolve_at = cell_hash as f64 / 1000.0;

            if progress > resolve_at {
                // This cell has resolved — show the new character
                out.push(*new_chars.get(col).unwrap_or(&' '));
            } else if progress > resolve_at * 0.5 {
                // This cell is jumbling — show random character
                let jumble_idx = ((cell_hash + (progress * 1000.0) as u64) % jumble_chars.len() as u64) as usize;
                out.push(jumble_chars[jumble_idx]);
            } else {
                // This cell still shows old content
                out.push(*old_chars.get(col).unwrap_or(&' '));
            }
        }

        // Use new line's styling but with the jumbled text
        // This preserves colors from the new slide
        if progress > 0.5 {
            result.push(rebuild_line_with_text(&new_line, &out, max_cols));
        } else {
            result.push(rebuild_line_with_text(&old_line, &out, max_cols));
        }
    }
    result
}

/// Rebuild a StyledLine replacing its text content while preserving span styling.
fn rebuild_line_with_text(source: &StyledLine, new_text: &str, _max_cols: usize) -> StyledLine {
    let chars: Vec<char> = new_text.chars().collect();
    let mut line = StyledLine::empty();
    let mut char_pos = 0;

    if source.spans.is_empty() {
        return StyledLine::plain(new_text);
    }

    for span in &source.spans {
        let span_len = span.text.chars().count();
        let take = span_len.min(chars.len().saturating_sub(char_pos));
        if take == 0 {
            char_pos += span_len;
            continue;
        }
        let replacement: String = chars[char_pos..char_pos + take].iter().collect();
        line.push(StyledSpan {
            text: replacement,
            ..span.clone()
        });
        char_pos += take;
    }
    // Any remaining characters not covered by original spans
    if char_pos < chars.len() {
        let rest: String = chars[char_pos..].iter().collect();
        line.push(StyledSpan::new(&rest));
    }
    line
}

/// Render an entrance animation frame.
pub fn render_entrance_frame(
    buffer: &[StyledLine],
    progress: f64,
    animation: EntranceAnimation,
    bg: Color,
) -> Vec<StyledLine> {
    match animation {
        EntranceAnimation::Typewriter => render_typewriter(buffer, progress),
        EntranceAnimation::FadeIn => render_fade_in(buffer, progress, bg),
        EntranceAnimation::SlideDown => render_slide_down(buffer, progress),
    }
}

/// Typewriter: reveal characters left-to-right.
fn render_typewriter(buffer: &[StyledLine], progress: f64) -> Vec<StyledLine> {
    let total_chars: usize = buffer.iter().map(line_char_count).sum();
    let reveal_count = (total_chars as f64 * progress) as usize;
    let mut chars_shown = 0;
    let mut result = Vec::with_capacity(buffer.len());

    for line in buffer {
        let line_len = line_char_count(line);
        if chars_shown >= reveal_count {
            result.push(StyledLine::empty());
        } else if chars_shown + line_len <= reveal_count {
            result.push(line.clone());
            chars_shown += line_len;
        } else {
            let remaining = reveal_count - chars_shown;
            let text = line_to_string(line);
            let visible: String = text.chars().take(remaining).collect();
            result.push(StyledLine::plain(&visible));
            chars_shown += line_len;
        }
    }
    result
}

/// FadeIn: interpolate fg from bg to full color.
fn render_fade_in(buffer: &[StyledLine], progress: f64, bg: Color) -> Vec<StyledLine> {
    let mut result = Vec::with_capacity(buffer.len());
    for line in buffer {
        let mut faded = StyledLine::empty();
        for span in &line.spans {
            let target_fg = span.fg.unwrap_or(Color::White);
            let current_fg = interpolate_color(bg, target_fg, progress);
            faded.push(StyledSpan {
                fg: Some(current_fg),
                ..span.clone()
            });
        }
        result.push(faded);
    }
    result
}

/// SlideDown: reveal lines top-to-bottom with delay.
fn render_slide_down(buffer: &[StyledLine], progress: f64) -> Vec<StyledLine> {
    let total = buffer.len();
    let reveal_rows = (total as f64 * progress) as usize;
    let mut result = Vec::with_capacity(total);
    for (i, line) in buffer.iter().enumerate() {
        if i < reveal_rows {
            result.push(line.clone());
        } else {
            result.push(StyledLine::empty());
        }
    }
    result
}

/// Render a loop animation frame, modifying the existing buffer.
pub fn render_loop_frame(
    buffer: &[StyledLine],
    animation: LoopAnimation,
    frame: u64,
    accent: Color,
    bg: Color,
    width: usize,
    height: usize,
) -> Vec<StyledLine> {
    match animation {
        LoopAnimation::Matrix => render_matrix(buffer, frame, width, height),
        LoopAnimation::Bounce => render_bounce(buffer, frame, accent, width, height),
        LoopAnimation::Pulse => render_pulse(buffer, frame, accent, bg),
        LoopAnimation::Sparkle => render_sparkle(buffer, frame, accent),
        LoopAnimation::Spin => render_spin(buffer, frame),
    }
}

/// Matrix: falling green characters raining top-to-bottom across the full screen.
/// Content is overlaid on top of the rain effect. Uses batched spans for performance.
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
    // Returns (brightness, char) for a given col,row,frame
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

    let mut result = Vec::with_capacity(height);

    for row in 0..height {
        let has_content = buffer.get(row)
            .map(|l| !line_to_string(l).trim().is_empty())
            .unwrap_or(false);

        if has_content {
            // Keep content as-is; no margin rain to keep it clean
            result.push(buffer[row].clone());
        } else {
            // Build rain line with batched spans (group consecutive same-brightness chars)
            let mut rain_line = StyledLine::empty();
            let mut batch = String::with_capacity(width);
            let mut cur_brightness: u8 = 255; // sentinel

            for col in 0..width {
                let (b, ch) = classify(col, row);
                if b != cur_brightness && !batch.is_empty() {
                    // Flush previous batch
                    rain_line.push(styled_rain_span(&batch, cur_brightness, bright_green, green, dim_green, dark_green));
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
                rain_line.push(styled_rain_span(&batch, cur_brightness, bright_green, green, dim_green, dark_green));
            }
            result.push(rain_line);
        }
    }
    result
}

fn styled_rain_span(text: &str, brightness: u8, bright: Color, green: Color, dim: Color, dark: Color) -> StyledSpan {
    match brightness {
        4 => StyledSpan::new(text).with_fg(bright).bold(),
        3 => StyledSpan::new(text).with_fg(green),
        2 => StyledSpan::new(text).with_fg(dim).dim(),
        1 => StyledSpan::new(text).with_fg(dark).dim(),
        _ => StyledSpan::new(text),
    }
}

/// Bounce: ASCII ball bouncing off screen edges, overlaid on top of all content.
fn render_bounce(
    buffer: &[StyledLine],
    frame: u64,
    accent: Color,
    width: usize,
    height: usize,
) -> Vec<StyledLine> {
    let ball = "●";
    // Simple bounce physics — triangle wave
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
            // Preserve original content before ball position
            let prefix: String = original_chars[..x_clamped].iter().collect();
            // Try to preserve styling from original spans
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
            // Pad if original was shorter
            if chars_emitted < x_clamped {
                new_line.push(StyledSpan::new(&" ".repeat(x_clamped - chars_emitted)));
            }
            let _ = prefix; // used via chars_emitted logic
        } else {
            // Original line is shorter than x_pos — pad with spaces, then ball
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

/// Pulse: title brightness oscillates via sine wave.
fn render_pulse(
    buffer: &[StyledLine],
    frame: u64,
    accent: Color,
    bg: Color,
) -> Vec<StyledLine> {
    // Sine wave oscillation: 0.3 to 1.0
    let t = 0.65 + 0.35 * (frame as f64 * 0.15).sin();
    let mut result = Vec::with_capacity(buffer.len());
    for (i, line) in buffer.iter().enumerate() {
        if i < 3 {
            // Apply pulse to title area (first few lines)
            let mut pulsed = StyledLine::empty();
            for span in &line.spans {
                let fg = span.fg.unwrap_or(accent);
                let pulsed_fg = interpolate_color(bg, fg, t);
                pulsed.push(StyledSpan {
                    fg: Some(pulsed_fg),
                    ..span.clone()
                });
            }
            result.push(pulsed);
        } else {
            result.push(line.clone());
        }
    }
    result
}

/// Sparkle: random star/sparkle characters twinkle on non-space cells.
/// Each frame, a different set of cells briefly becomes a sparkle character
/// in bright white/yellow, creating a twinkling starfield effect.
fn render_sparkle(
    buffer: &[StyledLine],
    frame: u64,
    accent: Color,
) -> Vec<StyledLine> {
    let sparkle_chars: &[char] = &['✦', '✧', '★', '☆', '✫', '✬', '·', '⁺', '✹', '✵'];
    let bright_white = Color::Rgb { r: 255, g: 255, b: 255 };
    let bright_yellow = Color::Rgb { r: 255, g: 255, b: 100 };
    let bright_cyan = Color::Rgb { r: 100, g: 255, b: 255 };

    let mut result = Vec::with_capacity(buffer.len());

    for (row, line) in buffer.iter().enumerate() {
        let text = line_to_string(line);
        let chars: Vec<char> = text.chars().collect();
        if chars.is_empty() || chars.iter().all(|c| c.is_whitespace()) {
            result.push(line.clone());
            continue;
        }

        // Determine which cells sparkle this frame
        let mut sparkle_map: Vec<Option<(char, Color)>> = vec![None; chars.len()];
        for col in 0..chars.len() {
            if chars[col].is_whitespace() { continue; }
            // Each cell has a "sparkle phase" — it sparkles for 2-3 frames then goes dark
            let cell_hash = (row as u64).wrapping_mul(7919).wrapping_add(col as u64 * 6271).wrapping_add(31);
            let sparkle_period = 15 + (cell_hash % 25); // 15-39 frames between sparkles
            let phase = (frame.wrapping_add(cell_hash)) % sparkle_period;
            if phase < 2 {
                // This cell is sparkling
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

        // Check if any cell sparkles on this line
        if sparkle_map.iter().all(|s| s.is_none()) {
            result.push(line.clone());
            continue;
        }

        // Rebuild line with sparkles injected
        let mut new_line = StyledLine::empty();
        let mut char_pos = 0;
        for span in &line.spans {
            let span_chars: Vec<char> = span.text.chars().collect();
            // Split span into runs of sparkle vs non-sparkle
            let mut run_start = 0;
            while run_start < span_chars.len() {
                let global_pos = char_pos + run_start;
                if global_pos < sparkle_map.len() {
                    if let Some((sch, scolor)) = sparkle_map[global_pos] {
                        // Sparkle cell
                        new_line.push(StyledSpan::new(&sch.to_string()).with_fg(scolor).bold());
                        run_start += 1;
                        continue;
                    }
                }
                // Find run of non-sparkle chars
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

/// Spin: ASCII art characters cycle through the brightness ramp, creating
/// a shimmering/morphing wave effect across the image. Each cell shifts
/// through nearby ASCII ramp characters at a different phase.
fn render_spin(
    buffer: &[StyledLine],
    frame: u64,
) -> Vec<StyledLine> {
    // Same ramp as ascii_art.rs for consistency
    const ASCII_RAMP: &[u8] = b" .'`^\",:;Il!i><~+_-?][}{1)(|/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$";

    let mut result = Vec::with_capacity(buffer.len());

    for (row, line) in buffer.iter().enumerate() {
        let text = line_to_string(line);
        let chars: Vec<char> = text.chars().collect();
        if chars.is_empty() || chars.iter().all(|c| c.is_whitespace()) {
            result.push(line.clone());
            continue;
        }

        // Check if this line has ASCII art characters (colored, non-whitespace cells)
        let has_art = line.spans.iter().any(|s| {
            !s.text.trim().is_empty() && s.fg.is_some() && s.text.chars().any(|c| !c.is_whitespace())
        });
        if !has_art {
            result.push(line.clone());
            continue;
        }

        // Rebuild with shifted ASCII ramp characters
        let mut new_line = StyledLine::empty();
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
                // Find current position in the ASCII ramp
                if let Some(ramp_pos) = ASCII_RAMP.iter().position(|&r| r == ch as u8) {
                    // Wave: each cell shifts by a sine-based offset depending on position and frame
                    let cell_phase = (row as f64 * 0.3 + global_col as f64 * 0.2).sin();
                    let wave = (frame as f64 * 0.12 + cell_phase * 3.0).sin();
                    let shift = (wave * 4.0) as i32; // ±4 positions in ramp
                    let new_pos = (ramp_pos as i32 + shift)
                        .clamp(1, ASCII_RAMP.len() as i32 - 1) as usize; // skip space at 0
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

// ── Helper functions ──

fn line_to_string(line: &StyledLine) -> String {
    line.spans.iter().map(|s| s.text.as_str()).collect()
}

fn line_char_count(line: &StyledLine) -> usize {
    line.spans.iter().map(|s| s.text.chars().count()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_transition() {
        assert_eq!(parse_transition("fade"), Some(TransitionType::Fade));
        assert_eq!(parse_transition("slide"), Some(TransitionType::SlideLeft));
        assert_eq!(parse_transition("dissolve"), Some(TransitionType::Dissolve));
        assert_eq!(parse_transition("unknown"), None);
    }

    #[test]
    fn test_parse_entrance() {
        assert_eq!(parse_entrance("typewriter"), Some(EntranceAnimation::Typewriter));
        assert_eq!(parse_entrance("fade_in"), Some(EntranceAnimation::FadeIn));
        assert_eq!(parse_entrance("slide_down"), Some(EntranceAnimation::SlideDown));
        assert_eq!(parse_entrance("unknown"), None);
    }

    #[test]
    fn test_parse_loop() {
        assert_eq!(parse_loop_animation("matrix"), Some(LoopAnimation::Matrix));
        assert_eq!(parse_loop_animation("bounce"), Some(LoopAnimation::Bounce));
        assert_eq!(parse_loop_animation("pulse"), Some(LoopAnimation::Pulse));
        assert_eq!(parse_loop_animation("unknown"), None);
    }

    #[test]
    fn test_animation_state_progress() {
        let state = AnimationState::new_transition(
            TransitionType::Fade,
            vec![StyledLine::plain("old")],
            vec![StyledLine::plain("new")],
        );
        // Just created — progress should be near 0
        assert!(state.progress() < 0.5);
        assert!(!state.is_done());
    }

    #[test]
    fn test_loop_never_done() {
        let state = AnimationState::new_loop(
            LoopAnimation::Pulse,
            vec![StyledLine::plain("test")],
        );
        assert!(!state.is_done());
    }

    #[test]
    fn test_fade_transition() {
        let bg = Color::Rgb { r: 0, g: 0, b: 0 };
        let old = vec![StyledLine::plain("old content")];
        let new = vec![StyledLine::plain("new content")];
        let result = render_transition_frame(&old, &new, 0.0, TransitionType::Fade, bg, 80);
        assert_eq!(result.len(), 1);
        let result_end = render_transition_frame(&old, &new, 1.0, TransitionType::Fade, bg, 80);
        assert_eq!(result_end.len(), 1);
    }

    #[test]
    fn test_dissolve_transition() {
        let bg = Color::Rgb { r: 0, g: 0, b: 0 };
        let old = vec![StyledLine::plain("AAAA")];
        let new = vec![StyledLine::plain("BBBB")];
        // At progress 0, all old
        let result_0 = render_transition_frame(&old, &new, 0.0, TransitionType::Dissolve, bg, 80);
        let text_0 = line_to_string(&result_0[0]);
        assert!(text_0.contains('A'));
        // At progress 1, all new
        let result_1 = render_transition_frame(&old, &new, 1.0, TransitionType::Dissolve, bg, 80);
        let text_1 = line_to_string(&result_1[0]);
        assert!(text_1.contains('B'));
    }

    #[test]
    fn test_typewriter_entrance() {
        let buffer = vec![StyledLine::plain("Hello World")];
        let bg = Color::Rgb { r: 0, g: 0, b: 0 };
        let half = render_entrance_frame(&buffer, 0.5, EntranceAnimation::Typewriter, bg);
        let text = line_to_string(&half[0]);
        assert!(text.len() < "Hello World".len());
    }
}
