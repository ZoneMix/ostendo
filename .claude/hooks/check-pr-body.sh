#!/usr/bin/env bash
# Project rules for PR descriptions, checked against what the branch changes.
set -euo pipefail
input=$(cat)
tool=$(jq -r '.tool_name' <<<"$input")
body=$(jq -r '.tool_input.body // empty' <<<"$input")
[[ "$tool" == *update_pull_request && -z "$body" ]] && exit 0
cd "${CLAUDE_PROJECT_DIR:-.}"
git fetch -q origin main 2>/dev/null || true
changed=$(git diff --name-only origin/main...HEAD)
missing=()

grep -Eqi '^#+ *testing' <<<"$body" || missing+=("a Testing section")
if grep -Eq '^src/export/(pptx|zip)\.rs$' <<<"$changed"; then
  grep -qi 'python-pptx' <<<"$body" || missing+=("python-pptx verification")
  grep -qi 'libreoffice' <<<"$body" || missing+=("LibreOffice Impress verification")
  grep -Eqi 'not (been )?opened in powerpoint' <<<"$body" || missing+=("the 'not opened in PowerPoint, Keynote or Google Slides' statement")
fi
if grep -Eq '^src/remote/' <<<"$changed"; then
  grep -Eqi '^#+ *security' <<<"$body" || missing+=("a Security section with the network reasoning (src/remote/ changed)")
fi

if ((${#missing[@]})); then
  printf 'Blocked: the PR description is missing:\n' >&2
  printf -- '- %s\n' "${missing[@]}" >&2
  exit 2
fi
exit 0
