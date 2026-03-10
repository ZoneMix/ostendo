use super::*;

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
}
