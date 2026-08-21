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
# Comparison (1) gates: it is the only one that exits non-zero. Comparison (2) is
# advisory, because a PR that updates the policy and the watcher together is
# correct while live protection still names the old contexts -- protection is
# applied at merge, not at lint. It also needs network and a token with repo scope,
# so it prints a NOTE and exits 0 when it cannot reach the API.

set -euo pipefail

REPO="${PV_REPO:-platevault/platevault}"
BRANCH="${PV_BRANCH:-main}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WATCHER="$SCRIPT_DIR/pv-watch-queue.sh"
FIXTURE="$SCRIPT_DIR/tests/pv-watch-queue.bats"
POLICY="$SCRIPT_DIR/branch-protection-main.json"

# A missing jq is a hard failure, not a skip. The gating comparison is offline and
# claims to always run, so skipping it would report a pass that measured nothing --
# the same fail-open this guard exists to prevent. pv-watch-queue.sh needs jq to
# run at all, so any environment that can use the watcher can check it.
command -v jq >/dev/null 2>&1 || {
  echo "FAIL: jq is required to compare the watcher against $POLICY." >&2
  exit 1
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

# Advisory, and it has to be. A PR that updates the policy and the watcher
# together is correct while live protection still names the old contexts, because
# protection is applied at merge, not at lint. Failing here would make `just lint`
# red for exactly that PR -- the case the gating comparison above was rewritten to
# admit.
echo "WARN: live branch protection on $REPO@$BRANCH differs from $POLICY."
[ -n "$live_missing" ] && {
  echo "  Required live but absent from the checked-in policy:"
  printf '    %s\n' "$live_missing"
}
[ -n "$live_extra" ] && {
  echo "  In the checked-in policy but not required live:"
  printf '    %s\n' "$live_extra"
}
cat <<EOF

This is expected on a PR that changes the required contexts, since protection is
applied at merge. Otherwise protection was changed out of band, and the checked-in
policy the watcher is validated against no longer describes what gates a merge.
Reapply the declared policy:
  bash $SCRIPT_DIR/branch-protection-main.sh apply
or record the intended change in $POLICY and resync the watcher.
EOF
exit 0
