//! WebSocket remote control command protocol.
//!
//! Defines the message types for remote control commands (navigation, display
//! toggles, code execution) and state broadcast. The remote control system uses
//! a simple JSON protocol over WebSocket:
//!
//! - **Inbound**: The browser sends `RemoteCommandMsg` JSON objects with a
//!   `"type": "command"` field and an `"action"` string (e.g., `"next"`, `"goto"`).
//! - **Outbound**: The presenter broadcasts `StateMessage` JSON to all connected
//!   clients whenever the presentation state changes (slide number, theme, timer, etc.).
//!
//! This module re-exports the `server` submodule which runs the WebSocket listener,
//! and keeps `html` private since it only contains the embedded remote UI page.

pub mod server;
mod html;

use serde::{Deserialize, Serialize};

/// Raw JSON message received from a remote control client over WebSocket.
///
/// This is the wire format — it arrives as JSON and is deserialized by serde.
/// The `msg_type` field is renamed from `"type"` in JSON because `type` is a
/// reserved keyword in Rust. After deserialization, the `action` string is
/// matched to produce a strongly-typed [`RemoteCommand`] enum variant.
#[derive(Debug, Clone, Deserialize)]
pub struct RemoteCommandMsg {
    /// Message category — currently always `"command"` for inbound messages.
    #[serde(rename = "type")]
    pub msg_type: String,
    /// The action to perform (e.g., `"next"`, `"prev"`, `"goto"`, `"toggle_fullscreen"`).
    pub action: String,
    /// Target slide number for `"goto"` commands. `None` for other actions.
    #[serde(default)]
    pub slide: Option<usize>,
    /// Theme slug for `"set_theme"` commands. `None` for other actions.
    #[serde(default)]
    pub theme: Option<String>,
}

/// Strongly-typed remote control command, converted from [`RemoteCommandMsg`].
///
/// This enum is sent through a standard library `mpsc` channel from the
/// WebSocket handler thread to the main render loop, where each variant
/// triggers the corresponding presenter action.
#[derive(Debug)]
pub enum RemoteCommand {
    // -- Navigation --
    /// Advance to the next slide.
    Next,
    /// Go back to the previous slide.
    Prev,
    /// Jump to a specific slide number (1-indexed).
    Goto(usize),
    /// Jump forward to the next section boundary.
    NextSection,
    /// Jump backward to the previous section boundary.
    PrevSection,
    /// Scroll the current slide content up (for long slides).
    ScrollUp,
    /// Scroll the current slide content down.
    ScrollDown,

    // -- Display toggles --
    /// Toggle fullscreen mode (hides/shows the status bar).
    ToggleFullscreen,
    /// Toggle speaker notes visibility.
    ToggleNotes,
    /// Toggle the theme name display in the status bar.
    ToggleThemeName,
    /// Toggle the section indicator in the status bar.
    ToggleSections,
    /// Switch between light and dark theme variants.
    ToggleDarkMode,

    // -- Scale adjustments --
    /// Increase content scale (zoom in).
    ScaleUp,
    /// Decrease content scale (zoom out).
    ScaleDown,
    /// Increase image scale.
    ImageScaleUp,
    /// Decrease image scale.
    ImageScaleDown,
    /// Increase terminal font size (Kitty only).
    FontUp,
    /// Decrease terminal font size (Kitty only).
    FontDown,
    /// Reset font size to the base level.
    FontReset,

    // -- Actions --
    /// Execute the current slide's code block (requires `--remote-exec` flag).
    ExecuteCode,
    /// Start or pause the presentation timer.
    TimerStart,
    /// Reset the presentation timer to zero.
    TimerReset,

    // -- Theme --
    /// Switch to a different theme by its slug identifier.
    SetTheme(String),
}

/// Presentation state broadcast to all connected remote control clients.
///
/// Every time the presenter state changes (slide navigation, toggle, scale change,
/// etc.), this struct is serialized to JSON and sent to every connected WebSocket
/// client. The remote UI uses these fields to update its display in real time.
#[derive(Debug, Clone, Serialize)]
pub struct StateMessage {
    /// Always `"state"` — lets the client distinguish state updates from other messages.
    #[serde(rename = "type")]
    pub msg_type: String,
    /// Current slide number (1-indexed).
    pub slide: usize,
    /// Total number of slides in the presentation.
    pub total: usize,
    /// Title of the current slide.
    pub slide_title: String,
    /// Speaker notes for the current slide (may be empty).
    pub notes: String,
    /// Formatted timer string (e.g., "00:05:30").
    pub timer: String,
    /// Plain-text content lines of the current slide (for preview).
    pub slide_content: Vec<String>,
    /// Current section name (empty if no sections defined).
    pub section: String,
    /// Whether fullscreen mode is active.
    pub is_fullscreen: bool,
    /// Whether speaker notes are visible on the terminal.
    pub is_notes_visible: bool,
    /// Whether the dark theme variant is active.
    pub is_dark_mode: bool,
    /// Whether the theme name is shown in the status bar.
    pub show_theme_name: bool,
    /// Whether the section indicator is shown in the status bar.
    pub show_sections: bool,
    /// Human-readable theme name (e.g., "Dracula").
    pub theme_name: String,
    /// Theme identifier slug (e.g., "dracula").
    pub theme_slug: String,
    /// Content scale percentage (50-200).
    pub scale: u8,
    /// Image scale offset from default.
    pub image_scale: i8,
    /// Font size offset from base.
    pub font_offset: i8,
    /// Whether the current slide has an executable code block.
    pub has_executable_code: bool,
    /// Whether the presentation timer is currently running.
    pub timer_running: bool,
    /// List of all available theme slugs (for the theme selector dropdown).
    pub themes: Vec<String>,
    /// Current theme background color as a hex string (e.g., "#282a36").
    pub theme_bg: String,
    /// Current theme accent color as a hex string.
    pub theme_accent: String,
    /// Current theme text color as a hex string.
    pub theme_text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remote_command_deserialization() {
        let json = r#"{"type":"command","action":"next"}"#;
        let msg: RemoteCommandMsg = serde_json::from_str(json).unwrap();
        assert_eq!(msg.action, "next");
        assert_eq!(msg.msg_type, "command");
    }

    #[test]
    fn test_remote_command_goto() {
        let json = r#"{"type":"command","action":"goto","slide":5}"#;
        let msg: RemoteCommandMsg = serde_json::from_str(json).unwrap();
        assert_eq!(msg.action, "goto");
        assert_eq!(msg.slide, Some(5));
    }

    #[test]
    fn test_remote_command_toggle() {
        let json = r#"{"type":"command","action":"toggle_fullscreen"}"#;
        let msg: RemoteCommandMsg = serde_json::from_str(json).unwrap();
        assert_eq!(msg.action, "toggle_fullscreen");
        assert_eq!(msg.msg_type, "command");
    }

    #[test]
    fn test_state_message_serialization() {
        let msg = StateMessage {
            msg_type: "state".to_string(),
            slide: 3,
            total: 10,
            slide_title: "Test Title".to_string(),
            notes: "Some notes".to_string(),
            timer: "00:05:30".to_string(),
            slide_content: vec!["Bullet 1".to_string(), "Bullet 2".to_string()],
            section: "intro".to_string(),
            is_fullscreen: false,
            is_notes_visible: true,
            is_dark_mode: true,
            show_theme_name: false,
            show_sections: true,
            theme_name: "Dracula".to_string(),
            theme_slug: "dracula".to_string(),
            scale: 100,
            image_scale: 0,
            font_offset: 0,
            has_executable_code: false,
            timer_running: true,
            themes: vec!["dracula".to_string(), "nord".to_string()],
            theme_bg: "#282a36".to_string(),
            theme_accent: "#bd93f9".to_string(),
            theme_text: "#f8f8f2".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"state\""));
        assert!(json.contains("\"slide\":3"));
        assert!(json.contains("\"total\":10"));
        assert!(json.contains("\"slide_title\":\"Test Title\""));
        assert!(json.contains("\"section\":\"intro\""));
        assert!(json.contains("\"is_dark_mode\":true"));
        assert!(json.contains("\"theme_name\":\"Dracula\""));
        assert!(json.contains("\"theme_slug\":\"dracula\""));
        assert!(json.contains("\"timer_running\":true"));
    }
}
