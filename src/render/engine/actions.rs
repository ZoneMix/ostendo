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
        match cmd {
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
        let slide_content = self
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
        let pal = &self.palette;
        let msg = StateMessage {
            msg_type: "state".to_string(),
            slide: self.current + 1,
            total: self.slides.len(),
            slide_title: slide.title.clone(),
            notes: slide.notes.clone(),
            timer: self.timer_text().unwrap_or_default(),
            slide_content,
            section: slide.section.clone(),
            is_fullscreen: self.fullscreen,
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
