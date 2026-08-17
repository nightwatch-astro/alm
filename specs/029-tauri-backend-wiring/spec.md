# Feature Specification: Tauri Backend Wiring

> **See Spec 030**: Implementation of this feature must follow
> [Spec 030 — UI Audit & Revision](../030-ui-audit-revision/spec.md)
> for UI patterns, lifecycle states, source folder types, and data model changes.

**Feature Branch**: `029-tauri-backend-wiring`

**Created**: 2026-05-25

**Status**: Implemented

**Input**: User description: "Wire the full Tauri desktop shell so the React frontend can run inside `tauri dev` with real (initially stub) command handlers instead of the mock layer."

**Depends On**: Spec 002 (Data Lifecycle State Model — merged), Spec 027 (Desktop Frontend Implementation — PR open)

**Enables**: Specs 003-026 (each spec replaces stubs with real implementations one command group at a time)

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Launch the App in Tauri (Priority: P1)

As a developer, I run `tauri dev` (or the equivalent workspace command) and the Astro Library Manager opens as a native desktop window with the full React frontend loaded inside it. The app navigates to the Sessions page, the sidebar renders with all navigation items, and the status bar shows app state. All pages are accessible and display stub data that matches the shape and feel of the current mock layer. No console errors from failed command invocations.

**Why this priority**: Until the app actually runs inside Tauri, no backend spec can be developed or tested. This is the prerequisite for all subsequent work.

**Independent Test**: Run `tauri dev`, wait for the window to open, navigate to every page (Sessions, Review, Calibration, Targets, Projects, Plans, Audit, Settings), and confirm each renders with data and no invoke errors in the webview console.

**Acceptance Scenarios**:

1. **Given** a clean checkout of the branch, **When** developer runs the Tauri dev command, **Then** the native window opens within 30 seconds and displays the Sessions page with stub session data.
2. **Given** the app is running in Tauri, **When** developer navigates to each of the 8 main pages (Sessions, Review, Calibration, Targets, Projects, Plans, Audit, Settings), **Then** each page renders without invoke errors and displays stub data.
3. **Given** the app is running in Tauri, **When** developer opens the webview developer console, **Then** there are zero errors from failed Tauri command invocations.
4. **Given** the app is running in Tauri, **When** developer navigates to the setup wizard (`/#/setup`), **Then** the 5-step wizard renders and each step is navigable.

---

### User Story 2 - Stub Command Surface Matches Frontend Expectations (Priority: P1)

As a developer working on a domain spec (e.g., spec 003 source setup), I can switch `VITE_USE_MOCKS` to `false` and the app runs against real Tauri command handlers that return realistic stub data. The command names, argument shapes, and response types match what the frontend expects. When I implement a real command handler for my spec, I replace the stub without changing the frontend API layer.

**Why this priority**: The stub surface is the contract between frontend and backend. If stubs don't match the frontend's expectations, every subsequent spec will fight type mismatches and naming conflicts.

**Independent Test**: Set `VITE_USE_MOCKS=false`, run `tauri dev`, navigate through all pages, and verify data renders correctly. Then replace one stub with a real implementation and confirm the frontend works without changes.

**Acceptance Scenarios**:

1. **Given** `VITE_USE_MOCKS=false`, **When** the app runs in Tauri, **Then** every command invocation from the frontend resolves successfully (no "command not found" or type mismatch errors).
2. **Given** the stub command surface, **When** a developer inspects the generated TypeScript bindings, **Then** every command has typed request/response signatures and `api/types.ts` has been deleted with all imports pointing at the generated bindings.
3. **Given** a stub command handler for `roots.register`, **When** a developer replaces it with a real implementation that reads/writes SQLite, **Then** the frontend calls the same command name with the same arguments and renders the response without code changes.

---

### User Story 3 - Persistent Database on First Launch (Priority: P1)

As a user launching the app for the first time, the application creates a persistent SQLite database in the platform-appropriate data directory (e.g., `~/.local/share/astro-library-manager/` on Linux, `~/Library/Application Support/` on macOS, `%APPDATA%` on Windows). Subsequent launches reuse the same database. A developer can override the database location via the `PV_DB_URL` environment variable for testing.

**Why this priority**: Without persistent storage, no backend spec can store or retrieve real data across app restarts.

**Independent Test**: Launch the app, verify the database file exists at the expected platform path, close and relaunch, verify data persists. Then set `PV_DB_URL` to a custom path and verify the override works.

**Acceptance Scenarios**:

1. **Given** a first launch with no existing database, **When** the app starts, **Then** a SQLite database file is created at the platform data directory with all migrations applied.
2. **Given** a database from a previous launch, **When** the app starts again, **Then** it reuses the existing database and applies any new migrations.
3. **Given** `PV_DB_URL=sqlite:///tmp/test.db`, **When** the app starts, **Then** it uses the specified path instead of the platform default.

---

### User Story 4 - Generated TypeScript Bindings Replace Hand-Written Types (Priority: P2)

As a developer, I run a binding generation command and it produces typed TypeScript bindings at `apps/desktop/src/bindings/index.ts` that cover all 30+ commands. The frontend `api/commands.ts` layer uses these generated types instead of the hand-written `api/types.ts`. Type safety is enforced end-to-end from Rust struct to TypeScript call site.

**Why this priority**: Type alignment between Rust and TypeScript prevents runtime shape mismatches. Doing this now prevents drift as domain specs add real implementations.

**Independent Test**: Run binding generation, verify the output covers all registered commands, replace one hand-written type import with the generated equivalent, and confirm `tsc --noEmit` passes.

**Acceptance Scenarios**:

1. **Given** all stub commands are registered in the Tauri specta builder, **When** binding generation runs (via `cargo test` in the desktop_shell crate), **Then** `apps/desktop/src/bindings/index.ts` contains typed wrappers for all 30+ commands.
2. **Given** the generated bindings, **When** `just typecheck` runs, **Then** there are zero type errors between the frontend call sites and the generated command signatures.
3. **Given** a developer adds a new Tauri command in Rust, **When** they regenerate bindings, **Then** the new command appears in the TypeScript bindings with correct types automatically.

---

### User Story 5 - Mock Layer Remains Available (Priority: P2)

As a frontend developer, I can still run `VITE_USE_MOCKS=true` with `just dev` (pure Vite, no Tauri) for fast iteration on UI changes. The mock layer continues to work exactly as before. The toggle between mock and real backends is seamless.

**Why this priority**: Frontend developers shouldn't need the full Rust toolchain just to iterate on UI. Preserving the mock path keeps the frontend development loop fast.

**Independent Test**: Run `just dev` with `VITE_USE_MOCKS=true`, navigate all pages, confirm mock data renders. Then switch to `tauri dev` with `VITE_USE_MOCKS=false` and confirm real stubs render.

**Acceptance Scenarios**:

1. **Given** `VITE_USE_MOCKS=true`, **When** developer runs `just dev` (Vite only), **Then** all pages render with mock data and no Tauri runtime is needed.
2. **Given** `VITE_USE_MOCKS=false`, **When** developer runs `tauri dev`, **Then** all pages render with stub data from real Tauri commands.
3. **Given** either mode, **When** developer navigates the full app, **Then** the UX is identical (same pages, same layout, same data shapes).

---

### Edge Cases

- What happens when a Tauri command is invoked but the stub is not yet registered? The app must show a user-friendly error, not a silent failure or crash.
- What happens when the database file path is not writable (e.g., permissions issue)? The app must surface a clear error on startup rather than silently falling back to in-memory.
- What happens when the frontend invokes a command with arguments that don't match the Rust handler's expected types? The generated bindings must catch this at compile time, not runtime.
- What happens on first launch when no database exists and the platform data directory doesn't exist yet? The app must create the directory tree.

## Requirements *(mandatory)*

### Functional Requirements

**Command Surface**

- **FR-001**: The Tauri backend MUST register a command handler for every command name the frontend invokes (30 commands as defined in `apps/desktop/src/api/commands.ts`).
  *(Reconciliation note, 2026-07-19, issue #764: `apps/desktop/src/api/
  commands.ts` no longer exists — spec 037's generated-bindings IPC
  migration deleted it; the frontend now calls `commands.*` from
  `apps/desktop/src/bindings/index.ts` directly.)*
- **FR-002**: Each stub command handler MUST return data matching the shape and types the frontend expects (as defined by the generated specta bindings).
- **FR-003**: Command naming MUST be aligned by using `#[specta(rename = "...")]` on Rust handlers to expose dotted names matching the frontend's existing invoke calls (e.g., `sessions.list`, `roots.register`). The frontend `commands.ts` is unchanged.
  *(Reconciliation note, 2026-07-19, issue #764: same drift as FR-001 —
  `commands.ts` is gone; the dotted names now come from tauri-specta's
  generated bindings, not a hand-written wrapper.)*
- **FR-004**: The existing 4 spec-002 lifecycle commands MUST continue to work unchanged.
- **FR-005**: Stub handlers MUST be clearly marked and trivially replaceable — each command group (sessions, calibration, targets, projects, plans, audit, settings, roots, scan, search, preferences, equipment, review, onboarding) MUST be in its own module. Each stub MUST emit a `tracing::debug!` log line with the command name on invocation, so developers can identify which commands are still stubs via `RUST_LOG=debug`. The tour capability shipped as `apps/desktop/src-tauri/src/commands/onboarding.rs` (spec 056), not the `commands/tour.rs` / `tour_complete_step` named during planning.

**Type Safety**

- **FR-006**: All Tauri commands MUST have Rust request/response types annotated with `specta::Type` and `serde::{Serialize,Deserialize}` for automatic TypeScript binding generation.
- **FR-007**: The binding generation test (`tests/bindings.rs`) MUST assert that all 30+ commands appear in the generated output.
- **FR-008**: The frontend MUST use the generated bindings from `apps/desktop/src/bindings/index.ts` as the authoritative type source. The hand-written `api/types.ts` MUST be deleted and all imports updated to point at the generated bindings.

**Database & Persistence**

- **FR-009**: On startup, the app MUST resolve a persistent on-disk SQLite database path using Tauri's platform data directory API.
- **FR-010**: The `PV_DB_URL` environment variable MUST override the default database path when set.
- **FR-011**: All existing migrations MUST apply successfully on first launch, creating the schema.
- **FR-012**: The app MUST create the database directory tree if it does not exist.

**AppState**

- **FR-013**: `AppState` MUST be expanded to hold service/repository references for all command groups, enabling domain specs to inject real implementations without restructuring.
  *(Reconciliation note, 2026-07-19, issue #764: the shipped `AppState`
  (`src-tauri/src/commands/lifecycle.rs`) stayed a thin pooled-handle
  struct — `repo: Arc<SqliteLifecycleRepository>`, `bus: EventBus`, plus the
  SIMBAD resolve-cache fields — not an expansion holding a
  service/repository reference per command group. Individual commands
  construct their own repositories from the shared pool/handles as needed,
  rather than AppState pre-holding one per domain.)*
- **FR-014**: `AppState` MUST be constructible with either real repositories (production) or stub/test fixtures.

**Developer Experience**

- **FR-015**: `tauri dev` MUST successfully launch the app with the frontend loaded inside a native window.
- **FR-016**: The mock layer (`VITE_USE_MOCKS=true`) MUST remain functional for pure frontend development without the Tauri runtime.
- **FR-017**: A workspace-level command (e.g., `just tauri-dev`) MUST be available to launch the Tauri dev environment.

### Key Entities

- **TauriCommand**: A registered IPC handler with a name, typed request, and typed response. Maps 1:1 to a frontend `invoke()` call.
- **AppState**: Shared state managed by Tauri, holding database pool, event bus, and per-domain service references.
- **StubHandler**: A temporary command implementation returning fixture data. Marked for replacement by domain specs.
- **Generated Binding**: A TypeScript function produced by tauri-specta that wraps `__TAURI_INVOKE` with correct types.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: `tauri dev` launches and displays the full UI within 30 seconds on a developer machine.
- **SC-002**: All 30+ frontend commands resolve successfully against Tauri stub handlers (zero invoke errors in console).
- **SC-003**: Generated TypeScript bindings cover 100% of registered commands with correct types.
- **SC-004**: `just typecheck` passes with the frontend using generated bindings.
- **SC-005**: `just lint` and `just test` pass with all new Rust command modules.
- **SC-006**: The SQLite database persists across app restarts at the platform-appropriate path.
- **SC-007**: The mock layer (`VITE_USE_MOCKS=true` + `just dev`) continues to work identically to pre-spec behavior.

## Clarifications

### Session 2026-05-25

- Q: How should command naming be aligned between frontend (dotted) and Tauri (snake_case)? → A: Rust commands use `#[specta(rename = "...")]` to expose dotted names — frontend `commands.ts` unchanged.
- Q: Should hand-written `api/types.ts` be deleted, re-exported, or kept alongside generated bindings? → A: Delete `api/types.ts` entirely, update all imports to point at generated `bindings/index.ts`.
- Q: Should stub commands log/trace when called for developer observability? → A: Stubs emit `tracing::debug!` on each call — visible in dev logs only.

## Assumptions

- The frontend command names and type shapes from spec 027 are the source of truth for the stub surface. If a command name or type needs to change, this spec updates the frontend to match.
- Stub data should be realistic but does not need to be dynamically generated or queryable — static fixture responses are sufficient.
- No real business logic is implemented in this spec. Stubs return hardcoded data. Domain specs (003-026) replace stubs with real implementations.
- The Tauri 2 + tauri-specta stack from spec 002 is the established pattern. This spec extends it, not replaces it.
- Cross-platform database path resolution uses Tauri's built-in path API (`app_data_dir`), not custom platform detection.
