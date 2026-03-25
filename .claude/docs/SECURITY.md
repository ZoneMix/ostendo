# Ostendo Security Review

**Version**: v0.4.1 | **Date**: 2026-03-25

## Threat Model

Ostendo is a terminal presentation tool. The presenter runs it locally on their machine. Threat actors:
- **Untrusted presentation files** — a malicious `.md` file opened by the presenter
- **Network attackers** — targeting the WebSocket remote control (when enabled)
- **Audience** — viewing the projected terminal (no direct access)

## Findings

### HIGH

**F-01: Silent shell fallback for unknown languages** (`executor.rs:134-137, 170`)
Both `execute_code()` and `execute_code_streaming()` fall through to `sh -c` for unrecognized language identifiers. A slide with `` ```oops +exec `` executes code as a shell script silently. The streaming path also lacks compiled-language handling (rust/c/c++/go run as shell).
- **Recommendation**: Return `Err` for unrecognized languages instead of falling back to `sh`.

**F-02: No timeout in execute_code_streaming** (`executor.rs:149-232`)
The synchronous path enforces a 30-second SIGKILL deadline. The streaming path (what Ctrl+E calls) has no watchdog. An infinite loop hangs the background thread indefinitely.
- **Recommendation**: Add a timeout thread that kills the child process group after `EXEC_TIMEOUT_SECS`.

### MEDIUM

**F-03: Fallback executes non-+exec code blocks** (`mod.rs:1175-1180`)
When Ctrl+E is pressed on a slide with no `+exec`-tagged block, the engine falls back to executing the first plain code block. This expands execution surface beyond the `+exec` marker.
- **Recommendation**: Only execute blocks explicitly tagged with `+exec` or `+pty`.

**F-04: No path traversal validation for image paths** (`parser.rs:690-712`)
Image paths like `../../../../etc/passwd` are resolved without checking they stay inside the presentation directory. The image crate fails gracefully but the file is opened.
- **Recommendation**: Canonicalize paths and verify they're under the presentation base directory.

### LOW

**F-05: osascript format string** (`font.rs:82-97`) — No injection risk; interpolated values are `u32` and string literals.

**F-06: No SVG file size check** (`mod.rs:145-182`) — Output capped at 2048px but input parsing is unbounded.

**F-07: No timeout in run_in_pty** (`pty.rs:43-44`) — PTY execution has no watchdog.

**F-08: Token in stderr** (`main.rs:235-236`) — `--remote-token` value appears in startup message.

### INFO

**F-09: Non-browser WebSocket clients bypass origin check** (`server.rs:119-121`) — By design; bearer token protects.

**F-10: CSP uses unsafe-inline** (`server.rs:171`) — Localhost-only; no XSS surface in static HTML.

## Existing Controls

| Control | Location | Status |
|---------|----------|--------|
| Bearer token (constant-time via `subtle`) | `server.rs` | PASS |
| 127.0.0.1-only WebSocket binding | `server.rs` | PASS |
| 8-connection semaphore | `server.rs` | PASS |
| 64KB code input limit | `executor.rs` | PASS |
| 1MB output limit | `executor.rs` | PASS |
| 30s SIGKILL timeout (sync path) | `executor.rs` | PASS |
| Process group isolation (`setsid`) | `executor.rs` | PASS |
| `--no-exec` flag | `main.rs` | PASS |
| `--remote-exec` independent gate | `main.rs` | PASS |
| GIF 800px max dimension | `image_util/mod.rs` | PASS |
| MAX_SLIDES=10000 | `parser.rs` | PASS |

## Accepted Risks

- **Code execution is a feature** — `+exec` blocks intentionally run arbitrary code. Gated behind `--no-exec`.
- **Presenter trusts their own files** — path traversal is low-risk since the presenter creates the presentation.
- **localhost-only remote** — WebSocket server only binds 127.0.0.1, not exposed externally.
