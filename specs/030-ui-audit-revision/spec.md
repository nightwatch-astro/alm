# Feature Specification: UI Audit & Revision

**Feature Branch**: `030-ui-audit-revision`

**Created**: 2026-05-26

**Status**: Superseded (2026-06-11) — UI/component tasks delivered by
[Spec 032 — Design V4](../032-design-v4-implementation/spec.md) (the current UI
truth, reached via Spec 031 Design V3). Surviving domain/back-end concepts
(equipment CRUD, session grouping, calibration matching, lifecycle states,
cleanup policy) are owned by their dependent backend specs (002, 005–019,
023–026), not by this audit. Retained as the historical design-audit baseline.
Its `tasks.md` was retired (2026-08-17); task state lives in beads.

**Input**: Interactive screen-by-screen UI audit of every screen in the Astro
Library Manager desktop app, consolidating findings into a comprehensive
redesign spec.

## User Scenarios & Testing

### User Story 1 — First-Run Setup (Priority: P1)

A new user installs the app and needs to register source folders, configure
processing tools, download target catalogs, and confirm before starting work.

**Why this priority**: The setup wizard is the first interaction every user has
with the app. A confusing or tedious first-run experience causes abandonment.

**Independent Test**: Complete the wizard from a fresh install, verify all
source types are registered, tools are validated, and the app advances to the
main workflow.

**Acceptance Scenarios**:

1. **Given** a fresh install, **When** the user opens the app, **Then** a
   4-step wizard is shown (Source Folders → Processing Tools → Catalogs →
   Confirm).
2. **Given** the Source Folders step, **When** the user adds folders via the
   native OS picker and tags each with a type, **Then** all six required types
   (Light Frames, Dark, Flat, Bias, Project, Inbox) must be registered before
   proceeding.
3. **Given** a non-empty folder is selected (excluding Inbox), **When** added,
   **Then** a warning badge is shown but the user is not blocked.
4. **Given** the Processing Tools step, **When** the user enables a tool and
   browses to an executable, **Then** the path is validated immediately and
   errors shown inline.
5. **Given** the Confirm step, **When** any required source type is missing or
   an enabled tool has an invalid path, **Then** the Finish button is blocked.

---

### User Story 2 — Inbox Session Review (Priority: P1)

A user drops new captures into the Inbox folder. The app detects them, groups
them into sessions, and the user reviews, edits properties, and confirms or
rejects each session.

**Why this priority**: Inbox review is the core ingestion workflow — every
frame enters the library through this screen.

**Independent Test**: Drop FITS files into an Inbox folder, verify sessions
appear in the Inbox screen, edit properties, confirm a session, and verify it
moves to Sessions or Calibration.

**Acceptance Scenarios**:

1. **Given** new FITS files in an Inbox folder, **When** detected by the
   filesystem watcher or startup scan, **Then** sessions appear in the Inbox
   list with auto-generated names (OBJECT + DATE + FILTER).
2. **Given** a session with conflicting properties (e.g., two gains), **When**
   the user views it, **Then** a banner states the session should be split and
   confirmation is blocked until resolved.
3. **Given** the user clicks Confirm, **When** all fields are marked correct,
   **Then** an overlay shows the confirmed properties and target directory
   structure before finalizing.
4. **Given** a confirmed session, **Then** it moves to Sessions (lights) or
   Calibration (darks/flats/bias) and is locked — it cannot be moved back.
5. **Given** the user clicks Reject, **Then** the session moves to Archive.

---

### User Story 3 — Project Lifecycle Workflow (Priority: P2)

A user creates a project, selects light sessions and calibration, generates
source views for their processing tool, processes externally, and marks the
project complete. Cleanup opportunities are then scanned.

**Why this priority**: Projects are the central workflow unit tying sessions,
calibration, and outputs together.

**Independent Test**: Create a project, add sessions and calibration, generate
source views, mark complete, and verify cleanup scan runs.

**Acceptance Scenarios**:

1. **Given** a new project in Setup phase, **When** the user adds sessions and
   calibration via the lifecycle sidebar, **Then** the source map columns
   populate correctly.
2. **Given** Ready phase, **When** the user clicks Generate Views, **Then**
   source views are created and the project auto-advances to Processing.
3. **Given** Processing phase, **When** the user clicks Mark Complete, **Then**
   the app scans for cleanup opportunities and shows a reviewable plan.
4. **Given** the cleanup plan, **When** the user approves, **Then** files are
   deleted per the approved plan and an audit event is recorded.

---

### User Story 4 — Consistent Navigation & Layout (Priority: P2)

A user navigates between Sessions, Calibration, Targets, and Archive using
consistent list/filter/sort controls and layout patterns across all screens.

**Why this priority**: Layout inconsistency across screens creates cognitive
load and makes the app feel unfinished.

**Independent Test**: Navigate to each main screen and verify search, group,
sort, and filter controls are identical in position and behavior.

**Acceptance Scenarios**:

1. **Given** any list screen (Inbox, Sessions, Calibration, Targets, Projects,
   Archive), **When** opened, **Then** the left sidebar has the same search,
   group, sort, and filter control layout.
2. **Given** Inbox or Projects, **When** a detail is selected, **Then** a right
   action sidebar is shown with lifecycle-contextual actions.
3. **Given** Sessions, Calibration, Targets, or Archive, **When** a detail is
   selected, **Then** actions appear in a top action bar (no right sidebar).

---

### User Story 5 — Settings Reorganization (Priority: P3)

A user opens Settings and finds logically grouped panes for data sources,
equipment, ingestion, naming, tools, calibration matching, catalogs, cleanup,
general appearance, and advanced/audit.

**Why this priority**: Settings is a configuration tool used occasionally, not
a daily workflow screen.

**Independent Test**: Navigate to each Settings pane and verify controls are
functional, consistently styled, and logically organized.

**Acceptance Scenarios**:

1. **Given** the Data Sources pane, **When** the user clicks Add Folder,
   **Then** a native OS picker opens and the folder can be tagged with a type.
2. **Given** the Data Sources pane, **When** a root exists, **Then** the user
   can edit, remove, or reveal it in Explorer.
3. **Given** the Equipment pane, **When** the user adds an optical train,
   **Then** an inline row appears with dropdowns for camera and telescope.
4. **Given** the Cleanup pane, **When** the user sets a per-type action,
   **Then** the setting applies globally without per-tool breakdown.

---

### User Story 6 — Status Bar & Sidebar Footer (Priority: P3)

A user glances at the bottom of the window and sidebar to see operational
status: inbox activity, library stats, cleanup opportunities, storage health,
and root connectivity.

**Why this priority**: Glanceable status reduces the need to navigate to check
system health.

**Independent Test**: Verify status bar shows inbox count, library stats,
cleanup available, and storage health. Verify sidebar footer shows root health.

**Acceptance Scenarios**:

1. **Given** files awaiting review, **When** the user looks at the status bar,
   **Then** an inbox badge shows the count, clickable to Inbox.
2. **Given** a volume with less than 10% free space, **When** the status bar
   renders, **Then** the storage health indicator shows a warning color.
3. **Given** a NAS root is offline, **When** the sidebar footer renders,
   **Then** an amber dot and the offline root name are shown.

---

### Edge Cases

- What happens when a session has frames with no OBJECT header? → "Unknown
  target" is shown; user must edit before confirming.
- What happens when all roots are offline? → Red dot in sidebar footer, all
  list screens show empty state with explanation.
- What happens when a user enables a tool but points to a bad executable? →
  Inline error immediately, Finish blocked in wizard, path error in Settings.
- What happens when a confirmed session is used in a project and the user
  tries to move it back to Inbox? → Action disabled with tooltip explaining
  why.
- What happens when a project is archived? → Project envelope deleted, original
  sessions/calibration managed independently from Archive screen.
- What happens when FITS SET-TEMP is missing? → Fall back to CCD-TEMP.
- What happens when a filter is not detected from FITS? → User selects from
  predefined list grouped by category, or adds custom.

## Requirements

### Functional Requirements

**Navigation & Layout**

- **FR-001**: App MUST show six top-level nav items plus Settings: Inbox,
  Sessions, Calibration, Targets, Projects, Archive.
- **FR-002**: "Review Queue" MUST be renamed to "Inbox" throughout the app.
- **FR-003**: "Plans" and "Audit Log" MUST be removed from top-level
  navigation.
- **FR-004**: Inbox and Projects MUST use a right action sidebar. Sessions,
  Calibration, Targets, and Archive MUST use a top action bar.
- **FR-005**: All list screens MUST share identical search, group-by, sort-by,
  and filter controls in the same layout position.
- **FR-006**: All action buttons within a view MUST be the same width with
  hotkeys shown.

**Setup Wizard**

- **FR-010**: The wizard MUST have 4 steps: Source Folders, Processing Tools,
  Catalogs, Confirm.
- **FR-011**: Source Folders step MUST use a single unified add-folder flow with
  a type selector (Light Frames, Dark, Flat, Bias, Project, Inbox).
- **FR-012**: All six source types MUST be required. Multiple folders per type
  MUST be allowed.
- **FR-013**: Folder emptiness MUST be checked (ignoring OS hidden files);
  non-empty folders (except Inbox) MUST show a warning but not block.
- **FR-014**: Processing Tools step MUST show PixInsight and Siril only
  (planetary tools hidden, DeepSkyStacker dropped).
- **FR-015**: Each enabled tool MUST have a file browser labelled "Choose
  executable" with immediate path validation.
- **FR-016**: Tool detection MUST be mock for v1, clearly labelled as mock.
- **FR-017**: Catalogs step MUST have a "Download All" button and individual
  catalog toggles.
- **FR-018**: Confirm step MUST show folder list with empty/not-empty status,
  tools with valid/invalid status, and catalogs downloaded.
- **FR-019**: Finish MUST be blocked if any required source type is missing or
  any enabled tool has an invalid path.
- **FR-020**: No "Reset wizard" button inside the wizard. No welcome step. No
  advanced/scan-depth controls.

**Inbox**

- **FR-030**: Inbox MUST detect files via filesystem watcher (live) and scan on
  startup.
- **FR-031**: Each inbox session MUST show OBJECT (or TYPE for calibration),
  DATE, FILTER, and optionally total integration time in the list.
- **FR-032**: Session detail MUST show a unified property table with per-field
  source indicator (FITS / User / Inferred / Default — the FR-137 pill set;
  a missing value is a value state per FR-135, not a source) and confirm
  checkbox.
- **FR-033**: Auto-detected properties MUST be visually highlighted.
- **FR-034**: Conflicting properties within a session MUST show an inline
  conflict indicator and a banner stating the session should be split.
  Confirmation MUST be blocked until conflicts are resolved.
- **FR-035**: Exposure time differences beyond a configurable margin (default
  2s) MUST flag the session for splitting.
- **FR-036**: Temperature grouping MUST use a configurable tolerance (default
  5°C).
- **FR-037**: The filter field MUST offer a predefined list grouped by category
  (Narrowband: Ha/SII/OIII/NII, Broadband: L/R/G/B, Dual-band: HO/SO,
  Other: UV/IR Cut) plus custom. Default for unfiltered/OSC: "L".
- **FR-038**: Filters MUST be auto-added as detected from FITS metadata.
- **FR-039**: Split MUST show a preview of conflicting properties and resulting
  session count. The original session is replaced by N new sessions.
- **FR-040**: Merge MUST search/select sessions whose properties match (inverse
  of split rules).
- **FR-041**: Confirmation overlay MUST show all confirmed properties and the
  target directory structure from the token pattern system.
- **FR-042**: Confirmed sessions MUST move to Sessions (lights) or Calibration
  (darks/flats/bias) and be locked.
- **FR-043**: Rejected sessions MUST move to Archive.
- **FR-044**: No "Skip" button, no "Re-open existing confirmation", no "what
  about calibration" section, no confidence scores.

**Sessions**

- **FR-050**: Sessions MUST display confirmed light frame data as read-only.
- **FR-051**: Three view modes MUST be available: List, Calendar Grid, Calendar
  Scroll.
- **FR-052**: Calendar Grid MUST show sessions as prominent visual badges with
  hover tooltips.
- **FR-053**: Calendar Scroll MUST be a vertical scrolling timeline with sticky
  month headers.
- **FR-054**: Session detail MUST use a single unified property table (not split
  columns).
- **FR-055**: "Move to Inbox" action MUST be disabled if the session is used in
  any project.
- **FR-056**: "Reveal in Explorer" MUST be a standard action on all detail
  views backed by files.

**Calibration**

- **FR-060**: Calibration MUST show both masters and individual sub-frame sets.
- **FR-061**: Masters MUST be distinguished from subs via FITS headers where
  possible and shown with a "Master" badge.
- **FR-062**: A highlighted "Matching Fingerprint" section MUST appear at the
  top of the detail, showing the properties that determine compatibility.
- **FR-063**: Compatible sessions MUST be shown as binary match (match or no
  match, no scores).
- **FR-064**: Aging badge MUST appear when age exceeds 1 year (configurable).

**Targets**

- **FR-070**: Targets MUST show identity, sessions table, coverage chart,
  projects table, and observing plans.
- **FR-071**: Coverage chart MUST filter by optical train (per-train view only).
- **FR-072**: Sessions table MUST support multiple projects per session,
  displayed as stacked clickable names.
- **FR-073**: Grouping options MUST include type, constellation, catalog, and
  project.

**Projects**

- **FR-080**: Project lifecycle MUST have 5 phases: Setup → Ready → Processing
  → Completed → Archived.
- **FR-081**: "Prepared" phase MUST be removed — generating source views
  auto-advances from Ready to Processing.
- **FR-082**: The right sidebar MUST show lifecycle-contextual actions that
  change per phase.
- **FR-083**: Source map MUST use the column layout (Lights / Darks / Flats /
  Bias).
- **FR-084**: Pipeline MUST be shown as a compact stats bar, not a flow chart.
- **FR-085**: Source views MUST show a compact status (generated/not, file
  counts, path) — no file-level symlink details.
- **FR-086**: Notes MUST support inline markdown editing with create/edit/view.
- **FR-087**: Cleanup MUST run automatically on project completion and be
  triggerable manually.
- **FR-088**: Cleanup MUST show a reviewable plan before any deletion.

**Archive**

- **FR-090**: Archive MUST be a top-level nav item showing archived items from
  all sources (sessions, calibration, projects).
- **FR-091**: Archive MUST support the same search, sort, and grouping as other
  list screens.
- **FR-092**: Archived sessions MUST be re-queueable to Inbox.
- **FR-093**: Delete/cleanup from Archive MUST follow reviewable filesystem
  plan rules.

**Settings**

- **FR-100**: Settings MUST have 11 panes: Data Sources, Equipment, Ingestion,
  Naming & Structure, Processing Tools, Calibration Matching, Target Catalogs,
  Cleanup, General, Advanced, Audit Log.
- **FR-101**: Data Sources MUST allow add, edit, remove, and reveal for
  registered roots. No greyed-out buttons. No scan defaults (moved to
  Ingestion). No restart wizard.
- **FR-102**: Equipment MUST consolidate optical trains, cameras, telescopes,
  and filter library. No mounts.
- **FR-103**: Optical train add MUST use inline-editable table rows with
  dropdowns for existing equipment.
- **FR-104**: Ingestion MUST include scan defaults (follow symlinks/junctions,
  hashing, metadata extraction), watcher toggle, rescan button, and grouping
  tolerances.
- **FR-105**: Naming patterns MUST differ per frame type by default (e.g.,
  darks don't use `{object}` token). Token insertion via dropdown. No dark flat
  type.
- **FR-106**: Source view strategy MUST be a dropdown with 4 options (NTFS
  junctions, symbolic links, hard links, full copy). No manifest-only, no
  hybrid.
- **FR-107**: Processing Tools MUST show only PixInsight and Siril. Each tool
  MUST have enable/disable toggle, "Choose executable" browser, and per-tool
  directory structure template with vendor defaults.
- **FR-108**: Calibration Matching MUST include tolerance settings for
  temperature, exposure time, aging, and equipment matching.
- **FR-109**: Target Catalogs MUST include Messier, NGC/IC, Caldwell,
  Sharpless, Abell Planetary with enable/disable toggles and Download All.
- **FR-110**: Cleanup MUST use a per-type action table (Keep/Archive/Delete)
  without per-tool breakdown or triggers. Manual vs auto-on-completion toggle.
- **FR-111**: No source protection age threshold, no file type protection, no
  category protection checkboxes.
- **FR-112**: General (renamed from Appearance) MUST include theme, font size,
  and density. No guided tour.
- **FR-113**: Advanced MUST merge application log into it.
- **FR-114**: Audit Log MUST be accessible as a Settings pane with search,
  filters, date range, and pagination.

**Status Bar & Sidebar Footer**

- **FR-120**: Status bar MUST show: inbox badge (when items pending), ingestion
  progress (when active), library stats, cleanup available, and storage health
  per volume.
- **FR-121**: Storage health MUST show warning color when a volume drops below
  10% free (configurable).
- **FR-122**: Sidebar footer MUST show root health with colored dot and offline
  root names, clickable to Data Sources settings.
- **FR-123**: Status bar MUST NOT show directory paths, "Idle" text, or "Last
  scan" timestamps.

**Durable Audit Coverage** *(iteration 2026-07-14, grilling Q15 / #647)*

- **FR-130**: Every attempted mutation of durable state or user data MUST
  write a durable audit row — including settings changes, protection
  overrides, equipment CRUD, source enable/disable/register/delete, and
  rescans/root operations — recording the outcome including refused/failed
  with a reason/code. Value-unchanged no-op writes are not mutations and
  emit no audit row; refused and failed attempts do. Debounced/noisy
  durable-data settings keys (e.g. the `pattern` naming key) are audited
  once at the committed value with before→after — never per keystroke
  (T122).
- **FR-131**: The durable `audit_log_entry` store MUST be the single source
  of truth for audit history. Audit-worthy actions MUST write the durable
  row AND emit a live event to the bus; emitting to the bus only is
  prohibited for mutations. Any `auditId` returned to the UI MUST resolve to
  a durable `audit_log_entry` row.
- **FR-132**: The Activity/log panel MUST read user-meaningful events from
  the durable audit store, and transient/internal noise from the live event
  bus.
- **FR-133**: The audit entry shape MUST generalize from a
  lifecycle-transition record to a generic mutation record: timestamp,
  actor, action, entity (type + id), outcome + reason, plus an optional
  before→after value pair for settings/protection changes.
- **FR-134**: Reads, navigation, UI state changes, and transient
  internal/periodic events MUST NOT be durably audited. UI-state settings
  keys (e.g. `rememberFollowLogs`, `plansListDefaultAgeCutoffDays`) fall
  under this exemption.

**Missing-Value Semantics & Detail Panels** *(iteration 2026-07-14, grilling Q16 / #620, #619)*

- **FR-135**: Every displayed metadata field MUST be distinguishable in one
  of three states everywhere it renders: a real value (including a real 0),
  unresolved/missing (no data), or not-applicable (the field does not apply
  to the entity). The three states MUST be modeled, not inferred at render
  time.
- **FR-136**: Missing values MUST be represented as null/None end-to-end —
  extraction → persistence → application layer → contract → UI. Numeric
  fields MUST NEVER default to 0 (or any other sentinel) to stand in for
  absence; contract DTO fields whose values can be absent MUST be nullable.
- **FR-137**: One shared value renderer MUST be used everywhere metadata
  field values are displayed — `renderValue(value, {source, applicability})`,
  where applicability comes from the field-applicability matrix
  (data-model.md), never inferred from data absence: real value → the value
  plus its source pill; unresolved → a distinct muted "unresolved" chip,
  never 0; not-applicable → blank/"—" without any chip. Composed
  identifying strings (list meta lines, titles) MUST omit missing tokens
  entirely — no chip, no sentinel.
- **FR-138**: Source/provenance indicators MUST only appear for present
  values. Absence MUST NOT be attributed to a source (no "FITS" pill on a
  missing value).
- **FR-139**: Detail panels MUST add information beyond the selected row's
  list columns — full metadata, provenance/source, related entities,
  history, and actions — and MUST lead with what is new. A small
  identifying summary of the row is permitted but MUST NOT dominate the
  panel.
- **FR-140**: Detail panels MUST stay minimal and curated — only relevant
  data for the entity, not a raw dump of every available field.

### Key Entities

- **Inbox Session**: Auto-detected grouping of FITS frames awaiting user
  review. Has properties (target, filter, date, camera, etc.), frame
  membership, and a confirm/reject lifecycle.
- **Confirmed Session**: Locked light frame session with read-only properties.
  Can be used in projects, moved to inbox (if unused), or archived.
- **Calibration Entry**: Master or sub-frame set with a matching fingerprint.
  Binary compatibility with sessions. Aging tracked.
- **Target**: Catalog entry used for session matching and project organization.
  Not an entity sessions are "linked" to — the target field on a session is
  metadata.
- **Project**: Lifecycle workflow unit binding sessions, calibration, a tool
  profile, source views, notes, and cleanup plans.
- **Archive Item**: Rejected or archived session, calibration, or project
  envelope. Can be re-queued or deleted.
- **Optical Train**: Named combination of telescope + camera with focal length.
  Used for session grouping and calibration matching.

## Success Criteria

### Measurable Outcomes

- **SC-001**: Users can complete the setup wizard in under 3 minutes for a
  standard library with 4-6 source folders and 1 processing tool.
- **SC-002**: Users can review and confirm an inbox session in under 60 seconds
  when all properties are auto-detected correctly.
- **SC-003**: Navigation between any two main screens takes a single click.
- **SC-004**: All list screens share identical control layout — a user who
  learns one screen can immediately use any other.
- **SC-005**: The project lifecycle from Setup to Completed requires no more
  than 5 user actions (add sources, generate views, process externally, mark
  complete, approve cleanup).
- **SC-006**: Settings pane count is reduced from 12 to 11 with no loss of
  functionality.
- **SC-007**: Top-level navigation items are reduced from 9 to 7 without
  hiding frequently used workflows.
- **SC-008**: Storage health warnings are visible at a glance without
  navigating to any specific screen.
- **SC-009**: 100% of mutation commands that return an `auditId` return one
  that resolves to a durable `audit_log_entry` row; zero mutation paths emit
  to the bus without a durable `audit_log_entry` write.
- **SC-010**: An entity with missing numeric metadata never displays a
  defaulted 0 (e.g., "Gain 0", "Exposure 0s", "Size 0 KB"); 100% of
  field-value renderings (table cells, property rows, meta lines) across
  Inbox, Sessions, Calibration, Targets, Projects, and Archive go through
  the shared renderer and are distinguishable as real / unresolved /
  not-applicable; composed identifying strings omit missing tokens.
- **SC-011**: Every detail panel presents at least one information class
  (full metadata, provenance, related entities, history, or actions) that
  is not present in its list row.

## Assumptions

- The app runs on a single platform per installation (no cross-platform config
  needed at runtime).
- FITS headers follow standard conventions for IMAGETYP, OBJECT, FILTER,
  INSTRUME, DATE-OBS, SET-TEMP, CCD-TEMP, GAIN, XBINNING.
- Users have at most 2-3 processing tools installed simultaneously.
- The filesystem watcher and startup scan are sufficient for inbox detection —
  no polling interval needed.
- Mock tool detection is acceptable for v1; real detection is deferred.
- NINA is the only observing plan format needed for v1.
- DeepSkyStacker support is deferred to a future spec.
- Planetary/lunar tools are deferred to a future spec.

## Iterations

### Iteration 2026-07-14: Durable audit coverage & unification (Q15 / #647)

**Change**: Every attempted mutation of durable state writes a durable
`audit_log_entry` row (outcome incl. refused/failed + reason/code); the
durable table becomes the authoritative audit record over the live event
bus;
the entry shape generalizes from a lifecycle-transition record to a generic
mutation record. Decisions locked by
`docs/development/ui-campaign-grilling-decisions-2026-07-13.md` §Q15.
**Scope**: Feature-wide (new requirement block)
**Artifacts updated**: spec.md (FR-130–FR-134, SC-009, §8.3), plan.md
(phase G, technical context), tasks.md (Phase 10, T120–T127),
data-model.md (generalized audit entry), contracts/commands.md (audit
semantics)
**Tasks added**: T120–T127
**Iteration record**: `iteration-2026-07-14-applied.md`

### Iteration 2026-07-14: Missing-value semantics & detail-as-delta (Q16 / #620, #619)

**Change**: Three distinguishable value states (real / unresolved /
not-applicable) modeled as null/None end-to-end with no numeric
zero-defaulting, rendered through one shared renderer with source pills
only on present values; detail panels reframed to add information over
their list rows. Decisions locked by
`docs/development/ui-campaign-grilling-decisions-2026-07-13.md` §Q16.
**Scope**: Feature-wide (new requirement block)
**Artifacts updated**: spec.md (FR-135–FR-140, SC-010–SC-011, §12),
plan.md (phase H, technical context), tasks.md (Phase 11, T128–T134),
data-model.md (metadata value states), contracts/commands.md
(missing-value semantics)
**Tasks added**: T128–T134
**Iteration record**: `iteration-2026-07-14-q16-applied.md`

---

# Detailed Specification

## 1. Setup Wizard

The current 8-step wizard (Welcome → Raw → Calibration → Project → Inbox →
Detect Tools → Catalogs → Confirm) is replaced with a streamlined 4-step
wizard.

### 1.1 Step 1 — Source Folders

A brief welcome paragraph at the top introduces the app and explains that the
user needs to register the folders where their astrophotography data lives.

Below the intro is an initially empty folder list with an **Add Folder** button.
Clicking it opens the **native OS folder picker** (not a browser popup). After
selecting a folder the user chooses its type from a selector.

**Folder types (all required):**

| Type | Description |
|------|-------------|
| Light Frames | Where unprocessed light sub-exposures are stored |
| Dark | Where dark calibration frames or masters are stored |
| Flat | Where flat calibration frames or masters are stored |
| Bias | Where bias/offset calibration frames or masters are stored |
| Project | Where processing projects and outputs are organized |
| Inbox | Drop folder for new captures awaiting classification |

- All six types are required. A validation summary below the list shows which
  required types are still missing.
- Multiple folders per type are allowed.
- All folders must be empty except Inbox. Standard OS hidden files
  (`.DS_Store`, `Thumbs.db`, `desktop.ini`, zone identifier files) are ignored
  when checking emptiness.
- Non-empty folders produce a visible warning badge but do **not** block
  progress.
- No "Advanced" disclosure or scan-depth controls.
- No "Reset wizard" button inside the wizard.

### 1.2 Step 2 — Processing Tools

Toggle switches for each supported tool. Planetary/lunar tools are hidden for
v1. Shown tools:

| Tool | Notes |
|------|-------|
| PixInsight | Primary deep-sky processing tool |
| Siril | Free open-source alternative |
| DeepSkyStacker | Windows stacking tool |

Each enabled tool shows:

- A **mock detection status** clearly labelled as mock (e.g.,
  "Auto-detection not yet available").
- A **file browser** button to locate the executable on disk.
- **Immediate path validation** — once a path is selected, the UI confirms the
  file exists or shows an error inline.

A tool can be toggled off again to remove its path requirement.

### 1.3 Step 3 — Catalogs

Target catalogs (NGC, IC, Messier, Caldwell, etc.) used for project management
and identifying which imaging sessions belong to which targets.

- **Download All** button to fetch every available catalog at once.
- Individual catalog toggles for selective download.
- Description explains the purpose: these catalogs let the app match FITS
  OBJECT headers to known targets and organize projects around catalog entries.

### 1.4 Step 4 — Confirm

Summary/confirmation screen. No free-text, no "what happens next."

Shows:

- **Folder list** — each registered folder with its type badge and an
  empty/not-empty status indicator. Non-empty folders (except Inbox) show a
  warning badge.
- **Processing tools** — each enabled tool with its path and valid/invalid
  status.
- **Catalogs** — which catalogs were downloaded.

**Blocked on Finish if:**
- Any of the six required source types has no folder registered.
- Any enabled processing tool has an invalid or missing executable path.

---

## 2. Inbox (was "Review Queue")

Rename "Review Queue" to **Inbox** throughout the app. This screen shows
sessions detected from files dropped into registered Inbox folders. Detection
uses a filesystem watcher (live) and a scan on app startup.

### 2.1 Left Sidebar — Session List

The left sidebar lists detected inbox sessions. Each entry shows:

- **Line 1**: OBJECT name (or "Unknown target" if no OBJECT header; or frame
  TYPE for calibration, e.g., "Dark", "Flat", "Bias")
- **Line 2**: Date / night
- **Line 3**: Filter (or "Unknown" if not detected)
- **Line 4**: Total integration time (optional, if available)

**Sort / filter / search** mirrors the Sessions screen: text search, group by
(target, date, filter, etc.), sort by (date, name, frame count), filter by
frame type. No confidence-based sorting or display.

### 2.2 Session Review Panel

#### Header

Session name (auto-generated from OBJECT/TYPE + DATE + FILTER), prominent
status badge, frame count + total integration time as subtitle. Styled as a
clear visual header — not the dense two-line layout of the current UI.

#### Properties Table

A single unified table (not split across columns). Each row shows:

| Column | Purpose |
|--------|---------|
| Field name | Property label |
| Value | Current value (editable inline) |
| Source | Badge: FITS / User / Inferred / Default (FR-137 pill set; a missing value renders as the unresolved chip per FR-137 — "missing" is not a source state) |
| Confirmed | Checkbox — user marks field as verified |

**Fields:**

| Field | Input type | Notes |
|-------|-----------|-------|
| Target / Object | Autocomplete from catalogs | "Unknown target" if missing from header; hidden for calibration |
| Frame type | Dropdown: Light, Dark, Flat, Bias | Changing type hides/shows other fields (e.g., Target hidden for calibration) |
| Filter | Dropdown grouped by category + custom | See filter list below; default "L" for OSC/unfiltered |
| Date / Night | Date picker | |
| Camera | Text / autocomplete | |
| Telescope / Optical train | Text / autocomplete | |
| Focal length | Numeric | |
| Exposure time | Read-only computed | Shows grouped rows: e.g., "300s x 10 = 50min" and "60s x 20 = 20min" |
| Gain | Numeric | ISO not shown |
| Binning | Dropdown: 1x1, 2x2, 3x3, 4x4 | |
| Set temperature | Numeric (°C) | Falls back to CCD-TEMP if SET-TEMP absent; shown for Lights and Darks only, hidden for Flats and Bias |
| Observer / Location | Text | Inferred from SITELAT/SITELONG if available |
| Timezone | Dropdown | Inferred from location if available |

**Filter list (predefined, grouped):**

| Category | Filters |
|----------|---------|
| Narrowband | Ha, SII, OIII, NII |
| Broadband | L, R, G, B |
| Dual-band | HO, SO |
| Other | UV/IR Cut |
| Custom | User-defined (free text) |

Filters are also added automatically as they are detected from FITS metadata
across all sessions. Users can manage their filter library in Settings.

**Auto-detected property highlighting**: properties extracted from FITS headers
are visually marked as auto-detected. If a session contains frames with
conflicting values for the same property (e.g., two different gains, two
different filters), the conflicting field shows an inline conflict indicator
and a banner appears stating the session should be split. Confirmation is
blocked until conflicts are resolved (either by splitting or manual override).

**Exposure time split rule**: if frames have exposure times differing by more
than a configurable margin (default 2s, configurable in Settings), the session
is flagged as needing a split.

**Temperature grouping tolerance**: frames are grouped into the same session if
their set temperature is within a configurable tolerance (default 5°C,
configurable in Settings — same setting used for dark frame matching).

#### Frames Summary

Shows exposure groups as rows:

```
300s x 10 = 50min
 60s x 20 = 20min
```

No HFR, no "frames flagged" — that data is not available at inbox stage.

#### Frames List

Expandable list of individual files belonging to this session. Per file:
filename and basic FITS header summary.

#### Missing Information Handling

If all or some frames are missing required metadata, a correction popup is
shown prompting the user to fill in the missing fields. Same edit UI as the
properties table.

### 2.3 Actions Sidebar

All buttons must be the **same width**. Hotkeys shown on every button.

| Action | Hotkey | Behavior |
|--------|--------|----------|
| Confirm | | Opens confirmation overlay (see 2.4). Blocked if: required fields missing, conflicts unresolved, or enabled tool paths invalid |
| Reject | | Moves session to Archive |
| Split | | Only visible when conflicting properties are detected. Shows preview: which properties conflict and how many sessions the split would produce (e.g., 2 gains x 2 filters = 4 sessions). Original session is replaced by N new sessions. Backend tracks file-to-session membership |
| Merge | | Search/select other sessions whose properties match merge criteria (inverse of split rules — if they would not be split, they can be merged) |
| Edit | | Enables inline editing of all property fields (replaces "Corrections") |

No "Skip" button (user can simply navigate away). No "Re-open existing
confirmation." No "what about calibration" section — calibration matching
happens in Sessions, not Inbox.

### 2.4 Confirmation Overlay

When the user clicks Confirm, an overlay shows:

- All confirmed properties in a read-only summary
- **Target directory structure** — where the session files will be moved,
  rendered from the token pattern system (e.g.,
  `Lights/M31/2026-01-15/Ha/`). Purely informational, not editable.
- Final Confirm / Cancel buttons

After confirmation the session moves to Sessions (for lights) or Calibration
(for darks/flats/bias) and is **locked** — it cannot be moved back to Inbox.

### 2.5 Session Naming Rules

**Acquisition (light) frames:**
`{OBJECT} - {DATE} - {FILTER}`
Example: "M31 - 2026-01-15 - Ha"
If OBJECT is missing: "Unknown target - 2026-01-15 - Ha"

**Calibration frames:**
`{TYPE} - {DATE} - {SET-TEMP}` (for darks)
`{TYPE} - {DATE} - {FILTER}` (for flats)
`{TYPE} - {DATE}` (for bias)
Examples: "Dark - 2026-01-15 - -10°C", "Flat - 2026-01-15 - Ha",
"Bias - 2026-01-15"

### 2.6 Archive

A new **top-level navigation item**. Shows archived (rejected) sessions with
the same search, sort, and grouping options as Inbox and Sessions.

Archive shows items from all sources: rejected inbox sessions, archived
calibration, archived projects.

From Archive, users can:
- **Re-queue** — move a session back to Inbox for re-review
- **Delete / clean up** — remove files (subject to reviewable filesystem plan
  per Constitution principle II)

---

## 3. Sessions

Confirmed light frame sessions. Data is **locked** — properties are read-only
unless the session is moved back to Inbox.

### 3.1 Left Sidebar — Session List

Same search, sort, filter, and grouping controls as Inbox. This pattern is the
**standard for all list views** across the app.

- Text search
- Group by (target, date, filter, optical train, camera, etc.)
- Sort by (date, name, frame count, integration time, etc.)
- Filter by state, filter, optical train — dropdowns positioned below group/sort
  for visual consistency
- State pills organized cleanly (not scattered)

Each session entry shows:

- **Line 1**: Target / Object name
- **Line 2**: Date / night
- **Line 3**: Filter
- **Line 4**: State pill badge

No confidence indicators or metadata provenance in the list.

### 3.2 Calendar View

Three view modes available: **List**, **Calendar Grid**, **Calendar Scroll**.

**Calendar Grid** (existing, improved):
- Single month grid as today, but sessions on a given date are more visually
  prominent (colored badges or blocks, not faint text).
- Hovering over a session shows a tooltip with key details (target, filter,
  frame count, integration time).

**Calendar Scroll** (new):
- Vertical scrolling timeline through months. Each month header is sticky.
- Sessions appear as cards on their dates, showing target + filter + frame
  count.
- Scrolling down moves backward through time.
- More space per entry than the grid — better for seeing what happened when.

**List** (existing):
- Standard list view as described in 3.1.

### 3.3 Session Detail — Main Content

Same unified property table layout as Inbox (section 2.2), but **read-only**.

- No split columns ("Session key" / "Equipment & site" removed — single table).
- No provenance summary.
- No "confirmed" badges on fields — the session is already locked, this is
  redundant.
- No "immutable" text — wasted space.
- No acquisition summary section (frame count, total integration, etc. as a
  separate block) — these values belong in the property table or header.

**Header**: session name, state badge, frame count + total integration time.
Top bar shows only relevant session-level data. No hotkeys in the top bar —
hotkeys belong on their buttons.

**Property table**: same fields as Inbox (section 2.2), displayed read-only.

**Frames list**: expandable file list. Selecting a frame shows extended FITS
metadata for that file. Per-frame status column removed (not useful on
confirmed sessions).

### 3.4 Top Action Bar & Main Content Sections

Sessions uses the **top action bar** model (no right sidebar). This applies to
all detail screens except Inbox and Projects (see section 5.1 for the hybrid
layout rationale).

**Top action bar** (consistent button sizing, hotkeys shown):

| Action | Behavior |
|--------|----------|
| Use in Project | Add session to a project |
| Move to Inbox | Returns session to Inbox for re-review. **Disabled** if the session is used in any project — tooltip explains why |
| Reveal in Explorer | Open containing folder in native OS file manager |
| Archive | Move to Archive |

No split, merge, confirm, reject, edit, or "new session / dupe in project"
actions.

**Main content sections** (below property table and frames list):

- **Calibration matches** — read-only display of matched calibration frames
  (darks, flats, bias). Informational only; actual calibration selection
  happens in the project context.
- **Project membership** — list of projects this session is used in (if any).
- **Notes** — free-text notes field.

### 3.5 Removed Elements

- "Target" link in sidebar (sessions are not linked to targets; target is a
  metadata field used for project matching)
- Provenance section
- "Needs reviewed provenance" language
- Hotkeys in the top bar for irrelevant actions (new session, dupe in project)
- Confidence indicators anywhere

---

## 4. Calibration

Confirmed calibration frames (darks, flats, bias) — both individual sub-frames
and masters. Data is **locked** after confirmation from Inbox.

### 4.1 Left Sidebar — Calibration List

Standard list controls (consistent with Inbox, Sessions, and all other list
views): text search, group by, sort by, filter by type/state.

Default grouping: by calibration type (Dark / Flat / Bias). Within each type,
entries are grouped into:

- **Masters** — stacked/integrated calibration frames. Distinguished from raw
  subs via FITS headers where possible (`IMAGETYP` = `master dark` etc.,
  `BITPIX` 32-bit float, processing tool `HISTORY`/`PROGRAM` keywords). Shown
  with a "Master" badge.
- **Sub-frame sets** — individual calibration subs grouped by matching
  fingerprint (temperature + exposure for darks, filter + optical train for
  flats, gain + camera for bias).

Each entry shows:

- **Line 1**: Type + identifying property (e.g., "Dark -10°C 300s",
  "Flat Ha AT106", "Bias ASI2600")
- **Line 2**: Date / night
- **Line 3**: Frame count (for sets) or "Master" badge
- **Line 4**: State pill badge

No confidence indicators.

### 4.2 Calibration Detail — Main Content

#### Header

Calibration name, type badge (Dark/Flat/Bias), master/sub badge, frame count,
state badge.

#### Matching Fingerprint (highlighted section)

Visually distinct section at the top of the detail area — the matching key that
determines what sessions this calibration is compatible with. Uses the same
field layout style as the property table but grouped under a clear heading.

Fields vary by type:

| Field | Dark | Flat | Bias |
|-------|------|------|------|
| Frame type | x | x | x |
| Exposure | x | x | |
| Set temperature | x | | |
| Gain | x | x | x |
| Offset | x | x | x |
| Binning | x | x | x |
| Camera | x | x | x |
| Sensor mode | x | x | x |
| Filter | | x | |
| Optical train | | x | |

#### Provenance Data

Not labelled "Provenance" — integrated into the detail as regular data fields:

| Field | Notes |
|-------|-------|
| File hash | Content hash for identity |
| Created date | From FITS DATE or filesystem |
| Created in | Processing tool if detectable from FITS headers |
| Source session | Link to the calibration sub-frame set (for masters) |
| Age | Calculated from creation date |

**Aging badge**: shown when age exceeds **1 year** (configurable in Settings).
Warning only — does not block usage.

#### Compatible Sessions

Binary match display — sessions either match or they don't, based on the
fingerprint tolerance rules (temperature within configurable tolerance, etc.).
No match scores. List of matching sessions with basic info (target, date,
filter).

#### Linked to Projects

Main content table showing projects that use this calibration:

| Column | Content |
|--------|---------|
| Project | Project name (linked) |
| Workflow profile | e.g., PixInsight/WBPP |
| Lifecycle | Project lifecycle state |
| Role | e.g., dark (lights), dark (flats) |
| Selected by | Auto-match or user override |
| Selected at | Date of association |

#### Frames List

For sub-frame sets: expandable list of individual files. Selecting a frame
shows extended FITS metadata.

For masters: single file with full FITS metadata display.

### 4.3 Top Action Bar & Main Content Sections

Calibration uses the **top action bar** model (no right sidebar), consistent
with Sessions and Targets.

**Top action bar** (same-width buttons, hotkeys shown):

| Action | Behavior |
|--------|----------|
| Use in Project | Associate with a project |
| Reveal in Explorer | Open containing folder in native OS file manager |
| Archive | Move to Archive |

No "Import master", "Re-run matching", "Mark superseded", or "Match
candidates" buttons.

**Main content sections** (below frames list):

- **Project membership** — which projects use this calibration
- **Notes** — free-text notes field

### 4.4 Removed Elements

- Masters / Calibration sessions / Match candidates tabs (all content shown
  in the unified list sidebar)
- Import master button
- Re-run matching button (matching is always live)
- Mark superseded (use Archive instead)
- Match scores and soft mismatches (binary match only)
- 90-day aging threshold (changed to 1 year)
- Provenance as a separate labelled section

### 4.5 Retroactive Addition — Reveal in Explorer

"Reveal in Explorer" is added as a standard action on **all** detail views
backed by files: Inbox (section 2.3), Sessions (section 3.4), and Calibration
(this section).

---

## 5. Targets

Collection/overview screen showing which sessions and projects relate to which
astronomical targets. Targets are catalog entries used for matching FITS OBJECT
headers — sessions are not "linked" to targets as an entity relationship, but
the target field on a session is used to find viable sessions for projects.

### 5.1 Layout Model — Hybrid Revision

Targets uses the **top action bar** model (no right sidebar). This applies to
all detail screens except Inbox:

- **Inbox**: right action sidebar (workflow-heavy, multi-step confirm flow)
- **Projects**: right lifecycle sidebar (phase-specific actions, quick stats)
- **Sessions, Calibration, Targets, Archive**: top action bar with consistent
  button sizing and hotkeys. Contextual info (notes, project membership,
  calibration matches) moves into the main content area as sections.

### 5.2 Left Sidebar — Target List

Standard list controls (consistent with all other list views): text search,
group by, sort by, filter.

**Grouping options:**
- Type (Deep sky, Planetary, Lunar, etc.)
- Constellation
- Catalog (NGC, IC, Messier, Caldwell, etc.)
- Project (has active project / no project)

Each target entry shows:

- **Line 1**: Target name
- **Line 2**: Type label (e.g., "Emission Nebula", "Planetary Nebula")
- **Line 3**: Session count + total integration time
- **Line 4**: Project count

### 5.3 Top Action Bar

Consistent button layout, hotkeys shown:

| Action | Behavior |
|--------|----------|
| Edit aliases | Edit target name aliases |
| Link plan | Associate a NINA observing plan file |
| New project | Create a new project for this target |

### 5.4 Target Detail — Main Content

#### Identity

| Field | Content |
|-------|---------|
| Primary name | e.g., "NGC 7000" |
| Aliases | Alternative names (e.g., "North America Nebula", "Caldwell 20") |
| Catalog IDs | All matching catalog entries |
| Kind | Object classification (emission nebula, galaxy, etc.) |
| RA / DEC | Coordinates |
| Constellation | Containing constellation |

#### Sessions

Table of acquisition sessions associated with this target:

| Column | Content |
|--------|---------|
| Night | Session date |
| Filter | Filter used |
| Frames | Frame count |
| Integration | Total integration time |
| Train | Optical train |
| State | Session state badge |
| Project | Project name(s) — if a session is used in multiple projects, show multiple project names stacked within the cell, each clickable to navigate to that project |

Clicking a session navigates to the session detail.

#### Coverage at a Glance

Integration hours broken down by filter, displayed as a bar chart or similar
visualization.

- **Optical train dropdown** — filters the coverage display to a single optical
  train. Per-train view only (no "all trains" combined view), since different
  trains have different focal lengths/FOV and coverage is not interchangeable.
- No low-coverage warning badges — the chart speaks for itself.

#### Projects

Table of projects that use this target:

| Column | Content |
|--------|---------|
| Project | Project name (clickable) |
| Profile | Workflow profile (e.g., PixInsight/WBPP) |
| Lifecycle | Project lifecycle state |
| Sessions | Session count in project |
| Outputs | Output count |

#### Observing Plans

Linked NINA plan files (.nina). Kept for v1, limited to NINA format only.

- List of linked plan files with filename and date
- "Link plan" action (also in top bar) to associate new plan files

### 5.5 Removed Elements

- Outputs / thumbnail grid (processing outputs are not managed by this app)
- "Open sessions view" button
- Coverage warning badges
- "New target" button at bottom of sidebar (targets come from catalogs and
  FITS metadata, not manual creation)

---

## 6. Projects

Projects are the central workflow unit — they bind light sessions, calibration,
a processing tool profile, source views, and lifecycle state together.

### 6.1 Layout Model

Projects use the **right sidebar** model (same as Inbox). Projects have a
multi-phase lifecycle with phase-specific actions, justifying a persistent
sidebar.

The current three-tab structure (Source Map / Pipeline / Combined) is removed.
A single consolidated view replaces all three.

### 6.2 Left Sidebar — Project List

Standard list controls (consistent with all other list views): text search,
group by, sort by, filter.

Each project entry shows:

- **Line 1**: Project name (user-defined, may span multiple targets/filters)
- **Line 2**: Target(s) summary
- **Line 3**: Integration time + session count
- **Line 4**: Lifecycle state pill

### 6.3 Lifecycle

Simplified to five phases:

```
Setup → Ready → Processing → Completed → Archived
```

"Prepared" is removed — generating source views automatically transitions
from Ready to Processing.

| Phase | Meaning |
|-------|---------|
| Setup | Selecting sessions, calibration, and workflow profile |
| Ready | Sources complete, user can generate views (triggers auto-advance to Processing) |
| Processing | Source views generated, user is working in external tool (PI/Siril/DSS) |
| Completed | User manually marks done. App scans for cleanup opportunities |
| Archived | Project envelope removed. Originals managed via Archive screen |

Transitions:

- Setup → Ready: user marks sources as complete
- Ready → Processing: **automatic** on "Generate views" — views are created,
  lifecycle advances immediately
- Processing → Completed: **manual** — user clicks "Mark complete"
- Completed → Archived: **manual** — moves project to Archive. Deletes project
  envelope (source views, processing temps) only, not original sessions or
  calibration. Original data is managed from the Archive screen independently.

### 6.4 Project Detail — Main Content

#### Header

Project name, lifecycle state badge, workflow profile badge (e.g.,
"PixInsight/WBPP").

#### Pipeline Stats Bar

Compact single-row summary below the header:

```
Sources: 3 lights, 4 cal  |  Views: 82 files  |  On disk: 8.4 GB  |  Outputs: 1 verified
```

Not a flow chart — a factual data summary.

#### Source Map

Column layout (kept from current UI — this works well):

| Lights | Darks | Flats | Bias |
|--------|-------|-------|------|

Each column shows assigned sessions/calibration as cards with date, filter,
frame count, and integration time. Calibration columns show both masters and
sub-frame sets where applicable.

Adding/removing sessions and calibration is a lifecycle-gated action (see
sidebar, section 6.5).

#### Source Views

Compact status display (not a file-level listing):

- **Status**: not generated / generated / error
- **File counts**: per-section breakdown (e.g., "Lights: 54, Darks: 50,
  Flats: 15, Bias: 30")
- **Path**: location of the generated folder
- **"Reveal in Explorer"** action to open the folder

No individual symlink/junction details. No function/path listings.

#### Notes

Inline markdown editor. Users can create and edit notes directly within the
project view. Notes are user-generated free-text documentation about the
project (processing decisions, issues encountered, results).

- Create new note button
- List of existing notes, each expandable to inline edit
- Markdown rendering with edit toggle

#### Cleanup (also accessible as sidebar action)

Shown after Completed phase. Automatic scan on completion, can also be
triggered manually.

Shows cleanup opportunities as a reviewable plan:

- **Intermediary artifacts**: drizzle integration files, rejection maps, local
  normalization data, process logs, temporary processing folder contents
- **Calibration sub-frames**: individual dark/flat/bias subs where a master
  has been created and verified. Clearly shows: "50 dark subs used to create
  MasterDark_300s_-10C.xisf — safe to remove?"
- **Per-item**: file path, size, reason it's cleanup-eligible
- **Total**: disk space recoverable

User reviews and approves the plan before any deletion (Constitution
principle II — reviewable filesystem mutation).

### 6.5 Right Sidebar

Lifecycle-contextual. Shows different actions depending on the current phase.

**Lifecycle state** — current phase badge + compact state visualization
(horizontal pill strip showing all phases with current highlighted).

**Phase-specific actions** (same-width buttons, hotkeys shown):

| Phase | Actions |
|-------|---------|
| Setup | Add session, Add calibration, Select profile, Mark sources complete |
| Ready | Generate views (auto-advances to Processing) |
| Processing | Reveal source views, Mark complete |
| Completed | Plan cleanup, Archive |
| Archived | (managed from Archive screen) |

**Actions available in all phases:**

| Action | Behavior |
|--------|----------|
| Reveal in Explorer | Open project folder in native OS file manager |
| Edit project | Edit name, profile (profile locked after Setup) |

**Quick stats** (compact, below actions):

- Total integration time
- On-disk size
- Session count (lights + calibration)
- Output count

### 6.6 Removed Elements

- Three-tab structure (Source Map / Pipeline / Combined)
- Command center tab
- Observe artifacts button
- Record output button
- "Prepared" lifecycle phase
- Manifests display (internal, not user-facing)
- Source view file-level details (symlink paths, function names)
- Right sidebar "Quick stats" as a sprawling block (now compact)
- Duplicate lifecycle blocks across tabs
- Import master button
- Re-run calibration matching button (matching is live)
- Pipeline flow chart visualization (replaced by stats bar)

---

## 7. Plans — Removed

Plans is **dropped as a top-level nav item**. Filesystem plans are shown inline
where they are triggered:

- Inbox confirmation overlay (section 2.4) — shows where files will be moved
- Project cleanup section (section 6.4) — shows cleanup-eligible files
- Archive delete flow — shows what will be deleted

Plan execution events appear in the Audit Log with a summary of what the plan
did. No separate plan detail view.

---

## 8. Audit Log — Moved to Settings

The Audit Log is moved from a top-level nav item into **Settings** as a pane
(see section 9). It is a diagnostic/admin tool, not a daily workflow screen.

### 8.1 Layout

Single-panel layout within the Settings screen (no right sidebar — entity
context is displayed inline within the event detail).

**Left sidebar / event list:**

- Search events (text)
- Filter: outcome (all / ok / applied / error)
- Filter: actor (all / user / system)
- Filter: event type (all / session.confirmed / plan.applied / scan.completed / etc.)
- Filter: **date range** (start date + end date pickers)
- **Pagination** for large event lists
- Sort: newest first (default) / oldest first

**Main content — event detail:**

Selecting an event shows:

| Section | Content |
|---------|---------|
| Header | Event name + outcome badge + timestamp |
| Entity | Type + ID (clickable link to the entity if it still exists) |
| State change | From → To state badges (if applicable) |
| Actor | User or System |
| Detail | Human-readable description of what happened |
| Plan summary | If this was a plan execution: what files were moved/deleted/created (inline, not a separate view) |
| Entity context | Entity summary (type, ID, total events, first/last seen) + transition history — rolled into the main content, not a separate panel |
| Related events | Other events for the same entity, shown as a compact list |

### 8.2 Removed Elements

- Top-level nav item (moved to Settings)
- Right panel (entity context merged into main content)
- Plan detail links (plan summary shown inline)

### 8.3 Durable Coverage & Unification (Q15 / #647)

*(Added by iteration 2026-07-14; decisions locked by
`docs/development/ui-campaign-grilling-decisions-2026-07-13.md` §Q15.)*

Issue #647 is architectural, not a coverage gap. Two disjoint audit stores
exist: the live event bus (hybrid: in-process broadcast for live UI plus a
durable `events` topic stream — a topic+payload log with no
outcome/refused semantics, not an audit record; settings changes,
protection sets, equipment CRUD, `sources.set_active`, and rescans emit
here only) and the durable `audit_log_entry` table (written only by
lifecycle transitions, plan-apply, and project-health). A protection-set
therefore returns an `auditId` that points at a bus event no audit read
can resolve, not an `audit_log_entry` row — violating constitution §II
("audit record for each attempted action and outcome").

**Store roles:** `audit_log_entry` is the authoritative audit record; the
`events` table is non-authoritative transient diagnostics and may be
pruned. Audit-worthy mutations therefore produce two durable rows (an
`audit_log_entry` row plus an `events` row via the bus) — accepted in v1;
any change to `events`-table persistence is deferred to the Q9 log-panel
iteration.

**Unification (FR-130–FR-134):**

1. **Coverage** — every attempted mutation of durable state or user data
   writes a durable audit row: settings changes, protection overrides,
   equipment CRUD, source enable/disable/register/delete (the Q5 delete
   cascade), rescans/root operations. Each row records the outcome,
   including refused/failed with a reason/code.
2. **Single source of truth** — the durable `audit_log_entry` table.
   Audit-worthy actions write the durable row *and* emit to the bus for
   live UI; the emit-to-bus-only pattern for mutations is eliminated. The
   Activity/log panel (Q9) reads durable audit for user-meaningful events
   plus the live event bus for transient/internal noise — making "activity
   is a view over the audit" literally true, and the Q10 manifest-history
   reframe viable.
3. **Generalized entry shape** — from a lifecycle-transition record to a
   generic mutation record: timestamp, actor, action, entity (type + id),
   outcome + reason, plus an optional before→after pair for
   settings/protection changes.

**The line (NOT durably audited):** reads, navigation, UI state, transient
internal/periodic events.

---

## 9. Settings

Settings is reorganized into logical panes. Layout and styling must be
consistent with the rest of the app. Each pane uses standard form controls
(tables, dropdowns, toggles, text inputs) — no ad-hoc layouts.

### 9.1 Pane Structure

| Pane | Content |
|------|---------|
| Data Sources | Registered source folders, add/remove/modify |
| Equipment | Optical trains, cameras, telescopes, filter library |
| Ingestion | Inbox watcher behavior, review defaults, session grouping tolerances |
| Naming & Structure | Token pattern builder, per-frame-type overrides, preview |
| Processing Tools | Tool toggles, executable paths, per-tool directory structure |
| Calibration Matching | Matching tolerances (temperature, exposure, etc.) |
| Target Catalogs | Catalog list with enable/disable toggles, download management |
| Cleanup | Cleanup policy per frame type, manual vs auto trigger |
| General | Theme, layout preferences, appearance |
| Advanced | Application log, debug settings |
| Audit Log | Event history (moved from top-level nav, see section 8) |

### 9.2 Data Sources

Registered source folders in a clean **table layout**:

| Column | Content |
|--------|---------|
| Path | Folder path |
| Type | Light Frames / Dark / Flat / Bias / Project / Inbox |
| Status | Active / missing / warning |
| Actions | Edit, Remove, Reveal in Explorer |

- **Add Folder** button (not greyed out) — opens native OS folder picker, then
  type selector (same flow as wizard step 1).
- Users can **remove** and **modify** registered roots (change path, change
  type).
- No "What happens to new files in the inbox" text — that belongs in
  Ingestion.
- No "Rescan" button here — rescanning is an Ingestion concern.
- No scan defaults (follow symlinks, hashing, etc.) — moved to Ingestion.
- No "Restart setup wizard" button — Settings provides full CRUD.
- Consistent font and styling throughout the table.

### 9.3 Equipment

New pane consolidating equipment management. Moved from Calibration Matching
(where "Detected Equipment" currently lives).

#### Optical Trains

Inline-editable table. Each row is an optical train. Adding a new row opens
inline fields with dropdowns to select from registered equipment or add custom:

| Column | Input |
|--------|-------|
| Name | Auto-generated from telescope + camera, or user-defined |
| Telescope | Dropdown from registered telescopes, or type custom |
| Camera | Dropdown from registered cameras, or type custom |
| Focal length | Numeric (may be computed from telescope) |
| Actions | Remove |

"Add optical train" inserts a new editable row at the bottom.

Equipment detected from FITS headers is auto-populated and available in
dropdowns.

#### Cameras

Table of detected + user-added cameras. Auto-populated from FITS `INSTRUME`
headers.

#### Telescopes

Table of detected + user-added telescopes. Auto-populated from FITS headers.

No mounts — mount is not relevant for calibration matching or session
grouping.

#### Filter Library

Table of filters with category grouping:

| Category | Filters |
|----------|---------|
| Narrowband | Ha, SII, OIII, NII |
| Broadband | L, R, G, B |
| Dual-band | HO, SO |
| Other | UV/IR Cut |
| Custom | User-defined |

Filters are auto-added as detected from FITS metadata. Users can add custom
filters and organize into categories.

### 9.4 Ingestion

Inbox watcher and session review configuration.

- **Filesystem watcher** toggle (live detection on/off)
- **Scan on startup** toggle
- **Rescan now** button (moved from Data Sources)
- **Scan defaults** (moved from Data Sources):
  - Follow symlinks: toggle (default off)
  - Follow junctions: toggle (default off)
  - Hashing mode: dropdown (lazy recommended)
  - Metadata extraction: dropdown (FITS + XISF + sidecar)
- **Session grouping tolerances**:
  - Exposure time margin: default 2s (configurable)
  - Temperature tolerance: default 5°C (configurable)
- **Default filter** for unfiltered/OSC frames: default "L"

### 9.5 Naming & Structure

Token pattern builder for file/folder naming.

#### Global Pattern

Visual pattern builder showing available tokens. Tokens are **draggable** to
reorder (must actually work — currently non-functional).

**Token insertion** — the "+ Token" and "+ Separator" buttons open a
**dropdown** listing all available tokens with descriptions. Selecting a token
inserts it into the pattern at the current position.

| Token | Description | Example |
|-------|-------------|---------|
| `{object}` | Target name | M31 |
| `{date}` | Session date | 2026-01-15 |
| `{filter}` | Filter name | Ha |
| `{exposure}` | Exposure time | 300s |
| `{camera}` | Camera name | ASI2600MM |
| `{train}` | Optical train | AT106-EDT |
| `{gain}` | Gain value | 100 |
| `{binning}` | Binning | 1x1 |
| `{temp}` | Set temperature | -10C |
| `{type}` | Frame type | Light |
| ... | (complete list) | |

#### Per-Frame-Type Overrides

Separate pattern configuration for each frame type (Light, Dark, Flat, Bias).
No "Dark flat" type. Each override shows a **live preview** using recent FITS
data to demonstrate the resulting path.

**Default patterns differ per type** — calibration types should not use
tokens that don't apply (e.g., darks have no `{object}`):

| Type | Default pattern |
|------|----------------|
| Light | `{object}/{filter}/{date}/lights/` |
| Dark | `{date}/{type}/{exposure}_{temp}/` |
| Flat | `{filter}/{date}/{type}/` |
| Bias | `{date}/{type}/` |

#### Source View Strategy

Simplified to a **dropdown** selecting the strategy with a brief clarification:

| Strategy | Description |
|----------|-------------|
| NTFS junctions | Directory junctions on Windows. WBPP-friendly, no admin. Default on Windows |
| Symbolic links | POSIX symlinks. Default on macOS/Linux. May need admin on Windows |
| Hard links | Same-volume only. Identical inode |
| Full copy | Duplicate every file. Use only for portable workflows |

No manifest-only option. No hybrid option.

#### Removed

- Manifest-only strategy
- Hybrid strategy
- Per-platform overrides (app runs on one platform with its own config)
- Default conflict policy (needs more thought — deferred or clarified inline)
- Large comparison table (replaced by dropdown + description)

### 9.6 Processing Tools

Tool configuration and directory structure.

Per tool (**PixInsight and Siril only** — planetary tools hidden,
DeepSkyStacker dropped for now):

- **Enable/disable** toggle
- **Executable path** — file browser labelled **"Choose executable"** (not
  "Choose folder"). Immediate path validation.
- **Directory structure template** — per-tool configuration of which folders
  are created, what they're named, and what goes in each folder. This defines
  the source view output structure for that tool's workflow profile. Defaults
  are based on vendor best practices (e.g., WBPP expected folder layout) but
  users can rename folders.
- **Processing directory** — where temporary processing files are stored
  (moved from Cleanup & Archive). Default: `processing/` relative to project
  root.
- **Output directory** — where final outputs are written. Default: `outputs/`
  relative to project root.

### 9.7 Calibration Matching

Matching tolerance configuration.

| Setting | Default | Notes |
|---------|---------|-------|
| Temperature tolerance | 5°C | How close set-temp must be for dark matching |
| Exposure time tolerance | 2s | For matching darks to lights |
| Aging threshold | 1 year | Warning badge on old calibration |
| Require same gain | Yes | Toggle |
| Require same binning | Yes | Toggle |

Layout must be consistent with the rest of Settings — standard form controls,
no ad-hoc card layouts.

### 9.8 Target Catalogs

Table of available catalogs:

| Column | Content |
|--------|---------|
| Catalog | Name |
| Description | Brief description of the catalog |
| Status | Downloaded / not downloaded / last synced date |
| Enabled | Toggle — enable/disable for matching |
| Entries | Number of objects in catalog |
| Actions | Sync, Remove |

**Available catalogs**: Messier, NGC/IC, Caldwell, Sharpless (Sh2), Abell
Planetary Nebulae. More can be added in future updates.

**Download All** button at the top.

### 9.9 Cleanup

Cleanup policy configuration.

#### Per-Type Cleanup Action

Simple table — one row per data type, one action column:

| Data Type | Action |
|-----------|--------|
| Light frames | Keep / Archive / Delete |
| Dark subs | Keep / Archive / Delete |
| Flat subs | Keep / Archive / Delete |
| Bias subs | Keep / Archive / Delete |
| Calibration masters | Keep / Archive / Delete |
| Registered frames | Keep / Archive / Delete |
| Calibrated frames | Keep / Archive / Delete |
| Debayered frames | Keep / Archive / Delete |
| Local normalization | Keep / Archive / Delete |
| Drizzle data | Keep / Archive / Delete |
| Integration cache | Keep / Archive / Delete |
| Stack output (intermediate) | Keep / Archive / Delete |
| Temporary files | Keep / Archive / Delete |
| Process logs | Keep / Archive / Delete |
| Process icons / tool config | Keep / Archive / Delete |

No per-tool breakdown (current per-tool tables make no sense). No trigger
column. No shared category (rolled into standard data types above). No file
type protection (intermediary .fits/.xisf files must be removable). No
category protection checkboxes.

#### When Cleanup Runs

- **Manual only** — user triggers cleanup from project sidebar
- **Propose on project completion** — app suggests cleanup when a project is
  marked Completed

Toggle between the two modes.

#### Removed

- Source protection age threshold (irrelevant — processing may happen same day)
- File type protection for intermediary files (these should be removable)
- Category protection (unnecessary)
- Approval requirements (removed)
- Policy matrix per-tool breakdown
- Shared category

### 9.10 General

Theme and visual preferences. Clean layout. Renamed from "Appearance."

- Theme selection (light/dark/system)
- Font size
- Density (compact/comfortable/spacious)

**Removed**: guided tour does not belong here. If a tour exists, it should be
accessible from a help menu or first-run experience.

### 9.11 Advanced

Diagnostic and debug tools.

- **Application log** — merged here from its own pane. Scrollable log viewer
  with filtering and search.
- **Database** — location, size, reset options
- **Debug mode** toggle
- **Export diagnostics** — for support/troubleshooting

### 9.12 Audit Log

See section 8 for full specification. Accessed as a Settings pane.

---

## 10. Navigation Structure — Revised

The main navigation sidebar is simplified to six workflow screens plus
Settings:

| Nav Item | Purpose |
|----------|---------|
| **Inbox** | Review detected sessions from inbox folders |
| **Sessions** | Confirmed light frame sessions |
| **Calibration** | Confirmed calibration (masters + subs) |
| **Targets** | Target catalog collection view |
| **Projects** | Processing project workflow |
| **Archive** | Archived/rejected items from all sources |
| **Settings** | Configuration + Audit Log |

**Removed from nav:**
- Review Queue (renamed to Inbox)
- Plans (dropped — inline in workflows)
- Audit Log (moved to Settings)

---

## 11. Status Bar & Sidebar Footer

### 11.1 Status Bar (bottom of window)

Glanceable operational status. Left-to-right layout:

| Position | Content | Behavior |
|----------|---------|----------|
| Left | **Ingestion progress** (when active): "Ingesting 12 files..." with progress bar. When idle: **Inbox badge** "5 awaiting review" | Ingestion progress only shown during active ingestion. Inbox badge clickable → navigates to Inbox. Hidden when inbox is empty |
| Center | **Library stats**: "1,247 files · 48.2 GB" | Static summary of indexed library |
| Center-right | **Cleanup available**: "2.1 GB reclaimable" | Shown when any completed projects have cleanup opportunities. Clickable → navigates to the first project with reclaimable space |
| Right | **Storage health**: free space per volume where roots are registered, e.g., "D: 284 GB free · E: 1.2 TB free" | Warning color (amber/red) when a volume drops below **10% free** (configurable in Settings). Normal color otherwise |

**Removed from status bar:**
- Directory path (not actionable)
- "Idle" status (too vague)
- "Last scan: 2h ago" (misleading — watcher is live)

### 11.2 Sidebar Footer

Below the main navigation list.

- **Root health indicator**: colored dot + compact text
  - All online: green dot, "N roots online"
  - Some offline: amber dot, "NAS-Astro offline" (names the specific offline root)
  - All offline: red dot, "All roots offline"
- Clickable → navigates to Data Sources settings

---

## 12. Missing-Value Semantics & Detail-as-Delta (Q16 / #620, #619)

*(Added by iteration 2026-07-14; decisions locked by
`docs/development/ui-campaign-grilling-decisions-2026-07-13.md` §Q16.)*

### 12.1 Three Value States, Fixed at the Model

Issue #620 is a data-model problem wearing a rendering costume. The shared
property renderer displays missing values as an em-dash, but by then the
model has already lost information: the em-dash still receives a "FITS"
source pill (absence rendered as attributed data), and a metadata-less
calibration master shows "Gain 0 · Exposure 0s · Size 0 KB" — default
zeros indistinguishable from a real Gain 0. Missing ≠ zero, and the render
layer cannot recover the distinction once 0 overwrote it.

Three states MUST be distinguishable everywhere a metadata value appears
(FR-135):

| State | Meaning | Example |
|-------|---------|---------|
| Real value | Data exists — including a real `0` | Gain 0 measured from the FITS header |
| Unresolved / missing | The field applies but no data exists | Master without gain metadata |
| Not-applicable | The field does not apply to this entity | Filter on a dark; set-temp on flats/bias (§2.2) |

Not-applicable is determined by the entity/frame-type model (which fields
apply to which entity kind), never inferred from data absence. The
authoritative per-entity × per-field applicability matrix lives in
data-model.md ("Metadata Value States").

A "Default" source pill denotes a modeled/configured fallback recorded
with its provenance in the data model (e.g., the configured default filter
"L" for OSC frames, §9.4). Values synthesized during DTO mapping (e.g., a
fabricated "1x1" binning) are never Default — they are prohibited
sentinels under FR-136.

**End-to-end null rule (FR-136)**: missing is represented as null/None at
every hop — extraction → persistence → application layer → contract → UI.
Defaulting a numeric to 0 (or any sentinel) at any hop is prohibited.
Contract DTO fields whose values can be absent at extraction are nullable.
The failure chain this repairs, concretely: the extraction model is
`Option`-typed and the persistence row keeps nullable fingerprint fields,
but the application layer collapses them (`unwrap_or(0.0)` on
exposure/gain in calibration matching) and the contract cannot carry
absence (`CalibrationFingerprint.exposure_s`/`gain` are non-optional), so
UI null-checks are dead code.

### 12.2 One Shared Renderer

One shared `renderValue(value, {source, applicability})` is the single
rendering path for metadata field values (FR-137), across Inbox, Sessions,
Calibration, Targets, Projects, Archive — everywhere. The `applicability`
input comes from the field-applicability matrix (data-model.md); the
renderer cannot derive not-applicable from `value` + `source` alone:

| State | Rendering |
|-------|-----------|
| Real value | The value, plus its source pill (FITS / User / Inferred / Default) |
| Unresolved | A distinct muted "unresolved" chip — no source pill, never 0 |
| Not-applicable | Blank / "—" without any chip |

Source/provenance pills couple to value presence (FR-138): absence is
never attributed to a source.

Composed identifying strings (list meta lines, card titles, header
subtitles) cannot host a chip; they omit missing tokens entirely — no
sentinel, no placeholder — as the calibration list meta line already does
(`CalibrationPage.tsx:114-115` filters absent tokens before joining).
SC-010's 100% scope covers field-value renderings: table cells, property
rows, and meta lines.

### 12.3 Detail-as-Delta (#619)

A detail panel adds information; it does not echo the row's columns back
(FR-139, FR-140):

- **Lead with what's new**: full metadata, provenance/source, related
  entities, history, actions.
- **Stay minimal and curated** — only relevant data for the entity, not a
  dump of every available field.
- A small identifying summary of the selected row is fine, but it must not
  dominate the panel.

---
