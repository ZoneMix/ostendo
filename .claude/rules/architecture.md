# Architecture -- Ostendo

## Module Dependency Map

```
main.rs
  -> markdown/parser.rs (parse_presentation)
  -> theme/mod.rs (ThemeRegistry::load)
  -> render/engine/mod.rs (Presenter::new, run)
  -> export/ (html, pdf)
  -> remote/server.rs (RemoteServer::start)

render/engine/mod.rs
  -> render/engine/rendering.rs (render_frame)
  -> render/engine/input.rs (event_loop, handle_key)
  -> render/engine/content.rs (render_table, render_columns, render_ascii_title)
  -> render/engine/ui.rs (build_status_bar, render_help, render_overview)
  -> render/engine/font.rs (kitty_font_size_absolute, ghostty_set_font_size)
  -> render/engine/state.rs (toggle_*, scale_*, apply_theme)
  -> render/engine/navigation.rs (next_slide, prev_slide, on_slide_changed)
  -> render/animation/ (AnimationState, render_*_frame)
  -> code/executor.rs (execute_code_streaming)
  -> image_util/render.rs (render_slide_image)
  -> terminal/protocols.rs (detect_protocol, detect_font_capability)

markdown/parser.rs
  -> markdown/regex_patterns.rs (all LazyLock<Regex> statics)
  -> markdown/inline.rs (parse_inline_formatting)
  -> markdown/tables.rs (TableParseState)
  -> presentation/slide.rs (Slide, all content types)
  -> render/animation/mod.rs (parse_transition, parse_entrance, parse_loop_animation)
```

## Rendering Pipeline

1. **Font change** -- If pending_font_size is set, apply via Kitty RC or Ghostty AppleScript
2. **Mode dispatch** -- Help/Overview modes have dedicated renderers, return early
3. **Smart redraw** -- If nothing changed, only redraw status bar (timer update)
4. **Content assembly** -- Build Vec<StyledLine> from slide data:
   - Section label, title (plain/FIGlet/decorated), subtitle
   - Bullets with inline formatting and word wrapping
   - Syntax-highlighted code blocks with exec badges
   - Tables with Unicode box-drawing borders
   - Block quotes with accent-colored left bar
   - Column layouts (side-by-side merge)
   - Images (protocol-specific or ASCII art)
   - Mermaid and native diagrams
5. **Alignment** -- Apply vertical/horizontal centering per slide directive
6. **Animation overlays** -- Apply active transition, entrance, or loop animations
7. **Viewport** -- Clamp scroll offset, determine visible line range
8. **Flush** -- Write status bar, content, footer, notes, command bar inside synchronized update
9. **Protocol images** -- Emit Kitty/iTerm2/Sixel escape data after text

## Theme Application

- Global theme set in front matter or CLI `--theme`
- Per-slide override: `<!-- theme: slug -->` applies for that slide only
- `apply_slide_theme()` called on every slide change
- Dark/light toggle: themes with `dark_variant` field support `D` key toggle
- WCAG 2.0 contrast validation: text:bg >= 4.5:1, accent:bg >= 3.0:1

## Animation System

Three categories, chained in order:

1. **Transition** (300-600ms): Plays between slides. Blends old + new buffers.
   - Fade (400ms): crossfade through background color
   - SlideLeft (300ms): horizontal slide of old and new content
   - Dissolve (600ms): per-character jumble then resolve
2. **Entrance** (500ms): Plays once when slide appears.
   - Typewriter: characters appear left-to-right
   - FadeIn: all content fades from background to full brightness
   - SlideDown: lines revealed top-to-bottom
3. **Loop** (infinite): Runs while slide is displayed.
   - Matrix: green cascading characters
   - Bounce: bouncing ball (triangle wave)
   - Pulse: brightness oscillation (sine wave)
   - Sparkle: random cells become star characters
   - Spin: ASCII brightness ramp cycling

Targeting: `sparkle(figlet)` or `sparkle(image)` restricts animation to matching `LineContentType`.

## Code Execution

- 8 languages: Python, Bash, JavaScript, Ruby, Rust, C, C++, Go
- Auto-wrapping: Rust/C/C++/Go snippets without `main` get wrapped automatically
- Streaming: background thread sends output via mpsc channel for real-time display
- Sandbox: process groups (`setsid`), 30s timeout, 64KB input, 1MB output
- PTY mode (`+pty`): preserves ANSI escape codes for interactive output

## Image Rendering

1. `detect_protocol()` probes env vars at startup (KITTY_WINDOW_ID, TERM_PROGRAM, etc.)
2. Per-slide `<!-- image_render: mode -->` can override
3. Image loaded and cached with `ImageCacheKey` (path, width, protocol, gif_frame, color)
4. Protocol rendering: Kitty (base64 chunks), iTerm2 (inline), Sixel (icy_sixel), ASCII (half-block)
5. GIF: frames stored as `Arc<Vec<GifFrame>>`, app-driven advance or Kitty native animation
