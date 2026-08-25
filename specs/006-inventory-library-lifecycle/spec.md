# Feature Specification: Inventory Lifecycle

> **AMENDED (2026-06-23) by [Spec 041 — Inbox Plan Surface](../041-inbox-plan-surface/spec.md), iteration "single-type inbox sub-items".**
> The session **review lifecycle** described here (six-state `SessionState`:
> discovered/candidate/needs_review/confirmed/rejected; the US2 "review & confirm before project use"
> gate) is **reduced**: sessions become derived/already-confirmed once per-file metadata is fixed at
> Inbox confirm. The shared `SessionState` enum is collapsed and the US2 AC2 project gate becomes
> auto-pass. Metadata correctness moves to the Inbox missing-mandatory gate (041 FR-047–FR-051). The
> enum change + existing-session-row migration are tracked as 041 tasks **T076/T077**. Treat US2's
> review-state requirements as superseded by 041.

> **AMENDED (2026-07-03) — reconciliation to 041 inbox split + 043 redesign + 040 Calibration.**
> The `mixed` session/frame-type concept is **removed** (041 splits mixed folders
> into single-type items at Inbox ingest, so a session can never be "mixed"). The
> Inventory **frame-type filter is dropped** (FR-002): the Sessions/Inventory view
> is lights-centric; dark/flat/bias filtering lives on the Calibration page (spec
> 040). **FR-007** (per-row Reveal in OS) and **FR-010** (Ignore action + Cmd+K
> "Show ignored") are implemented against the 043-redesigned **Sessions** surface;
> the FR-010 route is `/sessions?reviewFilter=ignored`. See `pending-iteration.md`.

> **AMENDED (2026-07-14) by the Q27 framing-layer iteration on
> [Spec 008 — Project Create, Onboard, And Edit](../008-project-create-onboard-edit/spec.md).**
> Spec 008 introduces a **framing layer** (`project → framing → session →
> frames`). Cross-spec delta for this spec: (1) a light **session may be a
> member of at most one framing** (a project sub-structure; membership is owned
> by spec 008, referenced by session id — no new field on the InventorySession
> projection is required); (2) the framing **clustering** groups a project's
> light sessions by target + optic-train + pointing + rotation within a tunable
> tolerance, reading session-level geometry persisted at confirm (spec-008
> F-Framing-1; the Q12 strict-gate iterate — not yet applied — will guarantee
> presence on new ingests, and NULL-geometry legacy sessions are excluded until
> a Q28 rescan backfill); (3) the Q27 **incremental ingestion-attribution**
> pass is the **first** pre-ingest pass at the Inbox confirm gate — the Q22
> duplicate-detection sweep joins the same pass when its iterate lands —
> producing ranked, user-picked suggestions (never an auto-merge), with the
> pick persisted via the confirm-request extension (spec-008 FR-022).
> No InventorySession state or projection shape changes. See the Iterations log.

> **See Spec 032**: UI implementation of this feature must follow
> [Spec 032 — Design V4](../032-design-v4-implementation/spec.md), the current UI
> truth, for layout, navigation, and component patterns. Spec 030 (UI Audit &
> Revision), cited here previously, is superseded as of 2026-06-11.

**Feature Branch**: `006-inventory-library-lifecycle`  
**Created**: 2026-05-09  
**Updated**: 2026-07-03  
**Status**: **Implemented** (closed 2026-07-03). Core lifecycle shipped and the 041/043/040 reconciliation iteration is fully applied — `mixed` removed, FR-002 filter dropped, FR-007 (Reveal-in-OS) + FR-010 (Ignore + Cmd+K) landed (all reconciliation tasks T430/T410/T411/T420/T421/T309/T403 done). The remaining open tasks are all **DEFERRED**, not unstarted: Playwright-in-WSL smoke tests (T107/T205/T306/T307/T310/T405), provenance follow-ups (T203/T204), missing/reconnect-source warnings (T402/T404, additive-contract, out of v1), diagram doc (T500), and the spec-002-blocked CI enum snapshot (T506). Obsolete: T308, T311. See `SPEC_STATUS.md`.  
**Input**: User description: "Specify the Inventory lifecycle, replacing Library tags/handling ambiguity with clear frame types, review state, source details, and consistent actions."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Move Reviewed Inbox Items To Inventory (Priority: P1)

As a user, I want Inbox items to become Inventory items through a clear move action so that accepted data is available for calibration and project workflows.

**Why this priority**: Inventory is the stable working library. It must not inherit ambiguous Inbox state.

**Independent Test**: Select a dark, bias, flat, and light from Inbox, move each to Inventory, and confirm they appear with frame type, source, session, review state, and actions.

**Acceptance Scenarios**:

1. **Given** an Inbox item has a single frame type, **When** the user moves it to Inventory, **Then** the Inventory item records frame type, source, session, and lifecycle event.
2. **Given** a folder held mixed frame types, **When** the user reaches Inventory, **Then** the items arrive already split into single-type entries by the Inbox ingest gate (spec 041); Inventory never receives a "mixed" item.
3. **Given** an Inventory item is selected, **When** the detail pane opens, **Then** it shows selected item data only, in structured rows.

---

### User Story 2 - Confirm Inventory Metadata (Priority: P2)

As a user, I want to review and confirm Inventory metadata before using it in projects so that calibration and light matching decisions are explicit.

**Why this priority**: Project creation depends on reviewed source and calibration information.

**Independent Test**: Open an Inventory item, review its details, confirm it, and verify its review state changes without creating a badge-style bubble or a separate Handling field.

**Acceptance Scenarios**:

1. **Given** an Inventory item has inferred frame type or session data, **When** the user confirms it, **Then** the item records a reviewed decision.
2. **Given** an Inventory item is not confirmed, **When** it is offered for project selection, **Then** the UI indicates that review is still needed.
3. **Given** a user corrects metadata, **When** the correction is saved, **Then** the corrected value becomes the reviewed value and the inferred value remains traceable.

### Edge Cases

- Duplicate physical files discovered through different sources.
- Folder that held mixed lights and calibration frames (already resolved into single-type items at Inbox ingest per spec 041 — never surfaces as a "mixed" Inventory item).
- Inventory item source root is missing or moved.
- Metadata is incomplete, contradictory, or unavailable.
- User needs to open the item location in the native file browser.

### Domain Questions To Resolve

- Which Inventory review fields are mandatory before a project can reference an item?
- Which stale source conditions block project use versus only warn?

## Requirements *(mandatory)*

### Functional Requirements

> **Reconciliation note (2026-07-19, issue #764)**: three drifts confirmed
> against the current tree. **FR-001**: the shipped surface is labeled
> "Sessions" everywhere (nav, command palette `common_sessions()`, route
> `/sessions`), not "Inventory" — this spec's own prose already calls it
> "Inventory/Sessions" throughout; treat "Sessions" as the actual product
> name. **FR-004/SC-003**: 041 FR-051 removed the review-state lifecycle
> almost entirely (no `reviewState`/`SessionState` rendering remains in
> `features/sessions/`) rather than restyling it as plain text — only the
> `ignored`/recoverable state survived (FR-010); FR-004/SC-003 describe a
> milder "restyle" outcome than what actually shipped. **FR-006**: no
> "primary action + small More menu" pattern exists in
> `features/sessions/` — row actions did not converge on that shape.
>
> **FR-001**: The product name for the stable library surface MUST be "Inventory".
- **FR-002**: The Inventory/Sessions view is lights-centric and MUST NOT expose a frame-type filter. Dark/flat/bias inventory is filtered on the Calibration page (spec 040), and Inbox single-type ingest (spec 041) means the Inventory ledger is already single-type. *(Superseded 2026-07-03: the original light/dark/flat/bias row filter was removed by the 043 redesign + 040 Calibration surface.)*
- **FR-003**: Inventory MUST NOT use ambiguous "tags" or "handling" fields as primary workflow controls.
- **FR-004**: Inventory MUST show review state as plain text or structured data, not as decorative state bubbles.
- **FR-005**: Inventory detail panes MUST show selected item details only.
- **FR-006**: Inventory actions MUST use the same primary action plus small More menu pattern as Inbox and Projects.
- **FR-007**: Each Inventory/Sessions row MUST offer an "Open location / Reveal in OS" action that opens the item's source location in the native OS file browser, wired to the existing native reveal command (spec 004) when the Tauri integration is available.
- **FR-008**: Inventory MUST preserve lifecycle references back to Inbox/source observations.
- **FR-009**: Mixed folders MUST be split before they can become Inventory items. This is enforced upstream by the Inbox single-type ingest gate (spec 041); Inventory therefore only ever receives single-type items.
- **FR-010**: The redesigned Sessions UI MUST expose an **Ignore** action (distinct from Reject) on a row/drawer so a discovered or needs-review session can be set to the `ignored` canonical state. A Cmd+K palette entry "Show ignored items" MUST navigate to `/sessions?reviewFilter=ignored`, surfacing `ignored` sessions that are otherwise excluded from the default ledger, from which they can be recovered (re-open). *(Reject = discard as not a usable session; Ignore = valid but hidden and recoverable.)*

### Key Entities

- **Inventory Item**: Reviewed or reviewable source data available for calibration and project workflows.
- **Inventory Review State**: One of the six canonical session states: `discovered`, `candidate`, `needs_review`, `confirmed`, `rejected`, or `ignored`.
- **Frame Type**: Light, dark, flat, bias, or dark flat.
- **Source Reference**: Original configured source root and discovered path.
- **Inventory Lifecycle Event**: Move, review, correction, stale source, archive, or removal event.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A user can filter the Inventory/Sessions ledger by source and review state in one interaction. *(Superseded 2026-07-03: frame-type filtering was removed from Inventory per FR-002; frame-type filtering now lives on the Calibration page, spec 040.)*
- **SC-002**: A selected Inventory item can be understood from the detail pane without reading row descriptions.
- **SC-003**: Confirming sample dark, bias, flat, and light items supports the guided first-project flow.
- **SC-004**: No Inventory table column is named Tags or Handling.

## Assumptions

- Inventory is local-first and backed by SQLite.
- Source data remains externally owned unless a specific mutation plan is approved.

## Out of Scope

- Cleanup/archive execution.
- Target catalog lookup.
- Processing-tool execution.

## Implementation Status

The desktop mockup at `apps/desktop/src/features/inventory/InventoryPage.tsx`
already realises the visual and interaction shape of Inventory against the
mock store in `apps/desktop/src/data/store.ts` and `apps/desktop/src/data/mock.ts`.
Implementation for this spec moves the underlying data and state machine into
Rust crates and a portable contract; the UI shell does not need to change.

The following surfaces are already shipped in the mockup and serve as the
visual contract this spec ratifies:

- **Grouped ledger by source root**: `InventoryPage` groups sessions by
  `InventorySource.path`, with a per-group header showing `kind` and
  source `state` as meta text. This satisfies FR-005's "details only in
  the detail pane" constraint while exposing source identity at the group
  level instead of as a row column.
- **No frame-type filter**: Per FR-002 (amended 2026-07-03) the Inventory/Sessions
  view is lights-centric and exposes no frame-type Select. Dark/flat/bias
  filtering lives on the Calibration page (spec 040); Inbox single-type ingest
  (spec 041) keeps the ledger single-type. (The backend projection retains an
  inert `frame_filter` parameter for contract stability; the UI does not surface it.)
- **Review-state filter**: A `Review` Select offers the canonical states
  `discovered | candidate | needs_review | confirmed | rejected | ignored`,
  sourced from `InventorySession.state`. The UI maps display labels locally:
  `discovered` and `candidate` display as "Needs review"; `ignored` is set via
  the **Ignore** action and reachable via the Cmd+K "Show ignored items" action
  navigating to `/sessions?reviewFilter=ignored` (FR-010). State surfaces as a
  `StateLabel` row cell and a `State` fact in the drawer; no badge bubble shows
  alongside row content (FR-004).
- **Action-bound primary CTA**: The drawer's primary `Confirm` button only
  renders when the selected session is in `needs_review`. This is the
  action-bound review pattern defined in spec 002 — the CTA exists because
  the action is available, not as decoration.
- **Action-bound overflow**: `Re-open review` appears in the row/drawer
  overflow Menu only when the session is NOT in `needs_review`. `Ignore` (set
  `ignored`) and `Reject session` (`tone: "danger"`) are grouped in a separate
  Menu section — Ignore hides a valid-but-not-now session recoverably, Reject
  discards it. All call `setSessionReviewState`, which is idempotent
  (re-applying the same state is a no-op in the store). Open location / Reveal
  in OS (FR-007) is also available per row.
- **Source-state surfacing**: `InventorySource.state` (`active | missing |
  disabled | reconnect_required`) is rendered in the group meta line. The
  spec keeps "stale source" semantics aligned with `LibraryRoot.state` from
  spec 002's data model; surfacing them at the group header avoids polluting
  every row.

Phase 0 research, Phase 1 plan / data model / contracts, and Phase 2 tasks
treat the mockup as the visual and interaction contract. The Rust port keeps
hook signatures (`useInventorySources`, `setSessionReviewState`,
`getInventorySources`) intact so the component tree under
`apps/desktop/src/features/inventory/` is not touched by the migration.

## Iterations

### Iteration 2026-07-14: Framing-layer session membership (Q27, cross-spec delta)

**Change**: Spec 008's Q27 framing layer references light sessions as **framing
members** (`project → framing → session → frames`). A light session belongs to
at most one framing; membership and the `Framing` entity are owned by spec 008.
The framing clustering reads session-level geometry (pointing/rotation/
optic-train) persisted at confirm by spec-008 F-Framing-1 — the Q12
strict-gate iterate (not yet applied) will guarantee those attributes on new
ingests; NULL-geometry legacy sessions are excluded until a Q28 rescan
backfill. The Q27 incremental attribution pass is the **first** pre-ingest
pass at the Inbox confirm gate; the Q22 duplicate sweep joins the same pass
when its iterate lands. No `InventorySession` state, projection shape, or
contract changes here — the delta is a documented reference from spec 008.
**Scope**: Cross-spec delta (documentation only; no new FR/entity in this spec).
**Artifacts updated**: spec.md (amendment note + log), data-model.md (framing-
membership reference note). *(2026-07-14 gate-fix update: Q22/Q12 references
restated as composition points / pending iterates, not existing artifacts.)*
