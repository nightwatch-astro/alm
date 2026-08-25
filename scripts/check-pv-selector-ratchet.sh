#!/usr/bin/env bash
# .pv-* CSS selector ratchet for e2e test files.
#
# Invariant: e2e test files (Playwright TS and Rust journeys) must NOT select
# DOM elements by .pv-* CSS class names. After the data-testid migration these
# selectors must be [data-testid="..."] or [data-kind="..."] instead.
#
# Catches .pv-* preceded by a dot in any quote context — single-quoted, double-
# quoted, or bare (e.g. By::Css(".pv-foo"), locator('.pv-foo'), querySelector).
#
# Intentional exceptions excluded by the grep -v filters:
#   - toHaveClass / toHaveAttribute: class existence checks, not selectors
#   - /pv-/: regex patterns in test assertions
#   - .pv-mono: decorative typography class — no structural testid equivalent;
#     typography tests legitimately query it for font-stack verification
#   - comment lines (// ...)
#
# Sealed at zero: any new .pv-* selector (other than the above) fails the build.
# Covers the Rust e2e files that the eslint alm/require-root-testid rule cannot.
#
# Usage:
#   bash scripts/check-pv-selector-ratchet.sh          # exits 0 on pass, 1 on fail

set -euo pipefail

# `cd "$(git rev-parse --show-toplevel)"` degrades to a no-op `cd ""` at exit 0
# when git fails, which scans the caller's cwd and reports a clean tree.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# grep exits 1 for "no matches" and >1 for "the scan itself failed"; collapsing
# the two reports a renamed or unreadable scan root as zero violations.
scan() {
  local root="$1" raw rc
  if [ ! -d "$root" ]; then
    echo "ERROR: scan root '$root' does not exist under $ROOT." >&2
    exit 2
  fi
  set +e
  raw=$(grep -rn '\.pv-[a-z]' "$root")
  rc=$?
  set -e
  if [ "$rc" -gt 1 ]; then
    echo "ERROR: grep exited $rc scanning '$root' — the scan failed, so this run proves nothing." >&2
    exit 2
  fi
  printf '%s' "$raw"
}

# Playwright TS: locator('.pv-*'), locator(".pv-*"), const FOO = '.pv-*', etc.
# Exclude toHaveClass/Attribute, /pv-/ regex, pv-mono (typography-only class),
# and comment lines.
# `scan` runs in a subshell, so its `exit 2` must be re-raised here explicitly.
ts_raw=$(scan tests/e2e/) || exit $?
ts_hits=""
if [ -n "$ts_raw" ]; then
  ts_hits=$(printf '%s\n' "$ts_raw" \
    | grep -v 'toHaveClass\|toHaveAttribute\|/pv-' \
    | grep -v '\.pv-mono' \
    | grep -v ':[[:space:]]*//' \
    | grep -v ':[[:space:]]*\*' \
    || true)
fi

# Rust: querySelector(".pv-*"), By::Css(".pv-*"), etc. — both quote styles.
# Exclude Rust comment lines (//!, ///, //).
rs_raw=$(scan crates/e2e-tests/tests/) || exit $?
rs_hits=""
if [ -n "$rs_raw" ]; then
  rs_hits=$(printf '%s\n' "$rs_raw" \
    | grep -v ':[[:space:]]*//[/!]\?' \
    || true)
fi

all_hits="${ts_hits}
${rs_hits}"
count=$(printf '%s\n' "$all_hits" | grep -c '[^[:space:]]' || true)

if [ "$count" -gt 0 ]; then
  echo "ERROR: $count .pv-* class selector(s) found in e2e test files."
  echo "Replace with [data-testid=\"...\"] or [data-kind=\"...\"] selectors."
  echo "Exception: .pv-mono is allowed (typography-only class, no testid equivalent)."
  echo ""
  printf '%s\n' "$all_hits" | grep '[^[:space:]]'
  exit 1
fi

echo "OK: zero .pv-* class selectors in e2e test files."
