#!/usr/bin/env bash
# Apply / inspect / remove branch protection on `main`.
#
# NOT APPLIED AUTOMATICALLY. The config lives in
# `scripts/branch-protection-main.json` so the required contexts are reviewable
# in a PR rather than living only in GitHub's UI, and so re-applying after a
# job rename is a one-liner instead of a click-path.
#
# Why each setting:
#
#   strict: true          Branches must be up to date with main before merging.
#                         Costly while main churns, correct once it settles.
#   enforce_admins: false Sole maintainer needs an override path when a runner
#                         or an upstream dependency breaks.
#   reviews: null         Solo repo; a required-review rule would just block.
#
# Required contexts are the GATE jobs, never the workers they depend on: a gate
# already depends on its shards, so requiring the shards as well would add
# nothing and would break whenever the shard count changes. A required name must
# also be published by exactly ONE job — branch protection matches contexts by
# exact name, and an evaluator that settles on the wrong same-named check run
# (a merge queue entry that reads the worker's `skipped`) deadlocks. Worker names
# therefore differ from their gate's: "Real-UI smoke (L3) subset — ubuntu-latest"
# vs the required "Real-UI smoke (L3) — ubuntu-latest".
#
# Excluded on purpose:
#   Real-UI journeys (L3) — <os>           the full matrix runs on main pushes
#                                          and dispatch, never on pull_request,
#                                          so it publishes nothing to require on
#                                          a PR. PRs are gated by the smoke
#                                          context instead (two-tier strategy in
#                                          e2e.yml's header).
#   Real-UI smoke (L3) — windows-latest    no windows smoke job exists; windows
#                                          L3 is main-push only.
#   Real-UI journeys (L3) — macos-latest   blocked upstream, see issue #489
#                                          (tauri-plugin-webdriver)
#   Unit + integration (L1+L2) — macos-latest
#                                          Held out only while the frontend
#                                          suite still runs on Rust-only PRs.
#                                          NOTE: issue #489 is about the E2E
#                                          macOS leg, NOT this one — the two
#                                          were previously conflated. Recent
#                                          history is 3 success / 3 cancelled /
#                                          2 failure, and both failures were
#                                          attributable (a frontend flake
#                                          dragged in by force_full, and a
#                                          paraglide bug). Reconsider including
#                                          it once those have landed.
#
# A context whose job is SKIPPED counts as satisfied by branch protection, so
# docs-only PRs do not hang. That is why `e2e.yml` gates whole JOBS with a
# job-level `if:` rather than step-level conditions — see the comment near the
# top of that workflow. Do not convert those to step-level.
#
# Usage:
#   scripts/branch-protection-main.sh show     # current protection (or 404)
#   scripts/branch-protection-main.sh apply    # PUT the config
#   scripts/branch-protection-main.sh remove   # DELETE protection
#   scripts/branch-protection-main.sh verify   # apply-time sanity checks only
set -euo pipefail

REPO="${REPO:-platevault/platevault}"
BRANCH="${BRANCH:-main}"
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
CONFIG="$here/branch-protection-main.json"

# Read the required contexts once, and refuse to report on an empty list: a
# `while read` loop over an empty producer leaves every accumulator at 0, so
# both verify_* functions returned PASS having inspected nothing whenever
# python3 was missing, the config was unreadable, or the config carried no
# required_status_checks.contexts.
read_contexts() {
  if ! command -v python3 >/dev/null; then
    echo "ERROR: python3 is required to read $CONFIG." >&2
    return 2
  fi
  local out
  out=$(python3 -c '
import json,sys
print("\n".join(json.load(open(sys.argv[1]))["required_status_checks"]["contexts"]))' "$CONFIG") || {
    echo "ERROR: could not read required_status_checks.contexts from $CONFIG." >&2
    return 2
  }
  if [ -z "${out//[[:space:]]/}" ]; then
    echo "ERROR: $CONFIG lists zero required_status_checks.contexts; there is nothing to verify." >&2
    return 2
  fi
  printf '%s\n' "$out"
}

# The context strings contain U+2014 EM DASH, not a hyphen. A hyphen produces a
# required context that never reports, which leaves every PR pending forever.
verify_contexts() {
  # Only the SEPARATOR matters. Hyphens inside a word are fine
  # ("UI mock-mode (Playwright)", "ubuntu-latest"); the failure mode is a
  # spaced hyphen " - " where the job name uses " — ".
  local bad=0 contexts
  contexts=$(read_contexts) || return $?
  while IFS= read -r ctx; do
    [ -n "$ctx" ] || continue
    case "$ctx" in
      *" - "*) echo "  WARN: '$ctx' uses ' - ' as a separator; job names use ' — ' (U+2014)" >&2; bad=1 ;;
    esac
  done <<< "$contexts"
  return $bad
}

# Guard against protecting on names no workflow produces.
verify_names_exist() {
  local missing=0
  local names contexts
  contexts=$(read_contexts) || return $?
  names=$(grep -hoE "^    name: .*" "$here/../.github/workflows/ci.yml" \
                                    "$here/../.github/workflows/e2e.yml" \
          | sed 's/^    name: //')
  if [ -z "${names//[[:space:]]/}" ]; then
    echo "ERROR: no job names parsed from ci.yml and e2e.yml; the name scan failed." >&2
    return 2
  fi
  while IFS= read -r ctx; do
    [ -n "$ctx" ] || continue
    # matrix jobs appear in source as `... — ${{ matrix.os }}`
    local stem="${ctx% — *}"
    if ! printf '%s\n' "$names" | grep -qF "$stem"; then
      echo "  WARN: no workflow job matches '$ctx' (stem '$stem')" >&2
      missing=1
    fi
  done <<< "$contexts"
  return $missing
}

case "${1:-show}" in
  show)
    gh api "repos/$REPO/branches/$BRANCH/protection" 2>&1 || true
    ;;
  verify)
    echo "Checking $CONFIG"
    verify_contexts && echo "  contexts: em dash OK"
    verify_names_exist && echo "  contexts: all match a workflow job name"
    ;;
  apply)
    verify_contexts || { echo "refusing to apply: hyphen/em-dash problem" >&2; exit 1; }
    # Refuse, as verify_contexts does. This used to warn and continue, which
    # made the check advisory against the very outcome it exists to prevent:
    # protecting on a context no workflow emits leaves every PR waiting forever
    # for a check that can never arrive. PR #1313 hit that shape from the other
    # direction — a skipped matrix job never published its expanded names — and
    # sat unmergeable until it was admin-merged.
    # rc 2 means the check could not run; ALLOW_MISSING_NAMES waives a known
    # missing name, never a scan that inspected nothing.
    names_rc=0
    verify_names_exist || names_rc=$?
    if [ "$names_rc" -eq 2 ]; then
      echo "refusing to apply: the required-context check could not run." >&2
      exit 2
    fi
    if [ "$names_rc" -ne 0 ]; then
      if [ "${ALLOW_MISSING_NAMES:-}" = "1" ]; then
        echo "  (continuing: ALLOW_MISSING_NAMES=1)" >&2
      else
        echo "refusing to apply: a required context matches no workflow job name." >&2
        echo "  Applying this would leave PRs permanently pending on that check." >&2
        echo "  If the workflow lands separately, re-run with ALLOW_MISSING_NAMES=1." >&2
        exit 1
      fi
    fi
    gh api -X PUT "repos/$REPO/branches/$BRANCH/protection" --input "$CONFIG"
    ;;
  remove)
    gh api -X DELETE "repos/$REPO/branches/$BRANCH/protection"
    ;;
  *)
    echo "usage: $0 {show|verify|apply|remove}" >&2
    exit 2
    ;;
esac
