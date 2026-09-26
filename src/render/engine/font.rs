//! Per-slide terminal font size (Kitty remote control, Ghostty keystrokes).
//!
//! A slide's `<!-- font_size: N -->` directive sets its offset; `]` / `[`
//! adjust it for this presentation and are remembered across sessions.

use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Stdio};

use crate::presentation::Slide;
use crate::terminal::protocols::FontSizeCapability;

/// Points per offset step.
const STEP_PT: f64 = 2.0;
const MIN_PT: f64 = 6.0;
const MAX_PT: f64 = 200.0;

pub(crate) struct FontControl {
    capability: FontSizeCapability,
    base: f64,
    applied: Option<f64>,
    pending: Option<f64>,
    directive: Vec<i8>,
    user: HashMap<usize, i8>,
}

impl FontControl {
    pub fn new(
        capability: FontSizeCapability,
        slides: &[Slide],
        saved: HashMap<usize, i8>,
    ) -> Self {
        let base = match capability {
            FontSizeCapability::KittyRemote => kitty_font_size().unwrap_or(11.0),
            FontSizeCapability::GhosttyKeystroke => ghostty_font_size().unwrap_or(13.0),
            FontSizeCapability::None => 0.0,
        };
        let mut font = Self {
            capability,
            base,
            applied: None,
            pending: None,
            directive: Vec::new(),
            user: saved,
        };
        font.set_directives(slides);
        font
    }

    pub fn available(&self) -> bool {
        self.capability.is_available()
    }

    /// Reads `font_size` directives. A directive of 1 is the terminal's own
    /// size; each step above or below changes the offset by two.
    pub fn set_directives(&mut self, slides: &[Slide]) {
        self.directive = slides
            .iter()
            .map(|s| {
                s.font_size
                    .map_or(0, |n| (n.saturating_sub(1)).saturating_mul(2))
            })
            .collect();
    }

    pub fn offset(&self, slide: usize) -> i8 {
        self.user
            .get(&slide)
            .copied()
            .unwrap_or_else(|| self.directive.get(slide).copied().unwrap_or(0))
    }

    pub fn user_offsets(&self) -> &HashMap<usize, i8> {
        &self.user
    }

    /// Schedules the size for `slide`, or the base size when `None`.
    pub fn request(&mut self, slide: Option<usize>) {
        if !self.available() {
            return;
        }
        let offset = slide.map_or(0, |s| self.offset(s));
        self.pending = Some((self.base + f64::from(offset) * STEP_PT).clamp(MIN_PT, MAX_PT));
    }

    pub fn adjust(&mut self, slide: usize, delta: i8) {
        let next = self.offset(slide).saturating_add(delta).clamp(-20, 40);
        self.user.insert(slide, next);
        self.request(Some(slide));
    }

    /// Drops the user adjustment so the slide's directive applies again.
    pub fn reset(&mut self, slide: usize) {
        self.user.remove(&slide);
        self.request(Some(slide));
    }

    /// The pending size if it differs from what the terminal already has.
    pub fn take_pending(&mut self) -> Option<f64> {
        let size = self.pending.take()?;
        (self.applied != Some(size)).then_some(size)
    }

    /// Sends a size change to the terminal.
    pub fn apply(&mut self, size: f64, out: &mut impl Write) {
        match self.capability {
            FontSizeCapability::KittyRemote => {
                let _ = out.write_all(kitty_escape(size).as_bytes());
                let _ = out.flush();
            }
            FontSizeCapability::GhosttyKeystroke => {
                ghostty_keystrokes((size - self.base).round() as i32)
            }
            FontSizeCapability::None => return,
        }
        self.applied = Some(size);
    }

    /// Puts the terminal back to its configured font size.
    pub fn restore(&mut self, out: &mut impl Write) {
        if self.applied.take().is_none() {
            return;
        }
        match self.capability {
            FontSizeCapability::KittyRemote => {
                let _ = out.write_all(&self.reset_escape());
                let _ = out.flush();
            }
            FontSizeCapability::GhosttyKeystroke => ghostty_keystrokes(0),
            FontSizeCapability::None => {}
        }
    }

    /// Bytes that reset Kitty to its configured size (size 0 means "default"),
    /// safe to emit even if the size never changed.
    pub fn reset_escape(&self) -> Vec<u8> {
        match self.capability {
            FontSizeCapability::KittyRemote => kitty_escape(0.0).into_bytes(),
            _ => Vec::new(),
        }
    }
}

fn kitty_escape(size: f64) -> String {
    format!(
        "\x1bP@kitty-cmd{{\"cmd\":\"set_font_size\",\"version\":[0,14,2],\"no_response\":true,\"payload\":{{\"size\":{size:.1}}}}}\x1b\\"
    )
}

fn kitty_font_size() -> Option<f64> {
    let output = Command::new("kitten")
        .args(["@", "get-font-size"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success());
    if let Some(size) = output.and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
    {
        return Some(size);
    }
    config_value(".config/kitty/kitty.conf", "font_size")
}

fn ghostty_font_size() -> Option<f64> {
    config_value(".config/ghostty/config", "font-size")
}

/// Reads `key value` or `key = value` from a config file under `$HOME`.
fn config_value(rel: &str, key: &str) -> Option<f64> {
    let home = std::env::var_os("HOME")?;
    let text = std::fs::read_to_string(std::path::Path::new(&home).join(rel)).ok()?;
    text.lines().find_map(|line| {
        let rest = line.trim().strip_prefix(key)?;
        let value = rest.trim_start().trim_start_matches('=').trim();
        value.parse().ok().filter(|v: &f64| *v > 0.0)
    })
}

/// Ghostty has no remote control, so reset (Cmd+0) and step with Cmd+= / Cmd+-.
/// Keystrokes go only to Ghostty: nothing is sent if another app is in front.
fn ghostty_keystrokes(steps: i32) {
    let key = if steps >= 0 { "=" } else { "-" };
    let script = format!(
        r#"tell application "System Events"
  if name of first application process whose frontmost is true is not "ghostty" then return
  keystroke "0" using {{command down}}
  repeat {} times
    keystroke "{key}" using {{command down}}
  end repeat
end tell"#,
        steps.unsigned_abs()
    );
    let _ = Command::new("osascript")
        .args(["-e", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font(saved: HashMap<usize, i8>, directives: &[Option<i8>]) -> FontControl {
        let slides: Vec<Slide> = directives
            .iter()
            .map(|&font_size| Slide {
                font_size,
                ..Slide::default()
            })
            .collect();
        let mut f = FontControl {
            capability: FontSizeCapability::KittyRemote,
            base: 12.0,
            applied: None,
            pending: None,
            directive: Vec::new(),
            user: saved,
        };
        f.set_directives(&slides);
        f
    }

    #[test]
    fn user_adjustments_override_directives_until_reset() {
        let mut f = font(HashMap::new(), &[Some(3), None]);
        f.request(Some(0));
        assert_eq!(f.take_pending(), Some(12.0 + 4.0 * STEP_PT));
        f.adjust(0, 1);
        assert_eq!(f.user_offsets().get(&0), Some(&5));
        f.reset(0);
        assert!(f.user_offsets().is_empty());
        assert_eq!(f.take_pending(), Some(12.0 + 4.0 * STEP_PT));
    }

    #[test]
    fn target_size_stays_positive_for_large_negative_offsets() {
        let mut f = font(HashMap::new(), &[Some(-20)]);
        f.request(Some(0));
        assert_eq!(f.take_pending(), Some(MIN_PT));
    }
}
