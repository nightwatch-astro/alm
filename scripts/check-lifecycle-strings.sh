#!/usr/bin/env bash
# Lifecycle string-comparison ratchet — ensures typed predicates stay in use.
#
# Invariant: ZERO field-level `.lifecycle == "..."` or `.lifecycle != "..."`
# comparisons exist in production Rust code. All lifecycle checks must go
# through typed ProjectState predicates or parse_str().
#
# Scope: *.rs files excluding tests/ segments and test_support.
#
# Usage:
#   bash scripts/check-lifecycle-strings.sh          # exits 0 on pass, 1 on fail
set -euo pipefail

# `cd "$(git rev-parse --show-toplevel)"` degrades to a no-op `cd ""` at exit 0
# when git fails, which scans the caller's cwd and reports a clean tree.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

for dir in crates apps; do
  if [ ! -d "$dir" ]; then
    echo "ERROR: scan root '$dir' does not exist under $ROOT." >&2
    exit 2
  fi
done

# grep exits 1 for "no matches" and >1 for "the scan itself failed"; collapsing
# the two reports an unreadable tree as zero violations.
set +e
raw=$(grep -rn '\.lifecycle\s*[!=]=\s*"' --include='*.rs' crates/ apps/)
rc=$?
set -e
if [ "$rc" -gt 1 ]; then
  echo "ERROR: grep exited $rc scanning crates/ apps/ — the scan failed, so this run proves nothing." >&2
  exit 2
fi

# grep -rn output: "path/file.rs:42:  content" — the comment-exclusion regex
# must match after the "path:line:" prefix, not at line start.
hits=""
if [ -n "$raw" ]; then
  hits=$(printf '%s\n' "$raw" \
    | grep -v '/tests/' \
    | grep -v 'test_support' \
    | grep -vE ':[0-9]+:\s*//' \
    || true)
fi

count=$(printf '%s' "$hits" | grep -c . || true)

if [ "$count" -gt 0 ]; then
  echo "ERROR: $count raw lifecycle string comparison(s) found."
  echo "Use ProjectState predicates (is_read_only, is_tool_locked, etc.) instead."
  echo "$hits"
  exit 1
fi

echo "OK: zero raw lifecycle string comparisons found."
