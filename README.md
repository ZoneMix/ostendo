# Ostendo

*Latin: ostendo — to show, to display, to exhibit.*

AI-native terminal presentations from markdown.

## Features

- **20 built-in themes** — all WCAG 2.0 compliant contrast ratios
- **Native image protocols** — Kitty, iTerm2, Sixel, and ASCII fallback with auto-detection
- **Live code execution** — `+exec` and `+pty` blocks with real-time streaming output
- **Per-slide font sizing** — via Kitty remote control protocol
- **Smart redraw** — no flicker, even in tmux (synchronized updates)
- **Hot reload** — edit your markdown, slides update live
- **WebSocket remote control** — navigate from any browser or mobile device
- **Rich content** — column layouts, tables, blockquotes, FIGlet ASCII art titles
- **Scrollable speaker notes** — toggle with `n`, scroll with `Shift+N`/`Shift+P`
- **Slide state persistence** — remembers last slide and scale per presentation
- **Image pre-rendering** — cached by path, width, and protocol

## Quick Start

```bash
cargo build --release
./target/release/ostendo presentations/examples/demo.md
./target/release/ostendo presentations/examples/demo.md --theme dracula
```

## Usage

| Flag | Description | Default |
|------|-------------|---------|
| `<file>` | Path to markdown presentation file | (required) |
| `-t, --theme <slug>` | Theme slug | `terminal_green` |
| `-s, --slide <N>` | Start at slide N | `1` |
| `--image-mode <mode>` | `auto\|kitty\|iterm\|sixel\|ascii` | `auto` |
| `--list-themes` | List themes and exit | |
| `--remote` | Enable WebSocket remote control | |
| `--remote-port <N>` | Remote control port | `8765` |
| `--validate` | Validate presentation and exit | |
| `--count` | Print slide count and exit | |
| `--export-titles` | Print slide titles and exit | |
| `--detect-protocol` | Print detected image protocol and exit | |
| `--scale <N>` | Content scale (50-200) | `80` |
| `--fullscreen` | Start without status bar | |
| `--timer` | Start with timer running | |

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `h` / `Left` / `Backspace` | Previous slide |
| `l` / `Right` / `Space` | Next slide |
| `j` / `Down` | Scroll down |
| `k` / `Up` | Scroll up |
| `J` / `K` (shift) | Next/previous section |
| `Ctrl+D` / `Ctrl+U` | Half page down/up |
| `g` + N + `Enter` | Go to slide N |
| `n` | Toggle speaker notes |
| `Shift+N` / `Shift+P` | Scroll speaker notes down/up |
| `f` | Toggle fullscreen |
| `T` | Toggle theme name |
| `?` | Help screen |
| `o` | Slide overview |
| `+` / `-` | Scale up/down |
| `]` / `[` | Font size up/down (Kitty remote control) |
| `Ctrl/Cmd+0` | Reset font size |
| `Ctrl+E` | Execute code block |
| `:` | Command mode |
| `q` | Quit |

## Writing Presentations

Ostendo uses standard markdown with HTML comment directives for slide control. Slides are separated by `---`. See [AGENTS.md](AGENTS.md) for the complete format specification and directive reference.

## AI-Driven Presentations

Ostendo is designed for AI agents to build presentations automatically:

1. Point your AI agent at [`AGENTS.md`](AGENTS.md) — it contains the full format spec
2. Provide your instructions, data, and any image assets
3. The agent generates a complete presentation in `presentations/<name>/presentation.md`
4. Hot reload lets you preview changes as the AI writes them

Ostendo itself was built with Claude Code Max (Opus 4.6) in a weekend — 5557 lines of Rust source code, 20 themes, and full image protocol support, all generated through AI-driven development.

## Themes

| Slug | Name | Background | Accent |
|------|------|------------|--------|
| `terminal_green` | Terminal Green | #0D0D0D | #00FF88 |
| `amber_warning` | Amber Warning | #0D0D0D | #FFB000 |
| `arctic_blue` | Arctic Blue | #0A0E17 | #00D4FF |
| `blood_moon` | Blood Moon | #1A0000 | #CC0000 |
| `blueprint` | Blueprint | #0A1628 | #5BA4CF |
| `catppuccin` | Catppuccin | #1E1E2E | #CBA6F7 |
| `clean_light` | Clean Light | #F5F5F0 | #2563EB |
| `cyber_red` | Cyber Red | #1A1A2E | #FF4444 |
| `dracula` | Dracula | #282A36 | #BD93F9 |
| `frost_glass` | Frost Glass | #0F172A | #38BDF8 |
| `matrix` | Matrix | #000000 | #00FF41 |
| `military_green` | Military Green | #1C2418 | #4A7C3F |
| `minimal_mono` | Minimal Mono | #FFFFFF | #E63946 |
| `neon_purple` | Neon Purple | #13111C | #A855F7 |
| `nord` | Nord | #2E3440 | #88C0D0 |
| `outrun` | Outrun | #1A0A2E | #FF2975 |
| `paper` | Paper | #FAF8F5 | #6B4C3B |
| `solarized` | Solarized | #002B36 | #B58900 |
| `sunset_warm` | Sunset Warm | #1A0A0A | #FF6B35 |
| `vaporwave` | Vaporwave | #1A0033 | #FF6EC7 |

Switch themes at runtime with `:theme <slug>` or press `T` to show the current theme name.

## Image Protocols

Ostendo auto-detects the best image protocol for your terminal:

- **Kitty** — native graphics protocol, best quality and performance (recommended)
- **iTerm2** — inline image protocol (iTerm2, WezTerm)
- **Sixel** — bitmap graphics (xterm, mlterm, foot)
- **ASCII** — character-based rendering (any terminal)

Override with `--image-mode <protocol>` or per-slide with `<!-- image_render: ascii -->`.

Valid `image_render` directive values: `ascii`, `kitty`, `iterm`, `sixel`.

## Terminal Compatibility

| Feature | Kitty | iTerm2 | WezTerm | tmux | Other |
|---------|:-----:|:------:|:-------:|:----:|:-----:|
| Native images | Yes | Yes | Yes | DCS passthrough | Sixel/ASCII |
| Per-slide font sizing | Yes (OSC 66) | No | No | No | No |
| Synchronized updates | Yes | Yes | Yes | Yes | Most |
| Hot reload | Yes | Yes | Yes | Yes | Yes |
| Remote control | Yes | Yes | Yes | Yes | Yes |

**Note:** OSC 66 font sizing theoretically works in any terminal that supports it, but has only been tested in Kitty.

**tmux caveat:** A stale `KITTY_WINDOW_ID` environment variable can cause issues. Unset it or start a fresh tmux session.

## Known Limitations

- Inline formatting markers spanning a wrap boundary will break (blockquotes and bullets)
- FIGlet ASCII titles overflow on narrow terminals with long text
- Font sizing via Kitty remote control protocol — Kitty terminal only
- Protocol images in tmux may have latency on first display

## Code Execution

Mark code blocks as executable:

````markdown
```python +exec {label: "demo"}
print("Hello!")
```
````

- `+exec` — capture stdout and display below the block
- `+pty` — run in a PTY for interactive programs
- Press `Ctrl+E` to execute

## Remote Control

```bash
ostendo presentation.md --remote --remote-port 8765
```

Opens a WebSocket server. Connect from any browser or mobile device to control slide navigation remotely.

## Source Tree

```
src/
  main.rs              CLI entry point
  markdown/
    parser.rs           Markdown + directive parser
  presentation/
    slide.rs            Slide data structures
    state.rs            Persistent state management
  render/
    engine.rs           TUI rendering engine
    layout.rs           Window size / layout math
    text.rs             Styled text spans
    progress.rs         Progress bar
  theme/
    schema.rs           Theme YAML schema
    builtin.rs          Built-in theme loader
    colors.rs           Hex color parsing
  image_util/
    mod.rs              Image utility module
    render.rs           Image rendering (all protocols)
  terminal/
    protocols.rs        Image protocol detection
    ascii_art.rs        ASCII art renderer
  code/
    highlight.rs        Syntax highlighting (syntect)
    executor.rs         Code execution
    pty.rs              PTY execution
  remote/
    server.rs           WebSocket server
    html.rs             Remote control web UI
```

## Building

```bash
cargo build              # Debug build
cargo build --release    # Release build
cargo test               # Run tests
```

Requires Rust 1.75+ (uses `LazyLock`).

## License

Licensed under the [GNU General Public License v3.0](LICENSE). You are free to use, modify, and distribute this software, but all derivative works must also be open source under GPLv3.
