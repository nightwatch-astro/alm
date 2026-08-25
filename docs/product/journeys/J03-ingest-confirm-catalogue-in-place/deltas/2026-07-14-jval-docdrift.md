> **MIGRATED:** current truth now lives at
> `docs/journeys/J03-ingest-confirm-catalogue-in-place/journey.md`.
> This delta is frozen legacy history: it records behavior as understood on
> its own date and may not match current behavior.
> See `docs/product/journeys/README.md`.

# jval-docdrift — J03-ingest-confirm-catalogue-in-place: Rescan-all-roots is inbox-only; catalogue plan's destructive-destination control observed absent

Task: jval-docdrift · Status: observed-live-2026-07-14

Journey-validation run 2026-07-14 (real Windows app, build 7e522c16) — observed doc-drift vs baseline.

## Behavior delta (this journey)

Baseline implies the Inbox's rescan mechanism surfaces new files from any registered root, including already-organized (catalogue-in-place) roots. The shipped app's Inbox page **"Rescan all roots" only rescans roots where `category==='inbox'`** (`InboxPage.tsx:161-167`) — surfacing a non-inbox (organized light-frames) root's new files instead requires **Settings → Data Sources → per-root Rescan**.

Baseline (Stage 3) states the catalogue plan's review overlay shows "the same destructive-destination control present (Archive vs System Trash) even though these actions don't need it." The live app's catalogue plan review was observed with that control **absent** — spec-compliant per the journey's own Touch & validate bullet ("destructive-destination control is absent or visibly inert for pure catalogue plans"), but the Stage-3 narrative text implying "present" is stale/misleading as written.

## Stages hit

- Stage 1 "Files under an organized root are ingested and classified exactly like Journey 2" — the rescan trigger for organized-root files is Data Sources' per-root Rescan, not Inbox's "Rescan all roots"
- Stage 3 "the same destructive-destination control present (Archive vs System Trash) even though these actions don't need it" — observed absent in the live app, not present

## Reviewer verification

1. Register an organized (non-inbox) light-frames root with new unscanned files; click Inbox's "Rescan all roots" — assert the new files do NOT surface.
2. From Settings → Data Sources, run per-root Rescan on that same root — assert the files now surface for classify/confirm.
3. Confirm a catalogue-in-place item; open the review overlay — assert the destructive-destination control is absent or inert, not an active present control.

## Rerun set (minimal)

- Layer-1: to be written in the owning task
- Layer-2: to be written in the owning task
- Manual-Windows: `journey-03-*` (catalogue in place) — re-walk with the live 2026-07-14 build
- Coverage-matrix: #3 (extend)
