# Ostendo Development Guide

## Build & Test

```bash
cargo build --release          # Release build
cargo test                     # Run all tests (128 tests)
cargo run --release -- --validate presentations/examples/test_presentation.md  # Validate presentation
```

## Architecture

```
src/
  main.rs              # CLI (clap), entry point
  render/engine/       # Core renderer (split into submodules)
    mod.rs             # Presenter struct, lifecycle (new/run), shared helpers (~650 lines)
    rendering.rs       # render_frame(), status bar redraw (~1175 lines)
    input.rs           # Event loop, key handling, remote polling (~440 lines)
    content.rs         # Tables, columns, titles, exec output (~450 lines)
    ui.rs              # Status bar, help overlay, overview grid (~350 lines)
    font.rs            # Terminal font/bg control protocols (~200 lines)
    state.rs           # Toggles, scale, theme, persistence (~137 lines)
    navigation.rs      # Slide movement, scrolling, animations (~120 lines)
  render/layout.rs     # WindowSize, terminal dimensions
  render/text.rs       # StyledLine/StyledSpan (virtual buffer)
  render/progress.rs   # Progress bar rendering
  markdown/parser.rs   # Markdown -> Vec<Slide> parser
  presentation/        # Slide struct, StateManager (JSON persistence)
  terminal/protocols.rs # Image protocol detection, FontSizeCapability
  theme/               # Theme registry, schema, color utilities
  code/                # Syntax highlighting, code execution
  image_util/          # Image loading, rendering (protocol-specific)
  image_util/mermaid.rs # Mermaid diagram rendering via mmdc CLI
  export/html.rs       # Self-contained HTML export
  export/pdf.rs        # PDF export via headless Chrome/wkhtmltopdf
  render/animation.rs  # Slide transitions + entrance/looping animations
  watch.rs             # File watcher for hot reload
  remote/              # WebSocket remote control server
```

## Key Patterns

- **Virtual buffer**: Rendering builds `Vec<StyledLine>` in memory, then writes to terminal in one pass within `BeginSynchronizedUpdate`/`EndSynchronizedUpdate`
- **Smart redraw**: `render_frame()` tracks last-rendered state; timer-only ticks only update the status bar (no image re-emission)
- **Image caching**: `image_cache` keyed by `(path, content_width, protocol, gif_frame_index)` — stale entries are naturally unreachable
- **Hot reload**: Background `FileWatcher` polls every 500ms, triggers `try_reload()` which re-parses and preserves slide position

## Terminal Requirements

- **Recommended**: Kitty terminal — full feature support (native image protocol, per-slide font sizing via DCS, OSC 66 per-element text scaling, best rendering quality)
- **iTerm2**: Supports images via inline image protocol; no font sizing support
- **tmux**: Works with DCS passthrough for images; stale `KITTY_WINDOW_ID` may cause issues — unset it or start a fresh session
- **Other terminals**: Sixel or ASCII fallback for images; font sizing gracefully degrades
- Synchronized updates prevent flicker in most terminals

## Theme System

- 29 built-in themes in `themes/*.yaml`
- All themes must pass contrast ratio tests (WCAG 2.0):
  - text:bg >= 4.5:1
  - accent:bg >= 3.0:1
  - code_bg:bg >= 1.2:1
- Runtime theme switching via `:theme <slug>` command

## Test Presentation

`presentations/examples/test_presentation.md` has 89 slides testing every feature. Each slide's speaker notes contain:
```
FEATURE: [name]
EXPECTED: [expected visual]
VERIFY: [what to check]
```

Use `AGENTS.md` feedback format: `Slide N - [Feature]: PASS/FAIL - [description]`

## Known Limitations

- Inline formatting markers spanning a wrap boundary will break (blockquotes and bullets)
- FIGlet ASCII titles overflow on narrow terminals with long text
- Font sizing via Kitty remote control protocol — Kitty terminal only
- Protocol images in tmux may have latency on first display
- Animated GIFs are downscaled to 800px max dimension for memory efficiency

## Feature Batches (v0.2.0)

### Batch 1: Quick Wins (author footer, alignment, accent override)
- [x] Create PresentationMeta struct + change parser return type
- [x] Feature 9: author/date in status bar + per-slide footer directive
- [x] Feature 6: per-slide vertical centering via <!-- align: center -->
- [x] Feature 11a: front matter accent color override
- [x] Tests + verification

### Batch 2: Theme System (gradients, title decorations, light/dark)
- [x] Feature 11b: gradient backgrounds (ThemeGradient, per-row interpolation)
- [x] Feature 11c: decorated title bars (underline/box/banner)
- [x] Feature 11d: light/dark theme variants + D keybinding
- [x] Create 3 light variant theme files
- [x] Tests + verification

### Batch 3: Animation System (transitions + entrance/looping)
- [x] Create src/render/animation.rs module
- [x] Feature 2: slide transitions (fade, slide-left, dissolve)
- [x] Feature 1a: entrance animations (typewriter, fade_in, slide_down)
- [x] Feature 1b: looping animations (matrix, bounce, pulse)
- [x] Tests + verification

### Batch 4: Code Execution (C/C++/Go/Ruby + preambles)
- [x] Feature 5a: C/C++/Go/Ruby language support + compiler detection
- [x] Feature 5b: code preamble directives
- [x] Tests + verification

### Batch 5: External Integrations (Mermaid + export)
- [x] Feature 7: Mermaid rendering via mmdc CLI
- [x] Feature 8a: HTML export (self-contained, themed)
- [x] Feature 8b: PDF export via headless Chrome/wkhtmltopdf
- [x] Tests + verification

## v0.2.1 Fixes & Enhancements

- [x] Dissolve transition: per-character jumbling with random symbols
- [x] Matrix animation: top-to-bottom rain with ASCII chars, full-width
- [x] Bounce animation: ball overlays all content, full-width
- [x] New loop animations: sparkle (twinkling stars), spin (ASCII ramp cycling)
- [x] Multi-block code execution: Ctrl+E cycles through executable blocks per slide
- [x] Column code blocks show +exec badge
- [x] Per-slide footer bar at bottom of screen (not in top status bar)
- [x] Footer alignment: `<!-- footer_align: left|center|right -->`
- [x] Image scroll fix: proper viewport bounds check for protocol images
- [x] Kitty image clear on scroll offset change
- [x] PDF export fix: flex-direction column in @media print CSS
- [x] Image scale caching: removed unnecessary cache clear on > < keys
- [x] FIGlet + sparkle/spin/matrix/bounce/pulse animation combinations
- [x] ASCII art image + sparkle/spin animation combinations
- [x] Image color override: `<!-- image_color: #hex -->`
- [x] Alignment variants: vcenter, hcenter (in addition to center/top)
- [x] Auto-wrap code execution for Rust/Go/C (no main function needed)
- [x] 99-slide test presentation covering every feature

## v0.3.0 Enhancements

- [x] Animated GIF support (background frame decoding, downscaled to 800px max)
- [x] Font size directive expanded: accepts -3 to 7 (negative = smaller than base)
- [x] `font_transition: none` directive for instant font changes
- [x] Loop animation targeting: `sparkle(figlet)`, `sparkle(image)`, `spin(figlet)`, etc.
- [x] LineContentType system for selective animation
- [x] Matrix rain character-level granularity (fills FIGlet whitespace)
- [x] `image_color` override now functional for ASCII art
- [x] Theme persistence across restarts
- [x] Help menu renders at base font size
- [x] GIF frame advancement within synchronized update blocks
- [x] Security: `--no-exec`, `--remote-exec`, `--remote-token` CLI flags
- [x] Security: WebSocket `execute_code` gated behind `--remote-exec`
- [x] Security: Token-based WebSocket authentication
