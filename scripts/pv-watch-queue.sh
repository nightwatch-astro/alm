#!/usr/bin/env bash
# Read-only dashboard for the platevault/platevault merge queue.
#
# A PR is READY only when it is clean, is neither draft nor release automation,
# and has exactly one accepted result for every required context. SUCCESS and
# SKIPPED are accepted because branch protection treats both as satisfied. The
# same classified record drives the dashboard and merge-order selection.
set -Eeuo pipefail

readonly REPO="${PV_REPO:-platevault/platevault}"
readonly E2E_WORKFLOW="Real-UI E2E (thirtyfour + nextest + tauri-plugin-webdriver)"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
readonly SCRIPT_DIR
readonly ORDER_FILE="${PV_ORDER_FILE:-$SCRIPT_DIR/pv-merge-order.txt}"

for command_name in gh jq date mktemp; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    printf 'ERROR: required command not found: %s\n' "$command_name" >&2
    exit 127
  fi
done

work_dir=$(mktemp -d "${TMPDIR:-/tmp}/pv-watch-queue.XXXXXX")
trap 'rm -rf -- "$work_dir"' EXIT

prs_json="$work_dir/prs.json"
classified_json="$work_dir/classified.json"
ci_runs_json="$work_dir/ci-runs.json"
e2e_runs_json="$work_dir/e2e-runs.json"
merged_json="$work_dir/merged.json"

main_sha=$(gh api "repos/$REPO/commits/main" --jq .sha)
if [[ ! "$main_sha" =~ ^[0-9a-fA-F]{40}$ ]]; then
  printf 'ERROR: GitHub returned an invalid main SHA: %s\n' "$main_sha" >&2
  exit 1
fi

gh pr list -R "$REPO" --state open --base main --limit 100 \
  --json number,title,isDraft,headRefName,mergeStateStatus,statusCheckRollup >"$prs_json"

# A duplicate required context is ambiguous. It is not safe to guess which
# result supersedes the other, so duplicates classify as STUCK.
jq -e '
  ["Detect changes (CI)",
   "Unit + integration (L1+L2) — ubuntu-latest",
   "Unit + integration (L1+L2) — windows-latest",
   "UI mock-mode (Playwright)",
   "Real-UI smoke (L3) — ubuntu-latest",
   "Supply chain (cargo-deny)"] as $required
  | [ .[]
      | . as $pr
      | ($pr.statusCheckRollup // []) as $rollup
      | (($pr.headRefName | test("^release-please"))
          or ($pr.title | test("^chore(\\(.*\\))?: release"; "i"))) as $release
      | [ $required[] as $name
          | ($rollup | map(select(.name == $name))) as $matches
          | if ($matches | length) == 0 then "ABSENT"
            elif ($matches | length) > 1 then "AMBIGUOUS"
            elif $matches[0].status != "COMPLETED" then "RUNNING"
            elif ($matches[0].conclusion == "SUCCESS"
                  or $matches[0].conclusion == "SKIPPED") then "OK"
            else "FAIL"
            end ] as $checks
      | (($release | not)
          and ($pr.isDraft | not)
          and $pr.mergeStateStatus == "CLEAN"
          and ($checks | all(. == "OK"))) as $ready
      | {
          number: $pr.number,
          title: ($pr.title[0:52]),
          draft: $pr.isDraft,
          release: $release,
          mergeState: $pr.mergeStateStatus,
          checks: $checks,
          done: ($checks | map(select(. == "OK")) | length),
          total: ($required | length),
          ready: $ready,
          bucket:
            (if $release or $pr.isDraft then "HELD"
             elif $ready then "READY"
             elif $pr.mergeStateStatus == "DIRTY" then "CONFLICT"
             elif ($checks | index("FAIL")) != null then "FAILING"
             elif ($checks | index("RUNNING")) != null then "WAITING"
             elif (($checks | index("ABSENT")) != null
                   or ($checks | index("AMBIGUOUS")) != null) then "STUCK"
             else "WAITING"
             end)
        }
    ]
' "$prs_json" >"$classified_json"

printf '== %s ==\n' "$(date -u '+%H:%M:%SZ')"
jq -r '
  group_by(.bucket)
  | sort_by(.[0].bucket as $bucket
      | ["READY", "FAILING", "STUCK", "CONFLICT", "WAITING", "HELD"]
      | index($bucket))
  | .[]
  | "-- \(.[0].bucket)  (\(length)) --",
    (sort_by(.number) | reverse | .[]
      | "   #\(.number)\(if .release then " [RELEASE - never merge]" elif .draft then " [draft - never merge]" else "" end)  \(.done)/\(.total)  \(.title)")
' "$classified_json"

printf '%s\n' \
  '   note: STUCK = a required context is absent or ambiguous.' \
  '   note: HELD = release or draft. Never rank or merge these.'

ready_numbers=()
while IFS= read -r _num; do ready_numbers+=("$_num"); done \
  < <(jq -r '.[] | select(.ready) | .number' "$classified_json" | sort -n)
ranked_numbers=()
if [[ -f "$ORDER_FILE" ]]; then
  while IFS= read -r line; do
    if [[ "$line" =~ ^[[:space:]]*([0-9]+) ]]; then
      ranked_numbers+=("${BASH_REMATCH[1]}")
    fi
  done <"$ORDER_FILE"
fi

contains_number() {
  local needle="$1"
  shift
  local number
  for number in "$@"; do
    [[ "$number" == "$needle" ]] && return 0
  done
  return 1
}

printf '%s\n' '-- MERGE ORDER --'
if [[ ! -f "$ORDER_FILE" ]]; then
  printf '   (no %s — every READY PR is unranked)\n' "$ORDER_FILE"
fi

first_ranked_ready=""
for number in "${ranked_numbers[@]}"; do
  if contains_number "$number" "${ready_numbers[@]}"; then
    [[ -n "$first_ranked_ready" ]] || first_ranked_ready="$number"
    printf '   ranked #%s  READY\n' "$number"
  else
    printf '   ranked #%s  (not ready or no longer open)\n' "$number"
  fi
done

unranked_ready=()
for number in "${ready_numbers[@]}"; do
  if ! contains_number "$number" "${ranked_numbers[@]}"; then
    unranked_ready+=("$number")
  fi
done
if ((${#unranked_ready[@]} > 0)); then
  printf '   !! UNRANKED AND READY:'
  printf ' #%s' "${unranked_ready[@]}"
  printf '\n   !! Re-rank the whole order before merging anything.\n'
fi

gh run list -R "$REPO" --branch main --workflow CI --limit 100 \
  --json headSha,status,conclusion,createdAt >"$ci_runs_json"
gh run list -R "$REPO" --workflow "$E2E_WORKFLOW" --limit 100 \
  --json headSha,status,conclusion,createdAt >"$e2e_runs_json"
gh pr list -R "$REPO" --state merged --limit 100 --json mergedAt >"$merged_json"

for json_file in "$ci_runs_json" "$e2e_runs_json" "$merged_json"; do
  jq -e 'type == "array"' "$json_file" >/dev/null
done

workflow_state() {
  local runs_file="$1"
  jq -r --arg sha "$main_sha" '
    [.[] | select(.headSha == $sha)] | first
    | if . == null then "ABSENT"
      elif .status != "completed" then "PENDING(\(.status))"
      elif .conclusion == "success" then "SUCCESS"
      else "FAILED(\(.conclusion // "unknown"))"
      end
  ' "$runs_file"
}

main_ci_state=$(workflow_state "$ci_runs_json")
main_e2e_state=$(workflow_state "$e2e_runs_json")
confirmed_main_sha=$(gh api "repos/$REPO/commits/main" --jq .sha)
if [[ ! "$confirmed_main_sha" =~ ^[0-9a-fA-F]{40}$ ]]; then
  printf 'ERROR: GitHub returned an invalid confirmation SHA: %s\n' "$confirmed_main_sha" >&2
  exit 1
fi

printf '%s\n' '-- MERGE GATE --'
if [[ "$confirmed_main_sha" != "$main_sha" ]]; then
  printf '   HOLD — main moved during inspection (%s -> %s). No merge candidate.\n' \
    "${main_sha:0:8}" "${confirmed_main_sha:0:8}"
elif [[ "$main_ci_state" != "SUCCESS" || "$main_e2e_state" != "SUCCESS" ]]; then
  printf '   HOLD — exact main %s is not green: CI=%s, real-UI=%s.\n' \
    "${main_sha:0:8}" "$main_ci_state" "$main_e2e_state"
  printf '%s\n' '          No merge candidate.'
elif ((${#unranked_ready[@]} > 0)); then
  printf '%s\n' '   HOLD — READY PRs are missing from the order. No merge candidate.'
elif [[ -n "$first_ranked_ready" ]]; then
  printf '   CLEAR — next merge is #%s\n' "$first_ranked_ready"
  printf '%s\n' \
    '          verify the required checks on its HEAD SHA immediately before merging.' \
    '          Only one merge may be in flight at a time.'
else
  printf '%s\n' '   CLEAR — but nothing ranked is READY.'
fi

printf '%s\n' '-- E2E --'
jq -r '
  ([.[] | select(.status != "completed")] | length) as $pending
  | ([.[] | select(.conclusion == "success")] | length) as $ok
  | ([.[] | select(.conclusion == "failure")] | length) as $failed
  | ([.[] | select(.conclusion == "cancelled")] | length) as $cancelled
  | "   in-flight: \($pending)   recent: \($ok) ok / \($failed) fail / \($cancelled) cancelled (superseded)"
' "$e2e_runs_json"

oldest_queued=$(jq -r '[.[] | select(.status == "queued")] | sort_by(.createdAt) | .[0].createdAt // empty' "$e2e_runs_json")
if [[ -n "$oldest_queued" ]]; then
  # GNU date uses -d; BSD/macOS date uses -j -f
  if date --version >/dev/null 2>&1; then
    queued_epoch=$(date -u -d "$oldest_queued" +%s)
  else
    queued_epoch=$(date -u -j -f "%Y-%m-%dT%H:%M:%SZ" "$oldest_queued" +%s 2>/dev/null \
      || date -u -j -f "%Y-%m-%dT%H:%M:%S+00:00" "$oldest_queued" +%s)
  fi
  printf '   oldest queued: %sm\n' "$((( $(date -u +%s) - queued_epoch ) / 60))"
fi
printf '%s\n' \
  '   note: cancelled = superseded by an update-branch.' \
  '         Never batch update-branch; each update cancels that branch run.'

printf '%s\n' '-- main --'
printf '   %s  CI=%s  real-UI=%s\n' "${confirmed_main_sha:0:8}" "$main_ci_state" "$main_e2e_state"
printf '   merged today: %s\n' "$(jq --arg day "$(date -u +%Y-%m-%d)" '[.[] | select(.mergedAt[0:10] == $day)] | length' "$merged_json")"
