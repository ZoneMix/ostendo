//! Runs `+exec` / `+pty` code blocks for live demos.
//!
//! Every run is bounded: its own process group (so children die with it), a
//! 30 s deadline, and a 1 MB output cap. Output streams back line by line, and
//! dropping the [`Execution`] kills whatever is still running.
//!
//! Compiled languages (Rust, C, C++, Go) are built in a temp dir first; bare
//! snippets without a `main` are wrapped in one.

use crate::presentation::ExecMode;
use anyhow::{bail, Result};
use regex::Regex;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::{Arc, LazyLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const MAX_CODE_LENGTH: usize = 64 * 1024;
const MAX_OUTPUT_BYTES: usize = 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(20);
/// How long to keep draining pipes after the process group is gone.
const DRAIN_GRACE: Duration = Duration::from_millis(500);

const STDERR_START: &str = "\x1b[31m";
const STDERR_END: &str = "\x1b[39m";

static C_FN_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(static\s+)?(int|void|char|float|double|long|unsigned|size_t|bool)\s+\**\w+\s*\(")
        .unwrap()
});

static CPP_FN_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(static\s+)?(int|void|char|float|double|long|unsigned|size_t|bool|auto|string|vector<.*>|std::\w+)\s+\**\w+\s*\(",
    )
    .unwrap()
});

/// A running (or finished) code execution.
pub struct Execution {
    rx: Receiver<Option<String>>,
    cancel: Arc<AtomicBool>,
}

impl Execution {
    /// Returns the next output line, `Ok(None)` once the run has finished, or
    /// `Err(TryRecvError::Empty)` when nothing new has arrived yet.
    pub fn try_recv(&self) -> Result<Option<String>, TryRecvError> {
        match self.rx.try_recv() {
            Err(TryRecvError::Disconnected) => Ok(None),
            other => other,
        }
    }
}

impl Drop for Execution {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// Starts executing `code` in the background.
///
/// `pty_cols` sets the pseudo-terminal width for [`ExecMode::Pty`].
///
/// # Errors
/// Fails without spawning anything if the code is too long or the language is
/// not supported.
pub fn spawn(
    language: &str,
    code: &str,
    mode: ExecMode,
    working_dir: Option<&Path>,
    pty_cols: u16,
) -> Result<Execution> {
    if code.len() > MAX_CODE_LENGTH {
        bail!("code exceeds {} KB limit", MAX_CODE_LENGTH / 1024);
    }
    let lang = normalize_language(language);
    if !SUPPORTED.contains(&lang) {
        bail!("unsupported language '{language}'");
    }

    let (tx, rx) = mpsc::sync_channel(512);
    let cancel = Arc::new(AtomicBool::new(false));
    let job = Job {
        lang,
        code: code.to_string(),
        pty: mode == ExecMode::Pty,
        pty_cols: pty_cols.max(20),
        working_dir: working_dir.map(Path::to_path_buf),
        sink: Arc::new(Sink {
            tx,
            bytes: AtomicUsize::new(0),
            overflowed: AtomicBool::new(false),
        }),
        cancel: Arc::clone(&cancel),
        deadline: Instant::now() + TIMEOUT,
    };
    thread::spawn(move || {
        if let Err(e) = job.run() {
            job.sink
                .send(format!("{STDERR_START}[error] {e}{STDERR_END}"));
        }
        let _ = job.sink.tx.send(None);
    });
    Ok(Execution { rx, cancel })
}

const SUPPORTED: [&str; 9] = [
    "python",
    "bash",
    "sh",
    "javascript",
    "ruby",
    "rust",
    "c",
    "cpp",
    "go",
];

fn normalize_language(lang: &str) -> &'static str {
    match lang.to_lowercase().as_str() {
        "python" | "python3" | "py" => "python",
        "bash" | "shell" | "zsh" => "bash",
        "sh" => "sh",
        "javascript" | "js" | "node" => "javascript",
        "ruby" | "rb" => "ruby",
        "rust" | "rs" => "rust",
        "c" => "c",
        "cpp" | "c++" | "cxx" | "cc" => "cpp",
        "go" | "golang" => "go",
        _ => "",
    }
}

/// Shared output channel with a byte budget across stdout and stderr.
struct Sink {
    tx: SyncSender<Option<String>>,
    bytes: AtomicUsize,
    overflowed: AtomicBool,
}

impl Sink {
    fn send(&self, line: String) -> bool {
        self.tx.send(Some(line)).is_ok()
    }

    fn remaining(&self) -> usize {
        MAX_OUTPUT_BYTES.saturating_sub(self.bytes.load(Ordering::Relaxed))
    }

    /// Records `n` bytes; returns false (once, with a notice) when over budget.
    fn charge(&self, n: usize) -> bool {
        let total = self.bytes.fetch_add(n, Ordering::Relaxed) + n;
        if total <= MAX_OUTPUT_BYTES {
            return true;
        }
        if !self.overflowed.swap(true, Ordering::Relaxed) {
            self.send(format!(
                "{STDERR_START}[output truncated at {} MB]{STDERR_END}",
                MAX_OUTPUT_BYTES / (1024 * 1024)
            ));
        }
        false
    }
}

struct Job {
    lang: &'static str,
    code: String,
    pty: bool,
    pty_cols: u16,
    working_dir: Option<PathBuf>,
    sink: Arc<Sink>,
    cancel: Arc<AtomicBool>,
    deadline: Instant,
}

struct Step {
    program: String,
    args: Vec<String>,
}

fn step(program: impl Into<String>, args: &[&str]) -> Step {
    Step {
        program: program.into(),
        args: args.iter().map(|a| a.to_string()).collect(),
    }
}

impl Job {
    fn run(&self) -> Result<()> {
        let build_dir = tempfile::Builder::new().prefix("ostendo-").tempdir()?;
        let (build, run) = self.plan(build_dir.path())?;
        if let Some(build) = build {
            if !self.run_step(&build, false)? {
                return Ok(());
            }
        }
        self.run_step(&run, self.pty)?;
        Ok(())
    }

    /// Returns the optional compile step and the run step.
    fn plan(&self, dir: &Path) -> Result<(Option<Step>, Step)> {
        let code = self.code.as_str();
        let path = |name: &str| dir.join(name).to_string_lossy().into_owned();
        let bin = path("main");
        let write = |name: &str, source: String| -> Result<String> {
            let p = path(name);
            std::fs::write(&p, source)?;
            Ok(p)
        };
        Ok(match self.lang {
            "python" => (None, step("python3", &["-u", "-c", code])),
            "bash" => (None, step("bash", &["-c", code])),
            "sh" => (None, step("sh", &["-c", code])),
            "javascript" => (None, step("node", &["-e", code])),
            "ruby" => (None, step("ruby", &["-e", code])),
            "rust" => {
                let src = write("main.rs", wrap_rust(code))?;
                let compile = step(
                    "rustc",
                    &["--edition", "2021", "-A", "warnings", "-o", &bin, &src],
                );
                (Some(compile), step(bin.clone(), &[]))
            }
            "c" => {
                let src = write("main.c", wrap_c(code))?;
                let compile = step("cc", &["-w", "-o", &bin, &src, "-lm"]);
                (Some(compile), step(bin.clone(), &[]))
            }
            "cpp" => {
                let src = write("main.cpp", wrap_cpp(code))?;
                let compile = step("c++", &["-std=c++17", "-w", "-o", &bin, &src]);
                (Some(compile), step(bin.clone(), &[]))
            }
            "go" => {
                let src = write("main.go", wrap_go(code))?;
                (None, step("go", &["run", &src]))
            }
            other => bail!("unsupported language '{other}'"),
        })
    }

    fn cwd(&self) -> PathBuf {
        self.working_dir
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// Runs one step to completion or until cancelled; returns whether it succeeded.
    fn run_step(&self, step: &Step, pty: bool) -> Result<bool> {
        let mut proc = if pty {
            self.spawn_pty(step)?
        } else {
            self.spawn_piped(step)?
        };
        let result = loop {
            if let Some(success) = proc.try_wait()? {
                break success;
            }
            if self.cancel.load(Ordering::Relaxed) || self.sink.overflowed.load(Ordering::Relaxed) {
                proc.kill();
                break false;
            }
            if Instant::now() >= self.deadline {
                proc.kill();
                self.sink.send(format!(
                    "{STDERR_START}[timed out after {}s]{STDERR_END}",
                    TIMEOUT.as_secs()
                ));
                break false;
            }
            thread::sleep(POLL_INTERVAL);
        };
        // Background children may still hold the pipes open.
        proc.kill();
        proc.join_readers();
        Ok(result)
    }

    fn spawn_piped(&self, step: &Step) -> Result<Proc> {
        let mut cmd = Command::new(&step.program);
        cmd.args(&step.args)
            .current_dir(self.cwd())
            .env("PYTHONUNBUFFERED", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            // A new session: the run gets its own process group and no
            // controlling terminal, so it cannot read or draw on the TUI's tty.
            // SAFETY: setsid is async-signal-safe and touches no parent state.
            unsafe {
                cmd.pre_exec(|| {
                    libc::setsid();
                    Ok(())
                });
            }
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| anyhow::anyhow!("{}: {e}", step.program))?;
        let mut readers = Vec::new();
        if let Some(out) = child.stdout.take() {
            readers.push(spawn_reader(out, false, Arc::clone(&self.sink)));
        }
        if let Some(err) = child.stderr.take() {
            readers.push(spawn_reader(err, true, Arc::clone(&self.sink)));
        }
        Ok(Proc {
            kind: ProcKind::Piped(child),
            readers,
        })
    }

    fn spawn_pty(&self, step: &Step) -> Result<Proc> {
        use portable_pty::{native_pty_system, CommandBuilder, PtySize};
        let pair = native_pty_system().openpty(PtySize {
            rows: 40,
            cols: self.pty_cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let mut cmd = CommandBuilder::new(&step.program);
        cmd.args(&step.args);
        cmd.cwd(self.cwd());
        cmd.env("TERM", "xterm-256color");
        cmd.env("PYTHONUNBUFFERED", "1");
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| anyhow::anyhow!("{}: {e}", step.program))?;
        // The master must see EOF once the child exits.
        drop(pair.slave);
        let reader = pair.master.try_clone_reader()?;
        Ok(Proc {
            kind: ProcKind::Pty {
                child,
                _master: pair.master,
            },
            readers: vec![spawn_reader(reader, false, Arc::clone(&self.sink))],
        })
    }
}

enum ProcKind {
    Piped(std::process::Child),
    Pty {
        child: Box<dyn portable_pty::Child + Send + Sync>,
        // Held so the reader does not see a hang-up before the child exits.
        _master: Box<dyn portable_pty::MasterPty + Send>,
    },
}

struct Proc {
    kind: ProcKind,
    readers: Vec<JoinHandle<()>>,
}

impl Proc {
    fn try_wait(&mut self) -> Result<Option<bool>> {
        Ok(match &mut self.kind {
            ProcKind::Piped(c) => c.try_wait()?.map(|s| s.success()),
            ProcKind::Pty { child, .. } => child.try_wait()?.map(|s| s.success()),
        })
    }

    #[cfg(unix)]
    fn pid(&self) -> Option<u32> {
        match &self.kind {
            ProcKind::Piped(c) => Some(c.id()),
            ProcKind::Pty { child, .. } => child.process_id(),
        }
    }

    /// Kills the whole process group (the child is a session leader).
    fn kill(&mut self) {
        #[cfg(unix)]
        if let Some(pid) = self.pid().and_then(|p| i32::try_from(p).ok()) {
            // SAFETY: plain syscall; a stale group id only yields ESRCH.
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
        }
        match &mut self.kind {
            ProcKind::Piped(c) => {
                let _ = c.kill();
                let _ = c.wait();
            }
            ProcKind::Pty { child, .. } => {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    fn join_readers(&mut self) {
        let give_up = Instant::now() + DRAIN_GRACE;
        while self.readers.iter().any(|r| !r.is_finished()) && Instant::now() < give_up {
            thread::sleep(POLL_INTERVAL);
        }
        for reader in self.readers.drain(..).filter(|r| r.is_finished()) {
            let _ = reader.join();
        }
    }
}

fn spawn_reader(
    stream: impl Read + Send + 'static,
    stderr: bool,
    sink: Arc<Sink>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            // Bound each read by the remaining budget so a newline-free flood
            // cannot grow the buffer without limit.
            let limit = sink.remaining() as u64 + 1;
            match (&mut reader).take(limit).read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if !sink.charge(n) {
                        break;
                    }
                    let text = String::from_utf8_lossy(&buf);
                    let line = text.trim_end_matches(['\n', '\r']);
                    let line = if stderr {
                        format!("{STDERR_START}{line}{STDERR_END}")
                    } else {
                        line.to_string()
                    };
                    if !sink.send(line) {
                        break;
                    }
                }
            }
        }
    })
}

/// Splits a snippet into top-level imports, helper items, and body lines.
fn split_snippet(
    code: &str,
    is_import: impl Fn(&str) -> bool,
    is_item_start: impl Fn(&str) -> bool,
) -> (Vec<&str>, Vec<&str>, Vec<&str>) {
    let (mut imports, mut items, mut body) = (Vec::new(), Vec::new(), Vec::new());
    let mut depth: i64 = 0;
    let mut in_item = false;
    for line in code.lines() {
        let trimmed = line.trim();
        let delta = trimmed.matches('{').count() as i64 - trimmed.matches('}').count() as i64;
        if in_item {
            items.push(line);
            depth = (depth + delta).max(0);
            in_item = depth > 0;
        } else if is_import(trimmed) {
            imports.push(line);
        } else if is_item_start(trimmed) {
            items.push(line);
            depth = delta.max(0);
            // A signature whose `{` is on the next line keeps the item open;
            // a one-line body or a `;` prototype closes it.
            in_item = depth > 0 || !(trimmed.contains('}') || trimmed.ends_with(';'));
        } else {
            body.push(line);
        }
    }
    (imports, items, body)
}

fn assemble(
    prelude: &str,
    imports: &[&str],
    items: &[&str],
    main_open: &str,
    body: &[&str],
    main_close: &str,
) -> String {
    let mut out = String::from(prelude);
    for line in imports.iter().chain([&""]).chain(items) {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(main_open);
    for line in body {
        out.push_str("    ");
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(main_close);
    out
}

fn wrap_rust(code: &str) -> String {
    if code.contains("fn main") {
        return code.to_string();
    }
    let (imports, items, body) = split_snippet(
        code,
        |l| l.starts_with("use "),
        |l| l.starts_with("fn ") || l.starts_with("struct ") || l.starts_with("impl "),
    );
    assemble("", &imports, &items, "fn main() {\n", &body, "}\n")
}

fn wrap_c(code: &str) -> String {
    if code.contains("int main") || code.contains("void main") {
        return code.to_string();
    }
    let (imports, items, body) = split_snippet(
        code,
        |l| l.starts_with("#include"),
        |l| C_FN_PATTERN.is_match(l),
    );
    assemble(
        "#include <stdio.h>\n#include <stdlib.h>\n#include <string.h>\n#include <math.h>\n",
        &imports,
        &items,
        "int main(void) {\n",
        &body,
        "    return 0;\n}\n",
    )
}

fn wrap_cpp(code: &str) -> String {
    if code.contains("int main") || code.contains("void main") {
        return code.to_string();
    }
    let (imports, items, body) = split_snippet(
        code,
        |l| l.starts_with("#include") || l.starts_with("using "),
        |l| CPP_FN_PATTERN.is_match(l),
    );
    assemble(
        "#include <iostream>\n#include <vector>\n#include <string>\n#include <algorithm>\n#include <cmath>\nusing namespace std;\n",
        &imports,
        &items,
        "int main() {\n",
        &body,
        "    return 0;\n}\n",
    )
}

fn wrap_go(code: &str) -> String {
    if code.contains("func main()") {
        return code.to_string();
    }
    let packages = ["fmt", "os", "strings", "strconv", "math", "time", "sort"];
    let used: Vec<String> = packages
        .iter()
        .filter(|p| {
            Regex::new(&format!(r"\b{p}\."))
                .map(|re| re.is_match(code))
                .unwrap_or(false)
        })
        .map(|p| format!("import \"{p}\""))
        .collect();
    let (_, items, body) = split_snippet(code, |_| false, |l| l.starts_with("func "));
    let imports: Vec<&str> = used.iter().map(String::as_str).collect();
    assemble(
        "package main\n\n",
        &imports,
        &items,
        "func main() {\n",
        &body,
        "}\n",
    )
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// Collects all output lines, failing if the run does not finish in time.
    fn collect(exec: Execution, limit: Duration) -> Vec<String> {
        let deadline = Instant::now() + limit;
        let mut lines = Vec::new();
        loop {
            match exec.try_recv() {
                Ok(Some(line)) => lines.push(line),
                Ok(None) => return lines,
                Err(_) => {
                    assert!(Instant::now() < deadline, "run did not finish: {lines:?}");
                    thread::sleep(Duration::from_millis(5));
                }
            }
        }
    }

    fn run(lang: &str, code: &str, limit: Duration) -> Vec<String> {
        collect(spawn(lang, code, ExecMode::Exec, None, 80).unwrap(), limit)
    }

    fn installed(program: &str) -> bool {
        Command::new(program)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
    }

    #[test]
    fn stderr_flood_does_not_deadlock() {
        let lines = run(
            "sh",
            "head -c 300000 /dev/zero | tr '\\0' x >&2; echo done",
            Duration::from_secs(10),
        );
        assert!(lines.iter().any(|l| l == "done"), "stdout lost");
        assert!(lines.iter().any(|l| l.starts_with(STDERR_START)));
    }

    #[test]
    fn output_cap_kills_endless_writer() {
        let lines = run("sh", "yes", Duration::from_secs(10));
        assert!(lines.iter().any(|l| l.contains("output truncated")));
    }

    #[test]
    fn background_children_are_killed_with_the_run() {
        // Without the group kill, `sleep` keeps the pipe open for 60s.
        let lines = run("sh", "sleep 60 & echo started", Duration::from_secs(5));
        assert_eq!(lines, ["started"]);
    }

    #[test]
    fn dropping_the_execution_kills_the_process() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("pid");
        let code = format!("echo $$ > {}; sleep 60", pid_file.display());
        let exec = spawn("sh", &code, ExecMode::Exec, None, 80).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let pid = loop {
            if let Ok(pid) = std::fs::read_to_string(&pid_file) {
                if !pid.trim().is_empty() {
                    break pid.trim().to_string();
                }
            }
            assert!(Instant::now() < deadline, "script never started");
            thread::sleep(Duration::from_millis(10));
        };
        drop(exec);
        let alive = || {
            Command::new("kill")
                .args(["-0", &pid])
                .status()
                .unwrap()
                .success()
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        while alive() {
            assert!(Instant::now() < deadline, "process {pid} survived drop");
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn invalid_utf8_is_kept_lossily() {
        let lines = run(
            "sh",
            "printf 'a\\377b\\n'; echo after",
            Duration::from_secs(5),
        );
        assert_eq!(lines, ["a\u{FFFD}b", "after"]);
    }

    #[test]
    fn rejects_unsupported_language_and_oversized_code() {
        assert!(spawn("yaml", "a: 1", ExecMode::Exec, None, 80).is_err());
        let huge = "x".repeat(MAX_CODE_LENGTH + 1);
        assert!(spawn("sh", &huge, ExecMode::Exec, None, 80).is_err());
    }

    #[test]
    fn pty_mode_reports_a_terminal() {
        let exec = spawn("sh", "[ -t 1 ] && echo tty", ExecMode::Pty, None, 80).unwrap();
        let lines = collect(exec, Duration::from_secs(5));
        assert_eq!(lines, ["tty"]);
    }

    #[test]
    fn bare_snippets_compile_and_run() {
        let cases = [
            (
                "rust",
                "use std::collections::HashMap;\nfn sq(x: i32) -> i32 { x * x }\nlet m: HashMap<i32, i32> = (1..4).map(|i| (i, sq(i))).collect();\nprintln!(\"{}\", m[&3]);",
                "rustc",
            ),
            (
                "c",
                "int twice(int n) {\n    return n * 2;\n}\nprintf(\"%d\\n\", twice(21));",
                "cc",
            ),
            ("cpp", "cout << 6 * 7 << endl;", "c++"),
            ("go", "fmt.Println(strings.Repeat(\"a\", 3))", "go"),
        ];
        let expected = ["9", "42", "42", "aaa"];
        for ((lang, code, tool), want) in cases.iter().zip(expected) {
            if !installed(tool) {
                eprintln!("skipping {lang}: {tool} not installed");
                continue;
            }
            let lines = run(lang, code, Duration::from_secs(60));
            assert_eq!(lines, [want], "{lang}");
        }
    }

    #[test]
    fn compile_errors_are_shown_and_stop_the_run() {
        let lines = run("rust", "let x: i32 = \"no\";", Duration::from_secs(60));
        assert!(lines.iter().any(|l| l.contains("mismatched types")));
    }
}
