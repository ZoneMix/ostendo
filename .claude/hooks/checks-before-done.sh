#!/usr/bin/env bash
# Before Claude reports a turn as done, the same checks CI runs must pass on
# whatever it changed. Skips when nothing build-relevant changed or the tree
# already passed.
set -uo pipefail
input=$(cat)
[[ $(jq -r '.stop_hook_active' <<<"$input") == true ]] && exit 0
cd "${CLAUDE_PROJECT_DIR:-.}"

relevant='^(src/|tests/|themes/|presentations/examples/|build\.rs|Cargo\.(toml|lock)|\.github/workflows/)'
changes=$( { git diff --name-only HEAD; git ls-files --others --exclude-standard; \
  git diff --name-only '@{upstream}'...HEAD 2>/dev/null; } | grep -E "$relevant" | sort -u)
[[ -z "$changes" ]] && exit 0

stamp=".git/claude-checks-passed"
tree=$( { git rev-parse HEAD; git diff HEAD; git ls-files --others --exclude-standard | xargs -r cat; } | shasum | cut -c1-40)
[[ -f "$stamp" && $(cat "$stamp") == "$tree" ]] && exit 0

run() { out=$("$@" 2>&1) || { echo "Not done: \`$*\` failed. Fix it before reporting the work as done." >&2; tail -n 40 <<<"$out" >&2; exit 2; }; }
run cargo fmt --check
run cargo clippy --all-targets -q -- -D warnings
run cargo test -q
for deck in presentations/examples/*.md; do run cargo run -q -- --validate "$deck"; done
echo "$tree" > "$stamp"
exit 0
