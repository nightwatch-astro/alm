#!/usr/bin/env bash
# Copyright (C) 2024-2026 Sjors Robroek
# SPDX-License-Identifier: AGPL-3.0-only
#
# Fail when a tracked file under .github/ is also matched by a gitignore rule
# (astro-plan-o1om). Sealed at zero.
#
# Why this exists: git keeps honouring a tracked file, so an ignore rule over one
# changes nothing about commits or CI and produces no warning. Every
# ignore-respecting scanner drops it. `.github/workflows/ci.yml` was ignored as
# `# parked: needs workflow-scoped push`, and for as long as that entry stood,
# ripgrep, opengrep, gitleaks, trivy, zizmor, actionlint, osv-scanner, and
# ast-grep all reported a clean primary CI workflow they never opened. Measured at
# the time: `rg -n 'run: just ' .github/workflows/` returned 0 and `--no-ignore`
# returned 5.
#
# The failure mode is a clean report, not an error, which is why it needs a guard
# rather than a convention.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT"

hidden="$(git ls-files -i -c --exclude-standard -- .github)"

if [ -z "$hidden" ]; then
  echo "OK: no tracked file under .github/ is hidden by a gitignore rule."
  exit 0
fi

echo "FAIL: tracked files under .github/ are matched by a gitignore rule:" >&2
printf '    %s\n' "$hidden" >&2
cat >&2 <<'EOF'

git still tracks these, so the ignore rule changes nothing about what gets
committed. What it changes is that every ignore-respecting scanner skips them and
reports the surface clean. Drop the rule, or untrack the file if it genuinely
should not be in the repo.

To find which rule matches:
  git check-ignore -v <path>
EOF
exit 1
