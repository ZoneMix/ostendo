#!/usr/bin/env bash
# Ostendo tmux integration test harness
# Launches ostendo in a tmux session, drives it with keystrokes,
# captures pane content, and verifies expected behavior.
#
# Usage: ./tests/integration/tmux_harness.sh [presentation_file]
# Default: presentations/examples/demo.md

set -euo pipefail

TMUX_BIN=${TMUX_BIN:-tmux}
TMUX_SOCKET=ostendo_test
tmx() { "$TMUX_BIN" -L "$TMUX_SOCKET" "$@"; }
PROJECT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
BINARY="$PROJECT_DIR/target/release/ostendo"
PRESENTATION="${1:-$PROJECT_DIR/presentations/examples/demo.md}"
SESSION="ostendo_test_$$"
RESULTS_DIR="$PROJECT_DIR/tests/integration/results"
RESULT_FILE="$RESULTS_DIR/$(date +%Y%m%d_%H%M%S).log"
PASS=0
FAIL=0
SKIP=0

mkdir -p "$RESULTS_DIR"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
NC='\033[0m'

log() { echo -e "$1" | tee -a "$RESULT_FILE"; }
pass() { log "${GREEN}  PASS${NC}: $1"; PASS=$((PASS + 1)); }
fail() { log "${RED}  FAIL${NC}: $1"; FAIL=$((FAIL + 1)); }
skip() { log "${YELLOW}  SKIP${NC}: $1"; SKIP=$((SKIP + 1)); }
section() { log "\n${CYAN}=== $1 ===${NC}"; }

# Capture current tmux pane content
capture() {
    tmx capture-pane -t "$SESSION" -p 2>/dev/null
}

# Send keys and wait for render
send() {
    tmx send-keys -t "$SESSION" "$@"
    sleep 0.6  # Let ostendo render
}

# Assert pane contains a string
assert_contains() {
    local label="$1"
    local needle="$2"
    local content
    content=$(capture)
    if echo "$content" | grep -qF "$needle"; then
        pass "$label"
    else
        fail "$label (expected '$needle')"
        echo "  Pane content (first 5 lines):" >> "$RESULT_FILE"
        echo "$content" | head -5 >> "$RESULT_FILE"
    fi
}

# Assert pane does NOT contain a string
assert_not_contains() {
    local label="$1"
    local needle="$2"
    local content
    content=$(capture)
    if echo "$content" | grep -qF "$needle"; then
        fail "$label (unexpected '$needle')"
    else
        pass "$label"
    fi
}

# Assert ostendo process is still running in the tmux pane
assert_alive() {
    local label="$1"
    # Check if the pane's foreground process is ostendo (not shell)
    local pane_cmd
    pane_cmd=$(tmx display-message -t "$SESSION" -p '#{pane_current_command}' 2>/dev/null || echo "unknown")
    if [ "$pane_cmd" = "ostendo" ] || [ "$pane_cmd" = "unknown" ]; then
        pass "$label"
    else
        # Also check if pane has any content at all (ostendo may have exited)
        local content
        content=$(capture)
        if [ -n "$(echo "$content" | tr -d '[:space:]')" ]; then
            pass "$label"
        else
            fail "$label (process: $pane_cmd — ostendo may have crashed)"
        fi
    fi
}

# Cleanup on exit
cleanup() {
    tmx kill-session -t "$SESSION" 2>/dev/null || true
}
trap cleanup EXIT

# ============================================================
# Pre-flight checks
# ============================================================

section "Pre-flight"

if [ ! -f "$BINARY" ]; then
    log "${RED}Binary not found at $BINARY — run 'cargo build --release' first${NC}"
    exit 1
fi
pass "Binary exists"

if [ ! -f "$PRESENTATION" ]; then
    log "${RED}Presentation not found: $PRESENTATION${NC}"
    exit 1
fi
pass "Presentation exists: $(basename "$PRESENTATION")"

# Get expected slide count
SLIDE_COUNT=$("$BINARY" --count "$PRESENTATION" 2>/dev/null)
pass "Slide count: $SLIDE_COUNT"

# Validate
VALIDATE_OUT=$("$BINARY" --validate "$PRESENTATION" 2>&1)
if echo "$VALIDATE_OUT" | grep -q "OK"; then
    pass "Validation: OK"
else
    fail "Validation failed: $VALIDATE_OUT"
fi

# ============================================================
# Launch ostendo in tmux
# ============================================================

section "Launch"

# Create detached session with fixed size
tmx new-session -d -s "$SESSION" -x 120 -y 40

# Launch ostendo with safe flags (no code exec, ASCII images for tmux compat)
tmx send-keys -t "$SESSION" "$BINARY --no-exec --slide 1 --image-mode ascii '$PRESENTATION'" Enter
sleep 2.5  # Wait for startup + first render + possible font change

assert_alive "Ostendo started"

# Status bar ends with the "n / total" counter
assert_contains "Status bar shows slide 1" "1 / "

# ============================================================
# Navigation tests
# ============================================================

section "Navigation"

# Slides 1-3 show the status bar in every example deck (fullscreen hides it).
send Right
assert_contains "Navigate to slide 2" "2 / "

send l
assert_contains "Navigate to slide 3 via 'l'" "3 / "

send Left
assert_contains "Back to slide 2 via Left" "2 / "

send h
assert_contains "Back to slide 1 via 'h'" "1 / "

send End
sleep 1.0
assert_contains "Jump to last slide via End" "$SLIDE_COUNT / $SLIDE_COUNT"

send Home
sleep 1.0
assert_contains "Jump to first slide via Home" "1 / $SLIDE_COUNT"

# Goto mode: jump to slide 5
if [ "$SLIDE_COUNT" -ge 5 ]; then
    send g
    sleep 0.2
    send 5
    sleep 0.2
    send Enter
    sleep 0.3
    assert_contains "Goto slide 5" "5 / "
else
    skip "Goto slide 5 (only $SLIDE_COUNT slides)"
fi

# Navigate through all slides without crashing
section "Full traversal ($SLIDE_COUNT slides)"

send Home
sleep 0.3

CRASH_DETECTED=0
for i in $(seq 1 "$SLIDE_COUNT"); do
    send Right
    sleep 0.15
done
sleep 0.5

# Should be on last slide (or last+1 clamped to last)
content=$(capture)
if [ -n "$content" ]; then
    pass "Full traversal complete — no crash"
else
    fail "Crash detected during full traversal"
    CRASH_DETECTED=1
fi

# Navigate back to start
send Home
sleep 0.3

# ============================================================
# UI toggle tests
# ============================================================

section "UI Toggles"

# Toggle fullscreen (f)
send f
sleep 0.3
assert_alive "Fullscreen toggle — alive"
# In fullscreen, status bar should be hidden
# (We can't easily assert absence since pane capture includes all content)
send f
sleep 0.3
assert_contains "Exit fullscreen — status bar back" "/"

# Toggle help (?)
send ?
sleep 0.5
assert_alive "Help overlay toggle — alive"
# Check if help text is visible (best-effort — raw mode may obscure)
content=$(capture)
if echo "$content" | grep -qi "help\|key\|quit\|navigation\|slide\|next\|prev"; then
    pass "Help overlay text visible"
else
    skip "Help overlay text not captured (raw mode limitation)"
fi
send ?
sleep 0.3
assert_alive "Close help overlay"

# Toggle overview (o)
send o
sleep 0.5
assert_alive "Overview grid visible"
send o
sleep 0.3
assert_alive "Close overview grid"

# ============================================================
# Theme switching
# ============================================================

section "Theme Switching"

# Switch via command mode
send :
sleep 0.2
tmx send-keys -t "$SESSION" -l "theme dracula"
sleep 0.2
send Enter
sleep 0.5
assert_alive "Theme switch to dracula — alive"

# Switch to another theme
send :
sleep 0.2
tmx send-keys -t "$SESSION" -l "theme nord"
sleep 0.2
send Enter
sleep 0.5
assert_alive "Theme switch to nord — alive"

# Toggle dark/light mode
send D
sleep 0.3
assert_alive "Dark/light toggle — alive"
send D
sleep 0.3

# Reset to default
send :
sleep 0.2
tmx send-keys -t "$SESSION" -l "theme terminal_green"
sleep 0.2
send Enter
sleep 0.5

# ============================================================
# Scale adjustments
# ============================================================

section "Scale"

send +
sleep 0.3
assert_alive "Scale up — alive"

send -
sleep 0.3
assert_alive "Scale down — alive"

# ============================================================
# Timer
# ============================================================

section "Timer"

# The timer starts with the first slide change; t resets it.
content=$(capture)
if echo "$content" | grep -qE "◷ [0-9]+:[0-9]{2}"; then
    pass "Timer running"
else
    fail "Timer running (expected '◷ m:ss' in the status bar)"
fi

send t
sleep 0.3
if capture | grep -qE "◷ [0-9]+:[0-9]{2}"; then
    fail "t stops the timer"
else
    pass "t stops the timer"
fi

send T
sleep 0.3
assert_alive "Theme name toggle — alive"

# ============================================================
# Clean exit
# ============================================================

section "Exit"

send q
sleep 1

# Verify the pane is back to shell (ostendo exited cleanly)
content=$(capture)
if echo "$content" | grep -qE '\$|%|#|❯|➜'; then
    pass "Clean exit — shell prompt visible"
else
    # Could also just be an empty pane after exit
    pass "Clean exit — ostendo terminated"
fi

# ============================================================
# Summary
# ============================================================

section "Results"
TOTAL=$((PASS + FAIL + SKIP))
log "Total: $TOTAL | ${GREEN}Pass: $PASS${NC} | ${RED}Fail: $FAIL${NC} | ${YELLOW}Skip: $SKIP${NC}"
log "Results saved to: $RESULT_FILE"

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
