# Ostendo

Terminal presentation tool: markdown in, slides out. Rust, GPL-3.0-only.

## Commands

```bash
cargo build --release
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo run --release -- --validate presentations/examples/test_presentation.md
```

CI (`.github/workflows/ci.yml`) runs fmt, clippy, and tests on every push and PR.
All four must pass before a commit lands.

## Lean code (mandatory)

Invoke the `lean-audit` skill before writing, changing, reviewing, or sweeping
code, tests, comments, or docs. Its gate, in short:

- **Code**: only what a current caller or user-visible behavior needs. Reuse the
  existing helper before writing a new one. No `#[allow(dead_code)]`, no
  "for completeness" variants, no `pub(crate)` that only a test uses.
- **Tests**: each one protects a named contract, fails on a credible regression,
  and is not already covered at a stronger boundary. Test through
  `parse_presentation()`, the public render/export functions, or the CLI rather
  than private helpers. Bug fixes get one regression test that fails before the
  fix.
- **Comments**: explain *why*, never *what*. No history, no version notes, no
  banners, no doc comments that restate the signature.
- **Docs**: no hard-coded counts (lines, tests, files). One source per fact.

## Rendering pipeline

```
markdown source
  -> markdown::parse_presentation()         -> (PresentationMeta, Vec<Slide>)
  -> Presenter::run()                       event loop (render/engine/input.rs)
  -> Presenter::render_frame()              render/engine/rendering.rs
       1. pending font change (Kitty RC / Ghostty)
       2. Help / Overview modes render and return
       3. smart redraw: nothing changed -> status bar only
       4. build Vec<StyledLine> virtual buffer from the slide
       5. alignment (vertical / horizontal centering)
       6. animation overlays: transition -> entrance -> loop
       7. clamp scroll, flush inside Begin/EndSynchronizedUpdate
       8. emit protocol images (Kitty / iTerm2 / Sixel) after text
```

## Module map

| Path | Owns |
|---|---|
| `main.rs` | CLI (clap), early-exit flags, export, launching the presenter |
| `markdown/parser.rs` | Markdown -> slides; directive handling |
| `markdown/regex_patterns.rs` | Every directive regex (`LazyLock<Regex>`) |
| `markdown/inline.rs`, `tables.rs` | Inline formatting, table cells |
| `presentation/slide.rs` | `Slide` and content types |
| `presentation/state.rs` | Persisted per-presentation state (JSON) |
| `render/engine/mod.rs` | `Presenter` struct and lifecycle |
| `render/engine/rendering.rs` | `render_frame()`, smart redraw, viewport |
| `render/engine/input.rs` | Event loop, keys, mouse, remote commands |
| `render/engine/content.rs` | FIGlet / decorated titles, exec output |
| `render/engine/columns.rs`, `table_render.rs` | Column layouts, tables |
| `render/engine/ui.rs` | Status bar, help overlay, overview grid |
| `render/engine/font.rs` | Kitty RC / Ghostty font size and transitions |
| `render/engine/{state,navigation,types,output,line_writer}.rs` | Toggles, slide movement, shared types, span output |
| `render/animation/` | Transitions, entrances, loop animations |
| `render/text.rs` | `StyledLine` / `StyledSpan` virtual buffer |
| `terminal/protocols.rs` | Image protocol and font capability detection |
| `terminal/ascii_art.rs` | Half-block ASCII image renderer |
| `image_util/` | Image loading, protocol rendering, Kitty protocol, Mermaid |
| `diagram/` | ASCII diagram DSL and renderers (box, bracket, vertical) |
| `code/` | Code execution sandbox, PTY, syntax highlighting |
| `export/` | HTML and PDF export |
| `remote/` | WebSocket remote control server and embedded UI |
| `theme/` | Theme registry, schema, colors, WCAG checks; themes are `themes/*.yaml`, embedded by `build.rs` |
| `watch.rs` | Hot-reload file watcher |

## Invariants

- The frame is built as `Vec<StyledLine>` and flushed once; never write to the
  terminal mid-frame.
- Animation functions take `&[StyledLine]` and return a new buffer.
- Horizontal centering must preserve `line.content_type`, or `sparkle(figlet)`
  / `spin(image)` targeting silently breaks.
- Protocol images must check `line_offset >= visible_start`, and Kitty images
  must be cleared when the scroll offset changes, not only on slide change.
- `prerender_images` must build the same `ImageCacheKey` (including per-image
  `image_scale`) that `render_frame` looks up.
- Text width is display width (`unicode-width`), and slicing is char-based.
  Never byte-slice user text.
- `font_size` directive range is -20..=20 (negative = smaller than base).
- Exec output keeps ANSI escapes.
- Code execution: own process group, 30s timeout, 64 KB input, 1 MB output,
  `--no-exec` disables it, `--remote-exec` gates WebSocket execution.

## Conventions

- `anyhow::Result` for fallible paths; no `.unwrap()` on anything derived from
  user markdown, the terminal, the filesystem, or the network.
- Prefer `pub(crate)`; regex statics live in `regex_patterns.rs`.
- Tests live in `#[cfg(test)] mod tests` next to the code (parser tests in
  `markdown/parser_tests.rs`).
- Themes must pass WCAG 2.0: text:bg >= 4.5, accent:bg >= 3.0.

## Where to look

| Task | Start here |
|---|---|
| New directive | `regex_patterns.rs` -> `parser.rs` -> `slide.rs` -> `.claude/docs/DIRECTIVE_REFERENCE.md` |
| New animation | `render/animation/` |
| Key binding | `render/engine/input.rs` -> `.claude/docs/KEYBOARD_SHORTCUTS.md` |
| Status bar | `render/engine/ui.rs` |
| Image protocol | `terminal/protocols.rs`, `image_util/render.rs` |
| New theme | `themes/*.yaml` (see the `theme-authoring` skill) |
| Export format | `export/` |

## Reference

- `.claude/docs/`: directives, keyboard shortcuts, CLI flags, animations, themes
- Skills: `lean-audit`, `presentation-format`, `theme-authoring`, `demo-scripts`
- `presentations/examples/test_presentation.md`: exercises every feature;
  speaker notes carry `FEATURE:` / `EXPECTED:` / `VERIFY:` checks
- `SECURITY.md`: threat model and sandbox details
