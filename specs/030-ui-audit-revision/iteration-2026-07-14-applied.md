---
status: applied
created: 2026-07-14
applied: 2026-07-14
change_request: "Formalize grilling decision Q15 (docs/development/ui-campaign-grilling-decisions-2026-07-13.md §Q15, issue #647) — durable audit coverage/unification. LOCKED decisions: (a) every attempted mutation of durable state/user data writes a durable audit row (settings changes, protection overrides, equipment CRUD, source enable/disable/register/delete, rescans/root ops) with outcome incl. refused/failed + reason/code; (b) durable audit_log_entry table is the single source of truth — audit-worthy actions write there AND emit to the bus for live UI; kill the emit-to-bus-only pattern for mutations; the Q9 Activity/log panel reads durable audit for user-meaningful events + ephemeral bus for transient noise; (c) generalize the audit entry shape from lifecycle-specific to a generic mutation record: timestamp, actor, action, entity(type+id), outcome+reason, optional before-after. NOT audited: reads, navigation, UI state, transient internal/periodic events."
scope: "Feature-wide (new requirement block: durable audit coverage & store unification)"
---

> **Superseded by the 2026-07-14 critique fix round** — see spec.md §8.3
> (store roles), FR-130/131/134, tasks.md T125. Historical record; content
> below reflects the pre-fix state.

## Change Summary

Add durable-audit coverage and store-unification requirements (grilling Q15,
issue #647) to spec-030: every attempted mutation of durable state writes a
durable `audit_log_entry` row, the durable table becomes the single source of
truth over the ephemeral bus, and the entry shape generalizes from a
lifecycle-transition record to a generic mutation record.

## Implementation Progress

- **Tasks completed**: tasks.md checkboxes are all `[ ]` (0 of 113 ticked),
  but spec-030 was implemented issue-driven — see `issue-map.md` (T001–T113
  mapped to closed GitHub issues #140+). Checkbox state is not authoritative
  for this spec.
- **Current phase**: post-implementation campaign; #647 is an open finding
  against the shipped Audit Log / event architecture.
- **Adhoc changes**: None on this branch (branch is spec-artifact-only).

### Current architecture (read-only orientation, cited)

Two disjoint stores exist today:

- **Ephemeral/live side** — `EventBus`
  (`crates/audit/src/bus.rs:37-40`): hybrid tokio broadcast (live UI) +
  durable `events` topic stream (`crates/persistence/core/migrations/0001_initial_schema.sql:720`).
  The `events` table is a topic+payload stream, not an audit record — it has
  no outcome/refused semantics. Typed payloads for settings changes,
  protection sets, plan/lifecycle progress, rescans etc. are declared in
  `crates/audit/src/event_bus.rs` (re-exported at `crates/audit/src/lib.rs:14-30`).
- **Durable audit side** — `audit_log_entry` table
  (`crates/persistence/core/migrations/0001_initial_schema.sql:130-143`): columns
  `audit_id, entity_type, entity_id, from_state, to_state, trigger, actor
  (user|system), outcome (applied|refused|failed), severity
  (workflow|diagnostic), request_id, at, payload`. Rust type `AuditLogEntry`
  is lifecycle-transition-shaped (`crates/audit-types/src/event.rs:106+`,
  doc: "Durable, append-only record of a lifecycle transition attempt").
  Writers today: lifecycle transitions
  (`crates/persistence/db/src/repositories/lifecycle.rs:423,511`) and the
  audit repository insert (`crates/persistence/db/src/repositories/audit.rs:216`).

The Q15 violation, concretely: protection-set mints an `audit_id` and
publishes to the bus only, then returns that id to the UI —
`crates/app/core/src/protection.rs:227-228` (new id + `bus.publish`) and
`:404-419` (acknowledge path returns `Ok(audit_id)`). No `audit_log_entry`
row exists for that id, violating constitution §II ("audit record for each
attempted action and outcome"). Settings mutations follow the same
bus-only pattern (`crates/app/settings/src/lib.rs:481,500,601,615,768`), as
do source ops — `sources.set_active`, root remap, first-run completion
(`crates/app/core/src/first_run.rs:503,542,597`). Equipment CRUD
(`crates/app/calibration/src/equipment.rs`) emits **no audit at all** —
neither bus nor durable (verified: zero `publish`/`audit` references).

## Impact Assessment

| Artifact | Action | Details |
|----------|--------|---------|
| spec.md | Modify | New FR block **Durable Audit Coverage** (FR-130–FR-134); new SC-009; new detailed-spec section 8.3 under "Audit Log — Moved to Settings" |
| plan.md | Modify | New implementation phase **G. Audit Unification**; technical-context note on the two-store split with citations |
| tasks.md | Add | New **Phase 10: Durable Audit Unification (Q15 / #647)**, tasks T120–T127; dependencies note |
| data-model.md | Add | New section **Audit Entry — Generalized Mutation Record** (current lifecycle shape → generic shape mapping) |
| contracts/commands.md | Add | New section on audit semantics: `auditId` returned by mutation commands MUST reference a durable row; audit list/read surface unchanged in shape, extended in coverage |
| research.md | No change | — |
| quickstart.md | No change | — |
| issue-map.md | No change | New tasks get issues via `/speckit.taskstoissues` later, not in this iteration |

## Risk Checks

- [x] No completed tasks invalidated — tasks.md has no ticked tasks; shipped
  issue-driven work is affected at the architecture level (bus-only emitters
  need durable writes), which is exactly what the new Phase 10 tasks cover.
- [x] No scope boundary violations — Audit Log/event surface is already
  spec-030 scope (FR-114, spec.md §8); Q15's iterate-map row assigns it to
  the spec-030 campaign.
- [x] No downstream dependency breaks — new phase depends only on existing
  audit plumbing; Q9 (log panel, spec-030) and Q10 (manifest history,
  spec-024 iterate) *depend on* this iteration, not the reverse.

## Planned Changes

### spec.md

1. Add a new FR group **"Durable Audit Coverage"** after the "Settings"
   FR group (FR-110–FR-114), numbered FR-130–FR-134:
   - **FR-130**: Every attempted mutation of durable state or user data MUST
     write a durable audit row — including settings changes, protection
     overrides, equipment CRUD, source enable/disable/register/delete, and
     rescans/root operations — recording the outcome including
     refused/failed with a reason/code.
   - **FR-131**: The durable `audit_log_entry` store MUST be the single
     source of truth for audit history. Audit-worthy actions MUST write the
     durable row AND emit a live event to the bus; emitting to the bus only
     is prohibited for mutations. Any `auditId` returned to the UI MUST
     resolve to a durable row.
   - **FR-132**: The Activity/log panel MUST read user-meaningful events from
     the durable audit store, and transient/internal noise from the ephemeral
     bus (makes Q9's "activity is a view over the audit" literally true).
   - **FR-133**: The audit entry shape MUST generalize from a
     lifecycle-transition record to a generic mutation record: timestamp,
     actor, action, entity (type + id), outcome + reason, plus an optional
     before→after value pair for settings/protection changes.
   - **FR-134**: Reads, navigation, UI state changes, and transient
     internal/periodic events MUST NOT be durably audited.
2. Add **SC-009**: 100% of mutation commands that return an `auditId` return
   one that resolves to a durable `audit_log_entry` row; zero mutation paths
   emit to the bus without a durable write.
3. Add detailed-spec **section 8.3 "Durable Coverage & Unification
   (Q15 / #647)"** after §8.2: the architecture reframe (two stores → one
   source of truth), the covered-mutations list, the outcome/reason
   requirement, the generalized record shape, and the not-audited line.

### plan.md

1. Add row **G. Audit Unification** to the Implementation Phases table:
   generalize the audit entry model, add durable writes to all bus-only
   mutation emitters, rewire the Activity/log panel read path.
2. Add a short technical-context note documenting the current two-store
   split with the citations from this iteration's orientation section.

### tasks.md

Add **Phase 10: Durable Audit Unification (Q15 / #647)** after Phase 9:

- T120: Generalize the durable audit entry model (`audit-types`) from
  lifecycle-transition shape to generic mutation record (action, generic
  entity type, reason/code, optional before→after) with a compatible
  migration for `audit_log_entry`.
- T121: Shared write-through helper: one path that writes the durable row
  and emits the bus event, returning the durable `audit_id`.
- T122: Settings mutations write durable audit rows (with before→after).
- T123: Protection overrides/acknowledgements write durable audit rows;
  returned `auditId` references the durable row.
- T124: Equipment CRUD writes durable audit rows.
- T125: Source enable/disable/register/delete and rescans/root ops write
  durable audit rows (Q5 delete-cascade audit lands here).
- T126: Activity/log panel reads durable audit for user-meaningful events +
  ephemeral bus for transient noise (Q9 wiring point).
- T127: Refusal/failure coverage tests: refused and failed mutations produce
  durable rows with outcome + reason/code; reads/navigation produce none.

Update "Dependencies & Execution Order": Phase 10 depends on existing audit
plumbing only; independent of Phases 3–9.

### data-model.md

Add section **"Audit Entry — Generalized Mutation Record"**: the target
shape (timestamp, actor, action, entity type+id, outcome+reason, optional
before→after), its mapping onto the existing `audit_log_entry` columns
(`trigger`→action; `from_state`/`to_state` subsumed by optional
before→after; reason/code as first-class queryable detail), and the
retention of `severity`/`request_id`. Entity types extend beyond the
lifecycle `EntityType` enum to cover settings, protection, equipment,
sources, and roots.

### contracts/commands.md

Add section **"Audit Semantics (iteration 2026-07-14)"**: mutation commands
that return an `auditId` now guarantee it resolves to a durable
`audit_log_entry` row; audit read/list commands keep their shape but their
coverage expands to all durable-state mutations; no new commands required
by this iteration.
