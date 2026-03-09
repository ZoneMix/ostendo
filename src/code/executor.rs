use anyhow::{bail, Result};
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

/// Maximum code length allowed for execution (64 KB).
const MAX_CODE_LENGTH: usize = 64 * 1024;
/// Maximum output size allowed from execution (1 MB).
const MAX_OUTPUT_SIZE: usize = 1024 * 1024;
/// Default execution timeout in seconds.
const EXEC_TIMEOUT_SECS: u64 = 30;

pub struct ExecutionResult {
    pub stdout: String,
    pub stderr: String,
}

pub fn execute_code(language: &str, code: &str, working_dir: Option<&std::path::Path>) -> Result<ExecutionResult> {
    if code.len() > MAX_CODE_LENGTH {
        bail!("Code exceeds maximum length of {} bytes", MAX_CODE_LENGTH);
    }
    let (cmd, args, ext) = match language {
        "python" | "python3" => ("python3", vec!["-c"], "py"),
        "bash" | "sh" => ("bash", vec!["-c"], "sh"),
        "javascript" | "js" | "node" => ("node", vec!["-e"], "js"),
        "rust" => {
            // For Rust, write to temp file, compile and run
            let dir = tempfile::tempdir()?;
            let src = dir.path().join("main.rs");
            let bin = dir.path().join("main");
            std::fs::write(&src, code)?;
            let compile = Command::new("rustc")
                .arg(&src)
                .arg("-o")
                .arg(&bin)
                .output()?;
            if !compile.status.success() {
                return Ok(ExecutionResult {
                    stdout: String::new(),
                    stderr: String::from_utf8_lossy(&compile.stderr).to_string(),
                });
            }
            let bin_str = bin.to_string_lossy().to_string();
            return run_with_timeout(&bin_str, &[], working_dir);
            // dir (tempdir) is dropped here, cleaning up automatically
        }
        _ => ("sh", vec!["-c"], "sh"),
    };

    if ext == "py" || ext == "sh" || ext == "js" {
        let mut full_args = args;
        full_args.push(code);
        return run_with_timeout(cmd, &full_args, working_dir);
    }

    Ok(ExecutionResult {
        stdout: String::new(),
        stderr: format!("Unsupported language: {}", language),
    })
}

/// Spawn code execution in a background thread, returning a receiver for streaming output lines.
/// Sends each line as it becomes available. Sends `None` when complete.
pub fn execute_code_streaming(language: &str, code: &str, working_dir: Option<&std::path::Path>) -> Result<mpsc::Receiver<Option<String>>> {
    if code.len() > MAX_CODE_LENGTH {
        bail!("Code exceeds maximum length of {} bytes", MAX_CODE_LENGTH);
    }

    let (tx, rx) = mpsc::channel();
    let language = language.to_string();
    let code = code.to_string();
    let wd = working_dir.map(|p| p.to_path_buf());

    std::thread::spawn(move || {
        let result = execute_code(&language, &code, wd.as_deref());
        match result {
            Ok(er) => {
                for line in er.stdout.lines() {
                    let _ = tx.send(Some(line.to_string()));
                }
                if !er.stderr.is_empty() {
                    for line in er.stderr.lines() {
                        let _ = tx.send(Some(format!("[stderr] {}", line)));
                    }
                }
            }
            Err(e) => {
                let _ = tx.send(Some(format!("[error] {}", e)));
            }
        }
        let _ = tx.send(None);
    });

    Ok(rx)
}

/// Run a command with a timeout. Kills the process if it exceeds EXEC_TIMEOUT_SECS.
fn run_with_timeout(cmd: &str, args: &[&str], working_dir: Option<&std::path::Path>) -> Result<ExecutionResult> {
    let mut command = Command::new(cmd);
    command.args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if let Some(wd) = working_dir {
        command.current_dir(wd);
    }
    let mut child = command.spawn()?;

    let timeout = Duration::from_secs(EXEC_TIMEOUT_SECS);
    let start = std::time::Instant::now();

    // Poll for completion with timeout
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                let stdout = child.stdout.take().map(|mut s| {
                    read_limited(&mut s, MAX_OUTPUT_SIZE)
                }).unwrap_or_default();
                let stderr = child.stderr.take().map(|mut s| {
                    read_limited(&mut s, MAX_OUTPUT_SIZE)
                }).unwrap_or_default();
                return Ok(ExecutionResult {
                    stdout,
                    stderr,
                });
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(ExecutionResult {
                        stdout: String::new(),
                        stderr: format!("Execution timed out after {} seconds", EXEC_TIMEOUT_SECS),
                    });
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => bail!("Failed to wait for process: {}", e),
        }
    }
}

/// Read from a stream up to `limit` bytes, returning as a String.
/// If the output exceeds the limit, it is truncated with a message.
fn read_limited(reader: &mut impl std::io::Read, limit: usize) -> String {
    let mut buf = vec![0u8; limit + 1];
    let mut total = 0;
    loop {
        match reader.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => {
                total += n;
                if total > limit {
                    total = limit;
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let mut s = String::from_utf8_lossy(&buf[..total]).into_owned();
    if total == limit {
        s.push_str("\n[output truncated at 1MB]");
    }
    s
}
