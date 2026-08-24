# Feature Specification: Bottom Log Viewer

> **UI Revised**: The UI design in this spec was revised by
> [Spec 030 — UI Audit & Revision](../030-ui-audit-revision/spec.md), which is
> superseded as of 2026-06-11. UI implementation of this feature must follow
> [Spec 032 — Design V4](../032-design-v4-implementation/spec.md), the current UI
> truth, for layout, navigation, and component patterns.

**Feature Branch**: `019-bottom-log-viewer`  
**Created**: 2026-05-09  
**Updated**: 2026-07-03  
**Status**: **Implemented** (closed 2026-07-03). Full-width bottom fold-out log panel (`apps/desktop/src/app/LogPanel.tsx`) with level chips, remembered follow-tail, entity cross-linking, 500-entry ring buffer, export, truncation marker, debug-gated diagnostics, and full i18n; backend `log_recent`/`log_export` commands + bus→Tauri forwarder. FR-001…FR-017 satisfied (desktop + Rust tests green). Closeout: added the T006/T011 jsdom unit tests, updated the T029 docs index, and **fixed a real bug** — the initial-hydration pull used a dotted `invoke('log.recent')` the real backend rejects; now routed through the generated `log_recent` binding. Remaining open: T028 Playwright quickstart (needs a Tauri runtime host, not WSL) — DEFERRED.  
**Input**: User description: "Specify the log viewer as a full-width bottom fold-out panel with log level and remembered follow behavior, not a focus rail."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Open Logs Without Losing Context (Priority: P1)

As a user, I want logs to open at the bottom of the app and consume application space so that I can inspect operation messages while keeping the current workspace visible.

**Why this priority**: Logs were explicitly moved out of the side/focus area and into a bottom fold-out.

**Independent Test**: Trigger an operation, open the log panel, and confirm it expands full-width across the bottom while resizing the workspace above it.

**Acceptance Scenarios**:

1. **Given** the log panel is closed, **When** the user opens it, **Then** it expands across the full bottom section.
2. **Given** the panel is open, **When** it expands, **Then** it takes vertical space from the app rather than floating over content.
3. **Given** the current page has selected details, **When** logs open, **Then** selection context remains unchanged.

---

### User Story 2 - Filter Log Noise (Priority: P2)

As a user, I want to choose a log level and optionally follow live logs so that I can troubleshoot without leaving the workflow.

**Why this priority**: Log controls should be useful but not expose irrelevant internal toggles.

**Independent Test**: Change the log level, toggle follow logs, close and reopen the panel, and confirm the follow preference is remembered.

**Acceptance Scenarios**:

1. **Given** logs include info, warning, and error events, **When** a log level is selected, **Then** the panel filters to that level and above.
2. **Given** follow logs is toggled, **When** the panel is closed and reopened, **Then** the chosen follow state is remembered.
3. **Given** a log event is shown, **When** metadata is inspected, **Then** request id and entity metadata are present without needing a setting.

---

### User Story 3 - Cross-Link Log Entries to Entities (Priority: P3)

As a user, I want a log row that references a plan, lifecycle transition, or inventory review to jump to the entity it describes so I can act on what the log surfaces.

**Independent Test**: Trigger a plan-apply error; from the log row, follow the cross-link to the plan detail page; confirm the entity, request id, and audit timeline match.

**Acceptance Scenarios**:

1. **Given** a log entry carries `entity_type` and `entity_id`, **When** the row is activated, **Then** the corresponding entity page opens.
2. **Given** a log entry carries only `request_id`, **When** the row is activated, **Then** the audit timeline opens filtered to that request id.

---

### User Story 4 - Bounded Retention and Export (Priority: P4)

As a user, I want the log to keep a bounded buffer in the UI and be exportable to a file so that I can hand a session log to support without exposing arbitrary format choices.

**Independent Test**: Generate more than the configured maximum entries; confirm the oldest are dropped; export the visible filter to JSON; confirm the file contains the displayed entries.

**Acceptance Scenarios**:

1. **Given** more than the maximum entries are produced, **When** the panel renders, **Then** the oldest entries are dropped first.
2. **Given** an export is requested with the current filter, **When** the export completes, **Then** the response includes a file path and an entry count.
3. **Given** the user has no permission to write the chosen path, **When** export is attempted, **Then** the operation returns `path.write.denied` and writes no file.

### Edge Cases

- Thousands of log events.
- Logs arrive while panel is closed.
- User filters to a level with no entries.
- Operation fails while panel is collapsed.
- Reduced-motion users open and close the panel.
- Export target path is read-only or missing parent directory.
- Subscriber resumes with a stale cursor after a long idle period.

### Domain Questions Resolved

- Maximum retained log lines in the UI: **500** (mockup-fixed ring buffer; see `research.md` R1).
- Whether logs are persisted across app restarts: **No in v1**; the in-memory ring is rebuilt per session. Durable audit history lives in `crates/audit/`; the log viewer is a session-scoped projection of recent audit events plus diagnostic events (see `research.md` R2).
- Severity bands shown: `all`, `info`, `warn`, `error`, `debug` (mockup-enforced).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Logs MUST be shown in a full-width bottom fold-out panel.
- **FR-002**: Opening logs MUST consume app layout space instead of overlaying important content.
- **FR-003**: The panel MUST include log level filtering covering `all`, `info`, `warn`, `error`, and `debug`.
- **FR-004**: Follow logs MUST be a panel control whose last state is remembered across panel open/close cycles within a session and across sessions.
- **FR-005**: Settings MUST NOT include "follow logs by default".
- **FR-006**: Request id and entity metadata MUST always be available in log entries.
- **FR-007**: Log export, if offered, MUST use JSON and MUST NOT expose export format as a user setting.
- **FR-008**: Logs MUST use functional labels and avoid "rail" terminology.
- **FR-009**: The UI buffer MUST be bounded; the v1 bound is 500 entries with oldest-first eviction.
- **FR-010**: A log entry MUST be cross-linkable when it carries `entity_type` and `entity_id`, falling back to `request_id` when only the request id is known.
- **FR-011**: The log subscription MUST be cursor-based so a reopened panel resumes after the last seen entry without replaying the full buffer.
- **FR-012**: Log entry `id` MUST use the prefixed format `aud:<n>` for audit-sourced entries and `dia:<n>` for diagnostic entries (A1).
- **FR-013**: The `source` field MUST be a closed enum aligned to spec 002 event-bus topic prefixes: `audit | diagnostic | catalog | plan | workflow | lifecycle | inventory | settings | project | target | tool` (R-SourceEnum).
- **FR-014**: When `logLevel != "debug"` (spec 018 setting), diagnostic entries MUST be hidden in the viewer and the diagnostics filter chip MUST be locked off (A3).
- **FR-015**: When the viewer's cursor predates retained history (vacuum gap), the stream response MUST include `truncated: true` and the UI MUST render an inline "History gap" marker at the top of the log list (A4).
- **FR-016**: `log.stream` requests MUST support an optional `source_filter: string[]` parameter to restrict entries to one or more source values (R-SourceFilter).
- **FR-017**: Each `LogEntry` MUST carry a `contract_version` field, currently `"2.0.0"` (H1). *(Reconciliation note, 2026-07-19, issue #764: originally specified as `"1"`; code and the JSON contract have agreed on `"2.0.0"` — `LOG_ENTRY_CONTRACT_VERSION` in `crates/contracts/core/src/log.rs` — since 2026-05-22. `data-model.md` and tasks.md T034 still say `"1"`; treat this spec.md line as authoritative.)*

### Key Entities

- **Log Entry**: Timestamped event with level, source, message, request id, optional entity metadata, and optional operation id.
- **Log Level Filter**: UI state controlling visible severity.
- **Follow State**: Remembered preference for auto-scrolling to new log events.
- **Operation Log Context**: Request/entity metadata linked to actions and lifecycle events.
- **Ring Buffer**: Bounded in-memory store with configured size and eviction policy.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Users can open logs from any main page without losing the selected item.
- **SC-002**: The panel can show representative operation failures without layout overlap.
- **SC-003**: Log level and follow behavior work with keyboard navigation.
- **SC-004**: Buffer eviction keeps the visible list bounded at the configured maximum under sustained log emission.

## Assumptions

- Logs are local and tied to audit/operation events.
- The bottom panel appears within the desktop app shell.

## Out of Scope

- Remote log streaming.
- Arbitrary export formats.
- Free-form full-text search inside the panel (covered by audit timeline in a separate feature).

## Implementation Status

The desktop mockup implements the user-visible behavior of US1 and US2 today.
Promotion to a backend-canonical log stream is plan work, not spec work.

### Mockup Files

> **Reconciliation note (2026-07-19, issue #764)**: both paths below moved —
> `apps/desktop/src/app/LogPanel.tsx` (not `ui/`) and
> `apps/desktop/src/data/logStore.ts` (not `store.ts`).
>
> **Reconciliation note (2026-07-20, issue #734)**: the collapsed-state idle
> preview line is GONE, not merely moved. `Shell.tsx` mounts `LogPanel` only
> while the panel is expanded, so the preview branch was unreachable in the
> shipped app, and `StatusBar` already owns the collapsed-state affordance. No
> FR required the preview — it was a mockup detail — so the dead branch was
> deleted rather than rewired.

- `apps/desktop/src/ui/LogPanel.tsx` - Bottom fold-out shell. Expand/collapse
  state, level filter chips (`all`/`info`/`warn`/`error`/`debug`), and
  per-entry level coloring.
- `apps/desktop/src/data/store.ts` - `useLog`, `appendLog`, `logPub`, and the
  seed log list. Maintains a 500-entry ring buffer via the publisher.
- Log emission sites in `apps/desktop/src/data/store.ts` cover plan create,
  plan apply progress, plan discard, lifecycle transitions, inventory review
  actions, and settings updates (non-noisy keys only).

### Behaviors Enforced By The Mockup

- Level filter is one of `all`, `info`, `warn`, `error`, `debug`. The filter
  defaults to `all` and is not persisted in v1.
- The buffer is a 500-entry ring; oldest entries are dropped first.
- Follow state persists across panel open/close within a session and is
  intended to persist across sessions once backed by the settings store
  (`rememberFollowLogs`).
- Each appended entry includes `id`, `time`, `level`, `source`, and `message`
  today; `request_id`, `entity_type`, and `entity_id` are the contract-level
  required fields once the backend stream lands (FR-006, FR-010).
- Settings exposes a log level control and a `rememberFollowLogs` preference;
  it does NOT expose a "follow logs by default" toggle (FR-005).

### Domain Questions Resolved (2026-05-22)

- **Level filter persistence (B-level-persistence)**: Session-only. The filter
  resets to `all` on each panel mount. Confirmed.
- **`debug` gating (A3)**: Diagnostic entries (and the debug filter chip) are
  hidden when `logLevel != "debug"` in spec 018 settings. When `logLevel == "debug"`,
  diagnostics are visible by default with a toggle in the log header.
- **`include_diagnostics` asymmetry (B-include_diagnostics-defaults)**: Stream
  default `true` (when debug mode), export default `false`. The asymmetry is
  intentional: exported files should contain only durable audit entries by default.

### US4 Acceptance Scenario (B2)

Updated acceptance scenario 2: "confirm the exported file contains only
audit-source entries (matching the `include_diagnostics=false` default);
diagnostic entries visible in the viewer are explicitly excluded unless the
'Include diagnostics' toggle is on."
