# CLI Flags

All flags from the `Cli` struct in `main.rs` (clap derive).

## Usage

```
ostendo [OPTIONS] [FILE]
```

## Positional Arguments

| Argument | Description | Required |
|---|---|---|
| `<file>` | Path to markdown presentation file | Yes (unless using --list-themes or --detect-protocol) |

## Options

| Flag | Short | Type | Default | Description |
|---|---|---|---|---|
| `--theme <slug>` | `-t` | string | `terminal_green` | Theme slug to use |
| `--slide <N>` | `-s` | integer | `1` | Start at specific slide number |
| `--image-mode <mode>` | | string | `auto` | Image render mode: `auto`, `kitty`, `iterm`, `sixel`, `ascii` |
| `--scale <N>` | | integer | `80` | Content scale percentage (50-200) |
| `--fullscreen` | | flag | false | Start with fullscreen mode (no status bar) |
| `--timer` | | flag | false | Start with timer running |
| `--no-exec` | | flag | false | Disable all code execution (+exec/+pty blocks) |
| `--export <FORMAT>` | | string | none | Export to `html` or `pdf` and exit |
| `--output <PATH>` | `-o` | path | auto | Output path for export |

## Early-Exit Flags

These flags perform an action and exit without starting the TUI:

| Flag | Description |
|---|---|
| `--list-themes` | Print all available theme slugs and exit |
| `--detect-protocol` | Detect and print the image protocol, then exit |
| `--validate` | Validate presentation (check images, empty slides) without running TUI |
| `--count` | Print the number of slides and exit |
| `--export-titles` | Print slide titles (one per line) and exit |

## Remote Control Flags

| Flag | Type | Default | Description |
|---|---|---|---|
| `--remote` | flag | false | Enable WebSocket remote control server |
| `--remote-port <N>` | integer | `8765` | Port for the remote control server |
| `--remote-exec` | flag | false | Allow code execution from WebSocket commands |
| `--remote-token <TOKEN>` | string | none | Bearer token for WebSocket authentication |

## Examples

```
ostendo presentation.md
ostendo --theme dracula --slide 5 presentation.md
ostendo --validate presentation.md
ostendo --export html --output slides.html presentation.md
ostendo --remote --remote-token mytoken presentation.md
ostendo --image-mode ascii --scale 90 presentation.md
```
