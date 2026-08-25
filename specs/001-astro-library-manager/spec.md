# Feature Specification: Astro Library Manager

> **See Spec 032**: UI implementation of this feature must follow
> [Spec 032 — Design V4](../032-design-v4-implementation/spec.md), the current UI
> truth, for layout, navigation, and component patterns. Spec 030 (UI Audit &
> Revision), cited here previously, is superseded as of 2026-06-11.
> Lifecycle states, source folder types, and data model concepts are owned by
> the dependent backend specs (002, 005-019, 023-026), not by spec 030.

**Feature Branch**: `001-astro-library-manager`  
**Created**: 2026-05-02  
**Status**: Closed (Umbrella / Planning / Prototype Baseline)
**Superseded By**: `specs/027-frontend-implementation/spec.md` (2026-05-24)
**Note**: This spec remains as historical reference for product intent and domain modeling. The authoritative UI design is now in `/DESIGN.md` (canvas wireframe export from Claude Design, 2026-05-24).
**Input**: User description: "Cross-platform local-first desktop application for managing astrophotography project structure, data ingestion, source mapping, calibration matching, PixInsight project preparation, and project lifecycle management."
**Closure**: Umbrella/specification baseline only; this feature spec is closed as a planning/prototype artifact and does not represent an authorized app-implementation workstream.

## Spec Scope Status

- Remaining implementation scope is split into follow-on specs (first in sequence: `specs/002-data-lifecycle-state-model/spec.md`).
- This file remains the high-level baseline for product intent, behavior, requirements, and future decomposition.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Index Existing Library (Priority: P1)

As an astrophotographer with an existing library under broad folders such as
`D:\Astrophotography\Raw`, `Masters`, `Process`, `Published`, `Databases`,
`Tools`, `Sharpcap Captures`, `Manual`, and `Pixinsight processes`, I want to
select one or more library roots and see an inventory of files, folders,
candidate sessions, calibration material, project-like folders, outputs, and
unknown items without changing the filesystem.

**Why this priority**: The product has no value until it can safely understand
an existing library without forcing migration.

**Independent Test**: Can be tested by pointing the app at a representative
messy library tree and verifying that it produces an inventory, confidence
labels, unknown/unclassified buckets, and no filesystem mutations.

**Acceptance Scenarios**:

1. **Given** a selected library root containing mixed raw, calibration, process,
   published, manual, tool, and database folders, **When** the user starts an
   initial scan, **Then** the app classifies discovered items into domain
   concepts with confidence levels and leaves uncertain items visible for review.
2. **Given** an existing processing project folder that does not match the
   supported project structure, **When** the app scans it, **Then** the app may
   classify it as project-like material but MUST NOT ingest it as an app-managed
   project until it has been migrated into the supported structure.
3. **Given** a library root on a removable or remapped drive, **When** the root is
   unavailable or moved, **Then** the app reports the missing root and offers a
   recovery/remapping workflow without losing stored relationships.
4. **Given** files that are reachable through symlinks or junctions, **When** the
   scan runs with default safety settings, **Then** the app records the link
   itself and does not follow it unless the user explicitly enables that behavior.

---

### User Story 2 - Ingest Acquisition and Calibration Data (Priority: P1)

As an astrophotographer adding new raw or calibration data, I want the app to
ingest new folders or files, read useful metadata when possible, identify
candidate acquisition sessions, calibration sessions, calibration masters,
targets, equipment, filters, sites, capture software, and optional observing
plan references, and preserve the source files as immutable references unless I
approve a plan.

**Why this priority**: Durable source records and calibration reuse are central
to project mapping and later cleanup safety.

**Independent Test**: Can be tested by ingesting sample deep-sky raw lights,
darks, flats, biases, and master calibration files from multiple nights and
checking that sessions, setup fingerprints, and calibration candidates are
created with confidence and review state.

**Acceptance Scenarios**:

1. **Given** new raw light frames with useful FITS/XISF metadata, **When** the
   user ingests them, **Then** the app groups them into candidate acquisition
   sessions and records source paths, target hints, metadata, equipment hints,
   filter hints, exposure details, and confidence.
2. **Given** reusable calibration data such as darks, biases, dark flats, or
   flats from a compatible setup, **When** the app evaluates calibration
   candidates, **Then** it shows reusable calibration relationships without
   forcing calibration material into a single object folder.
3. **Given** files with incomplete or conflicting metadata, **When** ingestion
   finishes, **Then** the app keeps them reviewable and does not silently assign
   high-confidence equipment, target, or calibration relationships.
4. **Given** a capture plan file or plan-like artifact associated with a session,
   **When** the app indexes or ingests the session, **Then** it can store the file
   as a referenced planning artifact and link it to the target/session without
   becoming the tool that executes or edits the plan.

---

### User Story 3 - Create and Map Processing Projects (Priority: P1)

As an astrophotographer preparing work for PixInsight/WBPP, I want to create a
processing project that references one or more acquisition sessions,
calibration sessions, calibration masters, targets, panels, filters, and
attempts, then select the intended processing tool or workflow profile and
generate a documented source map for tool-friendly input preparation.

**Why this priority**: The product is meant to connect library organization to
real processing project preparation, starting with PixInsight/WBPP and common
planetary/lunar workflows, not merely catalog files.

**Independent Test**: Can be tested by creating a multi-session target project
and a mosaic-style project, associating sources and calibration candidates, and
verifying that the project manifest explains exactly which source data is used.

**Acceptance Scenarios**:

1. **Given** multiple acquisition sessions for the same target, **When** the user
   creates a project, **Then** the app lets the user include selected sessions and
   records why each source set is included.
2. **Given** a mosaic target with multiple panels, **When** the user maps project
   sources, **Then** the app represents panel membership separately from session
   membership and calibration relationships.
3. **Given** several plausible calibration matches, **When** the user reviews
   project sources, **Then** the app displays candidate matches, confidence, and
   reasons so the user can accept, reject, or override them.
4. **Given** a new processing project, **When** the user creates it, **Then** the
   app lets the user select a processing tool or workflow profile such as
   PixInsight/WBPP or a planetary/lunar workflow profile and stores that choice
   as project configuration.
5. **Given** an existing target with known sessions and calibration candidates,
   **When** the user creates a new session or project from that target, **Then**
   the app pre-populates target metadata, aliases, known source sessions, and
   relevant prior context for review.

---

### User Story 4 - Prepare Tool Source Views (Priority: P2)

As an astrophotographer, I want the app to prepare project-local source views for
the selected processing tool using a safe, reviewed strategy so PixInsight/WBPP
or planetary/lunar tools can consume organized inputs without copying large
source data by default.

**Why this priority**: Processing tool preparation is a key workflow, but it
depends on correct indexing, ingestion, project mapping, and the selected tool
profile.

**Independent Test**: Can be tested by generating a project source view from an
approved project map and confirming that the view contains only expected source
references, links, manifests, or copies according to the selected strategy.

**Acceptance Scenarios**:

1. **Given** an approved project source map and selected workflow profile,
   **When** the user generates a source view plan, **Then** the app shows all
   proposed links, folders, manifests, copies, and naming decisions before
   anything is written.
2. **Given** a platform or selected tool profile where a preferred link type is
   unavailable or unsafe, **When** the user prepares a source view, **Then** the
   app recommends a safer fallback and explains the tradeoff.
3. **Given** a generated source view, **When** the user wants to remove it,
   **Then** the app can identify app-created links and generated files for safe
   cleanup without touching original source data.

---

### User Story 5 - Track Lifecycle, Outputs, Archive, and Cleanup (Priority: P2)

As an astrophotographer finishing projects, I want to track final outputs,
processing attempts, PixInsight-created artifacts, lifecycle state, archive
status, and cleanup candidates so I can reclaim disk space safely after final
stack or drizzle verification.

**Why this priority**: Disk recovery and archival safety are major product goals,
but cleanup is unsafe until source relationships and lifecycle state are
trustworthy.

**Independent Test**: Can be tested by marking a project as finalized, recording
final outputs and verification status, and generating a reviewable cleanup plan
that protects sources, masters, final outputs, manifests, and audit history by
default.

**Acceptance Scenarios**:

1. **Given** a project with final outputs and documented source usage, **When**
   the user marks final verification complete, **Then** the app identifies
   potential PixInsight intermediates as cleanup candidates with confidence and
   estimated reclaimable space.
2. **Given** PixInsight creates or updates directories inside a project
   processing workspace, **When** the app refreshes the project or observes the
   workspace, **Then** it registers discovered directories and files as
   processing artifacts without treating them as canonical app-managed data.
3. **Given** cleanup candidates that include registered or drizzle data, **When**
   the app explains the plan, **Then** it distinguishes regenerable intermediates
   from protected originals, masters, final outputs, manifests, and audit records.
4. **Given** an approved archive or cleanup plan, **When** the user applies it,
   **Then** every action is audited with source path, destination path when
   relevant, timestamp, result, and failure details.
5. **Given** global cleanup defaults and project-specific cleanup needs, **When**
   the user reviews cleanup settings or a generated cleanup plan, **Then** the
   app shows an inherited nested tree of project directories, subdirectories,
   resources, and PixInsight artifact categories where each node can inherit,
   enable, disable, or override cleanup behavior.
6. **Given** a project with sources, calibration decisions, source views,
   PixInsight artifacts, outputs, cleanup policies, and audit references,
   **When** the app generates project documentation, **Then** it writes a
   protected manifest or equivalent document that can be inspected outside the
   app while keeping the database canonical for relationships, lifecycle, and
   audit state.

---

### User Story 6 - Configure Rules and Recover Roots (Priority: P3)

As an astrophotographer with personal folder conventions and external drives, I
want to configure naming templates, classification rules, protected folders,
retention rules, aliases, taxonomy, and root remapping so the app supports my
library instead of forcing one convention.

**Why this priority**: Configurability is required for adoption, but defaults can
be researched and introduced after the core inventory and project workflows.

**Independent Test**: Can be tested by changing protected folder rules, adding a
target alias or equipment alias, remapping a moved root, and verifying that
classification and plans update without losing historical audit records.

**Acceptance Scenarios**:

1. **Given** an existing library with nonstandard folder names, **When** the user
   adds classification rules or aliases, **Then** future scans and existing
   review queues reflect the updated rules without moving files automatically.
2. **Given** an external drive mounted at a different path, **When** the user
   remaps the library root, **Then** relative paths, relationships, and project
   references resolve against the new root.

### User Story 7 - Track Targets and Observing History (Priority: P2)

As an astrophotographer planning and revisiting work by object, I want to see a
target-centered view of acquisition sessions, calibration context, projects,
outputs, plan references, and notes so I can understand what data I already have
before creating another session or project.

**Why this priority**: Target context is needed to create sessions and projects
cleanly, but full observing-plan authoring can remain outside v1.

**Independent Test**: Can be tested by creating or importing target metadata,
linking several sessions and projects to the target, attaching a plan file
reference, and verifying that the target view shows session dates, data
coverage, outputs, and project status.

**Acceptance Scenarios**:

1. **Given** sessions for the same target across multiple dates, **When** the user
   opens the target view, **Then** the app shows linked sessions, projects,
   filters/data types, outputs, and lifecycle status.
2. **Given** a target with aliases or inconsistent names in metadata/folders,
   **When** the user confirms target identity, **Then** the app stores aliases and
   uses them for future classification suggestions.
3. **Given** a NINA plan or other observing-plan artifact, **When** the user links
   it to a target or session, **Then** the app stores the reference and metadata
   without attempting to execute, schedule, or fully edit the plan in v1.

### Edge Cases

- Library roots contain millions of files or very large files where eager hashing
  would be too slow.
- Multiple files have identical names under different roots or sessions.
- Files have invalid, reserved, case-conflicting, or long paths on one platform
  but not another.
- FITS/XISF metadata is missing, inconsistent, vendor-specific, or contradicts
  folder names.
- Acquisition sessions contain multiple targets, filters, cameras, video files,
  still frames, or capture software exports in one folder.
- Targets have aliases, catalog identifiers, changing project goals, sessions
  across multiple nights, and optional capture-plan artifacts such as NINA plans.
- Darks, biases, dark flats, and other reusable calibration data may be captured
  once and reused across sessions when matching criteria support reuse.
- Flats can sometimes be reused across sessions for static setups, and flats from
  one imaging session can apply to multiple targets captured during that same
  compatible setup; other flats are invalid because focus, rotation, filter,
  sensor temperature, binning, dust state, or optical train changed.
- A target has several processing attempts, rejected sessions, or partial data.
- Existing processing project folders may not match the supported project
  structure and therefore cannot be imported as app-managed projects until a
  migration or restructuring step has occurred.
- Mosaic panels have different coverage, session counts, filters, or calibration
  matches.
- PixInsight-managed processing workspaces contain a mixture of final outputs,
  logs, process icons, intermediate registered frames, drizzle data, temporary
  files, generated directories, and manual notes.
- Planetary or lunar workflows may involve capture tools such as SharpCap and
  downstream stacking/sharpening/editing tools with different file types,
  intermediate folders, and cleanup candidates than deep-sky PixInsight/WBPP
  workflows.
- Global cleanup rules may be appropriate for most projects, while individual
  projects, directories, subdirectories, resources, or PixInsight artifact types
  need stricter or looser retention.
- The app crashes or is closed while a scan, plan generation, or plan application
  is in progress.

### Domain Questions To Resolve

- What conceptual model should be canonical: acquisition sessions, calibration
  sessions, calibration masters, processing projects, outputs, archives, or a
  different set of entities?
- What default folder structures and naming templates should be recommended, and
  which dimensions must remain configurable?
- What supported project structure is required before a project can be imported
  as app-managed, and what migration guidance or reviewed migration plan should
  be offered for nonconforming existing projects?
- What FITS/XISF keywords and related sidecar metadata are reliable enough for
  session identification, setup fingerprints, and calibration matching?
- What matching criteria and confidence model should be used for darks, flats,
  biases, dark flats, masters, filters, cameras, binning, temperature, gain,
  offset, exposure, date/night, target, and optical train?
- Which source view strategy should be default for each platform: symlink,
  junction, hard link, manifest-only, copy, or a hybrid?
- Which PixInsight/WBPP intermediate files are safe cleanup candidates after
  final verification, and which artifacts must be protected by default?
- Should PixInsight artifact observation happen through startup/refresh scans,
  optional folder monitoring, or both, and how should the app handle missed
  events or external changes?
- Should generated project manifests use JSON, JSONL, Markdown, or a hybrid, and
  how should manifest versioning/export avoid complicating a future remote
  service migration?
- Which processing tool and workflow profiles belong in v1, starting with
  PixInsight/WBPP and common planetary/lunar workflows, and how should later
  profiles such as Siril be added without changing the core project model?
- How should deep-sky, mosaic, planetary, lunar, solar, and landscape workflows
  differ for source mapping, calibration, artifacts, outputs, and cleanup?
- What target metadata, aliases, catalog identifiers, observing-plan references,
  and target-centered history belong in v1 versus later full planning features?
- What belongs in durable app records versus generated manifests or files stored
  beside projects?
- What language-neutral contract strategy best preserves future local or remote
  backend options?

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST allow users to register one or more library roots
  without requiring immediate filesystem migration.
- **FR-002**: System MUST scan registered roots and inventory files, folders,
  links, candidate sessions, candidate calibration material, project-like
  material, outputs, unknown items, and scan errors.
- **FR-003**: System MUST avoid following symlinks or junctions by default and
  MUST let users explicitly enable that behavior per root or operation.
- **FR-004**: System MUST store library roots separately from relative paths so
  roots can be remapped when drives or mount points change.
- **FR-005**: System MUST extract useful metadata from supported astrophotography
  files where possible and preserve raw metadata values for later review.
- **FR-006**: System MUST classify acquisition sessions, calibration sessions,
  calibration masters, targets, processing projects, outputs, and unknown
  material with a confidence level and explainable evidence.
- **FR-007**: System MUST let users review, confirm, correct, split, merge, or
  reject inferred sessions, targets, equipment, setups, panels, and
  relationships.
- **FR-008**: System MUST represent acquisition sessions as immutable source
  records that can be referenced by multiple processing projects; corrections
  MUST be recorded as reviewed metadata or relationship updates rather than
  silent mutation of the source session identity.
- **FR-009**: System MUST represent calibration frames, calibration sessions, and
  calibration masters as independent first-class library records, separate from
  targets, object folders, acquisition sessions, and processing projects.
- **FR-010**: System MUST support projects that reference multiple acquisition
  sessions, calibration sessions, calibration masters, filters, equipment setups,
  panels, and processing attempts.
- **FR-011**: System MUST support mosaic project modeling where panel membership
  is distinct from session membership and calibration membership.
- **FR-012**: System MUST maintain a target catalog with target metadata,
  aliases, optional catalog identifiers, linked sessions, linked projects,
  outputs, notes, and plan references.
- **FR-013**: System MUST allow users to create acquisition sessions and
  processing projects from a selected target so target metadata and prior context
  are stored in project/session metadata for review.
- **FR-013a**: System MUST use FITS/XISF `OBJECT` metadata as a target search
  hint when present, while requiring user review or confirmation before assigning
  ambiguous or unresolved targets.
- **FR-014**: System MUST support referenced observing-plan artifacts, including
  NINA plans where available, as metadata-linked files without executing,
  scheduling, or fully editing those plans in v1.
- **FR-015**: System MUST define a supported project structure required for
  app-managed projects.
- **FR-016**: System MUST create and own the supported outer project structure
  for app-managed projects, including app-controlled areas for project
  configuration, generated manifests, source views, notes, outputs, observed
  artifact registry, cleanup policy, and audit references.
- **FR-017**: System MUST keep selected processing-tool workspaces inside the
  app-owned project structure tool-managed/user-managed unless a specific
  generated artifact is explicitly app-created.
- **FR-018**: System MUST NOT ingest an existing processing project as an
  app-managed project unless it already conforms to the supported project
  structure or has first been migrated into that structure through user-approved
  guidance or a reviewed filesystem plan.
- **FR-019**: System MUST distinguish project-like brownfield material from
  app-managed projects during scan, inventory, and review.
- **FR-020**: System MUST allow each project to select a processing tool or
  workflow profile that informs source view generation, artifact classification,
  lifecycle expectations, and cleanup candidate rules.
- **FR-021**: System MUST include initial workflow profiles for PixInsight/WBPP
  and common planetary/lunar workflows, and MUST keep the profile model
  extensible for later tools such as Siril and other processing applications.
- **FR-022**: System MUST track capture and processing software context where it
  can be inferred or manually assigned, including capture tools such as SharpCap
  and common downstream stacking, sharpening, editing, or processing tools.
- **FR-023**: System MUST generate calibration candidate matches with confidence,
  match reasons, mismatch reasons, and user override state, including reuse cases
  where darks, biases, dark flats, flats, or masters apply across multiple
  sessions or projects.
- **FR-024**: System MUST maintain project manifests or equivalent generated
  documentation describing selected sources, calibration choices, output
  decisions, lifecycle state, cleanup policy, artifact observations, audit
  references, and user notes.
- **FR-025**: System MUST generate tool-friendly project source view plans from
  approved project source maps and selected workflow profile.
- **FR-026**: System MUST support source view strategies that can be compared and
  selected by safety, portability, disk use, and selected-tool compatibility.
- **FR-027**: System MUST track app-created source links, junctions, generated
  manifests, and generated folders so they can be cleaned up safely.
- **FR-028**: System MUST treat PixInsight processing workspaces as
  PixInsight-managed/user-managed areas rather than canonical app-managed data.
- **FR-029**: System MUST observe PixInsight processing workspaces on startup,
  refresh, or optional folder monitoring and register discovered directories and
  files as processing artifacts for review, lifecycle tracking, and cleanup
  planning.
- **FR-030**: System MUST classify observed PixInsight artifacts and other
  tool-specific processing artifacts separately from immutable sources, reusable
  calibration data, final outputs, manifests, notes, and app-generated source
  views.
- **FR-031**: System MUST define project lifecycle states covering at least
  candidate, active, prepared for selected processing tool, processing,
  finalized, archived, and cleanup-reviewed states, subject to research
  refinement.
- **FR-032**: System MUST track project outputs, final verification status,
  processing attempts, and cleanup/archive readiness.
- **FR-033**: System MUST identify cleanup and archive candidates with confidence
  levels, evidence, protected-category checks, and estimated reclaimable space.
- **FR-034**: System MUST protect original source files, calibration masters,
  final outputs, project manifests, user notes, audit records, and configured
  protected folders from cleanup by default.
- **FR-035**: System MUST support configurable cleanup policies at global and
  per-project levels, where projects inherit global defaults unless explicitly
  overridden.
- **FR-036**: System MUST support per-resource cleanup overrides for project
  directories, subdirectories, observed PixInsight artifacts, artifact groups,
  and PixInsight data types.
- **FR-037**: System MUST present cleanup policy and cleanup plan review as a
  nested directory/resource/type tree where users can check, uncheck, inherit,
  enable, disable, or override cleanup behavior at each applicable node.
- **FR-038**: System MUST allow cleanup plan actions to be changed during review,
  including keep, archive, trash, or delete only when permanent delete is
  explicitly enabled by policy.
- **FR-039**: System MUST represent every filesystem mutation as a reviewable plan
  before applying it.
- **FR-040**: System MUST require explicit user approval before applying a
  filesystem plan.
- **FR-041**: System MUST never silently overwrite existing files, folders, links,
  manifests, or audit records.
- **FR-042**: System MUST record an audit log for each planned and applied
  filesystem action, including failures and partial completion.
- **FR-043**: System MUST prefer archive or trash workflows over permanent delete
  for destructive cleanup actions.
- **FR-044**: System MUST allow users to configure naming templates,
  classification rules, retention rules, protected-folder rules, aliases, and
  taxonomy without editing application internals.
- **FR-045**: System MUST allow large-file hashing to be disabled, delayed, or
  limited to selected operations.
- **FR-046**: System MUST expose user-visible operation progress, pause/cancel
  behavior where safe, and recovery status for long scans, plan generation, and
  plan application.
- **FR-047**: System MUST preserve a language-neutral operation contract for
  frontend-to-core workflows, including request payloads, response payloads,
  errors, versioning, and long-running operation status.
- **FR-048**: System MUST treat the database as canonical for project
  relationships, lifecycle state, rules, cleanup policy, audit history, and
  operation state.
- **FR-049**: System MUST treat project manifests as generated, protected
  documentation or export artifacts rather than canonical state in v1.
- **FR-050**: System MUST version generated manifest formats and preserve a path
  to export/import or remote-service migration without requiring manually edited
  manifest files to become the source of truth.
- **FR-051**: System MUST include research decisions before finalizing default
  folder structures, naming rules, schema boundaries, lifecycle rules,
  calibration matching rules, source view strategies, and cleanup rules.

### Key Entities *(include if feature involves data)*

- **Library Root**: A user-registered filesystem root with platform path,
  identity hints, availability state, scan settings, and remapping history.
- **File or Folder Record**: A discovered filesystem item with root-relative path,
  type, size, timestamps, link status, optional hashes, metadata extraction
  status, and classification evidence.
- **Target**: An astronomical or lunar/planetary subject with metadata, aliases,
  optional catalog identifiers, linked acquisition sessions, linked projects,
  outputs, notes, and observing-plan references.
- **Observing Plan Reference**: A linked capture-plan artifact such as a NINA
  plan, stored as metadata and file reference for target/session context without
  making the app responsible for executing or editing the plan in v1.
- **Acquisition Session**: A source data grouping for captured lights or related
  raw material, linked to zero or more targets and independent from processing
  projects.
- **Calibration Session**: An independent grouping of calibration frames such as
  darks, flats, biases, dark flats, and related setup context that may serve
  multiple acquisition sessions or projects.
- **Calibration Master**: A reusable master calibration artifact with provenance,
  source frames, setup metadata, and compatibility evidence that may be linked to
  multiple sessions or projects.
- **Equipment and Optical Train**: Cameras, telescopes/lenses, reducers,
  rotators, focusers, filter wheels, filters, sites, and capture software used to
  build setup fingerprints.
- **Workflow Profile**: The selected processing tool or workflow family for a
  project, such as PixInsight/WBPP or a planetary/lunar workflow, that drives
  source view expectations, artifact classification, lifecycle cues, and cleanup
  policy defaults.
- **Software Tool**: Capture, stacking, sharpening, editing, or processing
  software inferred from files/folders/metadata or assigned by the user, such as
  SharpCap for capture or later workflow profiles such as Siril.
- **Project**: A processing workspace with its own mutable configuration and
  mapping records that reference immutable source sessions, calibration material,
  panels, selected workflow profile, software tool context, outputs, manifests,
  attempts, notes, lifecycle state, and cleanup/archive status.
- **Supported Project Structure**: The app-created, app-owned outer project
  envelope required for app-managed projects. It contains app-controlled areas
  for configuration, source views, generated documentation, notes, outputs,
  observed artifact registry, cleanup policy, and audit references, while
  selected processing-tool workspaces inside it remain tool/user-managed.
- **Project-Like Brownfield Material**: Existing processing folders or artifacts
  discovered during scans that may represent prior work but are not imported as
  app-managed projects unless they conform to the supported project structure or
  are migrated first.
- **Project Source**: A selected relationship between a project and an
  acquisition session, file set, calibration session, calibration master, or
  panel.
- **Source View**: A project-local representation for PixInsight input, generated
  from links, manifests, copies, folders, or a researched hybrid strategy.
- **Project Output**: Final or intermediate artifacts associated with a project,
  with verification, protection, and cleanup status.
- **Project Manifest**: Generated and protected project documentation, possibly
  machine-readable, human-readable, or both, that records selected sources,
  calibration decisions, source view state, observed processing artifacts,
  outputs, cleanup decisions, lifecycle state, and audit references while the
  database remains canonical.
- **Processing Artifact**: A PixInsight-created or user-created directory or file
  observed inside a processing workspace and registered for classification,
  lifecycle tracking, and cleanup planning without becoming canonical app-managed
  state.
- **Calibration Match Candidate**: A proposed calibration relationship with
  score, confidence, evidence, mismatches, and user decision.
- **Filesystem Plan**: A reviewable set of proposed move, copy, link, archive,
  delete, manifest, or cleanup actions.
- **Plan Item**: One filesystem action with source, destination, preconditions,
  conflict handling, expected result, and protection status.
- **Audit Log Entry**: Immutable record of plan creation, approval, application,
  action result, failure, retry, or rollback-related decision.
- **Rule or Template**: User-configurable classification, naming, retention,
  protected-folder, alias, taxonomy, and lifecycle policy.
- **Cleanup Policy**: Global, project-level, or per-resource rules that determine
  inherited cleanup behavior, protected categories, verification requirements,
  candidate artifact types, preferred action, and explicit overrides.
- **Cleanup Tree Node**: A project directory, subdirectory, resource, artifact
  group, or PixInsight data type shown in cleanup policy and plan review with
  inherited or overridden cleanup behavior.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A user can register an existing library root and complete an
  initial non-mutating inventory scan for at least 100,000 filesystem items while
  retaining a visible list of unclassified or low-confidence items.
- **SC-002**: 100% of filesystem mutation-capable operations MUST block
  application unless represented in a reviewable plan with clear source,
  destination, action type, conflict policy, and protection status before
  application is possible.
- **SC-003**: A user can create a multi-session project, select source sessions,
  review calibration candidates, and generate a project manifest in under 10
  minutes after the relevant source data has already been indexed.
- **SC-004**: For a finalized sample project, the app can estimate reclaimable
  disk space from cleanup candidates while protecting original sources,
  calibration masters, final outputs, manifests, notes, and audit history by
  default.
- **SC-005**: A moved or remounted library root can be remapped without losing
  project-to-source relationships or historical audit records.
- **SC-006**: Users can explain why a calibration candidate or cleanup candidate
  was proposed by inspecting recorded evidence and confidence details.
- **SC-007**: The product specification and plan identify explicit research
  decisions for architecture, folder structure, naming, metadata extraction,
  calibration matching, source views, lifecycle, filesystem safety, and contract
  strategy before implementation tasks begin.

## Assumptions

- The primary user is an individual astrophotographer managing a large local
  library and using PixInsight/WBPP, planetary/lunar tools, or later supported
  workflow profiles for processing.
- v1 is a local-first desktop GUI application; cloud sync and multi-user
  collaboration are later-phase extensions unless research proves they are
  necessary earlier.
- Existing source files may be messy, partially duplicated, inconsistently named,
  and spread across broad folders or external drives.
- Targets are first-class in v1 for organization and history. Full observing
  plan authoring, scheduling, and execution are later-phase features.
- The application may index existing messy libraries, but brownfield processing
  projects are not ingested as app-managed projects unless the supported project
  structure is already present or is created first through migration guidance or
  a reviewed migration plan.
- The app creates and owns the supported outer project structure for
  app-managed projects. Processing-tool workspaces inside that structure remain
  tool/user-managed unless an artifact is explicitly generated by the app.
- The application may recommend defaults, but source/session classification
  cannot assume a single folder convention, one object per session, or one
  project per object.
- Raw acquisition sessions are immutable source records and are referenced from
  projects by default rather than copied into each project by default.
- Projects keep separate mutable configuration and mapping records, so changing
  project source selections, calibration choices, or processing attempts does not
  rewrite the acquisition session.
- PixInsight processing workspaces are PixInsight-managed/user-managed. The app
  observes them through refresh/startup scans or optional monitoring and registers
  discovered artifacts for review and cleanup planning.
- Cleanup policies use inheritance: project policies inherit global defaults,
  directories and resources inherit project policy, and users can override
  cleanup behavior at any displayed project directory, subdirectory, resource, or
  PixInsight data type.
- Project manifests are valuable as documentation and portability artifacts, but
  the database remains canonical. JSON, JSONL, Markdown, or hybrid manifest
  formats are research decisions and must account for future remote-service
  migration.
- Calibration material is independent from target/project organization. Darks,
  biases, dark flats, flats, and masters can be reused across targets, sessions,
  or projects when setup compatibility supports reuse.
- Flat reuse is possible for static setups or multi-target sessions, but must be
  confidence-scored because dust, rotation, focus, filter, binning, and optical
  train changes can invalidate reuse.
- PixInsight/WBPP and other selected processing tools remain responsible for
  image/video processing.
- Projects select a workflow profile during creation. PixInsight/WBPP and common
  planetary/lunar workflows are v1 candidates; Siril and other tools should be
  supported later through the same profile model when researched.
- Technical stack preferences are inputs to research, not final architecture
  decisions in this specification.
- Contract/schema definitions are language-neutral source-of-truth artifacts.
  Tauri commands may be the first transport, but project semantics must remain
  portable to other local or remote backends.

## Out of Scope

- Calibrating, debayering, registering, integrating, drizzling, stacking, or
  editing images.
- Replacing PixInsight or WBPP.
- Unreviewed filesystem deletion or mutation.
- Requiring immediate migration of the existing library into a new folder
  convention.
- Cloud sync for v1 unless later research moves it into scope.
- A command-line interface for v1 unless later research identifies it as
  necessary for safe operations or testing.
- A backend contract that is inherently tied to one implementation language.
