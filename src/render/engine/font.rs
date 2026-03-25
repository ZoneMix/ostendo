//! Terminal font size control protocols.
//!
//! Supports Kitty terminal's remote control protocol for per-slide font sizing,
//! and Ghostty's keystroke injection via AppleScript on macOS.
//!
//! # How Font Sizing Works
//!
//! Each slide can have a font size directive (`<!-- font_size: 3 -->`). The
//! presenter stores a per-slide font offset and computes the target font size
//! as `original_font_size + (offset * 2pt)`. The offset can also be adjusted
//! interactively with `]` (increase) and `[` (decrease).
//!
//! # Kitty Protocol
//!
//! Font changes use Kitty's remote control protocol via escape sequences:
//! `\x1bP@kitty-cmd{JSON}\x1b\\`. The `no_response: true` flag prevents
//! Kitty from sending response bytes that would confuse crossterm's input parser.
//!
//! # Ghostty Protocol
//!
//! Since Ghostty doesn't expose a remote control API, font size is changed by
//! simulating Cmd+= and Cmd+- keystrokes via macOS AppleScript. This is slower
//! than Kitty's direct protocol but functional.
//!
//! # Font Restoration
//!
//! The original font size is queried at startup (via `kitten @ get-font-size`
//! for Kitty, or config file parsing for Ghostty) and restored on exit via
//! `reset_font_size()`.

use super::*;
use crate::presentation::ImagePosition;

impl Presenter {
    /// Send a Kitty set_font_size command directly via escape sequences.
    /// Uses no_response:true to prevent Kitty from sending responses that
    /// would pollute crossterm's terminal input stream.
    ///
    /// Kitty RC protocol: \x1bP@kitty-cmd{JSON}\x1b\\
    /// Payload: size (float), increment_op (null=absolute, "+"=add, "-"=subtract)
    ///
    /// When `flush` is false, the escape is written but not flushed — it will
    /// piggyback on the next render_frame() flush, avoiding a premature resize
    /// that causes flicker during slide transitions.
    pub(crate) fn kitty_font_size_absolute(&self, size: f64, flush: bool) {
        if self.font_capability != FontSizeCapability::KittyRemote {
            return;
        }
        let esc = kitty_font_escape(size);
        let _ = std::io::Write::write_all(&mut std::io::stdout(), esc.as_bytes());
        if flush {
            let _ = std::io::Write::flush(&mut std::io::stdout());
        }
    }

    /// Set Ghostty's font size to an absolute value via AppleScript keystroke
    /// simulation.  Resets to the config default first (Cmd+0), then sends
    /// the right number of Cmd+= or Cmd+- keystrokes to reach `target`.
    pub(crate) fn ghostty_set_font_size(&self, target: f64) {
        let base = self.original_font_size
            .as_ref()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(13.0);
        let delta = target - base;
        // Each Ghostty keystroke changes font by 1pt (default keybinding)
        let steps = delta.round() as i32;
        if steps == 0 {
            // Just reset to default
            let _ = std::process::Command::new("osascript")
                .args(["-e", r#"tell application "System Events" to keystroke "0" using {command down}"#])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            return;
        }
        let (key, count) = if steps > 0 {
            ("=", steps as u32)
        } else {
            ("-", (-steps) as u32)
        };
        // Build a single AppleScript that resets then sends N keystrokes
        let script = format!(
            r#"tell application "System Events"
  keystroke "0" using {{command down}}
  delay 0.02
  repeat {} times
    keystroke "{}" using {{command down}}
  end repeat
end tell"#,
            count, key
        );
        let _ = std::process::Command::new("osascript")
            .args(["-e", &script])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }

    /// Query Kitty's current font size at startup so we can restore it on exit.
    /// Tries `kitten @ get-font-size` first, then falls back to reading kitty.conf.
    pub(crate) fn query_kitty_font_size() -> Option<String> {
        // Try kitten @ get-font-size (requires allow_remote_control)
        if let Ok(output) = std::process::Command::new("kitten")
            .args(["@", "get-font-size"])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
        {
            if output.status.success() {
                let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !s.is_empty() && s.parse::<f64>().is_ok() {
                    return Some(s);
                }
            }
        }
        // Fallback: parse font_size from kitty.conf
        if let Some(home) = std::env::var_os("HOME") {
            let conf = std::path::Path::new(&home).join(".config/kitty/kitty.conf");
            if let Ok(content) = std::fs::read_to_string(conf) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("font_size") {
                        if let Some(val) = trimmed.strip_prefix("font_size") {
                            let val = val.trim();
                            if val.parse::<f64>().is_ok() {
                                return Some(val.to_string());
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Query Ghostty's configured font size by reading the config file.
    /// Ghostty config is at ~/.config/ghostty/config (key=value format).
    /// Default font size is 13pt if not configured.
    pub(crate) fn query_ghostty_font_size() -> Option<String> {
        if let Some(home) = std::env::var_os("HOME") {
            let conf = std::path::Path::new(&home).join(".config/ghostty/config");
            if let Ok(content) = std::fs::read_to_string(conf) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("font-size") {
                        // Ghostty uses "font-size = N" or "font-size=N"
                        if let Some(val) = trimmed.strip_prefix("font-size") {
                            let val = val.trim().trim_start_matches('=').trim();
                            if val.parse::<f64>().is_ok() {
                                return Some(val.to_string());
                            }
                        }
                    }
                }
            }
        }
        // Ghostty default is 13pt
        Some("13".to_string())
    }

    /// Restore font to original size captured at startup.
    pub(crate) fn reset_font_size(&self) {
        match self.font_capability {
            FontSizeCapability::KittyRemote => {
                if let Some(ref size) = self.original_font_size {
                    if let Ok(s) = size.parse::<f64>() {
                        self.kitty_font_size_absolute(s, true);
                    } else {
                        self.kitty_font_size_absolute(0.0, true);
                    }
                } else {
                    self.kitty_font_size_absolute(0.0, true);
                }
            }
            FontSizeCapability::GhosttyKeystroke => {
                // Cmd+0 resets to config default
                let _ = std::process::Command::new("osascript")
                    .args(["-e", r#"tell application "System Events" to keystroke "0" using {command down}"#])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
            FontSizeCapability::None => {}
        }
    }

    /// Set the terminal's default background color via OSC 11.
    ///
    /// OSC 11 is a standard terminal escape sequence that changes the terminal's
    /// default background. This is used so that when Kitty resizes cells during
    /// font changes, the newly created cells inherit the theme background color
    /// instead of flashing black.
    pub(crate) fn set_terminal_bg(color: Color) {
        if let Color::Rgb { r, g, b } = color {
            let esc = format!("\x1b]11;rgb:{:02x}/{:02x}/{:02x}\x1b\\", r, g, b);
            let _ = std::io::Write::write_all(&mut io::stdout(), esc.as_bytes());
            let _ = std::io::Write::flush(&mut io::stdout());
        }
    }

    /// Reset the terminal's default background color to its original value.
    pub(crate) fn reset_terminal_bg() {
        // OSC 111 resets to the terminal's configured default
        let _ = std::io::Write::write_all(&mut io::stdout(), b"\x1b]111\x1b\\");
        let _ = std::io::Write::flush(&mut io::stdout());
    }

    /// Compute the font size for the current slide and store it as pending.
    ///
    /// The font size is computed as: `original_font_size + (offset * 2.0)`.
    /// The change is deferred by storing it in `pending_font_size` rather than
    /// applying immediately. The actual font change happens at the beginning
    /// of the next `render_frame()` call, where the terminal can be properly
    /// queried for new dimensions after the resize.
    pub(crate) fn apply_slide_font(&mut self) {
        if !self.font_capability.is_available() {
            return;
        }
        let offset = self.slide_font_offsets.get(&self.current).copied().unwrap_or(0);
        let target = if let Some(ref orig) = self.original_font_size {
            if let Ok(base) = orig.parse::<f64>() {
                base + (offset as f64 * 2.0)
            } else {
                0.0
            }
        } else {
            0.0
        };
        self.pending_font_size = Some(target);
    }

    /// Applies a pending font size change, including optional scatter-dissolve transition.
    ///
    /// When `font_change_is_slide_transition` is set, a scatter-dissolve animation
    /// plays during the font change so the screen is never blank. Font step batches
    /// are interleaved between dissolve frames so the zoom and dissolve overlap.
    /// Otherwise, a plain font stepping approach is used (smooth or instant depending
    /// on the `font_transition` directive).
    ///
    /// After the font change, terminal dimensions are re-queried and resize events drained.
    pub(crate) fn apply_font_change(&mut self, target: f64) -> Result<()> {
        let mut font_applied = false;

        // ── Slide transition: scatter-dissolve interleaved with font stepping ──
        // Restored from v0.3.1: font changes gradually during the dissolve so
        // the terminal resizes smoothly and the dissolve masks any artifacts.
        if self.font_change_is_slide_transition != FontTransitionMode::None {
            let old_buf = self.last_rendered_buffer.clone();
            if !old_buf.is_empty() {
                // Clear Kitty images before the transition
                if self.image_protocol == ImageProtocol::Kitty {
                    let stdout = io::stdout();
                    let mut pre = stdout.lock();
                    pre.write_all(KITTY_CLEAR_IMAGES)?;
                    pre.flush()?;
                }

                // For Ghostty, fire the keystrokes now — the dissolve
                // animation covers the processing delay.
                if matches!(self.font_capability, FontSizeCapability::GhosttyKeystroke) {
                    self.ghostty_set_font_size(target);
                }

                // Build a full-screen buffer: status bar + separator + content.
                let status_rows = if self.show_fullscreen { 0u16 } else { 2 };
                let mut screen_buf: Vec<StyledLine> = Vec::new();
                if status_rows > 0 {
                    let bar = self.build_status_bar(self.width as usize);
                    screen_buf.push(bar);
                    screen_buf.push(StyledLine::empty());
                }
                for line in &old_buf {
                    screen_buf.push(line.clone());
                }

                // Calculate Kitty font stepping parameters
                let current_font = self.last_applied_font_size.unwrap_or(target);
                let font_delta = target - current_font;
                let num_font_steps = if font_delta.abs() > 0.3 {
                    (font_delta.abs() / 0.2).round() as usize
                } else {
                    0
                };
                let font_dir = if font_delta >= 0.0 { 1.0_f64 } else { -1.0_f64 };

                // Scale dissolve to cover font stepping, with a minimum
                // duration so small font changes don't feel too abrupt.
                let font_step_time_ms = num_font_steps as u32 * 8;
                let target_duration_ms = font_step_time_ms.max(400);
                let dissolve_frames = (target_duration_ms / 30).clamp(12, 20);
                let mut font_steps_sent = 0usize;

                for frame in 1..=dissolve_frames {
                    let progress = frame as f64 / dissolve_frames as f64;

                    // Re-query terminal dimensions (font steps resize the terminal)
                    if frame > 1 && num_font_steps > 0 {
                        self.window_size = WindowSize::query();
                        self.width = self.window_size.columns;
                        self.height = self.window_size.rows;
                    }
                    let tw = self.width as usize;

                    // Render one dissolve frame
                    {
                        let stdout = io::stdout();
                        let mut fw = BufWriter::with_capacity(64 * 1024, stdout.lock());
                        let dis_has_grad = self.gradient_from.is_some() && self.gradient_to.is_some();
                        let grad_total = (self.height.saturating_sub(status_rows)) as usize;
                        queue!(fw, BeginSynchronizedUpdate)?;
                        for row in 0..self.height {
                            let row_bg = if dis_has_grad && row >= status_rows {
                                self.row_bg_color((row - status_rows) as usize, grad_total.max(1))
                            } else {
                                self.bg_color
                            };
                            queue!(fw, cursor::MoveTo(0, row), SetBackgroundColor(row_bg))?;
                            if let Some(line) = screen_buf.get(row as usize) {
                                let mut col = 0usize;
                                for span in &line.spans {
                                    if col >= tw { break; }
                                    let span_bg = span.bg.unwrap_or(row_bg);
                                    let fg = span.fg.unwrap_or(self.text_color);
                                    let dimmed_fg = interpolate_color(fg, span_bg, progress * 0.7);
                                    let dimmed_bg = interpolate_color(span_bg, row_bg, progress * 0.7);
                                    for ch in span.text.chars() {
                                        if col >= tw { break; }
                                        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
                                        let group = col / 2;
                                        let hash = (row as u64).wrapping_mul(31)
                                            .wrapping_add(group as u64)
                                            .wrapping_mul(7919) % 1000;
                                        let threshold = hash as f64 / 1000.0;
                                        if threshold < progress {
                                            queue!(fw, SetBackgroundColor(row_bg))?;
                                            for _ in 0..cw { write!(fw, " ")?; }
                                        } else {
                                            queue!(fw, SetBackgroundColor(dimmed_bg))?;
                                            queue!(fw, SetForegroundColor(dimmed_fg))?;
                                            write!(fw, "{}", ch)?;
                                        }
                                        col += cw;
                                    }
                                }
                                if col < tw {
                                    for _ in 0..tw - col { write!(fw, " ")?; }
                                }
                            } else {
                                for _ in 0..tw { write!(fw, " ")?; }
                            }
                        }
                        queue!(fw, EndSynchronizedUpdate, ResetColor)?;
                        fw.flush()?;
                    }

                    // Interleave Kitty font step batch + pace the frame
                    let frame_target_ms = target_duration_ms / dissolve_frames;
                    let frame_start = std::time::Instant::now();

                    if num_font_steps > 0 && matches!(self.font_capability, FontSizeCapability::KittyRemote) {
                        let target_steps = ((num_font_steps as f64 * progress).round() as usize)
                            .min(num_font_steps);
                        let batch = target_steps - font_steps_sent;
                        if batch > 0 {
                            let stdout = io::stdout();
                            let mut pre = stdout.lock();
                            for s in 0..batch {
                                let step_idx = font_steps_sent + s + 1;
                                let intermediate = current_font + font_dir * 0.2 * step_idx as f64;
                                pre.write_all(kitty_font_escape(intermediate).as_bytes())?;
                                pre.flush()?;
                                std::thread::sleep(std::time::Duration::from_millis(8));
                            }
                            font_steps_sent = target_steps;
                        }
                    }

                    // Pad remaining time so the dissolve isn't too fast
                    let elapsed = frame_start.elapsed().as_millis() as u32;
                    if elapsed < frame_target_ms {
                        std::thread::sleep(std::time::Duration::from_millis(
                            (frame_target_ms - elapsed) as u64,
                        ));
                    }
                }

                // Final font step — land exactly on target
                if self.font_capability == FontSizeCapability::KittyRemote {
                    let stdout = io::stdout();
                    let mut pre = stdout.lock();
                    pre.write_all(kitty_font_escape(target).as_bytes())?;
                    pre.flush()?;
                } // else: Ghostty keystrokes already sent above
                font_applied = true;
                self.pending_dissolve_in = true;
            }
            self.font_change_is_slide_transition = FontTransitionMode::None;
        }

        // ── Plain font stepping (interactive ] / [ or slide with font_transition: none) ──
        if !font_applied {
            // Always skip stepping — jump directly to the target font size.
            // The old incremental stepping (0.2pt with 8ms sleeps) created a
            // visible "growing/shrinking" effect that felt slow and jarring.
            let skip_stepping = true;
            self.skip_next_font_stepping = false;
            match self.font_capability {
                FontSizeCapability::KittyRemote => {
                    let stdout = io::stdout();
                    let mut pre = stdout.lock();

                    if skip_stepping {
                        queue!(pre, BeginSynchronizedUpdate)?;
                        for row in 0..self.height {
                            queue!(pre, cursor::MoveTo(0, row), SetBackgroundColor(self.bg_color))?;
                            write!(pre, "{:width$}", "", width = self.width as usize)?;
                        }
                        queue!(pre, EndSynchronizedUpdate, ResetColor)?;
                        pre.flush()?;
                    }

                    if self.image_protocol == ImageProtocol::Kitty {
                        pre.write_all(KITTY_CLEAR_IMAGES)?;
                        pre.flush()?;
                    }

                    if !skip_stepping {
                        let current = self.last_applied_font_size.unwrap_or(target);
                        if (target - current).abs() > 0.3 {
                            let step = 0.2_f64;
                            let delta = target - current;
                            let dir = if delta >= 0.0 { 1.0 } else { -1.0 };
                            let num_steps = (delta.abs() / step).round() as usize;
                            for i in 1..num_steps {
                                let intermediate = current + dir * step * i as f64;
                                pre.write_all(kitty_font_escape(intermediate).as_bytes())?;
                                pre.flush()?;
                                std::thread::sleep(std::time::Duration::from_millis(8));
                            }
                        }
                    }

                    pre.write_all(kitty_font_escape(target).as_bytes())?;
                    pre.flush()?;
                    drop(pre);
                }
                FontSizeCapability::GhosttyKeystroke => {
                    self.ghostty_set_font_size(target);
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                FontSizeCapability::None => {}
            }
        }

        self.last_applied_font_size = Some(target);
        // Let the terminal settle after font change, then drain resize events
        std::thread::sleep(std::time::Duration::from_millis(30));
        while event::poll(std::time::Duration::from_millis(10))? {
            if let Event::Resize(w2, h2) = event::read()? {
                self.width = w2;
                self.height = h2;
            } else {
                break;
            }
        }
        self.window_size = WindowSize::query();
        self.width = self.window_size.columns;
        self.height = self.window_size.rows;
        self.needs_full_redraw = true;
        Ok(())
    }

    /// Runs the scatter-reveal dissolve-in animation after a font transition.
    ///
    /// Mirrors the dissolve-out so the transition feels symmetric. Protocol images
    /// are emitted on the final frame within the same synchronized update block so
    /// they appear atomically with the fully-revealed content.
    pub(crate) fn render_dissolve_in(
        &mut self,
        pending_protocol_images: &[(String, usize, usize, ImagePosition)],
        visible_start: usize,
        visible_end: usize,
        status_bar_rows: usize,
    ) -> Result<()> {
        self.pending_dissolve_in = false;
        let dissolve_lines = std::mem::take(&mut self.last_rendered_buffer);
        if dissolve_lines.is_empty() {
            self.active_animation = None;
            self.needs_full_redraw = true;
            return Ok(());
        }

        let dis_frames = 12u32;
        let dis_tw = self.width as usize;
        let dis_status = if self.show_fullscreen { 0u16 } else { 2 };
        let dis_content_rows = (self.height - dis_status) as usize;
        let dis_visible = dissolve_lines.len().min(dis_content_rows);
        let status_bar = if dis_status > 0 {
            self.build_status_bar(dis_tw)
        } else {
            StyledLine::empty()
        };

        for frame in 1..=dis_frames {
            let progress = frame as f64 / dis_frames as f64;
            let dim = (1.0 - progress) * 0.4;
            let is_last = frame == dis_frames;
            let stdout = io::stdout();
            let mut dw = BufWriter::with_capacity(64 * 1024, stdout.lock());
            queue!(dw, BeginSynchronizedUpdate)?;

            let din_has_grad = self.gradient_from.is_some() && self.gradient_to.is_some();
            let din_grad_total = dis_content_rows + if dis_status > 0 { 1 } else { 0 };

            // Status bar at full brightness
            if dis_status > 0 {
                queue!(dw, cursor::MoveTo(0, 0))?;
                self.queue_styled_line(&mut dw, &status_bar, dis_tw)?;
                let sep_bg = if din_has_grad {
                    self.row_bg_color(0, din_grad_total.max(1))
                } else {
                    self.bg_color
                };
                queue!(dw, cursor::MoveTo(0, 1), SetBackgroundColor(sep_bg))?;
                write!(dw, "{:width$}", "", width = dis_tw)?;
            }

            let din_grad_offset = if dis_status > 0 { 1 } else { 0 };

            // Content: per-cell scatter reveal
            for (i, line) in dissolve_lines[..dis_visible].iter().enumerate() {
                if line.is_scale_placeholder { continue; }
                let row = (dis_status as usize + i) as u16;
                let row_bg = if din_has_grad {
                    self.row_bg_color(din_grad_offset + i, din_grad_total.max(1))
                } else {
                    self.bg_color
                };
                queue!(dw, cursor::MoveTo(0, row), SetBackgroundColor(row_bg))?;
                let mut col = 0usize;
                for span in &line.spans {
                    if col >= dis_tw { break; }
                    let span_bg = span.bg.unwrap_or(row_bg);
                    let fg = span.fg.unwrap_or(self.text_color);
                    let dimmed_fg = interpolate_color(fg, span_bg, dim);
                    let dimmed_bg = interpolate_color(span_bg, row_bg, dim);
                    for ch in span.text.chars() {
                        if col >= dis_tw { break; }
                        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
                        let group = col / 2;
                        let hash = (row as u64).wrapping_mul(31)
                            .wrapping_add(group as u64)
                            .wrapping_mul(7919) % 1000;
                        let threshold = hash as f64 / 1000.0;
                        if threshold < progress {
                            queue!(dw, SetBackgroundColor(dimmed_bg),
                                       SetForegroundColor(dimmed_fg))?;
                            write!(dw, "{}", ch)?;
                        } else {
                            queue!(dw, SetBackgroundColor(row_bg))?;
                            write!(dw, "{:width$}", "", width = cw)?;
                        }
                        col += cw;
                    }
                }
                if col < dis_tw {
                    queue!(dw, SetBackgroundColor(row_bg))?;
                    write!(dw, "{:width$}", "", width = dis_tw - col)?;
                }
            }

            // Fill remaining rows
            for i in dis_visible..dis_content_rows {
                let row = (dis_status as usize + i) as u16;
                let row_bg = if din_has_grad {
                    self.row_bg_color(din_grad_offset + i, din_grad_total.max(1))
                } else {
                    self.bg_color
                };
                queue!(dw, cursor::MoveTo(0, row), SetBackgroundColor(row_bg))?;
                write!(dw, "{:width$}", "", width = dis_tw)?;
            }

            // Emit protocol images on the final frame
            if is_last {
                let dis_tw = self.width as usize;
                for (escape_data, line_offset, img_cols, img_pos) in pending_protocol_images {
                    if *line_offset >= visible_start && *line_offset < visible_end {
                        let display_row = line_offset - visible_start;
                        let screen_row = (status_bar_rows + display_row) as u16;
                        if *img_pos == ImagePosition::Right && *img_cols > 0 {
                            let right_col = dis_tw.saturating_sub(*img_cols) as u16;
                            queue!(dw, cursor::MoveTo(right_col, screen_row))?;
                        } else if *img_cols > 0 {
                            let center_col = (dis_tw.saturating_sub(*img_cols) / 2) as u16;
                            queue!(dw, cursor::MoveTo(center_col, screen_row))?;
                        } else {
                            queue!(dw, cursor::MoveTo(0, screen_row))?;
                        }
                        write!(dw, "{}", escape_data)?;
                    }
                }
            }

            queue!(dw, EndSynchronizedUpdate, ResetColor)?;
            dw.flush()?;
            std::thread::sleep(std::time::Duration::from_millis(25));
        }

        // The dissolve-in already revealed content, so skip any remaining
        // transition/entrance animation to avoid double-reveal.
        self.active_animation = None;
        self.needs_full_redraw = true;
        Ok(())
    }
}
