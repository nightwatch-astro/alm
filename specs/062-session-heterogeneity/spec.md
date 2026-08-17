# Feature Specification: Immutable Sessions and Observation Groups

**Feature Branch**: `spec/062-session-heterogeneity-artifacts`

**Created**: 2026-07-21

**Status**: Partial. US1, US3, and US4 shipped. US2 is not built: the 15 Tauri
commands invoked from
`apps/desktop/src/features/sessions/sessionsGroupsIpc.ts` (`panel_group_*`,
`mosaic_*`, `relation_proposal_*`, `matching_settings_*`) have no backend
handlers. Each name appears in that one TypeScript file and nowhere else in
`apps/`, `crates/`, or `packages/`.

**Input**: Product-owner decisions recorded in Bead `astro-plan-ic9h`: replace
mutable, folder-led session accumulation with immutable ingestion sessions;
relate light sessions through stable panel and mosaic groups; organize
calibration sessions by camera or optical profile; keep projects pinned to
explicit sessions; and make every later addition, correction, or topology
change reviewable.

## Product intent

Astrophotographers often capture one target over several nights, filters, and
pointings. A folder can mix those acquisitions, while identical metadata can
recur months later. Treating a folder or a metadata key as one indefinitely
growing session destroys the boundary between what was reviewed yesterday and
what arrived today.

This feature makes an approved Inbox ingestion the immutable boundary. It then
adds durable relationships above sessions so the application can show which
sessions belong to the same panel, which panels form a mosaic, and which
calibration sessions are compatible. These relationships help users find and
review related data without changing session membership or silently expanding
projects.

## Clarifications

### Session 2026-07-21

- Q: Does “series” introduce a separate persisted identity alongside canonical
  targets? → A: No. Spec 062 uses the canonical confirmed target identity;
  intentional cross-target grouping requires an explicit reviewed association.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Ingest immutable sessions (Priority: P1)

An astrophotographer reviews an Inbox plan containing mixed acquisitions. The
plan shows each session that will be created and the metadata that separates
it from the others. Approval creates those sessions once. A later ingestion,
even with identical metadata, creates a new related session rather than adding
files to an accepted one.

**Why this priority**: Stable session membership is the foundation for every
project, calibration match, correction, and audit record in this feature.

**Independent Test**: Approve a mixed Inbox ingestion, retry the same
operation, and later ingest additional matching files. Confirm that the retry
is idempotent, the later ingestion creates a distinct session, and no accepted
session changes its file membership.

**Acceptance Scenarios**:

1. **Given** one approved ingestion containing light frames with different
   observing nights, filters, exposures, gains, offsets, binning, readout
   modes, or image geometry, **When** the plan is applied, **Then** each distinct
   identity becomes a separate immutable session.
2. **Given** an approved session, **When** more matching frames arrive in a
   later approved ingestion, **Then** a new session is created and shown as
   related to the accepted session.
3. **Given** an approved ingestion is retried with the same operation identity,
   **When** the retry completes, **Then** it neither duplicates sessions nor
   appends files.
4. **Given** required identity metadata is missing or contradictory, **When**
   the Inbox plan is generated, **Then** automatic materialization is blocked
   or visibly degraded according to the relevant metadata rule; no value is
   invented.

---

### User Story 2 - Review same-panel and mosaic relationships (Priority: P2)

An astrophotographer sees separate sessions organized under stable panel
groups. The application can suggest that another session covers the same panel
or that several panels form a mosaic. The user reviews measured evidence before
accepting any change to an existing group.

**Why this priority**: Relationships recover the convenience that immutable
sessions intentionally give up, without returning to mutable session
membership.

**Independent Test**: Ingest sessions covering the same field and adjacent
mosaic panels. Accept and reject relation proposals, then ingest a bridging
panel. Confirm stable group identities, remembered rejection, and an explicit
merge proposal rather than a silent component merge.

**Acceptance Scenarios**:

1. **Given** a new light session, **When** it is materialized, **Then** it has a
   stable singleton panel group and the Sessions view displays that grouping.
2. **Given** sessions that differ in an immutable discriminator but satisfy the
   configured same-panel geometry and target relation, **When** matching runs,
   **Then** they are suggested as sibling sessions under one panel group.
3. **Given** adjacent panel groups whose captured footprints satisfy the mosaic
   range, **When** matching runs, **Then** the application proposes a mosaic and
   shows overlap, orientation, target, and equipment evidence.
4. **Given** a new panel would connect two accepted mosaics, **When** the panel
   is ingested, **Then** the application proposes a reviewed merge and does not
   combine the mosaics automatically.
5. **Given** a relation proposal was rejected, **When** equivalent evidence is
   encountered again, **Then** the automatic proposal remains suppressed until
   material evidence or configuration changes.

---

### User Story 3 - Add related data to an existing project (Priority: P2)

An astrophotographer receives more data for a project already being prepared or
processed. The project reports the related session but does not adopt it. If the
user adds it, the current source view becomes visibly stale. Updating the view
first shows an additive change plan and leaves all existing materialized content
unchanged.

**Why this priority**: Multi-night acquisition is routine. Users need to extend
active work without letting background ingestion rewrite a project or its
processing inputs.

**Independent Test**: Add a related session to projects in every lifecycle
state. For an allowed state, inspect and approve the Update View plan. Confirm
that only the new session is added and that completed or archived projects
refuse the addition.

**Acceptance Scenarios**:

1. **Given** a related session is created, **When** an existing project pins one
   of its siblings, **Then** the project shows a non-mutating "related session
   available" indication and does not add it automatically.
2. **Given** a project is setup-incomplete, ready, prepared, processing, or
   blocked, **When** the user explicitly adds a session, **Then** the project
   pins that exact session and any materialized view is marked stale.
3. **Given** a project is completed or archived, **When** the user attempts to
   add a session, **Then** the action is refused without changing membership.
4. **Given** a stale project view, **When** the user selects Update View,
   **Then** the application shows a reviewable plan containing only additions
   for newly pinned sessions.
5. **Given** existing materialized paths conflict with an addition, **When** the
   update plan is generated or applied, **Then** it stops for review and never
   rewrites, removes, or relocates existing content.

---

### User Story 4 - Find compatible calibration sessions (Priority: P2)

An astrophotographer imports darks, bias frames, and flats collected on
different nights. Each acquisition remains an immutable session. The
application organizes darks and bias frames under registered cameras and
calibration recipes, and organizes flats under optical profiles and filters.
It distinguishes recipe identity from age and temperature evidence.

**Why this priority**: Calibration reuse is valuable only when provenance and
compatibility remain visible. Incorrect reuse can damage every light frame in a
project.

**Independent Test**: Ingest supported calibration frames across observing
nights with complete, absent, and contradictory metadata. Verify session
boundaries, equipment assignment, freshness warnings, and explicit review for
red or unknown cases.

**Acceptance Scenarios**:

1. **Given** dark or bias acquisitions from different observing nights,
   **When** they otherwise share compatible recipe metadata, **Then** they
   remain separate sessions but appear under the applicable camera and recipe.
2. **Given** flat acquisitions for one optical profile, filter, and physical
   orientation, **When** exposure times vary within the ingestion, **Then**
   exposure variation does not split their flat relationship.
3. **Given** a calibration candidate crosses a configured age or physical
   rotation threshold, **When** it is offered for reuse, **Then** its fresh,
   stale, or red-review state is visible before selection.
4. **Given** a dark lacks a cooling set point, **When** it is ingested, **Then**
   it is preserved with unknown temperature mode and cannot be automatically
   assigned, matched, built, or reused.
5. **Given** a detected dark-flat frame, **When** scan classification completes,
   **Then** it is not misclassified as another calibration type and creates no
   Inbox item, session, match, plan, or user-facing dark-flat surface. No
   reclassification or exclusion control is offered because the file never
   becomes an Inbox candidate.

---

### User Story 5 - Correct metadata without rewriting history (Priority: P3)

An astrophotographer corrects metadata that changes a session's identity. The
application previews replacement sessions and their relationship consequences.
Approval preserves the old session and historical group revisions, creates one
or more replacements, and offers explicit project updates.

**Why this priority**: Metadata errors are inevitable, but correction must not
invalidate audits, completed processing, or already reviewed project inputs.

**Independent Test**: Correct identity metadata for a session used by a group,
mosaic, and project. Approve the replacement plan and confirm that history
remains queryable, current groups exclude the predecessor, and the project keeps
its old pin until the user accepts a replacement plan.

**Acceptance Scenarios**:

1. **Given** an identity-changing metadata correction, **When** the user opens
   the correction action, **Then** the application previews predecessor,
   replacement sessions, proposed panel revisions, stale mosaic edges, and each
   affected project's unchanged pin plus all available replacements.
2. **Given** the correction is accepted, **When** replacements are created,
   **Then** the predecessor remains immutable and queryable but is marked as
   superseded.
3. **Given** a current group contained the predecessor, **When** the correction
   is accepted, **Then** its successor revision contains reviewed replacements
   and never contains both predecessor and replacement.
4. **Given** a project pins the predecessor, **When** the correction is accepted,
   **Then** the project remains unchanged and shows an explicit replacement
   proposal identifying the predecessor and complete non-empty replacement set.
   Only approval of that separate proposal may atomically replace the project
   pin or change the materialized view.

### Edge Cases

- Missing field-of-view or reliable sky-orientation geometry prevents automatic
  same-session, same-panel, and mosaic classification. Manual relation is still
  available with the missing evidence disclosed.
- A meridian flip or a 180-degree-equivalent image axis does not create a false
  rotation mismatch. Parity remains separately checked.
- A mechanical rotator reading disagrees with solved sky orientation. Solved
  sky orientation governs spatial matching; the mechanical value remains
  visible evidence.
- A mosaic's intended target lies between panels or is absent from an outer
  panel. Per-panel object evidence remains available and the intended target is
  not inferred solely from one panel.
- A catalogue object lies entirely in an uncaptured mosaic gap. It is excluded
  from captured-object results. An extended object crossing captured panels and
  a gap remains visible as partially covered.
- A component bridge is added while another user is accepting a group revision.
  One acceptance succeeds; the stale proposal conflicts and must be refreshed.
- A calibration frame omits gain, offset, binning, readout mode, filter, or
  cooling set point. Each field follows its explicit absent/unknown rule; no
  value is inferred from a neighboring frame.
- A remote acquisition site differs from the computer's timezone. Observing
  night uses the confirmed acquisition-site timezone rather than the current
  machine timezone.
- Exposure timestamps disagree across metadata fields near the noon boundary.
  Any disagreement that changes immutable observing-night identity blocks
  automatic materialization for review.
- Configuration is tightened after groups were accepted. Existing sessions and
  accepted revisions do not change; only future suggestions use the new values.

## Requirements *(mandatory)*

### Functional Requirements

#### Immutable session identity

- **FR-001**: One explicitly approved Inbox materialization MUST be the durable,
  idempotent ingestion-operation boundary.
- **FR-002**: A materialized session's exact file membership MUST be immutable.
- **FR-003**: Repeating an ingestion operation MUST return its existing outcome
  without duplicating sessions or files.
- **FR-004**: A later ingestion MUST create a new session even when all available
  metadata matches an accepted session.
- **FR-005**: Session identity MUST be derived from frame metadata and ingestion
  provenance, not folder location.
- **FR-006**: Light-session identity MUST discriminate observing night,
  canonical confirmed target, filter, exposure, gain, offset state/value, horizontal
  and vertical binning, readout state/value, raster dimensions, recorded crop
  evidence, and compatible acquisition geometry.
- **FR-007**: Missing offset or binning MUST remain an explicit absent value.
  Absent values MAY match only another absent value and MUST carry a warning.
- **FR-008**: Gain MUST match exactly. Horizontal and vertical binning MUST be
  compared separately and MUST NOT be inferred.
- **FR-009**: Raster dimensions MUST match exactly. Crop or subframe status MUST
  be recorded only when supplied; the application MUST NOT infer full-frame,
  cropped, or subframed status from dimensions alone.
- **FR-010**: Readout mode, when present, MUST match exactly. Missing readout
  mode MUST remain absent and MUST be disclosed rather than blocking cameras
  that do not report it.

#### Observing night

- **FR-011**: Every supported light, dark, bias, and flat session MUST have an
  immutable observing-night identity.
- **FR-012**: Observing night MUST use a local civil noon-to-noon boundary at the
  acquisition site; it MUST NOT use sidereal time or apparent solar noon.
- **FR-013**: An acquisition site MUST retain a confirmable timezone and MUST
  NOT silently use the current machine timezone for a remote site.
- **FR-014**: The canonical exposure instant MUST be converted using the
  acquisition site's timezone rules effective on the exposure date.
- **FR-015**: A local timestamp MAY corroborate the canonical instant and MAY be
  used when site timezone is unavailable. A disagreement that changes the
  observing-night bucket MUST block automatic materialization for review.

#### Light-session geometry and relations

- **FR-016**: A light session MUST receive a stable singleton panel-group
  identity and an initial immutable accepted membership revision when it is
  materialized.
- **FR-017**: Same-session membership and same-panel sibling relation MUST be
  mutually exclusive. A pair MUST NOT hold both classifications.
- **FR-018**: Automatic same-session classification MUST be limited to one
  active ingestion/materialization and require every immutable discriminator to
  match.
- **FR-019**: Automatic same-session geometry MUST require all of: footprint
  coverage at least 95%, centre separation at most 2% of the smaller footprint
  diagonal, and residual solved sky-orientation difference at most 1 degree.
- **FR-085**: Footprint coverage for same-session, same-panel, and mosaic
  comparisons MUST equal captured intersection area divided by the smaller
  captured footprint area. A centre-distance rule is an additional same-panel
  guard and MUST NOT be treated as an overlap band.
- **FR-020**: Same-session comparisons MUST use an immutable representative and
  complete-linkage behavior; transitive chaining MUST NOT expand a session.
- **FR-021**: Same-panel sibling classification MUST allow deliberate session
  differences, including night, filter, exposure, gain, and offset, while
  requiring compatible target, acquisition geometry, and equipment evidence.
- **FR-086**: A durable cross-ingestion sibling or mosaic relation MUST require
  the same canonical confirmed target. Intentional cross-target grouping MUST
  require an explicit reviewed user/project association and MUST NOT create a
  separate Series identity. Geometry alone MAY produce a candidate only among
  sessions in the active ingestion and MUST NOT correlate unrelated historical
  fields.
- **FR-022**: Automatic same-panel sibling geometry MUST require all of:
  footprint coverage at least 90%, centre separation at most 5% of the smaller
  footprint diagonal, and residual solved sky-orientation difference at most 5
  degrees.
- **FR-023**: Same-panel sibling matching MUST use immutable representatives
  without transitive chaining.
- **FR-024**: Spatial matching MUST use captured sky footprints and solved sky
  orientation. Mechanical rotation MUST NOT be used as an automatic fallback.
- **FR-025**: Rotation comparison MUST treat 180-degree-equivalent image axes as
  equivalent after meridian-flip normalization while checking image parity
  separately.
- **FR-026**: Missing reliable footprint or orientation evidence MUST prevent
  automatic same-session, sibling, and mosaic classification. The application
  MUST allow an explicit manual relation with the missing evidence disclosed.

#### Guarded geometry configuration

- **FR-027**: Same-session controls MUST enforce these hard ranges and defaults:
  coverage 90-99.5% (default 95%, yellow below 93%); centre separation 0.5-5%
  (default 2%, yellow above 3%); rotation 0.25-3 degrees (default 1, yellow above
  2).
- **FR-028**: Sibling controls MUST enforce these hard ranges and defaults:
  coverage 80-95% (default 90%, yellow below 85%); centre separation 2-15%
  (default 5%, yellow above 10%); rotation 1-15 degrees (default 5, yellow above
  10).
- **FR-029**: Sibling configuration MUST NOT be stricter than same-session
  configuration: sibling minimum coverage is no greater, and sibling centre and
  rotation maxima are no smaller.
- **FR-030**: Risky but valid settings MUST show a yellow warning pill. Values
  outside hard bounds or violating cross-setting constraints MUST be red and
  unsaveable.
- **FR-031**: Configuration changes MUST affect future suggestions only and
  MUST NOT mutate accepted sessions, memberships, or revisions.

#### Mosaic groups and object evidence

- **FR-032**: Mosaic adjacency MUST relate panel groups rather than raw
  sessions.
- **FR-033**: Automatic mosaic adjacency MUST require actual footprint overlap
  within an inclusive configurable range, matching parity, compatible
  acquisition geometry, and a geometry-derived residual sky-orientation
  allowance capped at 10 degrees.
- **FR-087**: For each compared footprint pair, the application MUST determine
  the set of residual sky orientations for which normalized footprint coverage
  remains inside the configured inclusive mosaic-overlap band. An automatic
  edge MUST require the observed transported residual to belong to that set and
  its absolute value to be at most 10 degrees.
- **FR-034**: Mosaic overlap controls MUST enforce: minimum 1-20% (default 5%,
  yellow below 3%); maximum 20-60% (default 40%, yellow above 50%); minimum less
  than maximum; and maximum at least 10 percentage points below sibling minimum
  coverage.
- **FR-035**: Mosaic connectivity MAY be transitive across accepted adjacency
  edges. A bridge joining accepted components MUST create a reviewable merge
  proposal and MUST NOT merge them automatically.
- **FR-036**: Every accepted mosaic revision MUST identify its exact panel
  revisions and the accepted adjacency evidence used to derive it.
- **FR-037**: Full-mosaic object results MUST be filtered against the true union
  of captured panel footprints, preserving disconnected captured regions and
  uncaptured gaps.
- **FR-038**: Point-like or unknown-extent objects MUST be included only when
  their coordinates lie in the captured union.
- **FR-039**: Extended objects MUST use their available extent: zero captured
  intersection excludes them, partial intersection shows a partially covered
  state and estimated fraction, and full intersection shows the normal state.
- **FR-040**: Object results MUST retain per-session and per-panel containment
  evidence and deduplicate canonical object identities across panels.
- **FR-041**: Mosaic object filtering MUST NOT crop, mask, or modify image
  files. Intended-target identity MUST remain separately confirmed rather than
  being chosen solely from the mosaic centre.

#### Stable revisions and corrections

- **FR-042**: Accepted panel and mosaic membership MUST be represented by
  immutable revisions with one accepted head per stable logical group.
- **FR-043**: Adding a session to the same conceptual panel and replacing a
  corrected session within the same conceptual panel MUST create a reviewed
  successor revision while preserving the logical group identity.
- **FR-044**: A genuine panel or mosaic split, merge, or identity change MUST
  create new logical group identities linked to retired predecessors. Retired
  groups and historical revisions MUST remain queryable.
- **FR-045**: Group lineage MUST be acyclic.
- **FR-046**: A current group revision MUST NOT contain both a superseded
  session and its replacement.
- **FR-047**: One current, non-superseded light session MUST belong to at most
  one active panel group for the same canonical target or explicit reviewed
  cross-target association.
- **FR-048**: Group acceptance MUST be atomic. If any referenced session,
  revision, or accepted head became stale, the complete acceptance MUST fail
  without partial membership or lineage changes.
- **FR-049**: Rejected automatic proposals MUST be remembered by their basis,
  evidence, configuration, and reason. Materially changed evidence MAY be
  proposed again, and an explicit user action MUST remain available.
- **FR-050**: Identity-changing metadata correction MUST be a previewable
  reclassification plan. It MUST create immutable replacement sessions and
  mark predecessors as superseded rather than editing identity in place. The
  preview MUST name every predecessor and replacement, proposed source and
  destination panel revision, incident mosaic edge made stale, and project pin
  that remains unchanged.

#### Project membership and materialized views

- **FR-051**: Projects MUST pin exact session identities. Project membership
  MUST NOT follow a panel, mosaic, family, or related-session group
  automatically.
- **FR-052**: A related session MUST be shown as available without changing a
  project's membership.
- **FR-053**: Explicit session addition MUST be allowed while a project is
  setup-incomplete, ready, prepared, processing, or blocked, and prohibited
  while completed or archived.
- **FR-054**: Adding a session to a project with materialized content MUST mark
  its view visibly stale and identify the unmaterialized session.
- **FR-055**: Update View MUST produce a normal previewable filesystem plan and
  require explicit approval before applying additions.
- **FR-056**: An Update View plan MUST contain only content from sessions added
  since the materialized snapshot. Existing materialized content and paths MUST
  remain immutable.
- **FR-057**: A path collision or incompatibility MUST stop for review and MUST
  NOT rewrite existing content.
- **FR-058**: Metadata correction MUST leave project pins unchanged and show an
  explicit predecessor-to-replacement-set proposal. Approval MUST atomically
  remove the predecessor pin and add the complete non-empty replacement set
  authorized by the applied reclassification revision. It MUST use the same
  reviewable plan discipline as any other project membership change and MUST
  generate an additive/replacement view delta without mutating existing
  materialized content.
- **FR-059**: The add-versus-remove lifecycle asymmetry is intentional for this
  feature. Related-session suggestions and Update View are additive. Ordinary
  source removal remains prohibited for prepared, processing, completed, and
  archived projects and MUST never be triggered by grouping or correction.

#### Calibration organization and matching

- **FR-060**: Supported calibration types for this feature MUST be dark, bias,
  and flat. Dark-flat handling MUST remain unavailable to users and MUST NOT
  leak detected dark-flat frames into another supported type. Detection MUST
  terminate in a defensive no-op before Inbox candidates or plan events are
  produced. Manual reclassification MUST NOT offer dark-flat, and any dormant
  plan path receiving that type MUST reject or no-op rather than map it to dark
  or flat.
- **FR-061**: Every supported calibration ingestion MUST create immutable
  sessions separated by observing night.
- **FR-062**: Dark and bias sessions MUST be organized beneath a stable
  registered camera and a calibration recipe. Flat sessions MUST be organized
  beneath a stable optical profile and filter rather than a camera-only family.
- **FR-063**: Equipment registration MUST assign stable identities to cameras
  and telescopes/optical profiles. Capture-software metadata profiles MUST be
  extensible and distinguish their representative camera, telescope, filter,
  focal-length, and rotator fields without assuming that all software uses the
  same header names.
- **FR-064**: Camera names reported by standard instrument metadata and known
  device-specific camera metadata MAY identify the same registered camera. Two
  physically identical cameras that cannot be distinguished by supplied
  metadata are an accepted limitation and MAY share one registered identity.
- **FR-090**: Optical-profile matching MUST compare reported and, when reliable,
  independently calculated effective focal length against the immutable profile
  representative without transitive chaining. A difference at most 5% MUST
  resolve to the same profile, above 5% through 10% MUST require review, and
  above 10% MUST resolve as a different profile. Reported-versus-calculated
  disagreement above 10% MUST require review.
- **FR-065**: Dark recipe identity MUST use exact cooling set point, normalized
  exposure, exact gain, offset state/value, horizontal and vertical binning,
  optional readout state/value, and exact raster dimensions. Exposure MUST be
  compared to the immutable family representative without transitive chaining;
  the equivalence tolerance is `max(1 ms, min(100 ms, 0.05% of representative
  exposure))`. Actual sensor temperature MUST NOT fragment the recipe.
- **FR-066**: Regulated dark sessions MUST retain the minimum, median, maximum,
  and 95th percentile of valid per-frame absolute deviation from cooling set
  point, plus missing and invalid counts. Missing values MUST be excluded rather
  than coerced. Fewer than 80% valid readings MUST prevent an automatic
  thermally-stable result.
- **FR-088**: Dark thermal severity MUST use the 95th percentile: at most 0.5
  degrees normal; above 0.5 through 2 degrees yellow; above 2 degrees red and
  requiring explicit audited approval for automatic build or reuse. The
  moderate threshold MUST allow 0.1-2 degrees (default 0.5, warning above 1).
  The severe threshold MUST allow 0.5-5 degrees (default 2, warning above 3) and
  MUST exceed moderate by at least 0.5 degrees.
- **FR-067**: A dark without cooling set point MUST use unknown temperature
  mode and remain blocked from automatic recipe assignment, matching, building,
  and reuse pending equipment review. Explicit review MAY mark the stable camera
  as unregulated. Future missing-set-point sessions for that camera MAY then be
  assigned to its explicit unregulated recipe mode; the decision MUST NOT
  retroactively reclassify accepted sessions.
- **FR-068**: Marking a camera unregulated MUST affect future suggestions only.
  Actual temperature remains visible evidence and does not fragment the
  unregulated recipe. Automatic master matching, build selection, and reuse for
  unregulated darks MUST remain disabled until a separate reviewed policy
  exists. Manual selection MAY occur only with visible actual-temperature
  evidence, warnings, and audit.
- **FR-069**: Bias recipe identity MUST ignore cooling set point and actual
  temperature. It MUST use registered camera, exact gain, exact/absent offset,
  horizontal and vertical binning, optional exact/absent readout mode, and exact
  raster dimensions. Exposure duration MUST NOT be a bias recipe discriminator.
- **FR-070**: Dark and bias reuse aging MUST default to fresh through 270 days,
  yellow from 271 through 365 days, and red/manual beyond 365 days. Per-camera
  and per-kind configuration MUST allow both fresh and red boundaries to change,
  keep red at least 30 days after fresh, allow red up to 1,825 days, and warn
  above 730 days. Yellow candidates remain automatically eligible when every
  recipe and kind-specific check passes; red candidates require explicit
  audited manual selection.
- **FR-071**: Automatic dark or bias selection for the external processing
  handoff MUST choose one newest, compatible, sufficient session. Adding other
  sessions to that handoff MUST be an explicit reviewed selection. This feature
  MUST NOT construct or revise a native calibration master. Handoff creation and
  reviewed addition MUST each refuse aggregate selected-session source bytes
  above 17,592,186,044,416 and MUST expose cancellable verification progress.
- **FR-072**: Flat-session identity and cross-night compatibility MUST use
  optical profile, normalized filter label, exact captured gain, exact/absent
  offset, horizontal and vertical binning, exact raster, optional exact/absent
  readout mode, applicable camera geometry, and physical orientation. Missing or
  ambiguous gain MUST block automatic assignment. Flat exposure duration MUST
  NOT fragment the session or relation.
- **FR-091**: Flat filter identity MUST be a normalized captured label scoped to
  the optical profile rather than a global filter identity. Any captured value,
  including common labels such as `L` or `Ha`, remains known. Missing filter
  metadata MUST use an explicit no-filter/absent state and MUST NOT be replaced
  by an inferred filter.
- **FR-073**: Flat physical-orientation evidence MUST default to: at most 2
  degrees normal, above 2 through 5 degrees yellow, and above 5 degrees red with
  explicit approval. The normal boundary MUST be configurable from 0.5 through
  5 degrees and the red boundary up to 15 degrees, with warnings above 3 and 8
  degrees respectively.
- **FR-089**: Flat orientation MUST use only a capture field confirmed as a
  physical rotator angle, compared by minimum circular delta modulo 360 with no
  180-degree equivalence. Missing or unverified physical angle MUST remain
  absent, MUST NOT be inferred from solved sky orientation, and MUST show a
  yellow compatibility-unverified warning.
- **FR-074**: Same-night flat candidates MUST be offered automatically. Other
  nights MUST require selection and show: 0-1 nights fresh, 2-7 nights yellow,
  and more than 7 nights red. The red boundary MUST be configurable from 7
  through 365 nights and warn above 90 nights.
- **FR-075**: This feature MUST NOT score, exclude, or hide individual frames
  for scientific quality. All selected session frames remain available to the
  external processing workflow.

#### Scale and responsiveness

- **FR-076**: The supported correctness fixture MUST include 100,000 sessions,
  approximately 10 million session-frame memberships, 500,000 immutable panel
  or mosaic revisions, and 2 million membership, edge, or lineage relations.
- **FR-077**: Exceeding the tested fixture MUST NOT reject user data. The
  application MAY show a performance diagnostic.
- **FR-078**: Common session/group lists, filters, and details MUST complete at
  warm-cache p95 within 250 milliseconds on the documented reference machine.
- **FR-079**: New-session sibling and mosaic candidate discovery MUST complete
  at warm-cache p95 within 500 milliseconds, excluding metadata parsing, plate
  solving, catalogue/network access, and filesystem work.
- **FR-080**: Normal relation preview and validation through 1,000 nodes and
  5,000 relations MUST complete at p95 within 1 second.
- **FR-081**: A target-scoped stress component through 10,000 nodes and 50,000
  relations MUST complete within 5 seconds and expose progress and cancellation.
- **FR-082**: Final acceptance of a normal relation proposal MUST complete at
  p95 within 250 milliseconds and the stress fixture within 1 second, excluding
  separately reported writer-lock wait.
- **FR-083**: This feature MUST add no more than 250 milliseconds to startup and
  MUST NOT require loading the full relationship topology at startup.
- **FR-084**: Performance evidence MUST document the reference machine and
  report warm-cache p95 and cold-cache measurements separately.
- **FR-092**: A group-derived project action or destination proposal MUST expand
  exactly once to an immutable session-identity snapshot. Later group membership,
  label, or relation changes MUST NOT silently move paths or alter the proposal.
- **FR-093**: One persisted Update View plan MUST contain no more than 500 whole
  sessions, 100,000 filesystem items, 100,000 source frames, and
  17,592,186,044,416 source bytes. Generation
  MUST take a deterministic whole-session prefix, MUST expose continuation when
  unmaterialized sessions remain, and MUST return a typed resource refusal
  without persisting a partial plan when one session alone exceeds an item or
  source-frame or source-byte ceiling. Apply MUST complete preflight for every
  persisted item and source frame before its first filesystem write.

#### Review-surface accessibility

- **FR-094**: Every new Inbox, session/group relation, matching-settings,
  calibration-selection, metadata-correction, and Update View review surface
  MUST convey severity, eligibility, and required action with visible text and
  an accessibility-named icon in addition to color. Every surface MUST honor
  reduced-motion preferences and MUST NOT require animation, flashing, or
  continuous movement to understand state.
- **FR-095**: Every new review control and evidence disclosure MUST be keyboard
  operable. Modal focus MUST enter at the review heading, remain within the
  modal, and return to the invoking control. Failed submission MUST focus an
  error summary whose entries link to the affected controls or items.
- **FR-096**: Long-running discovery, preview, and apply operations MUST expose
  an accessibility-named cancel control whenever cancellation is safe. Preview
  readiness, stale changes, coalesced progress, cancellation state, resumable
  state, and completion MUST be announced to screen readers; blocking failures
  MUST use an assertive announcement.
- **FR-097**: An Update View correction plan MUST persist the complete ordered
  correction-overlay mapping preview and mapping count. Paginated mapping
  queries MUST expose that immutable collection, and the plan digest MUST bind
  its ordinals, entry identities, and exclusion codes.
- **FR-098**: Update View plan generation, approval, and apply MUST require
  trusted authorization for the project and each applicable source and
  destination root. Apply MUST require project mutation, source-read, and
  destination-write authorization.
- **FR-099**: A recoverable Update View interruption or cancellation MUST enter
  a distinct stopped state. Resume MUST atomically claim a new fencing
  generation while preserving the approved plan revision and digest. Failed
  MUST be reserved for a proven non-resumable outcome.
- **FR-100**: Update View MUST durably record an install intent and stable
  ownership proof before each atomic no-clobber install. Recovery after a crash
  before journal completion MUST reconcile the destination using that ownership
  proof, collision key, and strong fingerprint. Byte equality alone MUST NOT
  prove ownership, and ambiguous evidence MUST stop as a collision.

### Key Entities

- **Ingestion operation**: one approved Inbox materialization and its durable
  retry identity.
- **Session**: an immutable set of frame identities created by one ingestion,
  with frame type, observing night, acquisition metadata, and provenance.
- **Session supersession**: an append-only predecessor-to-replacement relation
  created by an approved metadata correction.
- **Panel group**: a stable same-pointing identity for one or more immutable
  light sessions. Every light session starts in a singleton panel group.
- **Panel-group revision**: an immutable accepted membership snapshot with a
  parent revision, exact sessions, representative evidence, configuration
  version, actor, reason, and decision provenance.
- **Mosaic**: a stable identity for a reviewed set of adjacent panel groups.
- **Mosaic revision**: an immutable snapshot of exact panel revisions and
  adjacency evidence.
- **Group lineage**: the acyclic derivation from retired groups to new groups
  after a conceptual split, merge, or identity change.
- **Relation proposal**: a reviewable candidate based on an immutable source
  revision, measured evidence, and configuration.
- **Project session pin**: explicit project membership in one exact session,
  independent of later group changes.
- **Camera registration**: a stable camera identity linked to observed metadata
  aliases and calibration recipes.
- **Optical profile**: a stable telescope/image-train identity used to organize
  lights and flats, with extensible capture-software metadata mapping.
- **Calibration recipe**: a compatible dark or bias acquisition identity under
  a registered camera.
- **Calibration session**: an immutable observing-night acquisition for dark,
  bias, or flat frames.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Re-ingesting identical files through the same approved operation
  creates zero duplicate sessions and changes zero accepted session memberships.
- **SC-002**: Ingesting additional matching files through a later operation
  creates a distinct session and changes zero already accepted memberships.
- **SC-003**: Every materialized light session is visible through exactly one
  current panel-group membership for its canonical target or explicit reviewed
  cross-target association.
- **SC-004**: Accepting, rejecting, correcting, splitting, or merging a group
  leaves every historical revision and project session pin queryable.
- **SC-005**: A project never gains a session because a related group changed;
  all membership additions are attributable to an explicit user approval.
- **SC-006**: Updating a stale project view changes only content for newly
  approved session pins and modifies zero pre-existing materialized entries.
- **SC-007**: All automatic same-session, sibling, and mosaic suggestions expose
  the measured evidence and active thresholds that caused the suggestion.
- **SC-008**: A test mosaic containing gaps excludes 100% of point objects wholly
  in the gaps while retaining extended objects that intersect captured panels.
- **SC-009**: Supported calibration sessions remain distinct across observing
  nights, and every automatic reuse recommendation displays recipe compatibility,
  age, and applicable temperature or orientation evidence.
- **SC-010**: Dark-flat test input produces zero user-facing Inbox items,
  sessions, matches, or plans and is never presented as dark, bias, or flat.
- **SC-011**: Concurrent acceptance of two proposals from the same base produces
  one accepted successor and one visible stale conflict, with zero partial
  memberships, edges, lineage, or project changes.
- **SC-012**: The scale fixture and every responsiveness target in FR-076 through
  FR-084 pass on the documented reference machine.
- **SC-013**: Unit tests cover identity derivation and missing-metadata behavior;
  real-database integration tests cover ingestion through calibration matching;
  and a real-backend user journey covers the reviewable session/group workflow.
- **SC-014**: In a moderated evaluation with at least 10 qualified
  astrophotographers, at least 90% of participants MUST complete all five review
  tasks without moderator intervention. Across all participants and tasks, at
  least 90% of the scored answers for each pre-action field, affected sessions,
  highest evidence severity, and approval result, MUST be correct. A qualified
  participant has independently acquired and processed multi-night data with at
  least one supported external processing workflow during the preceding year.

## Assumptions

- The product remains greenfield. Development databases may be reset; preserving
  the mutable session/group model is not a requirement.
- Existing raw files remain user-owned and in place. This feature stores
  metadata, relationships, and reviewed decisions; it does not copy raw files
  into an application-private library.
- Plate solving or equivalent reliable sky geometry is available for automatic
  spatial classification. Missing geometry degrades to explicit manual review.
- PixInsight/WBPP and Siril remain responsible for calibration, registration,
  stacking, and scientific frame rejection. Native master construction is a
  separately gated roadmap feature.
- Image locations are provenance, not identity. Moving or remapping a library
  root does not create a scientific session relation.
- Session removal unrelated to a correction or supersession remains governed by
  existing project lifecycle behavior and is outside this feature.
- Detected dark-flat metadata support is retained internally for future work,
  but no dark-flat product surface is part of this feature.

## Dependencies and boundaries

- Reliable sky-orientation transport and footprint measurement are prerequisites
  for automatic geometric matching. Their independent delivery does not permit
  product implementation before this feature's artifact gate passes.
- Native calibration-master construction (`astro-plan-1zp7`, GitHub #1425) and
  graph acceleration (`astro-plan-7633`, GitHub #1426) are roadmap features, not
  part of this specification.
- This specification supersedes the split-plan activity and product model from
  the earlier draft. Mixed input is reviewed as multiple immutable session items
  under stable metadata groups; no separate split-plan workflow is implemented.
- This specification supersedes the dark-flat product requirements in Specs
  001, 006, 027, and 041. Those historical artifacts remain unchanged as
  decision history. Dormant parsing, detection, domain representation,
  generated/reserved contracts, persistence guards, and negative tests remain;
  only reachable product flows are prohibited.
- Product implementation MUST NOT begin until `research.md`, `plan.md`,
  `data-model.md`, `contracts/`, and the Beads-native child decomposition under
  `astro-plan-ic9h` exist and the required SpecKit review gates pass. This
  repository prohibits a duplicate `tasks.md` tracker.
