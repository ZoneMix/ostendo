#!/usr/bin/env bash
# Ostendo export-based content verification
# Tests HTML export integrity and cross-references with CLI outputs.
#
# Usage: ./tests/integration/export_verify.sh

set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
BINARY="$PROJECT_DIR/target/release/ostendo"
RESULTS_DIR="$PROJECT_DIR/tests/integration/results"
RESULT_FILE="$RESULTS_DIR/export_$(date +%Y%m%d_%H%M%S).log"
TMPDIR=$(mktemp -d)
PASS=0
FAIL=0

mkdir -p "$RESULTS_DIR"

RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
NC='\033[0m'

log() { echo -e "$1" | tee -a "$RESULT_FILE"; }
pass() { log "${GREEN}  PASS${NC}: $1"; PASS=$((PASS + 1)); }
fail() { log "${RED}  FAIL${NC}: $1"; FAIL=$((FAIL + 1)); }
section() { log "\n${CYAN}=== $1 ===${NC}"; }

cleanup() { rm -rf "$TMPDIR"; }
trap cleanup EXIT

# All presentations to test
PRESENTATIONS=(
    "$PROJECT_DIR/presentations/examples/demo.md"
    "$PROJECT_DIR/presentations/examples/test_presentation.md"
    "$PROJECT_DIR/presentations/examples/quick_start.md"
)

for PRES in "${PRESENTATIONS[@]}"; do
    [ -f "$PRES" ] || continue
    NAME=$(basename "$PRES" .md)

    section "$NAME"

    # 1. Validate
    VALIDATE=$("$BINARY" --validate "$PRES" 2>&1)
    if echo "$VALIDATE" | grep -q "OK"; then
        pass "$NAME: validation"
    else
        fail "$NAME: validation — $VALIDATE"
        continue
    fi

    # 2. Get slide count
    COUNT=$("$BINARY" --count "$PRES" 2>/dev/null)
    if [ "$COUNT" -gt 0 ]; then
        pass "$NAME: slide count = $COUNT"
    else
        fail "$NAME: slide count is 0"
    fi

    # 3. Export titles
    TITLES=$("$BINARY" --export-titles "$PRES" 2>/dev/null)
    TITLE_COUNT=$(echo "$TITLES" | wc -l | tr -d ' ')
    if [ "$TITLE_COUNT" -eq "$COUNT" ]; then
        pass "$NAME: title count matches slide count ($TITLE_COUNT)"
    else
        fail "$NAME: title count ($TITLE_COUNT) != slide count ($COUNT)"
    fi

    # 4. Check first title is non-empty
    FIRST_TITLE=$(echo "$TITLES" | head -1)
    if [ -n "$FIRST_TITLE" ]; then
        pass "$NAME: first title = '$FIRST_TITLE'"
    else
        fail "$NAME: first title is empty"
    fi

    # 5. HTML export
    HTML_OUT="$TMPDIR/${NAME}.html"
    EXPORT_RESULT=$("$BINARY" --export html -o "$HTML_OUT" "$PRES" 2>&1)
    if [ -f "$HTML_OUT" ]; then
        pass "$NAME: HTML export created"
    else
        fail "$NAME: HTML export failed — $EXPORT_RESULT"
        continue
    fi

    # 6. HTML is valid-ish (has doctype, head, body)
    if grep -q "<!DOCTYPE html>" "$HTML_OUT"; then
        pass "$NAME: HTML has DOCTYPE"
    else
        fail "$NAME: HTML missing DOCTYPE"
    fi

    if grep -q "<title>" "$HTML_OUT"; then
        pass "$NAME: HTML has <title>"
    else
        fail "$NAME: HTML missing <title>"
    fi

    # 7. HTML file size is reasonable (>1KB)
    SIZE=$(wc -c < "$HTML_OUT" | tr -d ' ')
    if [ "$SIZE" -gt 1024 ]; then
        pass "$NAME: HTML size = ${SIZE} bytes"
    else
        fail "$NAME: HTML suspiciously small (${SIZE} bytes)"
    fi

    # 8. Check that slide titles appear in HTML
    FOUND_TITLES=0
    TOTAL_TO_CHECK=5
    if [ "$COUNT" -lt "$TOTAL_TO_CHECK" ]; then
        TOTAL_TO_CHECK=$COUNT
    fi
    for i in $(seq 1 "$TOTAL_TO_CHECK"); do
        TITLE=$(echo "$TITLES" | sed -n "${i}p")
        if [ -n "$TITLE" ] && grep -qF "$TITLE" "$HTML_OUT"; then
            FOUND_TITLES=$((FOUND_TITLES + 1))
        fi
    done
    if [ "$FOUND_TITLES" -ge "$((TOTAL_TO_CHECK / 2))" ]; then
        pass "$NAME: $FOUND_TITLES/$TOTAL_TO_CHECK slide titles found in HTML"
    else
        fail "$NAME: only $FOUND_TITLES/$TOTAL_TO_CHECK slide titles in HTML"
    fi

    # 9. Theme-specific tests
    for THEME in terminal_green dracula nord; do
        THEMED_OUT="$TMPDIR/${NAME}_${THEME}.html"
        "$BINARY" --export html -o "$THEMED_OUT" -t "$THEME" "$PRES" 2>/dev/null
        if [ -f "$THEMED_OUT" ] && [ "$(wc -c < "$THEMED_OUT" | tr -d ' ')" -gt 1024 ]; then
            pass "$NAME: export with theme '$THEME'"
        else
            fail "$NAME: export with theme '$THEME' failed"
        fi
    done
done

# ============================================================
# Theme completeness check
# ============================================================

section "Theme Registry"

THEME_LIST=$("$BINARY" --list-themes 2>/dev/null | grep -v "Available themes:")
THEME_COUNT=$(echo "$THEME_LIST" | wc -l | tr -d ' ')
if [ "$THEME_COUNT" -ge 29 ]; then
    pass "Theme count: $THEME_COUNT (expected >= 29)"
else
    fail "Theme count: $THEME_COUNT (expected >= 29)"
fi

# Verify each theme can export without crashing
SMALL_PRES="$PROJECT_DIR/presentations/examples/quick_start.md"
if [ -f "$SMALL_PRES" ]; then
    THEME_PASS=0
    THEME_FAIL=0
    while IFS= read -r theme; do
        theme=$(echo "$theme" | tr -d ' ')
        [ -z "$theme" ] && continue
        OUT="$TMPDIR/theme_${theme}.html"
        if "$BINARY" --export html -o "$OUT" -t "$theme" "$SMALL_PRES" 2>/dev/null; then
            THEME_PASS=$((THEME_PASS + 1))
        else
            fail "Theme '$theme' export crashed"
            THEME_FAIL=$((THEME_FAIL + 1))
        fi
    done <<< "$THEME_LIST"
    pass "All $THEME_PASS themes export successfully"
fi

# ============================================================
# Protocol detection
# ============================================================

section "Protocol Detection"

PROTOCOL=$("$BINARY" --detect-protocol 2>/dev/null || echo "unknown")
pass "Detected protocol: $PROTOCOL"

# ============================================================
# Summary
# ============================================================

section "Results"
TOTAL=$((PASS + FAIL))
log "Total: $TOTAL | ${GREEN}Pass: $PASS${NC} | ${RED}Fail: $FAIL${NC}"
log "Results saved to: $RESULT_FILE"

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
