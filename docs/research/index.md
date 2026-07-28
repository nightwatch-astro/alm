# Research Index

Index of durable research notes and per-feature research decisions. Topic notes
live in this directory; feature-scoped research lives under
`specs/NNN-feature-name/research.md`.

## Topic research notes

- [first-run-source-setup.md](./first-run-source-setup.md) — first-run source registration.
- [imagetyp-normalization.md](./imagetyp-normalization.md) — IMAGETYP → frame-type normalization.
- [implementation-dependencies.md](./implementation-dependencies.md) — cross-feature implementation dependencies.
- [lifecycle-state-model.md](./lifecycle-state-model.md) — data lifecycle state model.
- [025-plan-apply-decision-map.md](./025-plan-apply-decision-map.md): spec 025 research decisions (R1 through R8, R-FS-1, R-Pause-1, R-CAS-1) mapped to the executor and `app/core` modules that enforce them, including where the approval-token implementation diverges from R8.
- [044-frontend-astronomy-libraries.md](./044-frontend-astronomy-libraries.md) — astronomy/charting library selection for the planner (astronomy-engine, visx, react-table, moon filter model, FITS/XISF crate split); handover for spec 044 + orchestrator.

## Feature research decisions

Each active feature records its research in its spec folder. Notable:

- Spec 017 — Cleanup And Archive Review Plans: [`specs/017-cleanup-archive-review-plans/research.md`](../../specs/017-cleanup-archive-review-plans/research.md) (plan review UX, state machine, archive destination convention, retry model).
- Spec 018 — Settings Configuration Model: [`specs/018-settings-configuration-model/research.md`](../../specs/018-settings-configuration-model/research.md) (persistence shape, audit policy, override resolution, schema versioning).
- Spec 019, Bottom Log Viewer: [`specs/019-bottom-log-viewer/research.md`](../../specs/019-bottom-log-viewer/research.md) (500-entry ring buffer, diagnostic vs workflow-significant emission paths, follow-tail scroll handoff, retention, cursor semantics).
- Spec 020, Router And URL State: [`specs/020-router-url-state/research.md`](../../specs/020-router-url-state/research.md) (hash history for the Tauri origin, URL as the source of truth for filters and selection, per-route `validateSearch` tiers, deprecated-param migration).
- Spec 021 — Developer Contract Diagnostics: [`specs/021-developer-contract-diagnostics/research.md`](../../specs/021-developer-contract-diagnostics/research.md) (recording proxy, `dev-tools` compile-time feature gate, replay safety, redaction).
- Spec 025, Filesystem Plan Application: [`specs/025-filesystem-plan-application/research.md`](../../specs/025-filesystem-plan-application/research.md) (cross-platform move, archive vs trash, failure taxonomy, cancellation, pause/resume, concurrency). Decision-to-module map: [025-plan-apply-decision-map.md](./025-plan-apply-decision-map.md).

For the full set, see `specs/*/research.md`.

## Router history strategy (spec 020)

`apps/desktop/src/app/router.tsx` builds the router on `createHashHistory()`.
Every URL is `index.html#/route?search=…`, so a reload always fetches
`index.html` and the route is parsed in JS. `createBrowserHistory` needs the
host to serve arbitrary sub-paths, which the Tauri `file://` and `tauri://`
origins do not; `createMemoryHistory` loses the URL on reload and breaks
back/forward restore. A future web adapter swaps the history factory behind a
build flag and leaves the route tree unchanged.

URL state on the desktop buys in-session back/forward that restores filters and
selection, multi-window views that each carry their own route, and tests that
assert a view from a URL string. Address-bar sharing and bookmarking do not
apply, because the shell has no address bar. Spec 020 was narrowed to those
payoffs on 2026-06-10; the decision record is
[`docs/development/autonomous-run-2026-06-decisions.md`](../development/autonomous-run-2026-06-decisions.md)
(DV-006, D-007).

## Developer-mode entry point (spec 021)

Dev-tools builds (Cargo feature `dev-tools` / `VITE_DEV_TOOLS=true`) register a
hidden settings page at `/dev/settings` that toggles the `devMode` setting. It
is deliberately absent from the command palette and from Settings › Advanced
navigation — type the URL directly. Turning `devMode` on makes the recording
proxy capture contract calls and exposes Developer / Contracts
(`/dev/contracts`) via the command palette. Release builds omit the `dev-tools`
feature, so neither route exists at runtime.
