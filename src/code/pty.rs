use anyhow::Result;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::Read;

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
