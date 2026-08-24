> **MIGRATED:** current truth now lives at
> `docs/journeys/J07-archive-delete/journey.md`.
> This delta is frozen legacy history: it records behavior as understood on
> its own date and may not match current behavior.
> See `docs/product/journeys/README.md`.

# jval-docdrift — J07-archive-delete: Archive button auto-generates the plan in one click; Known-gap #1 is false

Task: jval-docdrift · Status: observed-live-2026-07-14

Journey-validation run 2026-07-14 (real Windows app, build 7e522c16) — observed doc-drift vs baseline.

## Behavior delta (this journey)

Baseline's Known-gaps bullet #1 says "There is no shipped UI button that generates an archive plan yet … only reachable by invoking the backend command directly." This is **FALSE** in the shipped app: the project-detail **Archive button refuses server-side, then auto-generates the plan and opens the review overlay in one click** (`ProjectDetail.tsx:250-327`, `handleGenerateArchivePlan`) — no backend-only IPC call is needed. The back-half (apply/lifecycle-flip/DELETE-gate) remains untestable on current data because #780 zeroes the plan items — that is a separate, still-open gap.

## Stages hit

- Stage 1 "Clicking 'Archive' on a completed project is refused unless a filesystem plan for the archive already exists and has been applied" — the refusal now auto-triggers plan generation and opens the review overlay in the same click, not a dead-end refusal requiring backend-only IPC

## Reviewer verification

1. On a `completed` project with no existing archive plan, click "Archive."
2. Assert the server-side refusal fires, then the plan is auto-generated and the review overlay opens in the same interaction — not two separate manual steps and not a backend-only path.
3. Note (do not treat as a new finding): the back-half apply/DELETE-gate flow may show empty plan items due to #780 — cross-check before flagging as regression.

## Rerun set (minimal)

- Layer-1: to be written in the owning task
- Layer-2: to be written in the owning task
- Manual-Windows: `journey-07-*` (archive) — re-walk with the live 2026-07-14 build
- Coverage-matrix: #17
