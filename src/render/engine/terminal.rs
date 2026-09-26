//! Terminal setup and guaranteed restoration, including after a panic.

use std::io::{self, Write};
use std::sync::{Mutex, Once};

use crossterm::style::Color;
use crossterm::{cursor, execute, terminal};

/// Button and wheel reporting (SGR encoded) without motion events, which would
/// otherwise wake the event loop on every mouse move.
const MOUSE_ON: &str = "\x1b[?1000h\x1b[?1006h";
const MOUSE_OFF: &str = "\x1b[?1000l\x1b[?1006l";

/// Extra bytes to write on exit, e.g. the escape restoring the font size.
static EXIT_EXTRA: Mutex<Vec<u8>> = Mutex::new(Vec::new());

/// Restores the terminal when dropped.
pub(crate) struct TerminalGuard;

impl TerminalGuard {
    pub fn enter() -> io::Result<Self> {
        static HOOK: Once = Once::new();
        HOOK.call_once(|| {
            let previous = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                restore();
                previous(info);
            }));
        });
        terminal::enable_raw_mode()?;
        let mut out = io::stdout();
        execute!(out, terminal::EnterAlternateScreen, cursor::Hide)?;
        out.write_all(MOUSE_ON.as_bytes())?;
        out.flush()?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore();
    }
}

/// Registers bytes to emit during restoration (replacing earlier ones).
pub(crate) fn on_exit(bytes: Vec<u8>) {
    if let Ok(mut extra) = EXIT_EXTRA.lock() {
        *extra = bytes;
    }
}

fn restore() {
    let mut out = io::stdout();
    if let Ok(extra) = EXIT_EXTRA.lock() {
        let _ = out.write_all(&extra);
    }
    // OSC 111 restores the default background; d=A frees Kitty image memory.
    let _ = out.write_all(b"\x1b]111\x1b\\");
    let _ = out.write_all(crate::image_util::kitty::DELETE_ALL_IMAGES.as_bytes());
    let _ = out.write_all(MOUSE_OFF.as_bytes());
    let _ = execute!(out, cursor::Show, terminal::LeaveAlternateScreen);
    let _ = terminal::disable_raw_mode();
}

/// Sets the terminal's default background (OSC 11) so cells the terminal
/// creates itself, e.g. after a font-size change, match the theme.
pub(crate) fn set_background(color: Color) {
    if let Color::Rgb { r, g, b } = color {
        let mut out = io::stdout();
        let _ = write!(out, "\x1b]11;rgb:{r:02x}/{g:02x}/{b:02x}\x1b\\");
        let _ = out.flush();
    }
}
