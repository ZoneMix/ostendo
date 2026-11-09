#!/usr/bin/env bash
# Blocks secrets in file edits and scratch output (screenshots, recordings, logs)
# landing in the repo instead of the scratchpad.
set -euo pipefail
input=$(cat)
path=$(jq -r '.tool_input.file_path // empty' <<<"$input")
text=$(jq -r '[.tool_input.content, .tool_input.new_string, (.tool_input.edits // [] | .[].new_string)] | map(select(. != null)) | join("\n")' <<<"$input")
dir=$(dirname "$0")

if grep -Eqf "$dir/secret-patterns.txt" <<<"$text"; then
  echo "Blocked: the edit to $path contains what looks like a credential. Use an environment variable or a placeholder." >&2
  exit 2
fi

root="${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel)}"
case "$path" in
  "$root"/*) rel="${path#"$root"/}" ;;
  *) exit 0 ;;
esac
case "$rel" in
  src/*|images/*|docs/*|presentations/*|themes/*|fonts/*) exit 0 ;;
esac
case "$rel" in
  *.png|*.jpg|*.jpeg|*.gif|*.cast|*.log|*.pptx|*.pdf|*.html)
    echo "Blocked: $rel looks like scratch output. Write it to the scratchpad, not the repo (project rule)." >&2
    exit 2 ;;
esac
exit 0
