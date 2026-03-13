//! PTY-based interactive code execution.
//!
//! Uses pseudo-terminal (PTY) allocation for code blocks that need realistic
//! stdin/stdout interaction (the `+pty` modifier on a code fence).  A PTY makes
//! the child process believe it is running in a real terminal, which is important
//! for programs that check `isatty()` or use line buffering tied to a TTY.
//!
//! The `portable_pty` crate provides cross-platform PTY support.  A virtual
//! terminal of 80x24 is allocated, the command is executed via `sh -c`, and
//! all output is captured into a `String`.

use anyhow::Result;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::Read;

/// Run a shell command inside a pseudo-terminal and capture its output.
///
/// # Parameters
///
/// - `command` -- a shell command string (passed to `sh -c`).
///
/// # Returns
///
/// The full stdout/stderr output as a `String`.  Because a PTY merges stdout and
/// stderr into a single stream, they cannot be separated.
///
/// # How it works
///
/// 1. A PTY pair (master + slave) is created with an 80x24 cell size.
/// 2. The command is spawned on the slave side.
/// 3. The slave is dropped so the child sees EOF on stdin.
/// 4. All output is read from the master side until the child exits.
#[allow(dead_code)]
pub fn run_in_pty(command: &str) -> Result<String> {
    let pty_system = NativePtySystem::default();
    let pair = pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    let mut cmd = CommandBuilder::new("sh");
    cmd.args(["-c", command]);

    let mut child = pair.slave.spawn_command(cmd)?;
    // Drop the slave so we get EOF when the child exits
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader()?;
    // Drop master write side so child sees EOF on stdin
    drop(pair.master);

    let mut output = String::new();
    reader.read_to_string(&mut output)?;

    let _ = child.wait();

    Ok(output)
}
