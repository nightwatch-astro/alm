# Data Model: Target Identity, History, And Notes

**Spec**: 023-target-identity-history-notes | **Date**: 2026-05-20

## Target

```
Target {
  id:                   Uuid           // UUIDv5 from canonical_designation per spec 013 R6
  primary_designation:  String         // canonical display name (e.g. "M 31"); R-1.3
  aliases:              String[]       // user nicknames + normalized variants
  catalog_refs:         CatalogRef[]   // structured catalog identifiers (R-1.4)
  notes?:               String         // per-target free text (R4); max 16 KB UTF-8 (A6)
  created_at:           Timestamp
  updated_at:           Timestamp      // bumped on any field change
}
```

```
CatalogRef {
  catalog_id:      String              // closed enum slug: messier|caldwell|sharpless|
                                       // abell_pn|abell_galaxies|arp|vdb|barnard|
                                       // lbn|ldn|melotte|common|openngc (R-1.4)
  catalog_display: String              // human-readable name (e.g. "Messier", "OpenNGC")
  designation:     String              // e.g. "M31", "NGC 224", "Sh2-155"
}
```

### Invariants

- `primary_designation` is non-empty; trimmed; unique within the targets table.
- `aliases[*]` is normalized (R2): trimmed, internal whitespace collapsed,
  compared case-insensitively. The stored form preserves user casing for
  display.
- A normalized alias appears on at most one target. Re-adding the same alias
  to its current target is a no-op success; adding it to a different target
  returns `alias.duplicate`.
- Removing an alias that is also `primary_designation` returns `alias.is_primary`.
- Renaming primary: `newPrimaryDesignation` MUST already exist in `aliases[]`;
  otherwise returns `designation.not_in_aliases`.
- `catalog_refs[*]` is unique on `(catalog_id, designation)` per target.
- `notes` is optional; absence and empty string are equivalent; max 16 KB
  UTF-8 bytes (A6).
- `updated_at >= created_at`.

## TargetSession (join view)

A read-only projection joining `sessions` to `targets` for the history list.

```
TargetSession {
  session_id:    Uuid
  captured_on:   Date | null           // night of acquisition (R3); null when
                                       // observer_location is null/unreviewed (R-3.1)
  filter?:       String
  exposure?:     String                // e.g. "300s x 24"
  frames?:       u32
  inventory_id:  Uuid                  // deep link target
}
```

Ordered reverse-chronologically by `captured_on`. Sessions with
`captured_on = null` are excluded from the target history response until
`AcquisitionSession.observer_location` is reviewed (R-3.1).

## TargetProject (join view)

A read-only projection joining `projects` to `targets`.

```
TargetProject {
  project_id:    Uuid
  name:          String
  lifecycle:     ProjectLifecycle      // $ref spec 002/009 canonical enum (E6)
  tool:          ProcessingTool        // REQUIRED — v1 projects always have a tool at create (spec 008 R-Tool-Req)
}
```

**OVERRIDE of spec 023 R-3.3 (GRILL 2026-05-22)**: `tool` is now REQUIRED on
`TargetProject`. The prior R-3.3 decision ("`tool` optional; null for
`setup_incomplete` projects") is explicitly overridden. Spec 008 ratified
R-Tool-Req, which makes `tool` a mandatory field at project creation time. No
v1 project can exist without a tool. The `setup_incomplete` state is only for
missing confirmed sources, not for missing tool. UI no longer needs to handle
`tool = null`.

## History Aggregate (returned by `target.get`)

```
TargetDetail {
  target:    Target
  sessions:  TargetSession[]           // ordered desc by captured_on
  projects:  TargetProject[]           // ordered by lifecycle then name
}
```

## Storage Notes

- Persisted by `crates/persistence/targets`. Table sketch:
  - `target(id, primary_designation, notes, created_at, updated_at)`
  - `target_alias(target_id, alias_normalized, alias_display)` with UNIQUE
    on `alias_normalized`.
  - `target_catalog_ref(target_id, catalog_id, catalog_display, designation)` with UNIQUE on
    `(target_id, catalog_id, designation)`.
- `sessions.target_id` (FK to `target.id`) is set by spec 013 confirmation.
- `projects.target_id` (FK to `target.id`) is set when a project is created
  against a target or when a source resolves to one.
- Indexes: `target_alias.alias_normalized` (lookup for Cmd+K and conflict
  check), `sessions.target_id + captured_on DESC` (history view),
  `projects.target_id` (projects-per-target view).

## Derived Names

- Cmd+K matches both `primary_designation` and any `alias_normalized` row.
- Target chip on Inventory and Project rows renders `primary_designation`.
