//! JSON persistence layer for saving and restoring presentation state across sessions.
//!
//! When a presenter adjusts font sizes, changes the theme, or navigates to a
//! specific slide, those choices are saved to a JSON file so that the next time
//! the same presentation is opened, everything picks up where it left off.
//!
//! State files live alongside the presentation Markdown file and follow the
//! naming convention `.ostendo-state.{stem}.json`, where `{stem}` is the
//! filename without its extension (e.g., `my_talk.md` produces
//! `.ostendo-state.my_talk.json`).
//!
//! The main entry points are:
//! - [`StateManager::load`] — reads (or creates) the state file for a presentation.
//! - [`StateManager::save`] — writes the current in-memory state back to disk.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Serializable snapshot of all per-presentation state that should survive
/// across sessions.
///
/// This struct is serialized to / deserialized from JSON using `serde`.
/// The `#[serde(default)]` attributes ensure that older state files missing
/// newer fields can still be loaded without errors — missing fields simply
/// take their default values.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct PresentationState {
    /// Per-slide scale percentages. Keyed by zero-based slide index.
    /// A value of `100` means no scaling; values below/above zoom out/in.
    pub slide_scales: HashMap<usize, u8>,

    /// The slide index the user was viewing when they last closed the presentation.
    /// Used to resume at the same position on next launch.
    #[serde(default)]
    pub current_slide: usize,

    /// Per-slide font size offsets. Keyed by zero-based slide index.
    /// Positive values increase the font size; negative values decrease it.
    /// Slides with an offset of `0` are removed from the map to keep the
    /// state file clean.
    #[serde(default)]
    pub slide_font_offsets: HashMap<usize, i8>,

    /// The slug of the last-selected theme (e.g., `"dracula"`, `"solarized-light"`).
    /// When present, the renderer loads this theme on startup instead of the default.
    #[serde(default)]
    pub theme_slug: Option<String>,

    /// Global image scale offset from `>` / `<` keys (-90 to +100).
    /// Persisted so the user's preferred image size survives restarts.
    #[serde(default)]
    pub image_scale_offset: i8,
}

/// Manages loading, querying, mutating, and saving [`PresentationState`].
///
/// Each `StateManager` is bound to a specific state file on disk (derived from
/// the presentation file path). All reads and writes go through the in-memory
/// `state` field; call [`save`](StateManager::save) to flush changes to disk.
pub struct StateManager {
    /// Absolute path to the `.ostendo-state.{stem}.json` file.
    path: PathBuf,
    /// The in-memory state. Mutations here are not persisted until `save()` is called.
    state: PresentationState,
}

impl StateManager {
    /// Load (or create) the state for a given presentation file.
    ///
    /// The state file path is derived from `presentation_path` by replacing the
    /// filename with `.ostendo-state.{stem}.json` in the same directory. If the
    /// file exists, it is deserialized; if it is missing or malformed, a fresh
    /// default state is used instead (no error is raised).
    ///
    /// # Parameters
    /// - `presentation_path` — path to the `.md` presentation file. Does not
    ///   need to exist on disk (useful for new presentations).
    pub fn load(presentation_path: &std::path::Path) -> Self {
        // Extract the filename stem (e.g., "my_talk" from "my_talk.md").
        let stem = presentation_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("default");
        let state_file = format!(".ostendo-state.{}.json", stem);
        // Place the state file in the same directory as the presentation.
        let state_path = presentation_path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join(state_file);
        // Attempt to read and parse; fall back to defaults on any failure.
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

    /// Returns the stored scale percentage for the given slide, or `None` if
    /// no custom scale has been set.
    ///
    /// # Parameters
    /// - `slide` — zero-based slide index.
    #[allow(dead_code)]
    pub fn get_scale(&self, slide: usize) -> Option<u8> {
        self.state.slide_scales.get(&slide).copied()
    }

    /// Sets the scale percentage for a specific slide.
    ///
    /// # Parameters
    /// - `slide` — zero-based slide index.
    /// - `scale` — scale percentage (e.g., 80 = 80%, 120 = 120%).
    #[allow(dead_code)]
    pub fn set_scale(&mut self, slide: usize, scale: u8) {
        self.state.slide_scales.insert(slide, scale);
    }

    /// Returns the slide index the user was on when the state was last saved.
    pub fn get_current_slide(&self) -> usize {
        self.state.current_slide
    }

    /// Records the current slide index so it can be restored on next launch.
    ///
    /// # Parameters
    /// - `slide` — zero-based slide index.
    pub fn set_current_slide(&mut self, slide: usize) {
        self.state.current_slide = slide;
    }

    /// Returns the font size offset for the given slide, or `None` if no
    /// custom offset has been set.
    ///
    /// # Parameters
    /// - `slide` — zero-based slide index.
    pub fn get_font_offset(&self, slide: usize) -> Option<i8> {
        self.state.slide_font_offsets.get(&slide).copied()
    }

    /// Sets the font size offset for a specific slide.
    ///
    /// An offset of `0` is treated as "no override" and removes the entry
    /// from the map to keep the persisted state file clean.
    ///
    /// # Parameters
    /// - `slide` — zero-based slide index.
    /// - `offset` — signed offset added to the base font size (-3 to 7).
    pub fn set_font_offset(&mut self, slide: usize, offset: i8) {
        if offset == 0 {
            self.state.slide_font_offsets.remove(&slide);
        } else {
            self.state.slide_font_offsets.insert(slide, offset);
        }
    }

    /// Returns the slug of the persisted theme, or `None` if the user has
    /// never switched themes.
    ///
    /// The return type `Option<&str>` is a borrowed string slice — it
    /// references the data owned by `self` without copying it.
    pub fn get_theme_slug(&self) -> Option<&str> {
        self.state.theme_slug.as_deref()
    }

    /// Stores the given theme slug so it will be restored on next launch.
    ///
    /// # Parameters
    /// - `slug` — the unique identifier for a theme (e.g., `"dracula"`).
    pub fn set_theme_slug(&mut self, slug: &str) {
        self.state.theme_slug = Some(slug.to_string());
    }

    /// Get the saved image scale offset.
    pub fn get_image_scale_offset(&self) -> i8 {
        self.state.image_scale_offset
    }

    /// Save the image scale offset.
    pub fn set_image_scale_offset(&mut self, offset: i8) {
        self.state.image_scale_offset = offset;
    }

    /// Writes the current in-memory state to the JSON file on disk.
    ///
    /// The file is written atomically as pretty-printed JSON for easy manual
    /// inspection. Returns an error if the file cannot be created or written.
    ///
    /// # Errors
    /// Returns `anyhow::Error` if serialization or file I/O fails.
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
