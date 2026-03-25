//! Type definitions for the presentation engine.
//!
//! This module contains enums and structs used throughout the engine
//! submodules. The `Presenter` struct itself lives in `mod.rs`.

use std::path::PathBuf;

use crate::presentation::{PresentationMeta, Slide};
use crate::render::text::StyledLine;

/// The current interaction mode of the presenter.
///
/// Ostendo is a modal application (similar to Vim). The mode determines
/// which keys are active and what is drawn on screen. For example, in
/// `Command` mode the bottom bar shows a `:` prompt, while `Overview`
/// mode replaces the slide with a grid of slide titles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Mode {
    /// Default mode: slide content visible, navigation keys active.
    Normal,
    /// Command-line input mode (`:theme dark`, `:goto 5`, etc.).
    Command,
    /// Numeric goto-slide input mode (`g` then type a number).
    Goto,
    /// Full-screen help overlay showing all keybindings and directives.
    Help,
    /// Grid overview of all slides for quick navigation.
    Overview,
}

/// Cache key for rendered image data.
///
/// Images are expensive to render (especially ASCII art conversion and
/// protocol encoding). This key uniquely identifies a rendered result so
/// it can be reused across frames. The key includes the GIF frame index
/// so each frame of an animated GIF gets its own cache entry.
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub(crate) struct ImageCacheKey {
    /// Filesystem path to the source image file.
    pub(crate) path: PathBuf,
    /// Target width in terminal columns (changes with scale/resize).
    pub(crate) render_width: usize,
    /// Numeric identifier for the image protocol (0=Kitty, 1=iTerm2, etc.).
    pub(crate) protocol: u8,
    /// For animated GIFs, which frame this entry represents. Always 0 for static images.
    pub(crate) gif_frame_index: usize,
    /// Optional hex color override from `<!-- image_color: #hex -->` directive.
    pub(crate) color_override: String,
}

/// How font size changes are animated during slide transitions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum FontTransitionMode {
    /// No animation — font changes instantly.
    None,
    /// Scatter-dissolve: characters randomly replaced by spaces during transition.
    Dissolve,
    /// Smooth fade: content uniformly dims to background, font changes, then fades in.
    Fade,
}

/// Cached rendered image data.
///
/// There are two rendering paths depending on the terminal's image protocol:
/// - **Lines**: ASCII/half-block art stored as styled text lines (works everywhere).
/// - **Protocol**: Raw escape sequence data for Kitty/iTerm2/Sixel (placed after
///   the text buffer is flushed, since protocol images overlay terminal cells).
pub(crate) enum CachedImage {
    /// ASCII or half-block art rendered as styled terminal text lines.
    Lines(Vec<StyledLine>),
    /// Raw protocol escape data (iTerm2/Sixel) plus the number of
    /// terminal rows the image occupies (used for placeholder spacing).
    Protocol { escape_data: String, placeholder_height: usize },
    /// Kitty v2: reference to a Kitty image. Contains the transmit escape
    /// for lazy transmission on first use, then only `a=p` placement (~50 bytes).
    KittyRef {
        image_id: u32,
        cols: usize,
        rows: usize,
        /// The full `a=t` transmit escape. Sent once on first render, then cleared.
        transmit_escape: Option<String>,
    },
}

/// Configuration parameters for constructing a [`super::Presenter`].
///
/// Bundles the startup options passed to [`super::Presenter::new`] to keep the
/// constructor signature within clippy's argument-count limit.
pub struct PresenterConfig {
    /// The parsed slide deck.
    pub slides: Vec<Slide>,
    /// Front-matter metadata (author, date, accent, default alignment).
    pub meta: PresentationMeta,
    /// The initial theme to use.
    pub theme: crate::theme::Theme,
    /// Starting slide index (0 = use saved state, >0 = override).
    pub start: usize,
    /// Path to the markdown file (for hot reload and code execution).
    pub presentation_path: PathBuf,
    /// CLI override for image protocol ("auto", "kitty", "iterm", "sixel", "ascii").
    pub image_mode: String,
    /// Optional WebSocket remote control channels.
    pub remote_channels: Option<(
        std::sync::mpsc::Receiver<crate::remote::RemoteCommand>,
        tokio::sync::broadcast::Sender<String>,
    )>,
    /// If true, disables all code execution.
    pub no_exec: bool,
    /// If true, allows remote-initiated code execution.
    pub remote_exec: bool,
}
