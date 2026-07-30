#!/usr/bin/env bats
#
# Regression tests for the CodeRabbit review-status scanner.
#
# The case that motivated the script: CodeRabbit posts an "Action performed /
# Review finished" acknowledgement even when it declined to review (rate limit
# or draft skip). A scanner that treats that marker as success reports an
# unreviewed PR as clean. Every test below pins one classification so that
# cannot regress.

setup() {
  export TEST_ROOT
  TEST_ROOT=$(mktemp -d)
  export SCRIPT="$BATS_TEST_DIRNAME/../check-coderabbit-review.sh"
  export CR_HEAD="cafebabecafebabecafebabecafebabecafebabe"
  export CR_REVIEWS="$TEST_ROOT/reviews.json"
  export CR_INLINE="$TEST_ROOT/inline.json"
  export CR_NOTICES="$TEST_ROOT/notices.json"

  mkdir -p "$TEST_ROOT/bin"
  export PATH="$TEST_ROOT/bin:$PATH"

  printf '[]\n' >"$CR_REVIEWS"
  printf '[]\n' >"$CR_INLINE"
  printf '[]\n' >"$CR_NOTICES"

  write_gh_stub
}

teardown() {
  rm -rf "$TEST_ROOT"
}

# Stub gh so the scanner's four API shapes read from fixture files. jq runs for
# real, which keeps the --jq filters under test rather than mocked away.
write_gh_stub() {
  cat >"$TEST_ROOT/bin/gh" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail

filter=""
for ((i = 1; i <= $#; i++)); do
  if [[ "${!i}" == "--jq" ]]; then
    next=$((i + 1))
    filter="${!next}"
  fi
done

if [[ "$1" == "pr" && "$2" == "view" ]]; then
  printf '%s\n' "$CR_HEAD"
  exit 0
fi

if [[ "$1" == "api" ]]; then
  case "$2" in
    *"/reviews") jq -r "$filter" <"$CR_REVIEWS" ;;
    *"/comments")
      if [[ "$2" == *"/issues/"* ]]; then
        jq -r "$filter" <"$CR_NOTICES"
      else
        jq -r "$filter" <"$CR_INLINE"
      fi
      ;;
    *) echo "unexpected api path: $2" >&2; exit 1 ;;
  esac
  exit 0
fi

echo "unexpected gh invocation: $*" >&2
exit 1
STUB
  chmod +x "$TEST_ROOT/bin/gh"
}

bot_review() { # $1 = commit_id
  printf '[{"user":{"login":"coderabbitai[bot]"},"commit_id":"%s","body":"Actionable comments posted: 1"}]\n' \
    "$1" >"$CR_REVIEWS"
}

bot_notice() { # $1 = body text
  jq -n --arg b "$1" '[{"user":{"login":"coderabbitai[bot]"},"body":$b}]' >"$CR_NOTICES"
}

inline_count() { # $1 = how many inline comments
  jq -n --argjson n "$1" \
    '[range($n) | {user:{login:"coderabbitai[bot]"}, body:"finding"}]' >"$CR_INLINE"
}

@test "a real review for head is REVIEWED and exits 0" {
  bot_review "$CR_HEAD"
  inline_count 3
  run bash "$SCRIPT" 1234
  [ "$status" -eq 0 ]
  [[ "$output" == *REVIEWED* ]]
  [[ "$output" == *"3 inline finding(s)"* ]]
}

@test "a rate-limit notice is RATE_LIMITED, not clean" {
  bot_notice 'Review limit reached — you have reached your PR review limit'
  run bash "$SCRIPT" 1234
  [ "$status" -eq 1 ]
  [[ "$output" == *RATE_LIMITED* ]]
}

@test "a rate limit plus a Review finished ack is still RATE_LIMITED" {
  # The exact shape that fooled the earlier review cycle.
  bot_notice 'rate limited by coderabbit.ai — Review limit reached
Action performed: Review finished.'
  run bash "$SCRIPT" 1234
  [ "$status" -eq 1 ]
  [[ "$output" == *RATE_LIMITED* ]]
  [[ "$output" == *"NOT evidence of a review"* ]]
}

@test "a fair-usage notice is RATE_LIMITED" {
  bot_notice 'Full review finished.

Your included review limit is currently reached under our Fair Usage Limits Policy.'
  run bash "$SCRIPT" 1234
  [ "$status" -eq 1 ]
  [[ "$output" == *RATE_LIMITED* ]]
}

@test "a draft skip is SKIPPED_DRAFT" {
  bot_notice 'skip review by coderabbit.ai — Review skipped. Draft detected.'
  run bash "$SCRIPT" 1234
  [ "$status" -eq 1 ]
  [[ "$output" == *SKIPPED_DRAFT* ]]
}

@test "a review of an older commit is STALE, never REVIEWED" {
  bot_review "0000000000000000000000000000000000000000"
  run bash "$SCRIPT" 1234
  [ "$status" -eq 1 ]
  [[ "$output" == *STALE* ]]
}

@test "a stale review outranks a leftover rate-limit notice" {
  # Findings were addressed and pushed after an earlier declined attempt: the
  # actionable state is 'needs a re-trigger for the new commits', not 'rate
  # limited'.
  bot_review "0000000000000000000000000000000000000000"
  bot_notice 'Review limit reached'
  run bash "$SCRIPT" 1234
  [ "$status" -eq 1 ]
  [[ "$output" == *STALE* ]]
  [[ "$output" != *RATE_LIMITED* ]]
}

@test "a real review at head outranks an old rate-limit notice" {
  # Ordering hazard: the decline notice stays in comment history forever, so a
  # notice-first scan keeps reporting RATE_LIMITED after the review lands.
  # Evidence at head wins, matching bot-review-probe.py's evidence-first rule.
  bot_notice 'Review limit reached — next review available in 48 minutes'
  bot_review "$CR_HEAD"
  inline_count 2
  run bash "$SCRIPT" 1234
  [ "$status" -eq 0 ]
  [[ "$output" == *REVIEWED* ]]
  [[ "$output" != *RATE_LIMITED* ]]
}

@test "a rate-limit notice plus a review of an older commit is STALE" {
  bot_notice 'Review limit reached — next included review will be available in 48 minutes'
  bot_review "0000000000000000000000000000000000000000"
  run bash "$SCRIPT" 1234
  [ "$status" -eq 1 ]
  [[ "$output" == *STALE* ]]
  [[ "$output" != *REVIEWED* ]]
  [[ "$output" != *RATE_LIMITED* ]]
}

@test "a draft-skip notice cannot mask a genuine review at head" {
  bot_notice 'skip review by coderabbit.ai — Review skipped. Draft detected.'
  bot_review "$CR_HEAD"
  run bash "$SCRIPT" 1234
  [ "$status" -eq 0 ]
  [[ "$output" == *REVIEWED* ]]
  [[ "$output" != *SKIPPED_DRAFT* ]]
}

@test "silence is NOT_REVIEWED" {
  run bash "$SCRIPT" 1234
  [ "$status" -eq 1 ]
  [[ "$output" == *NOT_REVIEWED* ]]
}

@test "a bare Review finished ack with no review body is NOT_REVIEWED" {
  bot_notice 'Action performed: Review finished.'
  run bash "$SCRIPT" 1234
  [ "$status" -eq 1 ]
  [[ "$output" == *NOT_REVIEWED* ]]
  [[ "$output" == *"NOT evidence of a review"* ]]
}

@test "a reviewed PR with zero findings still exits 0 but warns about late comments" {
  bot_review "$CR_HEAD"
  inline_count 0
  run bash "$SCRIPT" 1234
  [ "$status" -eq 0 ]
  [[ "$output" == *REVIEWED* ]]
  [[ "$output" == *"0 inline finding(s)"* ]]
}

@test "one declined PR fails a mixed batch" {
  bot_review "$CR_HEAD"
  run bash "$SCRIPT" 1234 5678
  [ "$status" -eq 0 ]

  bot_review "0000000000000000000000000000000000000000"
  run bash "$SCRIPT" 1234 5678
  [ "$status" -eq 1 ]
}

@test "--json emits one parseable object per PR" {
  bot_review "$CR_HEAD"
  inline_count 2
  run bash "$SCRIPT" --json 1234
  [ "$status" -eq 0 ]
  echo "$output" | jq -e '.status == "REVIEWED" and .inline_findings == 2 and .pr == 1234'
}

@test "--json is parseable for a detail containing double quotes" {
  # The draft-skip detail quotes "@coderabbitai full review"; printf-interpolated
  # JSON broke on it. Pipe through jq so invalid JSON fails the test.
  bot_notice 'skip review by coderabbit.ai — Review skipped. Draft detected.'
  run bash "$SCRIPT" --json 1234
  [ "$status" -eq 1 ]
  echo "$output" | jq -e '.status == "SKIPPED_DRAFT" and (.detail | contains("\"@coderabbitai full review\""))'
}

@test "--json is parseable for every status the scanner emits" {
  # status<TAB>notice-body pairs (bash 3.2: no associative arrays).
  local pairs=(
    $'RATE_LIMITED\trate limited by coderabbit.ai — Review limit reached\nAction performed: Review finished.'
    $'SKIPPED_DRAFT\tskip review by coderabbit.ai — Review skipped. Draft detected.'
    $'PENDING\tReview in progress'
    $'NOT_REVIEWED\tAction performed: Review finished.'
  )

  for pair in "${pairs[@]}"; do
    local want=${pair%%$'\t'*} body=${pair#*$'\t'}
    printf '[]\n' >"$CR_REVIEWS"
    bot_notice "$body"
    run bash "$SCRIPT" --json 1234
    [ "$status" -eq 1 ]
    echo "$output" | jq -e --arg want "$want" '.status == $want and (.detail | type) == "string"'
  done

  printf '[]\n' >"$CR_NOTICES"
  bot_review "$CR_HEAD"
  run bash "$SCRIPT" --json 1234
  [ "$status" -eq 0 ]
  echo "$output" | jq -e '.status == "REVIEWED"'

  bot_review "0000000000000000000000000000000000000000"
  run bash "$SCRIPT" --json 1234
  [ "$status" -eq 1 ]
  echo "$output" | jq -e '.status == "STALE"'

  # ERROR: gh cannot read the PR at all.
  printf '#!/usr/bin/env bash\nexit 1\n' >"$TEST_ROOT/bin/gh"
  chmod +x "$TEST_ROOT/bin/gh"
  run bash "$SCRIPT" --json 1234
  [ "$status" -eq 1 ]
  echo "$output" | jq -e '.status == "ERROR" and .inline_findings == 0'
}

@test "no arguments is a usage error" {
  run bash "$SCRIPT"
  [ "$status" -eq 2 ]
}

# --- wait-time reporting -----------------------------------------------------
#
# The bot writes "Next review available in: N minutes" relative to when the
# notice was posted, so the figure decays. These pin the conversion to an
# absolute deadline, which is the number a caller can actually act on.

rate_limit_notice_at() { # $1 = created_at, $2 = minutes quoted by the bot
  jq -n --arg t "$1" --arg m "$2" \
    '[{user:{login:"coderabbitai[bot]"},created_at:$t,
       body:("Review limit reached\n\n> **Next review available in:** **" + $m + " minutes**")}]' \
    >"$CR_NOTICES"
}

@test "a still-closed window reports minutes remaining and a UTC time" {
  # Posted 5 minutes ago quoting 30 → ~25 remaining.
  rate_limit_notice_at "$(date -u -r "$(( $(date -u +%s) - 300 ))" +%Y-%m-%dT%H:%M:%SZ)" 30
  run bash "$SCRIPT" 1234
  [ "$status" -eq 1 ]
  [[ "$output" == *RATE_LIMITED* ]]
  [[ "$output" =~ next\ review\ in\ ~2[0-9]\ min ]]
  [[ "$output" == *"UTC)"* ]]
}

@test "an elapsed window reports that it has reopened" {
  # Posted 2 hours ago quoting 30 minutes → long since reopened.
  rate_limit_notice_at "$(date -u -r "$(( $(date -u +%s) - 7200 ))" +%Y-%m-%dT%H:%M:%SZ)" 30
  run bash "$SCRIPT" 1234
  [ "$status" -eq 1 ]
  [[ "$output" == *"has reopened"* ]]
  [[ "$output" == *"safe to re-trigger"* ]]
}

@test "an hour-denominated figure is converted to minutes" {
  jq -n --arg t "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    '[{user:{login:"coderabbitai[bot]"},created_at:$t,
       body:"Review limit reached\n\n> **Next review available in:** **1 hour**"}]' \
    >"$CR_NOTICES"
  run bash "$SCRIPT" 1234
  [ "$status" -eq 1 ]
  [[ "$output" =~ next\ review\ in\ ~(59|60)\ min ]]
}

@test "a rate limit with no quoted figure admits the wait is unknown" {
  # An unparseable notice must never read as a green light: on 2026-07-30 a
  # false "safe to re-trigger" burned four reviews of quota.
  bot_notice 'Review limit reached — no figure given'
  run bash "$SCRIPT" 1234
  [ "$status" -eq 1 ]
  [[ "$output" == *RATE_LIMITED* ]]
  [[ "$output" == *"wait unknown"* ]]
  [[ "$output" != *"reopened"* ]]
  [[ "$output" != *"safe to re-trigger"* ]]
}

@test "the live fair-usage wording yields a real wait, not a reopened window" {
  # Verbatim from PR #1622, 2026-07-30T05:31:17Z. The earlier patterns expected
  # "next review available in" and dropped this entirely.
  jq -n --arg t "$(date -u -r "$(($(date -u +%s) - 300))" +%Y-%m-%dT%H:%M:%SZ)" \
    '[{user:{login:"coderabbitai[bot]"},created_at:$t,
       body:"Your included review limit is currently reached under our Fair Usage Limits Policy. This review may still proceed through usage-based billing if eligible. Your next included review will be available in 48 minutes."}]' \
    >"$CR_NOTICES"
  run bash "$SCRIPT" 1234
  [ "$status" -eq 1 ]
  [[ "$output" == *RATE_LIMITED* ]]
  [[ "$output" =~ next\ review\ in\ ~4[0-9]\ min ]]
  [[ "$output" != *"reopened"* ]]
}

@test "an unseen rewording of the notice still yields the wait figure" {
  # Detection is deliberately loose: a limit indicator plus any duration. This
  # invented wording matches none of the phrasings the bot has used so far.
  jq -n --arg t "$(date -u -r "$(($(date -u +%s) - 60))" +%Y-%m-%dT%H:%M:%SZ)" \
    '[{user:{login:"coderabbitai[bot]"},created_at:$t,
       body:"Fair Usage: reviews resume for this account in 2 hours."}]' >"$CR_NOTICES"
  run bash "$SCRIPT" 1234
  [ "$status" -eq 1 ]
  [[ "$output" == *RATE_LIMITED* ]]
  [[ "$output" =~ next\ review\ in\ ~11[0-9]\ min ]]
}

@test "the bold Next review available in wording still parses" {
  rate_limit_notice_at "$(date -u -r "$(($(date -u +%s) - 60))" +%Y-%m-%dT%H:%M:%SZ)" 31
  run bash "$SCRIPT" 1234
  [ "$status" -eq 1 ]
  [[ "$output" =~ next\ review\ in\ ~3[01]\ min ]]
}

@test "the wait figure is read from the notice, not the trailing ack" {
  # Real shape: the rate-limit notice comes first, a bare "Review finished" ack
  # last. Reading only the last comment loses the figure entirely.
  jq -n --arg t "$(date -u -r "$(( $(date -u +%s) - 60 ))" +%Y-%m-%dT%H:%M:%SZ)" \
    '[{user:{login:"coderabbitai[bot]"},created_at:$t,
       body:"Review limit reached\n\n> **Next review available in:** **25 minutes**"},
      {user:{login:"coderabbitai[bot]"},created_at:$t,
       body:"Action performed: Review finished."}]' >"$CR_NOTICES"
  run bash "$SCRIPT" 1234
  [ "$status" -eq 1 ]
  [[ "$output" =~ next\ review\ in\ ~2[0-9]\ min ]]
}
