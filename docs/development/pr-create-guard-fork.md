# The pr-create-guard is a local fork

`.claude/hooks/steering-git-workflow/scripts/pr-create-guard.py` and its
`.codex` copy are a fork of the `steering-git-workflow` package script. Both
files are tracked in git. `apm.lock.yaml` pins the package at
`srobroek/agentic-packages` commit `835c506a875d39d8bd736175fbcb407877c3e585`,
version 2.3.2.

## Why the fork exists

Upstream `main` removed bead-linkage validation in PR #856. The guard there now
checks only that `gh pr create` passes `--draft`. This repo depends on the
linkage check: it blocks a PR whose `Merge-Bead` trailer names a bead that is
missing, closed, unlabeled, or missing `branch` / `repo` / `origin_actor`
metadata.

The pinned commit `835c506` predates PR #856, so it still carries the linkage
check. The fork is that pinned script plus four local changes.

## What the fork changes against the pin

| Change | Effect |
| --- | --- |
| `advise()` | An inconclusive check allows the command and reports what went unverified, instead of denying. |
| `effective_cwd()` | Resolves a leading `cd -- <path> &&` so the bead lookup runs against the right `.beads` directory. |
| `metadata_ids()` and `MISS_MESSAGE` | Accepts the list shape `bd` writes for metadata id fields, and separates a genuine miss from a failed lookup. |
| `BeadUnavailable` with one retry, `timeout=30` | A stalled `bd` call no longer reads as "that bead does not exist". Upstream uses a single attempt at `timeout=5`. |

## What the lock records, and why it stays that way

`apm.lock.yaml` records the upstream package: `resolved_commit`, `version`, the
package `content_hash`, and a `deployed_file_hashes` entry per deployed file. The
recorded hash for the guard, `sha256:cde890f7...`, is the hash of the upstream
file at `835c506`. It is a record of the bytes APM deployed, not a claim about
the bytes on disk.

Leave those hashes alone. APM treats a deployed file whose on-disk hash matches
its recorded hash as APM-owned and unmodified, and therefore safe to delete
during prune and orphan cleanup. The mismatch is what marks the fork as
user-edited and blocks that deletion. Recording the fork's hash makes the fork
deletable.

The lockfile format has no fork, override, or patch field. A custom key on the
dependency entry survives `apm lock` but not `apm install`, which rebuilds each
entry and carries over only `deployed_files` and `deployed_file_hashes`. YAML
comments do not survive either, because the lock is written through a YAML dump.
This document is the record instead.

## Re-installing overwrites the fork

`apm install` treats the guard as an APM-managed file and rewrites it. Both
copies are tracked, so a clobber appears as a diff after any `apm install` or
`apm run install-agentic-tools`. Restore with `git checkout` on the two paths,
or re-apply the four changes above against the newly deployed script.

`apm audit --ci` reports the fork under its drift check, which replays an
install of the pinned commit into a scratch tree and compares. That finding is
expected.
