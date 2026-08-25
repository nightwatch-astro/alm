> **MIGRATED:** current truth now lives at
> `docs/journeys/J08-calibration-ingest-masters-matching/journey.md`.
> This delta is frozen legacy history: it records behavior as understood on
> its own date and may not match current behavior.
> See `docs/product/journeys/README.md`.

# jval-docdrift — J08-calibration-ingest-masters-matching: Master detection requires master-style filenames or IMAGETYP=master

Task: jval-docdrift · Status: observed-live-2026-07-14

Journey-validation run 2026-07-14 (real Windows app, build 7e522c16) — observed doc-drift vs baseline.

## Behavior delta (this journey)

Baseline (Stage 1) reads as if any calibration-root ingest of dark/flat/bias files yields individually tracked masters. The shipped spec-040 MasterDetector actually requires a **master-style filename** ("master" / "_stacked" in the filename) **or** `IMAGETYP` containing "master" — raw dark/flat fixtures without that naming/tag are **NOT** detected as masters; they ingest as ordinary calibration frames instead.

## Stages hit

- Stage 1 "Master calibration files ingest through the same Inbox pipeline as lights (Journey 2): a folder containing several master files … classifies as separate individual items"

## Reviewer verification

1. Ingest raw (non-stacked) dark/flat fixtures with ordinary filenames and no `IMAGETYP=master` — assert they do NOT appear as masters on the Calibration page.
2. Ingest the same fixtures renamed with "master"/"_stacked" in the filename, or with `IMAGETYP` containing "master" — assert they now appear as individual master rows.
3. Cross-check the detection rule against the spec-040 MasterDetector implementation.

## Rerun set (minimal)

- Layer-1: to be written in the owning task
- Layer-2: to be written in the owning task
- Manual-Windows: `journey-08-*` (calibration) — re-walk with the live 2026-07-14 build
- Coverage-matrix: #5
