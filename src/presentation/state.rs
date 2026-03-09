use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct PresentationState {
    pub slide_scales: HashMap<usize, u8>,
    #[serde(default)]
    pub current_slide: usize,
    #[serde(default)]
    pub slide_font_offsets: HashMap<usize, i8>,
}

pub struct StateManager {
    path: PathBuf,
    state: PresentationState,
}

impl StateManager {
    pub fn load(presentation_path: &std::path::Path) -> Self {
        let stem = presentation_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("default");
        let state_file = format!(".ostendo-state.{}.json", stem);
        let state_path = presentation_path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join(state_file);
        let state = if state_path.exists() {
            let content = std::fs::read_to_string(&state_path).unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            PresentationState::default()
        };
        Self {
            path: state_path,
            state,
        }
    }

    #[allow(dead_code)]
    pub fn get_scale(&self, slide: usize) -> Option<u8> {
        self.state.slide_scales.get(&slide).copied()
    }

    #[allow(dead_code)]
    pub fn set_scale(&mut self, slide: usize, scale: u8) {
        self.state.slide_scales.insert(slide, scale);
    }

    pub fn get_current_slide(&self) -> usize {
        self.state.current_slide
    }

    pub fn set_current_slide(&mut self, slide: usize) {
        self.state.current_slide = slide;
    }

    pub fn get_font_offset(&self, slide: usize) -> Option<i8> {
        self.state.slide_font_offsets.get(&slide).copied()
    }

    pub fn set_font_offset(&mut self, slide: usize, offset: i8) {
        if offset == 0 {
            self.state.slide_font_offsets.remove(&slide);
        } else {
            self.state.slide_font_offsets.insert(slide, offset);
        }
    }

    pub fn save(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(&self.state)?;
        std::fs::write(&self.path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_file_returns_default() {
        let tmp = tempfile::TempDir::new().unwrap();
        let fake_pres = tmp.path().join("nonexistent.md");
        let mgr = StateManager::load(&fake_pres);
        assert!(mgr.get_scale(0).is_none());
    }

    #[test]
    fn test_set_and_get_scale() {
        let tmp = tempfile::TempDir::new().unwrap();
        let fake_pres = tmp.path().join("test.md");
        let mut mgr = StateManager::load(&fake_pres);
        mgr.set_scale(0, 80);
        mgr.set_scale(5, 120);
        assert_eq!(mgr.get_scale(0), Some(80));
        assert_eq!(mgr.get_scale(5), Some(120));
        assert_eq!(mgr.get_scale(1), None);
    }

    #[test]
    fn test_roundtrip_save_load() {
        let tmp = tempfile::TempDir::new().unwrap();
        let fake_pres = tmp.path().join("test.md");

        {
            let mut mgr = StateManager::load(&fake_pres);
            mgr.set_scale(0, 75);
            mgr.set_scale(3, 150);
            mgr.save().unwrap();
        }

        {
            let mgr = StateManager::load(&fake_pres);
            assert_eq!(mgr.get_scale(0), Some(75));
            assert_eq!(mgr.get_scale(3), Some(150));
            assert_eq!(mgr.get_scale(1), None);
        }
    }

    #[test]
    fn test_overwrite_scale() {
        let tmp = tempfile::TempDir::new().unwrap();
        let fake_pres = tmp.path().join("test.md");
        let mut mgr = StateManager::load(&fake_pres);
        mgr.set_scale(0, 80);
        mgr.set_scale(0, 120);
        assert_eq!(mgr.get_scale(0), Some(120));
    }
}
