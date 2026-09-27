//! Code execution, hot reload, and the remote-control link.

use crate::presentation::ExecMode;
use crate::remote::{RemoteCommand, StateMessage};
use crate::theme::colors::color_to_hex;

use super::Presenter;

impl Presenter {
    /// Runs the slide's current executable block; once a run has finished,
    /// the next Ctrl+E moves on to the following block.
    pub(crate) fn execute_code(&mut self) {
        if !self.allow_exec {
            return;
        }
        let slide = &self.slides[self.current];
        let blocks = super::exec_blocks(slide);
        if blocks.is_empty() {
            return;
        }
        if self.exec_output.is_some() && self.exec.is_none() {
            self.exec_block += 1;
        }
        self.exec_block %= blocks.len();
        let cb = blocks[self.exec_block];
        let code = match slide.code_preambles.get(&cb.language) {
            Some(preamble) => format!("{preamble}\n{}", cb.code),
            None => cb.code.clone(),
        };
        let cols = u16::try_from(self.layout().content_width.saturating_sub(4)).unwrap_or(u16::MAX);
        let started = crate::code::executor::spawn(
            &cb.language,
            &code,
            cb.exec_mode.unwrap_or(ExecMode::Exec),
            self.presentation_path.parent(),
            cols,
        );
        match started {
            Ok(exec) => {
                self.exec = Some(exec);
                self.exec_output = Some(String::new());
            }
            Err(e) => {
                self.exec = None;
                self.exec_output = Some(format!("\x1b[31m{e}\x1b[39m"));
            }
        }
        self.invalidate();
    }

    /// Moves new output into `exec_output`; returns whether anything changed.
    pub(crate) fn poll_exec_output(&mut self) -> bool {
        let Some(exec) = &self.exec else {
            return false;
        };
        let mut changed = false;
        while let Ok(msg) = exec.try_recv() {
            changed = true;
            let Some(line) = msg else {
                self.exec = None;
                break;
            };
            let output = self.exec_output.get_or_insert_with(String::new);
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(&line);
        }
        changed
    }

    /// Re-reads the presentation and shows the first slide whose source
    /// changed, fully built; otherwise keeps the current position.
    pub(crate) fn reload(&mut self) {
        let Ok(source) = std::fs::read_to_string(&self.presentation_path) else {
            return;
        };
        let Ok((meta, slides)) =
            crate::markdown::parse_presentation(&source, self.presentation_path.parent())
        else {
            return;
        };
        if slides.is_empty() {
            return;
        }
        self.accent_override = crate::theme::colors::hex_to_color(&meta.accent);
        self.meta = meta;
        self.images.preload(&slides);
        self.font.set_directives(&slides);
        let edited = slides
            .iter()
            .zip(&self.slides)
            .position(|(new, old)| new.fingerprint != old.fingerprint)
            .or_else(|| (slides.len() > self.slides.len()).then_some(self.slides.len()));
        self.slides = slides;
        let last = self.slides.len() - 1;
        match edited {
            Some(i) => {
                if i.min(last) != self.current {
                    self.scroll = 0;
                }
                self.current = i.min(last);
                self.step = self.slides[self.current].steps.len();
            }
            None => {
                self.current = self.current.min(last);
                self.step = self.step.min(self.slides[self.current].steps.len());
            }
        }
        self.exec = None;
        self.exec_output = None;
        self.apply_slide_theme();
        self.font.request(Some(self.current));
        self.invalidate();
    }

    /// Moves the slide selected in the overview one place later (or earlier)
    /// in the file, keeping it selected and the current slide in view.
    pub(crate) fn move_slide(&mut self, later: bool) {
        let sel = self.overview_sel;
        let Some(first) = (if later { Some(sel) } else { sel.checked_sub(1) }) else {
            return;
        };
        if first + 1 >= self.slides.len() {
            return;
        }
        let path = std::fs::canonicalize(&self.presentation_path)
            .unwrap_or_else(|_| self.presentation_path.clone());
        let moved = std::fs::read_to_string(&path)
            .ok()
            .and_then(|source| crate::markdown::swap_adjacent_slides(&source, first));
        let Some(source) = moved else {
            self.notify("the file no longer matches the deck; reloaded".to_string());
            self.reload();
            return;
        };
        if let Err(e) = write_atomically(&path, &source) {
            self.notify(format!("cannot write {}: {e}", path.display()));
            return;
        }
        let current = match self.current {
            c if c == first => first + 1,
            c if c == first + 1 => first,
            c => c,
        };
        self.reload();
        self.current = current.min(self.slides.len() - 1);
        self.step = self.slides[self.current].steps.len();
        self.overview_sel = if later { sel + 1 } else { sel - 1 };
        self.apply_slide_theme();
        self.font.request(None);
    }

    pub(crate) fn poll_remote(&mut self) {
        let Some(rx) = self.remote_rx.take() else {
            return;
        };
        while let Ok(cmd) = rx.try_recv() {
            self.remote_command(cmd);
        }
        self.remote_rx = Some(rx);
    }

    fn remote_command(&mut self, cmd: RemoteCommand) {
        if self.blank && !matches!(cmd, RemoteCommand::ToggleBlank) {
            // Like the keyboard: the first command only brings the slide back.
            self.blank = false;
            return;
        }
        match cmd {
            RemoteCommand::ToggleBlank => self.blank = !self.blank,
            RemoteCommand::Next => self.next_slide(),
            RemoteCommand::Prev => self.prev_slide(),
            RemoteCommand::Goto(n) => self.goto_slide(n.saturating_sub(1)),
            RemoteCommand::NextSection => self.next_section(),
            RemoteCommand::PrevSection => self.prev_section(),
            RemoteCommand::ScrollUp => self.scroll_by(-3),
            RemoteCommand::ScrollDown => self.scroll_by(3),
            RemoteCommand::ToggleFullscreen => self.toggle_fullscreen(),
            RemoteCommand::ToggleNotes => self.toggle_notes(),
            RemoteCommand::ToggleThemeName => self.show_theme_name = !self.show_theme_name,
            RemoteCommand::ToggleSections => self.toggle_sections(),
            RemoteCommand::ToggleDarkMode => self.toggle_dark_mode(),
            RemoteCommand::ScaleUp => self.adjust_scale(5),
            RemoteCommand::ScaleDown => self.adjust_scale(-5),
            RemoteCommand::ImageScaleUp => self.adjust_image_scale(10),
            RemoteCommand::ImageScaleDown => self.adjust_image_scale(-10),
            RemoteCommand::FontUp if self.font.available() => self.font.adjust(self.current, 1),
            RemoteCommand::FontDown if self.font.available() => self.font.adjust(self.current, -1),
            RemoteCommand::FontReset if self.font.available() => self.font.reset(self.current),
            RemoteCommand::ExecuteCode if self.allow_remote_exec => self.execute_code(),
            RemoteCommand::TimerStart => {
                self.timer_start.get_or_insert_with(std::time::Instant::now);
            }
            RemoteCommand::TimerReset => self.timer_start = None,
            RemoteCommand::SetTheme(slug) => {
                if let Some(theme) = self.registry.get(&slug) {
                    self.set_base_theme(theme);
                }
            }
            _ => {}
        }
    }

    /// Sends the presenter state to remote clients when it changed.
    pub(crate) fn broadcast_state(&mut self) {
        let Some(tx) = &self.state_broadcast else {
            return;
        };
        if tx.receiver_count() == 0 {
            return;
        }
        let slide = &self.slides[self.current];
        let lines: Vec<String> = self
            .frame_cache
            .as_ref()
            .map(|(_, frame)| {
                frame
                    .lines
                    .iter()
                    .map(|l| {
                        l.spans
                            .iter()
                            .map(|s| s.text.as_str())
                            .collect::<String>()
                            .trim_end()
                            .to_string()
                    })
                    .collect()
            })
            .unwrap_or_default();
        // A phone is narrower than the terminal: drop the centering padding.
        let indent = lines
            .iter()
            .filter(|l| !l.is_empty())
            .map(|l| l.len() - l.trim_start_matches(' ').len())
            .min()
            .unwrap_or(0);
        let slide_content: Vec<String> = lines
            .iter()
            .skip_while(|l| l.is_empty())
            .map(|l| l.get(indent..).unwrap_or("").to_string())
            .collect();
        let pal = &self.palette;
        let msg = StateMessage {
            msg_type: "state".to_string(),
            slide: self.current + 1,
            total: self.slides.len(),
            slide_title: slide.title.clone(),
            notes: slide.notes.clone(),
            timer: self.timer_text().unwrap_or_default(),
            pace: match self.pace() {
                Some((_, true)) => "over time".to_string(),
                Some((Some(late), false)) => format!("{} behind", super::state::clock(late)),
                _ => String::new(),
            },
            up_next: match slide.steps.len() - self.step.min(slide.steps.len()) {
                0 => self
                    .slides
                    .get(self.current + 1)
                    .map_or_else(|| "End of deck".to_string(), |s| s.title.clone()),
                1 => "1 more step on this slide".to_string(),
                n => format!("{n} more steps on this slide"),
            },
            slide_content,
            section: slide.section.clone(),
            is_fullscreen: self.fullscreen,
            is_blank: self.blank,
            is_notes_visible: self.show_notes,
            is_dark_mode: pal.is_dark(),
            show_theme_name: self.show_theme_name,
            show_sections: self.show_sections,
            theme_name: self.theme.name.clone(),
            theme_slug: self.base_theme.slug.clone(),
            scale: self.scale,
            image_scale: self.image_scale_offset,
            font_offset: self.font.offset(self.current),
            has_executable_code: self.allow_exec && !super::exec_blocks(slide).is_empty(),
            timer_running: self.timer_start.is_some(),
            themes: self.registry.list(),
            theme_bg: color_to_hex(pal.bg),
            theme_accent: color_to_hex(pal.accent),
            theme_text: color_to_hex(pal.text),
        };
        let Ok(json) = serde_json::to_string(&msg) else {
            return;
        };
        if self.last_broadcast != json {
            let _ = tx.send(json.clone());
            self.last_broadcast = json;
        }
    }
}

/// Replaces `path` without ever leaving it half written, keeping its permissions.
fn write_atomically(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty());
    let mut tmp = tempfile::NamedTempFile::new_in(dir.unwrap_or(std::path::Path::new(".")))?;
    tmp.write_all(text.as_bytes())?;
    if let Ok(meta) = std::fs::metadata(path) {
        std::fs::set_permissions(tmp.path(), meta.permissions())?;
    }
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}
