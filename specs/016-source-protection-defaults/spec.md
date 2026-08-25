# Feature Specification: Source Protection Defaults

> **See Spec 032**: UI implementation of this feature must follow
> [Spec 032 — Design V4](../032-design-v4-implementation/spec.md), the current UI
> truth, for layout, navigation, and component patterns. Spec 030 (UI Audit &
> Revision), cited here previously, is superseded as of 2026-06-11.

**Feature Branch**: `016-source-protection-defaults`  
**Created**: 2026-05-09  
**Status**: Implemented (post-hoc record, verified 2026-08-24) — see
Implementation Status below for the per-item evidence.  
**Input**: User description: "Specify protection settings as per-source behavior with global defaults rather than only a global protection setting."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Set Source-Level Protection (Priority: P1)

As a user, I want each configured source to define its own protection behavior so that capture folders, calibration stores, and project folders can have different mutation rules.

**Why this priority**: Cleanup/archive safety depends on the specific source and ownership model.

**Independent Test**: Configure different protection behavior for Inbox, Inventory, calibration, and project sources and confirm cleanup/archive plans respect each source setting.

**Acceptance Scenarios**:

1. **Given** a source is protected, **When** a cleanup plan includes files from it, **Then** the plan marks them as blocked or requires explicit override.
2. **Given** a source inherits defaults, **When** the global default changes, **Then** inherited source behavior updates while overridden sources remain unchanged.
3. **Given** a source is externally owned, **When** mutation is requested, **Then** the app warns and requires review before any destructive action.

---

### User Story 2 - Apply Defaults To New Sources (Priority: P2)

As a user, I want global protection defaults for newly added sources so that common safety policy does not need to be repeated.

**Why this priority**: Per-source protection should not make setup tedious.

**Independent Test**: Change the default protection policy, add a new source, and confirm it inherits the default while remaining editable.

**Acceptance Scenarios**:

1. **Given** a default protection policy exists, **When** a source is added, **Then** the new source starts with inherited protection.
2. **Given** a source-level override exists, **When** a cleanup plan is generated, **Then** the override takes precedence over the default.

### Edge Cases

- Source root is moved or missing.
- Same physical path is configured under two source names.
- Project-generated folders exist under an externally owned source root.
- User attempts permanent delete from a protected source.

### Domain Questions To Resolve

- Which source categories should default to protected.
- Whether source protection applies to archive moves, deletes, or both.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Protection behavior MUST be configurable per source.
- **FR-002**: Settings MUST provide global defaults used by newly added or inherited sources.
- **FR-003**: Source-level overrides MUST be visible in the source detail/settings row.
- **FR-004**: Cleanup/archive plans MUST evaluate protection at source level.
- **FR-005**: Destructive actions against protected sources MUST require explicit warning and confirmation.
- **FR-006**: Protection settings MUST be auditable.
- **FR-007**: The `trash` destructive destination MUST NOT be blocked by
  `block_permanent_delete`. Only the `permanent_delete` action (distinct from
  `archive` and `trash`) is blocked and rewritten (R-OSTrash-Allowed, 2026-05-22).
- **FR-008**: The `plan.protection.check` response MUST include ONLY items
  requiring user acknowledgement. Normal and unprotected items MUST appear
  only as summary counts in `non_blocking_summary` (R-CheckScope, 2026-05-22).

### Key Entities

- **Source Protection Policy**: Rules controlling archive, delete, move, or modification operations for a source.
- **Protection Default**: Global policy inherited by sources without overrides.
- **Protection Override**: Source-specific policy.
- **Protected Plan Item**: Cleanup/archive plan entry affected by source protection.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Users can see whether a source inherits or overrides protection.
- **SC-002**: Protected-source items cannot be permanently deleted without explicit confirmation.
- **SC-003**: Cleanup/archive review explains why protected items are blocked or require approval.

## Assumptions

- Protection does not prevent read-only scanning.
- Permanent delete remains an advanced reviewed operation.

## Out of Scope

- OS-level filesystem permissions.
- Remote storage retention policies.
- "Freeze project" toggle (deferred to v1.x): a feature that would promote
  all sources involved in a project to `protected` for the duration of a
  milestone. Explicitly out of scope for v1.

## Implementation Status

**Mockup-done (apps/desktop):**

- Source Protection settings section (`SettingsPage.tsx::SourceProtectionSection`)
  exposes three-level default protection (`protected` / `normal` / `unprotected`),
  a `Block permanent delete` switch, and a `Protected categories` text input
  (`lights, masters, finals` by default).
- Settings store (`src/data/settings.ts`) persists `defaultProtection`,
  `blockPermanentDelete`, and `protectedCategories` keys.
- Per-source override surfaces (Sources detail / row) are scoped to future
  implementation; mockup demonstrates inheritance language only.

**Shipped** (verified against `origin/main` a52c637f2 on 2026-08-24; these four
items were carried as "Pending implementation" long after they landed):

- Per-source protection override storage and resolver (override → global default):
  `crates/app/core/src/protection/source_protection.rs:35` `get_source_protection`
  (a `None` `source_id` returns the global defaults directly) and `:102`
  `set_source_protection`; global defaults at
  `crates/app/core/src/protection/global_defaults.rs:36`. Exposed as
  `source_protection_get` / `source_protection_set`
  (`apps/desktop/src-tauri/src/commands/protection.rs:37`/`:56`). The per-source
  override UI shipped as
  `apps/desktop/src/features/settings/SourceProtectionOverride.tsx`, superseding
  the "scoped to future implementation" note above.
- Protection evaluation hook inside plan generation:
  `crates/app/core/src/protection/plan_check.rs` `plan_protection_check`, which
  marks items `requires_acknowledgement` at `:103`; command
  `plan_protection_check_cmd` (`protection.rs:80`).
- Protected categories enforcement: `protected_categories` is applied in cleanup
  generation at `crates/app/core/src/cleanup_generator/raw_frames.rs:33` and
  `cleanup_generator/scan.rs`, with the setting defined in
  `crates/app/settings/src/descriptors.rs`.
- Audit events for protection changes and protected-plan acknowledgements:
  `set_source_protection` returns a resolvable `audit_id`
  (`crates/app/core/src/protection/tests.rs:211` and `:230`, the latter asserting
  a durable `audit_log_entry` row), and `acknowledge_protected_item`
  (`plan_check.rs:132`) emits `protection.plan.acknowledged`
  (`tests.rs:395`-`:405`). Command surface at `protection.rs:101`.

End-to-end coverage: `crates/e2e-tests/tests/cleanup_protection_gate_journey.rs`.
