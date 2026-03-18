#!/usr/bin/env bash
# Run all Ostendo integration tests.
#
# Usage:
#   ./tests/integration/run_all.sh              # export + tmux tests
#   ./tests/integration/run_all.sh --visual     # also run Kitty visual tests
#   ./tests/integration/run_all.sh --screenshots # visual + capture screenshots

set -euo pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"
VISUAL=0
SCREENSHOTS=0

for arg in "$@"; do
    case "$arg" in
        --visual) VISUAL=1 ;;
        --screenshots) VISUAL=1; SCREENSHOTS=1 ;;
    esac
done

echo "=== Export Verification ==="
bash "$DIR/export_verify.sh"
echo ""

echo "=== tmux Live Test (demo.md) ==="
bash "$DIR/tmux_harness.sh"
echo ""

echo "=== tmux Live Test (test_presentation.md) ==="
bash "$DIR/tmux_harness.sh" "$(cd "$DIR/../.." && pwd)/presentations/examples/test_presentation.md"
echo ""

if [ "$VISUAL" = "1" ]; then
    echo "=== Kitty Visual Test ==="
    CAPTURE_SCREENSHOTS=$SCREENSHOTS bash "$DIR/kitty_visual.sh"
    echo ""
fi

echo "All tests complete."
