# Feature Specification: Target Lookup From FITS OBJECT

> **⚠ PARTIALLY SUPERSEDED (2026-06-18) by [Spec 035 — SIMBAD Target Resolution](../035-simbad-target-resolution/spec.md).**
> The offline-only resolution approach and the research-R4 deferral of online providers
> (Sesame/SIMBAD) are reversed: spec 035 makes **online SIMBAD resolution the primary path**, seeded by
> a bundled index + local cache; the spec-014 download flow this spec depended on is superseded. The
> **target-identity model** here (canonical target, `CatalogRef`, aliases, dedup) is **retained and
> reused** by spec 035.

> **See Spec 032**: UI implementation of this feature must follow
> [Spec 032 — Design V4](../032-design-v4-implementation/spec.md), the current UI
> truth, for layout, navigation, and component patterns. Spec 030 (UI Audit &
> Revision), cited here previously, is superseded as of 2026-06-11.

**Feature Branch**: `013-target-lookup-from-fits-object`  
**Created**: 2026-05-09  
**Status**: Draft  
**Input**: User description: "Specify target lookup using FITS OBJECT as a search hint, with catalog selection and manual correction."

## Implementation Status: NOT IMPLEMENTED

This feature has no implementation. Only the specification, plan, research,
data model, contracts, and tasks artifacts exist. The targeting crate
(`crates/targeting/`) referenced in the plan is a future boundary; no Rust or
TypeScript code has been written. All acceptance scenarios, user stories, and
tasks are pending review and have not been executed.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Use FITS OBJECT As Target Hint (Priority: P1)

As a user importing lights, I want the app to use the FITS `OBJECT` keyword as a target lookup hint so that target suggestions start from capture metadata.

**Why this priority**: Users expect light frames to contain target hints and do not want mandatory catalog downloads.

**Independent Test**: Import light frames with `OBJECT=M31` and confirm the target suggestion uses that value.

**Acceptance Scenarios**:

1. **Given** a light frame has `OBJECT`, **When** metadata is extracted, **Then** the target lookup uses that value as the initial query.
2. **Given** multiple object values exist in one session, **When** suggestions are shown, **Then** the app warns and asks the user to choose.

---

### User Story 1a - Resolve OBJECT Against Catalog (Priority: P1)

As a user importing lights, I want the FITS `OBJECT` value matched against a
local target catalog (Messier, Caldwell, Sharpless 2, Abell PN, Abell galaxy
clusters, Arp, van den Bergh, Barnard, LBN, LDN, Melotte, common names, and
OpenNGC) so that an exact match becomes a confirmed target identity without
manual lookup.

**Why this priority**: An exact catalog match is the most common case and
delivers the headline value of automatic target identification.

**Independent Test**: Import frames with `OBJECT=M31`, `OBJECT=NGC224`, and
`OBJECT=Andromeda Galaxy` and confirm each resolves to the same target identity
with high confidence.

**Acceptance Scenarios**:

1. **Given** `OBJECT=M101`, **When** lookup runs, **Then** the system returns a
   single match with the canonical designation `M 101` and high confidence.
2. **Given** `OBJECT=NGC5457`, **When** lookup runs, **Then** the same target
   identity as the M101 case is returned via the catalog alias.
3. **Given** `OBJECT=Pinwheel Galaxy`, **When** lookup runs, **Then** the popular
   name resolves to the same target identity.

---

### User Story 2 - Select Catalog Suggestions (Priority: P2)

As a user, I want target suggestions constrained by selected catalogs so that lookup results match my preferred naming sources.

**Why this priority**: Catalog preferences differ and should not require every catalog to be downloaded locally.

**Independent Test**: Enable Messier and NGC catalogs and verify suggestions prioritize those catalogs.

**Acceptance Scenarios**:

1. **Given** catalog lookup is enabled, **When** the app can connect to Sesame/SIMBAD, **Then** it searches with selected catalog preferences.
2. **Given** lookup is unavailable, **When** the user reviews the target, **Then** manual target entry remains available.

---

### User Story 2a - Fuzzy Match For Variant Spellings (Priority: P2)

As a user with capture software that writes inconsistent `OBJECT` values, I
want the lookup to tolerate spacing, casing, and punctuation variants so that
near-matches still resolve to the right target.

**Why this priority**: Variant spellings are common but not universal; fuzzy
match is required for usable coverage without being core to MVP.

**Independent Test**: Import frames with `OBJECT=m 101`, `OBJECT=ngc-5457`,
`OBJECT=pinwheel`, and `OBJECT=M101 LRGB` and confirm all return the M101
target identity with a confidence below the exact-match tier.

**Acceptance Scenarios**:

1. **Given** an `OBJECT` value with extra whitespace or punctuation, **When**
   lookup runs, **Then** the system returns the best match with `medium` or
   `high` confidence and surfaces the evidence used.
2. **Given** a value containing the catalog designation plus extra tokens (for
   example `M101 LRGB`), **When** lookup runs, **Then** the catalog token still
   resolves to the target identity and the extra tokens are recorded as
   evidence.

---

### User Story 3 - Unresolved And Ambiguous Fallback (Priority: P3)

As a user, I want unresolved or ambiguous `OBJECT` values to fail gracefully so
that the ingestion flow is never blocked by lookup gaps.

**Why this priority**: Edge cases are real but rare; they must not block the
P1 happy path.

**Independent Test**: Import frames with `OBJECT=Light`, `OBJECT=Target`, and a
deliberately ambiguous alias and confirm the lookup returns an unresolved or
ambiguous result with options for manual entry.

**Acceptance Scenarios**:

1. **Given** a generic value such as `Light`, **When** lookup runs, **Then** the
   system returns `unresolved` and offers manual target entry.
2. **Given** an ambiguous alias matching multiple catalog entries, **When**
   lookup runs, **Then** the system returns `ambiguous` with the candidate
   matches ranked by confidence.
3. **Given** the local catalog is unavailable, **When** lookup runs, **Then**
   the system returns `catalog.unavailable` and ingestion is not blocked.

---

### Edge Cases

- Missing `OBJECT`.
- Generic `OBJECT` values such as "Light" or "Target".
- Multiple targets in one folder.
- Network unavailable.
- Ambiguous catalog aliases.

### Domain Questions To Resolve

- Which online provider is canonical for v1 target lookup?
- How should selected catalog preferences influence ranking versus filtering?

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Light frame metadata extraction MUST preserve the raw FITS `OBJECT` value.
- **FR-002**: Target lookup MUST use FITS `OBJECT` as a hint, not as an automatic final target.
- **FR-003**: Users MUST be able to manually select or correct the target.
- **FR-004**: The active catalog set is the thirteen v1 catalogs downloaded via spec 014 (`catalog.download` flow): Messier, Caldwell, Sharpless 2, Abell PN, Abell galaxy clusters, Arp, van den Bergh, Barnard, LBN, LDN, Melotte, common names, and OpenNGC. The active set is server-derived from spec 018 settings at request time; callers cannot override per-request (R8).
- **FR-005**: After the first-run catalog download completes (spec 003 + spec 014), lookup MUST work offline. If the catalog data is not yet installed (first-run not yet completed), lookup returns `catalog.not_installed`.
- **FR-006**: Lookup failures MUST be non-blocking and logged.

### Key Entities

- **Object Hint**: Raw metadata value used for lookup.
- **Target Suggestion**: Candidate target name, identifiers, and coordinates.
- **Catalog Preference**: Active catalog set read from spec 018 settings; server-derived, not caller-supplied.
- **CatalogEquivalence**: Cross-catalog row asserting that two catalog entries refer to the same physical object (e.g. M31 ≡ NGC 224). See `data-model.md`.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Common target names in FITS `OBJECT` produce usable suggestions.
- **SC-002**: Users can correct target suggestions before project creation.
- **SC-003**: Lookup can be skipped or unavailable without blocking ingestion.

## Assumptions

- Target lookup uses online providers when available.
- FITS metadata extraction exists before target lookup is finalized.

## Out of Scope

- Full observing-plan authoring.
- Mandatory local catalog cache.
