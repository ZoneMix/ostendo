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
}

#[derive(Debug)]
pub enum RemoteCommand {
    Next,
    Prev,
    Goto(usize),
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
}

impl StateMessage {
    pub fn new(slide: usize, total: usize, title: &str, notes: &str, timer: &str, content: Vec<String>) -> Self {
        Self {
            msg_type: "state".to_string(),
            slide,
            total,
            slide_title: title.to_string(),
            notes: notes.to_string(),
            timer: timer.to_string(),
            slide_content: content,
        }
    }
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
    fn test_state_message_serialization() {
        let msg = StateMessage::new(3, 10, "Test Title", "Some notes", "00:05:30", vec!["Bullet 1".to_string(), "Bullet 2".to_string()]);
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"state\""));
        assert!(json.contains("\"slide\":3"));
        assert!(json.contains("\"total\":10"));
        assert!(json.contains("\"slide_title\":\"Test Title\""));
    }
}
