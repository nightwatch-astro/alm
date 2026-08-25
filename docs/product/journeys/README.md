# Frozen pre-migration journey history

> **MIGRATED:** current truth lives under `docs/journeys/`. Nothing in this
> directory describes current product behavior.

This tree is the pre-migration journey catalogue, frozen on 2026-07-15. It is
retained, not abandoned: the live journeys under `docs/journeys/` cite these
files by path in their `trace:` frontmatter as the provenance for their
baselines, and the per-task behavior deltas here exist nowhere else.

For current journey truth use `docs/journeys/`:

| For | Read |
|---|---|
| Per-journey routing table (generated) | `docs/journeys/INDEX.md` |
| Journey file format spec | `docs/journeys/FORMAT.md` |
| Per-project config and cross-cutting validator rules | `docs/journeys/README.md` |

## What is here

- `JNN-slug/journey.md` — 17 frozen baseline narratives (J01–J17). The live
  catalogue supersedes each of these and adds J18.
- `JNN-slug/deltas/*.md` — 58 per-task behavior deltas from the Wave 0
  campaign. These have no counterpart in the live tree.
- `INDEX.md`, `wave0-rerun-plan.md`, `wave0-task-index.md` — pre-format Wave 0
  rerun sheets.

Every file carries its own banner, because live journeys cite individual
`deltas/*.md` and `journey.md` paths rather than this directory.

## Editing

These files are a record of what was believed when they were written. Do not
update them to match current behavior — that would break their only purpose,
which is being the historical baseline the live journeys' `trace:` fields cite.
Known drift against today's code is expected and is not a defect here. Amend
the live journey under `docs/journeys/` instead.
