//! Mermaid diagram rendering via the external `mmdc` CLI
//! (`npm install -g @mermaid-js/mermaid-cli`).
//!
//! Diagrams render to transparent PNGs inside a private temporary directory
//! that is removed when the renderer is dropped. Successful renders are
//! reused for the same `(source, width)`.

use anyhow::{bail, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// mmdc drives a headless Chromium; a wedged browser must not stall the
/// presentation indefinitely.
const RENDER_TIMEOUT: Duration = Duration::from_secs(20);

pub struct MermaidRenderer {
    /// Created with a random name and owner-only permissions, so other users
    /// cannot pre-create it or plant symlinks for mmdc to write through.
    dir: tempfile::TempDir,
    rendered: HashMap<u64, PathBuf>,
}

impl MermaidRenderer {
    pub fn new() -> Result<Self> {
        Ok(Self {
            dir: tempfile::Builder::new()
                .prefix("ostendo-mermaid-")
                .tempdir()?,
            rendered: HashMap::new(),
        })
    }

    pub fn is_available() -> bool {
        let mut command = Command::new("mmdc");
        command
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        run(command, RENDER_TIMEOUT).is_ok_and(|status| status.success())
    }

    /// Render `source` to a PNG `width` pixels wide and return its path.
    ///
    /// # Errors
    ///
    /// Fails if `mmdc` cannot be started, exits unsuccessfully (the error
    /// includes its stderr), or runs longer than 20 seconds. Failures are not
    /// remembered, so a caller that retries every frame should cache them.
    pub fn render(&mut self, source: &str, width: usize) -> Result<PathBuf> {
        let key = cache_key(source, width);
        if let Some(path) = self.rendered.get(&key) {
            return Ok(path.clone());
        }

        let input = self.dir.path().join(format!("{key}.mmd"));
        let output = self.dir.path().join(format!("{key}.png"));
        let log = self.dir.path().join(format!("{key}.log"));
        std::fs::write(&input, source)?;

        let mut command = Command::new("mmdc");
        command
            .arg("-i")
            .arg(&input)
            .arg("-o")
            .arg(&output)
            .arg("-w")
            .arg(width.to_string())
            .args(["--backgroundColor", "transparent"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            // A file, not a pipe: nothing drains a pipe while we wait.
            .stderr(std::fs::File::create(&log)?);
        let status = run(command, RENDER_TIMEOUT)?;
        if !status.success() {
            let stderr = std::fs::read_to_string(&log).unwrap_or_default();
            bail!("mmdc failed ({status}): {}", stderr.trim());
        }

        self.rendered.insert(key, output.clone());
        Ok(output)
    }
}

fn cache_key(source: &str, width: usize) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    width.hash(&mut hasher);
    hasher.finish()
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
