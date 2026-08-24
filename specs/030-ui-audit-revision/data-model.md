# Data Model: UI Audit & Revision

Changes to the data model required by the UI redesign. Most changes are in
settings/configuration domain, not core domain entities.

## Source Folder Type Unification

The source folder kind enum is a 4-value set used only as a user-facing folder
category in the setup wizard and settings. It does NOT determine per-image frame
type; that is detected from image metadata (FITS `IMAGETYP` header) during
scan/ingest.

```
SourceFolderKind:
  - light_frames    (was: raw)
  - calibration     (covers darks, flats, and bias — kind collapsed from 6→4)
  - project
  - inbox
```

Migration notes:
- `raw` → `light_frames` (migration 0010).
- `dark`, `flat`, `bias` → `calibration` (migration 0032). Where the same path
  was registered under multiple of these kinds, only the earliest entry is kept
  (INSERT OR IGNORE by `created_at ASC`) and duplicates are silently dropped.
- Per-image frame type (light / dark / flat / bias) remains authoritative from
  FITS `IMAGETYP` metadata, so no information is lost by collapsing the folder
  kind.

## Equipment Entities

New entities separate from the existing detected-equipment display in
calibration matching. These are user-managed configuration.

### Camera

| Field | Type | Notes |
|-------|------|-------|
| id | UUID | Primary key |
| name | String | Display name (e.g., "ASI2600MM Pro") |
| aliases | String[] | Alternative names from FITS INSTRUME |
| auto_detected | Boolean | True if discovered from FITS headers |

### Telescope

| Field | Type | Notes |
|-------|------|-------|
| id | UUID | Primary key |
| name | String | Display name (e.g., "Esprit 100ED") |
| aliases | String[] | Alternative names from FITS headers |
| focal_length_mm | Integer? | Native focal length |
| auto_detected | Boolean | True if discovered from FITS headers |

### OpticalTrain

| Field | Type | Notes |
|-------|------|-------|
| id | UUID | Primary key |
| name | String | User-defined or auto-generated from telescope+camera |
| telescope_id | UUID? | FK to Telescope (nullable for custom) |
| camera_id | UUID? | FK to Camera (nullable for custom) |
| focal_length_mm | Integer | Effective focal length (may differ from telescope native) |

### Filter

| Field | Type | Notes |
|-------|------|-------|
| id | UUID | Primary key |
| name | String | Display name (e.g., "Ha", "SII") |
| category | Enum | narrowband / broadband / dual_band / other / custom |
| auto_detected | Boolean | True if discovered from FITS metadata |

Predefined filters seeded on first run: Ha, SII, OIII, NII, L, R, G, B, HO,
SO, UV/IR Cut.

## Processing Tool Configuration

### ProcessingTool

| Field | Type | Notes |
|-------|------|-------|
| id | String | Tool identifier (pixinsight, siril) |
| enabled | Boolean | User toggle |
| executable_path | String? | Path to executable, validated |
| processing_dir | String | Relative to project root (default: "processing/") |
| output_dir | String | Relative to project root (default: "outputs/") |
| directory_template | JSON | Per-folder structure definition |

### DirectoryTemplate

JSON structure defining the project folder layout the app creates. The app
creates folder-level junctions/symlinks — one per session, not per file.

```json
// PixInsight/WBPP
{
  "lights_dir": "Lights/",
  "calibration_dir": "Calibration/",
  "calibration_dark": "Dark/",
  "calibration_flat": "Flat/",
  "calibration_bias": "Bias/",
  "processing_dir": "processing/",
  "outputs_dir": "outputs/",
  "notes_dir": "notes/"
}

// Siril
{
  "lights_dir": "lights/",
  "calibration_dir": "calibration/",
  "calibration_dark": "darks/",
  "calibration_flat": "flats/",
  "calibration_bias": "biases/",
  "processing_dir": "process/",
  "outputs_dir": "outputs/",
  "notes_dir": "notes/"
}
```

Lights and flats are sub-grouped by filter (e.g., `Lights/Ha/`). Each
session junction folder is prefixed with `DATE_` for lights and flats to
enable WBPP custom grouping (e.g., `DATE_2024-11-30/`). Darks and bias
use descriptive names without the DATE_ prefix (e.g., `300s_-10C/`).

See research.md R3 for the full directory structure diagram.

## Cleanup Policy

### CleanupAction (per data type)

| Field | Type | Notes |
|-------|------|-------|
| data_type | Enum | See list below |
| action | Enum | keep / archive / delete |

Data types:
light_frames, dark_subs, flat_subs, bias_subs, calibration_masters,
registered_frames, calibrated_frames, debayered_frames, local_normalization,
drizzle_data, integration_cache, stack_output_intermediate, temporary_files,
process_logs, process_icons_tool_config

### CleanupTrigger

| Field | Type | Notes |
|-------|------|-------|
| mode | Enum | manual / auto_on_completion |

## Calibration Matching Tolerances

New settings (currently hardcoded or absent):

| Setting | Type | Default |
|---------|------|---------|
| temperature_tolerance_c | Float | 5.0 |
| exposure_tolerance_s | Float | 2.0 |
| aging_threshold_days | Integer | 365 |
| require_same_gain | Boolean | true |
| require_same_binning | Boolean | true |

## Ingestion Settings

Moved from data sources to ingestion config:

| Setting | Type | Default |
|---------|------|---------|
| watcher_enabled | Boolean | true |
| scan_on_startup | Boolean | true |
| follow_symlinks | Boolean | false |
| follow_junctions | Boolean | false |
| hashing_mode | Enum | lazy |
| metadata_extraction | Enum | fits_xisf_sidecar |
| default_filter | String | "L" |
| exposure_group_margin_s | Float | 2.0 |
| temperature_group_tolerance_c | Float | 5.0 |

## Project Lifecycle State

Simplify from 6 states to 5:

```
Before: setup → ready → prepared → processing → completed → archived
After:  setup → ready → processing → completed → archived
```

The `prepared` state is removed. Generating source views transitions directly
from `ready` to `processing`.

## Source View Strategy

Simplify from 6 options to 4:

```
Before: manifest_only, symbolic_links, ntfs_junctions, hard_links, full_copy, hybrid
After:  symbolic_links, ntfs_junctions, hard_links, full_copy
```

## Status Bar Data

Not a persisted entity — aggregated at runtime:

| Field | Source |
|-------|--------|
| inbox_pending_count | Count of unconfirmed inbox sessions |
| library_file_count | Total indexed files across all roots |
| library_size_bytes | Total size of indexed files |
| cleanup_reclaimable_bytes | Sum across all completed projects |
| volume_health | Per-volume: path, total_bytes, free_bytes, warning (free < 10%) |
| root_health | Per-root: path, online/offline status |

## Catalog List

Expanded from current:

| Catalog | Object Count |
|---------|-------------|
| Messier | 110 |
| NGC / IC | ~13,000 |
| Caldwell | 109 |
| Sharpless (Sh2) | 313 |
| Abell Planetary Nebulae | 86 |

Each catalog has: downloaded (boolean), enabled (boolean), last_synced (date).

## Audit Entry — Generalized Mutation Record

*(Iteration 2026-07-14, grilling Q15 / #647.)* The durable audit entry
generalizes from a lifecycle-transition record to a generic mutation record.
Target shape (constitution §II fields):

| Field | Meaning |
|-------|---------|
| timestamp | When the mutation was attempted |
| actor | user \| system |
| action | What was attempted (generalizes the lifecycle `trigger`) |
| entity (type + id) | What was mutated — extends beyond the lifecycle `EntityType` enum to settings, protection, equipment, sources, and roots |
| outcome + reason | applied \| refused \| failed, with a reason/code as first-class queryable detail |
| before → after (optional) | Value pair for settings/protection changes |

Mapping onto the existing `audit_log_entry` table
(`crates/persistence/core/migrations/0001_initial_schema.sql:130-143`):

- `at` → timestamp; `actor` unchanged; `trigger` → action.
- `from_state` / `to_state` are subsumed by the optional before→after pair
  (lifecycle transitions keep using them; non-lifecycle mutations carry
  before→after in the structured `payload` JSON).
- reason/code becomes first-class queryable detail as a concrete column,
  not a JSON1 query over `payload`.
- `severity` (workflow | diagnostic) and `request_id` are retained.

### Migration shape (T120)

Consistent with existing schema conventions (TEXT columns, named
`idx_audit_*` indexes; `crates/persistence/core/migrations/0001_initial_schema.sql:2990-3007`):

- Add nullable column `reason_code TEXT` — the machine-readable
  reason/code for `refused`/`failed` outcomes; NULL for `applied`.
  Human-readable detail stays in `payload`.
- Add index `idx_audit_outcome` on `(outcome, reason_code)` for
  refusal/failure queries.
- No other table changes: `entity_type` and `trigger` have no CHECK
  constraints, so the enum generalization is DB-free; the real surface is
  the Rust `EntityType` enum and its generated TS union.

Resulting column set: `audit_id, entity_type, entity_id, from_state,
to_state, trigger, actor, outcome, severity, request_id, at, payload,
reason_code`.

### Severity per mutation class

`severity` drives FR-132's user-meaningful filter. Assignment rule:
user-initiated mutations are `workflow`; system-initiated ones are
`diagnostic`.

| Mutation class | Severity |
|----------------|----------|
| Settings changes (durable-data keys) | workflow |
| Protection overrides/acknowledgements | workflow |
| Equipment CRUD | workflow |
| Source register/delete/enable/disable | workflow |
| User-initiated rescans / root ops (incl. remap) | workflow |
| Automatic/periodic rescans, system maintenance | diagnostic |

The live event bus stream is unchanged in shape. Audit-worthy mutations
write the `audit_log_entry` row and emit the bus event; `audit_log_entry`
is the authoritative audit record, while the bus's durable `events` table
is non-authoritative transient diagnostics (prunable). The resulting dual
durable rows are accepted in v1 (spec §8.3).

## Metadata Value States

*(Iteration 2026-07-14, grilling Q16 / #620.)* Every displayed metadata
field carries one of three modeled states (FR-135):

| State | Model representation |
|-------|---------------------|
| Real value (incl. real 0) | The value itself |
| Unresolved / missing | null/None — end-to-end, never a sentinel 0 |
| Not-applicable | Determined by the entity/frame-type model (which fields apply to which entity kind), never inferred from data absence |

**Null end-to-end rule (FR-136)**: nullable DB columns → `Option` app-layer
types → nullable contract DTO fields → `null` in the UI. No hop may
substitute a sentinel (0, empty string, epoch date) for absence.

Known offenders to fix first:

1. **Fingerprint gain/exposure** — `CalibrationFingerprint.exposure_s` /
   `gain` are non-optional `f64` in the contract
   (`crates/contracts/core/src/calibration.rs:96,99`), forcing the app
   layer to collapse the nullable persistence row
   (`crates/persistence/db/src/repositories/q_calibration.rs:93-94`) with
   `unwrap_or(0.0)`
   (`crates/app/calibration/src/matching.rs:739,741,794,796`) even though
   the extraction model is already `Option`-typed
   (`crates/metadata/core/src/lib.rs:221,223`). These fields become
   nullable.
2. **Master size** — the zero lives in SQL, not the mapping layer:
   `calibration_master_view` supplies no real size (as of 2026-08-24 it selects
   `CAST(NULL AS INTEGER) AS size_bytes` at `crates/persistence/core/migrations/0001_initial_schema.sql:3186`;
   it hardcoded `0 AS size_bytes` when this was written), flowing through the
   non-nullable row field
   (`crates/persistence/db/src/repositories/q_calibration.rs:92`
   `size_bytes: i64`) into the non-optional contract fields
   (`crates/contracts/core/src/calibration.rs` `CalibrationMaster.size_bytes`
   / `MasterDetail.size_bytes: u64`). The `unwrap_or(0)` at
   `matching.rs:748,803` is only a sign-conversion fallback — removing it
   changes nothing. Fix requires a **view-redefinition migration** (emit
   NULL or a computed size), `Option<i64>` on the row, and nullable
   `size_bytes` on both contract structs. Migration version: claim the
   next free number at implementation-merge time; reviewer duplicate-checks
   version numbers (same rule as Q15/Q27 iterations).
3. **String and identity sentinels at the same mapping sites** — FR-136
   prohibits any absence-synthesizing fallback, not just numeric zeros:
   fabricated binning `"1x1"` (`matching.rs:742,797`), empty-string camera
   (`:737,792`), self-referential `source_session_id` defaulted to the
   master's own id (`:745,800`), and `COALESCE` variants in SQL. The
   repo-wide sweep targets any `unwrap_or*`/`COALESCE` that synthesizes a
   value for absent metadata, numeric or string.

### Field-Applicability Matrix

The authoritative per-entity × per-field applicability source (FR-135,
FR-137's `applicability` input). Derived from spec §2.2 (Inbox property
table field visibility) and §4.2 (calibration matching fingerprint),
extended to masters; T131/T132 read applicability from here, never infer
it from data absence. ✓ = applicable (a missing value renders as
unresolved); — = not-applicable (blank/"—", no chip).

| Field | Light | Dark | Flat | Bias |
|-------|-------|------|------|------|
| Target / Object | ✓ | — | — | — |
| Frame type | ✓ | ✓ | ✓ | ✓ |
| Filter | ✓ | — | ✓ | — |
| Date / Night | ✓ | ✓ | ✓ | ✓ |
| Camera | ✓ | ✓ | ✓ | ✓ |
| Telescope / Optical train | ✓ | — | ✓ | — |
| Focal length | ✓ | — | ✓ | — |
| Exposure time | ✓ | ✓ | ✓ | — |
| Gain | ✓ | ✓ | ✓ | ✓ |
| Offset | ✓ | ✓ | ✓ | ✓ |
| Binning | ✓ | ✓ | ✓ | ✓ |
| Sensor mode | ✓ | ✓ | ✓ | ✓ |
| Set temperature | ✓ | ✓ | — | — |
| Observer / Location | ✓ | — | — | — |
| Timezone | ✓ | — | — | — |

Columns apply to both sub-frame sets and masters of each calibration kind.
Masters additionally carry the provenance fields of spec §4.2 (file hash,
created date, created-in tool, source session, age, size) — applicable to
all master kinds. Surfaces that render these entities inherit the matrix:
Sessions detail = Light column; Calibration detail = the entity's kind
column; Targets renders target-identity fields (spec §5.4, always
applicable) plus session rows per the Light column; Projects and Archive
render whatever entity kind an item is, using that kind's column.
`{object}` on calibration naming patterns (spec §9.5) follows the
Target/Object row.
