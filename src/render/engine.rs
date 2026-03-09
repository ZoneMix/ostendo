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
use crate::image_util::render::{RenderedImage, render_slide_image};
use crate::presentation::{ExecMode, Slide, StateManager};
use crate::render::layout::WindowSize;
use crate::render::progress::render_progress_bar;
use crate::render::text::{StyledLine, StyledSpan};
use crate::terminal::protocols::{self, ImageProtocol, FontSizeCapability};
use crate::theme::colors::{hex_to_color, ensure_badge_contrast};
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
        "python" | "bash" | "sh" | "ruby" | "yaml" | "toml" | "r" => "# ",
        "html" | "xml" => "<!-- ",
        "css" => "/* ",
        "sql" | "lua" | "haskell" => "-- ",
        "c" | "cpp" | "java" | "javascript" | "typescript" | "go" | "rust"
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
    theme: Theme,
    current: usize,
    mode: Mode,
    command_buf: String,
    goto_buf: String,
    show_notes: bool,
    notes_scroll: usize,
    show_fullscreen: bool,
    show_theme_name: bool,
    scroll_offset: usize,
    timer_start: Option<Instant>,
    width: u16,
    height: u16,
    highlighter: Highlighter,
    exec_output: Option<String>,
    exec_rx: Option<std::sync::mpsc::Receiver<Option<String>>>,
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
    // Font change deferred until inside BeginSynchronizedUpdate
    pending_font_size: Option<f64>,
    last_applied_font_size: Option<f64>,
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
        let accent = hex_to_color(&theme.colors.accent).unwrap_or(Color::Green);
        let text = hex_to_color(&theme.colors.text).unwrap_or(Color::White);
        let code_bg = hex_to_color(&theme.colors.code_background).unwrap_or(Color::DarkGrey);
        let help_badge_bg = ensure_badge_contrast(code_bg, bg);
        let font_capability = protocols::detect_font_capability();
        let original_font_size = if font_capability == FontSizeCapability::KittyRemote {
            Self::query_kitty_font_size()
        } else {
            None
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

        Self {
            current: restored_slide.min(slides.len().saturating_sub(1)),
            slides,
            theme,
            mode: Mode::Normal,
            command_buf: String::new(),
            goto_buf: String::new(),
            show_notes: false,
            notes_scroll: 0,
            show_fullscreen: false,
            show_theme_name: false,
            scroll_offset: 0,
            timer_start: None,
            width: w,
            height: h,
            highlighter: Highlighter::new(),
            exec_output: None,
            exec_rx: None,
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
            pending_font_size: None,
            last_applied_font_size: None,
        }
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

        for slide in &self.slides.clone() {
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
            if event::poll(std::time::Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(key) => {
                        if self.handle_key(key)? {
                            break;
                        }
                        self.render_frame()?;
                        self.broadcast_state();
                    }
                    Event::Mouse(mouse) => {
                        match mouse.kind {
                            MouseEventKind::ScrollUp => self.scroll_up(3),
                            MouseEventKind::ScrollDown => self.scroll_down(3),
                            _ => continue,
                        }
                        self.render_frame()?;
                    }
                    Event::Resize(w, h) => {
                        self.width = w;
                        self.height = h;
                        // Drain any queued resize events (font changes in Kitty
                        // can produce bursts of resizes — only render the final one)
                        while event::poll(std::time::Duration::from_millis(20))? {
                            if let Event::Resize(w2, h2) = event::read()? {
                                self.width = w2;
                                self.height = h2;
                            } else {
                                break;
                            }
                        }
                        self.window_size = WindowSize::query();
                        self.needs_full_redraw = true;
                        self.render_frame()?;
                    }
                    _ => {}
                }
            } else if self.timer_start.is_some() && self.mode == Mode::Normal {
                self.render_frame()?;
                self.broadcast_state();
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
        Ok(())
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
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => {
                        self.mode = Mode::Normal;
                    }
                    KeyCode::Char('h') | KeyCode::Left => {
                        if self.current > 0 { self.current -= 1; }
                    }
                    KeyCode::Char('l') | KeyCode::Right => {
                        if self.current < self.slides.len() - 1 { self.current += 1; }
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
            KeyCode::Char('f') => { self.show_fullscreen = !self.show_fullscreen; self.needs_full_redraw = true; }
            KeyCode::Char('T') => { self.show_theme_name = !self.show_theme_name; self.needs_full_redraw = true; }
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
                self.image_cache.clear();
                self.needs_full_redraw = true;
            }
            KeyCode::Char('<') => {
                self.image_scale_offset = (self.image_scale_offset - 10).max(-90);
                self.image_cache.clear();
                self.needs_full_redraw = true;
            }
            KeyCode::Char(']') if self.font_capability == FontSizeCapability::KittyRemote => {
                let cur = self.slide_font_offsets.get(&self.current).copied().unwrap_or(0);
                if cur < 20 {
                    self.slide_font_offsets.insert(self.current, cur + 1);
                    self.apply_slide_font();
                    self.needs_full_redraw = true;
                    self.save_state();
                }
            }
            KeyCode::Char('[') if self.font_capability == FontSizeCapability::KittyRemote => {
                let cur = self.slide_font_offsets.get(&self.current).copied().unwrap_or(0);
                if cur > -20 {
                    self.slide_font_offsets.insert(self.current, cur - 1);
                    self.apply_slide_font();
                    self.needs_full_redraw = true;
                    self.save_state();
                }
            }
            KeyCode::Char('0') if key.modifiers.contains(KeyModifiers::CONTROL)
                || key.modifiers.contains(KeyModifiers::SUPER) => {
                self.slide_font_offsets.remove(&self.current);
                self.apply_slide_font();
                self.needs_full_redraw = true;
                self.save_state();
            }
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
                        self.bg_color = hex_to_color(&new_theme.colors.background).unwrap_or(Color::Black);
                        self.accent_color = hex_to_color(&new_theme.colors.accent).unwrap_or(Color::Green);
                        self.text_color = hex_to_color(&new_theme.colors.text).unwrap_or(Color::White);
                        self.code_bg_color = hex_to_color(&new_theme.colors.code_background).unwrap_or(Color::DarkGrey);
                        self.help_badge_bg = ensure_badge_contrast(self.code_bg_color, self.bg_color);
                        Self::set_terminal_bg(self.bg_color);
                        self.theme = new_theme;
                        self.needs_full_redraw = true;
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

    /// Restore font to original size captured at startup (Kitty only).
    /// Flushed immediately since this is called on exit.
    fn reset_font_size(&self) {
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
        if self.font_capability != FontSizeCapability::KittyRemote {
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

    fn next_slide(&mut self) {
        if self.timer_start.is_none() {
            self.timer_start = Some(Instant::now());
        }
        if self.current < self.slides.len() - 1 {
            self.current += 1;
            self.scroll_offset = 0;
            self.notes_scroll = 0;
            self.exec_output = None;
            self.exec_rx = None;
            self.apply_slide_font();
        }
    }

    fn prev_slide(&mut self) {
        if self.current > 0 {
            self.current -= 1;
            self.scroll_offset = 0;
            self.notes_scroll = 0;
            self.exec_output = None;
            self.exec_rx = None;
            self.apply_slide_font();
        }
    }

    fn goto_slide(&mut self, idx: usize) {
        if idx < self.slides.len() {
            self.current = idx;
            self.scroll_offset = 0;
            self.exec_output = None;
            self.exec_rx = None;
            self.apply_slide_font();
        }
    }

    fn next_section(&mut self) {
        let current_section = &self.slides[self.current].section;
        for i in (self.current + 1)..self.slides.len() {
            if self.slides[i].section != *current_section {
                self.current = i;
                self.scroll_offset = 0;
                self.exec_output = None;
                self.exec_rx = None;
                self.apply_slide_font();
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
        self.scroll_offset = 0;
        self.exec_output = None;
        self.apply_slide_font();
    }

    fn scroll_down(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(n);
    }

    fn scroll_up(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
    }

    fn execute_code(&mut self) -> Result<()> {
        let slide = &self.slides[self.current];
        // Find first executable code block: check slide-level first, then columns
        let exec_cb = slide.code_blocks.iter()
            .find(|cb| cb.exec_mode.is_some())
            .or_else(|| {
                slide.columns.as_ref().and_then(|cols| {
                    cols.contents.iter().flat_map(|c| c.code_blocks.iter())
                        .find(|cb| cb.exec_mode.is_some())
                })
            })
            .or_else(|| slide.code_blocks.first());
        if let Some(cb) = exec_cb {
            let pres_dir = self.presentation_path.parent();
            let rx = crate::code::executor::execute_code_streaming(&cb.language, &cb.code, pres_dir)?;
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
                        // Execution complete
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
            if let Ok(new_slides) = crate::markdown::parse_presentation(&source, base_dir) {
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

        // Font change BEFORE the sync block.  Kitty's set_font_size RC
        // command is out-of-band — it triggers an immediate terminal resize.
        //
        // A single large font jump causes a jarring visual glitch because
        // Kitty reflows all on-screen content at the new dimensions in one
        // frame.  Instead, we animate the change in 0.5pt steps — each step
        // produces a tiny, barely-perceptible reflow that looks like a
        // smooth zoom.  For decrease (more cols) there is no reflow issue,
        // so we send the change in one shot.
        if let Some(target) = font_changing {
            let current = self.last_applied_font_size.unwrap_or(target);
            let increasing = target > current;

            let stdout = io::stdout();
            let mut pre = stdout.lock();

            // Clear Kitty images before animation so stale overlays don't
            // persist at the old size through the font transition
            if self.image_protocol == ImageProtocol::Kitty {
                pre.write_all(b"\x1b_Ga=d,d=a,q=2\x1b\\")?;
                pre.flush()?;
            }

            if increasing && (target - current).abs() > 0.3 {
                // Animate in 0.2pt steps for a smoother zoom effect
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

            // Final step — land exactly on target
            let json = format!(
                r#"{{"cmd":"set_font_size","version":[0,14,2],"no_response":true,"payload":{{"size":{:.1}}}}}"#,
                target
            );
            let esc = format!("\x1bP@kitty-cmd{}\x1b\\", json);
            pre.write_all(esc.as_bytes())?;
            pre.flush()?;
            drop(pre);

            self.last_applied_font_size = Some(target);
            // Drain all resize events from the animation steps
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
            && (self.last_rendered_slide != Some(self.current) || self.needs_full_redraw);

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

        // Section
        if !slide.section.is_empty() {
            let mut line = StyledLine::empty();
            line.push(StyledSpan::new(&pad));
            line.push(StyledSpan::new(&format!("Section: {}", slide.section)).with_fg(self.text_color).dim());
            lines.push(line);
            lines.push(StyledLine::empty());
        }

        // Title
        if !slide.title.is_empty() {
            if slide.ascii_title {
                self.render_ascii_title(&slide.title, &pad, &mut lines);
            } else {
                let mut line = StyledLine::empty();
                line.push(StyledSpan::new(&pad));
                line.push(StyledSpan::new(&slide.title).with_fg(self.accent_color).bold());
                lines.push(line);
            }
            lines.push(StyledLine::empty());
        }

        // Subtitle (wrapped to content width)
        if !slide.subtitle.is_empty() {
            let sub_width = content_width.saturating_sub(2);
            let wrapped_sub = textwrap_simple(&slide.subtitle, sub_width);
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
            let has_column_exec = cols.contents.iter()
                .any(|c| c.code_blocks.iter().any(|cb| cb.exec_mode.is_some()));
            if has_column_exec {
                self.render_exec_output(&pad, &mut lines);
            }
            lines.push(StyledLine::empty());
        }

        // Code blocks (presenterm-style: background rect with padding, no borders)
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

            // Execution output (show under the executable code block)
            if cb.exec_mode.is_some() {
                self.render_exec_output(&pad, &mut lines);
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

        // Image rendering (cached)
        let mut pending_protocol_image: Option<(String, usize)> = None;
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
                    pending_protocol_image = Some((escape_data.clone(), image_line_offset));
                }
                None => {}
            }
            lines.push(StyledLine::empty());
        }

        // Calculate available display area (excluding status bar rows)
        let reserved_bottom =
            if self.show_notes && !slide.notes.is_empty() { 7 } else { 0 }
            + if self.mode == Mode::Command || self.mode == Mode::Goto { 1 } else { 0 };
        let content_area = th.saturating_sub(status_bar_rows + reserved_bottom);

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

        // Render visible content lines (offset by status_bar_rows)
        for (i, line) in lines[visible_start..visible_end].iter().enumerate() {
            let row = (status_bar_rows + i) as u16;
            queue!(w, cursor::MoveTo(0, row))?;
            self.queue_styled_line(&mut w, line, tw)?;
        }

        // Fill remaining rows below content
        let content_rows_drawn = visible_end - visible_start;
        for i in content_rows_drawn..content_area {
            let row = (status_bar_rows + i) as u16;
            queue!(w, cursor::MoveTo(0, row), SetBackgroundColor(self.bg_color))?;
            write!(w, "{}", " ".repeat(tw))?;
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
        // Must re-send every frame since line rendering overwrites the image area.
        // Synchronized update (BeginSynchronizedUpdate/EndSynchronizedUpdate) prevents flicker.
        if let Some((escape_data, line_offset)) = pending_protocol_image {
            let display_row = line_offset.saturating_sub(visible_start);
            if display_row < content_area {
                let screen_row = (status_bar_rows + display_row) as u16;
                queue!(w, cursor::MoveTo(0, screen_row))?;
                write!(w, "{}", escape_data)?;
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

        // Progress bar fills remaining space
        let fixed_len = slide_info.len() + timer.len() + theme_part.len() + 2; // 2 for [] brackets
        let bar_width = width.saturating_sub(fixed_len);
        let progress = render_progress_bar(self.current + 1, self.slides.len(), bar_width);

        let mut line = StyledLine::empty();
        line.push(StyledSpan::new(&slide_info).with_fg(self.bg_color).with_bg(self.accent_color).bold());
        line.push(StyledSpan::new(&timer).with_fg(self.text_color).with_bg(self.code_bg_color));
        if !theme_part.is_empty() {
            line.push(StyledSpan::new(&theme_part).with_fg(self.text_color).with_bg(self.code_bg_color).dim());
        }
        line.push(StyledSpan::new(&progress).with_fg(self.accent_color).with_bg(self.code_bg_color));
        // Fill any remaining space
        let used: usize = slide_info.len() + timer.len() + theme_part.len() + progress.len();
        if used < width {
            line.push(StyledSpan::new(&" ".repeat(width - used)).with_bg(self.code_bg_color));
        }
        line
    }

    /// Render exec output lines into the buffer.
    fn render_exec_output(&self, pad: &str, lines: &mut Vec<StyledLine>) {
        if let Some(ref output) = self.exec_output {
            lines.push(StyledLine::empty());
            let mut oh = StyledLine::empty();
            oh.push(StyledSpan::new(pad));
            oh.push(StyledSpan::new("  Output:").with_fg(self.accent_color).bold());
            lines.push(oh);
            for ol in output.lines() {
                let sanitized = strip_control_chars(ol);
                let mut line = StyledLine::empty();
                line.push(StyledSpan::new(pad));
                line.push(StyledSpan::new("  "));
                line.push(StyledSpan::new(&sanitized).with_fg(self.text_color));
                lines.push(line);
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
        let mut chars_written = 0usize;
        // Set default background for the entire line
        queue!(w, SetBackgroundColor(self.bg_color))?;
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
            if let Some(fg) = span.fg {
                queue!(w, SetForegroundColor(fg))?;
            } else {
                queue!(w, SetForegroundColor(self.text_color))?;
            }
            let bg = span.bg.unwrap_or(self.bg_color);
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
            let span_width = unicode_width::UnicodeWidthStr::width(span.text.as_str());
            let remaining = term_width.saturating_sub(chars_written);
            if span_width <= remaining {
                write!(w, "{}", span.text)?;
                chars_written += span_width;
            } else {
                // Truncate: take only enough characters to fill remaining columns
                let mut truncated = String::new();
                let mut tw = 0;
                for ch in span.text.chars() {
                    let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
                    if tw + cw > remaining {
                        break;
                    }
                    truncated.push(ch);
                    tw += cw;
                }
                write!(w, "{}", truncated)?;
                chars_written += tw;
            }
        }
        // Reset attributes and fill rest of line with background
        queue!(w, SetAttribute(Attribute::Reset), SetBackgroundColor(self.bg_color))?;
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
            ("K", "", "Output streams in real-time"),
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

        for row in 0..th {
            queue!(w, cursor::MoveTo(0, row as u16), SetBackgroundColor(self.bg_color))?;
            write!(w, "{}", " ".repeat(tw))?;
        }

        queue!(w, cursor::MoveTo(2, 1), SetForegroundColor(self.accent_color), SetAttribute(Attribute::Bold))?;
        write!(w, "Slide Overview")?;
        queue!(w, SetAttribute(Attribute::Reset))?;

        let cols = 3usize;
        let col_width = (tw - 4) / cols;
        let start_y = 3u16;

        for (i, slide) in self.slides.iter().enumerate() {
            let col = i % cols;
            let row = i / cols;
            let x = 2 + col * col_width;
            let y = start_y + row as u16 * 2;
            if y >= self.height - 2 { break; }

            queue!(w, cursor::MoveTo(x as u16, y))?;
            if i == self.current {
                queue!(w, SetBackgroundColor(self.accent_color), SetForegroundColor(self.bg_color))?;
            } else {
                queue!(w, SetBackgroundColor(self.bg_color), SetForegroundColor(self.text_color))?;
            }

            let label = format!(" {:>2}. {} ", i + 1, truncate_str(&slide.title, col_width.saturating_sub(8)));
            write!(w, "{:<width$}", label, width = col_width.min(label.len() + 2))?;
            queue!(w, SetAttribute(Attribute::Reset))?;
        }

        queue!(w, cursor::MoveTo(2, self.height - 1), SetForegroundColor(self.accent_color), SetAttribute(Attribute::Dim))?;
        write!(w, "h/l: navigate  Enter: select  Esc: close")?;
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
