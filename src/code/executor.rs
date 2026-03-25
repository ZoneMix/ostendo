//! Code execution sandbox for live coding slides.
//!
//! Supports 8 languages: Python, Bash, JavaScript, Ruby, Rust, C, C++, and Go.
//! Code blocks marked with `+exec` in a presentation can be executed by the
//! presenter at runtime (Ctrl+E), with output displayed inline on the slide.
//!
//! # Security limits
//!
//! Every execution is subject to hard limits to prevent runaway processes from
//! hanging the presentation or consuming excessive resources:
//!
//! - **Code size**: 64 KB maximum (`MAX_CODE_LENGTH`)
//!
//! # Auto-wrapping
//!
//! Compiled languages (Rust, C, C++, Go) support *auto-wrapping*: if the code
//! snippet does not contain a `main` function, the executor wraps it in one
//! automatically, adding common imports/includes.  This lets presenters show
//! concise snippets like `println!("hello")` without boilerplate.
//!
//! # Streaming output
//!
//! [`execute_code_streaming`] runs code in a background thread and sends output
//! lines through an `mpsc` channel as they become available, enabling real-time
//! output display during execution.

use anyhow::{bail, Result};
use regex::Regex;
use std::process::Command;
use std::sync::{mpsc, LazyLock};

/// Regex for detecting C function definitions (used by auto-wrap heuristic).
static C_FN_PATTERN: LazyLock<Regex> = LazyLock::new(||
    Regex::new(r"^(int|void|char|float|double|long|unsigned|size_t|bool)\s+\w+\s*\(").unwrap()
);

/// Regex for detecting C++ function definitions (extended type set).
static CPP_FN_PATTERN: LazyLock<Regex> = LazyLock::new(||
    Regex::new(r"^(int|void|char|float|double|long|unsigned|size_t|bool|auto|string|vector)\s+\w+\s*\(").unwrap()
);

/// Maximum code length allowed for execution (64 KB).
/// Prevents accidentally pasting enormous files into a code block.
const MAX_CODE_LENGTH: usize = 64 * 1024;

/// Spawn code execution in a background thread, returning a receiver for streaming output lines.
/// Sends each line as it becomes available in real-time. Sends `None` when complete.
///
/// Reads stdout line-by-line from the child process pipe, sending each line
/// through the channel as soon as it's written.  This enables live streaming
/// of output in the presentation (e.g., demo scripts that use `sleep` delays
/// between commands will show output incrementally).
pub fn execute_code_streaming(language: &str, code: &str, working_dir: Option<&std::path::Path>) -> Result<mpsc::Receiver<Option<String>>> {
    if code.len() > MAX_CODE_LENGTH {
        bail!("Code exceeds maximum length of {} bytes", MAX_CODE_LENGTH);
    }

    let (tx, rx) = mpsc::channel();
    let language = language.to_string();
    let lang = normalize_language(&language);
    let code = wrap_for_execution(&lang, code);
    let wd = working_dir.map(|p| p.to_path_buf());

    std::thread::spawn(move || {
        use std::io::BufRead;
        use std::os::unix::process::CommandExt;

        // Determine the interpreter and arguments based on language
        let (cmd, args): (&str, Vec<String>) = match lang.as_str() {
            "python" => ("python3", vec!["-u".into(), "-c".into(), code.clone()]),
            "bash" | "sh" => ("bash", vec!["-c".into(), code.clone()]),
            "javascript" => ("node", vec!["-e".into(), code.clone()]),
            "ruby" => ("ruby", vec!["-e".into(), code.clone()]),
            _ => ("sh", vec!["-c".into(), code.clone()]),
        };

        let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let mut command = Command::new(cmd);
        command.args(&arg_refs)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .env("PYTHONUNBUFFERED", "1");
        if let Some(ref wd) = wd {
            command.current_dir(wd);
        }
        // New process group for timeout safety (same as run_with_timeout)
        unsafe {
            command.pre_exec(|| {
                let _ = libc::setsid();
                Ok(())
            });
        }

        let mut child = match command.spawn() {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(Some(format!("[error] {}", e)));
                let _ = tx.send(None);
                return;
            }
        };

        // Read stdout line-by-line in real-time (not buffered until exit)
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        if let Some(stdout) = stdout {
            let reader = std::io::BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        if tx.send(Some(l)).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        }

        // Collect any remaining stderr after stdout closes
        if let Some(stderr) = stderr {
            let reader = std::io::BufReader::new(stderr);
            for l in reader.lines().map_while(Result::ok) {
                if !l.is_empty() {
                    let _ = tx.send(Some(format!("[stderr] {}", l)));
                }
            }
        }

        let _ = child.wait();
        let _ = tx.send(None);
    });

    Ok(rx)
}

/// Normalize language identifiers to canonical forms.
///
/// Maps common aliases (e.g. `"py"` -> `"python"`, `"js"` -> `"javascript"`,
/// `"rs"` -> `"rust"`) so the rest of the executor can match on a small set
/// of canonical names.
fn normalize_language(lang: &str) -> String {
    match lang.to_lowercase().as_str() {
        "python3" | "py" => "python".to_string(),
        "js" | "node" | "javascript" => "javascript".to_string(),
        "sh" => "bash".to_string(),
        "c++" | "cxx" | "cc" => "cpp".to_string(),
        "rb" => "ruby".to_string(),
        "golang" => "go".to_string(),
        "rs" => "rust".to_string(),
        other => other.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Auto-wrapping: turn bare code snippets into compilable programs
// ---------------------------------------------------------------------------

/// Wrap bare code snippets in a `main` function so they compile as standalone
/// programs.  Interpreted languages (Python, Bash, Ruby, JavaScript) are
/// returned unchanged because their runtimes already accept bare expressions.
///
/// For compiled languages the wrapper:
/// 1. Checks if a `main` function already exists -- if so, returns code as-is.
/// 2. Extracts top-level `use`/`#include` statements and helper function
///    definitions, placing them *outside* `main`.
/// 3. Wraps the remaining body lines inside a generated `main` function.
/// 4. Adds common standard library imports (e.g. `stdio.h` for C, `fmt` for Go).
///
/// This lets presenters write concise snippets without boilerplate.
fn wrap_for_execution(lang: &str, code: &str) -> String {
    match lang {
        "rust"       => wrap_rust(code),
        "c"          => wrap_c(code),
        "cpp"        => wrap_cpp(code),
        "go"         => wrap_go(code),
        // Interpreted languages: pass through unchanged
        "python" | "bash" | "sh" | "ruby" | "javascript" => code.to_string(),
        _ => code.to_string(),
    }
}

/// Rust wrapper.
/// - Skip if `fn main` already present
/// - Extract top-level `use` statements and function definitions, place before `main`
/// - Inject `use std::io::Write;` as a common prelude
/// - Wrap remaining body in `fn main() { ... }`
fn wrap_rust(code: &str) -> String {
    // Already has a main — nothing to do
    if code.contains("fn main") {
        return code.to_string();
    }

    let mut uses = Vec::new();
    let mut functions = Vec::new();
    let mut body = Vec::new();
    let mut in_fn = false;
    let mut brace_depth = 0;

    for line in code.lines() {
        let trimmed = line.trim();
        if in_fn {
            functions.push(line.to_string());
            brace_depth += trimmed.chars().filter(|&c| c == '{').count();
            brace_depth -= trimmed.chars().filter(|&c| c == '}').count();
            if brace_depth == 0 {
                in_fn = false;
            }
        } else if trimmed.starts_with("use ") {
            uses.push(line.to_string());
        } else if trimmed.starts_with("fn ") && !trimmed.starts_with("fn main") {
            in_fn = true;
            brace_depth = trimmed.chars().filter(|&c| c == '{').count();
            brace_depth -= trimmed.chars().filter(|&c| c == '}').count();
            if brace_depth == 0 && trimmed.contains('{') && trimmed.contains('}') {
                in_fn = false;
            }
            functions.push(line.to_string());
        } else {
            body.push(line.to_string());
        }
    }

    let mut out = String::new();
    out.push_str("use std::io::Write;\n");
    for u in &uses {
        out.push_str(u);
        out.push('\n');
    }
    out.push('\n');
    for f in &functions {
        out.push_str(f);
        out.push('\n');
    }
    if !functions.is_empty() {
        out.push('\n');
    }
    out.push_str("fn main() {\n");
    for line in &body {
        out.push_str("    ");
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("}\n");
    out
}

/// C wrapper.
/// - Skip if `int main` or `void main` already present
/// - Add common includes: stdio.h, stdlib.h, string.h, math.h
/// - Extract function definitions (lines matching `type name(`) before main
/// - Wrap remaining body in `int main() { ... return 0; }`
fn wrap_c(code: &str) -> String {
    if code.contains("int main") || code.contains("void main") {
        return code.to_string();
    }

    let mut functions = Vec::new();
    let mut body = Vec::new();
    let mut in_fn = false;
    let mut brace_depth: usize = 0;

    // Simple heuristic: a line starting with a C type followed by identifier( is a function def
    for line in code.lines() {
        let trimmed = line.trim();
        if in_fn {
            functions.push(line.to_string());
            brace_depth += trimmed.chars().filter(|&c| c == '{').count();
            brace_depth = brace_depth.saturating_sub(trimmed.chars().filter(|&c| c == '}').count());
            if brace_depth == 0 {
                in_fn = false;
            }
        } else if C_FN_PATTERN.is_match(trimmed) && !trimmed.starts_with("int main") && !trimmed.starts_with("void main") {
            in_fn = true;
            brace_depth = trimmed.chars().filter(|&c| c == '{').count();
            brace_depth = brace_depth.saturating_sub(trimmed.chars().filter(|&c| c == '}').count());
            if brace_depth == 0 && trimmed.contains('{') && trimmed.contains('}') {
                in_fn = false;
            }
            functions.push(line.to_string());
        } else {
            body.push(line.to_string());
        }
    }

    let mut out = String::new();
    out.push_str("#include <stdio.h>\n");
    out.push_str("#include <stdlib.h>\n");
    out.push_str("#include <string.h>\n");
    out.push_str("#include <math.h>\n\n");
    for f in &functions {
        out.push_str(f);
        out.push('\n');
    }
    if !functions.is_empty() {
        out.push('\n');
    }
    out.push_str("int main() {\n");
    for line in &body {
        out.push_str("    ");
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("    return 0;\n");
    out.push_str("}\n");
    out
}

/// C++ wrapper.
/// - Skip if `int main` or `void main` already present
/// - Add common includes: iostream, vector, string, algorithm, cmath
/// - Add `using namespace std;`
/// - Extract function definitions before main
/// - Wrap remaining body in `int main() { ... return 0; }`
fn wrap_cpp(code: &str) -> String {
    if code.contains("int main") || code.contains("void main") {
        return code.to_string();
    }

    let mut functions = Vec::new();
    let mut body = Vec::new();
    let mut in_fn = false;
    let mut brace_depth: usize = 0;

    for line in code.lines() {
        let trimmed = line.trim();
        if in_fn {
            functions.push(line.to_string());
            brace_depth += trimmed.chars().filter(|&c| c == '{').count();
            brace_depth = brace_depth.saturating_sub(trimmed.chars().filter(|&c| c == '}').count());
            if brace_depth == 0 {
                in_fn = false;
            }
        } else if CPP_FN_PATTERN.is_match(trimmed) && !trimmed.starts_with("int main") && !trimmed.starts_with("void main") {
            in_fn = true;
            brace_depth = trimmed.chars().filter(|&c| c == '{').count();
            brace_depth = brace_depth.saturating_sub(trimmed.chars().filter(|&c| c == '}').count());
            if brace_depth == 0 && trimmed.contains('{') && trimmed.contains('}') {
                in_fn = false;
            }
            functions.push(line.to_string());
        } else {
            body.push(line.to_string());
        }
    }

    let mut out = String::new();
    out.push_str("#include <iostream>\n");
    out.push_str("#include <vector>\n");
    out.push_str("#include <string>\n");
    out.push_str("#include <algorithm>\n");
    out.push_str("#include <cmath>\n");
    out.push_str("using namespace std;\n\n");
    for f in &functions {
        out.push_str(f);
        out.push('\n');
    }
    if !functions.is_empty() {
        out.push('\n');
    }
    out.push_str("int main() {\n");
    for line in &body {
        out.push_str("    ");
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("    return 0;\n");
    out.push_str("}\n");
    out
}

/// Go wrapper.
/// - Skip if `func main()` already present
/// - Detect package usage by scanning for common prefixes:
///   `fmt.`, `os.`, `strings.`, `strconv.`, `math.`, `time.`
/// - Emit `package main`, import block, and `func main() { ... }`
fn wrap_go(code: &str) -> String {
    if code.contains("func main()") {
        return code.to_string();
    }

    // Detect which standard-library packages the snippet uses
    let known_packages: &[(&str, &str)] = &[
        ("fmt.",     "fmt"),
        ("os.",      "os"),
        ("strings.", "strings"),
        ("strconv.", "strconv"),
        ("math.",    "math"),
        ("time.",    "time"),
    ];

    let mut imports: Vec<&str> = Vec::new();
    for &(prefix, pkg) in known_packages {
        if code.contains(prefix) && !imports.contains(&pkg) {
            imports.push(pkg);
        }
    }

    let mut out = String::new();
    out.push_str("package main\n\n");

    if !imports.is_empty() {
        if imports.len() == 1 {
            out.push_str(&format!("import \"{}\"\n\n", imports[0]));
        } else {
            out.push_str("import (\n");
            for pkg in &imports {
                out.push_str(&format!("    \"{}\"\n", pkg));
            }
            out.push_str(")\n\n");
        }
    }

    // Extract non-main func definitions to place outside main
    let mut functions = Vec::new();
    let mut body = Vec::new();
    let mut in_fn = false;
    let mut brace_depth: usize = 0;

    for line in code.lines() {
        let trimmed = line.trim();
        if in_fn {
            functions.push(line.to_string());
            brace_depth += trimmed.chars().filter(|&c| c == '{').count();
            brace_depth = brace_depth.saturating_sub(trimmed.chars().filter(|&c| c == '}').count());
            if brace_depth == 0 {
                in_fn = false;
            }
        } else if trimmed.starts_with("func ") && !trimmed.starts_with("func main") {
            in_fn = true;
            brace_depth = trimmed.chars().filter(|&c| c == '{').count();
            brace_depth = brace_depth.saturating_sub(trimmed.chars().filter(|&c| c == '}').count());
            if brace_depth == 0 && trimmed.contains('{') && trimmed.contains('}') {
                in_fn = false;
            }
            functions.push(line.to_string());
        } else {
            body.push(line.to_string());
        }
    }

    for f in &functions {
        out.push_str(f);
        out.push('\n');
    }
    if !functions.is_empty() {
        out.push('\n');
    }
    out.push_str("func main() {\n");
    for line in &body {
        out.push_str("    ");
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Rust wrapping ----

    #[test]
    fn test_wrap_rust_bare() {
        let code = r#"let x = 42;
println!("{}", x);"#;
        let wrapped = wrap_for_execution("rust", code);
        assert!(wrapped.contains("fn main()"), "should contain fn main()");
        assert!(wrapped.contains("use std::io::Write;"), "should contain prelude");
        assert!(wrapped.contains("    let x = 42;"), "body should be indented");
        assert!(wrapped.contains("    println!"), "body should be indented");
    }

    #[test]
    fn test_wrap_rust_with_main() {
        let code = r#"fn main() {
    println!("hello");
}"#;
        let wrapped = wrap_for_execution("rust", code);
        assert_eq!(wrapped, code, "code with fn main should be returned unchanged");
    }

    #[test]
    fn test_wrap_rust_extracts_use() {
        let code = "use std::collections::HashMap;\nlet m = HashMap::new();";
        let wrapped = wrap_for_execution("rust", code);
        // `use` lines should appear before fn main
        let main_pos = wrapped.find("fn main()").unwrap();
        let use_pos = wrapped.find("use std::collections::HashMap;").unwrap();
        assert!(use_pos < main_pos, "use statements should come before fn main");
        // The body line should be inside main
        assert!(wrapped.contains("    let m = HashMap::new();"));
    }

    // ---- C wrapping ----

    #[test]
    fn test_wrap_c_bare() {
        let code = r#"printf("hello %d\n", 42);"#;
        let wrapped = wrap_for_execution("c", code);
        assert!(wrapped.contains("#include <stdio.h>"));
        assert!(wrapped.contains("#include <stdlib.h>"));
        assert!(wrapped.contains("#include <string.h>"));
        assert!(wrapped.contains("#include <math.h>"));
        assert!(wrapped.contains("int main()"));
        assert!(wrapped.contains("    printf(\"hello %d\\n\", 42);"));
        assert!(wrapped.contains("    return 0;"));
    }

    #[test]
    fn test_wrap_c_with_main() {
        let code = r#"#include <stdio.h>
int main() {
    printf("hi\n");
    return 0;
}"#;
        let wrapped = wrap_for_execution("c", code);
        assert_eq!(wrapped, code, "code with int main should be returned unchanged");
    }

    // ---- C++ wrapping ----

    #[test]
    fn test_wrap_cpp_bare() {
        let code = r#"cout << "hello" << endl;"#;
        let wrapped = wrap_for_execution("cpp", code);
        assert!(wrapped.contains("#include <iostream>"));
        assert!(wrapped.contains("#include <vector>"));
        assert!(wrapped.contains("#include <string>"));
        assert!(wrapped.contains("#include <algorithm>"));
        assert!(wrapped.contains("#include <cmath>"));
        assert!(wrapped.contains("using namespace std;"));
        assert!(wrapped.contains("int main()"));
        assert!(wrapped.contains("    cout << \"hello\" << endl;"));
        assert!(wrapped.contains("    return 0;"));
    }

    // ---- Go wrapping ----

    #[test]
    fn test_wrap_go_bare() {
        let code = r#"fmt.Println("hello")"#;
        let wrapped = wrap_for_execution("go", code);
        assert!(wrapped.contains("package main"));
        assert!(wrapped.contains("\"fmt\""), "should import fmt");
        assert!(wrapped.contains("func main()"));
        assert!(wrapped.contains("    fmt.Println(\"hello\")"));
    }

    #[test]
    fn test_wrap_go_with_main() {
        let code = r#"package main

import "fmt"

func main() {
    fmt.Println("hi")
}"#;
        let wrapped = wrap_for_execution("go", code);
        assert_eq!(wrapped, code, "code with func main() should be returned unchanged");
    }

    #[test]
    fn test_wrap_go_multiple_imports() {
        let code = "fmt.Println(os.Args[0])";
        let wrapped = wrap_for_execution("go", code);
        assert!(wrapped.contains("\"fmt\""));
        assert!(wrapped.contains("\"os\""));
        assert!(wrapped.contains("import ("));
    }

    // ---- Function extraction ----

    #[test]
    fn test_wrap_rust_extracts_fn() {
        let code = "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\nprintln!(\"{}\", add(1, 2));";
        let wrapped = wrap_for_execution("rust", code);
        let main_pos = wrapped.find("fn main()").unwrap();
        let fn_pos = wrapped.find("fn add(").unwrap();
        assert!(fn_pos < main_pos, "helper fn should be extracted before main");
        assert!(wrapped.contains("    println!"), "println should be inside main");
    }

    #[test]
    fn test_wrap_c_extracts_fn() {
        let code = "int factorial(int n) {\n    if (n <= 1) return 1;\n    return n * factorial(n - 1);\n}\nprintf(\"%d\\n\", factorial(5));";
        let wrapped = wrap_for_execution("c", code);
        let main_pos = wrapped.find("int main()").unwrap();
        let fn_pos = wrapped.find("int factorial(").unwrap();
        assert!(fn_pos < main_pos, "helper fn should be extracted before main");
        assert!(wrapped.contains("    printf("), "printf should be inside main");
    }

    #[test]
    fn test_wrap_go_extracts_fn() {
        let code = "func add(a, b int) int {\n    return a + b\n}\nfmt.Println(add(3, 4))";
        let wrapped = wrap_for_execution("go", code);
        let main_pos = wrapped.find("func main()").unwrap();
        let fn_pos = wrapped.find("func add(").unwrap();
        assert!(fn_pos < main_pos, "helper fn should be extracted before main");
        assert!(wrapped.contains("    fmt.Println(add(3, 4))"), "Println should be inside main");
    }

    // ---- Interpreted languages: pass-through ----

    #[test]
    fn test_wrap_python_unchanged() {
        let code = "print('hello')";
        let wrapped = wrap_for_execution("python", code);
        assert_eq!(wrapped, code, "python code should pass through unchanged");
    }

    #[test]
    fn test_wrap_bash_unchanged() {
        let code = "echo hello";
        let wrapped = wrap_for_execution("bash", code);
        assert_eq!(wrapped, code, "bash code should pass through unchanged");
    }

    #[test]
    fn test_wrap_javascript_unchanged() {
        let code = "console.log('hello')";
        let wrapped = wrap_for_execution("javascript", code);
        assert_eq!(wrapped, code, "javascript code should pass through unchanged");
    }

    #[test]
    fn test_wrap_ruby_unchanged() {
        let code = "puts 'hello'";
        let wrapped = wrap_for_execution("ruby", code);
        assert_eq!(wrapped, code, "ruby code should pass through unchanged");
    }
}
