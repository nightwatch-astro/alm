> **MIGRATED:** current truth now lives at
> `docs/journeys/J05-project-lifecycle/journey.md`.
> This delta is frozen legacy history: it records behavior as understood on
> its own date and may not match current behavior.
> See `docs/product/journeys/README.md`.

# jval-docdrift — J05-project-lifecycle: Duplicate-name error fires at Create-time, not as-you-type

Task: jval-docdrift · Status: observed-live-2026-07-14

Journey-validation run 2026-07-14 (real Windows app, build 7e522c16) — observed doc-drift vs baseline.

## Behavior delta (this journey)

Baseline (Stage 1) reads as if the duplicate-name check validates while typing. The shipped app actually fires the case-insensitive duplicate-name check **at Create-time** (on submit) — the wizard bounces back to Step 1 with the inline field error at that point, not "immediately as you type."

## Stages hit

- Stage 1 "Typing a name that already exists (case-insensitively) surfaces an inline field error immediately, not a generic toast, and creation is blocked from that step"

## Reviewer verification

1. On Create (`/projects/new`), type an existing project name (any casing) and, without submitting, assert no inline error appears yet.
2. Click Create/submit — assert the wizard bounces back to Step 1 with the inline field error only at that point.
3. Confirm the check is case-insensitive (a mixed-case duplicate still blocks).

## Rerun set (minimal)

- Layer-1: to be written in the owning task
- Layer-2: to be written in the owning task
- Manual-Windows: `journey-05-*` (project lifecycle) — re-walk with the live 2026-07-14 build
- Coverage-matrix: #7
