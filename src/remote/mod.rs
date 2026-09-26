//! WebSocket remote control protocol.
//!
//! - **Inbound**: clients send `{"type": "command", "action": "<name>", ...}`,
//!   which [`server`] turns into a [`RemoteCommand`] for the presenter.
//! - **Outbound**: the presenter broadcasts [`StateMessage`] JSON to every
//!   connected client whenever the presentation state changes.

mod html;
pub mod server;

use serde::{Deserialize, Serialize};

/// Wire format of an inbound message; `slide` and `theme` carry the argument
/// of `goto` and `set_theme`.
#[derive(Deserialize)]
struct RemoteCommandMsg {
    #[serde(rename = "type")]
    msg_type: String,
    action: String,
    #[serde(default)]
    slide: Option<usize>,
    #[serde(default)]
    theme: Option<String>,
}

impl RemoteCommandMsg {
    fn into_command(self) -> Option<RemoteCommand> {
        if self.msg_type != "command" {
            return None;
        }
        Some(match self.action.as_str() {
            "next" => RemoteCommand::Next,
            "prev" => RemoteCommand::Prev,
            "goto" => RemoteCommand::Goto(self.slide?),
            "next_section" => RemoteCommand::NextSection,
            "prev_section" => RemoteCommand::PrevSection,
            "scroll_up" => RemoteCommand::ScrollUp,
            "scroll_down" => RemoteCommand::ScrollDown,
            "toggle_fullscreen" => RemoteCommand::ToggleFullscreen,
            "toggle_notes" => RemoteCommand::ToggleNotes,
            "toggle_theme_name" => RemoteCommand::ToggleThemeName,
            "toggle_sections" => RemoteCommand::ToggleSections,
            "toggle_dark_mode" => RemoteCommand::ToggleDarkMode,
            "scale_up" => RemoteCommand::ScaleUp,
            "scale_down" => RemoteCommand::ScaleDown,
            "image_scale_up" => RemoteCommand::ImageScaleUp,
            "image_scale_down" => RemoteCommand::ImageScaleDown,
            "font_up" => RemoteCommand::FontUp,
            "font_down" => RemoteCommand::FontDown,
            "font_reset" => RemoteCommand::FontReset,
            "execute_code" => RemoteCommand::ExecuteCode,
            "timer_start" => RemoteCommand::TimerStart,
            "timer_reset" => RemoteCommand::TimerReset,
            "set_theme" => RemoteCommand::SetTheme(self.theme?),
            _ => return None,
        })
    }
}

/// A command from a remote client, delivered to the presenter's event loop.
#[derive(Debug)]
pub enum RemoteCommand {
    Next,
    Prev,
    /// 1-indexed slide number.
    Goto(usize),
    NextSection,
    PrevSection,
    ScrollUp,
    ScrollDown,
    ToggleFullscreen,
    ToggleNotes,
    ToggleThemeName,
    ToggleSections,
    ToggleDarkMode,
    ScaleUp,
    ScaleDown,
    ImageScaleUp,
    ImageScaleDown,
    FontUp,
    FontDown,
    FontReset,
    /// Honored only when the presenter runs with `--remote-exec`.
    ExecuteCode,
    TimerStart,
    TimerReset,
    /// Theme slug.
    SetTheme(String),
}

/// Presentation state broadcast to remote clients; the field names are the
/// JSON contract with the embedded control page.
#[derive(Debug, Clone, Serialize)]
pub struct StateMessage {
    /// Always `"state"`.
    #[serde(rename = "type")]
    pub msg_type: String,
    /// 1-indexed.
    pub slide: usize,
    pub total: usize,
    pub slide_title: String,
    pub notes: String,
    pub timer: String,
    /// Plain-text lines of the current slide for the preview pane.
    pub slide_content: Vec<String>,
    pub section: String,
    pub is_fullscreen: bool,
    pub is_notes_visible: bool,
    pub is_dark_mode: bool,
    pub show_theme_name: bool,
    pub show_sections: bool,
    pub theme_name: String,
    pub theme_slug: String,
    /// Content scale percentage.
    pub scale: u8,
    /// Offset from each image's own scale.
    pub image_scale: i8,
    /// Offset from the base font size.
    pub font_offset: i8,
    pub has_executable_code: bool,
    pub timer_running: bool,
    pub themes: Vec<String>,
    /// Hex colors (`#rrggbb`) so the page can match the active theme.
    pub theme_bg: String,
    pub theme_accent: String,
    pub theme_text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

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
