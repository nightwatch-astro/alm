#!/usr/bin/env bash
# Generated-artifact drift gate.
#
# Invariant: every committed generated artifact matches what its generator
# produces from the current sources. Run the generators first, then call this.
#
# Checks tracked modifications AND untracked additions. `git diff --exit-code`
# alone reports 0 for a regeneration that only ADDS a file, so a new binding or
# declaration could land uncommitted and the gate would pass.
#
# Usage:
#   bash scripts/check-generated-drift.sh                 # every artifact path
#   bash scripts/check-generated-drift.sh <path>...       # only these paths
# Exits 0 on pass, 1 on drift.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

if [[ $# -gt 0 ]]; then
  PATHS=("$@")
else
  PATHS=(
    "specs/*/contracts/*.generated.json"
    "apps/desktop/src/bindings/"
    "packages/contracts/src/generated/"
  )
fi

# Quoted, so git receives the wildcard as a pathspec and matches it itself. An
# unquoted expansion would leave the shell's own glob unmatched-and-literal when
# no spec has generated contracts yet, and git would reject the pathspec.
status="$(git status --porcelain --untracked-files=all -- "${PATHS[@]}")"

if [[ -z "$status" ]]; then
  echo "Generated artifacts are up to date."
  exit 0
fi

echo "Generated artifacts have drifted from their sources:" >&2
echo "$status" >&2
echo >&2
git --no-pager diff -- "${PATHS[@]}" >&2 || true
echo >&2
echo "Regenerate and commit the result. See the check-generated recipe." >&2
exit 1
