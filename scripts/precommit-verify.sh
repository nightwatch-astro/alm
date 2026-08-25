#!/usr/bin/env bash
# pre-commit wrapper that refuses a vacuous run.
#
# `pre-commit run --files <paths>` exits 0 when NO hook looked at any of those
# paths: the global `exclude:` in .pre-commit-config.yaml drops whole trees
# (specs/, .claude/, .specify/, .agents/, .codex/, .mcp.json,
# apps/desktop/src/bindings/) from every hook's file list, every hook prints
# "(no files to check) Skipped", and the run reports success. A checked-nothing
# run and a checked-everything run are then byte-identical in their exit status,
# which is how a specs-only change reached review with no gate behind it.
#
# This wrapper exits 1 on that case and always names the hooks that actually ran,
# so the count is reviewable evidence rather than an inference from exit 0.
#
# Usage: scripts/precommit-verify.sh <file>...
set -uo pipefail

if [[ $# -eq 0 ]]; then
  echo "usage: $0 <file>..." >&2
  exit 2
fi

out="$(pre-commit run --color=never --files "$@" 2>&1)"
status=$?
printf '%s\n' "$out"

# pre-commit terminates every hook's status line with Passed, Failed or Skipped.
ran="$(printf '%s\n' "$out" | grep -cE '(Passed|Failed)$' || true)"

echo ""
echo "pre-commit hooks that actually ran on these $# path(s): $ran"

if [[ "$ran" -eq 0 ]]; then
  echo "" >&2
  echo "VACUOUS RUN: no hook checked any of the $# path(s) given." >&2
  echo "Every hook skipped, so pre-commit's exit status says nothing about these files." >&2
  echo "Cause is almost always the global 'exclude:' in .pre-commit-config.yaml." >&2
  echo "Gate these paths with a check that sees them, or narrow the exclude." >&2
  exit 1
fi

exit "$status"
