# Data Model: Immutable Sessions and Observation Groups

**Feature**: `062-session-heterogeneity`

**Storage**: SQLite 3 with foreign keys, WAL, recursive CTE, and RTree enabled
**Public identifier format**: UUIDv7 stored as canonical lowercase `TEXT`

This schema replaces the mutable acquisition and calibration session model. It
does not preserve development-database compatibility. Existing raw files remain
outside the database.

## Storage conventions

- Every independently addressable entity table has `row_id INTEGER PRIMARY KEY`
  and `public_id TEXT NOT NULL UNIQUE`. `public_id` is a UUIDv7 generated before
  insertion and validated as canonical lowercase text. Junction tables use
  integer composite primary keys and do not invent public identities.
- Foreign keys and high-volume junction keys use the referenced `row_id` as an
  `INTEGER`. Composite uniqueness and indexes use integer keys unless a public
  identifier is part of the contract value itself.
- The tables below use contract-facing names such as `session_id` and describe
  them as UUIDs for semantic clarity. In physical DDL, each such relationship is
  stored as `<name>_row_id INTEGER`; repository projections join to `public_id`
  at the API boundary. A field shown as `id` denotes the table's `public_id`.
- Time instants use UTC RFC 3339 text with microseconds. Local civil dates use
  ISO `YYYY-MM-DD` text.
- Exposure durations use integer microseconds. Angles use integer microdegrees.
- Percentages and fractions use integer parts per million (`ppm`). Scientific
  source values remain separately available in frame evidence.
- Booleans use `INTEGER NOT NULL CHECK (value IN (0, 1))`.
- Enum-like columns use `TEXT NOT NULL CHECK (...)`.
- JSON is permitted only for canonical source payloads, audit payloads, and
  outbox payloads. Searchable identity, state, threshold, and relationship
  fields use typed columns or child tables.
- All foreign-key actions are restrictive unless a table is explicitly
  operational. Accepted history is never cascade-deleted.
- Append-only tables reject `UPDATE` and `DELETE` through database triggers.
  Mutable head and delivery-state tables are listed under transaction rules.
- Every committed domain mutation receives one monotonically increasing
  `repository_change_sequence`. Append-only rows store `created_sequence`;
  retirement, supersession, decision, and head movement also append a typed
  visibility or head-history row with its sequence. Cursor watermarks therefore
  reconstruct the exact projection visible at the first page without retaining
  a database transaction across requests. Operational lease and outbox-delivery
  updates do not advance the domain sequence.
- Cross-table and exact-cardinality rules are repository precommit queries, not
  SQLite constraints. Every writer, including migrations, maintenance jobs, and
  import tools, uses the repository transaction boundary that runs those queries
  under `BEGIN IMMEDIATE`.

## Ingestion and immutable sessions

### `session_materialization_operation`

One row represents any operation that creates immutable sessions. The operation
kind is `inbox_ingestion` or `metadata_reclassification`. The command ledger,
not this table, owns retry identity.

| Field | Type | Constraints and meaning |
|---|---|---|
| `id` | UUID | Public operation identity |
| `kind` | TEXT | `inbox_ingestion` or `metadata_reclassification` |
| `command_id` | UUID FK | `UNIQUE NOT NULL`; command that created the operation |
| `config_version_id` | UUID FK | Immutable configuration used to derive the operation |
| `state` | TEXT | `ready`, `applying`, `cancelling`, `cancelled`, `applied`, or `failed` |
| `state_version` | INTEGER | `NOT NULL DEFAULT 0`; compare-and-swap token |
| `started_at` | timestamp | Nullable until application starts |
| `finished_at` | timestamp | Present only for `cancelled`, `applied`, or `failed` |
| `failure_code` | TEXT | Present only for `failed` |
| `created_at` | timestamp | `NOT NULL` |

`inbox_ingestion_operation` is a one-to-one subtype keyed by materialization
operation. It stores `inbox_plan_id UNIQUE NOT NULL`, the approved plan digest,
and approval provenance. A metadata correction creates a new
`metadata_reclassification` operation. It may reuse frames from its predecessor
session without changing those frame records. Output ordinal and identity
uniqueness are scoped to the new materialization operation.

`inbox_materialization_plan_result_snapshot` pins one plan revision, its
canonical digest, configuration revision, and the exact input-evidence revision.
Each proposed-session child pins one acquisition-site resolution revision, so
mixed-site inputs remain partition-specific. Ordered typed children contain
every proposed session, each proposed session's frames, and every blocked frame
with bounded reason codes. Counts on the snapshot must equal its child rows.

`session_materialization_result_snapshot` is created only with a terminal
applied operation. Ordered children contain every output session, its exact
frame rows, every blocked frame, and the light session's singleton panel-group
and initial revision identities. The operation stores the result-snapshot ID
and scalar session, membership, singleton-group, and blocked-frame counts.
Neither result snapshot embeds a capped domain collection in JSON.

### `frame_record`

One row identifies one indexed source frame independently of its path.

| Field | Type | Constraints and meaning |
|---|---|---|
| `id` | UUID | Public frame identity |
| `file_record_id` | UUID FK | `UNIQUE NOT NULL`; indexed raw file identity |
| `content_fingerprint` | TEXT | Nullable strong or sampled fingerprint with algorithm prefix |
| `byte_size` | INTEGER | `NOT NULL CHECK (byte_size >= 0)` |
| `captured_metadata_digest` | TEXT | `NOT NULL`; SHA-256 over normalized evidence |
| `created_at` | timestamp | `NOT NULL` |

Path and library-root observations remain provenance on the existing file
record. Neither field participates in session identity.

### `frame_metadata_evidence`

Each immutable revision stores the values used by classification and identity
derivation. `frame_metadata_evidence_head(frame_row_id,
head_evidence_row_id, head_generation)` points to the accepted revision by CAS.
Each evidence row has `frame_row_id`, `revision_number`, optional predecessor,
actor, command, and creation time, with `UNIQUE(frame_row_id,
revision_number)`. Source field and parse state keep missing and invalid values
distinguishable.

`session_metadata_resolution` groups the exact frame-evidence revisions used
for one session-level decision. It stores `session_row_id`, `revision_number`,
predecessor, state, actor, command, and time, with `UNIQUE(session_row_id,
revision_number)`. `session_metadata_resolution_frame` pins each frame-evidence
revision. `session_metadata_resolution_head(session_row_id,
head_resolution_row_id, head_generation)` selects the accepted revision by CAS
and supplies the contract's `metadataResolutionRevision`.

| Field family | Typed fields |
|---|---|
| Classification | `frame_id` FK, `revision_number`, `detected_kind` (`light`, `dark`, `bias`, `flat`), `classification_source`, `classification_confidence` |
| Exposure time | `canonical_exposure_at_utc`, `canonical_time_source`, `local_exposure_text`, `local_time_parse_state` |
| Acquisition | `exposure_us`, `gain_text`, `offset_state`, `offset_value`, `binning_state`, `bin_x`, `bin_y`, `readout_state`, `readout_mode` |
| Image | `raster_width`, `raster_height`, `crop_state`, `crop_payload`, `parity` |
| Temperature | `cooling_setpoint_state`, `cooling_setpoint_millic`, `sensor_temperature_state`, `sensor_temperature_millic` |
| Equipment | `camera_reported`, `telescope_reported`, `focal_length_reported_um`, `focal_length_calculated_um`, `filter_state`, `filter_reported` |
| Orientation | `physical_rotator_state`, `physical_rotator_udeg`, `physical_rotator_field_id`, `sky_orientation_state`, `sky_orientation_udeg` |
| Geometry | `footprint_wkb`, `footprint_digest`, `centre_ra_udeg`, `centre_dec_udeg`, conservative unit-sphere `bbox_min/max_{x,y,z}_ppb`, `geometry_solver_version` |
| Provenance | `capture_profile_version_id`, `source_payload_json`, `recorded_at` |

State/value pairs use these checks:

- `offset_state`, `readout_state`, `filter_state`, and the two temperature
  states are `present`, `absent`, `invalid`, or `contradictory`.
- A value is non-null exactly when its state is `present`.
- `binning_state` is `present`, `absent`, `invalid`, or `contradictory`.
  `bin_x` and `bin_y` are both positive only when it is `present`.
- `crop_state` is `reported_full`, `reported_crop`, `reported_subframe`,
  `absent`, `invalid`, or `contradictory`. Raster dimensions never infer it.
- Physical rotation is `verified`, `absent`, `unverified`, `invalid`, or
  `contradictory`. Only `verified` can participate in flat matching.
- Sky orientation and physical rotation are independent fields.
- A dark-flat detector result is not an allowed `detected_kind`. Detection
  returns before `frame_record`, Inbox candidate, operation, or session rows
  are created.

### `acquisition_site`

| Field | Type | Constraints and meaning |
|---|---|---|
| `id` | UUID | Public site identity |
| `label` | TEXT | `NOT NULL` |
| `timezone_name` | TEXT | Nullable IANA timezone pending confirmation |
| `timezone_state` | TEXT | `confirmed`, `unconfirmed`, or `absent` |
| `latitude_udeg` | INTEGER | Nullable |
| `longitude_udeg` | INTEGER | Nullable |
| `created_at` | timestamp | `NOT NULL` |

Site edits create a new site version or evidence decision. They do not rewrite
an accepted session's observing-night snapshot.

`acquisition_site_resolution` is a stable reviewed aggregate with immutable
revisions. Each revision stores its state, selected site and timezone, timestamp
decision, canonical and local timestamp evidence, derived observing night,
decision provenance, and digest. Typed ordered children store candidate sites,
evidence references, and conflict codes. Each proposed session in an Inbox plan
pins one exact resolution revision. Approval is impossible until every pinned
revision is resolved. A later site edit cannot alter bound observing-night
evidence.

### `session`

| Field | Type | Constraints and meaning |
|---|---|---|
| `id` | UUID | Public immutable session identity |
| `materialization_operation_id` | UUID FK | `NOT NULL` |
| `kind` | TEXT | `light`, `dark`, `bias`, or `flat` |
| `ordinal_in_operation` | INTEGER | `NOT NULL CHECK (ordinal_in_operation >= 0)` |
| `identity_digest` | TEXT | `NOT NULL`; canonical identity tuple digest |
| `observing_night_date` | date | `NOT NULL` |
| `site_id` | UUID FK | Nullable only when local-time fallback was reviewed |
| `timezone_name_snapshot` | TEXT | Nullable IANA name used for conversion |
| `night_derivation` | TEXT | `acquisition_timezone` or `reviewed_local_fallback` |
| `canonical_target_id` | UUID FK | Required for light; null for calibration |
| `created_at` | timestamp | `NOT NULL` |

Constraints:

- `UNIQUE(materialization_operation_id, ordinal_in_operation)` makes operation
  replay return the existing sessions.
- `UNIQUE(materialization_operation_id, identity_digest)` prevents duplicate output
  inside one materialization. The same digest in another operation is allowed.
- A light requires `canonical_target_id`. A calibration session prohibits it.
- `acquisition_timezone` requires a confirmed site and a timezone snapshot.
- The observing night uses a local noon-to-noon boundary. Any timestamp
  disagreement that changes the date bucket blocks insertion.

### `session_frame`

| Field | Type | Constraints and meaning |
|---|---|---|
| `session_id` | UUID FK | Public projection of integer junction key |
| `frame_id` | UUID FK | Public projection of integer junction key |
| `materialization_operation_id` | UUID FK | Denormalized operation owning the parent session |
| `ordinal` | INTEGER | `NOT NULL CHECK (ordinal >= 0)` |
| `is_representative` | boolean | Exactly one row per session |

Primary key: `(session_id, frame_id)`. Additional constraints are
`UNIQUE(session_id, ordinal)` and a partial unique index on `session_id WHERE
is_representative = 1`. `UNIQUE(materialization_operation_id, frame_id)`
prevents one operation from assigning a frame to multiple output sessions. A
composite relationship or repository precommit query requires the denormalized
operation to equal the parent session's operation.

The session and all memberships are inserted in one transaction. A deferred
validation query requires at least one membership and exactly one immutable
representative before commit. There is no append operation for an accepted
session.

### `light_session_identity`

One row per light session repeats no frame membership. It records the complete
normalized identity used to partition frames.

| Field | Type | Constraints and meaning |
|---|---|---|
| `session_id` | UUID FK | Public parent identity; physical `session_row_id` is the key |
| `optical_profile_id` | UUID FK | Resolved immutable profile representative |
| `filter_label_id` | UUID FK | Captured label scoped to the optical profile |
| `exposure_us` | INTEGER | `NOT NULL CHECK (exposure_us >= 0)` |
| `gain_text` | TEXT | `NOT NULL`; exact normalized value |
| `offset_state`, `offset_value` | enum, integer | Exact/absent state pair |
| `binning_state`, `bin_x`, `bin_y` | enum, integer | Horizontal and vertical values remain separate |
| `readout_state`, `readout_mode` | enum, text | Exact/absent state pair |
| `raster_width`, `raster_height` | INTEGER | Both positive and exact |
| `crop_state`, `crop_payload` | enum, TEXT | Reported evidence only |
| `parity` | TEXT | `normal` or `mirrored` |
| `footprint_digest` | TEXT | Reliable captured footprint identity |
| `representative_orientation_udeg` | INTEGER | Solved sky orientation after axis normalization |

Missing or contradictory required fields prevent automatic materialization.
Allowed absent fields retain their state and warnings in the identity row.

### `session_supersession`

| Field | Type | Constraints and meaning |
|---|---|---|
| `predecessor_session_id` | UUID FK | Part of primary key |
| `replacement_session_id` | UUID FK | Part of primary key; differs from predecessor |
| `applied_reclassification_plan_revision_id` | UUID FK | `NOT NULL`; exact applied plan revision |
| `created_at` | timestamp | `NOT NULL` |

The primary key is `(predecessor_session_id, replacement_session_id)`.
Replacement sessions use the same kind. A recursive CTE rejects a cycle. A
session is current when no supersession path starts from it.

### `reclassification_plan`

The stable plan row holds its public identity and head generation.
`reclassification_plan_revision` has the plan row ID, `revision_number`, state
(`open`, `applied`, `discarded`, `stale`, or `refused`), source session,
metadata and equipment evidence revision numbers, basis digest, creator, and
timestamps. `UNIQUE(plan_row_id, revision_number)` orders immutable plan
revisions. The stable row selects the latest revision by head CAS.

Typed child tables preserve the complete preview:

- `reclassification_plan_input` records the predecessor session, exact frame
  membership, evidence heads, group heads, matching-settings revision, and
  canonical corrections.
- `reclassification_plan_output` records each replacement key, proposed
  identity, proposed equipment-resolution revision, and its non-empty frame
  partition.
- `reclassification_plan_panel_consequence` records each source panel revision,
  proposed membership, and action.
- `reclassification_plan_project_consequence` records every unchanged project
  pin and its replacement keys.
- `reclassification_plan_edge_consequence` records each incident edge and the
  reason it becomes stale.

`reclassification_plan_result_snapshot` pins the exact plan revision and basis
digest. Ordered child tables materialize all replacement sessions and frames,
panel consequences, destination panel-group and revision UUIDs, predecessor
retirements, `identity_change` lineage edges, stale mosaic edges, project
consequences, and project replacement sets. Each collection has a stored count
and a unique `(snapshot_id, ordinal)` key.

`reclassification_apply_result_snapshot` is created atomically with apply. Its
ordered children name every created replacement session, accepted destination
panel revision, retired predecessor group, inserted lineage edge, invalidated
mosaic edge, and project replacement proposal. Apply-result counts must equal
the complete child collections and the plan-result snapshot they realize.

Apply creates one `metadata_reclassification` materialization operation and
all replacement sessions atomically. An accepted replacement marks each
derived edge whose endpoint evidence depends on the predecessor as stale. The
old edge evidence remains immutable and queryable.

## Equipment and capture-profile evidence

### `camera`

| Field | Type | Constraints and meaning |
|---|---|---|
| `id` | UUID | Public registered camera identity |
| `display_name` | TEXT | `NOT NULL` |
| `regulation_head_decision_id` | UUID FK | Nullable accepted decision head |
| `head_generation` | INTEGER | `NOT NULL DEFAULT 0`; compare-and-swap token |
| `created_at` | timestamp | `NOT NULL` |

### `camera_regulation_decision`

| Field | Type | Constraints and meaning |
|---|---|---|
| `id` | UUID | Public immutable decision identity |
| `camera_id` | UUID FK | `NOT NULL` |
| `predecessor_decision_id` | UUID FK | Nullable first decision; same camera |
| `mode` | TEXT | `regulated` or `unregulated_reviewed` |
| `proposal_id` | UUID FK | Accepted reviewed decision |
| `config_version_id` | UUID FK | `NOT NULL` |
| `actor_id` | UUID | `NOT NULL` |
| `created_at` | timestamp | `NOT NULL` |

`UNIQUE(predecessor_decision_id)` permits one accepted successor. Changing the
mode inserts a decision and moves the camera pointer by CAS. Existing family
and calibration-session rows retain the exact decision UUID used at assignment.

### `equipment_alias_evidence`

| Field | Type | Constraints and meaning |
|---|---|---|
| `id` | UUID | Public evidence identity |
| `equipment_kind` | TEXT | `camera` or `optical_profile` |
| `camera_id` | UUID FK | Present only for camera evidence |
| `optical_profile_id` | UUID FK | Present only for profile evidence |
| `capture_profile_version_id` | UUID FK | Extractor mapping in force |
| `semantic_field` | TEXT | `camera`, `telescope`, `filter`, `focal_length`, or `rotator` |
| `source_field` | TEXT | Header/property name |
| `normalized_value` | TEXT | `NOT NULL` |
| `first_seen_frame_id` | UUID FK | `NOT NULL` |
| `review_state` | TEXT | `automatic`, `accepted`, or `rejected` |
| `created_at` | timestamp | `NOT NULL` |

Exactly one equipment foreign key is present. A partial unique index prevents
two accepted mappings of the same `(capture_profile_version_id,
semantic_field, normalized_value)`.

Equipment alias evidence is revisioned per semantic evidence identity. Each row
has `revision_number`, optional predecessor, and
`UNIQUE(evidence_identity_row_id, revision_number)`. An
`equipment_alias_evidence_head` row selects the accepted revision by CAS.

### `optical_profile`

| Field | Type | Constraints and meaning |
|---|---|---|
| `id` | UUID | Public image-train identity |
| `display_name` | TEXT | `NOT NULL` |
| `representative_camera_id` | UUID FK | Nullable |
| `representative_focal_length_um` | INTEGER | `NOT NULL CHECK (> 0)` |
| `representative_raster_width`, `representative_raster_height` | INTEGER | Positive |
| `representative_pixel_size_nm` | INTEGER | Nullable positive value |
| `created_at` | timestamp | `NOT NULL` |

Profile resolution compares a candidate directly with this immutable
representative. It never chains pairwise matches.

### `filter_label`

| Field | Type | Constraints and meaning |
|---|---|---|
| `id` | UUID | Public scoped identity |
| `optical_profile_id` | UUID FK | `NOT NULL` |
| `state` | TEXT | `captured` or `absent` |
| `normalized_label` | TEXT | Required for captured; null for absent |
| `created_at` | timestamp | `NOT NULL` |

`UNIQUE(optical_profile_id, state, normalized_label)` is supplemented by a
partial unique index allowing one absent label per optical profile.

### `session_equipment_resolution`

Each immutable revision records `session_row_id`, `revision_number`,
predecessor, `camera_id`, `optical_profile_id`, the exact alias-evidence
revision IDs, reported and calculated focal lengths, comparison severity,
assignment mode, accepted proposal UUID, configuration UUID, actor, and time.
`UNIQUE(session_row_id, revision_number)` orders the history.
`session_equipment_resolution_head(session_row_id, head_resolution_row_id,
head_generation)` selects the accepted revision by CAS. Required foreign keys
depend on session kind: dark/bias require a camera; lights/flats require an
optical profile. Reported and calculated focal lengths stay separately
queryable.

### `capture_profile` and `capture_profile_version`

`capture_profile` is the stable identity of capture software. Each immutable
`capture_profile_version` stores a version number, parser version, creation
time, and canonical digest. `capture_field_mapping` is keyed by `(version_id,
semantic_field, source_field)` and stores value type, precedence, unit, and a
`physical_rotator_confirmed` flag. This allows software-specific header names
without embedding them in matching code.

## Calibration families and sessions

### `calibration_family`

| Field | Type | Constraints and meaning |
|---|---|---|
| `id` | UUID | Public family identity |
| `kind` | TEXT | `dark`, `bias`, or `flat` |
| `camera_id` | UUID FK | Required for dark and bias |
| `optical_profile_id` | UUID FK | Required for flat |
| `filter_label_id` | UUID FK | Required for flat |
| `representative_session_id` | UUID FK | `UNIQUE NOT NULL`; immutable representative |
| `camera_regulation_decision_id` | UUID FK | Required for dark; exact decision used |
| `identity_digest` | TEXT | `NOT NULL`; unique only within the kind-specific owner scope |
| `created_at` | timestamp | `NOT NULL` |

The ownership check requires exactly the camera columns for dark/bias and
exactly the optical-profile/filter columns for flat. A family representative
does not change. A materially different acquisition creates another family.

### `dark_recipe_identity`

One row per dark family stores:

- `temperature_mode`: `regulated` or `unregulated_reviewed`;
- exact `cooling_setpoint_millic` for regulated mode and null otherwise;
- representative `exposure_us`;
- exact `gain_text`;
- exact/absent offset and readout state/value pairs;
- separate positive `bin_x` and `bin_y`;
- exact positive raster width and height.

The dark exposure tolerance is calculated against `representative_exposure_us`
as `max(1000, min(100000, representative_exposure_us / 2000))` microseconds.
A dark with no cooling set point and no reviewed unregulated camera decision has
no family row. It remains an immutable unassigned calibration session.

### `bias_recipe_identity`

One row per bias family stores exact gain, exact/absent offset, separate
binning, exact/absent readout, and exact raster dimensions. It has no exposure
or temperature discriminator.

### `flat_family_identity`

One row per flat family stores exact gain, exact/absent offset, separate
binning, exact/absent readout, exact raster dimensions, camera geometry, and
verified physical orientation when available. It has no exposure
discriminator. Missing or unverified physical orientation is retained as an
explicit state and yields compatibility-unverified results.

### `calibration_session`

| Field | Type | Constraints and meaning |
|---|---|---|
| `session_id` | UUID FK | Public parent identity; physical `session_row_id` is the key |
| `family_id` | UUID FK | Nullable only for blocked or review-required assignment |
| `assignment_state` | TEXT | `assigned`, `blocked_unknown_temperature`, or `needs_review` |
| `assignment_proposal_id` | UUID FK | Nullable for automatic same-operation assignment |
| `age_anchor_at_utc` | timestamp | `NOT NULL`; newest canonical exposure instant in session |
| `created_at` | timestamp | `NOT NULL` |

The family kind must equal the parent session kind. Sessions from different
observing nights remain distinct even when their family matches.

External-handoff sufficiency is a query over immutable recipe evidence and the
rebuildable file-availability projection: the session has complete required
recipe evidence and at least one indexed source frame that is available and
readable. It has no frame-count or scientific-quality minimum.
Availability changes do not alter session identity, family membership, or
accepted evidence.

### `dark_thermal_evidence`

One row per dark session stores `valid_count`, `missing_count`, `invalid_count`,
`minimum_abs_deviation_millic`, `median_abs_deviation_millic`,
`maximum_abs_deviation_millic`, `p95_abs_deviation_millic`, `valid_ratio_ppm`,
and `severity`. Missing and invalid readings are excluded from all statistics.
`valid_ratio_ppm < 800000` prohibits an automatically stable result. Severity
is `normal`, `yellow`, `red`, or `unknown`.

### `calibration_reuse_decision`

This append-only table records a manual or automatic use of one calibration
session for one light session or external-processing handoff. It stores exact
source and destination session UUIDs, family UUID, age in days, age severity,
thermal or orientation severity, decision mode, accepted proposal UUID,
configuration version UUID, actor UUID, and timestamp. A red or unregulated
selection requires `decision_mode = 'audited_manual'` and a non-null audit
event.

There is no dark-flat family, recipe, session, decision, or foreign-key target.

### `calibration_handoff`

One stable aggregate row stores the public handoff identity, project,
external processor, nullable head snapshot, and `head_generation`. Initial
creation inserts the aggregate and first snapshot together. A reviewed
addition inserts one successor snapshot and moves the head with CAS. A stale
head or generation refuses the complete command.

### `calibration_handoff_snapshot`

One append-only row stores the handoff aggregate, nullable same-handoff
predecessor snapshot, trusted-clock `evaluation_at`, exact
matching-settings revision, basis digest, frame and source-byte counts, the
16-TiB source-byte ceiling, actor, command, and creation
time. `UNIQUE(handoff_id, predecessor_snapshot_id)` permits one accepted
successor from a snapshot. The basis digest includes the project revision,
evaluation instant,
settings revision, ordered requirements, candidate evidence, selections, and
pinned frame identities.

`calibration_handoff_requirement` has primary key `(handoff_id,
requirement_id)`. It stores kind, camera, recipe and recipe revision, exact
evidence reference, and complete required-field state.
`calibration_handoff_snapshot_requirement` has primary key `(snapshot_id,
requirement_id)`, carries `handoff_id`, and has `UNIQUE(snapshot_id, ordinal)`.
Composite foreign keys to `(handoff_id, snapshot_id)` and `(handoff_id,
requirement_id)` prohibit cross-handoff mappings.

`calibration_handoff_candidate_evidence` has a globally unique evidence-ID
primary key and is evaluated for one snapshot, requirement, and session at that
snapshot's `evaluation_at`. It carries `handoff_id`; composite foreign keys
require its snapshot and requirement to belong to that handoff. It
stores recipe compatibility and completeness, age and thermal states,
availability counts and observation time, automatic eligibility, ordered
warning codes, and the evidence digest.

`calibration_handoff_selection` is a stable immutable row with globally unique
selection-ID primary key and `UNIQUE(handoff_id, selection_id)`. It references
one handoff requirement, session, candidate evidence, `automatic` or `reviewed`
source, review decision, and selection time. A composite evidence reference
requires handoff, requirement, and session to match the evidence row.

`calibration_handoff_snapshot_selection` has primary key `(snapshot_id,
selection_id)`, carries `handoff_id`, and has `UNIQUE(snapshot_id, ordinal)`.
Composite foreign keys require the snapshot and selection to belong to that
handoff. A successor copies only these bounded mapping rows and adds the new
selection; it does not copy selection, evidence, or frame records.

`calibration_handoff_review` stores the derived actor, bounded reason, ordered
acknowledged warnings, and timestamp for each reviewed selection.

`calibration_handoff_frame` is keyed by `(selection_id, frame_id)` and stores:

- the immutable session-membership ordinal and file record;
- source root, canonical relative path, and stable file identity;
- byte size, SHA-256 fingerprint, no-follow invariant, and verification time.

`UNIQUE(selection_id, session_membership_ordinal)` preserves order. Snapshot
frame queries join the snapshot-selection mapping to these stable rows.
Creation opens each frame once by root-relative no-follow resolution and hashes
the consumed bytes through that handle. Commit requires every selected-session
frame to be indexed, readable, and verified. No partial session is stored, and
retaining a selection never duplicates its frame rows.

`calibration_handoff_operation` stores fenced asynchronous creation or reviewed-
addition verification state,
frame and byte progress, cancellation state, and the terminal snapshot
reference. Hashing checks cancellation in bounded byte and time intervals. Only
the final transaction inserts the immutable snapshot, audit, outbox, and
terminal command result; cancellation commits none of those domain rows.

## Panel groups and immutable revisions

### `cross_target_association`

An accepted reviewed association permits intentional grouping across canonical
targets. It stores a stable UUID, purpose, accepted manual-relation proposal
UUID, actor UUID, and creation time.

- `cross_target_association_target` has primary key `(association_id,
  canonical_target_id)` and a deterministic target ordinal.
- An association has at least two targets.
- Before acceptance, proposed targets exist only in typed `relation_proposal`
  target-scope child rows.
- Accepting a `manual_relation` with `new_reviewed_cross_target` scope creates
  the association, target rows, and first relation revision atomically.
- Automatic proposals and previews cannot create an association.

### `panel_group`

| Field | Type | Constraints and meaning |
|---|---|---|
| `id` | UUID | Public conceptual panel identity |
| `canonical_target_id` | UUID FK | Present for normal same-target grouping |
| `cross_target_association_id` | UUID FK | Present only for explicit cross-target grouping |
| `status` | TEXT | `active` or `retired` |
| `head_revision_id` | UUID FK | Exact accepted immutable head |
| `head_generation` | INTEGER | `NOT NULL`; compare-and-swap token |
| `created_at` | timestamp | `NOT NULL` |
| `retired_at` | timestamp | Present only when retired |

Exactly one target-scope foreign key is present. The head foreign key is
deferred and must reference a revision of the same group.

### `panel_group_revision`

| Field | Type | Constraints and meaning |
|---|---|---|
| `id` | UUID | Public immutable revision identity |
| `panel_group_id` | UUID FK | `NOT NULL` |
| `revision_number` | INTEGER | `NOT NULL CHECK (revision_number >= 1)` |
| `parent_revision_id` | UUID FK | Nullable only for initial revision |
| `representative_session_id` | UUID FK | Exact immutable representative |
| `representative_evidence_id` | UUID FK | `NOT NULL UNIQUE REFERENCES relation_evidence(id)`; the evidence computed for the representative session; the repository projection returns it as `PanelGroupRevision.representativeEvidenceId` |
| `proposal_id` | UUID FK | Null only for the ingestion-created singleton |
| `config_version_id` | UUID FK | `NOT NULL` |
| `actor_id` | UUID | System or user actor |
| `reason_code` | TEXT | `NOT NULL` |
| `created_at` | timestamp | `NOT NULL` |

`UNIQUE(panel_group_id, revision_number)` and `UNIQUE(parent_revision_id)` make
accepted revision history linear within a stable group. A composite foreign
key ensures the parent belongs to the same group.

### `panel_revision_session`

Primary key: `(panel_revision_id, session_id)`. Each member must be a light
session in the revision target scope. Each row has a non-negative `ordinal` and
`UNIQUE(panel_revision_id, ordinal)`. A revision contains its representative.
A partial current-membership check, run before head CAS, rejects any current,
non-superseded session that is already in another active panel head.

Materializing a light session atomically inserts:

1. the session and frame memberships;
2. a new active panel group;
3. an initial panel revision containing only that session;
4. the group's `head_revision_id` and generation `0`.

Every committed light session therefore has one singleton group even when no
geometry is available.

### `panel_group_lineage`

| Field | Type | Constraints and meaning |
|---|---|---|
| `predecessor_group_id` | UUID FK | Part of primary key; retired group |
| `successor_group_id` | UUID FK | Part of primary key; different group |
| `kind` | TEXT | `split`, `merge`, or `identity_change` |
| `proposal_id` | UUID FK | Accepted proposal |
| `created_at` | timestamp | `NOT NULL` |

Lineage insertion and predecessor retirement occur in the same transaction.
A recursive CTE from the proposed successor rejects any path back to the
predecessor.

## Mosaics, edges, and captured-object evidence

### `mosaic_edge_evidence`

One immutable edge relates two exact panel revisions in canonical UUID order.
It stores target or cross-target scope, captured intersection over smaller
footprint (`overlap_ppm`), centre separation, observed transported residual
orientation, allowed residual interval, parity comparison, acquisition-geometry
result, endpoint footprint digests, solver version, configuration version,
evidence digest, and creation time.

Constraints:

- `left_panel_revision_id < right_panel_revision_id` prevents duplicate
  undirected edges.
- Automatic evidence requires reliable footprints, matching parity, compatible
  acquisition geometry, inclusive configured overlap, and observed residual in
  the geometry-derived allowed set with absolute value at most 10 degrees.
- `UNIQUE(left_panel_revision_id, right_panel_revision_id, evidence_digest)`.

### `mosaic_edge_invalidation`

This append-only table stores `edge_evidence_id`,
`applied_reclassification_plan_revision_id`, a bounded reason code, and
creation time. Its primary key is `(edge_evidence_id,
applied_reclassification_plan_revision_id)`. An edge is stale when it has at
least one invalidation. Edge queries expose the newest invalidation reason and
applied plan-revision ID while retaining the immutable edge evidence.

### `mosaic`

`mosaic` has the same stable-head shape as `panel_group`: UUID, target scope,
`active`/`retired` status, deferred `head_revision_id`, `head_generation`, and
timestamps.

### `mosaic_revision`

An immutable accepted revision stores UUID, mosaic UUID, numeric
`revision_number`, optional same-mosaic parent revision UUID, accepted proposal
UUID, configuration version UUID, actor, reason, and creation time. A
`captured_union_evidence_id` UUID FK `NOT NULL UNIQUE REFERENCES
relation_evidence(id)`
holds the union-coverage evidence the repository projection returns as
`MosaicRevision.capturedUnionEvidenceId`.
`UNIQUE(mosaic_id, revision_number)` orders revisions and
`UNIQUE(parent_revision_id)` permits one accepted successor from a head.

### `mosaic_revision_panel`

Primary key: `(mosaic_revision_id, panel_revision_id)`. Membership pins the
exact panel revision, not the mutable panel-group head. A uniqueness constraint
on `(mosaic_revision_id, panel_group_id)` prohibits two revisions of one panel
group in a mosaic snapshot. Each row has a non-negative `ordinal` and
`UNIQUE(mosaic_revision_id, ordinal)`.

### `mosaic_revision_edge`

Primary key: `(mosaic_revision_id, edge_evidence_id)`. Both edge endpoints must
be present in `mosaic_revision_panel`. The accepted edge set must connect the
revision's panels. A new edge that bridges two accepted mosaic components
creates a merge proposal and new mosaic identity; it never changes either head
automatically.

Historical revision queries return their pinned edge evidence even after an
invalidation. Current candidate discovery and traversal exclude evidence with
any `mosaic_edge_invalidation`; a replacement edge requires a new reviewed
proposal and a new mosaic revision.

### `mosaic_lineage`

This table mirrors `panel_group_lineage` with mosaic foreign keys. The same
recursive cycle check and atomic retirement rule apply.

### `mosaic_object_evidence`

One row per `(mosaic_revision_id, canonical_object_id)` stores object extent
kind, captured-union intersection state (`none`, `partial`, or `full`), nullable
covered fraction, union-geometry digest, and catalogue version. Rows with
`none` are excluded from user results but retained only in a bounded cache, not
accepted revision history. `mosaic_object_panel_evidence` stores per-panel and
per-session containment evidence and deduplicates by canonical object UUID.

Captured-union geometry preserves disconnected regions and gaps. It never
modifies image files.

## Relation proposals and remembered rejection

### `relation_proposal`

| Field | Type | Constraints and meaning |
|---|---|---|
| `id` | UUID | Public proposal identity |
| `proposal_revision` | INTEGER | `NOT NULL CHECK (proposal_revision >= 1)`; contract CAS token incremented by each state decision |
| `kind` | TEXT | `panel_add`, `panel_replace`, `panel_split`, `panel_merge`, `mosaic_create`, `mosaic_edge`, `mosaic_split`, `mosaic_merge`, or `manual_relation` |
| `basis_digest` | TEXT | `NOT NULL`; exact ordered inputs and relation kind |
| `evidence_id` | UUID FK | `NOT NULL UNIQUE REFERENCES relation_evidence(id)`; the envelope this proposal was measured from, returned as the contract `evidenceId` |
| `evidence_digest` | TEXT | `NOT NULL`; equals the `input_digest` of the referenced `relation_evidence` row |
| `config_version_id` | UUID FK | `NOT NULL` |
| `state` | TEXT | `pending`, `accepted`, `rejected`, `superseded`, or `stale` |
| `created_at`, `decided_at` | timestamp | Decision time follows state |
| `actor_id` | UUID | Nullable until decision |
| `reason_code` | TEXT | Required for rejection or correction |
| `superseded_by_proposal_id` | UUID FK | Required only for `superseded` |

`UNIQUE(kind, basis_digest, evidence_digest, config_version_id)` makes proposal
generation idempotent. Because `evidence_digest` restates the referenced
envelope's `input_digest`, that uniqueness key rejects a second proposal over the
same inputs, and a digest that disagrees with the referenced row is a write bug
the `manual.create` guard rejects rather than a state the table can hold. Specific input and proposed-output tables preserve
foreign keys. Every ordered child row has a non-negative `ordinal` and a
per-proposal ordinal uniqueness constraint in its typed table:

- `proposal_session_input(proposal_id, session_id, role, ordinal)`;
- `proposal_panel_revision_input(proposal_id, panel_revision_id, role,
  ordinal)`;
- `proposal_mosaic_revision_input(proposal_id, mosaic_revision_id, role,
  ordinal)`;
- `proposal_project_revision_input(proposal_id, project_membership_revision_id,
  role, ordinal)`;
- typed proposed-membership, proposed-edge, and proposed-lineage child tables,
  each with `UNIQUE(proposal_id, ordinal)`.

`proposal_measurement` is keyed by `(proposal_id, measurement_key)` and stores
typed integer value, unit, comparison, threshold, outcome, and source evidence
digest. Geometry payloads remain in the exact edge or frame evidence tables.
Corrected proposals retain measured evidence and store typed review overrides.

### `relation_evidence`

One row is the `RelationEvidence` envelope the contract requires on every
`RelationProposal`, `ManualRelationCreateRequest`, `PanelGroupRevision`, and
`MosaicRevision`. It is keyed by its own `id`, so a proposal, a panel-group
revision's representative session, and a mosaic revision's captured union each
reference the same table by UUID.

One envelope serves exactly one subject. `relation_proposal.evidence_id`,
`panel_group_revision.representative_evidence_id`, and
`mosaic_revision.captured_union_evidence_id` are each `UNIQUE`, which stops
reuse within a table but not across the three: a proposal and a panel-group
revision can both name one row and every foreign key still succeeds. Exclusive
ownership across the three tables is a repository precommit query under the
boundary described above, and the row also records which subject it belongs to:

| Field | Type | Constraints and meaning |
|---|---|---|
| `subject_kind` | TEXT | `NOT NULL`; `proposal`, `panel_group_representative`, or `mosaic_captured_union` |
| `subject_digest` | TEXT | `NOT NULL`; digest over the ordered subject identities the evidence was measured for: the proposal's ordered inputs, the representative session, or the mosaic's captured membership |

A referencing write computes `subject_digest` from its own inputs and stores it;
a reader that recomputes a different digest has found a mispaired row and MUST
treat the evidence as absent rather than authoritative. `subject_kind` makes the
mismatch cheap to detect, because an envelope measured for a representative
session can never legally sit under a mosaic's captured-union column.

The same precommit query requires the subject's `config_version_id` to equal the
envelope's. Without that check a subject can claim settings revision B while
carrying verdicts and thresholds measured under A, and the projection reports B
as `matchingSettingsRevision` over evidence that contradicts it. The equality
also keeps proposal deduplication honest, since
`UNIQUE(kind, basis_digest, evidence_digest, config_version_id)` reads the
proposal's copy of the revision, not the evidence's.

| Field | Type | Constraints and meaning |
|---|---|---|
| `id` | UUID | Public evidence identity; the contract `evidenceId` |
| `target_compatibility` | TEXT | `NOT NULL`; `same_target`, `reviewed_cross_target`, or `incompatible` |
| `parity` | TEXT | `NOT NULL`; `match`, `mismatch`, or `unknown` |
| `acquisition_geometry` | TEXT | `NOT NULL`; `compatible`, `incompatible`, or `unknown` |
| `equipment` | TEXT | `NOT NULL`; `compatible`, `incompatible`, or `unknown` |
| `footprint_coverage_ppm` | INTEGER | Nullable; `CHECK (value BETWEEN 0 AND 1000000)`; the optional `footprintCoveragePercent` |
| `centre_separation_ppm` | INTEGER | Nullable; `CHECK (value >= 0)`; the optional `centerSeparationPercent`, a percent, distinct from the angular `centre_separation_udeg` on `mosaic_edge_evidence` |
| `residual_sky_rotation_udeg` | INTEGER | Nullable; the optional `residualSkyRotationDeg` in microdegrees |
| `config_version_id` | UUID FK | `NOT NULL`; matching-settings revision the evidence was measured under |
| `input_digest` | TEXT | `NOT NULL`; digest over every input the stored verdicts derive from: the ordered footprints, the settings revision, the ordered resolved target identities behind `target_compatibility`, and the equipment-resolution revisions behind `equipment` |
| `created_sequence` | INTEGER | `NOT NULL`; append-only sequence |
| `created_at` | timestamp | `NOT NULL` |

`missingEvidenceCodes`, `allowedResidualRotationRangesDeg`, and
`thresholdSnapshot` are bounded contract lists, so each is a typed child table
with a per-evidence ordinal, never JSON:

- `relation_evidence_missing_code(evidence_id, ordinal, code)` with
  `UNIQUE(evidence_id, ordinal)` and `CHECK (ordinal BETWEEN 0 AND 99)`, matching
  the contract bound of 100; a `manual.create` guard requires it to enumerate
  every geometry or orientation measurement the proposal could not compute.
- `relation_evidence_allowed_rotation(evidence_id, ordinal, lower_udeg,
  upper_udeg)` with `UNIQUE(evidence_id, ordinal)`, `CHECK (ordinal BETWEEN 0 AND
  15)` for the contract bound of 16, and `CHECK (lower_udeg <= upper_udeg)`; the
  geometry-derived allowed residual intervals.
- `relation_evidence_measurement(evidence_id, measurement_key, integer_value,
  unit, comparison, threshold_min, threshold_max, outcome,
  source_evidence_digest)` keyed by `(evidence_id, measurement_key)`, mirroring
  `proposal_measurement`, including its closed `comparison` and `outcome`
  vocabularies, but scoped to the evidence envelope so representative and
  captured-union evidence carry their own `thresholdSnapshot`.

The stored `comparison` and `outcome` vocabularies are wider than the contract's.
`proposal_measurement` admits `inside` alongside the five single-sided
comparisons and `warn` alongside `pass` and `fail`, which is why the threshold is
a `threshold_min`/`threshold_max` pair rather than one column. Projecting into
`ThresholdMeasurement`, whose `thresholdValue` is one decimal, therefore needs a
stated rule:

- `lt` and `lte` read `threshold_max`; `gt` and `gte` read `threshold_min`; `eq`
  requires the two columns to be equal and reads either.
- `inside` has no contract counterpart. A row storing it projects as the
  `missingEvidenceCodes` entry for an unrepresentable measurement, not as a
  `ThresholdMeasurement` with an arbitrary bound chosen from the pair.
- `warn` projects as `fail`, because the contract's `outcome` is two-valued and a
  warning is not a pass.

A repository precommit query rejects a single-sided `comparison` whose
corresponding column is null, so the projection never has to invent a value.

The ordinal ranges above bound the row count. An ordinal is dense from zero,
so `CHECK (ordinal BETWEEN 0 AND 99)` plus per-evidence ordinal uniqueness caps
the child list at the contract's 100 without a counting trigger. `code`,
`measurement_key`, and `unit` stay open `TEXT`, exactly as `proposal_measurement`
already treats them, because the contract types them as `SafeText` rather than
closed enums; the accepted values are owned by the `manual.create` and
proposal-generation validators, not by a database `CHECK`. `comparison` and
`outcome` do carry `CHECK` constraints, over the wider stored vocabularies set
out above rather than over the contract's.

A committed light session's singleton panel revision references a
`relation_evidence` row whose geometry columns are null and whose
`missing_code` children enumerate the unavailable measurements, with
`parity`, `acquisition_geometry`, and `equipment` set to `unknown`. The row
therefore exists even when no geometry is available, satisfying the non-optional
contract field.

Durability splits between the envelope row and its measurement children, and
referential integrity, not the tier, fixes the write order.

The `relation_evidence` row is written in the same transaction as the subject
that references it. Three `NOT NULL` foreign keys point at it, so the subject
cannot commit before the envelope exists; batching the envelope behind its
subject is not available at any tier. Its content is nonetheless re-derivable
from the inputs `input_digest` covers, so a value lost to a crash costs a
recomputation rather than user knowledge, and no write-ahead record is required
beyond the transaction itself.

`relation_evidence_measurement`, `relation_evidence_missing_code`, and
`relation_evidence_allowed_rotation` rows are Tier 2 and may be batched or
written asynchronously. An envelope whose children have not yet landed reads as
an evidence row with an empty `thresholdSnapshot`, which the projection reports
as unmeasured rather than as passing.

The Tier 1 record stays the user decision: the proposal state, actor, reason code,
and typed review overrides on `relation_proposal`. Principle V of the project
governance document defines these tiers.

### `relation_decision_snapshot`

Acceptance, rejection, and correction create one immutable decision snapshot
keyed by proposal and proposal revision. It stores decision kind, counts,
actor, reason, audit identity, and creation time. Ordered child tables store
accepted revision references, retired group references, and other potentially
large result collections with `UNIQUE(decision_snapshot_id, ordinal)`.
Acceptance responses and events return the decision snapshot ID and counts;
paginated queries return its children. Audit records reference the decision
snapshot instead of embedding a collection capped below the supported stress
size.

### `relation_rejection`

One append-only row stores proposal kind, basis digest, evidence digest,
configuration version, actor, reason code, optional note, and timestamp.
`UNIQUE(kind, basis_digest, evidence_digest, config_version_id)` suppresses only
equivalent automatic proposals. A different evidence digest or configuration
version can produce another proposal. Manual proposal creation bypasses
automatic suppression and remains audited.

## Project pins and materialization snapshots

### `project_membership_revision`

| Field | Type | Constraints and meaning |
|---|---|---|
| `id` | UUID | Public immutable membership snapshot identity |
| `project_id` | UUID FK | `NOT NULL` |
| `revision_number` | INTEGER | `NOT NULL CHECK (revision_number >= 1)`; unique per project |
| `parent_revision_id` | UUID FK | Nullable for initial empty revision |
| `proposal_id` | UUID FK | Accepted explicit add or replacement action |
| `actor_id` | UUID | `NOT NULL` |
| `created_at` | timestamp | `NOT NULL` |

`UNIQUE(project_id, revision_number)` orders project membership.
`project_membership_revision_session` has primary key `(revision_id,
session_id)`. Each row records `pin_revision`, `pinned_at`, `pinned_by`, and
  `source` (`explicit_add`, `explicit_replacement`, or `project_creation`). An
explicit add may reference `related_session_evidence_id`. A replacement records
`replaces_session_id` and the exact applied reclassification-plan revision
whose supersession edge authorizes it. This is the complete pin provenance
projected as `ProjectSessionPin`.

Projects hold `membership_head_revision_id` and
`membership_head_generation`; updates use CAS. Membership revisions and their
session rows are the project source of truth. Existing `project_sources` rows
are migration inputs and compatibility projections only; no canonical write
path inserts or updates them. No panel, mosaic, or family foreign key can
expand project membership.

The lifecycle guard allows a new head only for `setup_incomplete`, `ready`,
`prepared`, `processing`, or `blocked`. It rejects `completed` and `archived`.
A session correction creates a proposal against the old membership head; it
does not change that head.

### `group_action_session_snapshot`

| Field | Type | Constraints and meaning |
|---|---|---|
| `id` | UUID | Public immutable expansion identity |
| `source_kind` | TEXT | `panel_revision` or `mosaic_revision` |
| `source_revision_id` | UUID | Exact source revision UUID |
| `source_digest` | TEXT | `NOT NULL` |
| `created_at` | timestamp | `NOT NULL` |

`group_action_snapshot_session` has primary key `(snapshot_id, session_id)`.
Every group-derived project action or destination proposal references this
snapshot. It expands a group once and never follows a later head.

### `project_materialization_snapshot`

| Field | Type | Constraints and meaning |
|---|---|---|
| `id` | UUID | Public completed filesystem-view snapshot identity |
| `project_id` | UUID FK | `NOT NULL` |
| `membership_revision_id` | UUID FK | Exact pins materialized at snapshot time |
| `predecessor_snapshot_id` | UUID FK | Nullable for first materialization |
| `applied_plan_id` | UUID FK | `UNIQUE NOT NULL` |
| `created_at` | timestamp | `NOT NULL` |

`project_materialization_snapshot_session` has primary key `(snapshot_id,
session_id)`, a non-negative ordinal, and `UNIQUE(snapshot_id, ordinal)`. It is
the exact set successfully materialized by that snapshot. The referenced
membership revision is the plan basis, not a claim that every pin in that
revision was materialized. Staleness and the next bounded batch compare the
current project pins with this exact set by ordinary joins and `EXCEPT`.

### `materialized_entry`

| Field | Type | Constraints and meaning |
|---|---|---|
| `id` | UUID | Public immutable entry identity |
| `project_id` | UUID FK | `NOT NULL`; explicit project scope |
| `first_snapshot_id` | UUID FK | First completed manifest containing the entry |
| `source_session_id` | UUID FK | Exact pinned source session |
| `source_frame_id` | UUID FK | Exact immutable frame |
| `destination_root_id` | UUID FK | `NOT NULL` |
| `relative_path` | TEXT | `NOT NULL`; normalized project-relative path |
| `content_fingerprint` | TEXT | Nullable verified content identity |
| `created_by_plan_id` | UUID FK | `NOT NULL`; retry provenance |
| `created_at` | timestamp | `NOT NULL` |

`UNIQUE(project_id, destination_root_id, relative_path)` makes an existing path
immutable. `materialization_snapshot_entry` maps each snapshot to its complete,
versioned manifest without changing entry rows.

### `project_manifest` and correction overlays

`project_manifest` stores project, immutable version, predecessor manifest,
materialization snapshot, creation command, actor, and time. Ordered
`project_manifest_entry` and `project_manifest_overlay` junctions expose the
active processing set. The project stores the current manifest ID and a CAS
generation; manifest and materialization heads move atomically.

`correction_overlay` stores project, nullable predecessor overlay, exact
applied reclassification-plan revision, mapping count, actor, command, and
creation time. `correction_overlay_mapping` has one ordered row per predecessor
entry and stores either a replacement entry or a bounded exclusion reason,
never both. `UNIQUE(overlay_id, predecessor_entry_id)` and
`UNIQUE(overlay_id, ordinal)` make the immutable mapping deterministic.

A replacement view preserves predecessor entries as a historical overlay in
older snapshots. Its successor manifest excludes predecessor entries and
includes newly created replacement entries. It never edits, removes, renames,
or relocates the predecessor's files.

The project holds `materialization_head_snapshot_id` and a CAS generation. A
view is stale when the membership head contains a session absent from the
materialization head's exact `project_materialization_snapshot_session` set.

### `materialization_update_plan`

This feature extends the application's existing generic filesystem plan,
approval, operation, item, and apply journal. It does not define a parallel
filesystem execution engine. Feature tables attach the project, base
materialization snapshot, target membership revision, pinned-session snapshot,
and source-view revision to the generic plan and attach session provenance to
generic plan items.

One immutable preview pins the project, base materialization snapshot, target
membership revision, state, content digest, actor, and timestamps.
`materialization_update_plan_session` contains only sessions in the target
membership revision but not the base snapshot. `materialization_plan_entry`
contains only candidate additions and records a collision state. Any collision
prevents apply. Applying a plan inserts new entries and a successor snapshot;
it does not update, remove, or relocate existing entries.

The plan also persists its complete work limits and observed counts: at most
500 whole sessions, 100,000 items, 100,000 source frames, and 16 TiB of source
bytes. It stores the
remaining-session count and next-session UUID when more work exists. A single
session that exceeds any applicable work ceiling is refused without a
partial plan. Replacement plans persist an ordered overlay-mapping preview;
its count and child rows are covered by the approved plan digest.

`materialization_install_intent` is plan-owned operational recovery state. One
row per plan item stores:

- the destination collision key and canonical destination;
- the approved fingerprint;
- the temporary entry's stable identity or unforgeable platform ownership token;
- the command ID, lease owner, and lease generation; and
- state `prepared`, `installed`, or `journaled`.

The prepared intent commits before atomic no-replace installation. Recovery may
advance it only after a no-follow open proves stable ownership, collision key,
and fingerprint. Byte equality alone is insufficient. The item journal and
adopted lease generation commit before another item starts. After install, the
executor makes the destination-directory entry durable before state `installed`
or journal commit. Recovery repeats that barrier before adopting a proven
installed destination.

Filesystem execution follows these rules:

- Preflight resolves every known destination and source availability before the
  first write.
- Any known collision blocks approval and apply without filesystem changes.
- A runtime race, source failure, or I/O failure can occur after earlier items
  succeeded.
- Each successful item is durably journaled with operation, plan, item,
  destination, fingerprint, resulting entry ID, command ID, lease owner, and
  lease generation before the next item starts.
- Retry recognizes an existing entry only when the same plan item created it
  and its journal or install intent proves stable ownership, fingerprint, and
  destination under the current adopted lease generation.
- Any other existing path is a collision.
- A stopped, failed, or stale operation creates no materialization head
  snapshot. `stopped` is recoverable through a newly fenced command;
  `failed` is terminal and requires a new plan.
- The successor snapshot and head CAS occur only after every item is journaled
  successfully.

`source_availability_rollup` stores the latest readable/available counts per
session for candidate and handoff queries. It is a rebuildable projection of
file observations and materialization entries. It never participates in
session, family, or project identity.

## Configuration, audit, and outbox

### `command_execution`

Every mutation, including refused and failed attempts, enters one generic
ledger before domain work starts.

| Field | Type | Constraints and meaning |
|---|---|---|
| `command_id` | UUID | Public command identity and primary retry key |
| `actor_id` | UUID FK | `NOT NULL`; resolved from trusted authentication context, never request payload |
| `operation` | TEXT | `NOT NULL`; contract command name |
| `canonical_payload_digest` | TEXT | `NOT NULL`; SHA-256 of the command name, canonical request, and trusted derived actor identity |
| `state` | TEXT | `received`, `executing`, `applied`, `refused`, or `failed` |
| `state_version` | INTEGER | `NOT NULL DEFAULT 0`; CAS token |
| `lease_generation` | INTEGER | `NOT NULL DEFAULT 0`; increases on every claim or reclaim |
| `lease_owner` | TEXT | Bounded worker identity while executing |
| `lease_expires_at`, `heartbeat_at` | timestamp | Required together while a worker owns the execution |
| `response_json` | TEXT | Canonical terminal response when small |
| `result_ref_type`, `result_ref_id` | TEXT, UUID | Typed terminal result reference when response is not embedded |
| `error_code` | TEXT | Present for refused or failed terminal results |
| `created_at`, `started_at`, `finished_at` | timestamp | Ordered command lifecycle timestamps |

Command recovery follows these rules:

- Command IDs are globally unique. A different actor-bound payload returns
  `idempotency.payload_mismatch`.
- Replay of a live leased execution returns `operation.in_progress`.
- Startup or retry may reclaim only an expired `received` or `executing` row
  through CAS on `state_version` and `lease_generation`.
- The reclaim transaction must prove that no terminal result, domain commit,
  terminal audit, or outbox row exists.
- `(command_id, lease_owner, lease_generation)` is the fencing token. Every
  heartbeat, state transition, item-journal claim or completion, domain/terminal
  commit, and pre-effect checkpoint predicates on the current token.
- Long-running workers recheck the token immediately before each irreversible
  filesystem install. A reclaimed owner transactionally adopts reconciled
  completed items under its new generation. A stale former owner cannot write
  another journal row, external effect, domain change, or terminal result.
- A discovered commit is reconciled into the ledger; ambiguous evidence fails
  closed.
- Domain changes, terminal result, audit, and outbox commit atomically. Replay
  returns the recorded result without running domain logic again.

### `matching_settings_revision`

One aggregate revision owns every matching value. The immutable parent stores
`revision_number`, predecessor, actor, command, time, and canonical digest, with
`UNIQUE(revision_number)`. Its scalar columns use these storage constraints and
initial defaults:

| Field | Hard range | Default | Warning boundary |
|---|---:|---:|---:|
| `same_session_coverage_min_ppm` | 900,000–995,000 | 950,000 | below 930,000 |
| `same_session_centre_max_ppm` | 5,000–50,000 | 20,000 | above 30,000 |
| `same_session_rotation_max_udeg` | 250,000–3,000,000 | 1,000,000 | above 2,000,000 |
| `sibling_coverage_min_ppm` | 800,000–950,000 | 900,000 | below 850,000 |
| `sibling_centre_max_ppm` | 20,000–150,000 | 50,000 | above 100,000 |
| `sibling_rotation_max_udeg` | 1,000,000–15,000,000 | 5,000,000 | above 10,000,000 |
| `mosaic_overlap_min_ppm` | 10,000–200,000 | 50,000 | below 30,000 |
| `mosaic_overlap_max_ppm` | 200,000–600,000 | 400,000 | above 500,000 |
| `dark_thermal_moderate_millic` | 100–2,000 | 500 | above 1,000 |
| `dark_thermal_severe_millic` | 500–5,000 | 2,000 | above 3,000 |
| `flat_orientation_normal_udeg` | 500,000–5,000,000 | 2,000,000 | above 3,000,000 |
| `flat_orientation_red_udeg` | above normal–15,000,000 | 5,000,000 | above 8,000,000 |
| `flat_red_age_days` | 7–365 | 7 | above 90 |

Cross-column checks enforce sibling coverage no greater than
same-session coverage, sibling centre and rotation maxima no smaller than
same-session values, mosaic minimum below maximum, mosaic maximum at least
100,000 ppm below sibling minimum coverage, and severe thermal threshold at
least 500 millicelsius above moderate.

`matching_settings_camera_policy` is keyed by `(settings_revision_row_id,
camera_row_id, kind)`. It stores dark/bias fresh and red age boundaries as part
of the same aggregate revision; it has no independent version stream. Initial
values are 270 and 365 days. Red must be at least 30 days after fresh and at
most 1,825 days. Values above 730 days carry a warning. A singleton
`matching_settings_head` points to the accepted aggregate revision and uses a
CAS generation. Accepted entities retain the exact aggregate revision that
produced them.

### `audit_event`

| Field | Type | Constraints and meaning |
|---|---|---|
| `id` | UUID | Public audit identity |
| `command_id` | UUID FK | `NOT NULL`; command ledger correlation |
| `operation_id` | UUID FK | Nullable ingestion operation |
| `proposal_id` | UUID FK | Nullable proposal |
| `decision_snapshot_id` | UUID FK | Nullable immutable large-result reference |
| `actor_id` | UUID | `NOT NULL` |
| `action` | TEXT | `NOT NULL` |
| `entity_type`, `entity_id` | TEXT, UUID | Audited aggregate |
| `outcome` | TEXT | `applied`, `rejected`, `refused`, or `failed` |
| `reason_code` | TEXT | `NOT NULL` |
| `payload_json` | TEXT | Nullable canonical JSON evidence summary |
| `occurred_at` | timestamp | `NOT NULL` |

Indexes support `(entity_type, entity_id, occurred_at DESC)` and
`(proposal_id, occurred_at)`.

Every mutation attempt has one audit row correlated by `command_id`. Applied,
rejected, refused, and failed terminal outcomes commit with the corresponding
terminal command result. Audit entity references expose public UUIDv7 values;
their physical typed references use integer row IDs when the entity type is
known. Potentially large before/after collections live in a typed immutable
decision snapshot and are paginated; the audit row stores its ID and counts.

### `outbox_event`

| Field | Type | Constraints and meaning |
|---|---|---|
| `id` | UUID | Public event and delivery identity |
| `command_id` | UUID FK | `NOT NULL`; mutation that emitted the event |
| `event_ordinal` | INTEGER | `NOT NULL CHECK (event_ordinal >= 0)` |
| `aggregate_type`, `aggregate_id` | TEXT, UUID | Source aggregate |
| `event_type` | TEXT | `NOT NULL` |
| `payload_json` | TEXT | `NOT NULL` canonical JSON |
| `occurred_at` | timestamp | `NOT NULL` |
| `published_at` | timestamp | Nullable operational delivery state |
| `attempt_count` | INTEGER | `NOT NULL DEFAULT 0` |
| `last_error` | TEXT | Nullable |

Outbox constraints are:

- `UNIQUE(command_id, event_ordinal)` prevents duplicate events on retry.
- A partial `(occurred_at, id) WHERE published_at IS NULL` index supports polling.
- Domain changes and outbox rows commit in one transaction.
- Only delivery fields are mutable.
- Spec 062 has one trusted in-process consumer. The single `published_at` field
  is not a multi-audience delivery protocol.
- Payloads use bounded, redacted event DTOs.
- `last_error` is a bounded safe code and summary. It excludes raw paths, source
  payloads, exception dumps, and secrets.

## State transitions

| Aggregate | Allowed transition | Required guard |
|---|---|---|
| Materialization operation | `ready → applying` | CAS on `state_version`; command payload matches |
| Materialization operation | `applying → cancelling` | Authorized cancellation request and current command fence |
| Materialization operation | `cancelling → cancelled` | Final transaction has not committed; no domain output exists |
| Materialization operation | `applying → applied` | Sessions, memberships, singleton groups, terminal command result, audit, and outbox all inserted |
| Materialization operation | `applying → failed` | Failure state, terminal command result, and audit commit atomically |
| Relation proposal | `pending → accepted` | Exact inputs remain heads/current; all validation passes; one atomic commit |
| Relation proposal | `pending → rejected` | Reason and rejection fingerprint inserted atomically |
| Relation proposal | `pending → stale` | Its own CAS transaction confirms `pending` and the expected `proposal_revision`; no acceptance rows are retained |
| Panel or mosaic | `active → retired` | Successor identities and acyclic lineage inserted in the same transaction |
| Project membership | head revision CAS | Lifecycle allows change; exact old head still current |
| Materialization | head snapshot CAS | Base snapshot and target membership remain current; zero collisions |
| Update View plan | `approved → applying` | Current authorization, exact digest, revisions, root identities, and command fence |
| Update View plan | `applying → stopped` | Recoverable interruption recorded without publishing heads |
| Update View plan | `stopped → applying` | New command ID and CAS-claimed lease generation; reconcile intents first |
| Update View plan | `applying → failed` | Proven non-resumable terminal outcome; no resume under this plan |

Accepted sessions, memberships, group revisions, mosaic edges, project
membership revisions, materialized entries, decisions, rejections, audit rows,
and configuration versions have no update or delete transition.

## Transaction and concurrency rules

All domain write commands acquire one `sqlx::SqliteConnection` and use that
same acquired connection for `BEGIN IMMEDIATE`, reads, repository precommit
queries, writes, and commit. They enable `PRAGMA foreign_keys = ON`, WAL
journaling, `PRAGMA synchronous = FULL`, and a configured busy timeout.
Connection initialization verifies the effective synchronous mode. Install-
intent and item-journal commits require this power-loss durability barrier before
the related filesystem step; a weaker connection mode refuses Update View apply.

1. Start proposal acceptance, ingestion application, correction, group lineage,
   project membership, and materialization apply with `BEGIN IMMEDIATE`. Writer
   lock wait is measured separately from command execution.
2. Read every expected head, state version, configuration UUID, and evidence
   digest inside that transaction.
3. Insert immutable rows before moving a head. Deferred foreign keys permit a
   new group and its initial revision to be created together.
4. Move a head with one CAS statement, for example `UPDATE panel_group SET
   head_revision_id = ?, head_generation = head_generation + 1 WHERE id = ? AND
   head_revision_id = ? AND head_generation = ? AND status = 'active'`.
5. Require `changes() = 1`. Otherwise roll back the complete transaction. A
   proposal becomes stale only through a separate CAS transaction whose update
   matches the proposal ID, `state = 'pending'`, and expected
   `proposal_revision`.
6. Insert the audit event and outbox events before commit.
7. Commit only after deferred foreign keys and invariant validation queries
   pass. Any failure rolls back all membership, edge, lineage, pin, audit, and
   outbox changes from that attempt.

`BEGIN IMMEDIATE` serializes the short acceptance window without holding a
writer lock during geometry computation. Geometry and proposal construction
run before the transaction against exact immutable inputs. Acceptance only
revalidates their digests and heads.

Recursive CTEs are required for:

- detecting a proposed session-supersession, panel-lineage, or mosaic-lineage
  cycle;
- expanding current mosaic connectivity from accepted edges;
- detecting whether a new edge bridges two accepted mosaic components;
- validating that every accepted mosaic panel is reachable from one seed;
- expanding group lineage and revision ancestry for history views.

Exact project-pin and materialization-snapshot differences use ordinary joins
and `EXCEPT`; they are set comparison, not graph traversal.

Boolean reachability CTEs project only the node key and use `UNION`, so visited nodes
deduplicate correctly. They use a target-scoped indexed seed and request
`configured_ceiling + 1` rows from SQLite. The sentinel row proves truncation;
its presence returns the typed topology-limit error and no partial result. A
complete component of exactly the configured ceiling remains distinguishable.
A depth value is not included in the deduplicated row because doing so would
make repeated nodes distinct.

Path-reporting CTEs follow a separate formulation:

- Use `UNION ALL` and carry an explicitly delimited visited-node token path.
- Reject a successor already present in that path.
- Request the returned-row ceiling plus one sentinel and enforce the depth
  ceiling, failing closed on either limit.
- Use this form only when callers need the path; ordinary reachability remains
  node-only.
- Never run at startup or load the complete topology.

Both lineage tables have indexes beginning with the predecessor key and
separate indexes beginning with the successor key.

Traversal previews that expose per-node depth do not use the node-only CTE.

- They run bounded iterative breadth-first search against indexed adjacency
  under one read watermark.
- Separate visited-node and visited-edge sets assign minimum depth and deduplicate
  cycles.
- Each frontier checks depth, node, and edge ceilings with a ceiling-plus-one
  sentinel before publishing progress.
- A sentinel returns the typed topology-limit result and no domain write.
- Boolean reachability and commit-time cycle checks keep the node-only CTE.

## Index plan

| Query | Index |
|---|---|
| Command retry and lease recovery | unique `command_execution(command_id)` plus partial `(lease_expires_at, command_id) WHERE state IN ('received', 'executing')` |
| Session lists | `(kind, created_at DESC, public_id)`; `(canonical_target_id, created_at DESC, public_id)`; optional date filters extend each prefix before the cursor keys |
| Operation result | `session(materialization_operation_id, ordinal_in_operation)` |
| Frame lookup | `session_frame(frame_id, session_id)` |
| Current-session test | `session_supersession(predecessor_session_id)` and `(replacement_session_id)` |
| Equipment resolution | normalized partial unique alias index plus `(equipment_kind, normalized_value)` |
| Calibration candidate | partial unique `(camera_id, kind, identity_digest) WHERE kind IN ('dark', 'bias')`; partial unique `(optical_profile_id, filter_label_id, identity_digest) WHERE kind = 'flat'` |
| Calibration recency | `calibration_session(family_id, age_anchor_at_utc DESC, session_id)` |
| Panel head lists | head-visibility rows by `(target_scope, accepted_sequence DESC, panel_group_id)` and session-membership lookup by current revision |
| Panel history | `panel_group_revision(panel_group_id, revision_number DESC, public_id)` |
| Panel member lookup | `panel_revision_session(session_id, panel_revision_id)` |
| Mosaic ordered members | `(mosaic_revision_id, ordinal, panel_revision_id)` and `(mosaic_revision_id, ordinal, edge_evidence_id)` |
| Mosaic history | `mosaic_revision(mosaic_id, revision_number DESC, public_id)` |
| Mosaic objects | `(mosaic_revision_id, canonical_object_id)` |
| Edge traversal | edge indexes beginning with each endpoint revision integer key |
| Panel lineage | `(predecessor_group_id, successor_group_id)` and `(successor_group_id, predecessor_group_id)` |
| Mosaic lineage | `(predecessor_mosaic_id, successor_mosaic_id)` and `(successor_mosaic_id, predecessor_mosaic_id)` |
| Proposal queue | `(state, kind, created_at DESC, public_id)` plus target- and subject-scope junction indexes |
| Ordered proposal and decision results | every child collection has `(owner_snapshot_id, ordinal, child_public_id)` or its documented composite identity |
| Rejection suppression | unique rejection fingerprint index |
| Project pins | `project_membership_revision_session(session_id, revision_id)` |
| Related-session indication | session target, panel member, and current-head indexes; no stored project mutation |
| Update View plans | plan-session, item, conflict, overlay-preview, install-intent, and journal indexes beginning with plan integer key and ending with ordinal/public identity |
| Reclassification results | every plan/apply snapshot child has `(snapshot_id, ordinal, child_public_id)` or its documented composite identity |
| Stale views | materialization membership and project head UUID indexes |
| Watermarked projections | visibility/head-history indexes beginning with aggregate/filter keys and ending with sequence plus public ID |
| Audit history | `(entity_type, entity_id, occurred_at DESC, id)` |
| Outbox delivery | partial `(occurred_at, id) WHERE published_at IS NULL` |

Every footprint evidence revision stores conservative unit-sphere Cartesian
bounds `bbox_min_x_ppb` through `bbox_max_z_ppb`, with each coordinate in
`[-1_000_000_000, 1_000_000_000]` and ordered min/max checks. This avoids an
RA-zero split and remains valid at the poles.

The baseline schema includes a three-dimensional SQLite `rtree_i32` virtual
table keyed by the footprint-evidence integer `row_id`.

- Unit-sphere minima are floored to parts per billion.
- Unit-sphere maxima are ceiled to parts per billion.
- The outward bounds make every RTree result a conservative superset; exact WKB
  geometry removes false positives.
- Repository writes maintain the evidence row and RTree entry in one
  transaction. Append-only triggers prevent direct-update drift.
- A rebuild verifies or recreates the RTree from typed bounds. WKB remains
  authoritative for exact intersection.

Candidate discovery first narrows by canonical target or reviewed cross-target
association, equipment/profile identity, parity, and mandatory `rtree_i32`.
Exact WKB intersection and transported-orientation checks run only on that
bounded set.

Property tests cover quantization cell boundaries, RA zero, both poles, and
random footprints, proving that the RTree shortlist never omits an exact WKB
intersection candidate.

## Commit-time invariants

The repository executes these invariant queries before every relevant commit
under `BEGIN IMMEDIATE`. SQLite cannot express their cross-table and exact-
cardinality semantics as native constraints. Database checks and triggers
enforce only local row rules and append-only behavior. No writer bypasses this
repository boundary.

1. Every session has at least one frame and exactly one representative.
   Within one materialization operation, every frame belongs to exactly one
   output session and every membership repeats the parent operation ID.
2. Every current, non-superseded light session appears in exactly one active
   panel head for its target scope.
3. A current panel head never contains both a superseded session and any of its
   replacements.
4. A current non-superseded light session belongs to at most one active panel
   head.
5. Same-session membership and sibling relation are mutually exclusive for a
   session pair.
6. Every panel or mosaic head references a revision of the same stable group.
7. Every mosaic edge endpoint is an exact panel revision in that mosaic
   revision, and the accepted edge set connects all members.
8. Panel lineage, mosaic lineage, and session supersession are acyclic.
9. Every proposal decision references the exact immutable inputs and
   configuration version reviewed by the actor.
10. Project membership consists only of exact session UUIDs and changes only
    through an accepted explicit proposal in an allowed lifecycle state.
11. A materialization update contains only sessions absent from its base
    snapshot and never changes an existing destination entry.
12. Red calibration reuse and every unregulated-dark selection has an accepted
    audited manual decision.
13. No persisted supported-kind enum contains `dark_flat`.
14. Every Inbox, materialization, reclassification plan, reclassification apply,
    proposal-decision, and Update View count equals its complete ordered typed
    snapshot collection.
15. A reviewed cross-target association is created only with the accepted
    manual-relation proposal and its first accepted relation revision; its exact
    target set has at least two members.
16. Every reclassification destination identity, predecessor retirement, and
    lineage edge equals the approved plan-result snapshot, and apply creates the
    complete set or none of it.
17. Every Update View journaled item has a same-plan install intent whose stable
    ownership evidence, collision key, fingerprint, and adopted command fence
    match. A published successor snapshot has no unjournaled plan item.
18. Every cursor watermark resolves through append-only visibility/head history;
    no page is evaluated against a newer mutable head.
19. Every domain mutation that consumers must observe has an outbox row in the
    same transaction.
20. Automatic same-session and same-panel partitions compare every candidate
    directly with the immutable representative. They never expand by
    transitive pairwise chaining.
21. A handoff snapshot contains every frame membership for every selection;
    each stored frame passed same-handle identity and content verification.
    Snapshot-requirement, snapshot-selection, selection-evidence, and frame
    references cannot cross handoff, requirement, session, or selection scope.
22. A completed materialization snapshot publishes its exact session set,
    manifest, active overlays, and both project heads in one CAS transaction.
