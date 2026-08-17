# Feature Specification: SIMBAD Resolver Caching, Dual-Lookup, and Cone-Search

**Feature Branch**: `052-simbad-caching-dual-lookup-cone-search`

**Created**: 2026-07-12

**Status**: Implemented (post-hoc record, verified 2026-07-28). The resolver
facade, dual TAP/Sesame lookup, oid recovery, normalize choke-point, seed-builder
`v_mag` column, cache-clear command, and cone search all ship at HEAD. The
remaining unchecked boxes in `tasks.md` are legacy paper: `T002` asks to bump
`simbad-resolver` 0.1.3 to 0.2.0, and the workspace is on 0.5.0. Task state for
this spec lives in beads, not `tasks.md`. Outstanding process gates (speckit-verify
run, Windows verification) are tracked separately.

**Input**: User description: "SIMBAD resolver: persistent resolve cache, in-use-gated persistence, dual lookup (catalogue-first with a name-resolver fallback), and cone-search from plate-solved coordinates to suggest a target at Inbox ingest."

## Overview

PlateVault resolves a FITS `OBJECT` value or a typed query into a **canonical target** (stable identity, ICRS coordinates, object type, aliases) using a bundled seed catalogue plus SIMBAD (spec-035). Three gaps remain, each shippable on its own:

1. **The resolve cache is not durable across restarts, and typing pollutes the system-of-record.** Spec-035 wrote a `canonical_target` row for every resolution, including transient typeahead. That both persists objects the user never adopts and, because the working cache is in-process, re-queries SIMBAD after every restart. This feature makes the resolve cache **persistent** and moves durable persistence to the moment a target is actually **in use** — added to a project, linked to an acquisition session, favourited, or confirmed in the Inbox — leaving pure search backed only by the cache. It also unifies typeahead/search over a single store and enriches adopted targets with magnitude and constellation.

2. **A single-source lookup misses objects SIMBAD only knows by name.** The current path queries SIMBAD's tabular service (TAP) only; objects reachable through SIMBAD's name resolver but not the TAP identifier path resolve to nothing. This feature adds a **dual lookup**: TAP first, a name-resolver **fallback** only on a TAP miss, fired only on an explicit resolve (not per keystroke).

3. **Plate-solved frames carry their sky pointing, but the app never uses it to suggest a target.** When a frame has been plate-solved (WCS) — or at least records mount pointing — the app knows where the telescope was aimed but still asks the user to name the target by hand. This feature adds a **cone-search**: at Inbox ingest, per light-frameset, it searches the neighbourhood of the frame's pointing and offers a **suggested** target link, carrying explicit confidence, that the user confirms.

Consistent with PlateVault's principles, nothing here processes images, and no target link is ever created without an explicit user action. SQLite remains the durable system-of-record; the resolve cache is a reproducible projection.

## Clarifications

### Session 2026-07-12

Decisions below were resolved with the user in a pre-spec grilling pass; recorded here so the spec is self-contained.

- Q: One spec or three? → A: One feature spec, three phased shippable deliverables (P1/P2/P3), each an independently-testable slice. Greenfield — no data migration.
- Q: Where does the resolve cache live and does it expire? → A: A single persistent cache file in the app-data dir (SIMBAD identities are universal, not per-library), **no TTL** (identities are stable), plus a manual "clear resolve cache" action.
- Q: When is a `canonical_target` row written? → A: Only when a target becomes **in use** — added to a project, linked to an acquisition session, favourited, or confirmed in the Inbox. Pure typeahead/search populates the cache only. **This supersedes spec-035 FR-006.**
- Q: How does online resolution find objects TAP misses? → A: TAP-first, name-resolver **fallback only** on a TAP miss, and only on an explicit resolve (Enter / confirm / "search harder") — never per typeahead keystroke.
- Q: Where do cone-search coordinates come from? → A: Plate-solved WCS pointing (high confidence) → mount pointing (medium) → none (no suggestion). **Never from the filename.**
- Q: How is a suggestion chosen and presented? → A: Confidence combines separation-from-centre, coordinate-source quality (WCS > mount), and catalogue prominence. High confidence → pre-selected; low → shown but not pre-selected. **Never silently auto-applied** — the user confirms.
- Q: What happens in multi-object frames? → A: Primary object = nearest-to-centre, tie-broken by prominence, with a default exclusion set of niche object types (double/multiple stars, etc.) that the user can override.
- Q: Online features offline? → A: Dual-lookup and cone-search require network and are gated by the existing online-resolve setting; offline degrades to seed + cache, and cone-search is unavailable.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Fast offline-first search with durable identity only for adopted targets (Priority: P1)

As a user searching for and selecting targets, I want search to be instant and to survive restarts without re-querying SIMBAD, and I want the app to record a durable target identity only for objects I actually use, so that my library is not cluttered with objects I merely typed and repeated searches are free.

**Why this priority**: This is the foundation. Persistent caching and in-use-gated persistence change the storage semantics every other phase builds on, and deliver immediate value on their own: faster search, a clean system-of-record, and enriched detail (magnitude, constellation) for adopted targets.

**Independent Test**: Search for a catalogued object, restart the app, and search again — the second search returns instantly with no network call. Confirm no `canonical_target` row exists for a searched-but-unadopted object, then add it to a project and confirm exactly one durable row now exists, carrying magnitude and constellation where the source provides them.

**Acceptance Scenarios**:

1. **Given** an object resolved once (from seed or online), **When** the app is restarted and the same object is searched again, **Then** it resolves instantly from the persistent cache with zero network calls.
2. **Given** a user types a query and browses suggestions but adopts nothing, **When** the search session ends, **Then** no `canonical_target` row is created for the browsed objects (cache only).
3. **Given** a searched object, **When** the user adds it to a project, links it to an acquisition session, favourites it, or confirms it in the Inbox, **Then** exactly one durable `canonical_target` row is created (or reused) for that physical object.
4. **Given** a target that has just become in use, **When** it is viewed, **Then** it shows its visual magnitude and IAU constellation when the source provides them.
5. **Given** the same physical object reached through two different alias variants or catalogues, **When** both are searched or adopted, **Then** they map to a single canonical identity (no split).
6. **Given** a populated persistent cache, **When** the user invokes "clear resolve cache", **Then** the cache is emptied and can be re-warmed from the bundled seed and existing durable targets without losing any `canonical_target` row.

---

### User Story 2 - Resolve objects SIMBAD only knows by name (Priority: P2)

As a user resolving an unusual designation or a name the tabular catalogue does not carry, I want the app to fall back to SIMBAD's name resolver when its primary lookup misses, so that objects I can find on SIMBAD's website also resolve in PlateVault.

**Why this priority**: Extends resolution coverage for real objects that the TAP-only path silently drops. Depends on P1's cache and normalization so a fallback hit is cached and deduplicated like any other. Standalone value: fewer "unresolved" dead ends.

**Independent Test**: Resolve a designation that the tabular service does not carry but the name resolver does; confirm it resolves on an explicit resolve action (not during typeahead), is written to the cache, and deduplicates to a single identity when its aliases are searched.

**Acceptance Scenarios**:

1. **Given** a designation absent from the tabular (TAP) path, **When** the user triggers an explicit resolve (Enter / confirm / "search harder"), **Then** the app falls back to the name resolver and returns the object.
2. **Given** the same query during as-you-type suggestions, **When** the user is still typing, **Then** the name-resolver fallback does NOT fire (only the local cache/seed answers typeahead).
3. **Given** a name-resolver hit that lacks a stable physical-object id, **When** it is resolved, **Then** the app recovers an id by re-enriching from the tabular path, and if none exists still produces a single stable identity so aliases of the same object do not split.
4. **Given** the online-resolve setting is disabled, **When** a resolve is attempted, **Then** neither the tabular path nor the fallback is used and resolution degrades to seed + cache.

---

### User Story 3 - Suggested target from plate-solved coordinates at Inbox ingest (Priority: P3)

As a user ingesting light frames that were plate-solved (or that record mount pointing), I want PlateVault to suggest the target the frames were aimed at, with a clear confidence, so that I can confirm it in one click instead of typing the object by hand — while never having a target silently assigned for me.

**Why this priority**: The headline convenience, and the most complex slice. Depends on P1 (a resolvable, cached, enriched catalogue) and benefits from P2 (broader coverage). Independent once resolution exists: it adds a coordinate-driven suggestion to the existing Inbox confirm gate.

**Independent Test**: Ingest a plate-solved light-frameset and confirm a high-confidence, pre-selected target suggestion appears; ingest a frameset with only mount pointing and confirm a lower-confidence, shown-but-not-pre-selected suggestion; ingest a frameset with neither and confirm no suggestion appears. In every case, confirm no target link exists until the user confirms one.

**Acceptance Scenarios**:

1. **Given** a plate-solved light-frameset (WCS pointing), **When** it reaches the Inbox confirm gate, **Then** the app runs a cone-search around the pointing and pre-selects a high-confidence target suggestion.
2. **Given** a frameset with only mount pointing (no plate solve), **When** it is ingested, **Then** a suggestion is shown at reduced confidence and is NOT pre-selected.
3. **Given** a frameset whose only clue is the filename, **When** it is ingested, **Then** no coordinate-based suggestion is produced (the filename is never used as a pointing source).
4. **Given** a frame whose field contains several catalogued objects, **When** a primary object is chosen, **Then** it is the object nearest the field centre, tie-broken by catalogue prominence, excluding niche object types in the default exclusion set (which the user can override).
5. **Given** any suggestion, **When** the user does nothing, **Then** no target link is created; **When** the user confirms it, **Then** the target link is created, the target becomes in use, and its `canonical_target` row is persisted (per P1).
6. **Given** a frameset already ingested, **When** the user requests it, **Then** the cone-search can be re-run on demand.
7. **Given** the online-resolve setting is disabled or the network is unavailable, **When** a frameset is ingested, **Then** cone-search is unavailable and ingest proceeds without a coordinate-based suggestion.

---

### Edge Cases

- **Offline / online-resolve disabled**: search and ingest degrade to seed + cache; the name-resolver fallback and cone-search are unavailable; no error blocks the user.
- **Unknown optics**: when focal length / sensor are unknown, the cone-search radius falls back to a documented default (~1°) rather than failing.
- **Field rotation**: the frame footprint used for matching accounts for camera rotation; a rotated field must not drop objects that fall inside the true footprint.
- **Name-resolver hit without a physical-object id**: identity is recovered by re-enrichment, else a stable synthetic identity preserves alias deduplication.
- **Cache vs seed vs durable-target conflict**: a single normalized identity per physical object wins; the durable `canonical_target` (system-of-record) is authoritative over the cache projection.
- **Ambiguous multi-object frame with all candidates excluded**: if every in-field object is in the exclusion set, no primary is pre-selected; candidates may still be shown for manual choice.
- **Clearing the cache**: removes only the reproducible projection; it never deletes a `canonical_target` row, and the cache re-warms from seed + durable targets.
- **Subs disagree on pointing**: a frameset whose subs disagree on pointing beyond a tolerance is treated as no reliable pointing rather than guessing a centre.

## Requirements *(mandatory)*

### Functional Requirements

**Persistent cache, in-use persistence, enrichment, normalization (US1)**

- **FR-001**: Typeahead and search MUST be served from a single unified store (bundled seed + persistent resolve cache + adopted durable targets); the system MUST NOT maintain a separate hand-rolled search path alongside it.
- **FR-002**: The resolve cache MUST be durable across application restarts, stored once globally in the app-data directory (SIMBAD identities are universal, not per-library), with **no expiry** (identities are stable) and a user-invokable "clear resolve cache" action.
- **FR-003**: SQLite `canonical_target` MUST remain the durable, foreign-key-anchored system-of-record; the resolve cache MUST be treated as a reproducible projection over it plus the bundled seed and online responses.
- **FR-004** *(SUPERSEDES spec-035 FR-006)*: A durable `canonical_target` row MUST be written only when a target becomes **in use** — added to a project, linked to an acquisition session, favourited, or confirmed in the Inbox. Pure typeahead/search MUST populate only the resolve cache and MUST NOT create `canonical_target` rows. (Reason: spec-035 FR-006 made the cache the durable record and persisted on every resolution, cluttering the system-of-record with un-adopted objects; persistence now follows adoption.)
- **FR-005**: At first run, the system MUST warm the persistent cache from the bundled seed, and MUST lazily incorporate existing durable `canonical_target` rows, so search works offline immediately.
- **FR-006**: When a target becomes in use (or is resolved online), the system MUST populate its visual magnitude and its IAU constellation when the source provides them; absence MUST be tolerated (both remain optional).
- **FR-007**: All identity strings (designations, aliases, user query, seed, tabular, and name-resolver results) MUST pass through a single normalization choke-point before being cached, persisted, or matched, applied identically across every source. Deduplication MUST key on the physical-object id first and the normalized primary designation second, and each alias MUST be normalized.

**Dual lookup (US2)**

- **FR-008**: Online resolution MUST query the tabular (TAP) path first and MUST consult the name-resolver fallback only when the tabular path returns no match.
- **FR-009**: The name-resolver fallback MUST fire only on an explicit resolve action (Enter, confirm, or "search harder"); it MUST NOT fire during as-you-type suggestions.
- **FR-010**: A name-resolver hit lacking a stable physical-object id MUST be re-enriched via the tabular path (by coordinates or main identifier) to recover one; when none can be recovered, the system MUST assign a stable synthetic identity derived from the normalized designation so aliases of the same object are not split.
- **FR-011**: Both online paths MUST be gated by the existing online-resolve setting; when disabled, resolution MUST use only seed + cache.

**Cone-search suggestion at Inbox ingest (US3)**

- **FR-012**: For a light-frameset at Inbox ingest, the system MUST derive a sky pointing from plate-solved WCS pointing (high confidence) when present, else from mount pointing (medium confidence), else produce no suggestion. The system MUST NOT derive pointing from the filename.
- **FR-013**: The system MUST perform a cone-search around the derived pointing, sized from the frame field of view (from focal length and sensor) with a documented default radius (~1°) when optics are unknown, accounting for camera rotation in the field footprint, and MUST consider the top-N in-field candidates.
- **FR-014**: Each suggestion MUST carry an explicit confidence combining separation from the field centre, coordinate-source quality (WCS > mount), and catalogue prominence. High-confidence suggestions MUST be pre-selected; low-confidence suggestions MUST be shown but not pre-selected. The system MUST NEVER silently auto-apply a target link.
- **FR-015**: For a frame containing multiple catalogued objects, the primary object MUST be the object nearest the field centre, tie-broken by catalogue prominence, excluding object types in a default exclusion set that the user can override.
- **FR-016**: A suggestion MUST be advisory only; the durable target link MUST be created only on explicit user confirmation, at which point the target becomes in use and its `canonical_target` row is persisted (FR-004).
- **FR-017**: Cone-search MUST run per light-frameset at ingest and MUST be re-runnable on demand for an already-ingested frameset.

**Cross-cutting**

- **FR-018**: Dual-lookup (US2) and cone-search (US3) require network access and MUST be gated by the online-resolve setting; when offline, search and ingest MUST degrade to seed + cache and cone-search MUST be unavailable, without blocking the user.

### Key Entities *(include if feature involves data)*

- **Canonical target (durable)**: The existing system-of-record identity (stable id, physical-object id, primary designation, ICRS coordinates, object type, aliases), now also carrying visual magnitude and IAU constellation for in-use targets. Written only on adoption (FR-004).
- **Resolve cache entry (projection)**: A persistent, non-authoritative record of a resolved identity and its normalized aliases, keyed for instant typeahead/search, re-derivable from seed + durable targets + online responses. No expiry.
- **Pointing**: A derived sky centre for a light-frameset, with a source quality (plate-solved WCS > mount > none). Never derived from the filename.
- **Cone-search suggestion**: An advisory target candidate for a frameset, carrying the candidate canonical identity, angular separation from the field centre, and an explicit confidence; may be pre-selected (high) or shown-only (low); becomes a durable link only on confirm.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After resolving an object once, restarting the app and searching the same object issues **zero** network calls and returns from the persistent cache.
- **SC-002**: Browsing search results without adopting anything creates **zero** `canonical_target` rows; adopting a target (project / session / favourite / Inbox confirm) creates exactly one durable row for that physical object.
- **SC-003**: An in-use target displays its visual magnitude and IAU constellation whenever the resolving source provides them (0% of adopted, source-populated targets show them blank).
- **SC-004**: An object carried only by SIMBAD's name resolver (a tabular miss) resolves via the fallback on an explicit resolve, is cached, and deduplicates to a single identity across its alias variants.
- **SC-005**: A plate-solved light-frameset yields a pre-selected high-confidence suggestion; a mount-only frameset yields a shown-but-not-pre-selected suggestion; a filename-only frameset yields no coordinate-based suggestion.
- **SC-006**: No target link is ever created without explicit user confirmation (0% silent auto-apply across cone-search suggestions).

## Assumptions

- Inbox confirm remains the single ingest gate; cone-search adds a suggestion to that gate and does not create a competing ingest path.
- The published SIMBAD resolver crate provides both a tabular path and a name-resolver fallback, a persistent-file cache backend, and magnitude in its resolved identity at the target version; constellation is derived from coordinates by the pinned sky-math library.
- Frame field-of-view and footprint (with rotation) are computable from focal length and sensor via the pinned field-matching library; when optics are unknown the documented default radius applies.
- Catalogue-prominence ranking and the default niche-object-type exclusion set are open research decisions (see research.md) with proposed defaults; they are refined during Phase-3 design, not left silent.
- The online-resolve setting (spec-035 FR-015) is the single gate for all network resolution introduced here.
