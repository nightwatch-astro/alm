# Data Model: Source View Generation

**Spec**: `specs/049-source-view-generation/spec.md`
**Status**: Implemented (post-hoc record, corrected 2026-08-24) — see `spec.md`'s
status header for the per-user-story evidence.

This feature is the **generation** counterpart of spec 026 and **reuses** its
entities. It adds no new durable table. The reused entities and the delta are
below, followed by the migration verdict.

## Reused Entities (spec 026 — unchanged schema)

### PreparedSourceView (reused)

`prepared_source_views` (migration 0029). This feature adds the
**first-materialization** write: a row created in state `current` when a
generation plan applies. Fields unchanged (`id`, `project_id`, `kind`, `state`,
`created_at`, `removed_at`).

- `kind` now records the **dominant** materialization for display; per-item
  `materialization` is authoritative (spec 026 FR-008 amended 2026-07-04, CL-2).

### PreparedSourceViewItem (reused)

`prepared_source_view_items` (migration 0029). Already carries
`inventory_item_id`, `view_relative_path`, `materialization`
(`symlink | junction | copy | hardlink`), `last_observed_state`.

- **No column change needed.** The per-item recorded link kind that CL-2 requires
  is exactly the existing `materialization` column. Generation writes the
  rule-chosen kind per item here.

## New / Extended Model Elements

### View Generation Plan (new `FilesystemPlan` variant)

A `FilesystemPlan` whose `origin` discriminator is **`prepared_view_generation`**
and `plan_type` is **`source_view_generation`** (parallel to spec 026's
`prepared_view_removal` / `prepared_view_regeneration`).

| Field      | Type                                | Notes |
|------------|-------------------------------------|-------|
| `plan_id`  | `PlanId`                            | Standard plan id. |
| `origin`   | const `prepared_view_generation`    | Routes audit and review surfaces; distinct from regeneration (FR-021a). |
| `plan_type`| const `source_view_generation`      | Distinct plan type. |
| `project_id` | `ProjectId`                       | Owning project. |
| `actions`  | `Vec<PlanAction>`                   | Per-item `link` (or opt-in `copy`) + `mkdir`, resolved against current inventory paths. Actions target only paths under the destination. |
| `warnings` | `Vec<GenerationWarning>`            | `no_calibration_applied` (unmatched light groups, FR-010a), `unresolved_source` (skipped/flagged, FR-019), `capability_drift` (fallback applied, FR-004b), `long_path` (FR-018). |

`link` and `mkdir` are already valid `plan_items.action` values (migration 0029),
so no plan-item schema change is required.

### DriveScope (domain, transient)

Per-item classification computed at plan time — `intra_drive` (source volume ==
destination volume) or `cross_drive`. Not persisted; drives link-kind selection.

### LinkKind resolution rule (domain, pure)

Deterministic mapping `(DriveScope, capability) → materialization`:

- `intra_drive` → `source_view_link_kind_intra_drive` (default `hardlink`),
  falling back per capability if drift (FR-004b).
- `cross_drive` → `source_view_link_kind_cross_drive` (default `symlink`);
  `hardlink` is **never** a valid cross-drive value (FR-004a).
- No achievable kind and no copy opt-in → refuse (`view.no_link_kind`).

### Link-Kind Settings Pair (spec 018 KV — flat `SettingsState` fields)

Added to `SettingsState` (`crates/domain/core/src/settings.rs`), persisted in the
existing `settings` table (migration 0013). **No migration.**

| Key | Type | Default | Section | Overridable per source? | Notes |
|-----|------|---------|---------|-------------------------|-------|
| `source_view_link_kind_intra_drive` | `"hardlink" \| "symlink" \| "junction"` | `"hardlink"` | Source Views | No | Default kind when source and destination share a volume. |
| `source_view_link_kind_cross_drive` | `"symlink" \| "junction"` | `"symlink"` | Source Views | No | Default kind when source and destination are on different volumes. **`hardlink` is not an allowed value** (cannot cross volumes — FR-004a). |

The settings/dialog UI **greys out** any value not currently achievable (symlink
without privilege → Developer Mode guidance, FR-004c). Because saved values are
capability-constrained, the plan-time fallback (FR-004b) is a rare drift-only path.

### Per-project destination override (spec 018 KV structured key or project manifest)

Default destination `<project>/source-views/<view>/` (spec 024 envelope). The
per-project override is persisted as a structured settings/KV key (e.g.
`source_view.<project_id>.destination`) — no dedicated column, **no migration**.
Per-generation override is a request field on `sourceview.generate` (not persisted
unless it becomes the view's recorded destination).

## State Transitions (generation additions to spec 026 machine)

```
(none)   -> current   (generation plan applied successfully — NEW, this spec)
current  -> stale/removed/failed/...   (spec 026, unchanged)
```

All other transitions (stale, removed, regenerated, failed, kind_diverged) are
owned by spec 026 and reused unchanged.

## Invariants (this spec)

- Generation writes only paths under the chosen destination; no action targets an
  inventory path (Constitution I).
- Every item's `materialization` is a rule-chosen, recorded value; the view carries
  no unrecorded/non-deterministic kind (spec 026 FR-008 as amended).
- Two sources never resolve to the same destination path: collision = plan
  validation error, never a silent suffix (FR-009a/FR-017).
- The generated tree contains zero tool-control files (Constitution III / SC-002).

## Migration Verdict

**One migration required: `0061_source_view_generation_origin.sql`.**

- **Why one:** the only durable-schema change is expanding the `plans.origin` CHECK
  (add `prepared_view_generation`) and `plans.plan_type` CHECK (add
  `source_view_generation`), using the SQLite table-recreate technique from
  migrations 0019/0029/0053.
- **Why not more:** per-item recorded link kind already exists
  (`prepared_source_view_items.materialization`, 0029); settings pair rides the
  spec 018 KV `settings` table (0013); the per-project destination override is a KV
  key. No new table, no new column.
- **Number check (2026-07-04):** highest committed migration is `0053`; open PR #414
  (`fix-project-path-root-anchor`) holds `0060_project_path_anchor`; redesign holds
  `0052_registered_sources_active`. Numbers `0054`–`0059` and `0061`+ are free
  across all open branches. **Next free ≥ 0061 is `0061`.**
