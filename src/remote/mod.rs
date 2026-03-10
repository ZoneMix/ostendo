pub mod server;
mod html;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteCommandMsg {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub action: String,
    #[serde(default)]
    pub slide: Option<usize>,
    #[serde(default)]
    pub theme: Option<String>,
}

#[derive(Debug)]
pub enum RemoteCommand {
    // Navigation
    Next,
    Prev,
    Goto(usize),
    NextSection,
    PrevSection,
    ScrollUp,
    ScrollDown,
    // Display toggles
    ToggleFullscreen,
    ToggleNotes,
    ToggleThemeName,
    ToggleSections,
    ToggleDarkMode,
    // Scale
    ScaleUp,
    ScaleDown,
    ImageScaleUp,
    ImageScaleDown,
    FontUp,
    FontDown,
    FontReset,
    // Actions
    ExecuteCode,
    TimerStart,
    TimerReset,
    // Theme
    SetTheme(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct StateMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub slide: usize,
    pub total: usize,
    pub slide_title: String,
    pub notes: String,
    pub timer: String,
    pub slide_content: Vec<String>,
    pub section: String,
    pub is_fullscreen: bool,
    pub is_notes_visible: bool,
    pub is_dark_mode: bool,
    pub show_theme_name: bool,
    pub show_sections: bool,
    pub theme_name: String,
    pub theme_slug: String,
    pub scale: u8,
    pub image_scale: i8,
    pub font_offset: i8,
    pub has_executable_code: bool,
    pub timer_running: bool,
    pub themes: Vec<String>,
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
