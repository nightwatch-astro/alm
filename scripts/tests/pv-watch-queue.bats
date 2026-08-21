#!/usr/bin/env bats

setup() {
  export TEST_ROOT
  TEST_ROOT=$(mktemp -d)
  export SCRIPT="$BATS_TEST_DIRNAME/../pv-watch-queue.sh"
  export PV_ORDER_FILE="$TEST_ROOT/order.txt"
  export PV_TEST_PRS="$TEST_ROOT/prs.json"
  export PV_TEST_CI="$TEST_ROOT/ci.json"
  export PV_TEST_E2E="$TEST_ROOT/e2e.json"
  export PV_TEST_MERGED="$TEST_ROOT/merged.json"
  export PV_TEST_MAIN_SHA="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  mkdir -p "$TEST_ROOT/bin"
  export PATH="$TEST_ROOT/bin:$PATH"

  printf '42\n' >"$PV_ORDER_FILE"
  printf '[]\n' >"$PV_TEST_MERGED"
  write_pr green false feature/test CLEAN
  write_main_runs success success exact

  write_gh_stub
}

write_gh_stub() {
  cat >"$TEST_ROOT/bin/gh" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail

if [[ "$1" == "pr" && "$2" == "list" ]]; then
  state=""
  for ((index = 1; index <= $#; index++)); do
    if [[ "${!index}" == "--state" ]]; then
      next=$((index + 1))
      state="${!next}"
    fi
  done
  if [[ "$state" == "open" ]]; then
    [[ "${PV_TEST_FAIL_PR_LIST:-0}" != "1" ]] || exit 1
    if [[ "${PV_TEST_MOVE_MAIN_ON_PR_LIST:-0}" == "1" ]]; then
      : >"$TEST_ROOT/main-moved"
    fi
    cat "$PV_TEST_PRS"
  else
    cat "$PV_TEST_MERGED"
  fi
  exit 0
fi

if [[ "$1" == "api" ]]; then
  if [[ -n "${PV_TEST_MAIN_SHA_NEXT:-}" && -f "$TEST_ROOT/main-moved" ]]; then
    printf '%s\n' "$PV_TEST_MAIN_SHA_NEXT"
  elif [[ -n "${PV_TEST_MAIN_SHA_NEXT:-}" && -f "$TEST_ROOT/main-sha-read" ]]; then
    printf '%s\n' "$PV_TEST_MAIN_SHA_NEXT"
  else
    : >"$TEST_ROOT/main-sha-read"
    printf '%s\n' "$PV_TEST_MAIN_SHA"
  fi
  exit 0
fi

if [[ "$1" == "run" && "$2" == "list" ]]; then
  workflow=""
  for ((index = 1; index <= $#; index++)); do
    if [[ "${!index}" == "--workflow" ]]; then
      next=$((index + 1))
      workflow="${!next}"
    fi
  done
  if [[ "$workflow" == "CI" ]]; then
    cat "$PV_TEST_CI"
  else
    cat "$PV_TEST_E2E"
  fi
  exit 0
fi

printf 'unexpected gh invocation:' >&2
printf ' %q' "$@" >&2
printf '\n' >&2
exit 2
STUB
  chmod +x "$TEST_ROOT/bin/gh"
}

teardown() {
  rm -rf -- "$TEST_ROOT"
}

write_pr() {
  local mode="$1" draft="$2" branch="$3" merge_state="$4"
  local checks
  checks=$(jq -n '[
    "Detect changes (CI)",
    "Unit + integration (L1+L2) — ubuntu-latest",
    "Unit + integration (L1+L2) — windows-latest",
    "UI mock-mode (Playwright)",
    "Real-UI smoke (L3) — ubuntu-latest",
    "Supply chain (cargo-deny)"
  ] | map({name: ., status: "COMPLETED", conclusion: "SUCCESS"})')

  case "$mode" in
    green) ;;
    missing) checks=$(jq '.[0:5]' <<<"$checks") ;;
    pending) checks=$(jq '.[5].status = "IN_PROGRESS" | .[5].conclusion = null' <<<"$checks") ;;
    failed) checks=$(jq '.[5].conclusion = "FAILURE"' <<<"$checks") ;;
    duplicate) checks=$(jq '. + [.[0]]' <<<"$checks") ;;
    *) return 2 ;;
  esac

  jq -n \
    --argjson draft "$draft" \
    --arg branch "$branch" \
    --arg merge_state "$merge_state" \
    --argjson checks "$checks" \
    '[{number: 42, title: "test queue policy", isDraft: $draft,
       headRefName: $branch, mergeStateStatus: $merge_state,
       statusCheckRollup: $checks}]' >"$PV_TEST_PRS"
}

write_main_runs() {
  local ci="$1" e2e="$2" sha_mode="$3"
  local sha="$PV_TEST_MAIN_SHA"
  [[ "$sha_mode" == exact ]] || sha="bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
  jq -n --arg sha "$sha" --arg state "$ci" '[{
    headSha: $sha,
    status: (if $state == "pending" then "in_progress" else "completed" end),
    conclusion: (if $state == "success" then "success" elif $state == "failed" then "failure" else null end),
    createdAt: "2026-07-21T00:00:00Z"
  }]' >"$PV_TEST_CI"
  jq -n --arg sha "$sha" --arg state "$e2e" '[{
    headSha: $sha,
    status: (if $state == "pending" then "queued" else "completed" end),
    conclusion: (if $state == "success" then "success" elif $state == "failed" then "failure" else null end),
    createdAt: "2026-07-21T00:00:00Z"
  }]' >"$PV_TEST_E2E"
}

set_next_main_sha() {
  export PV_TEST_MAIN_SHA_NEXT="bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
}

@test "six successful required contexts produce the ranked candidate" {
  run "$SCRIPT"
  [ "$status" -eq 0 ]
  [[ "$output" == *"-- READY  (1) --"* ]]
  [[ "$output" == *"CLEAR — next merge is #42"* ]]
}

@test "a missing required context is STUCK and never a candidate" {
  write_pr missing false feature/test CLEAN
  run "$SCRIPT"
  [ "$status" -eq 0 ]
  [[ "$output" == *"-- STUCK  (1) --"* ]]
  [[ "$output" == *"CLEAR — but nothing ranked is READY."* ]]
  [[ "$output" != *"next merge is #42"* ]]
}

@test "a pending required context is WAITING and never a candidate" {
  write_pr pending false feature/test CLEAN
  run "$SCRIPT"
  [ "$status" -eq 0 ]
  [[ "$output" == *"-- WAITING  (1) --"* ]]
  [[ "$output" != *"next merge is #42"* ]]
}

@test "a failed required context is FAILING and never a candidate" {
  write_pr failed false feature/test CLEAN
  run "$SCRIPT"
  [ "$status" -eq 0 ]
  [[ "$output" == *"-- FAILING  (1) --"* ]]
  [[ "$output" != *"next merge is #42"* ]]
}

@test "a draft with six successful contexts is held" {
  write_pr green true feature/test CLEAN
  run "$SCRIPT"
  [ "$status" -eq 0 ]
  [[ "$output" == *"-- HELD  (1) --"* ]]
  [[ "$output" == *"[draft - never merge]"* ]]
  [[ "$output" != *"next merge is #42"* ]]
}

@test "a release with six successful contexts is held" {
  write_pr green false release-please--branches--main CLEAN
  run "$SCRIPT"
  [ "$status" -eq 0 ]
  [[ "$output" == *"-- HELD  (1) --"* ]]
  [[ "$output" == *"[RELEASE - never merge]"* ]]
  [[ "$output" != *"next merge is #42"* ]]
}

@test "a duplicate required context fails closed as STUCK" {
  write_pr duplicate false feature/test CLEAN
  run "$SCRIPT"
  [ "$status" -eq 0 ]
  [[ "$output" == *"-- STUCK  (1) --"* ]]
  [[ "$output" != *"next merge is #42"* ]]
}

@test "a dirty PR with successful checks is a conflict, not a candidate" {
  write_pr green false feature/test DIRTY
  run "$SCRIPT"
  [ "$status" -eq 0 ]
  [[ "$output" == *"-- CONFLICT  (1) --"* ]]
  [[ "$output" != *"next merge is #42"* ]]
}

@test "a stale main workflow result holds the exact-SHA gate" {
  write_main_runs success success stale
  run "$SCRIPT"
  [ "$status" -eq 0 ]
  [[ "$output" == *"HOLD — exact main aaaaaaaa is not green: CI=ABSENT, real-UI=ABSENT."* ]]
  [[ "$output" == *"No merge candidate."* ]]
  [[ "$output" != *"next merge is #42"* ]]
}

@test "a pending exact-main real-UI run holds the gate" {
  write_main_runs success pending exact
  run "$SCRIPT"
  [ "$status" -eq 0 ]
  [[ "$output" == *"CI=SUCCESS, real-UI=PENDING(queued)"* ]]
  [[ "$output" == *"No merge candidate."* ]]
  [[ "$output" != *"next merge is #42"* ]]
}

@test "main moving during inspection holds the gate" {
  set_next_main_sha
  run "$SCRIPT"
  [ "$status" -eq 0 ]
  [[ "$output" == *"HOLD — main moved during inspection (aaaaaaaa -> bbbbbbbb)."* ]]
  [[ "$output" != *"next merge is #42"* ]]
}

@test "main moving during PR listing is detected from the initial snapshot" {
  set_next_main_sha
  export PV_TEST_MOVE_MAIN_ON_PR_LIST=1
  run "$SCRIPT"
  [ "$status" -eq 0 ]
  [[ "$output" == *"HOLD — main moved during inspection (aaaaaaaa -> bbbbbbbb)."* ]]
  [[ "$output" != *"next merge is #42"* ]]
}

@test "an unranked READY PR holds the queue for re-ranking" {
  printf '999\n' >"$PV_ORDER_FILE"
  run "$SCRIPT"
  [ "$status" -eq 0 ]
  [[ "$output" == *"UNRANKED AND READY: #42"* ]]
  [[ "$output" == *"HOLD — READY PRs are missing from the order."* ]]
  [[ "$output" != *"next merge is #42"* ]]
}

@test "a GitHub API failure exits before recommending a candidate" {
  export PV_TEST_FAIL_PR_LIST=1
  run "$SCRIPT"
  [ "$status" -ne 0 ]
  [[ "$output" != *"next merge is #42"* ]]
}
