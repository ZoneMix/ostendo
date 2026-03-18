# Ostendo Security Model

Ostendo is a **local-first terminal presentation tool**. It reads Markdown files from disk and renders them in your terminal. It is not a web application and does not serve content to the internet.

## Architecture

- **Input**: Local Markdown files only. No URL fetching, no remote content loading.
- **Output**: Terminal rendering via crossterm. Images via Kitty/iTerm2/Sixel protocols.
- **Network**: Optional WebSocket server for remote control, bound to `127.0.0.1` only.
- **Persistence**: Slide position state saved as local JSON files.

## Code Execution (`+exec` / `+pty`)

Ostendo supports executing code blocks marked with `+exec` or `+pty` directives. This is an intentional feature for live demos during presentations.

**Risks:**
- Code is passed directly to shell interpreters (bash, python3, node, rustc)
- Executed code inherits the user's full environment and permissions
- PTY mode (`+pty`) provides an interactive terminal session

**User responsibility:**
- Only open presentations from trusted sources
- Review any `+exec` or `+pty` blocks before presenting
- Code execution requires explicit opt-in via Markdown directives — it does not happen automatically

**Mitigations applied:**
- Code input capped at 64 KB
- Execution timeout of 30 seconds with process group kill (prevents fork bombs and orphaned children)
- Each execution runs in its own process group via `setsid()`
- Output capped at 1 MB to prevent OOM
- Terminal escape sequences stripped from execution output before display

## WebSocket Remote Control

When `--remote <port>` is used, Ostendo starts a WebSocket server for slide navigation from another device (e.g., phone).

**Security measures:**
- Server binds to `127.0.0.1` only — not accessible from other machines
- Origin header validation: only `127.0.0.1`, `localhost`, and `file://` origins accepted
- Origin checked via URL parsing (not substring matching) to prevent bypass
- Incoming message size capped at 4 KB
- Accepted commands: `next`, `prev`, `goto`, `next_section`, `prev_section`, `scroll_up`, `scroll_down`, `toggle_fullscreen`, `toggle_notes`, `toggle_theme_name`, `toggle_sections`, `toggle_dark_mode`, `scale_up`, `scale_down`, `image_scale_up`, `image_scale_down`, `font_up`, `font_down`, `font_reset`, `execute_code`, `timer_start`, `timer_reset`, `set_theme`; all other input ignored
- Remote code execution (`execute_code`) disabled by default; requires `--remote-exec` flag
- Optional bearer token authentication via `--remote-token <TOKEN>`; checked before WebSocket upgrade using constant-time comparison
- Connection concurrency limit: max 8 simultaneous WebSocket connections (not a per-IP rate limit)
- CSP, X-Frame-Options, and X-Content-Type-Options headers on the control page

## Security CLI Flags

Ostendo provides three CLI flags for security hardening:

- **`--no-exec`**: Disables all code execution (`+exec` / `+pty` blocks). When active, `Ctrl+E` is a no-op, exec badges are hidden from rendered slides, and the remote `execute_code` command is also blocked. Use this when presenting untrusted content.

- **`--remote-exec`**: Explicitly allows code execution from the WebSocket remote control. By default, remote `execute_code` commands are silently dropped even when `--remote` is enabled. This flag opts in to remote-triggered execution. Has no effect if `--no-exec` is also set (no-exec takes precedence).

- **`--remote-token <TOKEN>`**: Requires a bearer token for WebSocket connections. When set, all WebSocket upgrade requests must include the token via either an `Authorization: Bearer <TOKEN>` header or a `Sec-WebSocket-Protocol` header carrying the token. Unauthenticated connections receive HTTP 401 and are closed before the upgrade. The remote control URL printed at startup includes the token in a URL hash fragment (`#token=...`) so the embedded HTML page can pass it automatically.

## Image Handling

- Images loaded from local filesystem only — no URL fetching
- SVG rendering via `resvg`/`usvg` with external entities disabled by default
- SVG render size capped at 2048px
- Image scale clamped to 1-100%
- Zero-dimension images handled gracefully (early return)

## File Watcher

- Watches the presentation file for changes (hot reload)
- Polls every 500ms, reads from local filesystem only
- Note: absolute paths and `../` sequences in image directives are not currently restricted

## Presentation Parsing

- Slide count capped at 10,000 to prevent OOM from malicious input
- Font size clamped to 1-7

## Code Quality

- Two justified `unsafe` blocks in `executor.rs` for POSIX process group management (`setsid` + `kill`)
- All HTML output properly escaped with CSP headers
- Dependencies from crates.io with standard Cargo.lock pinning

## Reporting Vulnerabilities

If you discover a security issue, please open a private issue on the GitHub repository or contact the maintainer directly. Do not open public issues for security vulnerabilities.

## Audit Findings (March 2026)

| Finding | Severity | Status |
|---------|----------|--------|
| WebSocket origin substring bypass | Medium | Fixed — URL parsing |
| Unbounded WebSocket message size | Medium | Fixed — 4 KB cap |
| Division by zero in image scaling | Low | Fixed — zero-dimension guard |
| Unbounded code execution output | Medium | Fixed — 1 MB cap |
| Terminal escape injection from exec output | Medium | Fixed — control char stripping |
| Unbounded slide count | Low | Fixed — 10,000 cap |
| Code execution by design (`+exec`/`+pty`) | Info | Mitigated — `--no-exec` flag disables all execution |
| Remote code execution via WebSocket | Medium | Fixed — disabled by default, requires `--remote-exec` |
| Unauthenticated WebSocket access | Medium | Mitigated — `--remote-token` bearer auth available |
| Environment inheritance in exec | Info | Accepted — by design |
| PTY raw output (dead code) | Info | Not reachable |
| Windows build path separator | Low | Fixed — forward slash normalization |
