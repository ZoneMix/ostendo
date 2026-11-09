#!/usr/bin/env bash
# Runs on every Bash call. Blocks pushes that skip the claude/ branch rule or the
# checks, and commits that stage a credential. Deny rules catch the plain forms;
# this catches the spellings they miss (git -C . push, push --force-with-lease to main).
set -euo pipefail
input=$(cat)
cmd=$(jq -r '.tool_input.command // empty' <<<"$input")
dir=$(dirname "$0")
cd "${CLAUDE_PROJECT_DIR:-.}"

block() { echo "Blocked: $1" >&2; exit 2; }

if grep -Eq '(^|[;&|[:space:]])rm[[:space:]]+-[a-zA-Z]*r[a-zA-Z]*f?[[:space:]]+(/|~|\$HOME|\.|\*|src|\.git)([[:space:]/]|$)' <<<"$cmd"; then
  block "recursive delete of a top-level or source path. Delete specific files, or ask Bailey."
fi

if grep -Eq '(^|[;&|[:space:]])git([[:space:]]+-C[[:space:]]+[^[:space:]]+)?[[:space:]]+commit' <<<"$cmd"; then
  if git diff --cached -U0 | grep '^+' | grep -Eqf "$dir/secret-patterns.txt"; then
    block "the staged diff contains what looks like a credential."
  fi
fi

if grep -Eq '(^|[;&|[:space:]])git([[:space:]]+-C[[:space:]]+[^[:space:]]+)?[[:space:]]+push' <<<"$cmd"; then
  grep -Eq '[[:space:]](-f|--force)([[:space:]]|$)' <<<"$cmd" && block "force push."
  grep -Eq '[[:space:]:+](main|master)([[:space:]]|$)' <<<"$cmd" && block "push to main. Push a claude/ branch and open a PR."
  branch=$(git rev-parse --abbrev-ref HEAD)
  [[ "$branch" == claude/* ]] || block "current branch '$branch' is not a claude/ branch."
  checks() {
    cargo fmt --check && cargo clippy --all-targets -q -- -D warnings && cargo test -q || return
    for deck in presentations/examples/*.md; do cargo run -q -- --validate "$deck" || return; done
  }
  out=$(checks 2>&1) || block "checks failed before push:
$(tail -n 40 <<<"$out")"
fi
exit 0
