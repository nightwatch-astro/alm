#!/usr/bin/env bash
# Copyright (C) 2024-2026 Sjors Robroek
# SPDX-License-Identifier: AGPL-3.0-only
#
# Compare the required-status-check list hardcoded in pv-watch-queue.sh against
# what branch protection on `main` actually requires (astro-plan-lgyr).
#
# Why this exists: the queue scanner decides whether a PR is mergeable by
# matching check names against an embedded array. When branch protection changes
# and that array does not, the scanner does not error -- it silently
# misclassifies. Before this check, it named two contexts that never run on
# `pull_request` (so every PR read ABSENT on them) and omitted two that are
# genuinely required. Its bats fixture embedded the SAME wrong list, so the
# suite passed while measuring nothing. That is the failure this guards.
#
# Advisory by design, and deliberately so: it needs network and a token with
# repo scope, so it must not turn an offline `pnpm lint` red. Exit 0 with a
# NOTE when it cannot reach the API, non-zero only when it has a real answer
# and the lists disagree.

set -euo pipefail

REPO="${PV_REPO:-platevault/platevault}"
BRANCH="${PV_BRANCH:-main}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WATCHER="$SCRIPT_DIR/pv-watch-queue.sh"
FIXTURE="$SCRIPT_DIR/tests/pv-watch-queue.bats"

command -v gh >/dev/null 2>&1 || {
  echo "NOTE: gh not available; skipping required-context drift check."
  exit 0
}
command -v jq >/dev/null 2>&1 || {
  echo "NOTE: jq not available; skipping required-context drift check."
  exit 0
}

# Extract the embedded array: everything between `[` and `] as $required`.
embedded="$(
  sed -n '/as \$required/q;p' "$WATCHER" |
    sed -n '/^  \["/,$p' |
    grep -oE '"[^"]+"' |
    tr -d '"' |
    sort
)"
# Include the terminating line, which holds the last element.
embedded="$(
  {
    printf '%s\n' "$embedded"
    grep -oE '"[^"]+"' <(grep -m1 'as \$required' "$WATCHER") | tr -d '"'
  } | grep -v '^$' | sort -u
)"

if [ -z "$embedded" ]; then
  echo "FAIL: could not parse the \$required array out of $WATCHER." >&2
  echo "This check is not measuring anything until that parse is fixed." >&2
  exit 1
fi

actual="$(
  gh api "repos/$REPO/branches/$BRANCH/protection" \
    --jq '.required_status_checks.contexts[]' 2>/dev/null | sort -u
)" || true

if [ -z "$actual" ]; then
  echo "NOTE: could not read branch protection for $REPO@$BRANCH (offline, or"
  echo "      the token lacks repo scope); skipping required-context drift check."
  exit 0
fi

missing="$(comm -13 <(printf '%s\n' "$embedded") <(printf '%s\n' "$actual"))"
extra="$(comm -23 <(printf '%s\n' "$embedded") <(printf '%s\n' "$actual"))"

if [ -z "$missing" ] && [ -z "$extra" ]; then
  echo "OK: pv-watch-queue.sh matches branch protection ($(printf '%s\n' "$actual" | grep -c .) contexts)."
  exit 0
fi

echo "FAIL: pv-watch-queue.sh's required-context list has drifted from branch protection." >&2
[ -n "$missing" ] && {
  echo "  Required by protection but MISSING from the script:" >&2
  printf '    %s\n' "$missing" >&2
}
[ -n "$extra" ] && {
  echo "  Named by the script but NOT required (these read as absent on every PR):" >&2
  printf '    %s\n' "$extra" >&2
}
cat >&2 <<EOF

The scanner matches check names against that array to decide whether a PR is
mergeable. A stale entry does not error -- it misclassifies silently.

Update the \$required array in:
  $WATCHER
and the identical list in its fixture, or the bats suite will keep passing
while measuring the wrong thing:
  $FIXTURE
EOF
exit 1
