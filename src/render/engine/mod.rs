//! Central orchestrator for the presentation engine.
//!
//! The `Presenter` struct owns all rendering state and manages the complete
//! lifecycle from terminal setup through event processing to cleanup.
//!
//! # Architecture
//!
//! The engine is split across several submodules for maintainability:
//!
//! - **`rendering`** — The core `render_frame()` function that assembles slide
//!   content into a virtual buffer and writes it to the terminal.
//! - **`input`** — Event loop and keyboard/mouse/remote command handling.
//! - **`content`** — Renderers for tables, columns, ASCII art titles, and code output.
//! - **`ui`** — Status bar, help overlay, and overview grid mode.
//! - **`font`** — Terminal font size control via Kitty/Ghostty protocols.
//! - **`state`** — Toggle methods, scale adjustments, and theme persistence.
//! - **`navigation`** — Slide movement, scrolling, and animation triggers.
//!
//! # Virtual Buffer Pattern
//!
//! Rendering never writes directly to the terminal mid-frame. Instead, each
//! frame builds a `Vec<StyledLine>` in memory, then flushes everything to
//! stdout inside a `BeginSynchronizedUpdate` / `EndSynchronizedUpdate` block.
//! This prevents visible flicker even at high frame rates (30 fps for animations).

use anyhow::Result;
use crossterm::{
    cursor,
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers, MouseEventKind},
    queue,
    style::{Attribute, Color, SetAttribute, SetBackgroundColor, SetForegroundColor, ResetColor},
    terminal::{self, BeginSynchronizedUpdate, EndSynchronizedUpdate},
};
use std::collections::HashMap;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::code::highlight::Highlighter;
use crate::render::animation::{
    AnimationState, AnimationKind, parse_transition,
    render_transition_frame, render_entrance_frame,
    render_loop_frame,
};
use crate::image_util::render::{RenderedImage, render_slide_image};
use crate::presentation::{ExecMode, PresentationMeta, Slide, SlideAlignment, StateManager};
use crate::render::layout::WindowSize;
use crate::render::progress::render_progress_bar;
use crate::render::text::{LineContentType, StyledLine, StyledSpan};
use crate::terminal::protocols::{self, ImageProtocol, FontSizeCapability, TextScaleCapability};
use crate::theme::colors::{hex_to_color, ensure_badge_contrast, interpolate_color};
use crate::theme::Theme;

mod state;
mod navigation;
mod font;
mod ui;
mod content;
mod input;
mod rendering;

/// Kitty Graphics Protocol: delete all images (quiet mode).
const KITTY_CLEAR_IMAGES: &[u8] = b"\x1b_Ga=d,d=a,q=2\x1b\\";

/// Build a Kitty RC escape sequence to set font size to an absolute value.
fn kitty_font_escape(size: f64) -> String {
    format!(
        "\x1bP@kitty-cmd{{\"cmd\":\"set_font_size\",\"version\":[0,14,2],\"no_response\":true,\"payload\":{{\"size\":{:.1}}}}}\x1b\\",
        size
    )
}

/// Get the bullet indent string for a given nesting depth.
fn bullet_indent(depth: usize) -> &'static str {
    match depth {
        0 => "  * ",
        1 => "      - ",
        _ => "          > ",
    }
}

/// Compute content width from terminal width and scale percentage.
fn scaled_content_width(tw: usize, scale: u8) -> usize {
    ((tw as f64 * scale as f64 / 100.0) as usize).min(tw)
}

/// Resolve an `ImageRenderMode` to a concrete `ImageProtocol`.
fn resolve_image_protocol(mode: crate::presentation::ImageRenderMode, default: ImageProtocol) -> ImageProtocol {
    match mode {
        crate::presentation::ImageRenderMode::Kitty => ImageProtocol::Kitty,
        crate::presentation::ImageRenderMode::Iterm => ImageProtocol::Iterm2,
        crate::presentation::ImageRenderMode::Sixel => ImageProtocol::Sixel,
        crate::presentation::ImageRenderMode::Ascii => ImageProtocol::Ascii,
        crate::presentation::ImageRenderMode::Auto => default,
    }
}

/// Numeric key for image protocol (used in cache keys).
fn protocol_cache_key(proto: ImageProtocol) -> u8 {
    match proto {
        ImageProtocol::Kitty => 0,
        ImageProtocol::Iterm2 => 1,
        ImageProtocol::Sixel => 2,
        ImageProtocol::Ascii => 3,
    }
}

/// Strip terminal control characters from a string to prevent escape sequence injection.
/// Preserves printable characters, spaces, and tabs. Removes ANSI escape sequences,
/// and control characters (0x00-0x1F except \t, 0x7F).
fn strip_control_chars(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1B' {
            // Skip ANSI escape sequence: ESC followed by [ ... final byte
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next.is_ascii_alphabetic() || next == '~' {
                        break;
                    }
                }
            }
            // Also skip ESC ] (OSC) sequences terminated by BEL or ST
            else if chars.peek() == Some(&']') {
                chars.next();
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next == '\x07' {
                        break;
                    }
                    if next == '\x1B' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            continue;
        }
        if c == '\t' || (c >= ' ' && c != '\x7F') {
            out.push(c);
        }
    }
    out
}

/// Get comment prefix for a programming language (used in code block labels).
fn comment_prefix_for(lang: &str) -> &'static str {
    match lang {
        "python" | "python3" | "py" | "bash" | "sh" | "ruby" | "rb" | "yaml" | "toml" | "r" => "# ",
        "html" | "xml" => "<!-- ",
        "css" => "/* ",
        "sql" | "lua" | "haskell" => "-- ",
        "c" | "cpp" | "c++" | "java" | "javascript" | "js" | "typescript" | "go" | "golang" | "rust"
        | "swift" | "kotlin" | "scala" | "php" | "dart" | "zig" => "// ",
        _ => "// ",
    }
}

/// The current interaction mode of the presenter.
///
/// Ostendo is a modal application (similar to Vim). The mode determines
/// which keys are active and what is drawn on screen. For example, in
/// `Command` mode the bottom bar shows a `:` prompt, while `Overview`
/// mode replaces the slide with a grid of slide titles.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
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
struct ImageCacheKey {
    /// Filesystem path to the source image file.
    path: PathBuf,
    /// Target width in terminal columns (changes with scale/resize).
    render_width: usize,
    /// Numeric identifier for the image protocol (0=Kitty, 1=iTerm2, etc.).
    protocol: u8,
    /// For animated GIFs, which frame this entry represents. Always 0 for static images.
    gif_frame_index: usize,
    /// Optional hex color override from `<!-- image_color: #hex -->` directive.
    color_override: String,
}

/// Cached rendered image data.
///
/// There are two rendering paths depending on the terminal's image protocol:
/// - **Lines**: ASCII/half-block art stored as styled text lines (works everywhere).
/// - **Protocol**: Raw escape sequence data for Kitty/iTerm2/Sixel (placed after
///   the text buffer is flushed, since protocol images overlay terminal cells).
enum CachedImage {
    /// ASCII or half-block art rendered as styled terminal text lines.
    Lines(Vec<StyledLine>),
    /// Raw protocol escape data (Kitty/iTerm2/Sixel) plus the number of
    /// terminal rows the image occupies (used for placeholder spacing).
    Protocol { escape_data: String, placeholder_height: usize },
}

/// The main presentation engine.
///
/// `Presenter` owns every piece of state needed to run a terminal-based
/// slide show: the parsed slides, the current theme, image caches, animation
/// state, font control, and the connection to the optional WebSocket remote.
///
/// # Lifecycle
///
/// 1. `Presenter::new()` — parses theme colors, detects terminal capabilities,
///    preloads images, restores saved state from disk.
/// 2. `Presenter::run()` — enters raw mode, switches to the alternate screen,
///    runs `event_loop()`, then cleans up the terminal on exit.
/// 3. Inside the event loop, `render_frame()` is called on every input event,
///    animation tick, or timer update.
pub struct Presenter {
    // --- Slide Data ---

    /// The parsed slide deck. Each `Slide` contains title, bullets, code blocks,
    /// images, directives, and speaker notes.
    slides: Vec<Slide>,
    /// Front-matter metadata (author, date, accent override, default alignment).
    meta: PresentationMeta,
    /// The currently active theme (may differ from `base_theme` during light/dark toggling).
    theme: Theme,
    /// Zero-based index of the currently displayed slide.
    current: usize,

    // --- UI State ---

    /// Current interaction mode (Normal, Command, Goto, Help, Overview).
    mode: Mode,
    /// Text buffer for the `:command` input bar.
    command_buf: String,
    /// Text buffer for the `g` + number goto input.
    goto_buf: String,
    /// Whether the speaker notes panel is visible at the bottom.
    show_notes: bool,
    /// Scroll offset within the notes panel (for long notes).
    notes_scroll: usize,
    /// Whether the status bar is hidden (fullscreen mode).
    show_fullscreen: bool,
    /// Whether the theme name badge is shown in the status bar.
    show_theme_name: bool,
    /// Whether section labels are displayed above slide titles.
    show_sections: bool,
    /// Vertical scroll offset within the current slide's content (in lines).
    scroll_offset: usize,
    /// When `Some`, the presentation timer is running. The `Instant` marks
    /// when the timer was started (elapsed = now - start).
    timer_start: Option<Instant>,

    // --- Terminal Dimensions ---

    /// Current terminal width in columns (updated on resize events).
    width: u16,
    /// Current terminal height in rows (updated on resize events).
    height: u16,

    // --- Code Execution ---

    /// Syntax highlighter for code blocks (shared across all slides).
    highlighter: Highlighter,
    /// Accumulated stdout/stderr from the most recent code execution.
    exec_output: Option<String>,
    /// Channel receiver for streaming code execution output. `None` means
    /// no execution is in progress. Receives `Some(line)` for output and
    /// `None` when the process exits.
    exec_rx: Option<std::sync::mpsc::Receiver<Option<String>>>,
    /// Index of the currently-executing code block within the slide
    /// (Ctrl+E cycles through executable blocks).
    exec_block_index: usize,

    // --- Persistence ---

    /// JSON state manager for saving/restoring slide position, font offsets,
    /// and theme selection across restarts.
    state: StateManager,

    // --- Image Rendering ---

    /// Detected (or CLI-overridden) terminal image protocol (Kitty, iTerm2, Sixel, ASCII).
    image_protocol: ImageProtocol,
    /// Cache of rendered image data keyed by (path, width, protocol, frame).
    /// Avoids re-rendering images on every frame.
    image_cache: HashMap<ImageCacheKey, CachedImage>,
    /// Pre-loaded RGBA image data for all slide images (loaded at startup).
    preloaded_images: HashMap<PathBuf, image::RgbaImage>,
    /// Decoded GIF frames for animated images. The `Arc` allows sharing with
    /// background render threads without copying frame data.
    gif_frames: HashMap<PathBuf, std::sync::Arc<Vec<crate::image_util::GifFrame>>>,
    /// Handle for the background thread that decodes GIF frames at startup.
    /// `None` once decoding is complete or if no GIFs exist.
    gif_loading: Option<std::thread::JoinHandle<HashMap<PathBuf, Vec<crate::image_util::GifFrame>>>>,
    /// Receives pre-rendered GIF frame cache entries from background thread.
    gif_render_rx: Option<std::sync::mpsc::Receiver<(ImageCacheKey, CachedImage)>>,
    /// Current frame index for animated GIF playback (wraps around).
    gif_current_frame: usize,
    /// Timestamp of last GIF frame advance (used to honor per-frame delays).
    gif_last_advance: std::time::Instant,
    /// Terminal dimensions in both columns/rows and pixels (needed for image scaling).
    window_size: WindowSize,

    // --- Remote Control ---

    /// Channel receiver for commands from the WebSocket remote control server.
    /// `None` if remote control is not enabled.
    remote_rx: Option<std::sync::mpsc::Receiver<crate::remote::RemoteCommand>>,
    /// Broadcast sender for pushing presentation state to connected WebSocket clients.
    state_broadcast: Option<tokio::sync::broadcast::Sender<String>>,

    // --- Theme Colors ---
    // These are resolved from the theme's hex strings at startup for fast access.

    /// Background color for the slide area.
    bg_color: Color,
    /// Accent color used for titles, bullets markers, borders, and highlights.
    accent_color: Color,
    /// Primary text color for bullet content and body text.
    text_color: Color,
    /// Background color for code blocks and the status bar timer section.
    code_bg_color: Color,

    // --- Font & Scale ---

    /// Per-slide font size offsets (slide index -> offset in 2pt steps).
    /// Populated from markdown `<!-- font_size -->` directives and user `]`/`[` adjustments.
    slide_font_offsets: HashMap<usize, i8>,
    /// Global content scale percentage (default 80). Controls the width of the
    /// content area relative to the terminal width.
    global_scale: u8,
    /// Background color for keybinding badges in the help overlay.
    /// Computed to ensure contrast against the theme background.
    help_badge_bg: Color,
    /// Detected terminal font control capability (Kitty RC, Ghostty keystroke, or None).
    font_capability: FontSizeCapability,
    /// The terminal's original font size captured at startup, used to restore
    /// on exit and as the base for per-slide offsets.
    original_font_size: Option<String>,

    // --- Hot Reload ---

    /// File watcher that polls the presentation file for changes every 500ms.
    file_watcher: Option<crate::watch::FileWatcher>,
    /// Absolute path to the presentation markdown file (needed for reload and code execution).
    presentation_path: PathBuf,

    // --- Theme Gradient ---

    /// Starting color for the background gradient (top or left edge).
    gradient_from: Option<Color>,
    /// Ending color for the background gradient (bottom or right edge).
    gradient_to: Option<Color>,
    /// Whether the gradient runs vertically (true) or horizontally (false).
    gradient_vertical: bool,
    /// Whether we are currently showing the light variant of the theme.
    is_light_variant: bool,

    // --- Animations ---

    /// The currently playing one-shot animation (slide transition or entrance effect).
    /// `None` when no animation is active.
    active_animation: Option<crate::render::animation::AnimationState>,
    /// Active looping animations for the current slide (e.g., sparkle, matrix, pulse).
    /// Each entry is a (loop type, frame counter) pair.
    active_loop: Vec<(crate::render::animation::LoopAnimation, u64)>,
    /// The last rendered virtual buffer. Kept so slide transitions can use the
    /// previous frame as their "old" content for dissolve/fade effects.
    last_rendered_buffer: Vec<StyledLine>,

    // --- Mermaid Diagrams ---

    /// Optional Mermaid diagram renderer (requires `mmdc` CLI to be installed).
    mermaid_renderer: Option<crate::image_util::mermaid::MermaidRenderer>,

    // --- Font Change Transition State ---

    /// `Some(true/false)` when the user manually toggled fullscreen with `f`.
    /// Used to distinguish user intent from per-slide `<!-- fullscreen -->` directives.
    user_fullscreen_override: Option<bool>,
    /// Deferred font size target. Font changes happen at the start of `render_frame()`
    /// before the synchronized update block, so this queues the change.
    pending_font_size: Option<f64>,
    /// The last font size that was actually applied to the terminal (for delta calculations).
    last_applied_font_size: Option<f64>,
    /// True when font change was triggered by slide navigation (fade out old content).
    /// False when triggered by `]`/`[` interactive adjustment (no fade).
    font_change_is_slide_transition: bool,
    /// OSC 66 text scaling capability (disabled pending rendering fix, but kept for detection).
    #[allow(dead_code)]
    text_scale_cap: TextScaleCapability,
    /// When true, the next render will dissolve-in the new content after flush.
    /// This is set after a font-change dissolve-out completes.
    pending_dissolve_in: bool,

    // --- Smart Redraw Tracking ---
    // These fields cache the state from the last `render_frame()` call.
    // If nothing changed, only the status bar (timer) is redrawn, avoiding
    // expensive image re-emission and full-screen repaints.

    /// Slide index that was rendered last frame (or `None` on first render).
    last_rendered_slide: Option<usize>,
    /// Scroll offset that was rendered last frame.
    last_rendered_scroll: usize,
    /// Terminal width at last render.
    last_rendered_width: u16,
    /// Terminal height at last render.
    last_rendered_height: u16,
    /// Interaction mode at last render.
    last_rendered_mode: Mode,
    /// Content scale at last render.
    last_rendered_scale: u8,
    /// Image scale offset at last render.
    last_rendered_image_scale: i8,
    /// GIF frame index at last render (triggers image refresh when it changes).
    last_rendered_gif_frame: usize,
    /// Flag that forces a complete re-render on the next frame. Set by
    /// animations, resize events, slide changes, and theme switches.
    needs_full_redraw: bool,

    // --- Miscellaneous ---

    /// Runtime image scale adjustment from `>` / `<` keys (-100 to +100).
    image_scale_offset: i8,
    /// List of all available theme slugs (for remote control theme switching).
    theme_slugs: Vec<String>,
    /// The original theme before any light/dark variant toggling.
    base_theme: Theme,
    /// Whether code execution is allowed (`false` when `--no-exec` is passed).
    allow_exec: bool,
    /// Whether remote-initiated code execution is allowed (`--remote-exec` flag).
    allow_remote_exec: bool,
    /// Cached FIGlet font for rendering ASCII art titles. Loaded once at startup
    /// from the bundled `slant.flf` font file.
    figfont: Option<figlet_rs::FIGfont>,
}

impl Presenter {
    /// Create a new `Presenter` with the given slides, theme, and configuration.
    ///
    /// This performs the full initialization sequence:
    /// 1. Resolves theme colors from hex strings.
    /// 2. Applies front-matter accent color override (if present).
    /// 3. Detects terminal capabilities: image protocol, font control, text scaling.
    /// 4. Queries the terminal's current font size (for restore on exit).
    /// 5. Restores saved state from disk (slide position, font offsets, theme).
    /// 6. Pre-loads all slide images into memory (static images immediately, GIF
    ///    frames in a background thread).
    /// 7. Initializes the Mermaid renderer if any slide has mermaid blocks.
    ///
    /// # Parameters
    ///
    /// - `slides` — The parsed slide deck.
    /// - `meta` — Front-matter metadata (author, date, accent, default alignment).
    /// - `theme` — The initial theme to use.
    /// - `start` — Starting slide index (0 = use saved state, >0 = override).
    /// - `presentation_path` — Path to the markdown file (for hot reload and code execution).
    /// - `image_mode` — CLI override for image protocol ("auto", "kitty", "iterm", "sixel", "ascii").
    /// - `remote_channels` — Optional WebSocket remote control channels.
    /// - `no_exec` — If true, disables all code execution.
    /// - `remote_exec` — If true, allows remote-initiated code execution.
    pub fn new(
        slides: Vec<Slide>,
        meta: PresentationMeta,
        theme: Theme,
        start: usize,
        presentation_path: &Path,
        image_mode: &str,
        remote_channels: Option<(
            std::sync::mpsc::Receiver<crate::remote::RemoteCommand>,
            tokio::sync::broadcast::Sender<String>,
        )>,
        no_exec: bool,
        remote_exec: bool,
    ) -> Self {
        let bg = hex_to_color(&theme.colors.background).unwrap_or(Color::Black);
        let mut accent = hex_to_color(&theme.colors.accent).unwrap_or(Color::Green);
        // Presentation-level accent override from front matter
        if !meta.accent.is_empty() {
            if let Some(c) = hex_to_color(&meta.accent) {
                accent = c;
            }
        }
        let text = hex_to_color(&theme.colors.text).unwrap_or(Color::White);
        let code_bg = hex_to_color(&theme.colors.code_background).unwrap_or(Color::DarkGrey);
        let help_badge_bg = ensure_badge_contrast(code_bg, bg);
        // Parse gradient colors from theme
        let (gradient_from, gradient_to, gradient_vertical) = if let Some(ref grad) = theme.gradient {
            let from = hex_to_color(&grad.from);
            let to = hex_to_color(&grad.to);
            let vertical = grad.direction != "horizontal";
            (from, to, vertical)
        } else {
            (None, None, true)
        };
        let font_capability = protocols::detect_font_capability();
        let original_font_size = match font_capability {
            FontSizeCapability::KittyRemote => Self::query_kitty_font_size(),
            FontSizeCapability::GhosttyKeystroke => Self::query_ghostty_font_size(),
            FontSizeCapability::None => None,
        };
        let window_size = WindowSize::query();
        let (w, h) = (window_size.columns, window_size.rows);
        let state = StateManager::load(presentation_path);
        // Restore slide position from saved state (CLI --slide flag overrides)
        let restored_slide = if start == 0 {
            state.get_current_slide()
        } else {
            start
        };
        // Restore per-slide font offsets from saved state
        let mut slide_font_offsets: HashMap<usize, i8> = HashMap::new();
        for i in 0..slides.len() {
            // Markdown directive sets the base; saved state overrides
            if let Some(saved) = state.get_font_offset(i) {
                slide_font_offsets.insert(i, saved);
            } else if let Some(md_size) = slides[i].font_size {
                // Convert markdown font_size (-3..7) to offset: (size - 1) * 2pt steps
                let offset = (md_size - 1) * 2;
                if offset != 0 {
                    slide_font_offsets.insert(i, offset);
                }
            }
        }
        let image_protocol = match image_mode {
            "kitty" => ImageProtocol::Kitty,
            "iterm" | "iterm2" => ImageProtocol::Iterm2,
            "sixel" => ImageProtocol::Sixel,
            "ascii" => ImageProtocol::Ascii,
            _ => protocols::detect_protocol(),
        };
        let (remote_rx, state_broadcast) = match remote_channels {
            Some((rx, tx)) => (Some(rx), Some(tx)),
            None => (None, None),
        };
        // Preload all slide images into memory
        let mut preloaded_images = HashMap::new();
        let gif_frames: HashMap<PathBuf, std::sync::Arc<Vec<crate::image_util::GifFrame>>> = HashMap::new();
        // Collect GIF paths for background loading
        let mut gif_paths: Vec<PathBuf> = Vec::new();
        for slide in &slides {
            if let Some(ref img) = slide.image {
                if img.path.exists() && !preloaded_images.contains_key(&img.path) {
                    let ext = img.path.extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    if ext == "gif" {
                        // Load first frame quickly for initial display
                        if let Ok(loaded) = crate::image_util::load_image(&img.path) {
                            preloaded_images.insert(img.path.clone(), loaded);
                        }
                        gif_paths.push(img.path.clone());
                    } else if let Ok(loaded) = crate::image_util::load_image(&img.path) {
                        preloaded_images.insert(img.path.clone(), loaded);
                    }
                }
            }
        }
        // Decode GIF frames in background thread to avoid blocking startup
        let gif_loading: Option<std::thread::JoinHandle<HashMap<PathBuf, Vec<crate::image_util::GifFrame>>>> =
            if !gif_paths.is_empty() {
                Some(std::thread::spawn(move || {
                    let mut result = HashMap::new();
                    for path in gif_paths {
                        if let Some(frames) = crate::image_util::load_gif_frames(&path) {
                            result.insert(path, frames);
                        }
                    }
                    result
                }))
            } else {
                None
            };

        let base_theme = theme.clone();
        let mut presenter = Self {
            current: restored_slide.min(slides.len().saturating_sub(1)),
            slides,
            meta,
            theme,
            mode: Mode::Normal,
            command_buf: String::new(),
            goto_buf: String::new(),
            show_notes: false,
            notes_scroll: 0,
            show_fullscreen: false,
            show_theme_name: false,
            show_sections: true,
            scroll_offset: 0,
            timer_start: None,
            width: w,
            height: h,
            highlighter: Highlighter::new(),
            exec_output: None,
            exec_rx: None,
            exec_block_index: 0,
            state,
            image_protocol,
            image_cache: HashMap::new(),
            preloaded_images,
            gif_frames,
            gif_loading,
            gif_render_rx: None,
            gif_current_frame: 0,
            gif_last_advance: std::time::Instant::now(),
            window_size,
            remote_rx,
            state_broadcast,
            bg_color: bg,
            accent_color: accent,
            text_color: text,
            code_bg_color: code_bg,
            slide_font_offsets,
            global_scale: 80,
            help_badge_bg,
            font_capability,
            original_font_size,
            file_watcher: Some(crate::watch::FileWatcher::new(presentation_path.to_path_buf())),
            presentation_path: presentation_path.to_path_buf(),
            last_rendered_slide: None,
            last_rendered_scroll: 0,
            last_rendered_width: 0,
            last_rendered_height: 0,
            last_rendered_mode: Mode::Normal,
            last_rendered_scale: 80,
            last_rendered_image_scale: 0,
            last_rendered_gif_frame: 0,
            needs_full_redraw: true,
            image_scale_offset: 0,
            gradient_from,
            gradient_to,
            gradient_vertical,
            is_light_variant: false,
            active_animation: None,
            active_loop: Vec::new(),
            last_rendered_buffer: Vec::new(),
            mermaid_renderer: None,
            user_fullscreen_override: None,
            pending_font_size: None,
            last_applied_font_size: None,
            font_change_is_slide_transition: false,
            text_scale_cap: protocols::detect_text_scale_capability(),
            pending_dissolve_in: false,
            theme_slugs: crate::theme::ThemeRegistry::load().list(),
            base_theme,
            allow_exec: !no_exec,
            allow_remote_exec: remote_exec,
            figfont: {
                let font_data = include_str!("../../../fonts/slant.flf");
                figlet_rs::FIGfont::from_content(font_data)
                    .or_else(|_| figlet_rs::FIGfont::standard())
                    .ok()
            },
        };
        // Initialize mermaid renderer if any slide has mermaid blocks
        let has_mermaid = presenter.slides.iter().any(|s| !s.mermaid_blocks.is_empty());
        if has_mermaid && crate::image_util::mermaid::MermaidRenderer::is_available() {
            presenter.mermaid_renderer = Some(crate::image_util::mermaid::MermaidRenderer::new());
        }
        // Restore saved theme (unless CLI explicitly specified a non-default theme)
        if presenter.theme.slug == "terminal_green" {
            if let Some(saved_slug) = presenter.state.get_theme_slug() {
                if saved_slug != presenter.theme.slug {
                    let registry = crate::theme::ThemeRegistry::load();
                    if let Some(saved_theme) = registry.get(saved_slug) {
                        presenter.base_theme = saved_theme.clone();
                        presenter.apply_theme(saved_theme);
                    }
                }
            }
        }
        presenter
    }

    /// Enable or disable fullscreen mode (hides the status bar).
    pub fn set_fullscreen(&mut self, fs: bool) { self.show_fullscreen = fs; }

    /// Start the presentation timer from the current moment.
    pub fn start_timer(&mut self) { self.timer_start = Some(Instant::now()); }

    /// Reset (stop) the presentation timer.
    fn reset_timer(&mut self) { self.timer_start = None; }

    /// Set the default content scale percentage (e.g., 80 = content uses 80% of terminal width).
    pub fn set_default_scale(&mut self, scale: u8) {
        self.global_scale = scale;
    }

    /// Pre-render all slide images into the cache so navigation is instant.
    ///
    /// Iterates over every slide that has an image, computes the cache key
    /// based on current dimensions and scale, and renders any images not
    /// already in the cache. This is called once during `run()` before the
    /// event loop starts.
    fn prerender_images(&mut self) {
        let tw = self.width as usize;
        let th = self.height as usize;
        let content_width = scaled_content_width(tw, self.current_scale());
        let margin = tw.saturating_sub(content_width) / 2;

        // Collect render jobs from slides we haven't cached yet.
        // We must gather them first to avoid borrowing &self.slides while
        // mutating self.image_cache.
        struct PrerenderJob {
            img: crate::presentation::SlideImage,
            cache_key: ImageCacheKey,
            img_width: usize,
            img_max_height: usize,
            img_pad: String,
            effective_protocol: ImageProtocol,
        }
        let mut jobs = Vec::new();
        for slide in &self.slides {
            if let Some(ref img) = slide.image {
                let effective_protocol = resolve_image_protocol(img.render_mode, self.image_protocol);
                let proto_key = protocol_cache_key(effective_protocol);
                let effective_scale = (img.scale as i16 + self.image_scale_offset as i16).clamp(5, 100) as u8;
                let img_width = (content_width as f64 * effective_scale as f64 / 100.0).max(1.0) as usize;
                let img_max_height = (th as f64 * effective_scale as f64 / 100.0 / 2.0).max(1.0) as usize;
                let img_extra_margin = content_width.saturating_sub(img_width) / 2;
                let img_pad = " ".repeat(margin + img_extra_margin);
                let cache_key = ImageCacheKey {
                    path: img.path.clone(),
                    render_width: img_width,
                    protocol: proto_key,
                    gif_frame_index: 0,
                    color_override: img.color_override.clone(),
                };
                if !self.image_cache.contains_key(&cache_key) {
                    jobs.push(PrerenderJob {
                        img: img.clone(),
                        cache_key,
                        img_width,
                        img_max_height,
                        img_pad,
                        effective_protocol,
                    });
                }
            }
        }

        for job in jobs {
            let preloaded = self.preloaded_images.get(&job.img.path);
            let rendered = render_slide_image(
                &job.img, job.img_width, job.img_max_height, &job.img_pad,
                self.accent_color, self.text_color,
                job.effective_protocol, self.bg_color,
                &self.window_size, preloaded,
            );
            let cached = match rendered {
                RenderedImage::Lines(l) => CachedImage::Lines(l),
                RenderedImage::Protocol { escape_data, placeholder_height } => {
                    CachedImage::Protocol { escape_data, placeholder_height }
                }
            };
            self.image_cache.insert(job.cache_key, cached);
        }
    }

    /// Spawn a background thread to pre-render all GIF frames into cache entries.
    /// Results stream back via `gif_render_rx` and are merged in the event loop.
    fn spawn_gif_prerender(&mut self) {
        let tw = self.width as usize;
        let th = self.height as usize;
        let content_width = scaled_content_width(tw, self.current_scale());
        let margin = tw.saturating_sub(content_width) / 2;
        let image_protocol = self.image_protocol;
        let image_scale_offset = self.image_scale_offset;
        let accent_color = self.accent_color;
        let text_color = self.text_color;
        let bg_color = self.bg_color;
        let window_size = self.window_size;

        // Collect render jobs: (SlideImage metadata, frames, computed params)
        struct RenderJob {
            img: crate::presentation::SlideImage,
            frames: std::sync::Arc<Vec<crate::image_util::GifFrame>>,
            img_width: usize,
            img_max_height: usize,
            img_pad: String,
            effective_protocol: ImageProtocol,
            proto_key: u8,
        }
        let mut jobs = Vec::new();
        for slide in &self.slides {
            if let Some(ref img) = slide.image {
                if let Some(frames) = self.gif_frames.get(&img.path) {
                    let effective_protocol = resolve_image_protocol(img.render_mode, image_protocol);
                    let proto_key = protocol_cache_key(effective_protocol);
                    let effective_scale = (img.scale as i16 + image_scale_offset as i16).clamp(5, 100) as u8;
                    let img_width = (content_width as f64 * effective_scale as f64 / 100.0).max(1.0) as usize;
                    let img_max_height = (th as f64 * effective_scale as f64 / 100.0 / 2.0).max(1.0) as usize;
                    let img_extra_margin = content_width.saturating_sub(img_width) / 2;
                    let img_pad = " ".repeat(margin + img_extra_margin);
                    jobs.push(RenderJob {
                        img: img.clone(),
                        frames: std::sync::Arc::clone(frames),
                        img_width,
                        img_max_height,
                        img_pad,
                        effective_protocol,
                        proto_key,
                    });
                }
            }
        }
        if jobs.is_empty() { return; }

        let (tx, rx) = std::sync::mpsc::channel();
        self.gif_render_rx = Some(rx);

        std::thread::spawn(move || {
            for job in jobs {
                for (idx, frame) in job.frames.iter().enumerate() {
                    let cache_key = ImageCacheKey {
                        path: job.img.path.clone(),
                        render_width: job.img_width,
                        protocol: job.proto_key,
                        gif_frame_index: idx,
                        color_override: job.img.color_override.clone(),
                    };
                    let rendered = render_slide_image(
                        &job.img, job.img_width, job.img_max_height, &job.img_pad,
                        accent_color, text_color,
                        job.effective_protocol, bg_color,
                        &window_size, Some(&frame.image),
                    );
                    let cached = match rendered {
                        RenderedImage::Lines(l) => CachedImage::Lines(l),
                        RenderedImage::Protocol { escape_data, placeholder_height } => {
                            CachedImage::Protocol { escape_data, placeholder_height }
                        }
                    };
                    if tx.send((cache_key, cached)).is_err() { return; }
                }
            }
        });
    }

    /// Run the presentation.
    ///
    /// This is the top-level entry point that manages the full terminal lifecycle:
    ///
    /// 1. Pre-renders all slide images into the cache.
    /// 2. Applies initial font size and per-slide theme overrides.
    /// 3. Enters terminal raw mode and switches to the alternate screen
    ///    (Rust's `crossterm` library handles the low-level terminal setup).
    /// 4. Sets the terminal background color via OSC 11 so font-change
    ///    resizes don't flash black.
    /// 5. Runs the event loop (`event_loop()`), which blocks until the user quits.
    /// 6. On exit: restores original font size, leaves alternate screen,
    ///    disables raw mode, and saves state to disk.
    ///
    /// # Errors
    ///
    /// Returns `Err` if terminal setup/teardown fails or if an unrecoverable
    /// I/O error occurs during rendering.
    pub fn run(&mut self) -> Result<()> {
        self.prerender_images();
        // Apply initial slide's font offset (if restored from saved state)
        self.apply_slide_font();
        // Apply per-slide theme override for the starting slide
        self.apply_slide_theme();
        // Initialize loop/entrance animations for the starting slide
        {
            let slide = &self.slides[self.current];
            self.active_loop = slide.loop_animations.iter().map(|(la, _)| (*la, 0)).collect();
            if let Some(fs) = slide.fullscreen {
                self.show_fullscreen = fs;
            }
        }
        terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        crossterm::execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide, EnableMouseCapture)?;
        // Set terminal default background to theme bg so cells created by
        // font-change resizes inherit the correct color (no black flicker).
        Self::set_terminal_bg(self.bg_color);

        let result = self.event_loop();

        // Restore terminal default background before leaving alternate screen
        Self::reset_terminal_bg();
        self.reset_font_size();
        crossterm::execute!(stdout, DisableMouseCapture, cursor::Show, terminal::LeaveAlternateScreen)?;
        terminal::disable_raw_mode()?;

        // Save state on exit
        self.save_state();

        result
    }

    /// Execute the current slide's active code block.
    ///
    /// If a previous execution has completed, advances to the next executable
    /// block (Ctrl+E cycles through blocks). Collects all executable code blocks
    /// from both slide-level and column-level sources, prepends any preamble
    /// code, then spawns a streaming execution process.
    ///
    /// Does nothing if `--no-exec` was passed or if the slide has no executable blocks.
    fn execute_code(&mut self) -> Result<()> {
        if !self.allow_exec {
            return Ok(());
        }

        // If previous execution completed, advance to next block
        if self.exec_output.is_some() && self.exec_rx.is_none() {
            self.exec_block_index += 1;
            self.exec_output = None;
        }

        let slide = &self.slides[self.current];
        // Collect all executable code blocks: slide-level first, then columns
        let exec_blocks: Vec<&crate::presentation::CodeBlock> = slide.code_blocks.iter()
            .filter(|cb| cb.exec_mode.is_some())
            .chain(
                slide.columns.as_ref()
                    .map(|cols| cols.contents.iter().flat_map(|c| c.code_blocks.iter())
                        .filter(|cb| cb.exec_mode.is_some())
                        .collect::<Vec<_>>())
                    .unwrap_or_default()
            )
            .collect();
        // Fallback: if no exec blocks, try first code block
        let exec_blocks: Vec<&crate::presentation::CodeBlock> = if exec_blocks.is_empty() {
            slide.code_blocks.first().into_iter().collect()
        } else {
            exec_blocks
        };
        // Wrap around if past the last block
        if !exec_blocks.is_empty() && self.exec_block_index >= exec_blocks.len() {
            self.exec_block_index = 0;
        }
        if let Some(cb) = exec_blocks.get(self.exec_block_index) {
            // Prepend preamble if one exists for this language
            let code = if let Some(preamble) = slide.code_preambles.get(&cb.language) {
                format!("{}\n{}", preamble, cb.code)
            } else {
                cb.code.clone()
            };
            let pres_dir = self.presentation_path.parent();
            let rx = crate::code::executor::execute_code_streaming(&cb.language, &code, pres_dir)?;
            self.exec_output = Some(String::new());
            self.exec_rx = Some(rx);
        }
        Ok(())
    }

    /// Poll for streaming code execution output.
    ///
    /// Drains all available lines from `exec_rx` into `exec_output`.
    /// Returns `true` if any output was received (signals a needed redraw).
    /// When the channel sends `None`, execution is complete and the receiver
    /// is dropped.
    fn poll_exec_output(&mut self) -> bool {
        let mut got_output = false;
        if let Some(ref rx) = self.exec_rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    Some(line) => {
                        if let Some(ref mut output) = self.exec_output {
                            if !output.is_empty() { output.push('\n'); }
                            output.push_str(&line);
                        }
                        got_output = true;
                    }
                    None => {
                        // Execution complete — advance to next block
                        self.exec_rx = None;
                        got_output = true;
                        break;
                    }
                }
            }
        }
        got_output
    }

    /// Reload the presentation from disk, preserving slide position.
    fn try_reload(&mut self) {
        if let Ok(source) = std::fs::read_to_string(&self.presentation_path) {
            let base_dir = self.presentation_path.parent();
            if let Ok((new_meta, new_slides)) = crate::markdown::parse_presentation(&source, base_dir) {
                if !new_slides.is_empty() {
                    // Preload images for new slides
                    for slide in &new_slides {
                        if let Some(ref img) = slide.image {
                            if img.path.exists() && !self.preloaded_images.contains_key(&img.path) {
                                if let Ok(loaded) = crate::image_util::load_image(&img.path) {
                                    self.preloaded_images.insert(img.path.clone(), loaded);
                                }
                            }
                        }
                    }
                    // Apply accent override from refreshed meta
                    if !new_meta.accent.is_empty() {
                        if let Some(c) = hex_to_color(&new_meta.accent) {
                            self.accent_color = c;
                        }
                    }
                    self.meta = new_meta;
                    // Clamp current slide to new count
                    self.current = self.current.min(new_slides.len().saturating_sub(1));
                    self.slides = new_slides;
                    self.image_cache.clear();
                    self.needs_full_redraw = true;
                }
            }
        }
    }

    /// Write a single styled line to the terminal output buffer using the default background.
    ///
    /// Delegates to `queue_styled_line_with_bg` with the theme's background color.
    /// Each `StyledSpan` in the line is rendered with its own foreground, background,
    /// and text attributes (bold, italic, dim, etc.).
    fn queue_styled_line(&self, w: &mut impl Write, line: &StyledLine, term_width: usize) -> Result<()> {
        self.queue_styled_line_with_bg(w, line, term_width, self.bg_color)
    }

    /// Write a styled line with a custom default background (used for gradient rows).
    fn queue_styled_line_with_bg(&self, w: &mut impl Write, line: &StyledLine, term_width: usize, default_bg: Color) -> Result<()> {
        let mut chars_written = 0usize;
        // Set default background for the entire line
        queue!(w, SetBackgroundColor(default_bg))?;
        for span in &line.spans {
            if chars_written >= term_width {
                break;
            }
            // Reset attributes before each span to avoid leaking
            queue!(w, SetAttribute(Attribute::NoBold),
                      SetAttribute(Attribute::NoItalic),
                      SetAttribute(Attribute::NormalIntensity),
                      SetAttribute(Attribute::NotCrossedOut),
                      SetAttribute(Attribute::NoUnderline))?;
            let bg = span.bg.unwrap_or(default_bg);
            let fg = span.fg.unwrap_or(self.text_color);
            queue!(w, SetForegroundColor(fg))?;
            queue!(w, SetBackgroundColor(bg))?;
            if span.bold {
                queue!(w, SetAttribute(Attribute::Bold))?;
            }
            if span.italic {
                queue!(w, SetAttribute(Attribute::Italic))?;
            }
            if span.dim {
                queue!(w, SetAttribute(Attribute::Dim))?;
            }
            if span.strikethrough {
                queue!(w, SetAttribute(Attribute::CrossedOut))?;
            }
            if span.underline {
                queue!(w, SetAttribute(Attribute::Underlined))?;
            }
            // Truncate span text to fit within terminal width
            let base_width = unicode_width::UnicodeWidthStr::width(span.text.as_str());
            let scale_factor = if span.text_scale >= 2 { span.text_scale as usize } else { 1 };
            let effective_width = base_width * scale_factor;
            let remaining = term_width.saturating_sub(chars_written);
            if effective_width <= remaining {
                write_span_text(w, span.text_scale, &span.text)?;
                chars_written += effective_width;
            } else {
                let char_budget = remaining / scale_factor;
                let truncated = truncate_to_width(&span.text, char_budget);
                let trunc_w = unicode_width::UnicodeWidthStr::width(truncated.as_str());
                write_span_text(w, span.text_scale, &truncated)?;
                chars_written += trunc_w * scale_factor;
            }
        }
        // Reset attributes and fill rest of line with background
        queue!(w, SetAttribute(Attribute::Reset), SetBackgroundColor(default_bg))?;
        if chars_written < term_width {
            write!(w, "{}", " ".repeat(term_width - chars_written))?;
        }
        Ok(())
    }

    /// Returns true if the current slide has an animated GIF image.
    fn current_slide_has_gif(&self) -> bool {
        self.slides[self.current].image.as_ref()
            .map(|img| self.gif_frames.contains_key(&img.path))
            .unwrap_or(false)
    }

    /// Advance the GIF frame if the current frame's delay has elapsed.
    /// Returns true if the frame changed (needs redraw).
    fn advance_gif_frame(&mut self) -> bool {
        let img_path = match self.slides[self.current].image.as_ref() {
            Some(img) => &img.path,
            None => return false,
        };
        let frames = match self.gif_frames.get(img_path) {
            Some(f) => f,
            None => return false,
        };
        let current_delay = frames[self.gif_current_frame].delay_ms as u64;
        if self.gif_last_advance.elapsed().as_millis() as u64 >= current_delay {
            self.gif_current_frame = (self.gif_current_frame + 1) % frames.len();
            self.gif_last_advance = std::time::Instant::now();
            true
        } else {
            false
        }
    }
}

/// Truncate a string to at most `max` characters, adding "..." if truncated.
/// Used for labels in the overview grid and status bar where space is limited.
fn truncate_str(s: &str, max: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max {
        s.to_string()
    } else if max > 3 {
        let truncated: String = s.chars().take(max - 3).collect();
        format!("{}...", truncated)
    } else {
        s.chars().take(max).collect()
    }
}

/// Write text to the output buffer, applying OSC 66 text scaling when `scale >= 2`.
///
/// OSC 66 is a Kitty-specific escape sequence that tells the terminal to
/// render text at a larger size (2x, 3x, etc.) within a single cell span.
/// When scale is 1 (or 0), the text is written directly without any escape.
fn write_span_text(w: &mut impl Write, scale: u8, text: &str) -> Result<()> {
    if scale >= 2 {
        write!(w, "\x1b]66;s={};{}\x07", scale, text)?;
    } else {
        write!(w, "{}", text)?;
    }
    Ok(())
}

/// Truncate a string to fit within `max_cols` display columns.
///
/// Uses Unicode character widths (not byte count) so that wide characters
/// (CJK, emoji) are measured correctly. For example, a full-width character
/// counts as 2 columns.
fn truncate_to_width(s: &str, max_cols: usize) -> String {
    use unicode_width::UnicodeWidthChar;
    let mut result = String::new();
    let mut w = 0;
    for ch in s.chars() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
        if w + cw > max_cols {
            break;
        }
        result.push(ch);
        w += cw;
    }
    result
}

/// Simple word-wrapping: splits text at word boundaries to fit within `width` display columns.
///
/// Words that exceed the width on their own are placed on a single line without
/// breaking mid-word. Returns a `Vec<String>` where each entry is one wrapped line.
fn textwrap_simple(text: &str, width: usize) -> Vec<String> {
    if width == 0 || text.is_empty() {
        return vec![text.to_string()];
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in words {
        let test = if current.is_empty() { word.to_string() } else { format!("{} {}", current, word) };
        if unicode_width::UnicodeWidthStr::width(test.as_str()) > width && !current.is_empty() {
            lines.push(current);
            current = word.to_string();
        } else {
            current = test;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_to_width_ascii() {
        assert_eq!(truncate_to_width("hello world", 5), "hello");
        assert_eq!(truncate_to_width("hello", 10), "hello");
        assert_eq!(truncate_to_width("hello", 5), "hello");
        assert_eq!(truncate_to_width("", 5), "");
    }

    #[test]
    fn test_truncate_to_width_zero() {
        assert_eq!(truncate_to_width("hello", 0), "");
    }

    #[test]
    fn test_truncate_to_width_unicode() {
        // "→" is 1 display column but 3 bytes in UTF-8
        let arrow = "→";
        // "a→b" is 3 display columns; truncate to 2 gives "a→"
        let result = truncate_to_width(&format!("a{}b", arrow), 2);
        assert_eq!(result, format!("a{}", arrow));
        // truncate to 1 gives just "a"
        let result2 = truncate_to_width(&format!("a{}b", arrow), 1);
        assert_eq!(result2, "a");
    }

    #[test]
    fn test_textwrap_short_fits() {
        let result = textwrap_simple("hello world", 20);
        assert_eq!(result, vec!["hello world"]);
    }

    #[test]
    fn test_textwrap_wraps_at_width() {
        let result = textwrap_simple("hello world foo", 11);
        assert_eq!(result, vec!["hello world", "foo"]);
    }

    #[test]
    fn test_textwrap_long_word() {
        let result = textwrap_simple("superlongword short", 10);
        // Long word exceeds width but it's the first word, so it goes on its own line
        assert_eq!(result, vec!["superlongword", "short"]);
    }

    #[test]
    fn test_textwrap_empty() {
        let result = textwrap_simple("", 10);
        assert_eq!(result, vec![""]);
    }

    #[test]
    fn test_textwrap_zero_width() {
        let result = textwrap_simple("hello", 0);
        assert_eq!(result, vec!["hello"]);
    }

    #[test]
    fn test_textwrap_uses_display_width() {
        // Verify wrapping uses display columns not byte count
        // "→" is 1 display column but 3 bytes in UTF-8
        let text = "a b c d e f";
        let result = textwrap_simple(text, 5);
        assert_eq!(result[0], "a b c");
        assert_eq!(result[1], "d e f");
    }

    #[test]
    fn test_global_scale_range() {
        // Verify scale math: content_width at various scales
        let tw = 100usize;
        for scale in [50u8, 80, 100, 150, 200] {
            let content_width = scaled_content_width(tw, scale);
            assert!(content_width <= tw, "scale {} produced width {} > {}", scale, content_width, tw);
            assert!(content_width >= 50, "scale {} produced width {} < 50", scale, content_width);
        }
    }

    #[test]
    fn test_column_code_wrapping() {
        use crate::render::text::StyledSpan;
        // Simulate the column code wrapping logic from render_columns
        // with a line that exceeds the column width
        let code_content_width = 20usize;
        let long_line = "abcdefghijklmnopqrstuvwxyz0123456789"; // 36 chars

        // Simulate what happens with a single highlighted span
        let mut col_rows: Vec<Vec<StyledSpan>> = Vec::new();
        let mut spans: Vec<StyledSpan> = Vec::new();
        spans.push(StyledSpan::new("    ")); // left padding
        let mut char_count = 0usize;

        let txt = long_line;
        let mut offset = 0usize;
        let chars: Vec<char> = txt.chars().collect();
        while offset < chars.len() {
            let remaining = code_content_width.saturating_sub(char_count);
            if remaining == 0 {
                col_rows.push(spans);
                spans = Vec::new();
                spans.push(StyledSpan::new("    "));
                char_count = 0;
                continue;
            }
            let mut chunk = String::new();
            let mut chunk_w = 0usize;
            while offset < chars.len() {
                let cw = unicode_width::UnicodeWidthChar::width(chars[offset]).unwrap_or(0);
                if chunk_w + cw > remaining { break; }
                chunk.push(chars[offset]);
                chunk_w += cw;
                offset += 1;
            }
            if !chunk.is_empty() {
                spans.push(StyledSpan::new(&chunk));
                char_count += chunk_w;
            }
        }
        col_rows.push(spans); // final push

        // 36 chars with width 20: should produce 2 rows (20 + 16)
        assert_eq!(col_rows.len(), 2, "Expected 2 wrapped rows, got {}", col_rows.len());

        // First row: "    " + 20 chars = "abcdefghijklmnopqrst"
        let row0_text: String = col_rows[0].iter().map(|s| s.text.as_str()).collect();
        assert_eq!(row0_text, "    abcdefghijklmnopqrst");

        // Second row: "    " + 16 chars = "uvwxyz0123456789"
        let row1_text: String = col_rows[1].iter().map(|s| s.text.as_str()).collect();
        assert_eq!(row1_text, "    uvwxyz0123456789");
    }

    #[test]
    fn test_column_code_wrapping_multi_span() {
        use crate::render::text::StyledSpan;
        // Simulate multiple highlighted spans on one line (like the JSON highlighter produces)
        // e.g. 8 spaces + "Federated" + ": " + "arn:aws:iam::long-string"
        let code_content_width = 30usize;
        let spans_input: Vec<&str> = vec![
            "        ",          // 8 chars (indent)
            "\"Federated\"",     // 11 chars
            ": ",                // 2 chars
            "\"arn:aws:iam::ACCOUNT:oidc-provider/oidc.eks.REGION\"", // 51 chars
        ];
        // Total: 8 + 11 + 2 + 51 = 72 chars, should wrap into 3 rows at width 30

        let mut col_rows: Vec<Vec<StyledSpan>> = Vec::new();
        let mut current_spans: Vec<StyledSpan> = Vec::new();
        current_spans.push(StyledSpan::new("    ")); // left padding
        let mut char_count = 0usize;

        for span_text in &spans_input {
            let txt = span_text.trim_end_matches('\n');
            let mut offset = 0usize;
            let chars: Vec<char> = txt.chars().collect();
            while offset < chars.len() {
                let remaining = code_content_width.saturating_sub(char_count);
                if remaining == 0 {
                    col_rows.push(current_spans);
                    current_spans = Vec::new();
                    current_spans.push(StyledSpan::new("    "));
                    char_count = 0;
                    continue;
                }
                let mut chunk = String::new();
                let mut chunk_w = 0usize;
                while offset < chars.len() {
                    let cw = unicode_width::UnicodeWidthChar::width(chars[offset]).unwrap_or(0);
                    if chunk_w + cw > remaining { break; }
                    chunk.push(chars[offset]);
                    chunk_w += cw;
                    offset += 1;
                }
                if !chunk.is_empty() {
                    current_spans.push(StyledSpan::new(&chunk));
                    char_count += chunk_w;
                }
            }
        }
        col_rows.push(current_spans); // final push

        // 72 chars at width 30: row0=30, row1=30, row2=12 → 3 rows
        assert_eq!(col_rows.len(), 3, "Expected 3 wrapped rows, got {}. Rows: {:?}",
            col_rows.len(),
            col_rows.iter().map(|r| {
                let text: String = r.iter().map(|s| s.text.as_str()).collect();
                text
            }).collect::<Vec<_>>()
        );

        // Verify each row's content width (excluding 4-char padding) is <= code_content_width
        for (i, row) in col_rows.iter().enumerate() {
            let content_width: usize = row.iter().skip(1) // skip padding span
                .map(|s| unicode_width::UnicodeWidthStr::width(s.text.as_str()))
                .sum();
            assert!(content_width <= code_content_width,
                "Row {} content width {} exceeds code_content_width {}", i, content_width, code_content_width);
        }
    }

    #[test]
    fn test_scale_centering() {
        let tw = 100usize;
        let scale = 60u8;
        let content_width = scaled_content_width(tw, scale);
        let margin = tw.saturating_sub(content_width) / 2;
        let pad = " ".repeat(margin);
        assert_eq!(content_width, 60);
        assert_eq!(margin, 20);
        assert_eq!(pad.len(), 20);
        // Content should be centered: margin + content + margin = total
        assert!(margin + content_width + margin <= tw);
    }

    #[test]
    fn test_figfont_cached_produces_figlet_content_type() {
        let font_data = include_str!("../../../fonts/slant.flf");
        let fig = figlet_rs::FIGfont::from_content(font_data).ok();
        assert!(fig.is_some(), "FIGfont should load from bundled slant.flf");

        let fig = fig.unwrap();
        let rendered = fig.convert("Test");
        assert!(rendered.is_some(), "FIGfont should render 'Test'");

        let rendered_str = rendered.unwrap().to_string();
        let fig_lines: Vec<&str> = rendered_str.lines().collect();
        assert!(!fig_lines.is_empty(), "FIGlet output should have lines");

        // Verify lines have non-whitespace content (sparkle needs this)
        let has_content = fig_lines.iter().any(|l| l.chars().any(|c| !c.is_whitespace()));
        assert!(has_content, "FIGlet output should have non-whitespace characters");

        // Simulate what render_ascii_title does
        let mut lines: Vec<StyledLine> = Vec::new();
        for fig_line in &fig_lines {
            let mut line = StyledLine::empty();
            line.push(StyledSpan::new(fig_line).with_fg(Color::Green).bold());
            line.content_type = LineContentType::FigletTitle;
            lines.push(line);
        }

        // Verify content_type is preserved
        for line in &lines {
            assert_eq!(line.content_type, LineContentType::FigletTitle);
        }

        // Verify sparkle would animate these lines (target = "figlet")
        use crate::render::animation::{LoopAnimation, render_loop_frame};
        let sparkled = render_loop_frame(
            &lines, LoopAnimation::Sparkle, 42,
            Color::Green, Color::Black,
            80, 24,
            Some("figlet"),
        );
        assert_eq!(sparkled.len(), lines.len(), "Sparkle should preserve line count");

        // At frame 42, at least some cells should have sparkle characters
        let original_text: String = lines.iter()
            .flat_map(|l| l.spans.iter().map(|s| s.text.as_str()))
            .collect();
        let sparkled_text: String = sparkled.iter()
            .flat_map(|l| l.spans.iter().map(|s| s.text.as_str()))
            .collect();
        // Sparkle modifies some characters, so texts should differ
        assert_ne!(original_text, sparkled_text,
            "Sparkle should modify at least some characters at frame 42");
    }
}
