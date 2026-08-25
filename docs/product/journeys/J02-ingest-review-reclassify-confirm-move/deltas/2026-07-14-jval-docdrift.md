> **MIGRATED:** current truth now lives at
> `docs/journeys/J02-ingest-review-reclassify-confirm-move/journey.md`.
> This delta is frozen legacy history: it records behavior as understood on
> its own date and may not match current behavior.
> See `docs/product/journeys/README.md`.

# jval-docdrift — J02-ingest-review-reclassify-confirm-move: Destination-root picker surfaces inside the Review-plans overlay, not an inline Confirm modal

Task: jval-docdrift · Status: observed-live-2026-07-14

Journey-validation run 2026-07-14 (real Windows app, build 7e522c16) — observed doc-drift vs baseline.

## Behavior delta (this journey)

Baseline (Stage 4) reads as if the multi-root picker is an inline modal shown at Confirm time. The shipped app actually defers this: Confirm proceeds, the backend returns a typed `inbox.destination_root_required` signal, and the root picker surfaces inside the **"Review plans" overlay** (opened via a toast + the "Review plans (N)" button) — it is not an inline Confirm-time modal.

## Stages hit

- Stage 4 "If more than one destination library root is registered for that frame type, the user is forced to pick one via a root picker before a plan is generated"

## Reviewer verification

1. Register 2+ light-frame roots; confirm an inbox item without pre-selecting a destination root via the detail-header select.
2. Assert Confirm does not open an inline modal; instead a toast appears and the root choice surfaces inside the "Review plans (N)" overlay.
3. Assert the backend response carries `inbox.destination_root_required` before the picker renders.
4. Negative: with exactly one valid root, no picker appears anywhere (auto-picked, per baseline).

## Rerun set (minimal)

- Layer-1: to be written in the owning task
- Layer-2: to be written in the owning task
- Manual-Windows: `journey-02-*` (inbox confirm/move) — re-walk with the live 2026-07-14 build
- Coverage-matrix: #3
