# Ostendo

*Latin: to show, to display.*

Markdown slides, presented in your terminal. Write a deck in any editor, run
it with one command, and present live code, images, diagrams, and speaker
notes without leaving the shell.

![Ostendo presenting a code slide](docs/screenshots/code.png)

## Features

- **Plain markdown.** Slides are separated by `---`; directives are HTML
  comments, so decks still read well on GitHub.
- **Live code.** Run Python, Bash, JavaScript, Ruby, Rust, C, C++, or Go
  blocks with Ctrl+E and watch the output stream in, with colors.
- **Images everywhere.** Kitty, Ghostty, iTerm2, WezTerm, and Sixel graphics,
  true-color half blocks in every other terminal, and animated GIFs.
- **Diagrams.** A small arrow syntax renders box, bracket, or vertical flow
  diagrams; Mermaid renders when `mmdc` is installed.
- **Layout.** Columns, tables, quotes, FIGlet titles, footers, and per-slide
  themes, centered and wrapped to any terminal size.
- **Animations.** Fade, slide, and dissolve transitions; typewriter and fade-in
  entrances; matrix, sparkle, pulse, bounce, and spin loops.
- **Presenter tools.** Speaker notes, a timer, a slide overview, sections,
  hot reload on save, and a phone remote over WebSocket.
- **Themes.** Every built-in theme passes WCAG contrast checks, with
  dark/light pairs you can switch on stage.
- **Fast.** Only the rows that change are redrawn, inside synchronized
  updates: no flicker, even over SSH and tmux.
- **Export.** Self-contained HTML, or PDF through headless Chrome.

| | |
|:-:|:-:|
| ![Two code blocks side by side in columns, one with its output](docs/screenshots/columns.png) | ![The slide overview grid](docs/screenshots/overview.png) |
| Columns with live code | Overview (`o`) |

## Install

```bash
cargo install --git https://github.com/ZoneMix/ostendo
```

Or build from a clone with `cargo build --release` (Rust 1.89 or newer).

## Quick start

```bash
ostendo presentations/examples/quick_start.md
```

A minimal deck:

````markdown
---
title: Shipping Rust
author: Ada Lovelace
theme: nord
transition: fade
---

# Shipping Rust
<!-- ascii_title -->
<!-- align: center -->

From prototype to production

---

# Why it works
<!-- section: Background -->

- Fearless refactoring
- One binary to deploy

```python +exec
print("hello from the slide")
```

<!-- notes: Ask who has shipped Rust before. -->
````

Run `ostendo --validate talk.md` before presenting. The full syntax is in
[docs/PRESENTATION_FORMAT.md](docs/PRESENTATION_FORMAT.md); every feature is
demonstrated in `presentations/examples/test_presentation.md`.

## Keys

| Key | Action |
|---|---|
| `→` `l` `Space` `Enter` `PgDn` | Next build step or slide |
| `←` `h` `Backspace` `PgUp` | Previous build step or slide |
| `Home` / `End` | First / last slide |
| `↓` `j` / `↑` `k` | Scroll the slide |
| `Ctrl+D` / `Ctrl+U` | Scroll half a page |
| `J` / `K` | Next / previous section |
| `g` then a number, `Enter` | Go to slide |
| `o` | Overview of all slides |
| `n` | Speaker notes (`N` / `P` scroll them) |
| `/` | Search slide text and notes (`/` then Enter: next match) |
| `b` | Blank the screen (any key brings it back) |
| `Ctrl+E` | Run the code block (again: next block) |
| `f` | Hide the status bar |
| `t` | Start / reset the timer (it starts at the first slide change) |
| `T` | Show the theme name |
| `S` | Section labels above titles |
| `D` | Switch between a theme's dark and light versions |
| `+` / `-` | Content width |
| `>` / `<` | Image size |
| `]` / `[` / `0` | Font size up / down / reset (Kitty, Ghostty) |
| `:` | Command: `theme <slug>`, `goto <n>`, `timer reset`, `notes`, `overview`, `reload`, `q` |
| `?` | Help |
| `q` / `Ctrl+C` | Quit |

Ostendo remembers the slide, theme, and font adjustments for each
presentation.

## Command line

| Option | Effect |
|---|---|
| `-t, --theme <slug>` | Theme (overrides the front matter) |
| `-s, --slide <n>` | Start on slide *n* (default: where you left off) |
| `--image-mode <mode>` | `auto`, `kitty`, `iterm`, `sixel`, `blocks`, `ascii` |
| `--scale <percent>` | Content width, 40–100 (default 80) |
| `--fullscreen` | Start without the status bar |
| `--timer` | Start the timer immediately |
| `--no-exec` | Never run code blocks |
| `--remote` | Serve a remote control page on `127.0.0.1` |
| `--remote-port <port>` | Remote port (default 8765) |
| `--remote-token <token>` | Require a token for the remote |
| `--remote-exec` | Let the remote run code blocks |
| `--validate` | Check the deck and exit |
| `--export html\|pdf` | Export and exit (`-o` sets the path) |
| `--list-themes` | List themes with swatches |
| `--count`, `--export-titles` | Print the slide count or titles |
| `--detect-protocol` | Print the image protocol for this terminal |

## Terminals

| | Kitty | Ghostty | iTerm2 / WezTerm | Others |
|---|:-:|:-:|:-:|:-:|
| Images | Kitty graphics | Kitty graphics | Inline images | Half blocks (Sixel with `--image-mode sixel`) |
| Per-slide font size | Yes¹ | macOS² | – | – |
| Large titles (`text_scale`) | Yes | – | – | – |

1. Needs `allow_remote_control yes` in `kitty.conf`.
2. Sends Ghostty's zoom shortcuts; macOS asks for Accessibility permission
   the first time.

Inside tmux, Kitty and Ghostty images fall back to half blocks and font sizing
is off; iTerm2 images still work.

## Themes

`ostendo --list-themes` shows them all. Choose one with `--theme`, `theme:` in
the front matter, or `:theme <slug>` while presenting. To make your own, see
[docs/THEME_GUIDE.md](docs/THEME_GUIDE.md).

## Remote control

```bash
ostendo talk.md --remote --remote-token "$(openssl rand -hex 16)"
```

Open the printed URL to get a page with navigation buttons and the current
slide's notes. It listens on `127.0.0.1` only; forward the port (for example
with `ssh -L`) to use it from a phone. See [SECURITY.md](SECURITY.md) before
presenting decks you did not write.

## Export

```bash
ostendo talk.md --export html            # talk.html, images embedded
ostendo talk.md --export pdf -o talk.pdf # needs Chrome/Chromium or wkhtmltopdf
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) and [WISHLIST.md](WISHLIST.md).

## License

[GPL-3.0-only](LICENSE)
