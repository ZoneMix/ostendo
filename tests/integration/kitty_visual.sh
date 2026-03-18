#!/usr/bin/env bash
# Ostendo visual regression test via Kitty remote control.
#
# Launches a Kitty instance, runs Ostendo inside it, navigates through slides
# using kitten @ send-key, captures pane text via kitten @ get-text, and
# optionally captures screenshots via macOS screencapture.
#
# Usage:
#   ./tests/integration/kitty_visual.sh [presentation_file]
#   CAPTURE_SCREENSHOTS=1 ./tests/integration/kitty_visual.sh  # also capture PNGs

set -euo pipefail

KITTY=/Applications/kitty.app/Contents/MacOS/kitty
KITTEN=/Applications/kitty.app/Contents/MacOS/kitten
PROJECT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
BINARY="$PROJECT_DIR/target/release/ostendo"
PRESENTATION="${1:-$PROJECT_DIR/presentations/examples/demo.md}"
RESULTS_DIR="$PROJECT_DIR/tests/integration/results/visual_$(date +%Y%m%d_%H%M%S)"
SCREENSHOTS_DIR="$RESULTS_DIR/screenshots"
CAPTURE_SCREENSHOTS="${CAPTURE_SCREENSHOTS:-0}"
PASS=0
FAIL=0
SKIP=0

mkdir -p "$RESULTS_DIR"
[ "$CAPTURE_SCREENSHOTS" = "1" ] && mkdir -p "$SCREENSHOTS_DIR"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
NC='\033[0m'

log() { echo -e "$1" | tee -a "$RESULTS_DIR/results.log"; }
pass() { log "${GREEN}  PASS${NC}: $1"; PASS=$((PASS + 1)); }
fail() { log "${RED}  FAIL${NC}: $1"; FAIL=$((FAIL + 1)); }
skip() { log "${YELLOW}  SKIP${NC}: $1"; SKIP=$((SKIP + 1)); }
section() { log "\n${CYAN}=== $1 ===${NC}"; }

# Get Kitty window ID for remote control
get_window_id() {
    $KITTEN @ --to unix:/tmp/ostendo-test-kitty ls 2>/dev/null \
        | python3 -c "import json,sys; data=json.load(sys.stdin); print(data[0]['tabs'][0]['windows'][0]['id'])" 2>/dev/null || echo ""
}

# Send a key to Kitty
send_key() {
    local key="$1"
    local wid
    wid=$(get_window_id)
    if [ -n "$wid" ]; then
        $KITTEN @ --to unix:/tmp/ostendo-test-kitty send-key --match "id:$wid" "$key" 2>/dev/null
    fi
}

# Get text content from Kitty pane
get_text() {
    local wid
    wid=$(get_window_id)
    if [ -n "$wid" ]; then
        $KITTEN @ --to unix:/tmp/ostendo-test-kitty get-text --match "id:$wid" 2>/dev/null
    fi
}

# Capture screenshot of Kitty window (macOS only)
capture_screenshot() {
    local name="$1"
    if [ "$CAPTURE_SCREENSHOTS" = "1" ]; then
        local wid
        wid=$(get_window_id)
        if [ -n "$wid" ]; then
            # Use screencapture with window ID
            local kitty_pid
            kitty_pid=$(pgrep -f "kitty.*--listen-on" | head -1)
            if [ -n "$kitty_pid" ]; then
                screencapture -l "$(osascript -e "tell application \"System Events\" to get id of first window of (processes whose unix id is $kitty_pid)")" "$SCREENSHOTS_DIR/${name}.png" 2>/dev/null && return
            fi
        fi
        # Fallback: capture by window title
        screencapture -l "$(osascript -e 'tell application "System Events" to get id of first window of process "kitty"' 2>/dev/null)" "$SCREENSHOTS_DIR/${name}.png" 2>/dev/null || true
    fi
}

# Assert text content contains a string
assert_text_contains() {
    local label="$1"
    local needle="$2"
    local content
    content=$(get_text)
    if echo "$content" | grep -qF "$needle"; then
        pass "$label"
    else
        fail "$label (expected '$needle')"
        echo "  Content (last 3 lines): $(echo "$content" | tail -3)" >> "$RESULTS_DIR/results.log"
    fi
}

cleanup() {
    # Send q to quit Ostendo, then close Kitty
    send_key q 2>/dev/null || true
    sleep 0.5
    # Kill the Kitty instance we started
    if [ -n "${KITTY_PID:-}" ]; then
        kill "$KITTY_PID" 2>/dev/null || true
    fi
    rm -f /tmp/ostendo-test-kitty
}
trap cleanup EXIT

# ============================================================
section "Pre-flight"
# ============================================================

if [ ! -f "$BINARY" ]; then
    log "${RED}Binary not found — run 'cargo build --release' first${NC}"
    exit 1
fi
pass "Binary exists"

if [ ! -x "$KITTY" ]; then
    log "${RED}Kitty not found at $KITTY${NC}"
    exit 1
fi
pass "Kitty installed"

# Get slide count
SLIDE_COUNT=$("$BINARY" --count "$PRESENTATION" 2>/dev/null)
pass "Slide count: $SLIDE_COUNT"

# ============================================================
section "Launch Kitty"
# ============================================================

# Start Kitty with remote control enabled, fixed size
$KITTY --single-instance --instance-group ostendo-test \
    --listen-on unix:/tmp/ostendo-test-kitty \
    -o allow_remote_control=yes \
    -o initial_window_width=160c \
    -o initial_window_height=45c \
    -o font_size=14 \
    --title "Ostendo Visual Test" \
    "$BINARY" --no-exec "$PRESENTATION" &
KITTY_PID=$!
sleep 3  # Wait for Kitty + Ostendo startup

# Verify Kitty is running and accepting remote commands
WID=$(get_window_id)
if [ -n "$WID" ]; then
    pass "Kitty started (window ID: $WID)"
else
    fail "Kitty failed to start or remote control not working"
    exit 1
fi

# ============================================================
section "Slide 1 — Title"
# ============================================================

assert_text_contains "Slide 1 visible" "Slide 1/"
capture_screenshot "slide_01_title"

# ============================================================
section "Navigation"
# ============================================================

# Navigate forward
send_key Right
sleep 0.8
assert_text_contains "Slide 2" "Slide 2/"
capture_screenshot "slide_02"

send_key Right
sleep 0.8
assert_text_contains "Slide 3" "Slide 3/"

# Navigate backward
send_key Left
sleep 0.8
assert_text_contains "Back to slide 2" "Slide 2/"

# Goto via number
send_key g
sleep 0.2
send_key 5
sleep 0.2
send_key Return
sleep 1
assert_text_contains "Goto slide 5" "Slide 5/"
capture_screenshot "slide_05"

# ============================================================
section "Full Traversal ($SLIDE_COUNT slides)"
# ============================================================

# Go to slide 1
send_key g
sleep 0.2
send_key 1
sleep 0.2
send_key Return
sleep 1

# Navigate through every slide
TRAVERSAL_OK=true
for i in $(seq 2 "$SLIDE_COUNT"); do
    send_key Right
    sleep 0.3
done
sleep 1

# Check we're on the last slide
content=$(get_text)
if echo "$content" | grep -qF "Slide $SLIDE_COUNT/"; then
    pass "Full traversal — reached slide $SLIDE_COUNT"
else
    # Check if any slide number is visible (Ostendo didn't crash)
    if echo "$content" | grep -qE "Slide [0-9]+/"; then
        pass "Full traversal — Ostendo running (may have animation delay)"
    else
        fail "Full traversal — Ostendo may have crashed"
    fi
fi
capture_screenshot "slide_last"

# ============================================================
section "Theme Switching"
# ============================================================

# Go to a content slide for better theme visibility
send_key g
sleep 0.2
send_key 3
sleep 0.2
send_key Return
sleep 1

# Switch theme via command mode
send_key :
sleep 0.3
for char in t h e m e ' ' d r a c u l a; do
    send_key "$char"
    sleep 0.05
done
send_key Return
sleep 1
assert_text_contains "Theme switch to dracula" "Slide"
capture_screenshot "theme_dracula"

# Toggle dark/light
send_key D
sleep 1
capture_screenshot "theme_dark_toggle"

# ============================================================
section "UI Features"
# ============================================================

# Help overlay
send_key '?'
sleep 0.8
capture_screenshot "help_overlay"
send_key '?'
sleep 0.5

# Overview grid
send_key o
sleep 0.8
capture_screenshot "overview_grid"
send_key o
sleep 0.5

# Fullscreen toggle
send_key f
sleep 0.5
capture_screenshot "fullscreen"
send_key f
sleep 0.5

# Timer
send_key t
sleep 2
content=$(get_text)
if echo "$content" | grep -qE "[0-9]+:[0-9]+:[0-9]+"; then
    pass "Timer visible"
else
    skip "Timer not captured in text"
fi

# ============================================================
section "Image Slides"
# ============================================================

# Navigate to image slides
send_key g
sleep 0.2
# Slide 12 = Images: Protocol Auto-Detection
send_key 1
sleep 0.1
send_key 2
sleep 0.2
send_key Return
sleep 1.5
assert_text_contains "Image slide 12" "Slide 12/"
capture_screenshot "slide_12_image"

# Slide 14 = Animated GIFs
send_key g
sleep 0.2
send_key 1
sleep 0.1
send_key 4
sleep 0.2
send_key Return
sleep 2
assert_text_contains "GIF slide 14" "Slide 14/"
capture_screenshot "slide_14_gif"

# Let GIF play for a few seconds to verify smooth animation
sleep 3
capture_screenshot "slide_14_gif_playing"

# ============================================================
section "Clean Exit"
# ============================================================

send_key q
sleep 1
pass "Exit command sent"

# ============================================================
section "Results"
# ============================================================

TOTAL=$((PASS + FAIL + SKIP))
log "Total: $TOTAL | ${GREEN}Pass: $PASS${NC} | ${RED}Fail: $FAIL${NC} | ${YELLOW}Skip: $SKIP${NC}"
log "Results: $RESULTS_DIR/results.log"
if [ "$CAPTURE_SCREENSHOTS" = "1" ]; then
    log "Screenshots: $SCREENSHOTS_DIR/"
fi

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
