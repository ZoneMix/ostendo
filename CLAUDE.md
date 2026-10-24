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
  -> markdown::parse_presentation()      (PresentationMeta, Vec<Slide>)
  -> Presenter::run()                    event loop: render/engine/input.rs
  -> Presenter::slide_frame()            frame.rs: lay out the slide once into a
                                         SlideFrame, cached by FrameKey
  -> compose()                           compose.rs: scroll, animations, status
                                         bar, notes, footer, prompt -> Screen
  -> Display::present()                  display.rs: rewrite only changed rows,
                                         diff image placements, inside a
                                         synchronized update
```

Help and overview are Screens built by `chrome.rs` and go through the same
`Display`.

## Module map

| Path | Owns |
|---|---|
| `main.rs` | CLI (clap), `--validate`, `--list-themes`, export, launching the presenter |
| `markdown/split.rs`, `parser.rs` | Front matter and slide boundaries; markdown -> `Slide` |
| `markdown/regex_patterns.rs` | Line patterns (`LazyLock<Regex>`) |
| `markdown/inline.rs`, `tables.rs` | Inline formatting, table cells |
| `presentation/slide.rs` | `Slide`, `Block`, column and content types |
| `presentation/state.rs` | Per-presentation state saved between runs (JSON) |
| `presentation/rehearsal.rs` | Time per slide of timed runs; the `--report` table |
| `render/engine/mod.rs` | `Presenter` struct, `run()` |
| `render/engine/input.rs` | Event loop, tick rate, key bindings, `:` commands |
| `render/engine/navigation.rs`, `state.rs` | Slide changes; themes, toggles, persistence |
| `render/engine/actions.rs` | Code execution, hot reload, remote control |
| `render/engine/frame.rs` | Slide layout and cache, alignment, image placement |
| `render/engine/blocks.rs`, `columns.rs` | Element builders: titles, bullets, code, tables, quotes, callouts, columns |
| `render/engine/figures.rs` | Charts and QR codes |
| `render/engine/compose.rs`, `chrome.rs` | Full-screen assembly; status bar, notes, help, overview |
| `render/engine/display.rs` | Row-diffing terminal writer, image placement |
| `render/engine/images.rs` | Image loading, rendering cache, GIF frames, Mermaid |
| `render/engine/palette.rs` | Colors derived from the theme |
| `render/engine/ansi.rs` | Program output (SGR) -> styled spans |
| `render/engine/font.rs` | Per-slide font size (Kitty RC, Ghostty) |
| `render/engine/terminal.rs` | Terminal setup and restore, panic hook |
| `render/engine/record.rs` | `--record` asciicast writer |
| `render/engine/tests.rs` | Behavior tests on a headless `Presenter` (`presenter()`, `screen()`) |
| `render/animation/` | Transitions, entrances, loop animations |
| `render/text.rs` | `StyledLine` / `StyledSpan`, width-aware wrap and truncate |
| `terminal/protocols.rs` | Image protocol and font capability detection |
| `terminal/ascii_art.rs` | Half-block and character-art image rendering |
| `image_util/` | Image decoding, protocol encoders, Kitty protocol, Mermaid CLI |
| `diagram/` | Diagram DSL and renderers (box, bracket, vertical) |
| `math.rs` | TeX math to Unicode: one line inline, stacked for display |
| `code/` | Execution sandbox, PTY, syntax highlighting |
| `export/` | HTML, PDF, and PowerPoint (`pptx.rs`, with a small `zip.rs`) export |
| `remote/` | WebSocket remote control server and page; `audience.rs` + `vote.html`: poll voting |
| `theme/` | Theme registry, schema, color math; themes are `themes/*.yaml`, embedded by `build.rs` |
| `watch.rs` | Hot-reload file watcher |

## Invariants

- Never write to the terminal outside `Presenter::render` (the font change,
  the OSC 11 background, and `Display::present`); `--validate --size` builds
  a headless `Presenter` and must print nothing but its report.
- Anything that changes how a slide lays out must be in `FrameKey` or call
  `invalidate()`, which bumps `generation`; otherwise a stale cached frame is
  shown.
- Animation functions take `&[StyledLine]` and return a new buffer.
- Alignment must preserve `line.content_type`, or `sparkle(figlet)` /
  `spin(image)` targeting silently breaks.
- Images are placed in the `Screen`, never written directly; `Display` owns
  Kitty placements and re-sends inline images only when their rows change.
- Text width is display width (`unicode-width`), and slicing is char-based.
  Never byte-slice user text.
- `font_size` directive: -20..=20, 1 = the terminal's own size, 4 pt per step.
  `]`/`[` adjustments are saved per presentation, separately.
- Program output is untrusted: `ansi.rs` keeps SGR and drops every other
  escape.
- Code execution: own process group, 30 s timeout, 64 KB input, 1 MB output,
  no stdin; `--no-exec` disables it, `--remote-exec` gates WebSocket execution.

## Conventions

- `anyhow::Result` for fallible paths; no `.unwrap()` on anything derived from
  user markdown, the terminal, the filesystem, or the network.
- Prefer `pub(crate)`.
- Tests live in `#[cfg(test)] mod tests` next to the code (parser tests in
  `markdown/parser_tests.rs`). Presenter behavior (keys, build steps, what
  lands on screen) is tested through `render/engine/tests.rs`, which drives a
  real `Presenter` without a terminal.
- Themes must pass WCAG 2.0: text:bg >= 4.5, accent:bg >= 3.0.

## Where to look

| Task | Start here |
|---|---|
| New directive | `markdown/parser.rs` (`slide_directive`) -> `presentation/slide.rs` -> `docs/PRESENTATION_FORMAT.md` |
| New slide element | `presentation/slide.rs` (`Block`) -> `markdown/parser.rs` -> `render/engine/blocks.rs` or `figures.rs` -> `frame.rs` -> `export/html.rs` -> `navigation.rs` (`searchable`) |
| Build steps / pauses | `presentation/slide.rs` (`Step`) -> `frame.rs` (`conceal`) -> `navigation.rs` |
| New animation | `render/animation/` |
| Key binding | `render/engine/input.rs` (`normal_key`) -> help in `chrome.rs` -> README |
| Status bar | `render/engine/chrome.rs` (`status_bar`) |
| Image protocol | `terminal/protocols.rs`, `image_util/render.rs`, `render/engine/display.rs` |
| New theme | `themes/*.yaml` (see the `theme-authoring` skill) |
| Export format | `export/` |

## Reference

- `docs/PRESENTATION_FORMAT.md`: every directive and syntax rule
- `docs/THEME_GUIDE.md`: theme schema and contrast rules
- `README.md`: keys and CLI flags
- Skills: `lean-audit`, `presentation-format`, `theme-authoring`, `demo-scripts`
- `presentations/examples/test_presentation.md`: exercises every feature;
  speaker notes carry `FEATURE:` / `EXPECTED:` / `VERIFY:` checks
- `SECURITY.md`: threat model and sandbox details
