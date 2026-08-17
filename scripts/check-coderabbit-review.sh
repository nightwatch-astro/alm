#!/usr/bin/env bash
# CodeRabbit review-status scanner.
#
# Answers one question per PR: has CodeRabbit actually reviewed the current head,
# or does it only LOOK reviewed? The distinction matters because CodeRabbit posts
# an "Action performed / Review finished" acknowledgement even when it refused to
# review — on a rate limit or a draft skip. Reading that marker as success is how
# a review cycle records unreviewed PRs as "zero findings".
#
# A PR is REVIEWED only when a review body from coderabbitai[bot] exists for the
# current head SHA. Acknowledgements never count as evidence.
#
# Statuses:
#   REVIEWED      real review present for head; inline finding count reported
#   RATE_LIMITED  bot declined — "Review limit reached" / fair-usage notice.
#                 Reports when the window reopens as an absolute UTC time: the
#                 bot's own "in N minutes" is relative to when it was posted and
#                 is therefore stale on read.
#   SKIPPED_DRAFT bot declined — draft PR and auto-review is off for drafts
#   STALE         review exists, but only for an older commit than head
#   PENDING       review in progress
#   NOT_REVIEWED  no review and no explanatory notice
#
# Exit 0 when every scanned PR is REVIEWED; 1 otherwise. Only REVIEWED is a
# green light to adjudicate findings — every other status means re-trigger.
#
# Usage:
#   bash scripts/check-coderabbit-review.sh 1604 1605     # specific PRs
#   bash scripts/check-coderabbit-review.sh --open        # every open PR
#   bash scripts/check-coderabbit-review.sh --json 1604   # machine-readable
set -euo pipefail

BOT='coderabbitai[bot]'

# Matched loosely on purpose. The bot rephrases: it has posted both
# "**Next review available in:** **31 minutes**" and "Your next included review
# will be available in 48 minutes", and the earlier fixed-sentence pattern
# silently dropped the second one. So detection never demands a sentence shape:
# a rate-limit indicator establishes that the window is shut, and any nearby
# "<number> minutes|hours" figure supplies the wait. A future rewording still
# parses as long as it says it is a limit and quotes a duration.
RATE_LIMIT_RE='rate limited by coderabbit|review limit|fair usage'
# Non-alphabetic filler only, so the figure must belong to the unit it precedes:
# spans bold markers and whitespace ("**31** minutes") but never skips a word.
# Kept free of backslashes to stay valid in both grep -E and jq's test().
WAIT_FIGURE_RE='[0-9]+[^a-zA-Z]*(minute|hour)'

usage() {
  cat >&2 <<'EOF'
usage: check-coderabbit-review.sh [--json] (--open | <pr-number>...)

  --open   scan every open PR in the current repository
  --json   emit one JSON object per PR instead of aligned text

Exits 0 only when every scanned PR has a real CodeRabbit review for its
current head commit.
EOF
  exit 2
}

json_mode=0
open_mode=0
prs=()

while (($# > 0)); do
  case "$1" in
    --json) json_mode=1 ;;
    --open) open_mode=1 ;;
    -h | --help) usage ;;
    -*) printf 'unknown option: %s\n' "$1" >&2; usage ;;
    *) prs+=("$1") ;;
  esac
  shift
done

command -v gh >/dev/null || { echo 'check-coderabbit-review: gh is required' >&2; exit 2; }
((json_mode == 0)) || command -v jq >/dev/null ||
  { echo 'check-coderabbit-review: --json requires jq' >&2; exit 2; }

OPEN_SCAN_LIMIT=1000

if ((open_mode)); then
  ((${#prs[@]} == 0)) || { echo 'check-coderabbit-review: --open takes no PR numbers' >&2; usage; }
  # Capture separately from `mapfile` so a `gh` failure is reported as itself.
  # Piping straight into `mapfile` swallows the exit status, leaves `prs` empty,
  # and trips the generic `usage` path below -- so an auth, network, or API
  # error surfaces as "wrong arguments", sending the reader after the wrong bug.
  if ! open_prs=$(gh pr list --state open --limit "$OPEN_SCAN_LIMIT" --json number --jq '.[].number'); then
    echo 'check-coderabbit-review: gh pr list failed; cannot enumerate open PRs' >&2
    exit 2
  fi
  mapfile -t prs <<<"$open_prs"
  # An empty repo is legitimately zero PRs, not an error, but `mapfile` on an
  # empty string yields one empty element. Drop it so the count is honest.
  ((${#prs[@]} == 1)) && [[ -z ${prs[0]} ]] && prs=()
  # A silent truncation is the failure this whole script exists to prevent: a
  # partial scan that still exits 0 reports the PRs it never looked at as fine.
  # Say so loudly rather than trusting the cap to stay above the real count.
  if ((${#prs[@]} >= OPEN_SCAN_LIMIT)); then
    printf 'check-coderabbit-review: hit the --open cap of %d PRs; results are PARTIAL\n' \
      "$OPEN_SCAN_LIMIT" >&2
  fi
fi

((${#prs[@]} > 0)) || usage

# Report when the next review becomes available, given a rate-limit notice and
# the time it was posted. The bot writes a relative figure ("in 31 minutes"),
# which decays as the notice ages, so convert it to an absolute UTC instant and
# subtract elapsed time. Echoes a human phrase, or nothing when the notice
# carries no figure.
wait_hint() { # $1 = notice created_at (ISO 8601), $2 = notice body
  local created="$1" body="$2" minutes posted now deadline remaining

  # Any duration in the notice, whatever sentence surrounds it. The caller only
  # passes bodies that already matched RATE_LIMIT_RE, so the figure is the wait.
  minutes=$(grep -oiE "$WAIT_FIGURE_RE" <<<"$body" | head -1 || true)
  [[ -n "$minutes" ]] || return 0

  local value unit
  value=$(grep -oE '[0-9]+' <<<"$minutes" | head -1)
  unit=$(grep -oiE '(minute|hour)' <<<"$minutes" | head -1 | tr '[:upper:]' '[:lower:]')
  [[ "$unit" == hour ]] && value=$((value * 60))

  # No created_at (or an unparseable one): fall back to the bot's own figure.
  [[ -n "$created" ]] || { printf 'next review in ~%d min (per the notice)' "$value"; return 0; }

  # GNU and BSD date disagree on ISO-8601 parsing; try both.
  posted=$(date -u -d "$created" +%s 2>/dev/null ||
    date -u -j -f '%Y-%m-%dT%H:%M:%SZ' "$created" +%s 2>/dev/null || echo '')
  [[ -n "$posted" ]] || { printf 'next review in ~%d min (per the notice)' "$value"; return 0; }

  now=$(date -u +%s)
  deadline=$((posted + value * 60))
  remaining=$(((deadline - now + 59) / 60))

  if ((remaining > 0)); then
    printf 'next review in ~%d min (at %s UTC)' \
      "$remaining" "$(date -u -r "$deadline" +%H:%M 2>/dev/null || date -u -d "@$deadline" +%H:%M)"
  else
    printf 'review window has reopened (elapsed %d min ago) — safe to re-trigger' \
      "$((-remaining))"
  fi
}

# Classify one PR. Echoes: status<TAB>head<TAB>inline_count<TAB>detail
classify() {
  local pr="$1" head reviews inline notices status detail count

  head=$(gh pr view "$pr" --json headRefOid --jq '.headRefOid') || {
    printf 'ERROR\t-\t0\tcannot read PR\n'; return
  }

  # Review bodies for the current head only. A review of an older commit does
  # not vouch for what is on the branch now.
  reviews=$(gh api "repos/:owner/:repo/pulls/$pr/reviews" --paginate \
    --jq "[.[] | select(.user.login == \"$BOT\")] | length" 2>/dev/null || echo 0)
  local reviews_at_head
  reviews_at_head=$(gh api "repos/:owner/:repo/pulls/$pr/reviews" --paginate \
    --jq "[.[] | select(.user.login == \"$BOT\" and .commit_id == \"$head\")] | length" 2>/dev/null || echo 0)

  inline=$(gh api "repos/:owner/:repo/pulls/$pr/comments" --paginate \
    --jq "[.[] | select(.user.login == \"$BOT\")] | length" 2>/dev/null || echo 0)

  # Issue comments carry the bot's refusal notices. Match on the human-readable
  # text as well as the machine marker, since either may change independently.
  notices=$(gh api "repos/:owner/:repo/issues/$pr/comments" --paginate \
    --jq "[.[] | select(.user.login == \"$BOT\") | .body] | join(\"\n\")" 2>/dev/null || echo '')

  # The most recent limit notice that actually quotes a figure, with the time it
  # was posted. The bot's figure is relative to when it was WRITTEN, so the raw
  # number is stale on read — pair it with created_at to get a real wall-clock
  # deadline. Selecting the *last* bot comment is wrong here: that is usually the
  # bare "Review finished" ack, which carries no figure. Selected on signal words
  # plus a duration rather than a sentence, so a reworded notice still qualifies.
  local latest
  latest=$(gh api "repos/:owner/:repo/issues/$pr/comments" --paginate \
    --jq "[.[] | select(.user.login == \"$BOT\" and (.body | test(\"$RATE_LIMIT_RE\"; \"i\")) and (.body | test(\"$WAIT_FIGURE_RE\"; \"i\")))] | last | \"\(.created_at // \"\")\t\(.body // \"\")\"" \
    2>/dev/null || printf '\t')

  count="$inline"
  if ((reviews_at_head > 0)); then
    status=REVIEWED
    detail="$inline inline finding(s)"
  elif ((reviews > 0)); then
    # A prior review exists but not for head — e.g. findings were addressed and
    # pushed. This outranks the notice checks below, whose text may be left over
    # from an earlier declined attempt on an older commit.
    status=STALE
    detail="review exists but not for head ${head:0:8} — re-trigger for the new commits"
  elif grep -qiE "$RATE_LIMIT_RE" <<<"$notices"; then
    status=RATE_LIMITED
    local hint
    hint=$(wait_hint "${latest%%$'\t'*}" "${latest#*$'\t'}")
    # No parsed figure means no knowledge of the window. Never phrase that as
    # safe to re-trigger: a false green light burns review quota for nothing.
    detail="bot declined: review limit — ${hint:-wait unknown — re-check before re-triggering}"
    count=0
  elif grep -qE 'skip review by coderabbit|Review skipped' <<<"$notices"; then
    status=SKIPPED_DRAFT
    detail='bot declined: draft skip — comment "@coderabbitai full review"'
    count=0
  elif grep -qE 'Review in progress|currently reviewing' <<<"$notices"; then
    status=PENDING
    detail='review in progress'
    count=0
  else
    status=NOT_REVIEWED
    detail='no review and no notice from the bot'
    count=0
  fi

  # An acknowledgement with no review body is the exact trap this script exists
  # to catch: flag it so the reason is visible in the output.
  if [[ "$status" != REVIEWED ]] && grep -qE 'Review finished|Full review finished' <<<"$notices"; then
    detail="$detail (bot posted 'Review finished' — NOT evidence of a review)"
  fi

  printf '%s\t%s\t%s\t%s\n' "$status" "$head" "$count" "$detail"
}

failed=0
((json_mode)) || printf '%-8s %-14s %-10s %s\n' 'PR' 'STATUS' 'FINDINGS' 'DETAIL'

for pr in "${prs[@]}"; do
  IFS=$'\t' read -r status head count detail < <(classify "$pr")
  [[ "$status" == REVIEWED ]] || failed=1

  if ((json_mode)); then
    # jq --arg, never printf interpolation: several details embed double quotes
    # (the draft-skip line quotes "@coderabbitai full review"), which would emit
    # unparseable JSON on the interface that exists for machine consumers.
    jq -cn --arg pr "$pr" --arg status "$status" --arg head "$head" \
      --arg count "$count" --arg detail "$detail" \
      '{pr: ($pr | tonumber? // $pr), status: $status, head: $head,
        inline_findings: ($count | tonumber? // 0), detail: $detail}'
  else
    printf '%-8s %-14s %-10s %s\n' "#$pr" "$status" "$count" "$detail"
  fi
done

if ((failed)); then
  ((json_mode)) || cat >&2 <<'EOF'

Not every PR has a real CodeRabbit review. Do NOT record an unreviewed PR as
clean: re-trigger with `@coderabbitai full review` and re-check.

Note: CodeRabbit posts inline comments up to ~10 minutes after the review body,
so a REVIEWED PR reporting 0 findings should be re-checked once before it is
treated as genuinely clean.
EOF
  exit 1
fi

((json_mode)) || printf '\nAll %d PR(s) have a real CodeRabbit review for their current head.\n' "${#prs[@]}"
