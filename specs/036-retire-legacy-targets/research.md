# Research: Retire Legacy Target Tables

Phase 0 decisions for the design choices the spec/plan leave open.

## R1 — Display-alias storage

**Decision**: add a nullable `display_alias TEXT` column to `canonical_target`.

**Rationale**: a display alias is exactly one presentation label per target, owned by the
user, that must NOT participate in matching/typeahead and must survive re-resolution. A
single nullable column expresses that directly: `effective_label = display_alias ??
primary_designation`. It needs no normalization, no uniqueness, and no join.

**Alternatives considered**:
- A `target_alias` row with `kind='display'`: rejected — `target_alias` rows are
  normalized and matchable (they feed search), which is wrong for a pure display label;
  it would also pollute the alias list and the typeahead surface.
- A separate `target_display` table: rejected — over-modelled for a single optional value.

**Persistence rule (FR-012)**: `upsert_resolved` (re-resolution) MUST preserve an existing
`display_alias` on conflict (only update SIMBAD-derived fields). Greenfield lets us add
the column directly into migration 0031.

## R2 — Distinguishing user-added aliases from SIMBAD-derived

**Decision**: extend `target_alias.kind` CHECK from `('designation','common_name')` to
`('designation','common_name','user')`. User-added aliases are stored as `kind='user'`.

**Rationale**: users may add and remove their own aliases, but must not be able to remove
SIMBAD-derived designations/common names (those are canonical identity). A `kind` marker
makes removal authorization trivial (`DELETE ... WHERE kind='user'`) and keeps user
aliases visible/searchable like other aliases. Mirrors the spec-023 protection that
prevented removing the primary/identity aliases.

**Alternatives considered**: a boolean `is_user` column (equivalent, but `kind` already
exists and is the natural axis); a separate table (over-modelled).

**Normalization & dedup**: user aliases reuse the existing `normalized` + `UNIQUE
(target_id, normalized)` machinery, so duplicate adds are rejected by the DB (FR-008).

## R3 — Schema removal mechanism (greenfield)

**Decision**: edit the source migrations directly — delete the gen-2 target migrations and
surgically remove the `target` table + `acquisition_session.target_id`. No drop migration.

Reconciled 2026-08-24: the numbered chain this decision was written against no longer exists.
The schema is one editable baseline, `crates/persistence/core/migrations/0001_initial_schema.sql`
(decision bead `adr-1`), so "edit the source migration directly" now means editing that file.
The gen-2 tables are already absent from it; gen-1 `target` (`:2914`) and
`acquisition_session.target_id` (`:28`) survive, as the deferral in `plan.md` §Gen-1 records.
Any edit to the baseline changes the version-1 checksum and makes existing development
databases fail to open — see `docs/development/database-baseline-and-migrations.md`.

**Rationale**: greenfield (no production DBs, no data to preserve) means the cleanest end
state is "the legacy schema was never created." This avoids a confusing create-then-drop
history and the SQLite table-rebuild dance required to drop columns. Fresh DBs are the
only DBs.

**Risk/mitigation**: deleting a migration file is safe for fresh DBs (sqlx applies the
remaining files in order). Must confirm no later migration references the removed objects
— verified: only the deleted 0027 added the FK columns; 0031/0033 reference only gen-3.
The `acquisition_session.target_id` removal requires checking 0002 for an index/trigger
referencing it.

## R4 — Command surface continuity

**Decision**: reuse the existing dotted command names (`target.get`, `target.alias.add`,
`target.alias.remove`) repointed to gen-3; add `target.list`,
`target.display_alias.set`, `target.display_alias.clear`; remove `target.note.update`
and `target.primary.rename`.

**Rationale**: keeping the invoke names stable minimizes frontend churn and respects the
tauri-specta command-name rule (never rename an invoke target the frontend calls). The
removed names are genuinely retired features (D1, D2). The new display-alias names follow
the existing dotted convention.

**Caveat (recorded in memory `tauri-specta-command-name-mismatch`)**: the
`#[specta::specta(rename = "...")]` value must equal the registered tauri fn invoke
target the frontend calls; verify after regen.

## R5 — Inventory projection target name

**Decision**: drop the `LEFT JOIN target` and emit no target name from the inventory
projection for now.

**Rationale**: the legacy `target` table is never written by current code, so the join
already returns NULL — removing it is behaviour-preserving. Re-deriving an inventory
target name from gen-3 (via session→canonical_target) is out of scope here (no
session→canonical_target link exists yet; that's future ingest/US4 work).

## R6 — Targets list source

**Decision**: add a `target.list` command backed by a `canonical_target` list query
(id, primary_designation, display_alias, object_type), ordered by primary_designation.

**Rationale**: the Targets page list pane needs to enumerate targets; spec-035 only built
query-driven search, not a full list. A simple ordered local query suffices at this scale.
