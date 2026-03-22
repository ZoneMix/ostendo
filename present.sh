#!/bin/bash
# Launch Ostendo presentation in Kitty with clean environment
# Usage: ./present.sh [presentation_file] [font_size]

PRES="${1:-presentations/dakotacon-2026/presentation_a.md}"
FONT="${2:-24}"

cd "$(dirname "$0")"

env -u TMUX -u TMUX_PANE -u TERM_PROGRAM TERM=xterm-256color \
  kitty \
    -o allow_remote_control=yes \
    -o confirm_os_window_close=0 \
    -o font_size="$FONT" \
    --start-as=maximized \
    ./target/release/ostendo "$PRES"
