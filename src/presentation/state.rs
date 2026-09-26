//! Per-presentation state persisted between sessions in `.ostendo-state.{stem}.json` next to
//! the presentation file.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Missing fields fall back to their defaults so older or partial files keep what they have.
#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(default)]
struct PresentationState {
    current_slide: usize,
    /// Font size offsets the user applied, keyed by 0-based slide index.
    slide_font_offsets: HashMap<usize, i8>,
    theme_slug: Option<String>,
    image_scale_offset: i8,
}

pub struct StateManager {
    path: PathBuf,
    state: PresentationState,
}

impl StateManager {
    /// Loads the state file for `presentation_path`; a missing or unreadable file yields defaults.
    pub fn load(presentation_path: &Path) -> Self {
        let stem = presentation_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("default");
        let path = presentation_path
            .parent()
            .unwrap_or(Path::new("."))
            .join(format!(".ostendo-state.{stem}.json"));
        let state = std::fs::read_to_string(&path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default();
        Self { path, state }
    }

    pub fn get_current_slide(&self) -> usize {
        self.state.current_slide
    }

    pub fn set_current_slide(&mut self, slide: usize) {
        self.state.current_slide = slide;
    }

    pub fn font_offsets(&self) -> HashMap<usize, i8> {
        self.state.slide_font_offsets.clone()
    }

    pub fn set_font_offsets(&mut self, offsets: &HashMap<usize, i8>) {
        self.state.slide_font_offsets.clone_from(offsets);
    }

    pub fn get_theme_slug(&self) -> Option<&str> {
        self.state.theme_slug.as_deref()
    }

    pub fn set_theme_slug(&mut self, slug: &str) {
        self.state.theme_slug = Some(slug.to_string());
    }

    pub fn get_image_scale_offset(&self) -> i8 {
        self.state.image_scale_offset
    }

    pub fn set_image_scale_offset(&mut self, offset: i8) {
        self.state.image_scale_offset = offset;
    }

    /// Writes a temporary file in the same directory and renames it over the state file, so
    /// a crash mid-write cannot leave a truncated file behind.
    pub fn save(&self) -> Result<()> {
        let dir = match self.path.parent() {
            Some(dir) if !dir.as_os_str().is_empty() => dir,
            _ => Path::new("."),
        };
        let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
        tmp.write_all(serde_json::to_string_pretty(&self.state)?.as_bytes())?;
        tmp.persist(&self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_then_load_round_trips_persisted_fields() {
        let dir = tempfile::TempDir::new().unwrap();
        let deck = dir.path().join("talk.md");
        let offsets = HashMap::from([(0, 2), (3, -1)]);
        let mut state = StateManager::load(&deck);
        state.set_current_slide(4);
        state.set_font_offsets(&offsets);
        state.set_theme_slug("nord");
        state.set_image_scale_offset(-20);
        state.save().unwrap();

        let loaded = StateManager::load(&deck);
        assert!(dir.path().join(".ostendo-state.talk.json").exists());
        assert_eq!(loaded.get_current_slide(), 4);
        assert_eq!(loaded.font_offsets(), offsets);
        assert_eq!(loaded.get_theme_slug(), Some("nord"));
        assert_eq!(loaded.get_image_scale_offset(), -20);
    }

    #[test]
    fn partial_or_malformed_files_fall_back_per_field() {
        let dir = tempfile::TempDir::new().unwrap();
        let deck = dir.path().join("talk.md");
        let state_file = dir.path().join(".ostendo-state.talk.json");

        for (content, slide) in [
            (r#"{"current_slide": 7}"#, 7),
            (r#"{"current_slide": 2, "slide_scales": {"0": 80}}"#, 2),
            ("{not json", 0),
        ] {
            std::fs::write(&state_file, content).unwrap();
            assert_eq!(
                StateManager::load(&deck).get_current_slide(),
                slide,
                "{content}"
            );
        }
    }
}
