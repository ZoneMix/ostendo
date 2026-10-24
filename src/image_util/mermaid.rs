//! Mermaid diagram rendering via the external `mmdc` CLI
//! (`npm install -g @mermaid-js/mermaid-cli`).
//!
//! Diagrams render to transparent PNGs inside a private temporary directory
//! that is removed when the renderer is dropped. mmdc drives a headless
//! Chromium and takes seconds, so [`MermaidRenderer::job`] hands back work
//! for another thread.

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// A wedged browser must not hold a render forever.
const RENDER_TIMEOUT: Duration = Duration::from_secs(20);

/// Page width diagrams are drawn at. Image protocols scale the PNG to its
/// cells, so one size serves every terminal size and no resize re-renders.
const WIDTH: usize = 1600;

pub struct MermaidRenderer {
    /// Created with a random name and owner-only permissions, so other users
    /// cannot pre-create it or plant symlinks for mmdc to write through.
    dir: tempfile::TempDir,
}

impl MermaidRenderer {
    pub fn new() -> Result<Self> {
        Ok(Self {
            dir: tempfile::Builder::new()
                .prefix("ostendo-mermaid-")
                .tempdir()?,
        })
    }

    /// Work that renders `source` to a PNG named by `key` in this renderer's
    /// directory and returns its path.
    ///
    /// The job fails if `mmdc` is not installed, cannot be started, exits
    /// unsuccessfully (the error includes its stderr), or runs longer than
    /// 20 seconds.
    pub fn job(&self, source: &str, key: u64) -> impl FnOnce() -> Result<PathBuf> + Send {
        let dir = self.dir.path().to_path_buf();
        let source = source.to_string();
        move || render(&dir, &source, key)
    }
}

fn is_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        let mut command = Command::new("mmdc");
        command
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        run(command, RENDER_TIMEOUT).is_ok_and(|status| status.success())
    })
}

fn render(dir: &Path, source: &str, key: u64) -> Result<PathBuf> {
    if !is_available() {
        bail!("install mermaid-cli (mmdc) to render");
    }
    let input = dir.join(format!("{key}.mmd"));
    let output = dir.join(format!("{key}.png"));
    let log = dir.join(format!("{key}.log"));
    std::fs::write(&input, source)?;

    let mut command = Command::new("mmdc");
    command
        .arg("-i")
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .arg("-w")
        .arg(WIDTH.to_string())
        .args(["--backgroundColor", "transparent"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        // A file, not a pipe: nothing drains a pipe while we wait.
        .stderr(std::fs::File::create(&log)?);
    let status = run(command, RENDER_TIMEOUT)?;
    if !status.success() {
        let stderr = std::fs::read_to_string(&log).unwrap_or_default();
        bail!(
            "render failed: mmdc exited with {status}: {}",
            stderr.trim()
        );
    }
    Ok(output)
}

/// Run `command` to completion or kill it after `timeout`. On Unix it gets its
/// own process group so the browser processes it spawned die with it.
fn run(mut command: Command, timeout: Duration) -> Result<ExitStatus> {
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command.spawn()?;
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            #[cfg(unix)]
            // SAFETY: killpg only sends a signal; the group id is the child's
            // pid because of process_group(0) above.
            unsafe {
                libc::killpg(child.id() as libc::pid_t, libc::SIGKILL);
            }
            let _ = child.kill();
            let _ = child.wait();
            bail!("{command:?} timed out after {}s", timeout.as_secs());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn run_kills_commands_that_outlive_the_timeout() {
        let mut command = Command::new("sleep");
        command.arg("30");
        let started = Instant::now();
        assert!(run(command, Duration::from_millis(200)).is_err());
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
