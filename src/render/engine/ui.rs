//! UI chrome: status bar, help overlay, and overview grid mode.
//!
//! These are the "non-content" visual elements of the presentation. They are
//! rendered separately from the slide content and have their own drawing logic:
//!
//! - **Status bar** — Always visible (unless fullscreen). Shows slide number,
//!   timer, theme name, progress bar, and author/date footer.
//! - **Help overlay** — Full-screen two-column reference of all keybindings,
//!   directives, and CLI flags. Rendered directly to the terminal (bypasses
//!   the virtual buffer) since it replaces all content.
//! - **Overview grid** — Two-column list of all slides for quick navigation.
//!   Also rendered directly to the terminal.

use super::*;

impl Presenter {
    /// Build the status bar as a single `StyledLine`.
    ///
    /// The status bar layout (left to right):
    /// - Slide counter badge: `Slide 3/42` (inverted accent colors)
    /// - Timer display: `00:05:23` (on code_bg background)
    /// - Theme name (optional, toggled with `T` key)
    /// - Progress bar (fills remaining space)
    /// - Author/date footer from front matter (right-aligned, dimmed)
    ///
    /// The footer text is truncated if it would overflow the available space.
    ///
    /// # Parameters
    ///
    /// - `width` — Terminal width in columns.
    ///
    /// # Returns
    ///
    /// A `StyledLine` representing the complete status bar, ready for rendering.
    pub(crate) fn build_status_bar(&self, width: usize) -> StyledLine {
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

    /// Render the full-screen help overlay directly to the terminal.
    ///
    /// This bypasses the virtual buffer and writes directly to the terminal
    /// output using `crossterm::queue!()` macros. It clears any Kitty images,
    /// fills the background, and renders a two-column layout of all keybindings
    /// organized into sections (Navigation, Display, Font & Scale, Code Execution,
    /// Animations, Commands, CLI Flags).
    ///
    /// The bottom of the screen shows diagnostic info (image protocol, font
    /// size, theme name) and a "Press any key to close" hint.
    ///
    /// If the terminal is wide enough (>100 columns), a third section of
    /// markdown directives is shown below the two columns.
    pub(crate) fn render_help_buf(&self, w: &mut impl Write) -> Result<()> {
        let tw = self.width as usize;
        let th = self.height as usize;

        // Clear any Kitty images so they don't show through the help overlay
        if self.image_protocol == ImageProtocol::Kitty {
            w.write_all(KITTY_CLEAR_IMAGES)?;
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
                    ("<!-- font_size: 2 -->", "Set font size (-3..7, requires kitty)"),
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

    /// Render the slide overview grid directly to the terminal.
    ///
    /// Displays a two-column list of all slides with their numbers, titles,
    /// and section labels. The currently selected slide is highlighted with
    /// inverted colors (accent background).
    ///
    /// Navigation uses vim-style keys: `j`/`k` for up/down within a column,
    /// `h`/`l` for jumping between columns, Enter to select, Esc to close.
    ///
    /// The grid fills top-down then left-to-right (first column fills
    /// vertically before the second column begins).
    pub(crate) fn render_overview_buf(&self, w: &mut impl Write) -> Result<()> {
        let tw = self.width as usize;
        let th = self.height as usize;

        // Clear entire screen (this also clears any lingering protocol images)
        for row in 0..th {
            queue!(w, cursor::MoveTo(0, row as u16), SetBackgroundColor(self.bg_color))?;
            write!(w, "{}", " ".repeat(tw))?;
        }

        // Clear Kitty images if applicable
        if self.image_protocol == ImageProtocol::Kitty {
            w.write_all(KITTY_CLEAR_IMAGES)?;
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
