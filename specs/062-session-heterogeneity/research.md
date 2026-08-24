# Research: Immutable Sessions and Observation Groups

**Feature**: 062-session-heterogeneity

**Date**: 2026-07-21
**Decision status**: Accepted for planning

## Scope and evidence

This document resolves the implementation choices required by
[`spec.md`](spec.md). It does not define a second set of requirements.

Repository evidence at HEAD establishes these constraints:

- Light ingestion finds an `acquisition_session` by `session_key` and appends a
  file ID to a JSON array. A later ingestion with matching metadata therefore
  mutates the same session
  (`crates/app/targets/src/ingest_sessions.rs:459`).
- Missing acquisition-site data is replaced with UTC. Missing or invalid
  `DATE-OBS` is replaced with the wall-clock time
  (`crates/app/targets/src/ingest_sessions.rs:409`).
- Session keys contain target, filter, combined binning, gain, and night only
  (`crates/sessions/src/key.rs:60`).
- `acquisition_session.frame_ids` and `calibration_session.frame_ids` are JSON
  arrays. `project_sources` is a mutable relational exact-session link, not an
  immutable project-membership revision
  (`crates/persistence/core/migrations/0001_initial_schema.sql:2009`).
- The framing model mutates one `framing` row and one live membership join.
  Deleting or reassigning membership loses the accepted topology that preceded
  it (`crates/persistence/core/migrations/0001_initial_schema.sql:916`,
  `crates/persistence/core/src/repositories/framing.rs:168`).
- Registered cameras, telescopes, and optical trains have stable IDs. Camera
  aliases are stored as JSON and resolved with exact alias values
  (`crates/persistence/core/migrations/0001_initial_schema.sql:467-470`,
  `crates/persistence/core/src/repositories/equipment.rs:245`).
- Calibration fingerprints store nullable scalar metadata and support dark,
  flat, and bias (`crates/persistence/core/migrations/0001_initial_schema.sql:198`).
- Dark-flat is reserved in domain and contract enums, but it remains reachable
  in Inbox classification, grouping, override, and display helpers
  (`crates/metadata/core/src/lib.rs:24`,
  `crates/app/inbox/src/grouping/config.rs:66`,
  `apps/desktop/src/features/inbox/planPanelHelpers.ts:47`).
- SQLite runs in WAL mode with a five-second writer timeout
  (`crates/persistence/core/src/lib.rs:32`,
  `crates/persistence/core/src/lib.rs:79`).
- The workspace uses `skymath` 0.5 and `target-match` 0.4. The targeting layer
  already delegates spherical separation, optics-to-field conversion, and
  rotation-aware point containment to them
  (`crates/targeting/Cargo.toml:30`, `crates/targeting/src/coords.rs:85`,
  `crates/app/inbox/src/target_recommendations.rs:160`).
- The Sessions page receives the complete inventory response and filters it in
  the browser. Its table already uses row virtualization
  (`apps/desktop/src/features/sessions/SessionsPage.tsx:62`,
  `apps/desktop/src/features/sessions/SessionsTable.tsx:19`).
- Rust contract DTOs feed the generated TypeScript bindings through the Tauri
  command boundary (`crates/contracts/core/src/sessions.rs:4`,
  `apps/desktop/src-tauri/src/bootstrap/specta.rs:1`).

## D1 — A generic materialization operation is the immutable session boundary

**Decision**: Add a durable `session_materialization_operation` with
`inbox_ingestion` and `metadata_reclassification` subtypes. The generic command
ledger owns retry identity. Materialization creates sessions and `session_frame`
rows inside that operation. Accepted session membership has no update or delete
use case.

The Inbox subtype stores its approved plan digest and approval provenance. The
metadata-reclassification subtype stores its predecessor and correction basis.
The operation stores its input fingerprint, terminal outcome, and created
session IDs. Repeating the command ID returns the recorded operation outcome. A
later approval receives another command and operation ID and creates new
sessions even when its identity metadata equals an accepted session.

Session formation within one operation uses exact discriminator values plus a
complete-linkage geometry check against an immutable representative. A file may
belong to one materialized session for that operation. File identity remains
the existing `file_record.id`; path is provenance, not session identity.

**Rationale**:

- Operation identity distinguishes a retry from new work. Metadata equality
  cannot make that distinction.
- Normalized `session_frame` rows make membership indexable and prevent a
  read-modify-write race on JSON arrays.
- Immutable membership allows projects, proposals, and audit entries to name a
  stable input without copying the full session.

**Consequences**:

- The append path in `upsert_session` is replaced, not extended
  (`crates/app/targets/src/ingest_sessions.rs:486`).
- `UNIQUE (materialization_operation_id, file_record_id)` prevents duplicate
  materialization within a retry.
- A unique operation ID and stored input fingerprint reject accidental reuse of
  an ID for different input.
- Identity-changing corrections create replacement sessions and a
  `session_supersession` edge. They do not update identity columns or
  membership rows.
- Every light-session insert creates its singleton PanelGroup, initial revision,
  audit row, and outbox events in the same Phase 1 transaction. Failure commits
  none of those domain rows.

## D2 — Observing night uses civil noon and an IANA timezone

**Decision**: Store the confirmed acquisition site and its IANA timezone name.
Convert one canonical exposure instant through the timezone rules effective on
the exposure date. The observing-night key is the local calendar date at or
after 12:00, or the preceding date before 12:00.

Canonical instant precedence is format-specific and belongs in the metadata
profile described in D3. A local timestamp may corroborate the result. If UTC
and local evidence produce different noon-to-noon dates, automatic
materialization stops for review.

Use a maintained IANA timezone library at the application boundary. Keep the
pure session-domain function parameterized by the resolved local datetime.
`UtcOffset` alone is insufficient because the stored offset must vary with the
exposure date (`crates/sessions/src/key.rs:15`).

**Rationale**:

- Civil noon is deterministic and matches the specification without requiring
  solar-position or sidereal calculations.
- A timezone name preserves daylight-saving and historical offset rules. A
  fixed offset does not.
- Blocking contradictory identity evidence prevents a clock or machine
  timezone from silently moving frames between immutable sessions.

**Consequences**:

- UTC fallback and `now_utc()` fallback are removed from identity derivation
  (`crates/app/targets/src/ingest_sessions.rs:416`,
  `crates/app/targets/src/ingest_sessions.rs:438`).
- An absent site timezone may use a supplied local timestamp only with a visible
  degraded-evidence state.
- Tests cover exact noon, both sides of a daylight-saving transition, remote
  sites, invalid timestamps, and conflicting timestamp fields.

## D3 — Metadata profiles map capture software into typed evidence

**Decision**: Represent capture-software metadata as versioned profiles. A
profile maps semantic fields to ordered header candidates, parser rules, units,
and evidence quality. The normalized metadata record stores value, state
(`known`, `absent`, `invalid`, or `contradictory`), source field, raw value, and
profile version.

The first registry covers fields required by this feature:

| Semantic area | Fields |
|---|---|
| Time and site | canonical exposure instant, local timestamp, site timezone |
| Camera | instrument name, device-specific name, gain, offset, readout mode, cooling set point, sensor temperature |
| Raster | width, height, horizontal binning, vertical binning, supplied crop or subframe evidence |
| Optics | telescope or image-train name, reported focal length, pixel size, physical rotator angle |
| Sky solution | centre, four captured corners or WCS transform, solved orientation, parity |
| Acquisition | frame type, captured filter label, exposure, target evidence |

Profiles normalize header vocabulary. They do not identify equipment. Profile
results resolve against stable `camera_id` and `optical_profile_id` rows through
reviewed aliases and representative evidence.

An optical profile is a registered image-train identity. It records its camera,
telescope or lens, representative effective focal length, and optional physical
orientation reference. A flat filter identity is the normalized captured label
scoped to that optical profile. It is not a global `filters.id`.

**Rationale**:

- The existing metadata table has one nullable column per known field but no
  durable mapping from capture software to field meaning
  (`crates/persistence/core/migrations/0001_initial_schema.sql:1020`).
- The registered-equipment IDs provide stable anchors while aliases absorb
  capture-software spelling differences
  (`crates/persistence/core/migrations/0001_initial_schema.sql:467`).
- Explicit value states preserve the specification's absent-versus-known
  matching rules.

**Consequences**:

- Camera aliases may unify standard and device-specific metadata when the
  evidence names one physical camera.
- Two indistinguishable physical cameras may resolve to one ID. The UI discloses
  the evidence used.
- Effective focal length compares against one immutable optical-profile
  representative. It does not chain through pairwise near-matches.
- A profile change affects later extraction and relation proposals. It does not
  reinterpret accepted sessions.

## D4 — Calibration identity and reuse are separate policies

**Decision**: Persist calibration acquisition sessions separately by observing
night. Persist recipe identity separately from reuse eligibility.

| Kind | Recipe identity | Evidence outside identity |
|---|---|---|
| Dark | camera, temperature mode and exact set point, normalized exposure, exact gain, offset state/value, X/Y binning, readout state/value, raster | sensor-temperature distribution, age, frame count |
| Bias | camera, exact gain, offset state/value, X/Y binning, readout state/value, raster | age, frame count |
| Flat | optical profile, scoped captured filter label, exact gain, offset state/value, X/Y binning, readout state/value, raster, physical orientation | exposure distribution, age, orientation severity |

Dark exposure equivalence uses the immutable recipe representative and the
specified `max(1 ms, min(100 ms, 0.05%))` tolerance. Flat exposure is retained
as evidence and does not split a session or relation.

Store policy versions and measured evidence with each recommendation. Age,
thermal stability, and physical rotation produce `normal`, `yellow`, `red`, or
`unknown` severity. Yellow remains eligible where the specification permits it.
Red and unknown require the specified reviewed path.

For the external-processing handoff, a calibration session is **sufficient**
when all required recipe evidence is complete and at least one indexed source
frame is available and readable at preview. There is no minimum frame count and
no scientific-quality score. The application selects the newest sufficient
compatible dark or bias session automatically; any additional session requires
explicit reviewed selection. WBPP, Siril, or PixInsight decides whether the
provided frames are scientifically adequate.

Candidate preview may evaluate age at an explicit caller-supplied time for
comparison. Handoff creation always derives its evaluation instant from the
trusted core clock and stores it in the immutable basis. A sufficient candidate
is not necessarily executable: before snapshot commit, every frame in every
selected session must be readable and strongly verified from the same opened
handle used for handoff. If any frame fails, the whole session selection is
blocked; the application never silently drops individual frames.

Unknown dark cooling mode cannot enter automatic assignment, build, or reuse.
Marking a camera unregulated creates a policy decision for later sessions. It
does not alter accepted sessions. Native master construction remains outside
this feature; selection hands exact source sessions to the external processor.

**Rationale**:

- The matching code treats dark exposure and temperature as soft confidence
  dimensions (`crates/calibration/core/src/rules/dark.rs:69`). The feature
  requires exposure in recipe identity and temperature stability as measured
  reuse evidence.
- Bias code already excludes exposure and temperature from matching
  (`crates/calibration/core/src/rules/bias.rs:16`).
- Existing fingerprints lack separate X/Y binning, readout state, raster,
  cooling mode, distributions, and policy provenance
  (`crates/persistence/core/migrations/0001_initial_schema.sql:198`).

## D5 — Dark-flat detection terminates before Inbox materialization

**Decision**: Keep `DarkFlat` in internal parsers and reserved domain enums.
When scan classification detects it, record a diagnostic counter or log entry
and return no Inbox candidate. Do not create source sub-items, breakdown rows,
plan items, sessions, match candidates, or user controls.

Every defensive downstream conversion from `DarkFlat` returns `None` or a typed
unsupported error. No wildcard arm may map it to dark, flat, or bias. Manual
reclassification excludes it from both source and destination choices.

**Rationale**:

- Detection support avoids misclassification.
- The present master-registration fallback maps any unrecognized type to dark
  (`crates/app/inbox/src/plan_listener.rs:296`). Early termination and explicit
  downstream guards close that leak.
- UI filtering alone is insufficient because Inbox and plan rows would still
  exist.

**Consequences**:

- Negative tests start at scan input and assert zero rows across Inbox,
  session, match, and plan tables.
- Reserved generated contract variants may remain for compatibility, but no
  reachable response returns them.

## D6 — PanelGroup and Mosaic use stable identities with immutable revisions

**Decision**: Replace mutable project framing membership with two global
relationship layers:

1. `panel_group` is the stable logical same-pointing identity.
2. `panel_group_revision` is an immutable accepted session-membership snapshot.
3. `mosaic` is the stable logical identity for reviewed adjacent panels.
4. `mosaic_revision` names exact panel revisions and exact accepted adjacency
   evidence.

Each stable group has one accepted-head revision ID. Revisions store parent
revision, configuration version, representative evidence, actor, reason, and
decision provenance. Membership and adjacency use normalized child tables.

Every materialized light session receives a singleton panel group and initial
revision in the ingestion transaction. Adding a sibling or replacing a
superseded session creates a successor revision under the same logical group.
A split, merge, or identity change creates new logical group IDs and
`group_lineage` edges from retired predecessors.

Relation proposals snapshot their base head IDs, candidate membership,
evidence, thresholds, and proposal basis hash. Rejections persist that basis
hash and reason. Equivalent evidence stays suppressed until the evidence or
configuration hash changes.

**Rationale**:

- Stable IDs let users and projects refer to the concept of a panel without
  making its membership mutable.
- Exact revision references make mosaic evidence reproducible.
- A proposal based on stale heads can fail as one atomic conflict instead of
  partially changing topology.

**Consequences**:

- Same-session and sibling are mutually exclusive classifications.
- One non-superseded session has at most one active panel membership per target
  or reviewed cross-target association.
- Lineage writes check for cycles before accepting a split or merge.
- Existing `framing` and `framing_session` data need no compatibility bridge
  because the feature assumes resettable development databases
  (`crates/persistence/core/migrations/0001_initial_schema.sql:941`).

## D7 — Normalized SQLite remains the topology store

**Decision**: Keep SQLite and add normalized tables with foreign keys, unique
constraints, partial indexes, and immutable-row guards. Do not store topology
or membership as JSON.

Acceptance follows one transaction protocol:

1. Acquire a pooled connection.
2. Execute `BEGIN IMMEDIATE` to take the writer reservation before validation.
3. Re-read every referenced session, revision, and accepted head.
4. Validate uniqueness, supersession, lineage acyclicity, and proposal basis.
5. Insert the revision, memberships, edges, lineage, decision, and audit row.
6. Compare-and-swap the accepted head.
7. Commit, or roll back the entire acceptance.

`BEGIN IMMEDIATE` makes the stale-head race deterministic. WAL still permits
readers while SQLite serializes the competing writers. The repository already
tests this locking behavior (`crates/persistence/core/tests/two_writer_contention.rs:75`).

Use recursive CTEs for bounded ancestry, descendants, accepted mosaic
connectivity, bridge detection, and cycle checks. Every recursive query is
target-scoped or group-scoped, deduplicates visited IDs, and has an explicit
depth or row guard. Lists and candidate discovery use indexed non-recursive
prefilters before graph traversal.

Minimum index families are:

- session identity and operation membership;
- active panel membership by session and target;
- revision parent and stable-group head;
- mosaic edge endpoints and revision membership;
- lineage predecessor and successor;
- proposal status, basis hash, and source revision;
- project session pins and unmaterialized additions;
- calibration recipe lookup and age ordering.

Every externally callable mutation uses the generic command ledger for one
recorded result per command ID. Domain events use the generic outbox and commit
with their aggregate changes. Neither concern receives a Spec 062-only retry or
delivery mechanism.

**Rationale**:

- The topology is relational, transaction-bound, and local to one desktop
  database. A graph database adds another system without removing SQLite
  transaction requirements.
- Recursive CTEs cover lineage and connected-component reads at the specified
  scale when coarse predicates and indexes reduce the starting set.
- Normalized rows allow foreign keys and uniqueness constraints to enforce
  invariants that JSON arrays cannot enforce.

**Consequences**:

- Add one forward migration. Do not edit applied migration files; the migration
  runner validates embedded checksums (`crates/persistence/core/src/lib.rs:168`).
- Writer-lock wait is measured separately from acceptance execution time.
- The scale gate must prove the CTE plans with `EXPLAIN QUERY PLAN` and realistic
  cardinalities before any closure table or graph accelerator is considered.

## D8 — Geometry is split between skymath, target-match, and metadata

**Decision**: Use solved captured footprints as ordered sky boundaries plus
centre, solved sky orientation, parity, and evidence provenance. Keep pure
spherical transport and projection in skymath, reusable footprint topology in
target-match, and FITS/XISF WCS interpretation in Astro Plan metadata.

The required dependency order is:

1. Release the already reviewed skymath position-angle transport and gnomonic
   projection work tracked by `astro-plan-ic9h.1`.
2. Add opaque footprint comparison, rotation-interval, and hole-aware union
   APIs to target-match, tracked by `astro-plan-ic9h.2`. Use a maintained
   polygon-boolean library internally and do not expose its types.
3. Extend Astro Plan metadata extraction to preserve the full WCS transform,
   reference pixel, image dimensions, determinant/parity, distortion support,
   and raw provenance needed to construct the target-match footprint.

Target-match projects one comparison set onto a deterministic common gnomonic
tangent plane. Pair comparison uses the spherical midpoint; a union persists
one anchor for the whole input set. Boolean operations preserve holes and
disconnected components. Projection, antipodal, invalid-boundary, and
unsupported-WCS failures are typed failures that require manual relation
review. No layer silently substitutes a bounding box, cone, or approximate
mechanical rotation.

Coverage is `intersection area / smaller footprint area`. Centre separation is
an independent normalized diagonal guard. Orientation uses transported solved
sky axes, normalized modulo 180 for image-axis comparison, with parity checked
separately. Mechanical rotation remains display evidence and never substitutes
for solved sky orientation.

Coverage area is measured in that one common gnomonic plane. Numerical tests
compare it with a spherical reference across the supported field-size envelope
and fail when the configured error bound is exceeded. Rotation eligibility is
returned as a set of closed intervals because rectangular symmetry and
non-monotonic coverage can produce multiple disjoint ranges.

**Rationale**:

- HEAD already uses upstream spherical and field primitives
  (`crates/sessions/src/clustering.rs:563`,
  `crates/targeting/src/coords.rs:122`).
- The existing clustering compares centre and rotation only. It does not model
  captured intersection area (`crates/sessions/src/clustering.rs:186`).
- Polygon boolean operations have enough edge cases that target-match must use
  a maintained library rather than a local clipping implementation.

**Consequences**:

- Missing footprint, solved orientation, or parity blocks automatic spatial
  classification.
- Candidate generation first restricts by canonical target, compatible
  equipment, parity, and bounding radius. Polygon work runs only on that set.
- Same-session and sibling matching use immutable representatives and
  complete-linkage behavior. Accepted mosaic connectivity may be transitive
  across explicit edges.
- A bridge between accepted mosaic components creates a merge proposal. It does
  not combine them.
- Current metadata extraction is insufficient because it lacks a complete CD
  or PC+CDELT matrix, reference pixels, and reliable parity. Unsupported SIP,
  TPV, or other distortion terms remain visibly unresolved unless the selected
  WCS implementation handles them accurately.

## D9 — Mosaic object results use the captured footprint union

**Decision**: Treat mosaic object detection as catalogue-object coverage, not
image-content detection. Compute the polygon union of exact panel footprints in
the accepted mosaic revision. Preserve disconnected regions and holes.

For each canonical catalogue object:

- point-like or unknown-extent: include only when its coordinate lies in the
  union;
- extended with major axis, minor axis, and position angle: project an extent
  ellipse into the same tangent plane and intersect it with the union;
- zero intersection: exclude;
- partial intersection: include with `partially_covered` and an estimated
  fraction;
- full intersection: include with normal coverage.

Store or fetch optional extent values with provenance. Do not infer an extent
from object type. Deduplicate by canonical target ID while retaining each
session and panel containment record.

**Rationale**:

- The target contracts contain coordinates and magnitude but no angular extent
  (`crates/contracts/core/src/targets.rs:176`).
- The existing cone-search path refines a circular query to one rectangular
  point-containment test (`crates/app/inbox/src/cone_search.rs:321`). It cannot
  preserve mosaic gaps or estimate extended-object coverage.
- Union geometry directly enforces exclusion of an object wholly inside an
  uncaptured gap.

**Consequences**:

- Intended target identity remains an independent reviewed association.
- Object coverage never crops, masks, or writes an image.
- Extent sampling resolution and approximation error become benchmarked test
  parameters of the footprint adapter.

## D10 — Project membership is revisioned and filesystem views update additively

**Decision**: For the greenfield schema, replace `project_sources` as authority
with immutable `project_membership_revision` rows and normalized exact-session
membership. A project stores one compare-and-swap membership head. Do not pin a
panel group, mosaic, or revision as a live membership source.

A group-derived project action expands once into a sorted immutable session-ID
snapshot. The proposal and approval record that snapshot. Later group revisions
cannot change the project.

Adding or replacing a session creates a successor membership revision in
allowed lifecycle states. If a materialized source view exists, the new head is
stale until its current-session manifest is materialized. `Update View`
snapshots the prior materialized membership and builds plan items only for
later pins.

The existing generic plan/approve/apply machinery remains the only filesystem
executor. Spec 062 adds extension tables for the project basis, plan items,
per-item journal, and completed materialization snapshots. All destination
paths are canonical safe-relative paths beneath an approved destination root.
Traversal does not follow symlinks or other redirecting filesystem objects.
Apply revalidates the source identity and content fingerprint immediately
before each item.

Filesystem apply has these semantics:

- Plan generation and apply preflight inspect every destination.
- Any known collision refuses the operation before the first write.
- A collision introduced after preflight stops before the affected item.
- A source-fingerprint change or another runtime filesystem failure also stops
  before the affected item.
- Successful earlier items remain on disk and in the operation journal.
- Retry recognizes a destination only when the same operation created it and
  its recorded fingerprint still matches.
- An unrelated occupant remains a collision.
- The executor publishes no completed materialization snapshot until every plan
  item succeeds.

A correction offers an explicit old-to-new project proposal. Acceptance creates
a successor project-membership revision. Existing filesystem entries remain
immutable as a historical overlay. The versioned current processing manifest
excludes the predecessor session and includes the approved replacement. It does
not delete or reinterpret the predecessor's historical entries.

**Rationale**:

- Exact pins prevent background relation changes from altering processing
  inputs.
- Prepared views demonstrate the existing reproducible per-item projection
  boundary (`crates/persistence/core/migrations/0001_initial_schema.sql:1804`).
- Extension tables preserve one executor and one set of filesystem safety
  semantics.

**Consequences**:

- Related-session availability is a query result or durable notification, not a
  membership mutation.
- Completed and archived projects reject additions.
- Removal rules remain asymmetric and continue to follow project lifecycle
  policy.

## D11 — Contracts expose evidence; the UI loads bounded slices

**Decision**: Add Rust-first DTOs in `crates/contracts/core` and Tauri commands
for these surfaces:

- paginated session and panel-group list;
- session and immutable-membership detail;
- relation proposal list, evidence detail, accept, and reject;
- panel and mosaic revision history;
- related sessions available for a project;
- project add-session and Update View preview;
- camera, optical-profile, metadata-profile, and calibration-policy settings;
- calibration recipes, compatibility evidence, and reviewed selection;
- metadata-correction preview and acceptance.

List requests carry server-side filters, stable sort keys, cursor pagination,
and hard maximum page sizes. Detail requests load one group or proposal.
Candidate discovery, recursive traversal, object evidence, and export requests
carry explicit row, depth, work, and payload bounds with typed limit errors. No
startup request loads the full topology.

The Sessions ledger groups rows by panel group. Primary rows show identity,
night, filter, frame totals, equipment, relation state, and project pins.
Measured geometry, thresholds, lineage, calibration evidence, and rejection
reason live in the detail pane. Proposal acceptance and red calibration reuse
require an explicit review action with audit reason.

Generated TypeScript bindings remain the frontend type source. The existing
handwritten fixture types are updated from those bindings rather than defining
parallel contract shapes (`crates/contracts/core/src/sessions.rs:67`).

**Rationale**:

- Browser-side filtering of a complete inventory response cannot meet the
  100,000-session fixture target
  (`apps/desktop/src/features/sessions/SessionsPage.tsx:134`).
- Bounded list DTOs separate list performance from evidence-rich detail.
- The existing virtualized table and fold-out detail pattern can host the new
  relationship information without placing every field in the row
  (`apps/desktop/src/features/sessions/SessionsTable.tsx:19`).

## D12 — Verification is layered and measured at the specified fixture

**Decision**: Use deterministic fixtures and five verification layers.

| Layer | Required coverage |
|---|---|
| Pure unit and property tests | identity canonicalization, explicit absent states, noon boundary, exposure tolerance, orientation normalization, parity, polygon invariants, complete linkage |
| Real SQLite integration | operation retry, later identical ingestion, normalized membership, group acceptance, stale-head race, rejection suppression, recursive lineage, calibration selection, additive project update |
| Contract and frontend tests | generated binding agreement, pagination, evidence rendering, disabled red actions, detail-pane history, project stale-view indication |
| Real-backend journey | mixed ingestion through relation review, project addition, Update View preview, correction, and calibration selection |
| Scale and benchmarks | 100,000 sessions, about 10 million frame memberships, 500,000 revisions, and 2 million relation rows |

Benchmark reports name the reference machine and separate cold-cache from
warm-cache measurements. A versioned manifest fixes generator seed,
distributions, schema hash, query/action corpus, warmups, minimum samples,
nearest-rank percentile calculation, OS page-cache eviction protocol, and raw
result schema. Writer-lock wait is reported separately. Candidate discovery
excludes metadata parsing, solving, network, and filesystem time as required by
the specification.

Local diagnostics record bounded histograms and counters by query family for
latency, writer-lock wait, candidate count, recursive rows, limit failures,
cancellation latency, and Spec 062 startup delta. Labels use allowlisted enums
and never include paths, target names, session IDs, or other high-cardinality
user data.

The stress fixture includes:

- one target with 10,000 nodes and 50,000 relations;
- disconnected mosaics joined by a proposed bridge;
- lineage chains and rejected-proposal basis hashes;
- point objects in captured panels and gaps;
- extended objects with zero, partial, and full captured intersection;
- concurrent acceptance from the same base revision;
- dark, bias, and flat sessions across every severity boundary;
- missing and contradictory identity metadata.

**Rationale**:

- The repository already has real-database ingestion tests
  (`crates/app/core/tests/ingest_sessions_integration.rs:176`) and frontend
  Sessions tests (`apps/desktop/src/features/sessions/__tests__/SessionsPage.inventory.test.tsx:145`).
- The new invariants cross domain, database, IPC, and UI boundaries. No single
  test layer can prove them.

## Rejected alternatives

| Alternative | Reason for rejection |
|---|---|
| Keep one mutable session per metadata key | Cannot distinguish retry from later acquisition and changes accepted membership. |
| Add ingestion ID to the existing string key and retain JSON frames | Separates operations but leaves unindexable membership and read-modify-write races. |
| Use folder path as session identity | Remap and move operations would change scientific identity. |
| Use machine timezone or a fixed UTC offset | Produces wrong observing nights for remote sites and date-specific timezone rules. |
| Infer missing identity metadata from adjacent frames | Converts absence into fabricated identity and can irreversibly misgroup sessions. |
| Treat capture-software profiles as equipment identities | One software profile can drive many cameras and image trains. |
| Use one global filter table for flat identity | Captured labels are meaningful only within an optical profile and may collide across equipment. |
| Let dark-flat reach Inbox and hide it in the UI | Leaves plans, sessions, and fallback conversions reachable. |
| Mutate framing membership and retain audit text only | Audit text cannot reconstruct exact accepted membership or adjacency. |
| Store a precomputed transitive closure first | Adds write amplification before recursive CTEs have failed the measured fixture. |
| Add a graph database | Duplicates transaction and deployment concerns for a topology SQLite can query locally. |
| Hand-write spherical or polygon clipping math | Duplicates upstream coordinate code and accepts a large numerical edge-case burden. |
| Use centre distance as mosaic overlap | Cannot distinguish captured overlap from a gap and fails rotated or unequal fields. |
| Use mechanical rotator angle as solved sky orientation | Mechanical angle can differ from transported sky position angle. |
| Collapse a mosaic to one bounding rectangle | Includes uncaptured gaps and loses disconnected regions. |
| Auto-follow panel or mosaic membership in projects | Allows background grouping to change processing inputs. |
| Rebuild an entire materialized view after adding a session | Rewrites accepted paths and violates additive update semantics. |
| Load the full relationship graph into the frontend | Violates startup and list latency targets at the supported fixture. |
| Validate with mocked UI tests only | Cannot prove SQLite constraints, transaction races, generated contracts, or filesystem plan behavior. |
