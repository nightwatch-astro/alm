> **MIGRATED:** current truth now lives at
> `docs/journeys/J01-first-run-setup-data-sources/journey.md`.
> This delta is frozen legacy history: it records behavior as understood on
> its own date and may not match current behavior.
> See `docs/product/journeys/README.md`.

# jval-docdrift — J01-first-run-setup-data-sources: Wizard is 6 steps with a new Observing Site step; Project outputs is required

Task: jval-docdrift · Status: observed-live-2026-07-14

Journey-validation run 2026-07-14 (real Windows app, build 7e522c16) — observed doc-drift vs baseline.

## Behavior delta (this journey)

Baseline says the setup wizard is 5 steps. The shipped app actually ships a **6-step wizard**: a new **"Observing Site" step** (map picker + Name / Lat / Lon / Elevation / Timezone / Night-definition / Horizon; `apps/desktop/src/features/setup/steps/StepSite.tsx`) is inserted after Configuration, shipped via PR #686/#691.

Baseline says Step 1's four folder categories are "Light frames (required), Calibration, Project outputs, and Inbox (all optional)." The shipped app actually treats **"Project outputs" as a REQUIRED category** alongside Light frames — `REQUIRED_KINDS = ['light_frames','project']` (`apps/desktop/src/features/setup/sources-store.ts:32`).

## Stages hit

- Stage 1 "the app opens the setup wizard ("Setup · Step 1 of 5")" — wizard is now 6 steps (new Observing Site step inserted after Configuration/Stage 3)
- Stage 2 "Light frames (required), Calibration, Project outputs, and Inbox (all optional)" — Project outputs is now required, not optional

## Reviewer verification

1. Launch first-run setup on a fresh DB; count wizard steps in the stepper chrome — expect 6, with "Observing Site" appearing after Configuration and before Confirm.
2. Confirm `apps/desktop/src/features/setup/steps/StepSite.tsx` exists and is wired into the step sequence (map picker + Name / Lat / Lon / Elevation / Timezone / Night-definition / Horizon fields).
3. On Step 1 (Source Folders), try to reach Confirm/Finish without adding a Project-outputs folder — assert it is now blocked/flagged the same way Light frames is; cross-check `REQUIRED_KINDS` at `apps/desktop/src/features/setup/sources-store.ts:32`.
4. Negative: Calibration and Inbox remain optional (no such gate on those categories).

## Rerun set (minimal)

- Layer-1: to be written in the owning task
- Layer-2: to be written in the owning task
- Manual-Windows: `journey-01-*` (first-run setup) — re-walk with the live 2026-07-14 build
- Coverage-matrix: #1
