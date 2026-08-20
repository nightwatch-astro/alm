#!/usr/bin/env bash
# Copyright (C) 2024-2026 Sjors Robroek
# SPDX-License-Identifier: AGPL-3.0-only
#
# Compare the required-status-check list hardcoded in pv-watch-queue.sh against
# the repo's declared branch protection for `main` (astro-plan-lgyr).
#
# Two comparisons, because they answer different questions:
#
#   1. embedded array vs `branch-protection-main.json`. This is the gating one.
#      The JSON is the checked-in policy, so a PR that intentionally changes the
#      required contexts updates both files and passes. Comparing the proposed
#      watcher against still-live protection instead would fail every correct
#      synchronised PR, and applying protection first would only invert the
#      mismatch. Offline and deterministic.
#   2. `branch-protection-main.json` vs what the API reports live. This catches
#      protection changed out of band, which leaves the declared policy stale and
#      the comparison in (1) measuring the wrong target.
#
# Why this exists: the queue scanner decides whether a PR is mergeable by
# matching check names against an embedded array. When branch protection changes
# and that array does not, the scanner does not error -- it silently
# misclassifies. Before this check, it named two contexts that never run on
# `pull_request` (so every PR read ABSENT on them) and omitted two that are
# genuinely required. Its bats fixture embedded the SAME wrong list, so the
# suite passed while measuring nothing. That is the failure this guards.
#
# Comparison (1) always runs. Comparison (2) needs network and a token with repo
# scope, so it exits 0 with a NOTE when it cannot reach the API and non-zero only
# when it has a real answer and the lists disagree.

set -euo pipefail

REPO="${PV_REPO:-platevault/platevault}"
BRANCH="${PV_BRANCH:-main}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WATCHER="$SCRIPT_DIR/pv-watch-queue.sh"
FIXTURE="$SCRIPT_DIR/tests/pv-watch-queue.bats"
POLICY="$SCRIPT_DIR/branch-protection-main.json"

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

if [ ! -f "$POLICY" ]; then
  echo "FAIL: $POLICY is missing; there is no declared policy to compare against." >&2
  exit 1
fi

declared="$(jq -r '.required_status_checks.contexts[]' "$POLICY" | sort -u)"

if [ -z "$declared" ]; then
  echo "FAIL: $POLICY declares no required contexts." >&2
  echo "A policy requiring nothing would make every PR READY on an empty check set." >&2
  exit 1
fi

# --- (1) gating: embedded array vs the checked-in policy ---------------------

missing="$(comm -13 <(printf '%s\n' "$embedded") <(printf '%s\n' "$declared"))"
extra="$(comm -23 <(printf '%s\n' "$embedded") <(printf '%s\n' "$declared"))"

if [ -n "$missing" ] || [ -n "$extra" ]; then
  echo "FAIL: pv-watch-queue.sh's required-context list has drifted from $POLICY." >&2
  [ -n "$missing" ] && {
    echo "  Required by policy but MISSING from the script:" >&2
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
fi

echo "OK: pv-watch-queue.sh matches $(basename "$POLICY") ($(printf '%s\n' "$declared" | grep -c .) contexts)."

# --- (2) advisory: the checked-in policy vs live protection ------------------

command -v gh >/dev/null 2>&1 || {
  echo "NOTE: gh not available; skipping the live branch-protection comparison."
  exit 0
}

# Keep the API's exit status. An empty body from a successful call means
# protection requires zero contexts, which is real drift; conflating it with an
# unreachable API would hide the removal of the last required check.
if ! live="$(
  gh api "repos/$REPO/branches/$BRANCH/protection" \
    --jq '.required_status_checks.contexts // [] | .[]' 2>/dev/null
)"; then
  echo "NOTE: could not read branch protection for $REPO@$BRANCH (offline, or"
  echo "      the token lacks repo scope); skipping the live comparison."
  exit 0
fi
live="$(printf '%s\n' "$live" | grep -v '^$' | sort -u || true)"

live_missing="$(comm -13 <(printf '%s\n' "$declared") <(printf '%s\n' "$live"))"
live_extra="$(comm -23 <(printf '%s\n' "$declared") <(printf '%s\n' "$live"))"

if [ -z "$live_missing" ] && [ -z "$live_extra" ]; then
  echo "OK: $(basename "$POLICY") matches live protection on $REPO@$BRANCH."
  exit 0
fi

echo "FAIL: live branch protection on $REPO@$BRANCH differs from $POLICY." >&2
[ -n "$live_missing" ] && {
  echo "  Required live but absent from the checked-in policy:" >&2
  printf '    %s\n' "$live_missing" >&2
}
[ -n "$live_extra" ] && {
  echo "  In the checked-in policy but not required live:" >&2
  printf '    %s\n' "$live_extra" >&2
}
cat >&2 <<EOF

Protection was changed out of band, so the checked-in policy the watcher is
validated against no longer describes what gates a merge. Either reapply the
declared policy:
  bash $SCRIPT_DIR/branch-protection-main.sh
or record the intended change in $POLICY and resync the watcher.
EOF
exit 1
