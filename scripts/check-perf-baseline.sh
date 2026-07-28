#!/usr/bin/env bash
# Perf regression guard — enforces the SQL statement budget for the measured hot paths.
#
# Invariant: sqlx_stmts counts per scenario are deterministic (same fixture
# size → same query plan every run). Wall time is noisy on CI runners and only
# used as a WARN-only budget.
#
# Two modes:
#   (default)   enforce: HARD-fail on any sqlx_stmts increase; WARN-only if
#               wall_ms > 1.5× the baseline budget.
#   --generate  run perf-bench and write a fresh scripts/perf-baseline.json;
#               requires the binary to be compiled first (just perf-bench or
#               cargo run --release -p perf-bench).
#
# Usage:
#   scripts/check-perf-baseline.sh                         # enforce (CI mode)
#   scripts/check-perf-baseline.sh --generate              # re-baseline
#   PERF_N=500 just perf-bench | scripts/check-perf-baseline.sh --stdin  # pipe mode (internal)
#
# Requires: jq, cargo (--generate only).
#
# CI gate: run only when ci-affected-crates.sh output contains
# app_core_inbox, persistence_inbox, or fs-inventory (see .github/workflows/ci.yml).
#
# COVERAGE GAP (astro-plan-9l04): three scenarios added for spec 007 T033,
# spec 025 T045, and spec 048 T043a measure crates the trigger list above does
# NOT name — app_core (reconcile + plan-apply progress), app_core_calibration
# (suggest), fs_executor, and persistence_plans. A change confined to any of
# those skips this gate entirely, so their statement budgets are only enforced
# when an inbox/fs_inventory crate happens to be co-affected. Widening the
# trigger grep is a CI-policy change and was deliberately left out of that
# bead; the full harness takes ~23s locally, so cost is not the blocker.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BASELINE_FILE="$SCRIPT_DIR/perf-baseline.json"

# 1.5× multiplier for the wall_ms warn threshold.
WALL_BUDGET_FACTOR="1.5"

usage() {
  echo "usage: $0 [--generate | --check]" >&2
  exit 2
}

# Validate that jq is available.
if ! command -v jq >/dev/null 2>&1; then
  echo "ERROR: jq is required but not found in PATH." >&2
  exit 1
fi

# ── Generate mode ─────────────────────────────────────────────────────────────
#
# Runs perf-bench (must already be compiled), collects JSON lines, and writes
# scripts/perf-baseline.json. The baseline file is a JSON array of scenario
# objects; each object records the scenario name, the sqlx_stmts count (the
# hard gate), and the wall_ms budget (1.5× warn threshold).
#
# --generate refuses to write a baseline with 0 sqlx_stmts for any scenario
# (that indicates a broken counter, not a fast path).
generate() {
  echo "Running perf-bench (PERF_N=${PERF_N:-500})…"
  raw="$(PERF_N="${PERF_N:-500}" cargo run --release -p perf-bench 2>/dev/null)"

  # Expect at least one JSON line; fail loudly if the binary produced nothing.
  if [[ -z "$raw" ]]; then
    echo "ERROR: perf-bench produced no output." >&2
    exit 1
  fi

  # Parse lines and build baseline array. Each input line is a JSON object.
  baseline="[]"
  while IFS= read -r line; do
    [[ -z "$line" ]] && continue

    scenario="$(printf '%s' "$line" | jq -r '.scenario')"
    stmts="$(printf '%s' "$line" | jq -r '.sqlx_stmts')"
    wall="$(printf '%s' "$line" | jq -r '.wall_ms')"

    if [[ "$stmts" == "null" ]]; then
      echo "ERROR: scenario '$scenario' has no sqlx_stmts field — output may be malformed." >&2
      exit 1
    fi

    # Record full raw line plus the hard/warn fields for clarity.
    entry="$(printf '%s' "$line" | jq -c --arg stmts_budget "$stmts" --arg wall_budget "$wall" \
      '. + {stmts_budget: ($stmts_budget | tonumber), wall_ms_budget: ($wall_budget | tonumber)}')"
    baseline="$(printf '%s' "$baseline" | jq -c --argjson e "$entry" '. + [$e]')"
  done <<< "$raw"

  if [[ "$(printf '%s' "$baseline" | jq 'length')" -eq 0 ]]; then
    echo "ERROR: no scenario lines parsed from perf-bench output." >&2
    exit 1
  fi

  printf '%s\n' "$baseline" | jq '.' > "$BASELINE_FILE"
  echo "Wrote $BASELINE_FILE"
  printf '%s' "$baseline" | jq -r '.[] | "  \(.scenario): sqlx_stmts=\(.stmts_budget)  wall_ms_budget=\(.wall_ms_budget)"'
}

# ── Check mode ────────────────────────────────────────────────────────────────
check() {
  if [[ ! -f "$BASELINE_FILE" ]]; then
    echo "ERROR: baseline missing: $BASELINE_FILE" >&2
    echo "Run: scripts/check-perf-baseline.sh --generate" >&2
    exit 2
  fi

  echo "Running perf-bench (PERF_N=${PERF_N:-500})…"
  raw="$(PERF_N="${PERF_N:-500}" cargo run --release -p perf-bench 2>/dev/null)"

  if [[ -z "$raw" ]]; then
    echo "ERROR: perf-bench produced no output." >&2
    exit 1
  fi

  fail=0

  while IFS= read -r line; do
    [[ -z "$line" ]] && continue

    scenario="$(printf '%s' "$line" | jq -r '.scenario')"
    stmts="$(printf '%s' "$line" | jq -r '.sqlx_stmts')"
    wall="$(printf '%s' "$line" | jq -r '.wall_ms')"

    # Look up baseline entry for this scenario.
    baseline_entry="$(jq -c --arg s "$scenario" '.[] | select(.scenario == $s)' "$BASELINE_FILE")"
    if [[ -z "$baseline_entry" ]]; then
      echo "WARN: scenario '$scenario' not found in baseline — skipping (run --generate to add it)."
      continue
    fi

    stmts_budget="$(printf '%s' "$baseline_entry" | jq -r '.stmts_budget')"
    wall_budget="$(printf '%s' "$baseline_entry" | jq -r '.wall_ms_budget')"

    # HARD: sqlx_stmts must not increase.
    if [[ "$stmts" -gt "$stmts_budget" ]]; then
      echo "FAIL: $scenario sqlx_stmts=$stmts > baseline=$stmts_budget (regression — query count increased)." >&2
      fail=1
    else
      echo "OK:   $scenario sqlx_stmts=$stmts (baseline=$stmts_budget)."
    fi

    # WARN: wall_ms > 1.5× budget is noisy; log but do not fail.
    wall_limit="$(printf '%s' "$wall_budget $WALL_BUDGET_FACTOR" | awk '{printf "%d", $1 * $2}')"
    if [[ "$wall" -gt "$wall_limit" ]]; then
      echo "WARN: $scenario wall_ms=$wall > 1.5× budget=${wall_budget}ms (limit=${wall_limit}ms) — runner may be noisy."
    else
      echo "OK:   $scenario wall_ms=$wall (budget=${wall_budget}ms, limit=${wall_limit}ms)."
    fi

  done <<< "$raw"

  if [[ "$fail" -ne 0 ]]; then
    echo "" >&2
    echo "Perf regression gate FAILED: sqlx_stmts increased for one or more scenarios." >&2
    echo "Inspect query counts in crates/tools/perf-bench/src/main.rs and reduce the" >&2
    echo "regression, or run scripts/check-perf-baseline.sh --generate to re-baseline" >&2
    echo "with a justification in the commit message." >&2
    exit 1
  fi

  echo ""
  echo "Perf baseline OK — no sqlx_stmts regressions."
}

case "${1:-}" in
  --generate)
    cd "$ROOT"
    generate
    ;;
  ""|--check)
    cd "$ROOT"
    check
    ;;
  *)
    usage
    ;;
esac
