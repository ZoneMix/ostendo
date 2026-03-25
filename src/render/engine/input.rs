//! Event loop and input handling.
//!
//! Processes keyboard events, mouse input (scroll wheel), terminal resize
//! events, and commands from the optional WebSocket remote control server.
//!
//! # Event Loop Design
//!
//! The main loop uses `crossterm::event::poll()` with a dynamic timeout:
//! - **33ms** (~30 fps) when animations or GIFs are active.
//! - **100ms** when idle (saves CPU while still updating the timer).
//!
//! All pending events are drained in a tight inner loop before rendering,
//! which prevents mouse scroll flooding from causing redundant redraws.
//! After input handling, animation ticks, GIF frame advances, code execution
//! polling, hot reload checks, and remote command polling all happen in sequence.

use super::*;

impl Presenter {
    /// The main event loop that drives the presentation.
    ///
    /// This function blocks until the user presses `q` (or another quit trigger).
    /// Each iteration:
    /// 1. Checks for completed background GIF loading.
    /// 2. Drains pre-rendered GIF frames from the background render thread.
    /// 3. Polls for terminal events with a dynamic timeout.
    /// 4. Drains all pending events (prevents mouse flooding).
    /// 5. Renders a frame if input was received or the timer is running.
    /// 6. Ticks active animations (transitions, entrances, loops).
    /// 7. Advances GIF frames if their delay has elapsed.
    /// 8. Polls for streaming code execution output.
    /// 9. Checks for file changes (hot reload).
    /// 10. Polls for WebSocket remote commands.
    pub(crate) fn event_loop(&mut self) -> Result<()> {
        self.render_frame()?;
        self.broadcast_state();
        loop {
            // Check if background GIF loading has completed.
            // Only stores the decoded frames — NO synchronous encoding or
            // transmission here. GIF frames are rendered lazily in render_frame()
            // on first visit to the GIF slide (same as static images).
            if let Some(handle) = self.gif_loading.take() {
                if handle.is_finished() {
                    if let Ok(loaded) = handle.join() {
                        self.gif_frames.extend(loaded.into_iter().map(|(k, v)| (k, std::sync::Arc::new(v))));
                        self.needs_full_redraw = true;
                    }
                } else {
                    self.gif_loading = Some(handle);
                }
            }

            // Dynamic poll timeout: 33ms when animation/GIF active (~30fps), 100ms otherwise.
            // Kitty native animation: terminal drives GIF, so no app-side polling needed.
            let has_active_gif = self.current_slide_has_gif();
            let kitty_drives_gif = has_active_gif
                && self.kitty_animation_cap == crate::terminal::protocols::KittyAnimationCapability::Supported
                && self.slides[self.current].image.as_ref()
                    .map(|img| self.kitty_gif_ids.contains_key(&img.path))
                    .unwrap_or(false);
            let needs_gif_polling = has_active_gif && !kitty_drives_gif;
            let poll_ms = if self.active_animation.is_some() || !self.active_loop.is_empty() || needs_gif_polling { 33 } else { 100 };
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
                        if let Some(ea) = slide.entrance_animation {
                            self.active_animation = Some(AnimationState::new_entrance(ea, Vec::new()));
                        } else {
                            self.active_animation = None;
                        }
                    } else {
                        self.active_animation = None;
                    }
                    // Don't render now — the previous tick already showed a
                    // near-final frame.  Rendering immediately would cause a
                    // visible "pop" from ~97% brightness to 100%.  The next
                    // event-loop iteration will do a clean render instead.
                    self.needs_full_redraw = true;
                } else {
                    self.needs_full_redraw = true;
                    self.render_frame()?;
                }
            }

            // Tick loop animations
            if !self.active_loop.is_empty() {
                for (_, ref mut frame) in self.active_loop.iter_mut() {
                    *frame += 1;
                }
                self.needs_full_redraw = true;
                // Only render loop when no transition/entrance is active
                if self.active_animation.is_none() {
                    self.render_frame()?;
                }
            }

            // Advance animated GIF frame if delay has elapsed.
            // Skip when Kitty native animation is active — terminal drives playback.
            if needs_gif_polling && self.advance_gif_frame() {
                self.needs_full_redraw = true;
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

    /// Poll the WebSocket remote control channel for incoming commands.
    ///
    /// Drains all queued commands without blocking. Each command maps to the
    /// same action as its keyboard equivalent (next slide, toggle notes, etc.).
    /// If any command was received, triggers a re-render and broadcasts the
    /// updated state back to connected clients.
    ///
    /// The receiver is temporarily taken out of `self` via `Option::take()` to
    /// avoid a borrow conflict (we need `&mut self` for the command handlers
    /// while also reading from the receiver). It is put back after processing.
    pub(crate) fn poll_remote(&mut self) -> Result<()> {
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
                crate::remote::RemoteCommand::NextSection => self.next_section(),
                crate::remote::RemoteCommand::PrevSection => self.prev_section(),
                crate::remote::RemoteCommand::ScrollUp => self.scroll_up(3),
                crate::remote::RemoteCommand::ScrollDown => self.scroll_down(3),
                crate::remote::RemoteCommand::ToggleFullscreen => self.toggle_fullscreen(),
                crate::remote::RemoteCommand::ToggleNotes => self.toggle_notes(),
                crate::remote::RemoteCommand::ToggleThemeName => self.toggle_theme_name(),
                crate::remote::RemoteCommand::ToggleSections => self.toggle_sections(),
                crate::remote::RemoteCommand::ToggleDarkMode => self.toggle_dark_mode(),
                crate::remote::RemoteCommand::ScaleUp => self.scale_up(),
                crate::remote::RemoteCommand::ScaleDown => self.scale_down(),
                crate::remote::RemoteCommand::ImageScaleUp => self.image_scale_up(),
                crate::remote::RemoteCommand::ImageScaleDown => self.image_scale_down(),
                crate::remote::RemoteCommand::FontUp => self.adjust_font_offset(1),
                crate::remote::RemoteCommand::FontDown => self.adjust_font_offset(-1),
                crate::remote::RemoteCommand::FontReset => self.reset_font_offset(),
                crate::remote::RemoteCommand::ExecuteCode => {
                    if self.allow_remote_exec && self.allow_exec {
                        let _ = self.execute_code();
                    }
                }
                crate::remote::RemoteCommand::TimerStart => {
                    if self.timer_start.is_none() { self.start_timer(); }
                }
                crate::remote::RemoteCommand::TimerReset => self.reset_timer(),
                crate::remote::RemoteCommand::SetTheme(slug) => {
                    let registry = crate::theme::ThemeRegistry::load();
                    if let Some(new_theme) = registry.get(&slug) {
                        self.is_light_variant = new_theme.dark_variant.is_some();
                        self.base_theme = new_theme.clone();
                        self.apply_theme(new_theme);
                    }
                }
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

    /// Broadcast the current presentation state to all connected WebSocket clients.
    ///
    /// Serializes a `StateMessage` containing the current slide number, title,
    /// notes, timer, content, theme info, and all toggle states. This is sent
    /// as JSON over the broadcast channel. If no receivers are connected,
    /// the function returns immediately to avoid unnecessary work.
    pub(crate) fn broadcast_state(&self) {
        if let Some(ref tx) = self.state_broadcast {
            if tx.receiver_count() == 0 {
                return;
            }
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
            let has_exec = self.allow_exec && (slide.code_blocks.iter().any(|cb| cb.exec_mode.is_some())
                || slide.columns.as_ref().is_some_and(|cols|
                    cols.contents.iter().any(|c| c.code_blocks.iter().any(|cb| cb.exec_mode.is_some()))
                ));
            let font_offset = self.slide_font_offsets.get(&self.current).copied().unwrap_or(0);
            let msg = crate::remote::StateMessage {
                msg_type: "state".to_string(),
                slide: self.current + 1,
                total: self.slides.len(),
                slide_title: slide.title.clone(),
                notes: slide.notes.clone(),
                timer: self.format_timer(),
                slide_content: content,
                section: slide.section.clone(),
                is_fullscreen: self.show_fullscreen,
                is_notes_visible: self.show_notes,
                is_dark_mode: !self.is_light_variant,
                show_theme_name: self.show_theme_name,
                show_sections: self.show_sections,
                theme_name: self.theme.name.clone(),
                theme_slug: self.theme.slug.clone(),
                scale: self.global_scale,
                image_scale: self.image_scale_offset,
                font_offset,
                has_executable_code: has_exec,
                timer_running: self.timer_start.is_some(),
                themes: self.theme_slugs.clone(),
                theme_bg: crate::theme::colors::color_to_hex(self.bg_color),
                theme_accent: crate::theme::colors::color_to_hex(self.accent_color),
                theme_text: crate::theme::colors::color_to_hex(self.text_color),
            };
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = tx.send(json);
            }
        }
    }

    /// Handle a keyboard event, dispatching based on the current mode.
    ///
    /// Returns `Ok(true)` if the user wants to quit (pressed `q` in Normal mode),
    /// `Ok(false)` otherwise. The function first checks the current mode:
    ///
    /// - **Command** — Routes to `handle_command_key()` for `:` command input.
    /// - **Goto** — Routes to `handle_goto_key()` for numeric slide input.
    /// - **Help** — Any key returns to Normal mode.
    /// - **Overview** — Arrow keys / vim keys navigate, Enter selects, Esc closes.
    /// - **Normal** — Full keybinding set (navigation, toggles, scale, font, etc.).
    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> Result<bool> {
        match self.mode {
            Mode::Command => return self.handle_command_key(key),
            Mode::Goto => return self.handle_goto_key(key),
            Mode::Help => {
                self.mode = Mode::Normal;
                // Restore slide font instantly — no transition or stepping.
                self.font_change_is_slide_transition = FontTransitionMode::None;
                self.skip_next_font_stepping = true;
                self.apply_slide_font();
                self.needs_full_redraw = true;
                return Ok(false);
            }
            Mode::Overview => {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('o') => {
                        self.mode = Mode::Normal;
                        self.font_change_is_slide_transition = FontTransitionMode::None;
                        self.skip_next_font_stepping = true;
                        self.apply_slide_font();
                        self.needs_full_redraw = true;
                    }
                    KeyCode::Enter => {
                        self.mode = Mode::Normal;
                        self.font_change_is_slide_transition = FontTransitionMode::None;
                        self.skip_next_font_stepping = true;
                        self.apply_slide_font();
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
            KeyCode::Char('n') => self.toggle_notes(),
            KeyCode::Char('N') if self.show_notes => {
                self.notes_scroll += 1;
                self.needs_full_redraw = true;
            }
            KeyCode::Char('P') if self.show_notes => {
                self.notes_scroll = self.notes_scroll.saturating_sub(1);
                self.needs_full_redraw = true;
            }
            KeyCode::Char('f') => self.toggle_fullscreen(),
            KeyCode::Char('T') => self.toggle_theme_name(),
            KeyCode::Char('S') => self.toggle_sections(),
            KeyCode::Char('D') => self.toggle_dark_mode(),
            KeyCode::Char('+') | KeyCode::Char('=') => self.scale_up(),
            KeyCode::Char('-') => self.scale_down(),
            KeyCode::Char('>') => self.image_scale_up(),
            KeyCode::Char('<') => self.image_scale_down(),
            KeyCode::Char(']') if self.font_capability.is_available() => self.adjust_font_offset(1),
            KeyCode::Char('[') if self.font_capability.is_available() => self.adjust_font_offset(-1),
            KeyCode::Char('0') if key.modifiers.contains(KeyModifiers::CONTROL)
                || key.modifiers.contains(KeyModifiers::SUPER) => self.reset_font_offset(),
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

    /// Handle keyboard input while in Command mode (`:` prompt at the bottom).
    ///
    /// Esc cancels, Enter executes the command, Backspace deletes, and any
    /// printable character is appended to the command buffer. Always returns
    /// `Ok(false)` since commands cannot quit the application directly.
    pub(crate) fn handle_command_key(&mut self, key: KeyEvent) -> Result<bool> {
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

    /// Handle keyboard input while in Goto mode (`g` then type a slide number).
    ///
    /// Only accepts digit characters. Enter jumps to the entered slide number
    /// (1-based), Esc cancels.
    pub(crate) fn handle_goto_key(&mut self, key: KeyEvent) -> Result<bool> {
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

    /// Execute a colon command entered in Command mode.
    ///
    /// Supported commands:
    /// - `:theme <slug>` — Switch to a named theme.
    /// - `:goto <N>` — Jump to slide N (1-based).
    /// - `:notes` — Toggle speaker notes panel.
    /// - `:timer` / `:timer reset` — Start or reset the presentation timer.
    /// - `:overview` — Enter overview grid mode.
    /// - `:help` — Show the help overlay.
    /// - `:reload` — Force-reload the presentation file from disk.
    pub(crate) fn execute_command(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd.trim().splitn(2, ' ').collect();
        match parts.first().copied() {
            Some("theme") => {
                if let Some(slug) = parts.get(1) {
                    let registry = crate::theme::ThemeRegistry::load();
                    if let Some(new_theme) = registry.get(slug.trim()) {
                        self.base_theme = new_theme.clone();
                        self.apply_theme(new_theme);
                    }
                }
            }
            Some("goto") => {
                if let Some(n) = parts.get(1).and_then(|s| s.trim().parse::<usize>().ok()) {
                    self.goto_slide(n.saturating_sub(1));
                }
            }
            Some("notes") => self.toggle_notes(),
            Some("timer") => {
                if parts.get(1).map(|s| s.trim()) == Some("reset") {
                    self.reset_timer();
                } else if self.timer_start.is_none() {
                    self.start_timer();
                }
            }
            Some("overview") => self.mode = Mode::Overview,
            Some("help") => self.mode = Mode::Help,
            Some("reload") => self.try_reload(),
            _ => {}
        }
    }
}
