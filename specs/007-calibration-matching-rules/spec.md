# Feature Specification: Calibration Matching Rules

> **See Spec 032**: UI implementation of this feature must follow
> [Spec 032 — Design V4](../032-design-v4-implementation/spec.md), the current UI
> truth, for layout, navigation, and component patterns. Spec 030 (UI Audit &
> Revision), cited here previously, is superseded as of 2026-06-11.

**Feature Branch**: `007-calibration-matching-rules`
**Created**: 2026-05-09
**Last Updated**: 2026-06-23
**Status**: **Implemented** (closed 2026-07-03) — matching engine for bias/dark/flat + assign/ranking/candidate shipped (`crates/calibration/core/src/`), Tauri adapters + app use cases (`crates/app/calibration/`) and DTO mirrors in `contracts_core` in place. The 11 open tasks are all **DEFERRED**, not unstarted: 8 contract-test tasks blocked on the absent JSON-Schema test runner (domain guards already covered by unit tests), T040 blocked on spec-002's canonical `calibration_types` enum artifact, and T032/T033 doc/perf polish. (Earlier verify pass on 2026-06-23 accepted SC-001: matcher per-type keys are backend-configurable with defaults; the per-criterion surface is the spec-018 Calibration pane (FR-009 split); gates were green with `calibration_core` 74 tests.)
**Input**: User description: "Specify configurable calibration matching rules per calibration type, with recommendations and manual override."

## Implementation Status: Implemented (2026-07-03)

Superseding the original "UI scaffolding only" note: the matching engine is
built and wired end to end. Per-type rule evaluators live in
`crates/calibration/core/src/rules/{dark,flat,bias}.rs`; ranking/candidate
scoring in `candidate.rs`/`ranking.rs` (incl. `require_same_offset`,
same-session/night precedence, confidence clamping); assign/override in
`assign.rs`. App use cases (`suggest`/`batch_suggest`/`assign`/`load_config`)
are in `crates/app/calibration/src/matching.rs`, exposed via Tauri commands
`calibration_match_suggest`/`_assign`/`_suggest_batch`
(`apps/desktop/src-tauri/src/commands/calibration.rs`) with `contracts_core`
DTO mirrors. FR-001…FR-012 and SC-001…SC-005 are satisfied by shipped code and
unit tests. Remaining gaps are JSON-Schema **contract tests** (deferred on the
absent test runner) and doc/perf polish — no product behavior is missing.

Note: the Settings-page calibration-matching UI (toggleable criteria, offset
toggle) is owned by the **spec 043 redesign**; its offset toggle persists
locally pending a settings-key (`STUB-OFFSET-REQUIRED`) — a 043 concern, not a
007 acceptance item (the backend `require_same_offset` hard rule already exists).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Dark Frame Matching (Priority: P1)

As an astrophotographer with light frames captured at specific gain, offset,
exposure, and sensor temperature, I want the app to recommend dark masters that
match these acquisition parameters so that calibration is correct without me
hand-walking every dark library.

**Why this priority**: Darks are the most parameter-sensitive calibration type
and the most common reason a project blocks during preparation. Without dark
matching the rest of the feature is not useful.

**Independent Test**: Load a light session with known gain, offset, exposure,
and temperature, configure dark rules with default tolerances, and verify the
returned ranked list contains exact-parameter dark masters first and
within-tolerance dark masters second, with confidence values reflecting
dimension match counts.

**Acceptance Scenarios**:

1. **Given** a light session at gain 100, offset 50, exposure 300s, temperature
   -10C, **When** matching is requested, **Then** a dark master at identical
   gain/offset and exposure 300s within ±2C is returned with high confidence.
2. **Given** the same light session, **When** only out-of-tolerance darks
   exist, **Then** the response lists them with reduced confidence and a
   `dimensions_mismatched` array naming the failing dimensions.
3. **Given** dark match tolerance for temperature is widened in settings,
   **When** matching is re-requested, **Then** previously rejected darks now
   appear with confidence values reflecting the wider tolerance.

---

### User Story 2 - Flat Frame Matching (Priority: P2)

As an astrophotographer, I want flat masters recommended based on filter,
optical-train rotation, binning, and date proximity so that vignetting and
dust-mote correction matches the actual light frame capture state.

**Why this priority**: Flats are required for any project that integrates
multi-frame data, but their matching policy is workflow-dependent and benefits
from explicit configuration.

**Independent Test**: Load a multi-filter light session captured on a known
observing night, request flat matching, and verify each filter's recommended
flat master comes from the closest same-rotation, same-binning flat capture
night, with `dimensions_matched` showing filter, rotation, binning, and a
date-proximity score.

**Acceptance Scenarios**:

1. **Given** a light session with filter Ha and rotation 90°, **When** flat
   matching is requested, **Then** flat masters with matching filter and
   rotation from the same observing night are ranked above any other.
2. **Given** no same-night flats exist, **When** matching is requested,
   **Then** compatible flats from the nearest date are returned with confidence
   reduced for date proximity.
3. **Given** a flat master with a different binning, **When** matching is
   requested, **Then** it is excluded from the recommendation list because
   binning is a hard constraint.

---

### User Story 3 - Bias Frame Matching (Priority: P3)

As a user calibrating short-exposure or scaled-dark workflows, I want bias
masters recommended based on gain and offset alone so that bias is not blocked
by exposure or temperature requirements that do not apply.

**Why this priority**: Bias matching is the simplest case and only relevant for
some workflows, but it must not be conflated with dark matching rules.

**Independent Test**: Load a light session with bias-eligible workflow,
request bias matching, and confirm the recommendation set is filtered by gain
and offset only, with no exposure or temperature dimension entries in the
response.

**Acceptance Scenarios**:

1. **Given** bias masters at matching gain/offset, **When** matching is
   requested, **Then** they are returned with confidence based on gain/offset
   match only.
2. **Given** bias masters whose gain differs, **When** matching is requested,
   **Then** they are excluded because gain is a hard constraint for bias.

---

### User Story 4 - Manual Override (Priority: P4)

As a user preparing a project, I want to assign a specific calibration master
even when it conflicts with the auto-recommendation so that I retain final
control over calibration decisions.

**Why this priority**: Override is essential trust behavior but only meaningful
once auto-matching produces something to override.

**Independent Test**: Auto-match a session, then call assign with a non-top
master and `override=true`, and verify the assignment is recorded with
confidence and a flag indicating it was an override.

**Acceptance Scenarios**:

1. **Given** a recommendation set, **When** the user assigns a master that is
   not the top recommendation with `override=true`, **Then** the assignment is
   persisted and the response reports the assigned master's confidence.
2. **Given** the user attempts to assign a master with incompatible hard-rule
   dimensions, **When** the request is sent without override, **Then** the call
   returns `incompatible.dimensions` and the assignment is not made.
3. **Given** the user retries the same incompatible assignment with
   `override=true`, **Then** the assignment is recorded with a lowered
   confidence and a mismatched-dimensions audit note.

---

### User Story 5 - Batch Calibration Suggestions (Priority: P2)

As a user preparing a project, I want to request calibration suggestions for
multiple light sessions at once so that project-wide calibration preparation
does not require a separate call per session.

**Why this priority**: A project may have dozens of light sessions across
multiple targets, filters, and nights. Batch suggestions make the project
preparation workflow practical.

**Independent Test**: Seed 3 light sessions with differing parameters, call
`calibration.match.suggest.batch` with all three session IDs, and verify
each session returns per-type candidates with correct `status` values
(`match`, `ambiguous`, `no_match`, or `observer_location_missing`). Verify
partial success: if one session lacks observer_location, that session returns
`observer_location_missing` while others return matches.

**Acceptance Scenarios**:

1. **Given** multiple light sessions, **When** batch suggest is requested,
   **Then** each session receives independent ranked candidates per type.
2. **Given** one session has `observer_location: null`, **When** batch suggest
   is requested, **Then** that session's result returns
   `status: "observer_location_missing"` and others are unaffected.
3. **Given** a `calibration_types` filter is provided, **When** batch suggest
   is requested, **Then** only the requested types are evaluated per session.

---

### Edge Cases

- Darks without reliable temperature metadata fall back to gain+offset+exposure
  matching with reduced confidence.
- Flats captured after local midnight but belonging to the same observing
  night are grouped by observing-night semantics, not calendar date.
- Multiple light sessions on adjacent calendar dates do not share flats unless
  same-night/session grouping or compatibility fallback explicitly allows it.
- Missing camera or telescope metadata excludes the candidate from hard-rule
  matching and surfaces a "metadata gap" reason instead of a silent rejection.
- A user-provided master file (not directory) is treated as a single calibration
  master with metadata derived from headers, not from sibling files.

### Domain Questions To Resolve

- Default tolerance values per dimension (temperature, date proximity) at
  first launch.
- Whether bias matching should ever consider exposure for scaled-dark
  workflows, or treat exposure as implicit.
- Whether confidence is a single 0–1 scalar or a structured breakdown the UI
  must render.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The matching engine MUST support calibration types `dark`, `flat`,
  and `bias` in v1. `dark_flat` is reserved in the `CalibrationType` enum for
  forward-compatibility but MUST NOT be matched, suggested, or assigned in v1.
  Files with dark_flat IMAGETYP values land as `unclassified` at the inbox level
  (spec 005 ripple). The `dark_flat` slot MUST NOT appear in any Settings UI
  filter chip, assignment dropdown, or calibration type selector in v1.
- **FR-002**: Matching rules MUST be configurable independently per
  calibration type (dark, bias, flat).
- **FR-003**: Dark matching MUST default to gain (exact), offset (exact),
  exposure (±configurable tolerance), and temperature (±configurable tolerance).
- **FR-004**: Flat matching MUST default to filter (exact), gain (exact —
  hard-gate, ratified 2026-05-23; no soft tolerance, mixed-gain flats require
  manual override), rotation (±0.5° default), binning (exact), optic_train
  (exact), and date proximity scored against the light session's observing
  night.
- **FR-005**: Bias matching MUST default to gain (exact) and offset (exact),
  with exposure and temperature explicitly excluded unless configured.
- **FR-006**: System MUST return ranked recommendations with per-candidate
  confidence and explicit `dimensions_matched` / `dimensions_mismatched` lists.
- **FR-007**: Recommendations MUST remain advisory; manual override MUST be
  accepted via an explicit override flag.
- **FR-008**: Flat recommendations MUST prioritize same-session and
  same-observing-night flats before compatibility fallback.
- **FR-009**: Same-night flat matching MUST use observing-night/session
  semantics, not plain calendar-date equality.
- **FR-010**: Master calibration mode MUST allow selecting a file instead of a
  directory.
- **FR-011**: Matching explanations MUST be inspectable via the contract
  response, not only via UI tooltips.
- **FR-012**: Hard-rule mismatches (e.g., gain for darks, binning for flats)
  MUST exclude candidates from the auto list but MAY be assigned via override.

### Key Entities

- **Calibration Type**: Dark, bias, flat, or dark flat.
- **Matching Rule**: Selected metadata fields, tolerances, and hard/soft
  classification per calibration type.
- **Calibration Master**: A calibration artifact (file or directory) with
  extracted metadata used as the right-hand side of matching.
- **Calibration Match**: Candidate relationship between a light session and a
  calibration master with confidence and dimension breakdown.
- **Observing Night**: The acquisition night/session grouping used to associate
  after-midnight captures with the same practical imaging session.
- **Manual Override**: User-selected assignment that supersedes the auto-pick
  and is recorded with override provenance.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Users can configure dark, bias, and flat matching independently
  from a single settings surface.
- **SC-002**: For a session with complete metadata, the matcher produces at
  least one ranked candidate per applicable calibration type when masters
  exist.
- **SC-003**: Manual override is preserved across project edits and reported in
  the response payload.
- **SC-004**: Flat recommendation responses indicate whether each candidate
  was selected by same-session, same-night, or compatibility fallback.
- **SC-005**: Recommendation responses surface every excluded dimension so the
  UI can render a "why not" view without a second call.

## Assumptions

- Light frames are the anchor for project calibration recommendations.
- Metadata extraction (specs 003/004) can provide camera, telescope, filter,
  exposure, gain, offset, binning, and temperature where headers contain them.
- The session and observing-night concepts come from the sessions crate and
  are already populated for any candidate light session.

## Out of Scope

- Pixel-level calibration validation.
- Automatic application of calibration to images.
- Cross-library master sharing.
- Authoring new calibration masters (the matcher consumes existing masters).
