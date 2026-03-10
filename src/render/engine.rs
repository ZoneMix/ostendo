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
    AnimationState, AnimationKind, parse_transition, parse_entrance,
    parse_loop_animation, render_transition_frame, render_entrance_frame,
    render_loop_frame,
};
use crate::image_util::render::{RenderedImage, render_slide_image};
use crate::presentation::{ExecMode, PresentationMeta, Slide, SlideAlignment, StateManager};
use crate::render::layout::WindowSize;
use crate::render::progress::render_progress_bar;
use crate::render::text::{StyledLine, StyledSpan};
use crate::terminal::protocols::{self, ImageProtocol, FontSizeCapability, TextScaleCapability};
use crate::theme::colors::{hex_to_color, ensure_badge_contrast, interpolate_color};
use crate::theme::Theme;

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

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Normal,
    Command,
    Goto,
    Help,
    Overview,
}

/// Cached rendered image data.
enum CachedImage {
    Lines(Vec<StyledLine>),
    Protocol { escape_data: String, placeholder_height: usize },
}

pub struct Presenter {
    slides: Vec<Slide>,
    meta: PresentationMeta,
    theme: Theme,
    current: usize,
    mode: Mode,
    command_buf: String,
    goto_buf: String,
    show_notes: bool,
    notes_scroll: usize,
    show_fullscreen: bool,
    show_theme_name: bool,
    show_sections: bool,
    scroll_offset: usize,
    timer_start: Option<Instant>,
    width: u16,
    height: u16,
    highlighter: Highlighter,
    exec_output: Option<String>,
    exec_rx: Option<std::sync::mpsc::Receiver<Option<String>>>,
    exec_block_index: usize,
    state: StateManager,
    image_protocol: ImageProtocol,
    image_cache: HashMap<(PathBuf, usize, u8), CachedImage>,
    preloaded_images: HashMap<PathBuf, image::RgbaImage>,
    window_size: WindowSize,
    remote_rx: Option<std::sync::mpsc::Receiver<crate::remote::RemoteCommand>>,
    state_broadcast: Option<tokio::sync::broadcast::Sender<String>>,
    bg_color: Color,
    accent_color: Color,
    text_color: Color,
    code_bg_color: Color,
    slide_font_offsets: HashMap<usize, i8>,
    global_scale: u8,
    help_badge_bg: Color,
    font_capability: FontSizeCapability,
    original_font_size: Option<String>,
    file_watcher: Option<crate::watch::FileWatcher>,
    presentation_path: PathBuf,
    gradient_from: Option<Color>,
    gradient_to: Option<Color>,
    gradient_vertical: bool,
    is_light_variant: bool,
    active_animation: Option<crate::render::animation::AnimationState>,
    active_loop: Option<(crate::render::animation::LoopAnimation, u64)>,
    last_rendered_buffer: Vec<StyledLine>,
    mermaid_renderer: Option<crate::image_util::mermaid::MermaidRenderer>,
    /// True when fullscreen was toggled by user (f key), not by a slide directive.
    user_fullscreen_override: Option<bool>,
    // Font change deferred until inside BeginSynchronizedUpdate
    pending_font_size: Option<f64>,
    last_applied_font_size: Option<f64>,
    /// True when font change was triggered by slide navigation (fade out old content).
    /// False when triggered by `]`/`[` interactive adjustment (no fade).
    font_change_is_slide_transition: bool,
    /// OSC 66 text scaling capability (disabled pending rendering fix, but kept for detection)
    #[allow(dead_code)]
    text_scale_cap: TextScaleCapability,
    /// When true, the next render will dissolve-in the new content after flush.
    pending_dissolve_in: bool,
    // Smart redraw tracking
    last_rendered_slide: Option<usize>,
    last_rendered_scroll: usize,
    last_rendered_width: u16,
    last_rendered_height: u16,
    last_rendered_mode: Mode,
    last_rendered_scale: u8,
    last_rendered_image_scale: i8,
    needs_full_redraw: bool,
    image_scale_offset: i8,
}

impl Presenter {
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
                // Convert markdown font_size (1-7) to offset: (size - 1) * 2pt steps
                let offset = (md_size as i8 - 1) * 2;
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
        for slide in &slides {
            if let Some(ref img) = slide.image {
                if img.path.exists() && !preloaded_images.contains_key(&img.path) {
                    if let Ok(loaded) = crate::image_util::load_image(&img.path) {
                        preloaded_images.insert(img.path.clone(), loaded);
                    }
                }
            }
        }

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
            needs_full_redraw: true,
            image_scale_offset: 0,
            gradient_from,
            gradient_to,
            gradient_vertical,
            is_light_variant: false,
            active_animation: None,
            active_loop: None,
            last_rendered_buffer: Vec::new(),
            mermaid_renderer: None,
            user_fullscreen_override: None,
            pending_font_size: None,
            last_applied_font_size: None,
            font_change_is_slide_transition: false,
            text_scale_cap: protocols::detect_text_scale_capability(),
            pending_dissolve_in: false,
        };
        // Initialize mermaid renderer if any slide has mermaid blocks
        let has_mermaid = presenter.slides.iter().any(|s| !s.mermaid_blocks.is_empty());
        if has_mermaid && crate::image_util::mermaid::MermaidRenderer::is_available() {
            presenter.mermaid_renderer = Some(crate::image_util::mermaid::MermaidRenderer::new());
        }
        presenter
    }

    pub fn set_fullscreen(&mut self, fs: bool) { self.show_fullscreen = fs; }
    pub fn start_timer(&mut self) { self.timer_start = Some(Instant::now()); }
    pub fn set_default_scale(&mut self, scale: u8) {
        self.global_scale = scale;
    }

    /// Pre-render all slide images into the cache so navigation is instant.
    fn prerender_images(&mut self) {
        let tw = self.width as usize;
        let th = self.height as usize;
        let scale = self.current_scale();
        let content_width = ((tw as f64 * scale as f64 / 100.0) as usize).min(tw);

        for slide in &self.slides {
            if let Some(ref img) = slide.image {
                let effective_protocol = match img.render_mode {
                    crate::presentation::ImageRenderMode::Kitty => ImageProtocol::Kitty,
                    crate::presentation::ImageRenderMode::Iterm => ImageProtocol::Iterm2,
                    crate::presentation::ImageRenderMode::Sixel => ImageProtocol::Sixel,
                    crate::presentation::ImageRenderMode::Ascii => ImageProtocol::Ascii,
                    crate::presentation::ImageRenderMode::Auto => self.image_protocol,
                };
                let proto_key = match effective_protocol {
                    ImageProtocol::Kitty => 0,
                    ImageProtocol::Iterm2 => 1,
                    ImageProtocol::Sixel => 2,
                    ImageProtocol::Ascii => 3,
                };
                let cache_key = (img.path.clone(), content_width, proto_key);
                if !self.image_cache.contains_key(&cache_key) {
                    let margin = tw.saturating_sub(content_width) / 2;
                    let pad = " ".repeat(margin);
                    let preloaded = self.preloaded_images.get(&img.path);
                    let rendered = render_slide_image(
                        img, content_width, th / 2, &pad,
                        self.accent_color, self.text_color,
                        effective_protocol, self.bg_color,
                        &self.window_size, preloaded,
                    );
                    let cached = match rendered {
                        RenderedImage::Lines(l) => CachedImage::Lines(l),
                        RenderedImage::Protocol { escape_data, placeholder_height } => {
                            CachedImage::Protocol { escape_data, placeholder_height }
                        }
                    };
                    self.image_cache.insert(cache_key, cached);
                }
            }
        }
    }

    pub fn run(&mut self) -> Result<()> {
        self.prerender_images();
        // Apply initial slide's font offset (if restored from saved state)
        self.apply_slide_font();
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

    /// Persist current state (slide position, font offsets) to disk.
    fn save_state(&mut self) {
        self.state.set_current_slide(self.current);
        for (&slide, &offset) in &self.slide_font_offsets {
            self.state.set_font_offset(slide, offset);
        }
        let _ = self.state.save();
    }

    fn event_loop(&mut self) -> Result<()> {
        self.render_frame()?;
        self.broadcast_state();
        loop {
            // Dynamic poll timeout: 33ms when animation active (~30fps), 100ms otherwise
            let poll_ms = if self.active_animation.is_some() || self.active_loop.is_some() { 33 } else { 100 };
            let mut had_input = false;
            if event::poll(std::time::Duration::from_millis(poll_ms))? {
                // Drain ALL pending events before rendering (prevents mouse event flooding)
                loop {
                    match event::read()? {
                        Event::Key(key) => {
                            if self.handle_key(key)? {
                                return Ok(());
                            }
                            had_input = true;
                        }
                        Event::Mouse(mouse) => {
                            match mouse.kind {
                                MouseEventKind::ScrollUp => { self.scroll_up(3); had_input = true; }
                                MouseEventKind::ScrollDown => { self.scroll_down(3); had_input = true; }
                                _ => {} // ignore move/drag events
                            }
                        }
                        Event::Resize(w, h) => {
                            self.width = w;
                            self.height = h;
                            self.window_size = WindowSize::query();
                            self.needs_full_redraw = true;
                            had_input = true;
                        }
                        _ => {}
                    }
                    // Drain remaining events without blocking
                    if !event::poll(std::time::Duration::from_millis(0))? {
                        break;
                    }
                }
                if had_input {
                    self.render_frame()?;
                    self.broadcast_state();
                }
            } else if self.timer_start.is_some() && self.mode == Mode::Normal {
                self.render_frame()?;
                self.broadcast_state();
            }

            // Tick active animation
            if let Some(ref mut anim) = self.active_animation {
                anim.tick();
                if anim.is_done() {
                    // Chain: transition -> entrance animation if slide has one
                    if matches!(anim.kind, AnimationKind::Transition(_)) {
                        let slide = &self.slides[self.current];
                        if let Some(ea) = slide.entrance_animation.as_deref().and_then(parse_entrance) {
                            self.active_animation = Some(AnimationState::new_entrance(ea, Vec::new()));
                        } else {
                            self.active_animation = None;
                        }
                    } else {
                        self.active_animation = None;
                    }
                }
                self.needs_full_redraw = true;
                self.render_frame()?;
            }

            // Tick loop animation
            if let Some((_, ref mut frame)) = self.active_loop {
                *frame += 1;
                self.needs_full_redraw = true;
                // Only render loop when no transition/entrance is active
                if self.active_animation.is_none() {
                    self.render_frame()?;
                }
            }

            // Poll for streaming code execution output (only re-render in Normal mode)
            if self.mode == Mode::Normal && self.poll_exec_output() {
                self.needs_full_redraw = true;
                self.render_frame()?;
            }

            // Poll for file changes (hot reload)
            if let Some(ref watcher) = self.file_watcher {
                if watcher.check_modified() {
                    self.try_reload();
                    self.render_frame()?;
                }
            }

            // Poll for remote commands
            self.poll_remote()?;
        }
    }

    fn poll_remote(&mut self) -> Result<()> {
        // Take the receiver out to avoid borrow conflict with &mut self
        let rx = match self.remote_rx.take() {
            Some(rx) => rx,
            None => return Ok(()),
        };
        let mut got_command = false;
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                crate::remote::RemoteCommand::Next => self.next_slide(),
                crate::remote::RemoteCommand::Prev => self.prev_slide(),
                crate::remote::RemoteCommand::Goto(n) => self.goto_slide(n.saturating_sub(1)),
            }
            got_command = true;
        }
        // Put the receiver back
        self.remote_rx = Some(rx);
        if got_command {
            self.render_frame()?;
            self.broadcast_state();
        }
        Ok(())
    }

    fn broadcast_state(&self) {
        if let Some(ref tx) = self.state_broadcast {
            let slide = &self.slides[self.current];
            let mut content: Vec<String> = Vec::new();
            // Subtitle
            if !slide.subtitle.is_empty() {
                content.push(slide.subtitle.clone());
                content.push(String::new());
            }
            // Bullets
            for b in &slide.bullets {
                let indent = "  ".repeat(b.depth);
                content.push(format!("{}{}", indent, b.text));
            }
            // Code blocks
            for cb in &slide.code_blocks {
                content.push(String::new());
                if !cb.label.is_empty() {
                    content.push(format!("[{}]", cb.label));
                }
                for code_line in cb.code.lines() {
                    content.push(format!("  {}", code_line));
                }
            }
            // Block quotes
            for bq in &slide.block_quotes {
                content.push(String::new());
                for qline in &bq.lines {
                    content.push(format!("> {}", qline));
                }
            }
            // Tables
            for table in &slide.tables {
                content.push(String::new());
                content.push(table.headers.join(" | "));
                for row in &table.rows {
                    content.push(row.join(" | "));
                }
            }
            // Column content
            if let Some(ref cols) = slide.columns {
                for (i, col) in cols.contents.iter().enumerate() {
                    content.push(format!("--- Column {} ---", i + 1));
                    for b in &col.bullets {
                        let indent = "  ".repeat(b.depth);
                        content.push(format!("{}{}", indent, b.text));
                    }
                    for cb in &col.code_blocks {
                        for code_line in cb.code.lines() {
                            content.push(format!("  {}", code_line));
                        }
                    }
                }
            }
            let msg = crate::remote::StateMessage::new(
                self.current + 1,
                self.slides.len(),
                &slide.title,
                &slide.notes,
                &self.format_timer(),
                content,
            );
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = tx.send(json);
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<bool> {
        match self.mode {
            Mode::Command => return self.handle_command_key(key),
            Mode::Goto => return self.handle_goto_key(key),
            Mode::Help => {
                self.mode = Mode::Normal;
                return Ok(false);
            }
            Mode::Overview => {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('o') => {
                        self.mode = Mode::Normal;
                        self.needs_full_redraw = true;
                    }
                    KeyCode::Enter => {
                        self.mode = Mode::Normal;
                        self.needs_full_redraw = true;
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        if self.current < self.slides.len() - 1 { self.current += 1; }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        if self.current > 0 { self.current -= 1; }
                    }
                    KeyCode::Char('h') | KeyCode::Left => {
                        // Jump to same position in previous column
                        let th = self.height as usize;
                        let rows_per_col = (th.saturating_sub(5)) / 2;
                        if rows_per_col > 0 && self.current >= rows_per_col {
                            self.current -= rows_per_col;
                        }
                    }
                    KeyCode::Char('l') | KeyCode::Right => {
                        let th = self.height as usize;
                        let rows_per_col = (th.saturating_sub(5)) / 2;
                        if rows_per_col > 0 && self.current + rows_per_col < self.slides.len() {
                            self.current += rows_per_col;
                        }
                    }
                    _ => {}
                }
                return Ok(false);
            }
            Mode::Normal => {}
        }

        match key.code {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('h') | KeyCode::Left => self.prev_slide(),
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Char(' ') => self.next_slide(),
            KeyCode::Char('j') | KeyCode::Down => self.scroll_down(1),
            KeyCode::Char('k') | KeyCode::Up => self.scroll_up(1),
            KeyCode::Char('J') => self.next_section(),
            KeyCode::Char('K') => self.prev_section(),
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.scroll_down(self.height as usize / 2);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.scroll_up(self.height as usize / 2);
            }
            KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.execute_code()?;
            }
            KeyCode::Char('g') => {
                self.mode = Mode::Goto;
                self.goto_buf.clear();
            }
            KeyCode::Char('n') => { self.show_notes = !self.show_notes; self.notes_scroll = 0; self.needs_full_redraw = true; }
            KeyCode::Char('N') if self.show_notes => {
                self.notes_scroll += 1;
                self.needs_full_redraw = true;
            }
            KeyCode::Char('P') if self.show_notes => {
                self.notes_scroll = self.notes_scroll.saturating_sub(1);
                self.needs_full_redraw = true;
            }
            KeyCode::Char('f') => {
                self.show_fullscreen = !self.show_fullscreen;
                // Track user toggle so slide directives don't override it
                self.user_fullscreen_override = Some(self.show_fullscreen);
                self.needs_full_redraw = true;
            }
            KeyCode::Char('T') => { self.show_theme_name = !self.show_theme_name; self.needs_full_redraw = true; }
            KeyCode::Char('S') => { self.show_sections = !self.show_sections; self.needs_full_redraw = true; }
            KeyCode::Char('D') => {
                // Toggle light/dark theme variant
                let registry = crate::theme::ThemeRegistry::load();
                if let Some(variant) = registry.get_variant(&self.theme, !self.is_light_variant) {
                    self.is_light_variant = !self.is_light_variant;
                    self.apply_theme(variant);
                }
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                self.global_scale = (self.global_scale + 5).min(200);
                self.needs_full_redraw = true;
            }
            KeyCode::Char('-') => {
                self.global_scale = self.global_scale.saturating_sub(5).max(50);
                self.needs_full_redraw = true;
            }
            KeyCode::Char('>') => {
                self.image_scale_offset = (self.image_scale_offset + 10).min(100);
                self.needs_full_redraw = true;
            }
            KeyCode::Char('<') => {
                self.image_scale_offset = (self.image_scale_offset - 10).max(-90);
                self.needs_full_redraw = true;
            }
            KeyCode::Char(']') if self.font_capability.is_available() => {
                let cur = self.slide_font_offsets.get(&self.current).copied().unwrap_or(0);
                if cur < 20 {
                    self.slide_font_offsets.insert(self.current, cur + 1);
                    self.font_change_is_slide_transition = false;
                    self.apply_slide_font();
                    self.needs_full_redraw = true;
                    self.save_state();
                }
            }
            KeyCode::Char('[') if self.font_capability.is_available() => {
                let cur = self.slide_font_offsets.get(&self.current).copied().unwrap_or(0);
                if cur > -20 {
                    self.slide_font_offsets.insert(self.current, cur - 1);
                    self.font_change_is_slide_transition = false;
                    self.apply_slide_font();
                    self.needs_full_redraw = true;
                    self.save_state();
                }
            }
            KeyCode::Char('0') if key.modifiers.contains(KeyModifiers::CONTROL)
                || key.modifiers.contains(KeyModifiers::SUPER) => {
                self.slide_font_offsets.remove(&self.current);
                self.font_change_is_slide_transition = false;
                self.apply_slide_font();
                self.needs_full_redraw = true;
                self.save_state();
            }
            KeyCode::Char('o') => { self.mode = Mode::Overview; self.needs_full_redraw = true; }
            KeyCode::Char('?') => self.mode = Mode::Help,
            KeyCode::Char(':') => {
                self.mode = Mode::Command;
                self.command_buf.clear();
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_command_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => {
                let cmd = self.command_buf.clone();
                self.mode = Mode::Normal;
                self.execute_command(&cmd);
            }
            KeyCode::Backspace => { self.command_buf.pop(); }
            KeyCode::Char(c) => self.command_buf.push(c),
            _ => {}
        }
        Ok(false)
    }

    fn handle_goto_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => {
                if let Ok(n) = self.goto_buf.parse::<usize>() {
                    self.goto_slide(n.saturating_sub(1));
                }
                self.mode = Mode::Normal;
            }
            KeyCode::Char(c) if c.is_ascii_digit() => self.goto_buf.push(c),
            _ => {}
        }
        Ok(false)
    }

    fn execute_command(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd.trim().splitn(2, ' ').collect();
        match parts.first().copied() {
            Some("theme") => {
                if let Some(slug) = parts.get(1) {
                    let registry = crate::theme::ThemeRegistry::load();
                    if let Some(new_theme) = registry.get(slug.trim()) {
                        self.apply_theme(new_theme);
                    }
                }
            }
            Some("goto") => {
                if let Some(n) = parts.get(1).and_then(|s| s.trim().parse::<usize>().ok()) {
                    self.goto_slide(n.saturating_sub(1));
                }
            }
            Some("notes") => self.show_notes = !self.show_notes,
            Some("timer") => {
                if parts.get(1).map(|s| s.trim()) == Some("reset") {
                    self.timer_start = None;
                } else if self.timer_start.is_none() {
                    self.timer_start = Some(Instant::now());
                }
            }
            Some("overview") => self.mode = Mode::Overview,
            Some("help") => self.mode = Mode::Help,
            Some("reload") => self.try_reload(),
            _ => {}
        }
    }

    fn current_scale(&self) -> u8 {
        self.global_scale
    }

    /// Apply a theme, updating all cached color fields.
    fn apply_theme(&mut self, new_theme: Theme) {
        self.bg_color = hex_to_color(&new_theme.colors.background).unwrap_or(Color::Black);
        self.accent_color = hex_to_color(&new_theme.colors.accent).unwrap_or(Color::Green);
        self.text_color = hex_to_color(&new_theme.colors.text).unwrap_or(Color::White);
        self.code_bg_color = hex_to_color(&new_theme.colors.code_background).unwrap_or(Color::DarkGrey);
        // Presentation-level accent override persists across theme switches
        if !self.meta.accent.is_empty() {
            if let Some(c) = hex_to_color(&self.meta.accent) {
                self.accent_color = c;
            }
        }
        // Parse gradient
        if let Some(ref grad) = new_theme.gradient {
            self.gradient_from = hex_to_color(&grad.from);
            self.gradient_to = hex_to_color(&grad.to);
            self.gradient_vertical = grad.direction != "horizontal";
        } else {
            self.gradient_from = None;
            self.gradient_to = None;
        }
        self.help_badge_bg = ensure_badge_contrast(self.code_bg_color, self.bg_color);
        Self::set_terminal_bg(self.bg_color);
        self.theme = new_theme;
        self.image_cache.clear();
        self.needs_full_redraw = true;
    }

    /// Compute the background color for a given row, applying gradient if configured.
    fn row_bg_color(&self, row: usize, total_rows: usize) -> Color {
        if let (Some(from), Some(to)) = (self.gradient_from, self.gradient_to) {
            let t = if total_rows <= 1 { 0.0 } else { row as f64 / (total_rows - 1) as f64 };
            interpolate_color(from, to, t)
        } else {
            self.bg_color
        }
    }

    /// Send a Kitty set_font_size command directly via escape sequences.
    /// Uses no_response:true to prevent Kitty from sending responses that
    /// would pollute crossterm's terminal input stream.
    ///
    /// Kitty RC protocol: \x1bP@kitty-cmd{JSON}\x1b\\
    /// Payload: size (float), increment_op (null=absolute, "+"=add, "-"=subtract)
    ///
    /// When `flush` is false, the escape is written but not flushed — it will
    /// piggyback on the next render_frame() flush, avoiding a premature resize
    /// that causes flicker during slide transitions.
    fn kitty_font_size_absolute(&self, size: f64, flush: bool) {
        if self.font_capability != FontSizeCapability::KittyRemote {
            return;
        }
        let json = format!(
            r#"{{"cmd":"set_font_size","version":[0,14,2],"no_response":true,"payload":{{"size":{:.1}}}}}"#,
            size
        );
        let esc = format!("\x1bP@kitty-cmd{}\x1b\\", json);
        let _ = std::io::Write::write_all(&mut std::io::stdout(), esc.as_bytes());
        if flush {
            let _ = std::io::Write::flush(&mut std::io::stdout());
        }
    }

    /// Set Ghostty's font size to an absolute value via AppleScript keystroke
    /// simulation.  Resets to the config default first (Cmd+0), then sends
    /// the right number of Cmd+= or Cmd+- keystrokes to reach `target`.
    fn ghostty_set_font_size(&self, target: f64) {
        let base = self.original_font_size
            .as_ref()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(13.0);
        let delta = target - base;
        // Each Ghostty keystroke changes font by 1pt (default keybinding)
        let steps = delta.round() as i32;
        if steps == 0 {
            // Just reset to default
            let _ = std::process::Command::new("osascript")
                .args(["-e", r#"tell application "System Events" to keystroke "0" using {command down}"#])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            return;
        }
        let (key, count) = if steps > 0 {
            ("=", steps as u32)
        } else {
            ("-", (-steps) as u32)
        };
        // Build a single AppleScript that resets then sends N keystrokes
        let script = format!(
            r#"tell application "System Events"
  keystroke "0" using {{command down}}
  delay 0.02
  repeat {} times
    keystroke "{}" using {{command down}}
  end repeat
end tell"#,
            count, key
        );
        let _ = std::process::Command::new("osascript")
            .args(["-e", &script])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }

    /// Query Kitty's current font size at startup so we can restore it on exit.
    /// Tries `kitten @ get-font-size` first, then falls back to reading kitty.conf.
    fn query_kitty_font_size() -> Option<String> {
        // Try kitten @ get-font-size (requires allow_remote_control)
        if let Ok(output) = std::process::Command::new("kitten")
            .args(["@", "get-font-size"])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
        {
            if output.status.success() {
                let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !s.is_empty() && s.parse::<f64>().is_ok() {
                    return Some(s);
                }
            }
        }
        // Fallback: parse font_size from kitty.conf
        if let Some(home) = std::env::var_os("HOME") {
            let conf = std::path::Path::new(&home).join(".config/kitty/kitty.conf");
            if let Ok(content) = std::fs::read_to_string(conf) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("font_size") {
                        if let Some(val) = trimmed.strip_prefix("font_size") {
                            let val = val.trim();
                            if val.parse::<f64>().is_ok() {
                                return Some(val.to_string());
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Query Ghostty's configured font size by reading the config file.
    /// Ghostty config is at ~/.config/ghostty/config (key=value format).
    /// Default font size is 13pt if not configured.
    fn query_ghostty_font_size() -> Option<String> {
        if let Some(home) = std::env::var_os("HOME") {
            let conf = std::path::Path::new(&home).join(".config/ghostty/config");
            if let Ok(content) = std::fs::read_to_string(conf) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("font-size") {
                        // Ghostty uses "font-size = N" or "font-size=N"
                        if let Some(val) = trimmed.strip_prefix("font-size") {
                            let val = val.trim().trim_start_matches('=').trim();
                            if val.parse::<f64>().is_ok() {
                                return Some(val.to_string());
                            }
                        }
                    }
                }
            }
        }
        // Ghostty default is 13pt
        Some("13".to_string())
    }

    /// Restore font to original size captured at startup.
    fn reset_font_size(&self) {
        match self.font_capability {
            FontSizeCapability::KittyRemote => {
                if let Some(ref size) = self.original_font_size {
                    if let Ok(s) = size.parse::<f64>() {
                        self.kitty_font_size_absolute(s, true);
                    } else {
                        self.kitty_font_size_absolute(0.0, true);
                    }
                } else {
                    self.kitty_font_size_absolute(0.0, true);
                }
            }
            FontSizeCapability::GhosttyKeystroke => {
                // Cmd+0 resets to config default
                let _ = std::process::Command::new("osascript")
                    .args(["-e", r#"tell application "System Events" to keystroke "0" using {command down}"#])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
            FontSizeCapability::None => {}
        }
    }

    /// Set the terminal's default background color via OSC 11.
    /// This ensures any new cells created by terminal resizes (from font
    /// changes) inherit the theme bg instead of the terminal's default black.
    fn set_terminal_bg(color: Color) {
        if let Color::Rgb { r, g, b } = color {
            let esc = format!("\x1b]11;rgb:{:02x}/{:02x}/{:02x}\x1b\\", r, g, b);
            let _ = std::io::Write::write_all(&mut io::stdout(), esc.as_bytes());
            let _ = std::io::Write::flush(&mut io::stdout());
        }
    }

    /// Reset the terminal's default background color to its original value.
    fn reset_terminal_bg() {
        // OSC 111 resets to the terminal's configured default
        let _ = std::io::Write::write_all(&mut io::stdout(), b"\x1b]111\x1b\\");
        let _ = std::io::Write::flush(&mut io::stdout());
    }

    /// Compute the font size for the current slide and store it as pending.
    /// The actual escape sequence is written inside render_frame()'s
    /// BeginSynchronizedUpdate block so the font change and content arrive
    /// atomically, preventing flicker.
    fn apply_slide_font(&mut self) {
        if !self.font_capability.is_available() {
            return;
        }
        let offset = self.slide_font_offsets.get(&self.current).copied().unwrap_or(0);
        let target = if let Some(ref orig) = self.original_font_size {
            if let Ok(base) = orig.parse::<f64>() {
                base + (offset as f64 * 2.0)
            } else {
                0.0
            }
        } else {
            0.0
        };
        self.pending_font_size = Some(target);
    }

    /// Start animations for a slide change: transition, entrance, and/or loop.
    fn start_slide_animations(&mut self) {
        let slide = &self.slides[self.current];
        let old_buffer = self.last_rendered_buffer.clone();

        // Determine transition: per-slide directive overrides global meta
        let transition_str = slide.transition.as_deref()
            .or(if self.meta.transition.is_empty() { None } else { Some(self.meta.transition.as_str()) });

        if let Some(tt) = transition_str.and_then(parse_transition) {
            let has_entrance = slide.entrance_animation.as_deref()
                .and_then(parse_entrance).is_some();
            let mut anim = AnimationState::new_transition(tt, old_buffer, Vec::new());
            // When an entrance animation follows, the transition only fades out
            // old content — the entrance handles revealing new content.
            anim.exit_only = has_entrance;
            self.active_animation = Some(anim);
        } else if let Some(ea) = slide.entrance_animation.as_deref().and_then(parse_entrance) {
            self.active_animation = Some(AnimationState::new_entrance(ea, Vec::new()));
        }

        // Set up loop animation (runs independently after transition/entrance complete)
        self.active_loop = slide.loop_animation.as_deref()
            .and_then(parse_loop_animation)
            .map(|la| (la, 0));

        self.needs_full_redraw = true;
    }

    /// Reset transient state after changing slides.
    fn on_slide_changed(&mut self) {
        self.scroll_offset = 0;
        self.notes_scroll = 0;
        self.exec_output = None;
        self.exec_rx = None;
        self.exec_block_index = 0;
        self.font_change_is_slide_transition = true;
        // Apply per-slide fullscreen directive. User toggle (f key) is sticky
        // until the next slide change, then directives take control again.
        self.user_fullscreen_override = None;
        if let Some(fs) = self.slides[self.current].fullscreen {
            self.show_fullscreen = fs;
            self.needs_full_redraw = true;
        } else {
            // No directive: revert to non-fullscreen (default)
            if self.show_fullscreen {
                self.show_fullscreen = false;
                self.needs_full_redraw = true;
            }
        }
        self.apply_slide_font();
        self.start_slide_animations();
    }

    fn next_slide(&mut self) {
        if self.timer_start.is_none() {
            self.timer_start = Some(Instant::now());
        }
        if self.current < self.slides.len() - 1 {
            self.current += 1;
            self.on_slide_changed();
        }
    }

    fn prev_slide(&mut self) {
        if self.current > 0 {
            self.current -= 1;
            self.on_slide_changed();
        }
    }

    fn goto_slide(&mut self, idx: usize) {
        if idx < self.slides.len() {
            self.current = idx;
            self.on_slide_changed();
        }
    }

    fn next_section(&mut self) {
        let current_section = &self.slides[self.current].section;
        for i in (self.current + 1)..self.slides.len() {
            if self.slides[i].section != *current_section {
                self.current = i;
                self.on_slide_changed();
                return;
            }
        }
    }

    fn prev_section(&mut self) {
        let current_section = &self.slides[self.current].section;
        let mut section_start = self.current;
        while section_start > 0 && self.slides[section_start - 1].section == *current_section {
            section_start -= 1;
        }
        if section_start == 0 { return; }
        let prev_section = &self.slides[section_start - 1].section;
        let mut target = section_start - 1;
        while target > 0 && self.slides[target - 1].section == *prev_section {
            target -= 1;
        }
        self.current = target;
        self.on_slide_changed();
    }

    fn scroll_down(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(n);
    }

    fn scroll_up(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
    }

    fn execute_code(&mut self) -> Result<()> {
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

    fn format_timer(&self) -> String {
        match self.timer_start {
            Some(start) => {
                let elapsed = start.elapsed().as_secs();
                let h = elapsed / 3600;
                let m = (elapsed % 3600) / 60;
                let s = elapsed % 60;
                format!("{:02}:{:02}:{:02}", h, m, s)
            }
            None => "00:00:00".to_string(),
        }
    }

    // ── Rendering (buffered – no flicker) ──────────────────────────

    fn render_frame(&mut self) -> Result<()> {
        // If font size is changing, apply it before rendering and re-query
        // dimensions.  The ioctl(TIOCGWINSZ) returns the new size
        // synchronously after Kitty processes the font change, so we don't
        // need to sleep or wait for SIGWINCH.  We also drain any resize
        // events already queued so the event loop doesn't re-render again.
        let font_changing = if let Some(target) = self.pending_font_size.take() {
            if self.last_applied_font_size != Some(target) {
                Some(target)
            } else {
                None
            }
        } else {
            None
        };

        // Font change BEFORE the sync block — the terminal resize triggered
        // by font changes must settle before we query dimensions and render.
        if let Some(target) = font_changing {
            let mut font_applied = false;

            // ── Slide transition: scatter-dissolve interleaved with font stepping ──
            // The dissolve plays DURING the font change so the screen is never
            // blank.  Each 2-column group gets a pseudo-random dissolve time;
            // surviving characters dim progressively.  Font step batches are
            // sent between dissolve frames so the zoom and dissolve overlap.
            if self.font_change_is_slide_transition {
                let old_buf = self.last_rendered_buffer.clone();
                if !old_buf.is_empty() {
                    // Clear Kitty images before the transition
                    if self.image_protocol == ImageProtocol::Kitty {
                        let stdout = io::stdout();
                        let mut pre = stdout.lock();
                        pre.write_all(b"\x1b_Ga=d,d=a,q=2\x1b\\")?;
                        pre.flush()?;
                    }

                    // For Ghostty, fire the keystrokes now — the dissolve
                    // animation covers the processing delay.
                    if matches!(self.font_capability, FontSizeCapability::GhosttyKeystroke) {
                        self.ghostty_set_font_size(target);
                    }

                    // Build a full-screen buffer: status bar + separator + content.
                    // This lets the dissolve affect the entire screen uniformly.
                    let status_rows = if self.show_fullscreen { 0u16 } else { 2 };
                    let mut screen_buf: Vec<StyledLine> = Vec::new();
                    if status_rows > 0 {
                        let bar = self.build_status_bar(self.width as usize);
                        screen_buf.push(bar);
                        screen_buf.push(StyledLine::empty()); // separator
                    }
                    for line in &old_buf {
                        screen_buf.push(line.clone());
                    }

                    // Calculate Kitty font stepping parameters
                    let current_font = self.last_applied_font_size.unwrap_or(target);
                    let font_delta = target - current_font;
                    let num_font_steps = if font_delta.abs() > 0.3 {
                        (font_delta.abs() / 0.2).round() as usize
                    } else {
                        0
                    };
                    let font_dir = if font_delta >= 0.0 { 1.0_f64 } else { -1.0_f64 };

                    // Scale dissolve to cover font stepping, with a minimum
                    // duration so small font changes don't feel too abrupt.
                    // Target: ~400ms minimum, scaling up for large changes.
                    let font_step_time_ms = num_font_steps as u32 * 8;
                    let target_duration_ms = font_step_time_ms.max(400);
                    let dissolve_frames = (target_duration_ms / 30).clamp(12, 20);
                    let mut font_steps_sent = 0usize;

                    for frame in 1..=dissolve_frames {
                        let progress = frame as f64 / dissolve_frames as f64;

                        // Re-query terminal dimensions (font steps resize the terminal)
                        if frame > 1 && num_font_steps > 0 {
                            self.window_size = WindowSize::query();
                            self.width = self.window_size.columns;
                            self.height = self.window_size.rows;
                        }
                        let tw = self.width as usize;

                        // ── Render one dissolve frame ──
                        {
                            let stdout = io::stdout();
                            let mut fw = BufWriter::with_capacity(64 * 1024, stdout.lock());
                            queue!(fw, BeginSynchronizedUpdate)?;
                            for row in 0..self.height {
                                queue!(fw, cursor::MoveTo(0, row), SetBackgroundColor(self.bg_color))?;
                                if let Some(line) = screen_buf.get(row as usize) {
                                    let mut col = 0usize;
                                    for span in &line.spans {
                                        if col >= tw { break; }
                                        let span_bg = span.bg.unwrap_or(self.bg_color);
                                        let fg = span.fg.unwrap_or(self.text_color);
                                        // Fade fg toward span's own bg (handles inverted badges)
                                        let dimmed_fg = interpolate_color(fg, span_bg, progress * 0.7);
                                        // Fade span bg toward screen bg
                                        let dimmed_bg = interpolate_color(span_bg, self.bg_color, progress * 0.7);
                                        for ch in span.text.chars() {
                                            if col >= tw { break; }
                                            let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
                                            let group = col / 2;
                                            let hash = (row as u64).wrapping_mul(31)
                                                .wrapping_add(group as u64)
                                                .wrapping_mul(7919) % 1000;
                                            let threshold = hash as f64 / 1000.0;
                                            if threshold < progress {
                                                queue!(fw, SetBackgroundColor(self.bg_color))?;
                                                for _ in 0..cw { write!(fw, " ")?; }
                                            } else {
                                                queue!(fw, SetBackgroundColor(dimmed_bg))?;
                                                queue!(fw, SetForegroundColor(dimmed_fg))?;
                                                write!(fw, "{}", ch)?;
                                            }
                                            col += cw;
                                        }
                                    }
                                    if col < tw {
                                        for _ in 0..tw - col { write!(fw, " ")?; }
                                    }
                                } else {
                                    for _ in 0..tw { write!(fw, " ")?; }
                                }
                            }
                            queue!(fw, EndSynchronizedUpdate, ResetColor)?;
                            fw.flush()?;
                        }

                        // ── Interleave Kitty font step batch + pace the frame ──
                        let frame_target_ms = target_duration_ms / dissolve_frames;
                        let frame_start = std::time::Instant::now();

                        if num_font_steps > 0 && matches!(self.font_capability, FontSizeCapability::KittyRemote) {
                            let target_steps = ((num_font_steps as f64 * progress).round() as usize)
                                .min(num_font_steps);
                            let batch = target_steps - font_steps_sent;
                            if batch > 0 {
                                let stdout = io::stdout();
                                let mut pre = stdout.lock();
                                for s in 0..batch {
                                    let step_idx = font_steps_sent + s + 1;
                                    let intermediate = current_font + font_dir * 0.2 * step_idx as f64;
                                    let json = format!(
                                        r#"{{"cmd":"set_font_size","version":[0,14,2],"no_response":true,"payload":{{"size":{:.1}}}}}"#,
                                        intermediate
                                    );
                                    let esc = format!("\x1bP@kitty-cmd{}\x1b\\", json);
                                    pre.write_all(esc.as_bytes())?;
                                    pre.flush()?;
                                    std::thread::sleep(std::time::Duration::from_millis(8));
                                }
                                font_steps_sent = target_steps;
                            }
                        }

                        // Pad remaining time so the dissolve isn't too fast
                        let elapsed = frame_start.elapsed().as_millis() as u32;
                        if elapsed < frame_target_ms {
                            std::thread::sleep(std::time::Duration::from_millis(
                                (frame_target_ms - elapsed) as u64,
                            ));
                        }
                    }

                    // Final font step — land exactly on target
                    match self.font_capability {
                        FontSizeCapability::KittyRemote => {
                            let json = format!(
                                r#"{{"cmd":"set_font_size","version":[0,14,2],"no_response":true,"payload":{{"size":{:.1}}}}}"#,
                                target
                            );
                            let esc = format!("\x1bP@kitty-cmd{}\x1b\\", json);
                            let stdout = io::stdout();
                            let mut pre = stdout.lock();
                            pre.write_all(esc.as_bytes())?;
                            pre.flush()?;
                        }
                        _ => {} // Ghostty keystrokes already sent above
                    }
                    font_applied = true;
                    self.pending_dissolve_in = true;
                }
                self.font_change_is_slide_transition = false;
            }

            // ── Interactive ] / [ — plain font stepping, no dissolve ──
            if !font_applied {
                match self.font_capability {
                    FontSizeCapability::KittyRemote => {
                        let current = self.last_applied_font_size.unwrap_or(target);
                        let increasing = target > current;

                        let stdout = io::stdout();
                        let mut pre = stdout.lock();

                        if self.image_protocol == ImageProtocol::Kitty {
                            pre.write_all(b"\x1b_Ga=d,d=a,q=2\x1b\\")?;
                            pre.flush()?;
                        }

                        if increasing && (target - current).abs() > 0.3 {
                            let step = 0.2_f64;
                            let num_steps = ((target - current) / step).round() as usize;
                            for i in 1..num_steps {
                                let intermediate = current + step * i as f64;
                                let json = format!(
                                    r#"{{"cmd":"set_font_size","version":[0,14,2],"no_response":true,"payload":{{"size":{:.1}}}}}"#,
                                    intermediate
                                );
                                let esc = format!("\x1bP@kitty-cmd{}\x1b\\", json);
                                pre.write_all(esc.as_bytes())?;
                                pre.flush()?;
                                std::thread::sleep(std::time::Duration::from_millis(8));
                            }
                        }

                        let json = format!(
                            r#"{{"cmd":"set_font_size","version":[0,14,2],"no_response":true,"payload":{{"size":{:.1}}}}}"#,
                            target
                        );
                        let esc = format!("\x1bP@kitty-cmd{}\x1b\\", json);
                        pre.write_all(esc.as_bytes())?;
                        pre.flush()?;
                        drop(pre);
                    }
                    FontSizeCapability::GhosttyKeystroke => {
                        self.ghostty_set_font_size(target);
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                    FontSizeCapability::None => {}
                }
            }

            self.last_applied_font_size = Some(target);
            // Drain all resize events from the font change
            while event::poll(std::time::Duration::from_millis(10))? {
                if let Event::Resize(w2, h2) = event::read()? {
                    self.width = w2;
                    self.height = h2;
                } else {
                    break;
                }
            }
            self.window_size = WindowSize::query();
            self.width = self.window_size.columns;
            self.height = self.window_size.rows;
            self.needs_full_redraw = true;
        }

        let stdout = io::stdout();
        let mut w = BufWriter::with_capacity(256 * 1024, stdout.lock());
        queue!(w, BeginSynchronizedUpdate)?;

        match self.mode {
            Mode::Help => {
                self.last_rendered_mode = Mode::Help;
                return self.render_help_buf(&mut w);
            }
            Mode::Overview => {
                self.last_rendered_mode = Mode::Overview;
                return self.render_overview_buf(&mut w);
            }
            _ => {}
        }

        // Smart redraw: skip full re-render when nothing changed (timer-only ticks)
        let state_changed = self.needs_full_redraw
            || self.pending_dissolve_in
            || self.last_rendered_slide != Some(self.current)
            || self.last_rendered_scroll != self.scroll_offset
            || self.last_rendered_width != self.width
            || self.last_rendered_height != self.height
            || self.last_rendered_mode != self.mode
            || self.last_rendered_scale != self.global_scale
            || self.last_rendered_image_scale != self.image_scale_offset;

        if !state_changed && self.mode == Mode::Normal {
            // Only update status bar (timer display) without re-emitting images
            return self.render_status_bar_only(&mut w);
        }

        // Track whether we need to clear old Kitty images (slide change or redraw)
        let need_kitty_clear = self.image_protocol == ImageProtocol::Kitty
            && (self.last_rendered_slide != Some(self.current)
                || self.needs_full_redraw
                || self.last_rendered_scroll != self.scroll_offset);

        let slide = self.slides[self.current].clone();
        let tw = self.width as usize;
        let th = self.height as usize;

        let scale = self.current_scale();
        let content_width = ((tw as f64 * scale as f64 / 100.0) as usize).min(tw);
        let margin = tw.saturating_sub(content_width) / 2;
        let pad = " ".repeat(margin);

        // Build virtual buffer (status bar rendered separately to avoid flicker on scroll)
        let mut lines: Vec<StyledLine> = Vec::new();
        let status_bar_rows: usize = if !self.show_fullscreen { 2 } else { 0 };

        // Section (respects per-slide directive and global toggle)
        let show_section = slide.show_section.unwrap_or(self.show_sections);
        if show_section && !slide.section.is_empty() {
            let mut line = StyledLine::empty();
            line.push(StyledSpan::new(&pad));
            line.push(StyledSpan::new(&format!("Section: {}", slide.section)).with_fg(self.text_color).dim());
            lines.push(line);
            lines.push(StyledLine::empty());
        }

        // Title (with optional decoration)
        if !slide.title.is_empty() {
            let decoration = slide.title_decoration.as_deref()
                .or(self.theme.title_decoration.as_deref());
            if slide.ascii_title {
                self.render_ascii_title(&slide.title, &pad, &mut lines);
            } else if let Some(dec) = decoration {
                self.render_title_decorated(&slide.title, dec, content_width, &pad, &mut lines);
            } else {
                let mut line = StyledLine::empty();
                line.push(StyledSpan::new(&pad));
                // OSC 66 title scaling is disabled for now — multicell blocks get
                // destroyed during full redraws (e.g. fullscreen toggle). The data
                // model and parser support remain; re-enable by applying
                // slide.title_scale here when the rendering path is fixed.
                let title_span = StyledSpan::new(&slide.title).with_fg(self.accent_color).bold();
                line.push(title_span);
                lines.push(line);
            }
            lines.push(StyledLine::empty());
        }

        // Subtitle (wrapped to content width)
        if !slide.subtitle.is_empty() {
            let sub_width = content_width.saturating_sub(2);
            let wrapped_sub = textwrap_simple(&slide.subtitle, sub_width);
            // OSC 66 subtitle scaling disabled (same reason as title — see above).
            for wline in &wrapped_sub {
                let mut line = StyledLine::empty();
                line.push(StyledSpan::new(&pad));
                let subtitle_spans = crate::markdown::parser::parse_inline_formatting(
                    wline, self.text_color, self.code_bg_color,
                );
                for span in subtitle_spans {
                    line.push(span);
                }
                lines.push(line);
            }
            lines.push(StyledLine::empty());
        }

        // Bullets
        for bullet in &slide.bullets {
            let indent = match bullet.depth {
                0 => "  * ",
                1 => "      - ",
                _ => "          > ",
            };
            let wrapped = textwrap_simple(&bullet.text, content_width.saturating_sub(indent.len() + 2));
            for (i, wline) in wrapped.iter().enumerate() {
                let mut line = StyledLine::empty();
                line.push(StyledSpan::new(&pad));
                if i == 0 {
                    line.push(StyledSpan::new(indent).with_fg(self.accent_color));
                } else {
                    line.push(StyledSpan::new(&" ".repeat(indent.len())));
                }
                let inline_spans = crate::markdown::parser::parse_inline_formatting(
                    wline, self.text_color, self.code_bg_color,
                );
                for span in inline_spans {
                    line.push(span);
                }
                lines.push(line);
            }
        }
        if !slide.bullets.is_empty() {
            lines.push(StyledLine::empty());
        }

        // Column layouts
        if let Some(ref cols) = slide.columns {
            self.render_columns(cols, content_width, &pad, &mut lines);
            // Show exec output if columns have executable code blocks
            // Column exec blocks come after slide-level exec blocks in index order
            let slide_exec_count = slide.code_blocks.iter()
                .filter(|cb| cb.exec_mode.is_some()).count();
            let col_exec_blocks: Vec<&crate::presentation::CodeBlock> = cols.contents.iter()
                .flat_map(|c| c.code_blocks.iter())
                .filter(|cb| cb.exec_mode.is_some())
                .collect();
            if !col_exec_blocks.is_empty() {
                // Column exec block index starts after slide-level exec blocks
                let col_local_idx = self.exec_block_index.saturating_sub(slide_exec_count);
                if self.exec_block_index >= slide_exec_count && col_local_idx < col_exec_blocks.len() {
                    self.render_exec_output(&pad, &mut lines);
                }
            }
            lines.push(StyledLine::empty());
        }

        // Code blocks (presenterm-style: background rect with padding, no borders)
        let mut exec_render_idx: usize = 0;
        for cb in slide.code_blocks.iter() {
            let label = if cb.label.is_empty() { cb.language.clone() } else { cb.label.clone() };
            let inner_pad = 4; // 2 left + 2 right padding inside block
            let block_width = content_width;

            // Vertical padding top (empty line with code_bg)
            let mut vpad_top = StyledLine::empty();
            vpad_top.push(StyledSpan::new(&pad));
            vpad_top.push(StyledSpan::new(&" ".repeat(block_width)).with_bg(self.code_bg_color));
            lines.push(vpad_top);

            // Language label line (dimmed, like a comment)
            if !label.is_empty() {
                let comment_prefix = comment_prefix_for(&cb.language);
                let label_text = format!("  {}{}", comment_prefix, label);
                let label_width = unicode_width::UnicodeWidthStr::width(label_text.as_str());
                let label_pad = block_width.saturating_sub(label_width);
                let mut ll = StyledLine::empty();
                ll.push(StyledSpan::new(&pad));
                ll.push(StyledSpan::new(&label_text).with_fg(self.accent_color).with_bg(self.code_bg_color).dim());
                if label_pad > 0 {
                    ll.push(StyledSpan::new(&" ".repeat(label_pad)).with_bg(self.code_bg_color));
                }
                lines.push(ll);
            }

            // Highlighted code lines (truncated to block_width)
            let code_content_width = block_width.saturating_sub(inner_pad);
            let highlighted = self.highlighter.highlight(&cb.code, &cb.language);
            for hline in &highlighted {
                let mut line = StyledLine::empty();
                line.push(StyledSpan::new(&pad));
                line.push(StyledSpan::new("    ").with_bg(self.code_bg_color)); // left padding
                let mut line_char_count = inner_pad;
                for span in hline {
                    let txt = span.text.trim_end_matches('\n');
                    let span_w = unicode_width::UnicodeWidthStr::width(txt);
                    let remaining = code_content_width.saturating_sub(line_char_count.saturating_sub(inner_pad));
                    if remaining == 0 {
                        break;
                    }
                    if span_w <= remaining {
                        line.push(StyledSpan::new(txt)
                            .with_fg(span.fg)
                            .with_bg(self.code_bg_color));
                        line_char_count += span_w;
                    } else {
                        let truncated = truncate_to_width(txt, remaining);
                        let tw = unicode_width::UnicodeWidthStr::width(truncated.as_str());
                        line.push(StyledSpan::new(&truncated)
                            .with_fg(span.fg)
                            .with_bg(self.code_bg_color));
                        line_char_count += tw;
                        break;
                    }
                }
                // Pad to block_width with code_bg
                let pad_needed = block_width.saturating_sub(line_char_count);
                if pad_needed > 0 {
                    line.push(StyledSpan::new(&" ".repeat(pad_needed)).with_bg(self.code_bg_color));
                }
                lines.push(line);
            }

            // Vertical padding bottom (empty line with code_bg)
            let mut vpad_bot = StyledLine::empty();
            vpad_bot.push(StyledSpan::new(&pad));
            vpad_bot.push(StyledSpan::new(&" ".repeat(block_width)).with_bg(self.code_bg_color));
            lines.push(vpad_bot);

            // Exec mode indicator
            if cb.exec_mode.is_some() {
                let mut el = StyledLine::empty();
                el.push(StyledSpan::new(&pad));
                let mode_str = match cb.exec_mode {
                    Some(ExecMode::Exec) => "  [Ctrl+E to execute]",
                    Some(ExecMode::Pty) => "  [Ctrl+E to run in PTY]",
                    None => "",
                };
                el.push(StyledSpan::new(mode_str).with_fg(self.accent_color).dim());
                lines.push(el);
            }

            // Execution output (show only under the currently-executed block)
            if cb.exec_mode.is_some() {
                if exec_render_idx == self.exec_block_index {
                    self.render_exec_output(&pad, &mut lines);
                }
                exec_render_idx += 1;
            }
            lines.push(StyledLine::empty());
        }

        // Tables
        for table in &slide.tables {
            self.render_table(table, content_width, &pad, &mut lines);
            lines.push(StyledLine::empty());
        }

        // Block quotes (with text wrapping)
        for bq in &slide.block_quotes {
            let bq_prefix_width = 4; // "  │ "
            let bq_available = content_width.saturating_sub(bq_prefix_width + margin);
            for qline in &bq.lines {
                let wrapped = textwrap_simple(qline, bq_available);
                for wline in &wrapped {
                    let mut line = StyledLine::empty();
                    line.push(StyledSpan::new(&pad));
                    line.push(StyledSpan::new("  │ ").with_fg(self.accent_color).dim());
                    let inline_spans = crate::markdown::parser::parse_inline_formatting(
                        wline, self.text_color, self.code_bg_color,
                    );
                    for span in inline_spans {
                        line.push(span.italic());
                    }
                    lines.push(line);
                }
            }
            if !bq.lines.is_empty() {
                lines.push(StyledLine::empty());
            }
        }

        // Protocol images (Kitty/iTerm2) need escape data written after buffer flush
        let mut pending_protocol_images: Vec<(String, usize)> = Vec::new();

        // Mermaid diagrams
        for mermaid_block in &slide.mermaid_blocks {
            if let Some(ref mut renderer) = self.mermaid_renderer {
                // Use actual pixel width if available, else estimate at 2x for quality
                let pixel_width = if self.window_size.pixel_width > 0 {
                    self.window_size.pixel_width as usize
                } else {
                    content_width * 16
                };
                match renderer.render(&mermaid_block.source, pixel_width) {
                    Ok(png_path) => {
                        // Load and render the PNG as an image
                        let mermaid_img = crate::presentation::SlideImage {
                            path: png_path,
                            alt_text: String::from("Mermaid diagram"),
                            position: crate::presentation::ImagePosition::Below,
                            render_mode: crate::presentation::ImageRenderMode::Auto,
                            scale: 80,
                            color_override: String::new(),
                        };
                        let effective_protocol = self.image_protocol;
                        let img_max_height = th / 2;
                        let preloaded = self.preloaded_images.get(&mermaid_img.path);
                        let rendered = render_slide_image(
                            &mermaid_img, content_width, img_max_height, &pad,
                            self.accent_color, self.text_color,
                            effective_protocol, self.bg_color,
                            &self.window_size, preloaded,
                        );
                        match rendered {
                            RenderedImage::Lines(l) => lines.extend(l),
                            RenderedImage::Protocol { escape_data, placeholder_height } => {
                                let image_line_offset = lines.len();
                                for _ in 0..placeholder_height {
                                    lines.push(StyledLine::empty());
                                }
                                pending_protocol_images.push((escape_data, image_line_offset));
                            }
                        }
                    }
                    Err(_) => {
                        // Fallback: show source as visible code block with warning
                        lines.push(StyledLine::empty());
                        let mut warn = StyledLine::empty();
                        warn.push(StyledSpan::new(&pad));
                        warn.push(StyledSpan::new("  ┌─ Mermaid Diagram (render failed) ─┐").with_fg(self.accent_color));
                        lines.push(warn);
                        lines.push(StyledLine::empty());
                        let code_fg = Color::Rgb { r: 130, g: 200, b: 130 };
                        for src_line in mermaid_block.source.lines() {
                            let mut line = StyledLine::empty();
                            line.push(StyledSpan::new(&pad));
                            line.push(StyledSpan::new("  │ "));
                            line.push(StyledSpan::new(src_line).with_fg(code_fg));
                            lines.push(line);
                        }
                    }
                }
            } else {
                // No renderer available — show source as a visible code-like block
                lines.push(StyledLine::empty());
                let mut header = StyledLine::empty();
                header.push(StyledSpan::new(&pad));
                header.push(StyledSpan::new("  ┌─ Mermaid Diagram (install mmdc to render) ─┐").with_fg(self.accent_color));
                lines.push(header);
                lines.push(StyledLine::empty());
                let code_fg = Color::Rgb { r: 130, g: 200, b: 130 }; // green-ish for diagram source
                for src_line in mermaid_block.source.lines() {
                    let mut line = StyledLine::empty();
                    line.push(StyledSpan::new(&pad));
                    line.push(StyledSpan::new("  │ "));
                    line.push(StyledSpan::new(src_line).with_fg(code_fg));
                    lines.push(line);
                }
                lines.push(StyledLine::empty());
                let mut footer = StyledLine::empty();
                footer.push(StyledSpan::new(&pad));
                footer.push(StyledSpan::new("  └─ npm install -g @mermaid-js/mermaid-cli ──┘").with_fg(self.accent_color).dim());
                lines.push(footer);
            }
            lines.push(StyledLine::empty());
        }

        // Image rendering (cached)
        if let Some(ref img) = slide.image {
            // Per-image render mode override from markdown directives
            let effective_protocol = match img.render_mode {
                crate::presentation::ImageRenderMode::Kitty => ImageProtocol::Kitty,
                crate::presentation::ImageRenderMode::Iterm => ImageProtocol::Iterm2,
                crate::presentation::ImageRenderMode::Sixel => ImageProtocol::Sixel,
                crate::presentation::ImageRenderMode::Ascii => ImageProtocol::Ascii,
                crate::presentation::ImageRenderMode::Auto => self.image_protocol,
            };
            let proto_key = match effective_protocol {
                ImageProtocol::Kitty => 0,
                ImageProtocol::Iterm2 => 1,
                ImageProtocol::Sixel => 2,
                ImageProtocol::Ascii => 3,
            };
            // Apply image_scale directive + runtime offset
            let effective_scale = (img.scale as i16 + self.image_scale_offset as i16).clamp(5, 100) as u8;
            let img_width = (content_width as f64 * effective_scale as f64 / 100.0).max(1.0) as usize;
            let img_max_height = (th as f64 * effective_scale as f64 / 100.0 / 2.0).max(1.0) as usize;
            let cache_key = (img.path.clone(), img_width, proto_key);
            if !self.image_cache.contains_key(&cache_key) {
                let preloaded = self.preloaded_images.get(&img.path);
                let rendered = render_slide_image(
                    img, img_width, img_max_height, &pad,
                    self.accent_color, self.text_color,
                    effective_protocol, self.bg_color,
                    &self.window_size, preloaded,
                );
                let cached = match rendered {
                    RenderedImage::Lines(l) => CachedImage::Lines(l),
                    RenderedImage::Protocol { escape_data, placeholder_height } => {
                        CachedImage::Protocol { escape_data, placeholder_height }
                    }
                };
                self.image_cache.insert(cache_key.clone(), cached);
            }
            match self.image_cache.get(&cache_key) {
                Some(CachedImage::Lines(cached_lines)) => {
                    lines.extend(cached_lines.clone());
                }
                Some(CachedImage::Protocol { escape_data, placeholder_height }) => {
                    // Record line offset where image should be drawn
                    let image_line_offset = lines.len();
                    for _ in 0..*placeholder_height {
                        lines.push(StyledLine::empty());
                    }
                    pending_protocol_images.push((escape_data.clone(), image_line_offset));
                }
                None => {}
            }
            lines.push(StyledLine::empty());
        }

        // Calculate available display area (excluding status bar rows)
        let has_slide_footer = slide.footer.is_some();
        let reserved_bottom =
            if self.show_notes && !slide.notes.is_empty() { 7 } else { 0 }
            + if self.mode == Mode::Command || self.mode == Mode::Goto { 1 } else { 0 }
            + if has_slide_footer { 1 } else { 0 };
        let content_area = th.saturating_sub(status_bar_rows + reserved_bottom);

        // Vertical centering: per-slide alignment overrides global default_alignment
        let effective_alignment = slide.alignment
            .or(self.meta.default_alignment)
            .unwrap_or(SlideAlignment::Top);
        let do_vcenter = matches!(effective_alignment, SlideAlignment::Center | SlideAlignment::VCenter);
        let do_hcenter = matches!(effective_alignment, SlideAlignment::Center | SlideAlignment::HCenter);

        if do_vcenter && lines.len() < content_area {
            let padding_rows = (content_area - lines.len()) / 2;
            if padding_rows > 0 {
                let mut padded = Vec::with_capacity(lines.len() + padding_rows);
                for _ in 0..padding_rows {
                    padded.push(StyledLine::empty());
                }
                padded.append(&mut lines);
                lines = padded;
            }
        }

        // Horizontal centering: center each line's content within the terminal width.
        // Lines already start with `margin` spaces (the pad), so we subtract that
        // to get the actual content width, then compute the correct left offset.
        if do_hcenter {
            for line in &mut lines {
                let line_width: usize = line.spans.iter()
                    .map(|s| unicode_width::UnicodeWidthStr::width(s.text.as_str()))
                    .sum();
                // Content width is the line minus the existing left margin
                let content_text_width = line_width.saturating_sub(margin);
                if content_text_width > 0 && content_text_width < tw {
                    let desired_left = (tw.saturating_sub(content_text_width)) / 2;
                    if desired_left > margin {
                        let extra = desired_left - margin;
                        let mut centered = StyledLine::empty();
                        centered.push(StyledSpan::new(&" ".repeat(extra)));
                        for span in &line.spans {
                            centered.push(span.clone());
                        }
                        *line = centered;
                    }
                }
            }
        }

        // Apply animations to the buffer
        if let Some(ref mut anim) = self.active_animation {
            match anim.kind {
                AnimationKind::Transition(tt) => {
                    // Update new_buffer with current content
                    anim.new_buffer = lines.clone();
                    let progress = anim.progress();
                    lines = render_transition_frame(
                        &anim.old_buffer, &anim.new_buffer,
                        progress, tt, self.bg_color, content_width,
                        anim.exit_only,
                    );
                }
                AnimationKind::Entrance(ea) => {
                    anim.new_buffer = lines.clone();
                    let progress = anim.progress();
                    lines = render_entrance_frame(&anim.new_buffer, progress, ea, self.bg_color);
                }
                AnimationKind::Loop(_) => {
                    // Loops are handled below (separate from active_animation)
                }
            }
        }

        // Apply loop animation (runs independently, only when no transition/entrance active)
        // Use full terminal width (tw) so matrix/bounce fill edge-to-edge
        if self.active_animation.is_none() {
            if let Some((la, frame)) = self.active_loop {
                lines = render_loop_frame(
                    &lines, la, frame,
                    self.accent_color, self.bg_color,
                    tw, content_area,
                );
            }
        }

        // Cache current buffer for transition source on next slide change
        self.last_rendered_buffer = lines.clone();

        // Clamp scroll
        if lines.len() > content_area {
            let max_scroll = lines.len().saturating_sub(content_area);
            self.scroll_offset = self.scroll_offset.min(max_scroll);
        } else {
            self.scroll_offset = 0;
        }

        let visible_start = self.scroll_offset;
        let visible_end = (visible_start + content_area).min(lines.len());

        // ── Write buffered frame ──

        // Render fixed status bar at rows 0-1 (only when not scroll-only change)
        let scroll_only = !self.needs_full_redraw
            && self.last_rendered_slide == Some(self.current)
            && self.last_rendered_width == self.width
            && self.last_rendered_height == self.height
            && self.last_rendered_mode == self.mode
            && self.last_rendered_scale == self.global_scale
            && self.last_rendered_image_scale == self.image_scale_offset;

        if !scroll_only && !self.show_fullscreen {
            let bar = self.build_status_bar(tw);
            queue!(w, cursor::MoveTo(0, 0))?;
            self.queue_styled_line(&mut w, &bar, tw)?;
            queue!(w, cursor::MoveTo(0, 1), SetBackgroundColor(self.bg_color))?;
            write!(w, "{}", " ".repeat(tw))?;
        }

        // Render visible content lines (offset by status_bar_rows), with per-row gradient.
        // When dissolve-in is pending, render blank content (the dissolve loop
        // will progressively reveal it after this frame flushes).
        let has_gradient = self.gradient_from.is_some() && self.gradient_to.is_some();
        if self.pending_dissolve_in {
            for i in 0..content_area {
                let row = (status_bar_rows + i) as u16;
                queue!(w, cursor::MoveTo(0, row), SetBackgroundColor(self.bg_color))?;
                write!(w, "{}", " ".repeat(tw))?;
            }
        } else {
            for (i, line) in lines[visible_start..visible_end].iter().enumerate() {
                if line.is_scale_placeholder { continue; }
                let row = (status_bar_rows + i) as u16;
                queue!(w, cursor::MoveTo(0, row))?;
                if has_gradient {
                    let screen_row = visible_start + i;
                    let total = content_area.max(1);
                    let row_bg = self.row_bg_color(screen_row, total);
                    self.queue_styled_line_with_bg(&mut w, line, tw, row_bg)?;
                } else {
                    self.queue_styled_line(&mut w, line, tw)?;
                }
            }
        }

        // Fill remaining rows below content
        let content_rows_drawn = visible_end - visible_start;
        for i in content_rows_drawn..content_area {
            let row = (status_bar_rows + i) as u16;
            let fill_bg = if has_gradient {
                self.row_bg_color(visible_start + content_rows_drawn + (i - content_rows_drawn), content_area.max(1))
            } else {
                self.bg_color
            };
            queue!(w, cursor::MoveTo(0, row), SetBackgroundColor(fill_bg))?;
            write!(w, "{}", " ".repeat(tw))?;
        }

        // Per-slide custom footer bar (rendered at bottom of content area)
        if has_slide_footer {
            if let Some(ref footer_text) = slide.footer {
                use crate::presentation::FooterAlign;
                let footer_row = (status_bar_rows + content_area) as u16;
                let footer_bg = self.bg_color;
                queue!(w, cursor::MoveTo(0, footer_row), SetBackgroundColor(footer_bg))?;
                let text = footer_text.as_str();
                let text_width = unicode_width::UnicodeWidthStr::width(text);
                queue!(w, SetForegroundColor(self.accent_color))?;
                match slide.footer_align {
                    FooterAlign::Left => {
                        let pad_right = tw.saturating_sub(text_width + 1);
                        write!(w, " {}{}", text, " ".repeat(pad_right))?;
                    }
                    FooterAlign::Center => {
                        let pad_left = tw.saturating_sub(text_width) / 2;
                        let pad_right = tw.saturating_sub(pad_left + text_width);
                        write!(w, "{}{}{}", " ".repeat(pad_left), text, " ".repeat(pad_right))?;
                    }
                    FooterAlign::Right => {
                        let pad_left = tw.saturating_sub(text_width + 1);
                        write!(w, "{}{} ", " ".repeat(pad_left), text)?;
                    }
                }
            }
        }

        // Notes panel (fills entire reserved area with background)
        if self.show_notes && !slide.notes.is_empty() {
            let notes_rows = 6usize; // 1 separator + 5 content rows = 6, +1 reserved
            let notes_y = (th as u16).saturating_sub(7);

            // Separator line
            queue!(w, cursor::MoveTo(0, notes_y), SetBackgroundColor(self.code_bg_color), SetForegroundColor(self.accent_color))?;
            let all_note_lines: Vec<&str> = slide.notes.lines().collect();
            let scroll_indicator = if all_note_lines.len() > notes_rows {
                let max_scroll = all_note_lines.len().saturating_sub(notes_rows);
                self.notes_scroll = self.notes_scroll.min(max_scroll);
                format!(" [{}/{}] N/P scroll", self.notes_scroll + 1, max_scroll + 1)
            } else {
                self.notes_scroll = 0;
                String::new()
            };
            let sep: String = format!("─── Notes{} {}", scroll_indicator, "─".repeat(tw))
                .chars().take(tw).collect();
            let sep_pad = tw.saturating_sub(sep.chars().count());
            write!(w, "{}{}", sep, " ".repeat(sep_pad))?;

            // Content rows (scrollable, fill all 6 remaining rows)
            let visible_notes: Vec<&str> = all_note_lines
                .iter()
                .skip(self.notes_scroll)
                .take(notes_rows)
                .copied()
                .collect();
            for i in 0..notes_rows {
                queue!(w, cursor::MoveTo(0, notes_y + 1 + i as u16), SetBackgroundColor(self.code_bg_color), SetForegroundColor(self.text_color))?;
                if let Some(note_line) = visible_notes.get(i) {
                    let truncated: String = note_line.chars().take(tw.saturating_sub(2)).collect();
                    let trunc_cols = truncated.chars().count();
                    write!(w, " {}{}", truncated, " ".repeat(tw.saturating_sub(trunc_cols + 2)))?;
                } else {
                    write!(w, "{}", " ".repeat(tw))?;
                }
            }
        }

        // Command bar
        if self.mode == Mode::Command {
            let y = th as u16 - 1;
            queue!(w, cursor::MoveTo(0, y), SetBackgroundColor(self.code_bg_color), SetForegroundColor(self.accent_color))?;
            write!(w, ":{}{}", self.command_buf, " ".repeat(tw.saturating_sub(self.command_buf.len() + 1)))?;
        }

        // Goto indicator
        if self.mode == Mode::Goto {
            let y = th as u16 - 1;
            queue!(w, cursor::MoveTo(0, y), SetBackgroundColor(self.code_bg_color), SetForegroundColor(self.accent_color))?;
            write!(w, "goto: {}{}", self.goto_buf, " ".repeat(tw.saturating_sub(self.goto_buf.len() + 7)))?;
        }

        // Clear old Kitty images right before placing new content, so the
        // delete and new frame appear atomically within the synchronized update.
        if need_kitty_clear {
            write!(w, "\x1b_Ga=d,d=a,q=2\x1b\\")?;
        }

        // Write protocol image data after line rendering (Kitty/iTerm2/Sixel).
        // Skip during dissolve-in — images will render on the next normal frame.
        if !self.pending_dissolve_in {
            for (escape_data, line_offset) in &pending_protocol_images {
                if *line_offset >= visible_start && *line_offset < visible_end {
                    let display_row = line_offset - visible_start;
                    let screen_row = (status_bar_rows + display_row) as u16;
                    queue!(w, cursor::MoveTo(0, screen_row))?;
                    write!(w, "{}", escape_data)?;
                }
            }
        }

        queue!(w, EndSynchronizedUpdate, ResetColor)?;
        w.flush()?;

        // Update smart redraw tracking
        self.last_rendered_slide = Some(self.current);
        self.last_rendered_scroll = self.scroll_offset;
        self.last_rendered_width = self.width;
        self.last_rendered_height = self.height;
        self.last_rendered_mode = self.mode;
        self.last_rendered_scale = self.global_scale;
        self.last_rendered_image_scale = self.image_scale_offset;
        self.needs_full_redraw = false;

        // ── Dissolve-in: scatter-reveal new content after font transition ──
        // Mirrors the dissolve-out so the transition feels symmetric.
        // Images are emitted on the final frame within the same sync block
        // so they appear atomically with the fully-revealed content.
        if self.pending_dissolve_in {
            self.pending_dissolve_in = false;
            let dissolve_lines = self.last_rendered_buffer.clone();
            if !dissolve_lines.is_empty() {
                let dis_frames = 12u32;
                let dis_tw = self.width as usize;
                let dis_status = if self.show_fullscreen { 0u16 } else { 2 };
                let dis_content_rows = (self.height - dis_status) as usize;
                let dis_visible = dissolve_lines.len().min(dis_content_rows);
                for frame in 1..=dis_frames {
                    let progress = frame as f64 / dis_frames as f64;
                    let dim = (1.0 - progress) * 0.4;
                    let is_last = frame == dis_frames;
                    let stdout = io::stdout();
                    let mut dw = BufWriter::with_capacity(64 * 1024, stdout.lock());
                    queue!(dw, BeginSynchronizedUpdate)?;
                    // Status bar at full brightness
                    if dis_status > 0 {
                        let bar = self.build_status_bar(dis_tw);
                        queue!(dw, cursor::MoveTo(0, 0))?;
                        self.queue_styled_line(&mut dw, &bar, dis_tw)?;
                        queue!(dw, cursor::MoveTo(0, 1), SetBackgroundColor(self.bg_color))?;
                        for _ in 0..dis_tw { write!(dw, " ")?; }
                    }
                    // Content: per-cell scatter reveal
                    for (i, line) in dissolve_lines[..dis_visible].iter().enumerate() {
                        if line.is_scale_placeholder { continue; }
                        let row = (dis_status as usize + i) as u16;
                        queue!(dw, cursor::MoveTo(0, row), SetBackgroundColor(self.bg_color))?;
                        let mut col = 0usize;
                        for span in &line.spans {
                            if col >= dis_tw { break; }
                            let span_bg = span.bg.unwrap_or(self.bg_color);
                            let fg = span.fg.unwrap_or(self.text_color);
                            let dimmed_fg = interpolate_color(fg, span_bg, dim);
                            let dimmed_bg = interpolate_color(span_bg, self.bg_color, dim);
                            for ch in span.text.chars() {
                                if col >= dis_tw { break; }
                                let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
                                let group = col / 2;
                                let hash = (row as u64).wrapping_mul(31)
                                    .wrapping_add(group as u64)
                                    .wrapping_mul(7919) % 1000;
                                let threshold = hash as f64 / 1000.0;
                                if threshold < progress {
                                    queue!(dw, SetBackgroundColor(dimmed_bg),
                                               SetForegroundColor(dimmed_fg))?;
                                    write!(dw, "{}", ch)?;
                                } else {
                                    queue!(dw, SetBackgroundColor(self.bg_color))?;
                                    for _ in 0..cw { write!(dw, " ")?; }
                                }
                                col += cw;
                            }
                        }
                        if col < dis_tw {
                            queue!(dw, SetBackgroundColor(self.bg_color))?;
                            for _ in 0..dis_tw - col { write!(dw, " ")?; }
                        }
                    }
                    // Fill remaining rows
                    for i in dis_visible..dis_content_rows {
                        let row = (dis_status as usize + i) as u16;
                        queue!(dw, cursor::MoveTo(0, row), SetBackgroundColor(self.bg_color))?;
                        for _ in 0..dis_tw { write!(dw, " ")?; }
                    }
                    // Emit protocol images on the final frame so they appear
                    // atomically with fully-revealed content (no flicker).
                    if is_last {
                        for (escape_data, line_offset) in &pending_protocol_images {
                            if *line_offset >= visible_start && *line_offset < visible_end {
                                let display_row = line_offset - visible_start;
                                let screen_row = (status_bar_rows + display_row) as u16;
                                queue!(dw, cursor::MoveTo(0, screen_row))?;
                                write!(dw, "{}", escape_data)?;
                            }
                        }
                    }
                    queue!(dw, EndSynchronizedUpdate, ResetColor)?;
                    dw.flush()?;
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
            }
            // The dissolve-in already revealed content, so skip any remaining
            // transition/entrance animation to avoid double-reveal.
            self.active_animation = None;
            self.needs_full_redraw = true;
        }

        Ok(())
    }

    /// Redraw only the status bar line (for timer-only updates without re-emitting images).
    fn render_status_bar_only(&self, w: &mut impl Write) -> Result<()> {
        let tw = self.width as usize;
        if !self.show_fullscreen {
            queue!(w, cursor::MoveTo(0, 0))?;
            let bar = self.build_status_bar(tw);
            self.queue_styled_line(w, &bar, tw)?;
        }
        queue!(w, EndSynchronizedUpdate, ResetColor)?;
        w.flush()?;
        Ok(())
    }

    fn build_status_bar(&self, width: usize) -> StyledLine {
        let slide_info = format!(" Slide {}/{} ", self.current + 1, self.slides.len());
        let timer = format!(" {} ", self.format_timer());

        let theme_part = if self.show_theme_name {
            format!(" {} ", self.theme.name)
        } else {
            String::new()
        };

        // Global author/date — always present in top bar
        let footer_part = if !self.meta.author.is_empty() || !self.meta.date.is_empty() {
            let parts: Vec<&str> = [self.meta.author.as_str(), self.meta.date.as_str()]
                .iter()
                .filter(|s| !s.is_empty())
                .copied()
                .collect();
            format!(" {} ", parts.join(" · "))
        } else {
            String::new()
        };

        // Cap footer to prevent overflow
        let max_footer_len = width.saturating_sub(slide_info.len() + timer.len() + theme_part.len() + 10);
        let footer_part = if footer_part.len() > max_footer_len && max_footer_len > 4 {
            format!(" {}… ", &footer_part[1..max_footer_len.saturating_sub(2)])
        } else if footer_part.len() > max_footer_len {
            String::new()
        } else {
            footer_part
        };

        // Progress bar fills remaining space
        let footer_sep = if footer_part.is_empty() { 0 } else { 3 }; // " · "
        let fixed_len = slide_info.len() + timer.len() + theme_part.len() + footer_part.len() + footer_sep + 2;
        let bar_width = width.saturating_sub(fixed_len);
        let progress = render_progress_bar(self.current + 1, self.slides.len(), bar_width);

        let mut line = StyledLine::empty();
        line.push(StyledSpan::new(&slide_info).with_fg(self.bg_color).with_bg(self.accent_color).bold());
        line.push(StyledSpan::new(&timer).with_fg(self.text_color).with_bg(self.code_bg_color));
        if !theme_part.is_empty() {
            line.push(StyledSpan::new(&theme_part).with_fg(self.text_color).with_bg(self.code_bg_color).dim());
        }
        line.push(StyledSpan::new(&progress).with_fg(self.accent_color).with_bg(self.code_bg_color));
        if !footer_part.is_empty() {
            line.push(StyledSpan::new(" · ").with_fg(self.text_color).with_bg(self.code_bg_color).dim());
            line.push(StyledSpan::new(&footer_part).with_fg(self.text_color).with_bg(self.code_bg_color).dim());
        }
        // Fill any remaining space
        let used: usize = slide_info.len() + timer.len() + theme_part.len() + progress.len() + footer_part.len() + footer_sep;
        if used < width {
            line.push(StyledSpan::new(&" ".repeat(width - used)).with_bg(self.code_bg_color));
        }
        line
    }

    /// Render exec output lines into the buffer, wrapping long lines.
    fn render_exec_output(&self, pad: &str, lines: &mut Vec<StyledLine>) {
        if let Some(ref output) = self.exec_output {
            let prefix_width = pad.len() + 2; // pad + "  "
            let wrap_width = (self.width as usize).saturating_sub(prefix_width + 1);
            lines.push(StyledLine::empty());
            let mut oh = StyledLine::empty();
            oh.push(StyledSpan::new(pad));
            oh.push(StyledSpan::new("  Output:").with_fg(self.accent_color).bold());
            lines.push(oh);
            for ol in output.lines() {
                let sanitized = strip_control_chars(ol);
                if wrap_width > 0 && unicode_width::UnicodeWidthStr::width(sanitized.as_str()) > wrap_width {
                    // Wrap long lines by character width
                    let chars: Vec<char> = sanitized.chars().collect();
                    let mut pos = 0;
                    while pos < chars.len() {
                        let mut line = StyledLine::empty();
                        line.push(StyledSpan::new(pad));
                        line.push(StyledSpan::new("  "));
                        let mut chunk = String::new();
                        let mut w = 0;
                        while pos < chars.len() {
                            let cw = unicode_width::UnicodeWidthChar::width(chars[pos]).unwrap_or(0);
                            if w + cw > wrap_width { break; }
                            chunk.push(chars[pos]);
                            w += cw;
                            pos += 1;
                        }
                        line.push(StyledSpan::new(&chunk).with_fg(self.text_color));
                        lines.push(line);
                    }
                } else {
                    let mut line = StyledLine::empty();
                    line.push(StyledSpan::new(pad));
                    line.push(StyledSpan::new("  "));
                    line.push(StyledSpan::new(&sanitized).with_fg(self.text_color));
                    lines.push(line);
                }
            }
        }
    }

    fn render_ascii_title(&self, title: &str, pad: &str, lines: &mut Vec<StyledLine>) {
        let font_data = include_str!("../../fonts/slant.flf");
        let fig = match figlet_rs::FIGfont::from_content(font_data)
            .or_else(|_| figlet_rs::FIGfont::standard())
        {
            Ok(f) => f,
            Err(_) => {
                // Graceful fallback: render as plain bold title
                let mut line = StyledLine::empty();
                line.push(StyledSpan::new(pad));
                line.push(StyledSpan::new(title).with_fg(self.accent_color).bold());
                lines.push(line);
                return;
            }
        };
        let content_width = self.width as usize - pad.len();

        // Helper: check if rendered FIGlet fits within content_width
        let fits = |text: &str| -> Option<String> {
            fig.convert(text).and_then(|rendered| {
                let s = rendered.to_string();
                let max_w = s.lines().map(|l| l.chars().count()).max().unwrap_or(0);
                if max_w <= content_width { Some(s) } else { None }
            })
        };

        // Try full title first
        if let Some(rendered_str) = fits(title) {
            for fig_line in rendered_str.lines() {
                let mut line = StyledLine::empty();
                line.push(StyledSpan::new(pad));
                line.push(StyledSpan::new(fig_line).with_fg(self.accent_color).bold());
                lines.push(line);
            }
            return;
        }

        // Try splitting into words, rendering each word on its own FIGlet line
        let words: Vec<&str> = title.split_whitespace().collect();
        if words.len() > 1 {
            let mut all_fit = true;
            let mut word_renders: Vec<String> = Vec::new();
            for word in &words {
                if let Some(rendered_str) = fits(word) {
                    word_renders.push(rendered_str);
                } else {
                    all_fit = false;
                    break;
                }
            }
            if all_fit {
                for rendered_str in &word_renders {
                    for fig_line in rendered_str.lines() {
                        let mut line = StyledLine::empty();
                        line.push(StyledSpan::new(pad));
                        line.push(StyledSpan::new(fig_line).with_fg(self.accent_color).bold());
                        lines.push(line);
                    }
                }
                return;
            }
        }

        // Fallback: plain bold title when FIGlet doesn't fit
        let mut line = StyledLine::empty();
        line.push(StyledSpan::new(pad));
        line.push(StyledSpan::new(title).with_fg(self.accent_color).bold());
        lines.push(line);
    }

    /// Render a decorated title. Decoration styles: "underline", "box", "banner".
    fn render_title_decorated(
        &self,
        title: &str,
        decoration: &str,
        content_width: usize,
        pad: &str,
        lines: &mut Vec<StyledLine>,
    ) {
        let title_width = unicode_width::UnicodeWidthStr::width(title);
        match decoration {
            "underline" => {
                let mut tl = StyledLine::empty();
                tl.push(StyledSpan::new(pad));
                tl.push(StyledSpan::new(title).with_fg(self.accent_color).bold());
                lines.push(tl);
                let mut ul = StyledLine::empty();
                ul.push(StyledSpan::new(pad));
                ul.push(StyledSpan::new(&"─".repeat(title_width)).with_fg(self.accent_color));
                lines.push(ul);
            }
            "box" => {
                let box_w = title_width + 4; // 2 padding each side
                let top = format!("┌{}┐", "─".repeat(box_w.saturating_sub(2)));
                let mid = format!("│ {} │", title);
                let bot = format!("└{}┘", "─".repeat(box_w.saturating_sub(2)));
                for s in [&top, &mid, &bot] {
                    let mut line = StyledLine::empty();
                    line.push(StyledSpan::new(pad));
                    line.push(StyledSpan::new(s).with_fg(self.accent_color).bold());
                    lines.push(line);
                }
            }
            "banner" => {
                let banner_w = content_width;
                let text_pad = banner_w.saturating_sub(title_width + 2);
                let left = text_pad / 2;
                let right = text_pad - left;
                let banner_text = format!("{}{}{}", " ".repeat(left + 1), title, " ".repeat(right + 1));
                let mut line = StyledLine::empty();
                line.push(StyledSpan::new(pad));
                line.push(StyledSpan::new(&banner_text).with_fg(self.bg_color).with_bg(self.accent_color).bold());
                lines.push(line);
            }
            _ => {
                // "none" or unknown — plain title
                let mut line = StyledLine::empty();
                line.push(StyledSpan::new(pad));
                line.push(StyledSpan::new(title).with_fg(self.accent_color).bold());
                lines.push(line);
            }
        }
    }

    /// Render column layout content side-by-side
    fn render_table(
        &self,
        table: &crate::presentation::Table,
        content_width: usize,
        pad: &str,
        lines: &mut Vec<StyledLine>,
    ) {
        use crate::presentation::TableAlign;

        let num_cols = table.headers.len();
        if num_cols == 0 { return; }

        // Calculate column widths based on content
        let mut col_widths: Vec<usize> = table.headers.iter().map(|h| h.len()).collect();
        for row in &table.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < col_widths.len() {
                    col_widths[i] = col_widths[i].max(cell.len());
                }
            }
        }
        // Add padding (1 space each side)
        let col_widths: Vec<usize> = col_widths.iter().map(|w| w + 2).collect();
        let total_w: usize = col_widths.iter().sum::<usize>() + num_cols + 1; // +1 for borders
        // Ensure we don't exceed content_width
        let _ = total_w.min(content_width);

        // Helper to format a cell with alignment
        let fmt_cell = |text: &str, width: usize, align: TableAlign| -> String {
            let inner_w = width.saturating_sub(2); // minus padding
            let truncated = if text.len() > inner_w { &text[..inner_w] } else { text };
            let pad_total = inner_w.saturating_sub(truncated.len());
            match align {
                TableAlign::Right => format!(" {:>width$} ", truncated, width = inner_w),
                TableAlign::Center => {
                    let left_pad = pad_total / 2;
                    let right_pad = pad_total - left_pad;
                    format!(" {}{}{} ", " ".repeat(left_pad), truncated, " ".repeat(right_pad))
                }
                TableAlign::Left => format!(" {:<width$} ", truncated, width = inner_w),
            }
        };

        let get_align = |i: usize| -> TableAlign {
            table.alignments.get(i).copied().unwrap_or(TableAlign::Left)
        };

        // Top border: ┌───┬───┐
        let mut top = String::from("┌");
        for (i, w) in col_widths.iter().enumerate() {
            top.push_str(&"─".repeat(*w));
            if i < num_cols - 1 { top.push('┬'); } else { top.push('┐'); }
        }
        let mut tl = StyledLine::empty();
        tl.push(StyledSpan::new(pad));
        tl.push(StyledSpan::new("  "));
        tl.push(StyledSpan::new(&top).with_fg(self.accent_color).dim());
        lines.push(tl);

        // Header row: │ Name │ Value │
        let mut hl = StyledLine::empty();
        hl.push(StyledSpan::new(pad));
        hl.push(StyledSpan::new("  "));
        hl.push(StyledSpan::new("│").with_fg(self.accent_color).dim());
        for (i, header) in table.headers.iter().enumerate() {
            let cell = fmt_cell(header, col_widths[i], get_align(i));
            hl.push(StyledSpan::new(&cell).with_fg(self.accent_color).bold());
            hl.push(StyledSpan::new("│").with_fg(self.accent_color).dim());
        }
        lines.push(hl);

        // Header separator: ├───┼───┤
        let mut sep = String::from("├");
        for (i, w) in col_widths.iter().enumerate() {
            sep.push_str(&"─".repeat(*w));
            if i < num_cols - 1 { sep.push('┼'); } else { sep.push('┤'); }
        }
        let mut sl = StyledLine::empty();
        sl.push(StyledSpan::new(pad));
        sl.push(StyledSpan::new("  "));
        sl.push(StyledSpan::new(&sep).with_fg(self.accent_color).dim());
        lines.push(sl);

        // Data rows
        for row in &table.rows {
            let mut rl = StyledLine::empty();
            rl.push(StyledSpan::new(pad));
            rl.push(StyledSpan::new("  "));
            rl.push(StyledSpan::new("│").with_fg(self.accent_color).dim());
            for (i, cell) in row.iter().enumerate() {
                let w = if i < col_widths.len() { col_widths[i] } else { cell.len() + 2 };
                let formatted = fmt_cell(cell, w, get_align(i));
                rl.push(StyledSpan::new(&formatted).with_fg(self.text_color));
                rl.push(StyledSpan::new("│").with_fg(self.accent_color).dim());
            }
            lines.push(rl);
        }

        // Bottom border: └───┴───┘
        let mut bot = String::from("└");
        for (i, w) in col_widths.iter().enumerate() {
            bot.push_str(&"─".repeat(*w));
            if i < num_cols - 1 { bot.push('┴'); } else { bot.push('┘'); }
        }
        let mut bl = StyledLine::empty();
        bl.push(StyledSpan::new(pad));
        bl.push(StyledSpan::new("  "));
        bl.push(StyledSpan::new(&bot).with_fg(self.accent_color).dim());
        lines.push(bl);
    }

    fn render_columns(
        &self,
        cols: &crate::presentation::ColumnLayout,
        content_width: usize,
        pad: &str,
        lines: &mut Vec<StyledLine>,
    ) {
        let total_ratio: u16 = cols.ratios.iter().map(|&r| r as u16).sum();
        if total_ratio == 0 || cols.contents.is_empty() { return; }

        // Calculate column widths
        let separator_width = 3; // " | "
        let usable = content_width.saturating_sub(separator_width * (cols.ratios.len() - 1));
        let col_widths: Vec<usize> = cols.ratios.iter()
            .map(|&r| (usable as f64 * r as f64 / total_ratio as f64).floor() as usize)
            .collect();

        // Each column row: (spans, is_code) — styled spans instead of plain text
        let mut col_lines: Vec<Vec<(Vec<StyledSpan>, bool)>> = Vec::new();
        for (i, content) in cols.contents.iter().enumerate() {
            let cw = col_widths.get(i).copied().unwrap_or(20);
            let mut col_rows: Vec<(Vec<StyledSpan>, bool)> = Vec::new();

            // Bullets with inline formatting, themed markers, and word wrapping
            for bullet in &content.bullets {
                if bullet.text.is_empty() { continue; }
                let indent = match bullet.depth {
                    0 => "  * ",
                    1 => "      - ",
                    _ => "          > ",
                };
                let text_width = cw.saturating_sub(indent.len());
                if text_width == 0 { continue; }
                let wrapped = textwrap_simple(&bullet.text, text_width);
                for (wi, wline) in wrapped.iter().enumerate() {
                    let mut spans = Vec::new();
                    if wi == 0 {
                        spans.push(StyledSpan::new(indent).with_fg(self.accent_color));
                    } else {
                        spans.push(StyledSpan::new(&" ".repeat(indent.len())));
                    }
                    let inline_spans = crate::markdown::parser::parse_inline_formatting(
                        wline, self.text_color, self.code_bg_color,
                    );
                    for span in inline_spans {
                        spans.push(span);
                    }
                    col_rows.push((spans, false));
                }
            }

            // Add spacing between bullets and code blocks
            if !content.bullets.is_empty() && !content.code_blocks.is_empty() && !col_rows.is_empty() {
                col_rows.push((vec![StyledSpan::new(&" ".repeat(cw))], false));
            }

            // Code blocks with syntax highlighting
            for cb in &content.code_blocks {
                let inner_pad = 4usize;
                let code_content_width = cw.saturating_sub(inner_pad);

                // Vertical padding top
                col_rows.push((vec![StyledSpan::new(&" ".repeat(cw)).with_bg(self.code_bg_color)], true));

                // Language label
                let label = if cb.label.is_empty() { cb.language.clone() } else { cb.label.clone() };
                if !label.is_empty() {
                    let comment_prefix = comment_prefix_for(&cb.language);
                    let label_text = format!("  {}{}", comment_prefix, label);
                    col_rows.push((vec![
                        StyledSpan::new(&label_text).with_fg(self.accent_color).with_bg(self.code_bg_color).dim(),
                    ], true));
                }

                // Highlighted code lines — soft-wrap in columns
                let highlighted = self.highlighter.highlight(&cb.code, &cb.language);
                for hline in &highlighted {
                    let mut spans = Vec::new();
                    spans.push(StyledSpan::new("    ").with_bg(self.code_bg_color));
                    let mut char_count = 0usize;
                    for span in hline {
                        let txt = span.text.trim_end_matches('\n');
                        let mut offset = 0usize;
                        let chars: Vec<char> = txt.chars().collect();
                        while offset < chars.len() {
                            let remaining = code_content_width.saturating_sub(char_count);
                            if remaining == 0 {
                                // Push current line and start a new wrapped line
                                col_rows.push((spans, true));
                                spans = Vec::new();
                                spans.push(StyledSpan::new("    ").with_bg(self.code_bg_color));
                                char_count = 0;
                                continue;
                            }
                            // Take as many chars as fit in remaining width
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
                                spans.push(StyledSpan::new(&chunk).with_fg(span.fg).with_bg(self.code_bg_color));
                                char_count += chunk_w;
                            }
                        }
                    }
                    col_rows.push((spans, true));
                }

                // Vertical padding bottom
                col_rows.push((vec![StyledSpan::new(&" ".repeat(cw)).with_bg(self.code_bg_color)], true));

                // Exec mode indicator for column code blocks
                if cb.exec_mode.is_some() {
                    let mode_str = match cb.exec_mode {
                        Some(ExecMode::Exec) => "  [Ctrl+E to execute]",
                        Some(ExecMode::Pty) => "  [Ctrl+E to run in PTY]",
                        None => "",
                    };
                    col_rows.push((vec![
                        StyledSpan::new(mode_str).with_fg(self.accent_color).dim(),
                    ], false));
                }
            }
            col_lines.push(col_rows);
        }

        // Find max height
        let max_height = col_lines.iter().map(|c| c.len()).max().unwrap_or(0);

        // Merge side-by-side
        for row in 0..max_height {
            let mut line = StyledLine::empty();
            line.push(StyledSpan::new(pad));
            for (i, col) in col_lines.iter().enumerate() {
                let cw = col_widths.get(i).copied().unwrap_or(20);
                if let Some((spans, is_code)) = col.get(row) {
                    // Calculate display width of spans
                    let span_width: usize = spans.iter()
                        .map(|s| unicode_width::UnicodeWidthStr::width(s.text.as_str()))
                        .sum();
                    // Push styled spans
                    for span in spans {
                        line.push(span.clone());
                    }
                    // Pad remaining width
                    let pad_needed = cw.saturating_sub(span_width);
                    if pad_needed > 0 {
                        if *is_code {
                            line.push(StyledSpan::new(&" ".repeat(pad_needed)).with_bg(self.code_bg_color));
                        } else {
                            line.push(StyledSpan::new(&" ".repeat(pad_needed)));
                        }
                    }
                } else {
                    // Empty row — just pad
                    line.push(StyledSpan::new(&" ".repeat(cw)));
                }

                if i < col_lines.len() - 1 {
                    line.push(StyledSpan::new(" │ ").with_fg(self.accent_color).dim());
                }
            }
            lines.push(line);
        }
    }

    /// Write a styled line to a buffered writer, filling to full terminal width
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

    fn render_help_buf(&self, w: &mut impl Write) -> Result<()> {
        let tw = self.width as usize;
        let th = self.height as usize;

        // Clear any Kitty images so they don't show through the help overlay
        if self.image_protocol == ImageProtocol::Kitty {
            write!(w, "\x1b_Ga=d,d=a,q=2\x1b\\")?;
        }

        // Fill background for all rows
        for row in 0..th {
            queue!(w, cursor::MoveTo(0, row as u16), SetBackgroundColor(self.bg_color))?;
            write!(w, "{}", " ".repeat(tw))?;
        }

        // Section = "H" (header), "K" (key/desc), "S" (separator), "I" (info dim)
        let detected_proto = format!("{:?}", self.image_protocol);
        let slide_offset = self.slide_font_offsets.get(&self.current).copied().unwrap_or(0);
        let font_info = format!("slide {}: {:+} ({}pt/step)",
            self.current + 1, slide_offset, 2,
        );

        // Two-column layout
        let left_col: Vec<(&str, &str, &str)> = vec![
            ("H", "Navigation", ""),
            ("K", "h / ← / Backspace", "Previous slide"),
            ("K", "l / → / Space", "Next slide"),
            ("K", "j / ↓", "Scroll down"),
            ("K", "k / ↑", "Scroll up"),
            ("K", "J (shift)", "Next section"),
            ("K", "K (shift)", "Previous section"),
            ("K", "Ctrl+D / Ctrl+U", "Half page down/up"),
            ("K", "g + N + Enter", "Go to slide N"),
            ("S", "", ""),
            ("H", "Display", ""),
            ("K", "n", "Toggle speaker notes"),
            ("K", "f", "Toggle fullscreen (hide status bar)"),
            ("K", "T", "Toggle theme name in status"),
            ("K", "S", "Toggle section labels"),
            ("K", "?", "Show/hide this help"),
            ("K", "o", "Slide overview"),
            ("S", "", ""),
            ("H", "Font & Scale", ""),
            ("K", "+ / =", "Increase content scale"),
            ("K", "-", "Decrease content scale"),
            ("K", "> / <", "Increase/decrease image scale"),
            ("K", "] / [", "Increase/decrease font size"),
            ("K", "Ctrl/Cmd+0", "Reset font size"),
        ];

        let right_col: Vec<(&str, &str, &str)> = vec![
            ("H", "Code Execution", ""),
            ("K", "Ctrl+E", "Execute code block (+exec)"),
            ("K", "", "Auto-wrap: rust, c, c++, go"),
            ("K", "", "Also: python, bash, ruby, js"),
            ("S", "", ""),
            ("H", "Animations", ""),
            ("K", "<!-- transition: fade -->", "fade|slide|dissolve"),
            ("K", "<!-- animation: typewriter -->", "typewriter|fade_in|slide_down"),
            ("K", "<!-- loop_animation: pulse -->", "matrix|bounce|pulse|sparkle|spin"),
            ("S", "", ""),
            ("H", "Text Scaling (OSC 66)", ""),
            ("K", "<!-- title_scale: 3 -->", "Scale title (2-7x, Kitty)"),
            ("K", "<!-- text_scale: 2 -->", "Scale title+subtitle"),
            ("S", "", ""),
            ("H", "Layout Directives", ""),
            ("K", "<!-- fullscreen -->", "Hide status bar on slide"),
            ("K", "<!-- show_section: false -->", "Hide section label"),
            ("S", "", ""),
            ("H", "Commands (: mode)", ""),
            ("K", ":theme <slug>", "Switch theme"),
            ("K", ":goto <N>", "Jump to slide N"),
            ("K", ":notes", "Toggle notes panel"),
            ("K", ":timer / :timer reset", "Start/reset timer"),
            ("K", ":overview", "Slide overview grid"),
            ("K", ":help", "Show this help"),
            ("K", "q / Ctrl+C", "Quit"),
            ("S", "", ""),
            ("H", "CLI Flags", ""),
            ("K", "--theme <slug>", "Set presentation theme"),
            ("K", "--slide <N>", "Start at slide N"),
            ("K", "--image-mode <mode>", "auto|kitty|iterm|sixel|ascii"),
            ("K", "--remote", "Enable WebSocket remote control"),
            ("K", "--remote-port <N>", "Remote control port (default: 8765)"),
            ("K", "--validate", "Validate without running TUI"),
            ("K", "--list-themes", "List available themes"),
        ];

        // Title
        let title = "Ostendo Help";
        let title_x = (tw.saturating_sub(title.len())) / 2;
        queue!(w, cursor::MoveTo(title_x as u16, 1), SetForegroundColor(self.accent_color), SetAttribute(Attribute::Bold))?;
        write!(w, "{}", title)?;
        queue!(w, SetAttribute(Attribute::Reset))?;

        // Separator
        let sep_str = "─".repeat(tw.saturating_sub(8));
        queue!(w, cursor::MoveTo(4, 2), SetForegroundColor(self.accent_color), SetAttribute(Attribute::Dim))?;
        write!(w, "{}", sep_str)?;
        queue!(w, SetAttribute(Attribute::Reset))?;

        let start_y = 4u16;
        let left_x = 4u16;
        let col_width = (tw / 2).saturating_sub(4); // max chars per column with gap
        let right_x = (tw / 2) as u16;

        // Render helper: renders entries for a column, truncated to col_width
        macro_rules! render_entries {
            ($w:expr, $entries:expr, $x:expr, $max_w:expr) => {
                for (i, (kind, key, desc)) in $entries.iter().enumerate() {
                    let y = start_y + i as u16;
                    if y >= th as u16 - 1 { break; }
                    queue!($w, cursor::MoveTo($x, y))?;
                    match *kind {
                        "H" => {
                            queue!($w, SetBackgroundColor(self.bg_color), SetForegroundColor(self.accent_color), SetAttribute(Attribute::Bold))?;
                            let text = format!("▸ {}", key);
                            let truncated = truncate_to_width(&text, $max_w);
                            write!($w, "{}", truncated)?;
                            queue!($w, SetAttribute(Attribute::Reset))?;
                        }
                        "K" => {
                            let mut written = 0usize;
                            if !key.is_empty() {
                                let badge = format!(" {} ", key);
                                let badge_t = truncate_to_width(&badge, $max_w);
                                queue!($w, SetBackgroundColor(self.help_badge_bg), SetForegroundColor(self.accent_color))?;
                                write!($w, "{}", badge_t)?;
                                written += unicode_width::UnicodeWidthStr::width(badge_t.as_str());
                                queue!($w, SetBackgroundColor(self.bg_color))?;
                            }
                            if !desc.is_empty() && written < $max_w {
                                let desc_text = format!(" {}", desc);
                                let desc_t = truncate_to_width(&desc_text, $max_w - written);
                                queue!($w, SetForegroundColor(self.text_color))?;
                                write!($w, "{}", desc_t)?;
                            }
                            queue!($w, SetAttribute(Attribute::Reset))?;
                        }
                        "I" => {
                            queue!($w, SetForegroundColor(self.text_color), SetAttribute(Attribute::Dim))?;
                            let text = format!("  {}", desc);
                            let truncated = truncate_to_width(&text, $max_w);
                            write!($w, "{}", truncated)?;
                            queue!($w, SetAttribute(Attribute::Reset))?;
                        }
                        "S" => {
                            queue!($w, SetForegroundColor(self.accent_color), SetAttribute(Attribute::Dim))?;
                            write!($w, "{}", "─".repeat($max_w.min(30)))?;
                            queue!($w, SetAttribute(Attribute::Reset))?;
                        }
                        _ => {}
                    }
                }
            };
        }

        render_entries!(w, left_col, left_x, col_width);
        render_entries!(w, right_col, right_x, col_width);

        // Status info at the bottom
        let info_y = th as u16 - 4;
        let info_sep = "─".repeat(tw.saturating_sub(8));
        queue!(w, cursor::MoveTo(4, info_y), SetForegroundColor(self.accent_color), SetAttribute(Attribute::Dim))?;
        write!(w, "{}", info_sep)?;
        queue!(w, SetAttribute(Attribute::Reset))?;

        // Status info
        queue!(w, cursor::MoveTo(4, info_y + 1), SetBackgroundColor(self.bg_color), SetForegroundColor(self.text_color), SetAttribute(Attribute::Dim))?;
        write!(w, "Image protocol: ")?;
        queue!(w, SetAttribute(Attribute::NormalIntensity), SetForegroundColor(self.accent_color), SetAttribute(Attribute::Bold))?;
        write!(w, "{}", detected_proto)?;
        queue!(w, SetAttribute(Attribute::NoBold), SetAttribute(Attribute::Dim), SetForegroundColor(self.text_color))?;
        write!(w, "   Font size: ")?;
        queue!(w, SetAttribute(Attribute::NormalIntensity), SetForegroundColor(self.accent_color), SetAttribute(Attribute::Bold))?;
        write!(w, "{}", font_info)?;
        queue!(w, SetAttribute(Attribute::NoBold), SetAttribute(Attribute::Dim), SetForegroundColor(self.text_color))?;
        write!(w, "   Theme: ")?;
        queue!(w, SetAttribute(Attribute::NormalIntensity), SetForegroundColor(self.accent_color), SetAttribute(Attribute::Bold))?;
        write!(w, "{}", self.theme.name)?;
        queue!(w, SetAttribute(Attribute::Reset), SetBackgroundColor(self.bg_color))?;

        // Close hint
        queue!(w, cursor::MoveTo(4, info_y + 2), SetBackgroundColor(self.bg_color), SetForegroundColor(self.text_color), SetAttribute(Attribute::Dim))?;
        write!(w, "Press any key to close")?;
        queue!(w, SetAttribute(Attribute::Reset), SetBackgroundColor(self.bg_color))?;

        // Markdown directives help
        if tw > 100 {
            let dir_y = start_y + (left_col.len().max(right_col.len()) as u16) + 2;
            if dir_y < info_y - 2 {
                queue!(w, cursor::MoveTo(4, dir_y), SetForegroundColor(self.accent_color), SetAttribute(Attribute::Bold))?;
                write!(w, "▸ Markdown Directives")?;
                queue!(w, SetAttribute(Attribute::Reset))?;
                let directives = [
                    ("<!-- section: name -->", "Set slide section"),
                    ("<!-- timing: 1.0 -->", "Set timing in minutes"),
                    ("<!-- ascii_title -->", "Render title as FIGlet ASCII art"),
                    ("<!-- font_size: 2 -->", "Set font size (1-7, requires kitty)"),
                    ("<!-- column_layout: [1,1] -->", "Define column ratios"),
                    ("<!-- column: 0 -->", "Start column content"),
                    ("<!-- image_render: ascii|kitty|iterm|sixel -->", "Per-image render mode"),
                    ("<!-- notes: ... -->", "Speaker notes"),
                ];
                let dir_max = tw.saturating_sub(8);
                for (j, (dir, desc)) in directives.iter().enumerate() {
                    let dy = dir_y + 1 + j as u16;
                    if dy >= info_y - 1 { break; }
                    queue!(w, cursor::MoveTo(6, dy), SetBackgroundColor(self.help_badge_bg), SetForegroundColor(self.accent_color))?;
                    let badge = format!(" {} ", dir);
                    let badge_t = truncate_to_width(&badge, dir_max);
                    let badge_w = unicode_width::UnicodeWidthStr::width(badge_t.as_str());
                    write!(w, "{}", badge_t)?;
                    queue!(w, SetBackgroundColor(self.bg_color), SetForegroundColor(self.text_color))?;
                    if badge_w < dir_max {
                        let desc_t = truncate_to_width(&format!(" {}", desc), dir_max - badge_w);
                        write!(w, "{}", desc_t)?;
                    }
                }
                queue!(w, SetAttribute(Attribute::Reset))?;
            }
        }

        queue!(w, EndSynchronizedUpdate, ResetColor)?;
        w.flush()?;
        Ok(())
    }

    fn render_overview_buf(&self, w: &mut impl Write) -> Result<()> {
        let tw = self.width as usize;
        let th = self.height as usize;

        // Clear entire screen (this also clears any lingering protocol images)
        for row in 0..th {
            queue!(w, cursor::MoveTo(0, row as u16), SetBackgroundColor(self.bg_color))?;
            write!(w, "{}", " ".repeat(tw))?;
        }

        // Clear Kitty images if applicable
        if self.image_protocol == ImageProtocol::Kitty {
            write!(w, "\x1b_Ga=d\x1b\\")?;
        }

        queue!(w, cursor::MoveTo(2, 1), SetForegroundColor(self.accent_color), SetAttribute(Attribute::Bold))?;
        write!(w, "Slide Overview")?;
        queue!(w, SetAttribute(Attribute::Reset))?;

        // Two-column layout, reading top-down per column
        let num_cols = 2usize;
        let col_width = (tw - 6) / num_cols;
        let start_y = 3u16;
        let rows_per_col = (th.saturating_sub(5)) / 2; // 2 rows per entry (label + blank)
        let total_slots = rows_per_col * num_cols;

        for (i, slide) in self.slides.iter().enumerate() {
            if i >= total_slots { break; }

            // Top-down then left-to-right: column fills vertically first
            let col = i / rows_per_col;
            let row_in_col = i % rows_per_col;
            if col >= num_cols { break; }

            let x = 2 + col * (col_width + 2);
            let y = start_y + row_in_col as u16 * 2;
            if y >= self.height - 2 { break; }

            queue!(w, cursor::MoveTo(x as u16, y))?;
            if i == self.current {
                queue!(w, SetBackgroundColor(self.accent_color), SetForegroundColor(self.bg_color))?;
            } else {
                queue!(w, SetBackgroundColor(self.bg_color), SetForegroundColor(self.text_color))?;
            }

            let section = slide.section.as_str();
            let section_tag = if section.is_empty() {
                String::new()
            } else {
                format!(" [{}]", truncate_str(section, 10))
            };
            let max_title = col_width.saturating_sub(8 + section_tag.len());
            let label = format!(" {:>2}. {}{} ", i + 1, truncate_str(&slide.title, max_title), section_tag);
            write!(w, "{:<width$}", label, width = col_width)?;
            queue!(w, SetAttribute(Attribute::Reset))?;
        }

        queue!(w, cursor::MoveTo(2, self.height - 1), SetForegroundColor(self.accent_color), SetAttribute(Attribute::Dim))?;
        write!(w, "j/k: navigate  Enter: select  Esc: close")?;
        queue!(w, SetAttribute(Attribute::Reset), EndSynchronizedUpdate, ResetColor)?;
        w.flush()?;
        Ok(())
    }
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else if max > 3 {
        format!("{}...", &s[..max - 3])
    } else {
        s[..max].to_string()
    }
}

/// Truncate a string to fit within `max_cols` display columns.
/// Write text, wrapping with OSC 66 when `scale >= 2`.
fn write_span_text(w: &mut impl Write, scale: u8, text: &str) -> Result<()> {
    if scale >= 2 {
        write!(w, "\x1b]66;s={};{}\x07", scale, text)?;
    } else {
        write!(w, "{}", text)?;
    }
    Ok(())
}

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
            let content_width = ((tw as f64 * scale as f64 / 100.0) as usize).min(tw);
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
        let content_width = ((tw as f64 * scale as f64 / 100.0) as usize).min(tw);
        let margin = tw.saturating_sub(content_width) / 2;
        let pad = " ".repeat(margin);
        assert_eq!(content_width, 60);
        assert_eq!(margin, 20);
        assert_eq!(pad.len(), 20);
        // Content should be centered: margin + content + margin = total
        assert!(margin + content_width + margin <= tw);
    }
}
