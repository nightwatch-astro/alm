> **MIGRATED:** current truth now lives at
> `docs/journeys/J06-cleanup-scan-review-apply/journey.md`.
> This delta is frozen legacy history: it records behavior as understood on
> its own date and may not match current behavior.
> See `docs/product/journeys/README.md`.

# jval-docdrift — J06-cleanup-scan-review-apply: PR #413 UI gap is stale; real blocker is the protection/audit backend

Task: jval-docdrift · Status: observed-live-2026-07-14

Journey-validation run 2026-07-14 (real Windows app, build 7e522c16) — observed doc-drift vs baseline.

## Behavior delta (this journey)

Baseline's Known-gaps note says the cleanup review UI "requires PR #413 (open) — pre-#413 the project detail's Cleanup section has no 'Scan for cleanup candidates' button at all." That note is **stale**: the scan/review/generate cleanup UI (Scan-for-candidates button, grouped-by-kind UI, destination picker, review overlay) is **fully implemented**. The real remaining blocker for this journey is the **protection/audit backend** — #780 (artifact-missing reconcile), #807 (cosmetic protected-ack), #766 (zero audit rows) — not missing UI.

## Stages hit

- Stage 1 "Scan for cleanup candidates runs a read-only preview … groups candidates by kind … totals the reclaimable size" — button and full scan UI are shipped, contradicting the Known-gaps "no button at all" note
- Stage 3 "its protection must be explicitly acknowledged before Approve & apply becomes clickable" — acknowledgement UI works, but the backing audit/protection reconcile has open defects (#780/#807/#766)

## Reviewer verification

1. Open a project's Cleanup section — assert "Scan for cleanup candidates" is present and functional (not absent).
2. Run a scan with protected items present — assert grouped-by-kind UI, destination picker, and review overlay all render.
3. Acknowledge a protected item and apply — cross-check audit rows against #766 (zero audit rows) and #780 (artifact-missing reconcile) to confirm whether the backend gap still reproduces.

## Rerun set (minimal)

- Layer-1: to be written in the owning task
- Layer-2: to be written in the owning task
- Manual-Windows: `journey-06-*` (cleanup) — re-walk with the live 2026-07-14 build
- Coverage-matrix: #17
