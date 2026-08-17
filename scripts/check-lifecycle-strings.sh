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

cd "$(git rev-parse --show-toplevel)"

# grep -rn output: "path/file.rs:42:  content" — the comment-exclusion regex
# must match after the "path:line:" prefix, not at line start.
hits=$(grep -rn '\.lifecycle\s*[!=]=\s*"' --include='*.rs' crates/ apps/ \
  | grep -v '/tests/' \
  | grep -v 'test_support' \
  | grep -vE ':[0-9]+:\s*//' \
  || true)

count=$(echo "$hits" | grep -c . || true)

if [ "$count" -gt 0 ]; then
  echo "ERROR: $count raw lifecycle string comparison(s) found."
  echo "Use ProjectState predicates (is_read_only, is_tool_locked, etc.) instead."
  echo "$hits"
  exit 1
fi

echo "OK: zero raw lifecycle string comparisons found."
