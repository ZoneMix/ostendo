# Security

Ostendo renders local markdown files in your terminal. A presentation is
content you chose to open, but it can run code and read files, so treat a deck
from someone else like a script from someone else.

## Reporting a vulnerability

Use GitHub's private vulnerability reporting on this repository (Security →
Report a vulnerability). Please do not open a public issue.

## Code execution

`+exec` and `+pty` blocks run only when the presenter presses Ctrl+E. They run
with your user account and environment; there is no filesystem or network
sandbox. Review every executable block in a deck you did not write, or present
it with `--no-exec`, which disables execution entirely (including from the
remote) and hides the Ctrl+E hints.

Limits on every run:

- 64 KB of source, 1 MB of output, 30 seconds of wall time.
- stdin is closed; the program cannot read your keyboard.
- The program gets its own process group (`setsid`); on timeout, on leaving the
  slide, or on quit, the whole group is killed, including background children.
- Output is displayed through an SGR-only filter: colors survive, but cursor
  movement, OSC, and other escape sequences are dropped, so program output
  cannot rewrite the screen or talk to your terminal.

## Files the deck can read

Image paths are not restricted: `![](/any/path)` displays any image your user
can read. `--export html` embeds only files that decode as images (or SVG files
that contain `<svg`), so pointing an image at a credentials file does not copy
it into the export. Mermaid diagrams are rendered by `mmdc` in a private
temporary directory with a 20 second limit.

## Remote control (`--remote`)

- Listens on `127.0.0.1` only. To reach it from a phone, forward the port
  yourself (for example over SSH); the remote is not built for exposure to a
  network.
- Browser connections are accepted only from `127.0.0.1`, `localhost`, or
  `file://` origins (parsed, not substring matched), which stops other
  websites from driving your presentation.
- `--remote-token TOKEN` requires the token on every connection (checked in
  constant time before the WebSocket upgrade). The startup URL carries it in
  the fragment so the control page can send it. Tokens are limited to
  `A-Z a-z 0-9 . _ ~ -`.
- At most 8 connections at once; a connection that does not finish its
  handshake within 5 seconds is dropped; messages over 4 KB are rejected.
- The remote can navigate, toggle views, and change themes. It can run code
  only with `--remote-exec`, and never with `--no-exec`.
- The control page is served with a restrictive Content-Security-Policy.

## Local state

Ostendo writes `.ostendo-state.<name>.json` next to each presentation (slide,
theme, and font adjustments), `.ostendo-rehearsals.<name>.json` (time per
slide of timed runs, for `--report`), and nothing else outside the export and
recording paths you choose.

## Parsing limits

Presentations are capped at 10,000 slides; directive values are clamped to
their documented ranges.
