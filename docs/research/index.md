# Research Index

Index of durable research notes and per-feature research decisions. Topic notes
live in this directory; feature-scoped research lives under
`specs/NNN-feature-name/research.md`.

## Topic research notes

- [first-run-source-setup.md](./first-run-source-setup.md) — first-run source registration.
- [imagetyp-normalization.md](./imagetyp-normalization.md) — IMAGETYP → frame-type normalization.
- [implementation-dependencies.md](./implementation-dependencies.md) — cross-feature implementation dependencies.
- [lifecycle-state-model.md](./lifecycle-state-model.md) — data lifecycle state model.
- [044-frontend-astronomy-libraries.md](./044-frontend-astronomy-libraries.md) — astronomy/charting library selection for the planner (astronomy-engine, visx, react-table, moon filter model, FITS/XISF crate split); handover for spec 044 + orchestrator.
- [007-025-048-perf-gate-measurements.md](./007-025-048-perf-gate-measurements.md) — measured perf-gate results for calibration suggest (spec 007 T033), plan-apply progress (spec 025 T045), and 10k-frame reconcile (spec 048 SC-005).

## Feature research decisions

Each active feature records its research in its spec folder. Notable:

- Spec 017 — Cleanup And Archive Review Plans: [`specs/017-cleanup-archive-review-plans/research.md`](../../specs/017-cleanup-archive-review-plans/research.md) (plan review UX, state machine, archive destination convention, retry model).
- Spec 018 — Settings Configuration Model: [`specs/018-settings-configuration-model/research.md`](../../specs/018-settings-configuration-model/research.md) (persistence shape, audit policy, override resolution, schema versioning).
- Spec 021 — Developer Contract Diagnostics: [`specs/021-developer-contract-diagnostics/research.md`](../../specs/021-developer-contract-diagnostics/research.md) (recording proxy, `dev-tools` compile-time feature gate, replay safety, redaction).

For the full set, see `specs/*/research.md`.

## Developer-mode entry point (spec 021)

Dev-tools builds (Cargo feature `dev-tools` / `VITE_DEV_TOOLS=true`) register a
hidden settings page at `/dev/settings` that toggles the `devMode` setting. It
is deliberately absent from the command palette and from Settings › Advanced
navigation — type the URL directly. Turning `devMode` on makes the recording
proxy capture contract calls and exposes Developer / Contracts
(`/dev/contracts`) via the command palette. Release builds omit the `dev-tools`
feature, so neither route exists at runtime.
