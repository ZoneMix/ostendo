use std::env;

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum ImageProtocol {
    Kitty,
    Iterm2,
    Sixel,
    Ascii,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FontSizeCapability {
    /// Kitty remote control protocol (DCS-based, requires allow_remote_control in kitty.conf)
    KittyRemote,
    /// Ghostty keystroke simulation via macOS AppleScript (requires Accessibility permission)
    GhosttyKeystroke,
    None,
}

impl FontSizeCapability {
    /// Returns true if this capability supports any form of font size control.
    pub fn is_available(&self) -> bool {
        !matches!(self, FontSizeCapability::None)
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextScaleCapability {
    /// Kitty OSC 66 text sizing protocol
    Osc66,
    None,
}

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
