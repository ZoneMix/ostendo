//! Terminal capability detection from environment variables, run once at
//! startup. No escape-sequence probing: reading terminal replies is
//! unreliable, especially inside tmux.
//!
//! | Variable                                        | Indicates                         |
//! |-------------------------------------------------|-----------------------------------|
//! | `KITTY_WINDOW_ID`, `TERM=*kitty*`               | Kitty (graphics + font control)   |
//! | `TERM_PROGRAM=ghostty`                          | Ghostty (Kitty graphics)          |
//! | `TERM_PROGRAM=iTerm.app`, `LC_TERMINAL=iTerm2`  | iTerm2 inline images              |
//! | `TERM_PROGRAM=WezTerm`                          | WezTerm (iTerm2 inline images)    |
//! | `TMUX`                                          | tmux (Kitty vars may be stale)    |
//!
//! Any other terminal gets colored half blocks, which render everywhere; Sixel is
//! never auto-detected and must be chosen with `--image-mode sixel`.

use std::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageProtocol {
    Kitty,
    Iterm2,
    Sixel,
    /// Colored half-block cells: works in any true-color terminal.
    Blocks,
    /// Character-ramp art (`image_render: ascii`).
    Ascii,
}

/// Runtime font size control for the `font_size` directive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FontSizeCapability {
    /// Kitty remote control; needs `allow_remote_control yes` in `kitty.conf`.
    KittyRemote,
    /// Ghostty via AppleScript keystrokes (macOS only); needs the
    /// Accessibility permission.
    GhosttyKeystroke,
    /// Font directives are ignored.
    None,
}

impl FontSizeCapability {
    pub fn is_available(&self) -> bool {
        !matches!(self, FontSizeCapability::None)
    }
}

/// Never available inside tmux: Kitty variables may be stale there and
/// simulated keystrokes would reach the wrong pane.
pub fn detect_font_capability() -> FontSizeCapability {
    if env::var("TMUX").is_ok() {
        return FontSizeCapability::None;
    }
    if env::var("KITTY_WINDOW_ID").is_ok() {
        return FontSizeCapability::KittyRemote;
    }
    if cfg!(target_os = "macos")
        && env::var("TERM_PROGRAM").is_ok_and(|p| p.eq_ignore_ascii_case("ghostty"))
    {
        return FontSizeCapability::GhosttyKeystroke;
    }
    FontSizeCapability::None
}

/// OSC 66 renders individual text runs at 2x-7x, giving large titles without
/// FIGlet. Only Kitty implements it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextScaleCapability {
    Osc66,
    None,
}

/// Kitty only, and not through tmux, where passthrough is untested.
pub fn detect_text_scale_capability() -> TextScaleCapability {
    if env::var("TMUX").is_err() && env::var("KITTY_WINDOW_ID").is_ok() {
        TextScaleCapability::Osc66
    } else {
        TextScaleCapability::None
    }
}

/// Pick the image protocol for the current terminal; see the module table.
pub fn detect_protocol() -> ImageProtocol {
    let term_program = env::var("TERM_PROGRAM").unwrap_or_default();

    // LC_TERMINAL survives into tmux; TERM_PROGRAM is overwritten there.
    if term_program == "iTerm.app"
        || env::var("LC_TERMINAL").is_ok_and(|v| v == "iTerm2")
        || env::var("ITERM_SESSION_ID").is_ok()
        || term_program == "WezTerm"
    {
        return ImageProtocol::Iterm2;
    }

    if term_program.eq_ignore_ascii_case("ghostty") {
        return ImageProtocol::Kitty;
    }

    // Inside tmux, KITTY_WINDOW_ID may be inherited from an earlier Kitty
    // session while the attached terminal is something else.
    if env::var("TMUX").is_err()
        && (env::var("TERM").is_ok_and(|t| t.contains("kitty"))
            || env::var("KITTY_WINDOW_ID").is_ok())
    {
        return ImageProtocol::Kitty;
    }

    // Unknown terminals (Alacritty, VTE, Windows Terminal, ...) would show
    // nothing for a protocol they lack; half blocks render everywhere.
    ImageProtocol::Blocks
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::sync::Mutex;

    const DETECTION_VARS: [&str; 6] = [
        "TERM_PROGRAM",
        "LC_TERMINAL",
        "ITERM_SESSION_ID",
        "TMUX",
        "KITTY_WINDOW_ID",
        "TERM",
    ];

    /// Environment variables are process-global; every test that touches
    /// them holds this lock.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct Restore(Vec<(&'static str, Option<OsString>)>);

    impl Drop for Restore {
        fn drop(&mut self) {
            for (key, value) in &self.0 {
                match value {
                    Some(value) => env::set_var(key, value),
                    None => env::remove_var(key),
                }
            }
        }
    }

    /// Run `detect` with exactly `vars` set among the detection variables, so
    /// the host terminal cannot influence the result.
    fn with_env<T>(vars: &[(&'static str, &str)], detect: fn() -> T) -> T {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _restore = Restore(DETECTION_VARS.map(|k| (k, env::var_os(k))).to_vec());
        for key in DETECTION_VARS {
            env::remove_var(key);
        }
        for (key, value) in vars {
            env::set_var(key, value);
        }
        detect()
    }

    #[test]
    fn detects_image_protocol() {
        let tmux = ("TMUX", "/tmp/tmux-1000/default,1234,0");
        let cases: &[(&[(&str, &str)], ImageProtocol)] = &[
            (&[("TERM_PROGRAM", "iTerm.app")], ImageProtocol::Iterm2),
            (&[("LC_TERMINAL", "iTerm2"), tmux], ImageProtocol::Iterm2),
            (&[("TERM_PROGRAM", "WezTerm")], ImageProtocol::Iterm2),
            (&[("TERM_PROGRAM", "ghostty")], ImageProtocol::Kitty),
            (&[("KITTY_WINDOW_ID", "5")], ImageProtocol::Kitty),
            (&[("TERM", "xterm-kitty")], ImageProtocol::Kitty),
            (&[("KITTY_WINDOW_ID", "5"), tmux], ImageProtocol::Blocks),
            (&[("TERM", "alacritty")], ImageProtocol::Blocks),
            (&[("TERM", "xterm-256color")], ImageProtocol::Blocks),
            (&[], ImageProtocol::Blocks),
        ];
        for (vars, expected) in cases {
            assert_eq!(with_env(vars, detect_protocol), *expected, "{vars:?}");
        }
    }

    #[test]
    fn detects_font_capability() {
        let cases: &[(&[(&str, &str)], FontSizeCapability)] = &[
            (&[("KITTY_WINDOW_ID", "1")], FontSizeCapability::KittyRemote),
            (
                &[("KITTY_WINDOW_ID", "1"), ("TMUX", "t")],
                FontSizeCapability::None,
            ),
            (&[], FontSizeCapability::None),
        ];
        for (vars, expected) in cases {
            assert_eq!(
                with_env(vars, detect_font_capability),
                *expected,
                "{vars:?}"
            );
        }
    }

    #[test]
    fn detects_text_scale_capability() {
        let cases: &[(&[(&str, &str)], TextScaleCapability)] = &[
            (&[("KITTY_WINDOW_ID", "1")], TextScaleCapability::Osc66),
            (
                &[("KITTY_WINDOW_ID", "1"), ("TMUX", "t")],
                TextScaleCapability::None,
            ),
            (&[], TextScaleCapability::None),
        ];
        for (vars, expected) in cases {
            assert_eq!(
                with_env(vars, detect_text_scale_capability),
                *expected,
                "{vars:?}"
            );
        }
    }
}
