# Feature Specification: Generated Project Source View Removal

> **See Spec 030**: UI implementation of this feature must follow
> [Spec 030 — UI Audit & Revision](../030-ui-audit-revision/spec.md)
> for layout, navigation, and component patterns.

**Feature Branch**: `026-generated-project-source-view-removal`  
**Created**: 2026-05-09  
**Updated**: 2026-07-03  
**Status**: Implemented (2026-08-17); P3 stale-detection sweep landed  
**Input**: User description: "Specify removing generated project source views and app-created links/folders without touching original Inventory data."

> **Generation path active.** Source-view generation is specified by
> [Spec 049 — Source View Generation](../049-source-view-generation/spec.md) and
> shipped as the `sourceview_generate` command, so this spec's
> remove/regenerate machinery has live views to act on.
>
> Spec 049 CL-2 relaxes **FR-008 / the single-kind invariant** below to
> *deterministic kind per drive-scope, recorded per item*. A generation's link
> kind resolves from a settings pair (intra-drive default vs cross-drive
> default), is chosen per item by rule, and is recorded; a view may carry more
> than one recorded kind. FR-008 and the data-model invariant are amended
> accordingly.

## Implementation Status

Shipped: persistence (migration 0029), domain types, the persistence
repository, app-core use cases, contract DTOs, the `preparedview_list` /
`preparedview_remove` / `preparedview_regenerate` commands, and the frontend
helpers (`source-views.ts`, `SourceViewsSection`).

Stale detection is active: `sweep_view_staleness`
(`crates/app/projects/src/source_view_verify.rs:224`) runs on view listing
(`crates/app/projects/src/prepared_views.rs:126`) and after plan apply
(`crates/app/core/src/plan_apply/finalizers.rs:177`, `:222`).

Open items, tracked in beads:

- Windows junction/reparse-point specifics at apply time, scoped with spec 025's
  cross-platform work. The `SourceViewRemove` plan is created and applied by the
  spec 025 executor via the `archive` action type.
- Data-migration scan for pre-existing `kind_diverged` records.
- End-to-end plan-apply integration tests.
- Per-view-item audit emission and the UI audit-history surface.
- The E-026-1 envelope sweep.

The canonical project database remains the source of truth; prepared source
views are reproducible projections and removing them MUST be reversible by
regeneration.
### User Story 3 - Regenerate a Removed Source View (Priority: P2)

As a user, after removing a generated source view to free disk space, I want to
regenerate it from the canonical database so that the workflow remains
reproducible without re-importing inventory.

**Why this priority**: Removal is only safe if regeneration is cheap, reviewed,
and produces the same logical view.

**Independent Test**: Remove a view, then issue a regenerate request and confirm
a new plan is produced that re-creates the same item set from canonical sources.

**Acceptance Scenarios**:

1. **Given** a previously removed source view, **When** the user requests
   regeneration, **Then** a new filesystem plan is produced from the canonical
   database with the same logical item set.
2. **Given** the canonical inventory references have changed since removal,
   **When** the user regenerates, **Then** the plan reflects the current
   canonical state and flags any unresolved references.

### User Story 4 - Detect Stale Source Views (Priority: P3)

As a user, I want the app to detect when a generated source view has gone stale
(inventory moved, removed, or remapped) so that I can decide to remove or
regenerate it before it misleads downstream tools.

**Acceptance Scenarios**:

1. **Given** a source view references an inventory item that no longer resolves,
   **When** the project is opened, **Then** the view is marked stale with the
   broken reference visible.

### User Story 5 - Audit Source View Removal (Priority: P3)

Every view removal MUST emit per-item audit events covering attempted action,
outcome, and any failures, consistent with the constitution's reviewable
filesystem mutation principle.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Remove Generated Source Views (Priority: P1)

As a user, I want to remove generated project source views when a project is changed or cleaned up so that app-created links/folders do not linger.

**Why this priority**: Earlier 'Prepared Sources' wording represented generated workflow source views; removal needs a bounded spec with clear source-truth boundaries.

**Independent Test**: Generate project source views, remove them through a reviewed plan, and confirm original Inventory files remain untouched.

**Acceptance Scenarios**:

1. **Given** a project has generated source views, **When** the user creates a removal plan, **Then** only app-created links/folders are included.
2. **Given** the plan is applied, **When** removal succeeds, **Then** original Inventory files remain unchanged and project lifecycle records the removal.
3. **Given** a generated source view cannot be removed, **When** apply completes, **Then** the item is marked failed with retry context.

---

### User Story 2 - Distinguish Generated Views From Source Data (Priority: P2)

As a user, I want the UI to clearly distinguish generated project views from original data so that cleanup is safe.

**Why this priority**: The app must never make generated views look like canonical data.

**Independent Test**: Open project detail and confirm generated source views show their source Inventory references and generated state.

**Acceptance Scenarios**:

1. **Given** a generated source view exists, **When** it is shown in project detail, **Then** it references the Inventory item it projects.
2. **Given** a generated view is stale, **When** the project opens, **Then** the app indicates it needs regeneration or removal.

### Edge Cases

- Generated source view points through a symlink or junction.
- User manually deleted generated view outside the app.
- Generated view path conflicts with a user-owned folder.
- Removal plan includes both generated views and archive actions.

### Domain Questions To Resolve

- ~~Which generated view strategies are supported at first release.~~
  **RESOLVED (R-026-Strategies, GRILL 2026-05-22)**: v1 ships
  `symlink`, `junction`, and `copy`. `hardlink` is reserved/deferred
  to v1.x. See Out of Scope.
- ~~Whether stale generated views are automatically included in cleanup plans.~~
  **RESOLVED (R-026-StaleAutoInclude, GRILL 2026-05-22)**: Stale views
  NEVER auto-mutate. Cleanup plans (spec 017) MAY include stale views
  as passive candidates in their preview; the user explicitly approves
  any action.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Generated source views MUST be tracked separately from Inventory data.
- **FR-002**: Removal plans MUST include only app-created links/folders unless the user explicitly adds other reviewed items.
- **FR-003**: Removal MUST happen through filesystem plan review and apply, following the full spec 017/025 pipeline: `plan.approve` (with approvalToken) → `plan.apply` (with per-item FS revalidation, paused state, `plan.resume`). See R-026-Pipeline.
- **FR-004**: Project detail MUST show generated source view state and source Inventory references.
- **FR-005**: Missing generated views MUST be marked missing or stale, not silently removed from history.
- **FR-006**: Removal outcomes MUST be logged per item.
- **FR-007** *(R-026-Strategies, GRILL 2026-05-22)*: v1 view strategies
  are `symlink`, `junction`, and `copy` only. `hardlink` is reserved
  and deferred to v1.x.
- **FR-008** *(A2, GRILL 2026-05-22; AMENDED 2026-07-04 by spec 049 CL-2)*:
  A generated view's materialization kind is resolved **deterministically per
  drive-scope** (intra-drive vs cross-drive) from the spec 049 link-kind settings
  pair and **recorded per item** (`PreparedSourceViewItem.materialization`). The
  original strict invariant — `PreparedSourceView.kind` equals every item's
  `materialization` — is **relaxed** to: every item's kind MUST be chosen by a
  deterministic rule and recorded, and the view MUST NOT contain any **unrecorded
  or non-deterministic** kind. `PreparedSourceView.kind` records the **dominant**
  kind for display; per-item `materialization` is authoritative. A view whose items
  carry more than one recorded kind (e.g., hardlink intra-drive + symlink
  cross-drive) is **valid**. Only a request that would produce an unrecorded/
  non-deterministic kind is refused (`view.mixed_kind`). **Revert note:** to restore
  the strict single-kind invariant, re-require `kind` == every item's
  `materialization` here and change spec 049 FR-004/FR-022 to refuse
  cross-drive-forced mixed views.
- **FR-009** *(A3, GRILL 2026-05-22)*: Copy-kind stale detection
  includes content hash mismatch. Link-kind (symlink/junction) views
  skip content hash on stale detection.
- **FR-010** *(A4, GRILL 2026-05-22)*: A view in state `removed` has an
  indefinite regenerable lifetime. The `PreparedSourceView` record is
  never hard-deleted; regeneration always remains available.
- **FR-011** *(R-026-Dest-Archive, GRILL 2026-05-22)*: The destructive
  destination for view removal is always `archive`. There is no
  user-selectable `destructiveDestination` field on the remove request;
  the server hard-codes `archive` for the underlying `FilesystemPlan`.
- **FR-012** *(R-026-Lifecycle, GRILL 2026-05-22)*: View removal and
  regeneration are allowed when the owning project is in
  `setup_incomplete | ready | prepared | processing | blocked |
  completed`. Requests on an `archived` project MUST be refused with
  `lifecycle.read_only`. Use the R-Unarchive path (spec 009) to
  unarchive before operating on views.
- **FR-013** *(R-026-StaleAutoInclude, GRILL 2026-05-22)*: Stale views
  MUST NOT be automatically mutated. Spec 017 cleanup plans MAY include
  stale views as passive candidates; the user explicitly approves any
  action taken against them.

### Key Entities

- **Generated Source View**: App-created link/folder/view used by a project workflow.
- **Source View Removal Plan**: Reviewable plan for removing generated views.
- **Generated View State**: Current, stale, missing, removed, or failed.
- **Source View Reference**: Link back to source Inventory item and project.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A removal plan can prove it will not touch original Inventory files.
- **SC-002**: Project detail clearly identifies generated views and their Inventory sources.
- **SC-003**: Missing or failed generated view removals remain visible until resolved.

## Assumptions

- Generated project source views replace the older user-facing "Prepared Sources" wording.
- Filesystem plan application handles the actual removal operation.

## Out of Scope

- Processing-tool execution.
- Deleting original source data without cleanup/archive review.
- **`hardlink` view strategy** *(D-026-H1 / R-026-Strategies, GRILL
  2026-05-22)*: Hardlink creation, removal, and regeneration are
  deferred to v1.x. The `kind` enum reserves the value; no
  implementation is provided in v1.
- **User-selectable destructive destination**: The remove contract does
  not expose a `destructiveDestination` field. Archive is hard-coded
  (R-026-Dest-Archive, GRILL 2026-05-22).
- **Envelope sweep for existing contracts** *(E-026-1, GRILL 2026-05-22)*:
  The camelCase + `contractVersion` + `requestId` envelope sweep for
  `preparedview.remove.json` and `preparedview.regenerate.json` is
  deferred to the cross-spec final envelope sweep pass.
