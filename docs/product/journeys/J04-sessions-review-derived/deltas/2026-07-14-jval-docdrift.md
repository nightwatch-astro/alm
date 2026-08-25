> **MIGRATED:** current truth now lives at
> `docs/journeys/J04-sessions-review-derived/journey.md`.
> This delta is frozen legacy history: it records behavior as understood on
> its own date and may not match current behavior.
> See `docs/product/journeys/README.md`.

# jval-docdrift — J04-sessions-review-derived: PR #415 interaction-parity gap is closed; frame-type filter intentionally removed

Task: jval-docdrift · Status: observed-live-2026-07-14

Journey-validation run 2026-07-14 (real Windows app, build 7e522c16) — observed doc-drift vs baseline.

## Behavior delta (this journey)

Baseline's Known-gaps note says Inbox-level interaction parity (filter/camera dropdowns, group + secondary sort, `aria-sort`, "Grouped by X" footer hint) "requires PR #415 (open)." **PR #415 is MERGED** — all of these are live and working on the shipped Sessions list.

The shipped Sessions list **intentionally has no frame-type filter** (`SessionsPage.tsx:14`: sessions are light frames only; calibration lives on its own page/journey). This is a deliberate scope decision, not a missing control — reviewers should treat frame type as implicit here, not flag the absent filter as a gap.

## Stages hit

- Stage 1 "target/filter/camera filters, group + secondary sorts, every sortable header" (List chrome) — dropdowns/secondary-sort/`aria-sort`/footer hint are now live; frame-type filter is intentionally absent

## Reviewer verification

1. On Sessions, assert filter and camera dropdowns are present and functional.
2. Assert a secondary sort is available in addition to the primary group/sort control.
3. Inspect column headers for `aria-sort` on the active sortable column.
4. Assert a "Grouped by X" footer hint renders when grouping is active.
5. Negative: confirm there is no frame-type filter control, and that this is expected (cross-check `SessionsPage.tsx:14`), not a defect.

## Rerun set (minimal)

- Layer-1: to be written in the owning task
- Layer-2: to be written in the owning task
- Manual-Windows: `journey-04-*` (sessions) — re-walk with the live 2026-07-14 build
- Coverage-matrix: #6
