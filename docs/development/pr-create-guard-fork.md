# The pr-create-guard is a local fork

`.claude/hooks/steering-git-workflow/scripts/pr-create-guard.py` and its
`.codex` copy are a fork of the `steering-git-workflow` package script. The two
copies are byte-identical and both are tracked in git. `apm.lock.yaml` pins the
package at `srobroek/agentic-packages` commit
`835c506a875d39d8bd736175fbcb407877c3e585`, version 2.3.2.

## Why the fork exists

Upstream `main` removed bead-linkage validation in PR #856. The guard there
checks only that `gh pr create` passes `--draft`. This repo depends on the
linkage check: it blocks a PR whose `Merge-Bead` trailer names a bead that is
missing, closed, unlabeled, or missing `branch` / `repo` / `origin_actor`
metadata, and whose `Closes-Bead` trailer lacks a `blocks` edge to the merge
bead.

The pinned commit `835c506` predates PR #856, so it still carries the linkage
check. Do not delete the fork to align with upstream: that deletes the check.

## Every divergence from the pin

Line numbers are in the deployed fork. Fetch the pinned original with:

```sh
gh api "repos/srobroek/agentic-packages/contents/packages/steering-git-workflow/scripts/pr-create-guard.py?ref=835c506a875d39d8bd736175fbcb407877c3e585" \
  --jq .content | base64 -d > /tmp/pr-create-guard-upstream.py
diff -u /tmp/pr-create-guard-upstream.py \
  .claude/hooks/steering-git-workflow/scripts/pr-create-guard.py
```

The pinned original hashes to `sha256:cde890f723c015ac2efbed493c007493ae0435dd1d0e8dfb10b929695127bc2f`.

| # | Site | Divergence |
| --- | --- | --- |
| 1 | module docstring, lines 1-7 | Points at this document and states that `apm install` overwrites the file. |
| 2 | line 13 | Adds `import re`. |
| 3 | `advise()`, line 73 | New. Emits a `PreToolUse` allow decision carrying the reason a check went unverified. Upstream has only `deny()`. |
| 4 | `payload_command()`, line 94 | Returns `effective_cwd(command, cwd)` instead of the raw session `cwd`. |
| 5 | `effective_cwd()`, line 112 | New. Parses a leading `cd <path> &&` out of the command and returns that directory, falling back to the session directory when the path does not resolve to a directory. |
| 6 | `beads_workspace()`, line 360 | Asks `bd where` with `timeout=30` instead of walking parents for a `.beads` directory. A `.beads` in a shared ancestor such as `~` made every repository under the home directory look beads-enabled. |
| 7 | `metadata_ids()`, line 401 | New. `bd update --set-metadata` stringifies a list, so the field reads back as the JSON text `["id"]`. This parses that text and returns a set, or `None` when the value is not a list of strings. |
| 8 | `MISS_MESSAGE`, line 422 | New. Regex over `bd` stderr matching `no issue found`, `not found`, `no such`, `does not exist`, `unknown bead`, `unknown issue`. Separates a bead that does not exist from a lookup that failed. |
| 9 | `BeadUnavailable`, line 427 | New exception. Raised when the lookup itself could not complete, so absence of a record is not read as absence of a bead. |
| 10 | `bead_record()`, line 437 | Rewritten. Two attempts at `timeout=30` instead of one at `timeout=5`; captures stderr instead of discarding it; raises `BeadUnavailable` when `bd` is missing, times out, returns unparsable JSON, or fails with a message `MISS_MESSAGE` does not match; unwraps a single-element list payload; treats a payload carrying `error` as a miss. |
| 11 | `validate()`, line 495 | Compares `tracks_beads` and `closes_beads` through `metadata_ids()` rather than `set()` directly. |
| 12 | `main()`, line 588 | An unparsable command calls `advise()` and exits 0 instead of `deny()`. A `BeadUnavailable` raised out of `validate()` calls `advise()` and exits 0. |
| 13 | `__main__` block, line 633 | Wraps `main()` so any `BaseException` other than `SystemExit` exits 0. A payload whose `cwd` was not a string previously raised `TypeError` and exited 1. |

## What the lock records, and why it stays that way

`apm.lock.yaml` records the upstream package: `resolved_commit`, `version`, the
package `content_hash`, and a `deployed_file_hashes` entry per deployed file. The
recorded hash for the guard is the hash of the upstream file at `835c506`. It
records the bytes APM deployed, not the bytes on disk.

Do not rewrite those hashes to match the fork. Two independent reasons:

1. The mismatch protects the fork. `apm_cli/integration/cleanup.py:444-455`
   skips deleting a deployed file whose on-disk hash differs from its recorded
   hash; `:457-460` unlinks it when the hashes match. Recording the forked hash
   makes the fork deletable during prune and orphan cleanup.
2. A lock edit does not stick. `apm_cli/install/phases/lockfile.py:254-257`
   rehashes deployed files from disk on install, so any hand-written value is
   overwritten on the next `apm install`.

The lockfile format carries no fork, override, or patch field
(`apm_cli/deps/lockfile.py:131-199`, allowlist at `:370-395`). A custom key on
the dependency entry survives `apm lock` but not `apm install`. YAML comments do
not survive either, because the lock is written through a YAML dump. This
document is the record instead.

## Re-installing overwrites the fork

`apm install` treats the guard as an APM-managed file and rewrites it. Both
copies are tracked, so a clobber appears as a diff after any `apm install` or
`apm run install-agentic-tools`. Restore with `git checkout` on the two paths.
When the newly deployed script is a version this repo wants to keep, re-apply
the divergences above to it and copy the result to both paths.

`apm audit --ci` reports the fork under its drift check, which replays an
install of the pinned commit into a scratch tree and compares. That finding is
expected.
