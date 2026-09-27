//! The event loop and keyboard handling.

use std::io::{self, Write};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind,
};

use super::types::Mode;
use super::Presenter;

/// Frame interval while something is moving.
const ANIMATION_TICK: Duration = Duration::from_millis(16);
/// Upper bound on sleeping, so file changes and remote commands are noticed.
const IDLE_TICK: Duration = Duration::from_millis(250);

impl Presenter {
    pub(crate) fn event_loop(&mut self) -> Result<()> {
        let mut out = io::stdout();
        loop {
            self.render(&mut out)?;
            self.broadcast_state();
            if event::poll(self.poll_timeout())? {
                loop {
                    if self.handle_event(event::read()?) {
                        return Ok(());
                    }
                    if !event::poll(Duration::ZERO)? {
                        break;
                    }
                }
            }
            self.tick();
        }
    }

    fn poll_timeout(&self) -> Duration {
        let slide = &self.slides[self.current];
        let gif = slide
            .image
            .as_ref()
            .is_some_and(|i| self.images.is_animated(&i.path));
        let moving = self.animation.is_some()
            || !slide.loop_animations.is_empty()
            || gif
            || self.exec.is_some();
        if moving && self.mode == Mode::Normal {
            return ANIMATION_TICK;
        }
        match self.timer_start {
            Some(start) => {
                let into_second = start.elapsed().subsec_millis();
                Duration::from_millis(u64::from(1000 - into_second) + 5).min(IDLE_TICK)
            }
            None => IDLE_TICK,
        }
    }

    /// Advances everything time-driven; runs once per loop iteration.
    fn tick(&mut self) {
        if self.animation.as_ref().is_some_and(|a| a.is_done()) {
            let finished = self.animation.take();
            let entrance = self.slides[self.current].entrance_animation;
            if let (Some(a), Some(e)) = (finished, entrance) {
                if matches!(
                    a.kind,
                    crate::render::animation::AnimationKind::Transition(_)
                ) {
                    self.animation =
                        Some(crate::render::animation::AnimationState::new_entrance(e));
                }
            }
        }
        if self.images.poll_gif_loading() {
            self.invalidate();
        }
        if let Some(img) = &self.slides[self.current].image {
            let path = img.path.clone();
            self.images.advance_gif(&path);
        }
        if self.poll_exec_output() {
            self.invalidate();
        }
        if self.watcher.as_ref().is_some_and(|w| w.check_modified()) {
            self.reload();
        }
        self.poll_remote();
    }

    /// Composes the screen and writes whatever changed.
    pub(crate) fn render(&mut self, out: &mut impl Write) -> Result<()> {
        if let Some(size) = self.font.take_pending() {
            self.change_font(size, out)?;
        }
        let screen = self.compose();
        let transmit = self.images.take_outbox();
        if !transmit.is_empty() {
            out.write_all(&transmit)?;
        }
        let mut frame = Vec::new();
        if self.terminal_bg != Some(self.palette.bg) {
            self.terminal_bg = Some(self.palette.bg);
            frame.extend_from_slice(super::terminal::background_escape(self.palette.bg).as_bytes());
        }
        self.display
            .present(&screen, self.width, self.palette.text, &mut frame)?;
        out.write_all(&frame)?;
        out.flush()?;
        if let Some(recorder) = &mut self.recorder {
            recorder.output(&frame);
        }
        Ok(())
    }

    /// Changes the font size and waits briefly for the terminal to re-layout,
    /// so the next frame is drawn once at the new size.
    fn change_font(&mut self, size: f64, out: &mut impl Write) -> Result<()> {
        crossterm::queue!(out, crossterm::terminal::BeginSynchronizedUpdate)?;
        let before = (self.window.columns, self.window.rows);
        self.font.apply(size, out);
        let deadline = Instant::now() + Duration::from_millis(200);
        loop {
            self.window = crate::render::layout::WindowSize::query();
            if (self.window.columns, self.window.rows) != before || Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        self.resize(self.window.columns, self.window.rows);
        Ok(())
    }

    /// Opens the current slide in `$VISUAL` / `$EDITOR`, then reloads, which
    /// shows the slide that changed.
    fn edit_slide(&mut self) {
        let editor = std::env::var("VISUAL")
            .or_else(|_| std::env::var("EDITOR"))
            .unwrap_or_else(|_| if cfg!(windows) { "notepad" } else { "vi" }.to_string());
        let line = self.slides[self.current].line;
        let Some((program, args)) = editor_command(&editor, &self.presentation_path, line) else {
            return;
        };
        let status = super::terminal::hand_over(|| {
            std::process::Command::new(&program).args(&args).status()
        });
        if let Err(e) = status {
            self.notify(format!("cannot start {program}: {e}"));
        }
        // The hand-over reset the font and freed Kitty's images.
        let (cols, rows) = crossterm::terminal::size().unwrap_or((self.width, self.height));
        self.resize(cols, rows);
        self.font.request(Some(self.current));
        self.reload();
    }

    fn resize(&mut self, width: u16, height: u16) {
        self.window = crate::render::layout::WindowSize::query();
        self.width = width.max(1);
        self.height = height.max(1);
        if let Some(recorder) = &mut self.recorder {
            recorder.resize(self.width, self.height);
        }
        self.images.clear();
        self.display.invalidate();
        self.terminal_bg = None;
    }

    /// Handles one terminal event; returns true to quit.
    fn handle_event(&mut self, event: Event) -> bool {
        match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => return self.handle_key(key),
            Event::Mouse(m) if self.mode == Mode::Normal => match m.kind {
                MouseEventKind::ScrollDown => self.scroll_by(3),
                MouseEventKind::ScrollUp => self.scroll_by(-3),
                _ => {}
            },
            Event::Resize(w, h) => self.resize(w, h),
            _ => {}
        }
        false
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('c') {
            return true;
        }
        if self.blank {
            // The key that brings the slide back does nothing else.
            self.blank = false;
            return false;
        }
        match self.mode {
            Mode::Help => {
                self.mode = Mode::Normal;
                self.font.request(Some(self.current));
            }
            Mode::Overview => self.overview_key(key),
            Mode::Command | Mode::Goto | Mode::Search => {
                let typed = key.modifiers.difference(KeyModifiers::SHIFT).is_empty();
                return self.prompt_key(key.code, typed);
            }
            Mode::Normal => return self.normal_key(key.code, ctrl),
        }
        false
    }

    fn normal_key(&mut self, code: KeyCode, ctrl: bool) -> bool {
        let page = isize::try_from(self.height / 2).unwrap_or(1);
        match code {
            KeyCode::Char('q') => return true,
            KeyCode::Char('d') if ctrl => self.scroll_by(page),
            KeyCode::Char('u') if ctrl => self.scroll_by(-page),
            KeyCode::Char('e') if ctrl => self.execute_code(),
            KeyCode::Right | KeyCode::Char('l' | ' ') | KeyCode::Enter | KeyCode::PageDown => {
                self.next_slide()
            }
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Backspace | KeyCode::PageUp => {
                self.prev_slide()
            }
            KeyCode::Home => self.goto_slide(0),
            KeyCode::End => self.goto_slide(usize::MAX),
            KeyCode::Down | KeyCode::Char('j') => self.scroll_by(1),
            KeyCode::Up | KeyCode::Char('k') => self.scroll_by(-1),
            KeyCode::Char('J') => self.next_section(),
            KeyCode::Char('K') => self.prev_section(),
            KeyCode::Char('g') => self.open_prompt(Mode::Goto),
            KeyCode::Char(':') => self.open_prompt(Mode::Command),
            KeyCode::Char('n') => self.toggle_notes(),
            KeyCode::Char('N') => self.notes_scroll += 1,
            KeyCode::Char('P') => self.notes_scroll = self.notes_scroll.saturating_sub(1),
            KeyCode::Char('f') => {
                self.fullscreen_default = !self.fullscreen;
                self.fullscreen = self.fullscreen_default;
            }
            KeyCode::Char('t') => self.toggle_timer(),
            KeyCode::Char('T') => self.show_theme_name = !self.show_theme_name,
            KeyCode::Char('S') => self.toggle_sections(),
            KeyCode::Char('D') => self.toggle_dark_mode(),
            KeyCode::Char('+' | '=') => self.adjust_scale(5),
            KeyCode::Char('-') => self.adjust_scale(-5),
            KeyCode::Char('>') => self.adjust_image_scale(10),
            KeyCode::Char('<') => self.adjust_image_scale(-10),
            KeyCode::Char(']') => self.adjust_font(1),
            KeyCode::Char('[') => self.adjust_font(-1),
            KeyCode::Char('0') => self.reset_font(),
            KeyCode::Char('o') => self.open_overview(),
            KeyCode::Char('b') => self.blank = true,
            KeyCode::Char('e') => self.edit_slide(),
            KeyCode::Char('/') => self.open_prompt(Mode::Search),
            KeyCode::Char('?') => {
                self.mode = Mode::Help;
                self.font.request(None);
            }
            _ => {}
        }
        false
    }

    fn open_overview(&mut self) {
        self.overview_sel = self.current;
        self.mode = Mode::Overview;
        self.font.request(None);
    }

    fn open_prompt(&mut self, mode: Mode) {
        self.input.clear();
        self.mode = mode;
    }

    /// `typed` is false for chords (Ctrl/Alt), which never insert text.
    fn prompt_key(&mut self, code: KeyCode, typed: bool) -> bool {
        match code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Backspace => {
                if self.input.pop().is_none() {
                    self.mode = Mode::Normal;
                }
            }
            KeyCode::Enter => {
                let input = std::mem::take(&mut self.input);
                let mode = std::mem::replace(&mut self.mode, Mode::Normal);
                return match mode {
                    Mode::Goto => {
                        self.goto_number(&input);
                        false
                    }
                    Mode::Search => {
                        self.search(&input);
                        false
                    }
                    _ => self.execute_command(&input),
                };
            }
            KeyCode::Char(c) if typed && (self.mode != Mode::Goto || c.is_ascii_digit()) => {
                self.input.push(c)
            }
            _ => {}
        }
        false
    }

    fn goto_number(&mut self, text: &str) {
        if let Ok(n) = text.trim().parse::<usize>() {
            self.goto_slide(n.saturating_sub(1));
        }
    }

    /// Runs a `:` command; returns true to quit.
    fn execute_command(&mut self, cmd: &str) -> bool {
        let (name, arg) = cmd.trim().split_once(' ').unwrap_or((cmd.trim(), ""));
        match name {
            "q" | "quit" => return true,
            "theme" => {
                if let Some(theme) = self.registry.get(arg.trim()) {
                    self.set_base_theme(theme);
                    self.save_state();
                }
            }
            "goto" => self.goto_number(arg),
            n if n.parse::<usize>().is_ok() => self.goto_number(n),
            "notes" => self.toggle_notes(),
            "timer" => {
                if arg.trim() == "reset" {
                    self.timer_start = None;
                } else {
                    self.timer_start.get_or_insert_with(Instant::now);
                }
            }
            "overview" => self.open_overview(),
            "help" => self.mode = Mode::Help,
            "reload" => self.reload(),
            _ => {}
        }
        false
    }

    /// Rows of cards per overview page (must match `overview_screen`).
    fn overview_rows(&self) -> usize {
        (usize::from(self.height).saturating_sub(4) / 3).max(1)
    }

    fn overview_key(&mut self, key: KeyEvent) {
        let last = self.slides.len() - 1;
        let rows = self.overview_rows();
        let sel = self.overview_sel;
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let move_later = match key.code {
            KeyCode::Char('J') => Some(true),
            KeyCode::Char('K') => Some(false),
            KeyCode::Down if shift => Some(true),
            KeyCode::Up if shift => Some(false),
            _ => None,
        };
        if let Some(later) = move_later {
            return self.move_slide(later);
        }
        self.overview_sel = match key.code {
            KeyCode::Down | KeyCode::Char('j') => (sel + 1).min(last),
            KeyCode::Up | KeyCode::Char('k') => sel.saturating_sub(1),
            KeyCode::Right | KeyCode::Char('l') => (sel + rows).min(last),
            KeyCode::Left | KeyCode::Char('h') => sel.saturating_sub(rows),
            KeyCode::Home => 0,
            KeyCode::End => last,
            KeyCode::Enter => {
                self.mode = Mode::Normal;
                self.font.request(Some(self.current));
                self.goto_slide(sel);
                return;
            }
            KeyCode::Esc | KeyCode::Char('o' | 'q') => {
                self.mode = Mode::Normal;
                self.font.request(Some(self.current));
                return;
            }
            _ => sel,
        };
    }

    fn adjust_font(&mut self, delta: i8) {
        if self.font.available() {
            self.font.adjust(self.current, delta);
            self.save_state();
        }
    }

    fn reset_font(&mut self) {
        if self.font.available() {
            self.font.reset(self.current);
            self.save_state();
        }
    }
}

/// The program and arguments that open `path` at `line` in `editor`, which
/// may carry its own arguments (`code --wait`). Editors without a known way
/// to take a line just open the file.
fn editor_command(
    editor: &str,
    path: &std::path::Path,
    line: usize,
) -> Option<(String, Vec<String>)> {
    let mut words = editor.split_whitespace();
    let program = words.next()?.to_string();
    let mut args: Vec<String> = words.map(str::to_string).collect();
    let name = std::path::Path::new(&program)
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let file = path.display().to_string();
    match name.as_str() {
        "vi" | "vim" | "nvim" | "nano" | "emacs" | "emacsclient" | "micro" | "kak" | "joe"
        | "mg" | "ne" => args.extend([format!("+{line}"), file]),
        "code" | "code-insiders" | "codium" | "cursor" => {
            args.extend(["--goto".to_string(), format!("{file}:{line}")]);
        }
        "subl" | "zed" | "hx" | "helix" => args.push(format!("{file}:{line}")),
        _ => args.push(file),
    }
    Some((program, args))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{presenter, presenter_at, screen};
    use super::*;

    fn press(p: &mut Presenter, keys: &str) {
        for c in keys.chars() {
            let code = if c == '\n' {
                KeyCode::Enter
            } else {
                KeyCode::Char(c)
            };
            p.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
        }
    }

    #[test]
    fn the_overview_reorders_slides_in_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deck.md");
        let deck = "# A\n---\n# B\n---\n# C\n";
        std::fs::write(&path, deck).unwrap();
        let mut p = presenter_at(deck, path.clone());
        press(&mut p, "oJ");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# B\n---\n# A\n---\n# C\n"
        );
        assert_eq!(
            (p.mode, p.overview_sel),
            (Mode::Overview, 1),
            "the selection follows"
        );
        assert_eq!(
            p.slides[p.current].title, "A",
            "the current slide stays current"
        );
        press(&mut p, "K");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), deck);
        press(&mut p, "K");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            deck,
            "nothing before the first"
        );
    }

    #[test]
    fn editors_open_the_file_at_the_slide_line() {
        let path = std::path::Path::new("/talks/deck.md");
        let cases: &[(&str, &str, &[&str])] = &[
            ("nvim", "nvim", &["+12", "/talks/deck.md"]),
            (
                "/usr/bin/vim -u NONE",
                "/usr/bin/vim",
                &["-u", "NONE", "+12", "/talks/deck.md"],
            ),
            (
                "code --wait",
                "code",
                &["--wait", "--goto", "/talks/deck.md:12"],
            ),
            ("hx", "hx", &["/talks/deck.md:12"]),
            ("gedit", "gedit", &["/talks/deck.md"]),
        ];
        for (editor, program, args) in cases {
            let (p, a) = editor_command(editor, path, 12).unwrap();
            assert_eq!(
                (p.as_str(), a),
                (*program, args.iter().map(|s| s.to_string()).collect())
            );
        }
        assert!(editor_command("  ", path, 1).is_none());
    }

    #[test]
    fn blanking_hides_everything_and_the_next_key_only_restores() {
        let mut p = presenter("# One\n---\n# Two");
        press(&mut p, "b");
        assert!(screen(&mut p).iter().all(String::is_empty));
        press(&mut p, "l");
        assert_eq!(p.current, 0, "the restoring key must not advance");
        assert!(screen(&mut p).iter().any(|r| r.contains("One")));
    }

    #[test]
    fn search_finds_text_and_notes_wrapping_around() {
        let mut p = presenter(
            "# One\n---\n# Two\n<!-- notes: mention the Needle -->\n---\n# Three\n- a needle here",
        );
        press(&mut p, "/needle\n");
        assert_eq!(p.current, 1);
        press(&mut p, "/\n");
        assert_eq!(p.current, 2, "empty query repeats the last search");
        press(&mut p, "/\n");
        assert_eq!(p.current, 1, "search wraps past the end");
        press(&mut p, "/haystack\n");
        assert_eq!(p.current, 1);
        let bar = screen(&mut p).pop().unwrap();
        assert!(bar.contains("no slide mentions “haystack”"), "{bar}");
    }
}
