# Keyboard Shortcuts

All key bindings from `input.rs`. Organized by mode.

## Normal Mode

### Navigation

| Key | Action |
|---|---|
| `h` / `Left` / `Backspace` | Previous slide |
| `l` / `Right` / `Space` | Next slide |
| `j` / `Down` | Scroll down (1 line) |
| `k` / `Up` | Scroll up (1 line) |
| `J` (shift) | Next section |
| `K` (shift) | Previous section |
| `Ctrl+D` | Scroll down half page |
| `Ctrl+U` | Scroll up half page |
| `g` | Enter Goto mode (type slide number + Enter) |

### Display

| Key | Action |
|---|---|
| `n` | Toggle speaker notes panel |
| `N` (shift, notes visible) | Scroll notes down |
| `P` (shift, notes visible) | Scroll notes up |
| `f` | Toggle fullscreen (hide status bar) |
| `T` | Toggle theme name in status bar |
| `S` | Toggle section labels |
| `D` | Toggle dark/light mode |
| `?` | Show help overlay |
| `o` | Slide overview grid |

### Font & Scale

| Key | Action |
|---|---|
| `+` / `=` | Increase content scale (+5%) |
| `-` | Decrease content scale (-5%) |
| `>` | Increase image scale (+10%) |
| `<` | Decrease image scale (-10%) |
| `]` | Increase font size (+1 offset, Kitty/Ghostty) |
| `[` | Decrease font size (-1 offset, Kitty/Ghostty) |
| `Ctrl+0` / `Cmd+0` | Reset font size to base |

### Code Execution

| Key | Action |
|---|---|
| `Ctrl+E` | Execute code block marked with `+exec` |

### Mode Entry

| Key | Action |
|---|---|
| `:` | Enter Command mode |
| `q` | Quit presentation |

## Command Mode (`:` prefix)

| Command | Action |
|---|---|
| `:theme <slug>` | Switch to named theme |
| `:goto <N>` | Jump to slide N (1-based) |
| `:notes` | Toggle notes panel |
| `:timer` | Start timer (if not running) |
| `:timer reset` | Reset timer to 00:00:00 |
| `:overview` | Enter overview grid mode |
| `:help` | Show help overlay |
| `:reload` | Force-reload presentation from disk |
| `Esc` | Cancel command, return to Normal |
| `Enter` | Execute command |
| `Backspace` | Delete last character |

## Goto Mode (`g` prefix)

| Key | Action |
|---|---|
| `0-9` | Type slide number |
| `Enter` | Jump to entered slide number |
| `Esc` | Cancel, return to Normal |

## Overview Mode (`o`)

| Key | Action |
|---|---|
| `j` / `Down` | Move selection down |
| `k` / `Up` | Move selection up |
| `h` / `Left` | Move to previous column |
| `l` / `Right` | Move to next column |
| `Enter` | Select slide, return to Normal |
| `Esc` / `q` / `o` | Close overview, return to Normal |

## Help Mode (`?`)

| Key | Action |
|---|---|
| Any key | Close help, return to Normal |

## Mouse

| Input | Action |
|---|---|
| Scroll up | Scroll content up (3 lines) |
| Scroll down | Scroll content down (3 lines) |
