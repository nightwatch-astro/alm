# What blocks a `gh pr create`

Two hooks gate agent-authored pull requests. Both are APM-deployed from
`srobroek/agentic-packages`; neither is forked in this repository.

| Hook | Refuses |
| --- | --- |
| `.claude/hooks/steering-git-workflow/scripts/pr-create-guard.py` | a `gh pr create` without `--draft` |
| `.claude/hooks/beads/scripts/pr-merge-bead-guard.py` | a branch whose merge bead cannot serve the merge queue |

## The merge bead is found by branch

`pr-merge-bead-guard.py` resolves the merge bead by matching
`metadata.branch` against the head branch, and refuses when it finds none.
`metadata.branch` must EQUAL the head branch: a bead carrying some other
branch, or a prefix of it, anchors nothing and the pull request is refused as
if the bead did not exist. Cross-fork `owner:branch` values are matched on the
branch alone.

The guard also refuses two beads anchoring one branch, a bead missing either
`pr:merge` or `agent:integrator`, a bead that is not open, a bead with an
assignee, and a bead missing `repo` or `origin_actor` metadata.

A repository holding no bead with a merge-queue label is left alone, so
tracking work in beads does not by itself demand a merge bead.

## PR-body trailers are not read

No hook reads the pull request body. `Merge-Bead:`, `Tracks-Bead:` and
`Closes-Bead:` trailers are not required, not parsed, and not compared against
`tracks_beads` / `closes_beads` metadata. A `blocks` edge from a work bead to
its merge bead is not checked at creation; the shepherd still uses such edges
to close work after landing.

Both hooks fail open. A missing `bd`, an unparsable command, or a lookup that
cannot complete allows the command with an advisory rather than denying it.
Bead lookups allow 30 seconds and retry once.

## History

Until PR #1706 this repository carried a fork of `pr-create-guard.py` that
also enforced the trailers, the `blocks` edge, and the metadata set-match.
Dropping the fork dropped those eight checks; the reduction was measured and
accepted rather than discovered. `docs/development/pr-create-guard-fork.md`
recorded the fork and was deleted with it.
