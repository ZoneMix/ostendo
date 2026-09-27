//! Types shared across the engine submodules.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use crate::presentation::{PresentationMeta, Slide};
use crate::remote::RemoteCommand;
use crate::render::text::StyledLine;
use crate::terminal::protocols::ImageProtocol;
use crate::theme::Theme;

use super::display::ImageKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Normal,
    /// `:` command prompt.
    Command,
    /// `g` + slide number.
    Goto,
    Help,
    Overview,
}

/// Startup options for [`super::Presenter::new`].
pub struct PresenterConfig {
    pub slides: Vec<Slide>,
    pub meta: PresentationMeta,
    pub theme: Theme,
    /// The theme was chosen explicitly (CLI or front matter), so a theme saved
    /// from an earlier session must not replace it.
    pub theme_explicit: bool,
    /// Zero-based start slide; `None` resumes the saved position.
    pub start: Option<usize>,
    pub presentation_path: PathBuf,
    /// Forced image protocol; `None` auto-detects.
    pub image_protocol: Option<ImageProtocol>,
    pub remote: Option<(
        Receiver<RemoteCommand>,
        tokio::sync::broadcast::Sender<String>,
    )>,
    pub allow_exec: bool,
    pub allow_remote_exec: bool,
    pub fullscreen: bool,
    pub timer: bool,
    /// Content width as a percentage of the terminal width.
    pub scale: u8,
}

/// Where things go on screen for the current terminal size and toggles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Layout {
    pub width: usize,
    pub height: usize,
    pub content_top: usize,
    pub content_rows: usize,
    pub content_width: usize,
    pub margin: usize,
}

/// An image positioned relative to a slide frame.
#[derive(Clone)]
pub(crate) struct FrameImage {
    /// First frame line the image covers.
    pub line: usize,
    /// Column offset within the content area.
    pub col: usize,
    pub rows: usize,
    pub kind: ImageKind,
    /// Pinned to the top-right of the content area instead of flowing with text.
    pub pinned_right: bool,
}

/// A slide laid out for one content width: text lines plus image placements.
#[derive(Clone, Default)]
pub(crate) struct SlideFrame {
    pub lines: Vec<StyledLine>,
    pub images: Vec<FrameImage>,
    /// Line ranges of whole elements (title, a list, a table), which
    /// horizontal centering moves as one so their left edges stay aligned.
    pub units: Vec<std::ops::Range<usize>>,
}

impl SlideFrame {
    /// Appends an element's lines as one centering unit.
    pub fn push_unit(&mut self, lines: impl IntoIterator<Item = StyledLine>) {
        let start = self.lines.len();
        self.lines.extend(lines);
        if self.lines.len() > start {
            self.units.push(start..self.lines.len());
        }
    }
}
