//! Low-level styled line writing for terminal output.
//!
//! These `impl Presenter` methods handle writing `StyledLine` data to the
//! terminal, applying foreground/background colors, text attributes, and
//! OSC 66 text scaling.

use anyhow::Result;
use crossterm::{
    queue,
    style::{Attribute, Color, SetAttribute, SetBackgroundColor, SetForegroundColor},
};
use std::io::Write;

use super::*;

impl Presenter {
    /// Write a single styled line to the terminal output buffer using the default background.
    ///
    /// Delegates to `queue_styled_line_with_bg` with the theme's background color.
    /// Each `StyledSpan` in the line is rendered with its own foreground, background,
    /// and text attributes (bold, italic, dim, etc.).
    pub(crate) fn queue_styled_line(&self, w: &mut impl Write, line: &StyledLine, term_width: usize) -> Result<()> {
        self.queue_styled_line_with_bg(w, line, term_width, self.bg_color)
    }

    /// Write a styled line with a custom default background (used for gradient rows).
    pub(crate) fn queue_styled_line_with_bg(&self, w: &mut impl Write, line: &StyledLine, term_width: usize, default_bg: Color) -> Result<()> {
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
}
