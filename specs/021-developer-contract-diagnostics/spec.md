# Feature Specification: Developer Contract Diagnostics

> **See Spec 030**: Implementation of this feature must follow
> [Spec 030 — UI Audit & Revision](../030-ui-audit-revision/spec.md)
> for UI patterns, lifecycle states, source folder types, and data model changes.

**Feature Branch**: `021-developer-contract-diagnostics`  
**Created**: 2026-05-09  
**Status**: Implemented (dev-tools gated)  
**Input**: User description: "Specify API/contract diagnostics as developer-facing references, not normal user settings."

## Implementation Status

Implemented behind the workspace `dev-tools` feature (default off); release
binaries omit the surface.

- Route `/dev/contracts`: `apps/desktop/src/dev/ContractsPage.tsx`,
  `apps/desktop/src/app/router.tsx:231`.
- Commands: `dev_contracts_list`, `dev_calls_list`, `dev_export`,
  `dev_schema_get`, `dev_mode` (`apps/desktop/src-tauri/src/commands/dev.rs`).

The `ts_hash` / `rust_hash` / `mismatch` trio (T015) is inert: both hash fields
are hardcoded `None` at `apps/desktop/src-tauri/src/commands/dev.rs:406` and
`crates/app/core/src/dev_contracts.rs:189`, so no mismatch is ever computed.
Either drop the fields from `crates/contracts/core/src/dev.rs:42` or implement
the hashing described in `research.md` R9.
## User Scenarios & Testing *(mandatory)*

### User Story 1 - Inspect Contract References (Priority: P1)

As a developer or advanced tester, I want to inspect the operation contracts the
running app exposes — their names, versions, and JSON Schema paths — so the
frontend, Tauri, and core boundaries are debuggable without reading source.

**Why this priority**: Contract metadata is the entry point for every other
diagnostic. Without a list of contracts and versions, mismatch reports and call
inspection have nowhere to anchor.

**Independent Test**: Open the developer surface, see a list of every registered
contract, its semantic version, and a link to its JSON Schema on disk; confirm
the list is not visible from the normal Settings tree.

**Acceptance Scenarios**:

1. **Given** the developer surface is open, **When** the contract list loads,
   **Then** every UI-to-core contract registered at build time is shown with
   name and version.
2. **Given** the normal Settings tree is open, **When** sections are listed,
   **Then** there is no API Contracts entry.
3. **Given** a contract row is activated, **When** the JSON Schema is fetched,
   **Then** the schema body is shown verbatim.

### User Story 2 - Inspect Recent Contract Calls (Priority: P2)

As a developer debugging a failed user action, I want to see the last 100
contract calls with their request, response or error, start time, and duration
so I can reconstruct what the UI sent and what the core returned.

**Why this priority**: Without a recent-calls view, every reproduction requires
re-running with external tracing. The recent-calls buffer is the value-add over
the contract list alone.

**Independent Test**: Trigger five contract calls (one success, one validation
error, one not-found, one long-running, one cancelled); open the developer
surface; confirm all five appear in reverse-chronological order with the
correct request payload, response or error, started_at, and duration_ms.

**Acceptance Scenarios**:

1. **Given** recording is on, **When** a contract call completes, **Then** the
   call row appears in the recent-calls list within one second.
2. **Given** more than 100 calls have been made, **When** the list is shown,
   **Then** only the most recent 100 are retained; older calls are dropped.
3. **Given** a payload field is flagged sensitive, **When** the call is
   displayed, **Then** the field value is redacted but the field name and
   shape are preserved.

### User Story 3 - View JSON Schemas Inline (Priority: P3)

As a developer cross-checking a contract version against a payload, I want a
schema viewer that pretty-prints the JSON Schema for a contract and lets me
copy it to the clipboard.

**Why this priority**: Schema viewing closes the loop between a recorded call
and its declared shape. It is lower priority than the call list itself because
the file path from US1 already lets a developer open the schema externally.

**Independent Test**: From the contract list, activate "view schema"; the
schema renders pretty-printed; the copy action puts valid JSON Schema text on
the clipboard.

**Acceptance Scenarios**:

1. **Given** a contract row, **When** "view schema" is activated, **Then** the
   schema is shown with two-space indentation and syntax highlighting.
2. **Given** a recorded call, **When** "view schema for this contract" is
   activated, **Then** the schema for the contract version recorded on the call
   row is shown, not necessarily the current version.

### User Story 4 - Hidden By Default and Performance-Safe (Priority: P4)

As a product owner, I want the developer surface hidden from normal navigation
and the call-recording proxy disabled by default so end users never encounter
it and never pay its performance cost.

**Why this priority**: Discoverability and performance are non-functional
guarantees that protect the rest of the product. They are P4 because they
constrain the other stories rather than delivering new behavior.

**Independent Test**: In a production build (compiled WITHOUT the `dev-tools`
Cargo feature) with developer mode off, search the top-level navigation,
Settings, and command palette; the developer surface is not reachable and
the route + proxy are not compiled in. Enable developer mode in a `dev-tools`
build; restart the app; the command-palette entry appears and the route resolves.

**Acceptance Scenarios**:

1. **Given** developer mode is off, **When** the user opens the command
   palette, **Then** no developer-diagnostics entry is shown.
2. **Given** developer mode is off, **When** the user enters the route URL
   directly, **Then** the route shows a "developer mode disabled" stub and
   does not subscribe to the call stream.
3. **Given** developer mode is on, **When** the user opens the command
   palette and types "contracts", **Then** the developer surface entry
   appears and opens at `/dev/contracts`.
4. **Given** developer mode toggle is set to off, **When** the app is
   restarted, **Then** the recording proxy is fully uninstalled and adds no
   overhead.
5. **Given** developer mode toggle is set to off but the app has not been
   restarted, **When** a contract call runs, **Then** recording continues
   until the next restart. (Informational; not a fail condition — full
   uninstall requires a restart.)

### Edge Cases

- A contract version registered in TypeScript does not match the version
  registered in Rust at startup.
- A schema file referenced by `ContractMeta.schema_path` is missing on disk.
- A contract call exceeds the largest payload size the recorder will retain.
- A request contains a path or secret that must be redacted before storage.
- The developer surface is opened in a packaged production build where
  `devMode` cannot be enabled at runtime.
- Replaying the last call mutates state that has since changed.

### Domain Questions — RESOLVED

- **Whether developer mode is a build flag, a runtime toggle, or both.**
  RESOLVED (A-021-2, R-DevFeature): Both. Compile-time `dev-tools` Cargo
  feature gates the surface in release vs developer builds. Runtime toggle
  `devMode` (Settings store, boolean, default false) controls proxy
  installation within a `dev-tools` build. Toggling off requires restart for
  full proxy uninstall.

- **Which payload fields are considered sensitive by default.**
  RESOLVED (A-021-3, R4): Any field named `password`, `token`, `secret`, or
  `api_key` at any depth. Additionally, ALL filesystem paths are redacted by
  default (replaced with `${LIBRARY_ROOT}/...`-style placeholder). Per-export
  opt-in toggle `includeVerbatimPaths: boolean` (default false) restores
  verbatim paths in the diagnostic export.

- **Whether replay is allowed for write contracts or only read contracts in v1.**
  RESOLVED (A-021-4, R6): `replay_safe` defaults to `false`; contracts must
  opt in to `replay_safe: true` explicitly. Write contracts must NOT set
  `replay_safe: true` unless present in an explicit allow-list. A CI lint
  snapshot test enforces this (see tasks T037).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: API contract references MUST NOT appear as a normal Settings
  section or in the default top-level navigation.
- **FR-002**: The developer surface MUST list each registered contract with
  name, semantic version, and the absolute path of its JSON Schema.
- **FR-003**: The developer surface MUST list the most recent 100 contract
  calls with request payload, response or error, start time, and duration.
- **FR-004**: A schema viewer MUST be available from each contract row.
- **FR-005**: A "replay last call" action MUST be available on each call row;
  v1 MUST restrict replay to contracts marked `replay_safe = true`.
- **FR-006**: Contract mismatch warnings between TypeScript and Rust
  registries MUST be visible on the contract list.
- **FR-007**: JSON MUST be the only diagnostic export format unless a later
  spec adds another.
- **FR-008**: The recording proxy MUST be disabled when developer mode is off.
  Full uninstall of the proxy (zero overhead) takes effect at the NEXT app
  restart after toggling off. Toggling off without restart leaves the proxy
  running until restart (see US4 acceptance scenarios 4 and 5).
  In production builds compiled without the `dev-tools` Cargo feature, the
  proxy, route, and all `dev.*` Tauri commands are omitted entirely from the
  binary; the toggle has no effect in such builds.
- **FR-009**: Sensitive fields declared per contract MUST be redacted before
  the call payload is stored.
- **FR-010**: The developer route MUST be reachable only through the command
  palette (Cmd+K / Ctrl+K) when developer mode is on.

### Key Entities

- **Contract Reference**: Schema or generated type boundary between UI, Tauri,
  and core.
- **Contract Call**: A single request/response pair captured by the recording
  proxy.
- **Diagnostic Export**: JSON snapshot of contract metadata and the recent
  calls buffer.
- **Contract Mismatch**: Version or schema disagreement detected at app
  startup.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Normal users do not encounter API Contract settings.
- **SC-002**: A developer can identify the contract version and last payload
  for any failed call within 30 seconds of reproducing it.
- **SC-003**: Diagnostic export uses JSON consistently.
- **SC-004**: With developer mode off, the contract dispatch path is
  byte-for-byte identical to the no-recording baseline (no proxy frame in
  flame charts).

## Compile-Time Gating (R-DevFeature)

Release binaries are compiled WITHOUT the `dev-tools` Cargo feature. This
causes the following to be omitted from the binary entirely:

- The `/dev/contracts` route and its components.
- The recording proxy (`recorder.ts`) — not loaded, not bundled.
- The `dev.contracts.list` and `dev.calls.list` Tauri commands.

In `dev-tools` builds, the `devMode` runtime toggle in the Settings store
controls whether the proxy is installed at app boot. Toggling off requires a
restart for full proxy uninstall (FR-008).

Note: `tauri.conf.json` and `Cargo.toml` edits for the `dev-tools` feature
flag are deferred to the Rust implementation phase and MUST NOT be applied in
the spec session.

## Assumptions

- JSON Schema contracts remain the language-neutral transport boundary.
- Developer mode is opt-in and persisted per device, not per library.
- The command palette (Cmd+K) is the only navigation entry point in v1.
- The `dev-tools` Cargo feature is absent in release builds; present in
  developer/CI builds.

## Out of Scope

- User-facing API configuration.
- Multiple export formats.
- Cross-session call history (calls are session-only).
- Remote contract diagnostics across devices.
