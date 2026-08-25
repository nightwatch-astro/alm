# Feature Specification: Target Identity, History, And Notes

> **RECONCILED + IMPLEMENTED on gen-3 (2026-06-23).** This spec originally targeted the **retired gen-2
> model** (`target_aliases`, `target_id` FKs, `primary_designation` column) that spec 036 deleted; identity
> now lives in the spec-035 `canonical_target` id-space and **Targets is a primary nav page** (reversing the
> old "not top-level nav" assertion). The feature has now been **rebuilt on gen-3**:
> - **Identity + aliases** (US1) — `target.get`, `target.alias.add/remove` shipped (gen-3).
> - **US2 linked sessions / US3 linked projects / US4 observing notes** — shipped via new commands
>   `target.sessions.list`, `target.projects.list`, `target.note.get/update` (migration `0048_target_notes`
>   adds `canonical_target.notes`), wired into `TargetDetailV2.tsx`.
> - **`target.primary.rename` is DROPPED** (out of scope; the contract is orphaned).
>
> The Foundations tasks below (new `target_alias` table, `target_id` FKs, `primary_designation` column) are
> **obsolete** — superseded by the 035/036 gen-3 model.
>
> **Closed out 2026-06-23.** Caveats resolved: note edits now emit a `target.note.updated` audit event;
> the projects route was migrated to UUID-keyed `selected` (parseString), so linked-project rows now open
> the right project. FR-004's 16 KB note cap is enforced (`note.content_too_large`). `speckit-verify` passed
> (FR-009 dropped + SC-002 nav reversal are sanctioned scope changes; FR-005/FR-007 observing-plan refs
> remain deferred / out of scope).

> **See Spec 032**: UI implementation of this feature must follow
> [Spec 032 — Design V4](../032-design-v4-implementation/spec.md), the current UI
> truth, for layout, navigation, and component patterns. Spec 030 (UI Audit &
> Revision), cited here previously, is superseded as of 2026-06-11.

**Feature Branch**: `023-target-identity-history-notes`  
**Created**: 2026-05-09  
**Updated**: 2026-06-23  
**Status**: **Closed (2026-06-23)** — US1–US4 shipped on gen-3 + caveats resolved + `speckit-verify` passed. `target.primary.rename` dropped; FR-005/FR-007 observing-plan refs deferred. See banner.  
**Input**: User description: "Specify target identity, aliases, target history, observing-plan references, and notes as bounded follow-on features beyond FITS OBJECT lookup."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - View Target Identity (Priority: P1)

As a user, I want to open a target detail page from Cmd+K, an Inventory row,
or a Project source so I can see canonical identity (primary name, catalog
refs, aliases) without targets becoming a primary nav destination.

**Why this priority**: Identity resolution is the foundation; everything else
(history, projects-per-target, notes) hangs off a stable target id.

**Independent Test**: Search "M31" in Cmd+K, open the resulting target detail
at `/targets/$targetId`; confirm primary name, aliases, and catalog references
render. Repeat the entry from an Inventory item and a Project source link.

**Acceptance Scenarios**:

1. **Given** multiple names refer to the same target, **When** the user opens
   target detail, **Then** all aliases and catalog identifiers are listed.
2. **Given** a user manually corrects a target alias, **When** the correction
   is saved, **Then** the original FITS `OBJECT` hint remains visible in
   provenance.
3. **Given** a target has catalog aliases, **When** the target is opened,
   **Then** aliases and catalog identifiers are listed separately from notes.
4. **Given** Targets is not in primary nav, **When** the user inspects the app
   shell, **Then** no top-level Targets entry exists; access is only via
   Cmd+K, Inventory, or Project links.

---

### User Story 2 - See Sessions Over Time (Priority: P2)

As a user, I want to see every acquisition session linked to a target ordered
by capture date so I understand what data I have already collected across
years and which gaps remain.

**Why this priority**: Multi-year acquisition history is the primary reason
target detail exists; users plan continuation by reviewing prior captures.

**Independent Test**: For a target with sessions captured in three different
years, open target detail and confirm the sessions section lists each session
grouped/ordered by `captured_on` with filter, exposure, and frame counts.

**Acceptance Scenarios**:

1. **Given** a target has linked sessions across multiple years, **When**
   target detail opens, **Then** sessions render in reverse-chronological
   order with date, filter, and frame summary.
2. **Given** a session row is selected, **When** the user activates it,
   **Then** the app deep-links to the corresponding Inventory item.
3. **Given** a target has no sessions yet, **When** detail opens, **Then** an
   explicit empty state explains that imports will appear here.

---

### User Story 3 - See Projects Per Target (Priority: P3)

As a user, I want to see every project that uses a target so I can pivot from
acquisition history to processing state for the same object.

**Why this priority**: Connecting acquisition to processing per target makes
"have I already worked on this" answerable from one screen.

**Independent Test**: Create two projects that reference the same target; open
target detail and confirm both projects appear with their lifecycle state and
link out to project detail.

**Acceptance Scenarios**:

1. **Given** a target has one or more projects, **When** target detail opens,
   **Then** a Projects section lists each project with its lifecycle state.
2. **Given** a project is archived, **When** the projects list renders,
   **Then** the archived lifecycle tone is visible.
3. **Given** a project is opened from the list, **When** the user activates a
   row, **Then** the app navigates to that project's detail route.

---

### User Story 4 - Observing Notes Per Target (Priority: P4)

As a user, I want to keep free-text observing notes per target so I can
record seeing, framing intent, plate-scale plans, or capture problems that
should persist across sessions.

**Why this priority**: Notes are useful but lower priority than identity,
history, and projects.

**Independent Test**: Edit the notes field on a target, refresh, and confirm
content persists with an updated-at timestamp; verify a per-session note
(spec 005) remains distinct from the per-target note.

**Acceptance Scenarios**:

1. **Given** a target detail is open, **When** the user edits the notes
   field and saves, **Then** the note is stored and `updated_at` is refreshed.
2. **Given** per-target and per-session notes both exist, **When** target
   detail renders, **Then** the per-target note is shown and per-session
   notes remain attached to their session rows.
3. **Given** a target name is later corrected, **When** the alias edit
   completes, **Then** the per-target note survives unchanged.

### User Story 5 - Correct A Duplicate Or Wrong Identity (Priority: P2)

As a user who finds that two separate targets have been created for the same
celestial object (or that a target was mis-named), I want to remove an alias
and rename the primary designation so that the identity record reflects the
correct catalog designation.

**Why this priority**: Spec 013 auto-creates targets from catalog equivalences;
users may encounter names that do not match their preferred designation and
need a lightweight remediation path without a full merge/split workflow.

**Independent Test**: A target with primary `NGC 224` has alias `M 31` added.
Remove alias `NGC 224` (failing with `alias.is_primary`). Rename primary from
`NGC 224` to `M 31` (success); confirm prior primary `NGC 224` becomes an
alias. Then remove alias `NGC 224` (success).

**Acceptance Scenarios**:

1. **Given** a target with a wrong primary designation, **When** the user
   renames the primary to an existing alias, **Then** the old primary becomes
   an alias and the alias becomes the new primary.
2. **Given** a target with an unwanted alias, **When** the user removes the
   alias, **Then** the alias is deleted and an audit event is written.
3. **Given** the user tries to remove an alias that is currently the primary,
   **When** the request is processed, **Then** the system returns
   `alias.is_primary` and no mutation occurs.

---

### Edge Cases

- Same target has conflicting catalog coordinates.
- FITS `OBJECT` is generic or wrong.
- User intentionally splits two aliases into separate targets.
- Linked observing-plan file moved or renamed.
- Target notes contain multiline technical comments.

### Domain Questions To Resolve

- **Final target identity merge/split workflow**: RESOLVED — spec 013 ships a
  cross-catalog equivalence table that handles automatic catalog unification.
  Manual full-merge/split (moving sessions across target records) is deferred;
  the v1 remediation path is `target.alias.remove` + `target.primary.rename`
  (US5 + R-3.4). See spec 013 data-model.md `CatalogEquivalence`.
- **Which observing-plan systems are recognized first**: DEFERRED (R5) — out of
  scope for v1.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Target identity MUST be a durable record separate from raw FITS `OBJECT` hints.
- **FR-002**: Target aliases and catalog identifiers MUST be stored as structured references.
- **FR-003**: Target detail MUST show linked sessions and projects contextually.
- **FR-004**: Target notes MUST be editable (max 16 KB UTF-8; A6) and auditable. *(gen-3: enforced — the
  16 KB cap returns `note.content_too_large`; edits emit the `target.note.updated` audit event.)*
- **FR-005**: Observing-plan references MUST be contextual links, not primary navigation. *(deferred — no
  observing-plan surface exists yet; R5 out of scope.)*
- **FR-006**: Manual target corrections MUST preserve the original hint and provenance. *(gen-3: satisfied
  structurally — the original SIMBAD `primary_designation` is immutable and always visible (it is the
  `effectiveLabel` fallback), resolution provenance is carried by `canonical_target.source`
  (`seed`/`resolved`/`user-override`, spec 035), and user aliases are purely **additive** — adding or
  removing a `kind='user'` alias cannot destroy or overwrite the original designation. There is no
  destructive "rename" path: `target.primary.rename` was dropped in favour of an additive `display_alias`.)*
- **FR-007**: Missing observing-plan references MUST warn without deleting historical records.
- **FR-008**: Users MUST be able to remove an alias from a target (`target.alias.remove`), with `alias.is_primary` error when the alias is also the primary designation.
  *(Reconciliation note, 2026-07-19, issue #764: like FR-009, this scenario
  is structurally foreclosed by the gen-3 model — `crates/app/targets/src/
  target_management.rs`'s alias-remove path carries no `is_primary` guard;
  there is no protected "primary alias" concept to violate. The 2026-06-23
  closing banner reconciled FR-009's drop but missed this one — both share
  the same root cause.)*
- **FR-009**: Users MUST be able to rename the primary designation by promoting an existing alias (`target.primary.rename`), which demotes the prior primary to alias status.

### Key Entities

- **Target Identity**: Durable target record with canonical display name and coordinates where known.
- **Target Alias**: Alternate name, catalog identifier, or user alias.
- **Target History Entry**: Linked session, project, artifact, note, or plan reference.
- **Observing Plan Reference**: Linked capture-plan file or external reference.
- **Target Note**: User-authored note with audit metadata.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A user can link multiple target name hints to one target identity.
- **SC-002**: Target history can be opened from Inventory or Projects without adding a Targets primary nav item.
- **SC-003**: Notes and observing-plan references survive target name correction.

## Assumptions

- FITS `OBJECT` target lookup exists before full target history implementation.
- Catalog metadata enrichment remains optional.

## Out of Scope

- Full observing-plan authoring.
- Mandatory local catalog cache.
- Automatic target merge without review.
