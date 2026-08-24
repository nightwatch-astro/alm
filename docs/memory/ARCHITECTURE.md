# Architecture

Last reviewed: 2026-05-26 (crate paths re-derived 2026-08-24)

## System Overview

Tauri 2.x desktop app with a React frontend and Rust backend. The backend
is split into granular crates for domain, metadata, filesystem, lifecycle,
audit, and persistence. Language-neutral JSON Schema contracts define the
UI-to-core boundary; Tauri commands are the first adapter.

## Major Components

- **apps/desktop/src/** — React 18 + TanStack Router frontend, Mantine-free
  (custom CSS vars design system). Pages per spec 030: Inbox (was Review),
  Sessions, Calibration, Targets, Projects, Archive (new), Settings (with
  Audit Log). Plans removed from nav. Setup wizard is 4 steps.
- **apps/desktop/src-tauri/** — Tauri shell with specta-generated TypeScript
  bindings. Commands use dotted names (`roots.register`, `sessions.list`).
- **crates/domain/core/** — pure domain types and invariants.
- **crates/app/core/** — use-case orchestration (register source, complete
  first run, etc.). Thin layer over persistence + validation.
- **crates/persistence/** — SQLite via sqlx, split per bounded context
  (`core`, `calibration`, `inbox`, `lifecycle`, `plans`, `sessions`, `targets`,
  `topology`). `core` owns the pool, migrations, and shared write primitives;
  the rest own repositories. Repository
  pattern. Single `alm.db` at platform data dir.
- **crates/contracts/core/** — Rust DTOs matching JSON Schema contracts.
  All types derive `specta::Type` for binding generation.
- **crates/audit/** — event bus for audit trail. Topics like
  `first_run.completed`, lifecycle transitions.
- **packages/contracts/** — canonical JSON Schemas + generated TS types
  via json2ts. Allowlisted per-spec in `build-schemas.mjs`.

## Boundaries

- **Frontend ↔ Backend**: Tauri IPC via specta-generated typed commands.
  Result wrapper: `{ status: "ok", data: T } | { status: "error", error: E }`.
  DTOs use `#[serde(rename_all = "camelCase")]`.
- **Use case ↔ Repository**: async functions taking `&SqlitePool`. Errors
  are `ContractError` (domain) or `DbError` (persistence), mapped at the
  use-case layer.
- **Mock mode**: `VITE_USE_MOCKS=true` routes frontend commands through
  `api/mocks.ts` instead of Tauri IPC. Enables browser-only dev.

## Integrations

- **@tauri-apps/plugin-dialog** — native OS directory picker.
- **sqlx + SQLite** — local persistence. Migrations in
  `crates/persistence/core/migrations/`.
- **specta + tauri-specta** — TS binding generation from Rust types.
- **PixInsight/WBPP** — external; ALM prepares inputs but never calls it.

## Risks / Complexity Hotspots

- **Generated bindings drift**: specta regenerates `bindings/index.ts` on
  build. If the build doesn't run, TS types lag behind Rust changes.
- **JsonAny for untyped params**: `contracts_core::JsonAny` wraps
  `serde_json::Value` to avoid specta infinite recursion. Fragile if
  used for complex nested structures.
- **Cross-platform path handling**: no canonicalization yet. `/foo/bar`
  and `/foo/bar/` are treated as different paths. Windows case
  sensitivity not handled by SQLite UNIQUE constraint.
- **Mock/real divergence**: mock mode returns fixture data that may not
  match the real contract response shape over time.
