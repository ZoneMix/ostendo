//! The presenter: owns presentation state and drives the terminal.
//!
//! Each loop iteration composes the whole screen in memory (the slide itself
//! is laid out once and cached in a [`types::SlideFrame`]) and hands it to
//! [`display::Display`], which writes only the rows that changed.

mod actions;
mod ansi;
mod blocks;
mod chrome;
mod columns;
mod compose;
mod display;
mod figures;
mod font;
mod frame;
mod images;
mod input;
mod navigation;
mod palette;
mod state;
mod terminal;
mod types;

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::Receiver;
use std::time::Instant;

use anyhow::Result;
use crossterm::style::Color;

use crate::code::executor::Execution;
use crate::code::highlight::Highlighter;
use crate::presentation::{CodeBlock, PresentationMeta, Slide, StateManager};
use crate::remote::RemoteCommand;
use crate::render::animation::AnimationState;
use crate::render::layout::WindowSize;
use crate::render::text::StyledLine;
use crate::terminal::protocols::{self, TextScaleCapability};
use crate::theme::{Theme, ThemeRegistry};

pub use types::PresenterConfig;
use types::{Mode, SlideFrame};

pub struct Presenter {
    slides: Vec<Slide>,
    meta: PresentationMeta,
    presentation_path: PathBuf,
    current: usize,
    /// How many of the current slide's build steps have been shown.
    step: usize,
    mode: Mode,
    /// Text typed at the `:` or goto prompt.
    input: String,
    overview_sel: usize,

    width: u16,
    height: u16,
    window: WindowSize,
    scale: u8,
    image_scale_offset: i8,
    scroll: usize,
    max_scroll: usize,
    fullscreen: bool,
    /// Fullscreen for slides without a `<!-- fullscreen -->` directive.
    fullscreen_default: bool,
    show_notes: bool,
    notes_scroll: usize,
    show_theme_name: bool,
    show_sections: bool,
    timer_start: Option<Instant>,
    /// `b`: nothing on screen until the next key.
    blank: bool,
    /// A short message shown in the status bar, and when it was posted.
    notice: Option<(String, Instant)>,
    last_search: String,

    registry: ThemeRegistry,
    /// The theme the user chose; `theme` may differ on slides that override it.
    base_theme: Theme,
    theme: Theme,
    accent_override: Option<Color>,
    palette: palette::Palette,

    highlighter: Highlighter,
    figfont: figlet_rs::FIGfont,
    osc66: bool,
    images: images::ImageStore,
    font: font::FontControl,
    display: display::Display,
    /// Bumped whenever anything a slide frame depends on changes.
    generation: u64,
    frame_cache: Option<(frame::FrameKey, Rc<SlideFrame>)>,
    /// What was on screen last frame; the "before" side of a transition.
    last_lines: Vec<StyledLine>,
    animation: Option<AnimationState>,
    loop_started: Instant,

    allow_exec: bool,
    allow_remote_exec: bool,
    exec: Option<Execution>,
    exec_output: Option<String>,
    /// Ctrl+E index of the block `exec_output` belongs to.
    exec_block: usize,

    state: StateManager,
    watcher: Option<crate::watch::FileWatcher>,
    remote_rx: Option<Receiver<RemoteCommand>>,
    state_broadcast: Option<tokio::sync::broadcast::Sender<String>>,
    last_broadcast: String,
}

impl Presenter {
    pub fn new(cfg: PresenterConfig) -> Self {
        let state = StateManager::load(&cfg.presentation_path);
        let registry = ThemeRegistry::load();
        let saved_theme = state.get_theme_slug().and_then(|slug| registry.get(slug));
        let base_theme = match saved_theme {
            Some(theme) if !cfg.theme_explicit => theme,
            _ => cfg.theme,
        };
        let accent_override = crate::theme::colors::hex_to_color(&cfg.meta.accent);
        let palette = palette::Palette::new(&base_theme, accent_override);
        let current = cfg
            .start
            .unwrap_or_else(|| state.get_current_slide())
            .min(cfg.slides.len() - 1);
        let window = WindowSize::query();
        let protocol = cfg
            .image_protocol
            .unwrap_or_else(protocols::detect_protocol);
        let font = font::FontControl::new(
            protocols::detect_font_capability(),
            &cfg.slides,
            state.font_offsets(),
        );
        let fullscreen = cfg.slides[current].fullscreen.unwrap_or(cfg.fullscreen);
        let (remote_rx, state_broadcast) = match cfg.remote {
            Some((rx, tx)) => (Some(rx), Some(tx)),
            None => (None, None),
        };
        Self {
            images: images::ImageStore::new(protocol, &cfg.slides),
            meta: cfg.meta,
            watcher: Some(crate::watch::FileWatcher::new(
                cfg.presentation_path.clone(),
            )),
            presentation_path: cfg.presentation_path,
            current,
            step: 0,
            mode: Mode::Normal,
            input: String::new(),
            overview_sel: current,
            width: window.columns.max(1),
            height: window.rows.max(1),
            window,
            scale: cfg.scale.clamp(40, 100),
            image_scale_offset: state.get_image_scale_offset(),
            scroll: 0,
            max_scroll: 0,
            fullscreen,
            fullscreen_default: cfg.fullscreen,
            show_notes: false,
            notes_scroll: 0,
            show_theme_name: false,
            show_sections: false,
            timer_start: cfg.timer.then(Instant::now),
            blank: false,
            notice: None,
            last_search: String::new(),
            theme: base_theme.clone(),
            base_theme,
            registry,
            accent_override,
            palette,
            highlighter: Highlighter::new(),
            figfont: figlet_font(),
            osc66: protocols::detect_text_scale_capability() == TextScaleCapability::Osc66,
            font,
            display: display::Display::default(),
            generation: 0,
            frame_cache: None,
            last_lines: Vec::new(),
            animation: None,
            loop_started: Instant::now(),
            allow_exec: cfg.allow_exec,
            allow_remote_exec: cfg.allow_exec && cfg.allow_remote_exec,
            exec: None,
            exec_output: None,
            exec_block: 0,
            state,
            remote_rx,
            state_broadcast,
            last_broadcast: String::new(),
            slides: cfg.slides,
        }
    }

    /// Runs the presentation until the user quits.
    ///
    /// # Errors
    /// Fails if the terminal cannot be set up or written to.
    pub fn run(&mut self) -> Result<()> {
        let _guard = terminal::TerminalGuard::enter()?;
        terminal::on_exit(self.font.reset_escape());
        terminal::set_background(self.palette.bg);
        self.apply_slide_theme();
        self.font.request(Some(self.current));
        let result = self.event_loop();
        self.font.restore(&mut std::io::stdout());
        self.save_state();
        result
    }
}

fn figlet_font() -> figlet_rs::FIGfont {
    figlet_rs::FIGfont::from_content(include_str!("../../../fonts/slant.flf"))
        .or_else(|_| figlet_rs::FIGfont::standard())
        .expect("bundled FIGlet fonts parse")
}

/// Executable blocks in Ctrl+E order: slide-level first, then columns.
fn exec_blocks(slide: &Slide) -> Vec<&CodeBlock> {
    let columns = slide.columns.iter().flat_map(|c| &c.contents);
    slide
        .code_blocks
        .iter()
        .chain(columns.flat_map(|c| &c.code_blocks))
        .filter(|cb| cb.exec_mode.is_some())
        .collect()
}

#[cfg(test)]
mod tests;
