//! Terminal capability detection.
//!
//! Probes the terminal environment (via environment variables) to determine
//! which image protocol and font sizing capability are available.  This module
//! runs once at startup and the results are stored for the lifetime of the
//! presentation.
//!
//! # Detection strategy
//!
//! The detection is entirely based on environment variables -- no escape-sequence
//! probing is performed (which would require reading terminal responses and can
//! be unreliable inside tmux).  The heuristics are:
//!
//! | Variable              | Indicates                                  |
//! |-----------------------|--------------------------------------------|
//! | `KITTY_WINDOW_ID`     | Kitty terminal (graphics + font control)   |
//! | `TERM_PROGRAM=iTerm.app` / `LC_TERMINAL=iTerm2` | iTerm2        |
//! | `TERM_PROGRAM=WezTerm` | WezTerm (uses iTerm2 image protocol)      |
//! | `TERM_PROGRAM=ghostty` | Ghostty (uses Kitty graphics protocol)    |
//! | `TMUX`                | Running inside tmux (affects font control)  |
//!
//! # Capabilities detected
//!
//! - [`ImageProtocol`] -- which image display protocol to use
//! - [`FontSizeCapability`] -- whether per-slide font sizing is possible
//! - [`TextScaleCapability`] -- whether OSC 66 per-element text scaling works

use std::env;

/// Which image display protocol the terminal supports.
///
/// Detected once at startup by [`detect_protocol`] and used throughout
/// the image rendering pipeline to choose the correct escape sequences.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImageProtocol {
    /// Kitty Graphics Protocol -- highest quality, supports Kitty and Ghostty.
    Kitty,
    /// iTerm2 Inline Images -- widely supported (iTerm2, WezTerm, many others).
    Iterm2,
    /// Sixel -- legacy VT340 bitmap format, works in xterm and mlterm.
    Sixel,
    /// ASCII art fallback -- works everywhere but at low resolution.
    Ascii,
}

/// Whether (and how) the terminal supports changing the font size at runtime.
///
/// Font size changes are used for the `<!-- font_size: N -->` slide directive,
/// which lets presenters enlarge text for emphasis or shrink it to fit more
/// content on screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FontSizeCapability {
    /// Kitty remote control protocol (DCS-based).
    /// Requires `allow_remote_control yes` in `kitty.conf`.
    KittyRemote,
    /// Ghostty keystroke simulation via macOS AppleScript.
    /// Requires Accessibility permission in System Settings.
    /// Only available on macOS because it uses `osascript`.
    GhosttyKeystroke,
    /// No font size control available -- font directives are silently ignored.
    None,
}

impl FontSizeCapability {
    /// Returns true if this capability supports any form of font size control.
    pub fn is_available(&self) -> bool {
        !matches!(self, FontSizeCapability::None)
    }
}

/// Detect whether the current terminal supports runtime font size changes.
///
/// Returns [`FontSizeCapability::None`] inside tmux because font control
/// escape sequences and keystroke simulation target the wrong process when
/// multiplexed.
pub fn detect_font_capability() -> FontSizeCapability {
    // Font control doesn't work reliably through tmux — env vars become stale
    // and keystroke simulation targets the wrong pane.
    if env::var("TMUX").is_ok() {
        return FontSizeCapability::None;
    }
    if env::var("KITTY_WINDOW_ID").is_ok() {
        return FontSizeCapability::KittyRemote;
    }
    // Ghostty sets TERM_PROGRAM=ghostty when running directly
    if env::var("TERM_PROGRAM").unwrap_or_default().to_lowercase() == "ghostty" {
        // Only available on macOS (uses AppleScript for keystroke simulation)
        if cfg!(target_os = "macos") {
            return FontSizeCapability::GhosttyKeystroke;
        }
    }
    FontSizeCapability::None
}

/// Whether the terminal supports OSC 66 per-element text scaling.
///
/// OSC 66 allows individual text spans to be rendered at 2x-7x their normal
/// size, which is used for large slide titles without needing FIGlet ASCII art.
/// Currently only Kitty implements this protocol extension.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextScaleCapability {
    /// Kitty OSC 66 text sizing protocol -- scales individual text runs.
    Osc66,
    /// No per-element scaling available.
    None,
}

/// Detect whether the terminal supports OSC 66 per-element text scaling.
///
/// Only returns [`TextScaleCapability::Osc66`] when running directly in Kitty
/// (not through tmux, where passthrough support is untested).
pub fn detect_text_scale_capability() -> TextScaleCapability {
    // Only Kitty supports OSC 66 text sizing; tmux passthrough not yet tested
    if env::var("TMUX").is_ok() {
        return TextScaleCapability::None;
    }
    if env::var("KITTY_WINDOW_ID").is_ok() {
        return TextScaleCapability::Osc66;
    }
    TextScaleCapability::None
}

/// Whether Kitty supports native animation frames (a=f).
///
/// Ghostty uses the Kitty graphics protocol for static images but does NOT
/// support the animation extension. When animation is not available, Ostendo
/// falls back to app-driven frame advance (Phase 1 placement commands).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KittyAnimationCapability {
    /// Real Kitty terminal — supports a=f, a=a for native animation.
    Supported,
    /// Ghostty, tmux, or non-Kitty — no animation frame support.
    None,
}

/// Detect whether the terminal supports Kitty native animation.
///
/// Only real Kitty (not Ghostty, not tmux) supports animation frames.
pub fn detect_kitty_animation() -> KittyAnimationCapability {
    // Ghostty uses Kitty graphics but does NOT support animation frames
    let term_program = env::var("TERM_PROGRAM").unwrap_or_default().to_lowercase();
    if term_program == "ghostty" {
        return KittyAnimationCapability::None;
    }
    // Only enable for real Kitty outside tmux
    if env::var("KITTY_WINDOW_ID").is_ok() && env::var("TMUX").is_err() {
        return KittyAnimationCapability::Supported;
    }
    KittyAnimationCapability::None
}

/// Detect which image display protocol the current terminal supports.
///
/// Checks environment variables in priority order and returns the best
/// available protocol.  Falls back to [`ImageProtocol::Iterm2`] because
/// the iTerm2 inline image protocol is the most widely supported among
/// modern terminal emulators.
///
/// # tmux caveat
///
/// Inside tmux, `KITTY_WINDOW_ID` can be *stale* (inherited from a previous
/// Kitty session even though the terminal is now iTerm2).  To avoid misdetection,
/// Kitty is only selected outside tmux.  iTerm2 detection uses `LC_TERMINAL`
/// which tmux preserves correctly.
pub fn detect_protocol() -> ImageProtocol {
    let term_program = env::var("TERM_PROGRAM").unwrap_or_default();
    let lc_terminal = env::var("LC_TERMINAL").unwrap_or_default();
    let in_tmux = env::var("TMUX").is_ok();

    // iTerm2 detection — LC_TERMINAL persists through tmux, TERM_PROGRAM doesn't
    if term_program == "iTerm.app"
        || lc_terminal == "iTerm2"
        || env::var("ITERM_SESSION_ID").is_ok()
    {
        return ImageProtocol::Iterm2;
    }

    // WezTerm supports iTerm2 image protocol
    if term_program == "WezTerm" {
        return ImageProtocol::Iterm2;
    }

    // Ghostty supports Kitty graphics protocol natively
    if term_program.to_lowercase() == "ghostty" {
        return ImageProtocol::Kitty;
    }

    // Kitty detection — only trust KITTY_WINDOW_ID when NOT in tmux.
    // Inside tmux, KITTY_WINDOW_ID can be stale (inherited from a previous
    // Kitty session but now running in iTerm2/another terminal).
    if !in_tmux {
        let term = env::var("TERM").unwrap_or_default();
        if term.contains("kitty") || env::var("KITTY_WINDOW_ID").is_ok() {
            return ImageProtocol::Kitty;
        }
    }

    // Sixel detection (some terminals set SIXEL capability)
    // Most modern terminals with sixel support also support other protocols,
    // so this is a lower priority fallback.

    // Default to iTerm2 protocol — widely supported by modern terminals
    // (iTerm2, WezTerm, Ghostty, etc.) and degrades gracefully
    ImageProtocol::Iterm2
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to remove env vars and restore them using a simple RAII guard.
    // We use serial tests (one-by-one) so env mutations are safe.
    struct EnvGuard {
        key: &'static str,
        original: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, val: &str) -> Self {
            let original = std::env::var(key).ok();
            std::env::set_var(key, val);
            Self { key, original }
        }

        fn remove(key: &'static str) -> Self {
            let original = std::env::var(key).ok();
            std::env::remove_var(key);
            Self { key, original }
        }
    }

    // Mutex to serialize env-dependent tests (env vars are process-global)
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.original {
                Some(v) => std::env::set_var(self.key, v),
                None => std::env::remove_var(self.key),
            }
        }
    }

    // --- FontSizeCapability::is_available ---

    #[test]
    fn kitty_remote_is_available() {
        assert!(FontSizeCapability::KittyRemote.is_available());
    }

    #[test]
    fn ghostty_keystroke_is_available() {
        assert!(FontSizeCapability::GhosttyKeystroke.is_available());
    }

    #[test]
    fn none_font_capability_is_not_available() {
        assert!(!FontSizeCapability::None.is_available());
    }

    // --- detect_font_capability ---

    #[test]
    fn font_capability_is_none_inside_tmux() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _tmux = EnvGuard::set("TMUX", "/tmp/tmux-test,1234,0");
        let _kitty = EnvGuard::remove("KITTY_WINDOW_ID");
        let _term = EnvGuard::remove("TERM_PROGRAM");
        assert_eq!(detect_font_capability(), FontSizeCapability::None);
    }

    #[test]
    fn font_capability_is_kitty_when_kitty_id_set_outside_tmux() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _tmux = EnvGuard::remove("TMUX");
        let _kitty = EnvGuard::set("KITTY_WINDOW_ID", "1");
        assert_eq!(detect_font_capability(), FontSizeCapability::KittyRemote);
    }

    #[test]
    fn font_capability_is_none_without_kitty_or_ghostty() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _tmux = EnvGuard::remove("TMUX");
        let _kitty = EnvGuard::remove("KITTY_WINDOW_ID");
        let _term = EnvGuard::remove("TERM_PROGRAM");
        assert_eq!(detect_font_capability(), FontSizeCapability::None);
    }

    // --- detect_text_scale_capability ---

    #[test]
    fn text_scale_is_none_inside_tmux() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _tmux = EnvGuard::set("TMUX", "/tmp/tmux-test,1234,0");
        let _kitty = EnvGuard::remove("KITTY_WINDOW_ID");
        assert_eq!(detect_text_scale_capability(), TextScaleCapability::None);
    }

    #[test]
    fn text_scale_is_osc66_when_kitty_id_set_outside_tmux() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _tmux = EnvGuard::remove("TMUX");
        let _kitty = EnvGuard::set("KITTY_WINDOW_ID", "42");
        assert_eq!(detect_text_scale_capability(), TextScaleCapability::Osc66);
    }

    #[test]
    fn text_scale_is_none_without_kitty() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _tmux = EnvGuard::remove("TMUX");
        let _kitty = EnvGuard::remove("KITTY_WINDOW_ID");
        assert_eq!(detect_text_scale_capability(), TextScaleCapability::None);
    }

    // --- detect_kitty_animation ---

    #[test]
    fn kitty_animation_is_none_for_ghostty() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _term = EnvGuard::set("TERM_PROGRAM", "ghostty");
        let _kitty = EnvGuard::set("KITTY_WINDOW_ID", "1");
        let _tmux = EnvGuard::remove("TMUX");
        assert_eq!(detect_kitty_animation(), KittyAnimationCapability::None);
    }

    #[test]
    fn kitty_animation_is_supported_for_real_kitty_outside_tmux() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _term = EnvGuard::remove("TERM_PROGRAM");
        let _kitty = EnvGuard::set("KITTY_WINDOW_ID", "1");
        let _tmux = EnvGuard::remove("TMUX");
        assert_eq!(detect_kitty_animation(), KittyAnimationCapability::Supported);
    }

    #[test]
    fn kitty_animation_is_none_inside_tmux() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _term = EnvGuard::remove("TERM_PROGRAM");
        let _kitty = EnvGuard::set("KITTY_WINDOW_ID", "1");
        let _tmux = EnvGuard::set("TMUX", "/tmp/tmux-1,100,0");
        assert_eq!(detect_kitty_animation(), KittyAnimationCapability::None);
    }

    // --- detect_protocol ---

    #[test]
    fn protocol_is_iterm2_for_iterm_app_term_program() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _tp = EnvGuard::set("TERM_PROGRAM", "iTerm.app");
        let _lc = EnvGuard::remove("LC_TERMINAL");
        let _is = EnvGuard::remove("ITERM_SESSION_ID");
        let _tmux = EnvGuard::remove("TMUX");
        assert_eq!(detect_protocol(), ImageProtocol::Iterm2);
    }

    #[test]
    fn protocol_is_iterm2_for_lc_terminal_iterm2() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _tp = EnvGuard::remove("TERM_PROGRAM");
        let _lc = EnvGuard::set("LC_TERMINAL", "iTerm2");
        let _is = EnvGuard::remove("ITERM_SESSION_ID");
        let _tmux = EnvGuard::remove("TMUX");
        assert_eq!(detect_protocol(), ImageProtocol::Iterm2);
    }

    #[test]
    fn protocol_is_iterm2_for_wezterm() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _tp = EnvGuard::set("TERM_PROGRAM", "WezTerm");
        let _lc = EnvGuard::remove("LC_TERMINAL");
        let _is = EnvGuard::remove("ITERM_SESSION_ID");
        let _tmux = EnvGuard::remove("TMUX");
        assert_eq!(detect_protocol(), ImageProtocol::Iterm2);
    }

    #[test]
    fn protocol_is_kitty_for_ghostty() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _tp = EnvGuard::set("TERM_PROGRAM", "ghostty");
        let _lc = EnvGuard::remove("LC_TERMINAL");
        let _is = EnvGuard::remove("ITERM_SESSION_ID");
        let _tmux = EnvGuard::remove("TMUX");
        assert_eq!(detect_protocol(), ImageProtocol::Kitty);
    }

    #[test]
    fn protocol_is_kitty_when_kitty_window_id_set_outside_tmux() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _tp = EnvGuard::remove("TERM_PROGRAM");
        let _lc = EnvGuard::remove("LC_TERMINAL");
        let _is = EnvGuard::remove("ITERM_SESSION_ID");
        let _tmux = EnvGuard::remove("TMUX");
        let _kw = EnvGuard::set("KITTY_WINDOW_ID", "5");
        let _term = EnvGuard::remove("TERM");
        assert_eq!(detect_protocol(), ImageProtocol::Kitty);
    }

    #[test]
    fn protocol_defaults_to_iterm2_when_no_env_vars_set() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _tp = EnvGuard::remove("TERM_PROGRAM");
        let _lc = EnvGuard::remove("LC_TERMINAL");
        let _is = EnvGuard::remove("ITERM_SESSION_ID");
        let _tmux = EnvGuard::remove("TMUX");
        let _kw = EnvGuard::remove("KITTY_WINDOW_ID");
        let _term = EnvGuard::remove("TERM");
        assert_eq!(detect_protocol(), ImageProtocol::Iterm2);
    }
}
