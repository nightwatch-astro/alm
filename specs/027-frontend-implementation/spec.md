# Feature Specification: Desktop Frontend Implementation

> **Superseded**: All UI layout, navigation, and component design in this spec
> is superseded by
> [Spec 032 — Design V4](../032-design-v4-implementation/spec.md), the current UI
> truth (reached via Spec 031 Design V3). Spec 030, cited here as the successor,
> is itself superseded by 031/032. This spec is reference only.

**Feature Branch**: `027-frontend-implementation`

**Created**: 2026-05-24

**Status**: Implemented

**Input**: User description: "End-to-end implementation of the Astro Library Manager React frontend matching all wireframes from the Claude Design canvas export and DESIGN.md contracts."

**Source of Truth**: `/DESIGN.md` (canvas wireframe export) + `docs/design/canvas-wireframes-2026-05-24/` (15 wireframe JSX files + shell primitives + annotation notes)

**Supersedes**: Spec 001 UI prototype, Spec 022 visual design system (technical infrastructure inherited)

**Design Decisions**: 19 grill-me decisions documented in pre-spec review session (2026-05-24)

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Navigate and Browse Sessions (Priority: P1)

As an astrophotographer with a scanned library, I open the app and land on the Sessions page showing all my acquisition sessions in a sortable table. I can group sessions by target, month, filter, or optical train without leaving the page. I can switch to a calendar view to spot imaging gaps. I can click any session to see its full detail (metadata with provenance, framesets, calibration matches, linked projects, history).

**Why this priority**: Sessions is the primary working surface — the app's home page. Every other workflow starts from or returns to sessions. Without this, the app has no useful landing experience.

**Independent Test**: Can be tested by loading mock session data and verifying the table renders with all columns, group-by changes the visual grouping without navigating, calendar shows sessions on correct nights, and session detail displays metadata with provenance glyphs.

**Acceptance Scenarios**:

1. **Given** a library with 247 sessions, **When** user opens the app, **Then** the Sessions page displays with all sessions in a sortable date-descending table showing target, filter, night, frames, integration, train, state, confidence, and projects columns.
2. **Given** the sessions list, **When** user selects "group by target", **Then** sessions visually collapse into target-grouped sections with per-group integration totals and project pills on group headers.
3. **Given** the sessions list, **When** user clicks "Calendar", **Then** a 3-month calendar grid renders showing session cards on the nights they were captured, with empty cells making gaps visible.
4. **Given** a session row, **When** user clicks it, **Then** session detail opens with tabbed content (Overview/Framesets/Cal matches/Projects/History), metadata KV pairs with inline provenance glyphs (●◐○◇▢▣), and confidence indicators.

---

### User Story 2 - App Shell and Navigation (Priority: P1)

As a user, I see a desktop application frame with a collapsible sidebar listing all navigation destinations (Review queue, Sessions, Calibration, Targets, Projects, Plans, Audit log, Settings). The sidebar shows badge counts where relevant. A status bar at the bottom shows app state and expands into a log panel for long-running operations. A command palette (Cmd+K) lets me search and navigate anywhere.

**Why this priority**: The shell is the container for everything. Without it, no page can render in context.

**Independent Test**: Can be tested by verifying the sidebar renders all nav items, collapse/expand persists, status bar shows idle state, command palette opens on Cmd+K and filters results, and the log panel expands/collapses.

**Acceptance Scenarios**:

1. **Given** the app is running, **When** user views the shell, **Then** a title bar (28px), collapsible sidebar (184px expanded / 44px collapsed), main content area, and status bar (22px) are visible.
2. **Given** the sidebar is expanded, **When** user clicks the collapse button, **Then** sidebar collapses to 44px icon rail with single-letter glyphs, and the state persists across sessions.
3. **Given** the status bar shows "Idle", **When** user clicks it, **Then** the log panel expands showing active operations, progress bars, recent events, and warnings.
4. **Given** any page, **When** user presses Cmd+K, **Then** the command palette opens showing navigation targets, recent sessions/targets/projects, and common actions.
5. **Given** the Review queue has 48 items, **Then** the sidebar shows "48" badge next to Review queue.

---

### User Story 3 - Review Sessions and Unclassified Files (Priority: P1)

As a user with newly scanned data, I open the Review queue to see sessions needing attention. The three-pane layout shows a queue list (sorted by confidence ascending), a focused session with evidence of why it needs review, and a decision panel. I can also filter to see unclassified files that couldn't be grouped into sessions. Keyboard shortcuts (Cmd+1 confirm, Cmd+2 reject, Cmd+3 skip, J/K navigation) let me work efficiently.

**Why this priority**: The review workflow is how raw scanned data becomes confirmed, usable sessions. Without it, the library can't progress past "discovered" state.

**Independent Test**: Can be tested by loading sessions in needs_review state, verifying three-pane layout renders, keyboard shortcuts trigger correct actions, and decisions persist as reviewed metadata.

**Acceptance Scenarios**:

1. **Given** sessions in needs_review state, **When** user opens Review queue, **Then** a three-pane layout shows: queue list (left 220px), evidence pane (center flex), decision panel (right 320px).
2. **Given** a session with missing reviewed observer_location, **When** it's focused in the queue, **Then** a yellow blocking-reason banner shows "observer_location not reviewed" and the Confirm button is disabled.
3. **Given** the review queue, **When** user presses Cmd+1, **Then** the current session transitions to confirmed and the next item in the queue auto-focuses.
4. **Given** the review queue, **When** user selects "Unclassified files" filter, **Then** file-level items that couldn't be grouped into sessions appear in the queue list.

---

### User Story 4 - Create and Manage Projects (Priority: P2)

As a user with confirmed sessions, I create a new project via a 6-step wizard (Name & profile → Sources → Calibration → Source views → Naming & layout → Review plan). The wizard maps sessions to the project, selects per-filter calibration flats + shared darks/bias, generates source views, and produces a filesystem plan for review before creation. The project detail page shows three interchangeable views: Command center (kit grid), Pipeline (horizontal flow), and Combined (both).

**Why this priority**: Projects are the unit of work that prepares data for PixInsight/Siril. Without project creation, the app can't fulfill its core value proposition of organizing data for processing tools.

**Independent Test**: Can be tested by running through the full wizard with mock data, verifying each step renders correctly, the plan review shows correct filesystem operations, and the resulting project detail page displays all three view modes.

**Acceptance Scenarios**:

1. **Given** the Projects page, **When** user clicks "+ New project", **Then** a 6-step wizard opens with a step rail at top and persistent right-rail summary.
2. **Given** wizard step 3 (Calibration), **Then** flats are mapped per filter (each light filter gets its own master flat selection) with shared darks/bias/dark-flats below, scores visible inline.
3. **Given** wizard step 4 (Source views), **Then** the settings default strategy appears as a prominent chip with reasoning; a "Use different strategy" disclosure expands to show radio options.
4. **Given** wizard step 6 (Review plan), **Then** a filesystem plan table shows all items (mkdir, write, junction/symlink), a safety banner, and an "Approve & create" button.
5. **Given** an existing project, **When** user opens its detail page, **Then** a 3-way view toggle (Command center/Pipeline/Combined) appears in the header, persisting the user's choice per project.

---

### User Story 5 - Review and Approve Filesystem Plans (Priority: P2)

As a user with a generated filesystem plan (from project creation, cleanup, archive, or source view operations), I review it in a dedicated plan review page with two toggleable views: Table (one row per operation, sortable/filterable) and Diff (before/after filesystem side-by-side). The approval system uses three tiers: simple approve for safe plans, approve + confirmation for trash/archive, and approve + explicit checkbox for permanent delete.

**Why this priority**: The plan-review-approve-apply pattern is the core safety mechanism of the product. Every filesystem mutation goes through this gate.

**Independent Test**: Can be tested by generating mock plans of varying destructiveness and verifying the table/diff views render correctly, approval gates match plan contents, and permanent delete requires the explicit checkbox.

> **Retired 2026-07-20** (issue #748): the Diff view named in this story and its
> independent test is withdrawn. Plan review ships the Table view only. See the
> FR-045 and FR-047 notes.

**Acceptance Scenarios**:

1. **Given** a plan in ready_for_review state, **When** user opens plan review, **Then** the Table view shows per-row: action pill, source, destination, status, dry-run result, provenance origin.
2. ~~**Given** the plan review page, **When** user clicks "Diff", **Then** a side-by-side before/after filesystem view shows with glyphs (− removed, + added, → archived, ✕ deleted, 🔒 protected).~~ **Retired 2026-07-20** (issue #748) with FR-045/FR-047.
3. **Given** a plan with trash operations, **When** user clicks "Approve", **Then** a confirmation dialog appears before applying.
4. **Given** a plan with permanent delete, **Then** a separate "I understand and accept" checkbox appears below the table, and the Approve button stays disabled until checked.
5. **Given** a plan where any dry-run precondition failed (✕), **Then** the Approve button is disabled with an explanation.

---

### User Story 6 - Browse Targets with Coverage (Priority: P2)

As a user planning imaging sessions, I open the Targets page to see all my astronomical targets in a three-pane layout. The detail pane shows coverage-at-a-glance (filter × hours horizontal bars) — the single most important planning view answering "do I have enough Ha yet?" Sessions and projects linked to the target are visible below.

**Why this priority**: Target-centric planning is a core workflow for deciding what to shoot next and whether existing data is sufficient for a project.

**Independent Test**: Can be tested by loading targets with linked sessions and verifying the three-pane layout, coverage bars render correctly per filter, and the "New project →" button pre-fills target context.

**Acceptance Scenarios**:

1. **Given** targets with linked sessions, **When** user opens Targets, **Then** a three-pane layout shows: target list (left, with session count + integration badges), target detail (right).
2. **Given** a selected target with Ha/OIII/SII sessions, **Then** coverage bars show horizontal bars per filter with hours, and ⚠ warnings when coverage is below recommended thresholds.
3. **Given** a target detail, **When** user clicks "New project →", **Then** the project wizard opens with the target context pre-populated.

---

### User Story 7 - Configure Settings (Priority: P3)

As a user setting up the app or adjusting preferences, I open Settings to configure: data sources (roots with DirPickers), naming patterns (token + separator drag builder with live preview), source view strategy (radio table with per-platform reasoning), cleanup policy (per-tool matrix with processing directory configuration), root recovery, equipment & trains, tools, logs, catalogs, and protection defaults.

**Why this priority**: Settings enable correct app behavior but are not the primary daily-use surface. Most users configure once and revisit rarely.

**Independent Test**: Can be tested by verifying each settings pane renders, DirPickers invoke native OS dialog, token pattern builder produces correct preview output, and cleanup policy matrix reflects per-tool actions.

**Acceptance Scenarios**:

1. **Given** Settings → Data Sources, **Then** a table of roots shows each with DirPicker (folder icon + path + "Choose folder…" button), category pill, online/offline state, file count, and actions.
2. **Given** Settings → Naming & Structure, **Then** a token + separator builder shows draggable token chips (blue, monospace) and separator chips (grey, editable), with 3 live preview examples below.
3. **Given** Settings → Cleanup & Archive, **Then** a processing directory section (DirPickers, default "processing/") appears above a policy matrix (rows = data types, columns = PI/Siril/Planetary) with per-cell action dropdowns.
4. **Given** Settings → Equipment & Trains, **Then** auto-detected equipment from metadata appears with editable aliases and named optical train configurations.

---

### User Story 8 - First-Run Onboarding and Guided Tour (Priority: P3)

As a new user launching the app for the first time, I see a centered 4-step setup wizard (Welcome → Sources → Scan settings → Confirm) that registers my library roots by category. After setup completes and data is scanned, non-blocking overlay hints (anchored to real UI elements) guide me through confirming my first session, creating my first project, and opening it in my processing tool.

**Why this priority**: First-run is a one-time experience. The guided tour helps discovery but the app is usable without it.

**Independent Test**: Can be tested by triggering first-run state (no registered roots), walking through all 4 wizard steps, then verifying overlay hints appear at the correct anchor points after initial scan.

**Acceptance Scenarios**:

1. **Given** no library roots registered, **When** app launches, **Then** a centered single-column wizard (max 720px) appears with a 4-step rail at top.
2. **Given** the wizard Sources step, **Then** DirPickers (not text inputs) are used for every directory selection, with category labels (Raw/Calibration/Project/Inbox).
3. **Given** wizard completion and initial scan, **When** user lands on Sessions, **Then** a non-blocking overlay hint anchors to the first confirmable session row.
4. **Given** a tour hint is visible, **When** user clicks dismiss, **Then** the hint disappears and the next step's hint appears at the appropriate time.

---

### User Story 9 - Calibration Masters Management (Priority: P3)

As a user managing calibration data, I open the Calibration page to see masters in a three-pane layout grouped by kind (darks/flats/bias). Selecting a master shows its fingerprint (camera/exposure/temp/gain/binning), provenance, usage stats, linked projects, and a table of compatible acquisition sessions with match scores.

**Why this priority**: Calibration management supports project creation but is not the primary daily workflow. Users interact with it during project setup or when masters age.

**Independent Test**: Can be tested by loading calibration masters and verifying three-pane layout, fingerprint card values, aging warnings (>90 days), and compatible sessions table with scores.

**Acceptance Scenarios**:

1. **Given** calibration masters exist, **When** user opens Calibration, **Then** a three-pane layout shows masters grouped by kind in the left list with age badges (e.g., "23d", "180d ⚠").
2. **Given** a selected master, **Then** the detail pane shows fingerprint KV card (with provenance glyphs), provenance card (source session, tool, date), and usage card (session count, project count).
3. **Given** a master older than 90 days, **Then** it shows a ⚠ aging warning in the list.

---

### User Story 10 - Audit Log and History (Priority: P3)

As a user tracking what happened in my library, I open the Audit log to see an immutable append-only record of every action. Each row shows timestamp, event type (dot-notation), entity, state change (from → to), actor (user/system), outcome (applied/refused/failed/paused), and structured details. I can filter by event type, outcome, actor, and date range, and export to JSONL.

**Why this priority**: The audit log is a safety and trust feature — important for confidence but not a daily workflow surface.

**Independent Test**: Can be tested by generating audit events and verifying they appear in the log with correct dot-notation naming, state change display, outcome pills (including "refused" as first-class), filtering, and JSONL export produces valid output.

**Acceptance Scenarios**:

1. **Given** audit events exist, **When** user opens Audit log, **Then** a filterable table shows events with monospace timestamps, dot-notation event names, from→to state, actor, and outcome pills.
2. **Given** a refused transition (e.g., blocked confirm due to unreviewed field), **Then** it appears as a first-class row with `outcome: refused` pill and blocking reason in detail.
3. **Given** audit log with events, **When** user clicks "Export JSONL", **Then** a file downloads with one event per line.

---

### Edge Cases

- What happens when sidebar is collapsed and user navigates to a three-pane page? The three-pane page works with the collapsed icon rail (44px) + list pane (220px). If sidebar was expanded, it remains expanded alongside the list pane (404px total left).
- What happens when a plan item fails mid-application? Plan transitions to "paused" state. Status bar shows warning. User can resume or cancel from plan review page.
- What happens when a library root goes offline? Root shows "offline" state in Data Sources. Badge/warning appears in sidebar footer. Reconnect action available.
- What happens when the Review queue is empty? Review queue nav item shows no badge (but remains visible — never hidden). Page shows an empty state with "All caught up" messaging.
- What happens when command palette returns no results? Shows "No results for [query]" with suggested alternatives (e.g., "Try searching by target name or filter").
- What happens when first-run wizard is closed before completion? App state persists wizard progress. Next launch shows where user left off. No registered roots = wizard appears again.

## Requirements *(mandatory)*

### Functional Requirements

**App Shell & Navigation**

- **FR-001**: App MUST render a desktop window frame with title bar (28px), collapsible sidebar, main content area, and status bar (22px).
- **FR-002**: Sidebar MUST show all navigation destinations: Review queue, Sessions, Calibration, Targets, Projects, Plans, Audit log, Settings. Review queue MUST always be visible (not conditional).
- **FR-003**: Sidebar MUST collapse to a 44px icon rail with single-letter glyphs. Collapse state MUST persist across sessions (stored in local preferences).
- **FR-004**: Sidebar MUST show badge counts on nav items where applicable (Review queue item count, Plans pending count).
- **FR-005**: Status bar MUST show current app state (Idle, scanning, applying plan) and MUST expand into a log panel on click showing active operations with progress bars, recent events, and warnings.
- **FR-006**: App MUST provide a command palette (Cmd+K / Ctrl+K) for global search (sessions, targets, projects by name), page navigation, and common actions (new project, start scan).
- **FR-007**: App MUST support three layout patterns: sidebar (default), three-pane (sidebar + list pane + detail), and centered single-column (for wizards).

**Shared Primitives**

- **FR-008**: App MUST implement these shared components matching DESIGN.md §4 specs: Pill (6 variants: neutral/ghost/ok/warn/danger/info), Confidence (bar + label for 6 levels), Provenance (glyph: ●reviewed/◐inferred/○observed/◇generated/▢planned/▣applied), Lock (🔒 glyph), KV (key-value row with provenance + confidence), Box (bordered card), Section (titled content block), Btn (button with primary/danger/small/active variants), DirPicker (native OS directory picker only — never a text input), FilterBar (chip-based filter display), Toolbar (thin horizontal bar with optional sub-bar).
- **FR-009**: DirPicker MUST invoke the native OS directory picker dialog (Tauri `dialog.open({ directory: true })`). It MUST display: folder icon + read-only path + "Choose folder…" button. No text input, paste, dropdown, or drag-drop.
- **FR-010**: All density-sensitive components MUST respect the global density setting: compact (24px row), comfortable (32px row, default), spacious (40px row).

**Sessions Page**

- **FR-011**: Sessions page MUST display a sortable table with columns: warning glyph, target, filter, night, frames, integration, optical train, state (pill), confidence (bar), projects (pills for each linked project).
- **FR-012**: Sessions MUST support 4 group-by modes (target, month, filter, optical train) selectable via chip row above the table. Group-by changes only visual grouping, not the data.
- **FR-013**: Sessions MUST support a Calendar view toggle showing 3 months of day cells with session cards. Clicking a day MUST filter the list to that night.
- **FR-014**: Sessions MUST show that a session can be linked to multiple projects in the "Projects (re-used)" column.
- **FR-015**: Sessions toolbar MUST provide bulk actions (Confirm, Split, Merge, Use in project) that operate on multi-selected rows (checkbox + Shift-click range select).

**Session Detail**

- **FR-016**: Session detail MUST show tabbed content: Overview, Framesets, Calibration matches, Linked projects, History.
- **FR-017**: Session key metadata (target, filter, binning, gain, night, fingerprint) MUST be read-only after confirmation. Corrections MUST be via "Re-open to review" action.
- **FR-018**: Every metadata value MUST display its provenance glyph inline. Confidence MUST be shown as a separate indicator from review state.
- **FR-019**: Session detail MUST show a provenance summary tile counting reviewed/inferred/observed/missing fields.

**Review Queue**

- **FR-020**: Review queue MUST use a three-pane layout: icon rail (if sidebar collapsed) or full sidebar + queue list (220px) + evidence pane (flex) + decision panel (320px).
- **FR-021**: Review queue MUST support filtering: Sessions only / All items / Unclassified files.
- **FR-022**: Review queue MUST support keyboard shortcuts: Cmd+1 confirm, Cmd+2 reject, Cmd+3 skip, J/K next/prev.
- **FR-023**: When a session cannot be confirmed due to missing reviewed provenance, a blocking-reason banner MUST name the specific field(s) that need review.
- **FR-024**: Decisions MUST persist as reviewed metadata entries without modifying source FITS headers.

**Calibration**

- **FR-025**: Calibration page MUST use a three-pane layout with masters list (grouped by kind: darks/flats/bias) on left, master detail on right.
- **FR-026**: Master detail MUST show: fingerprint card (camera/sensor/exposure/temp/gain/binning with provenance), provenance card (source session, creation date, tool, age), usage card (session count, project count), compatible sessions table (with scores and soft mismatches).
- **FR-027**: Masters older than 90 days MUST show a ⚠ aging warning.

**Targets**

- **FR-028**: Targets page MUST use a three-pane layout with target list (left, showing session count + integration badges) and target detail (right).
- **FR-029**: Target detail MUST show coverage-at-a-glance as horizontal bars per filter showing accumulated hours, with ⚠ warnings when below recommended thresholds.
- **FR-030**: Target detail MUST show linked sessions table and linked projects with lifecycle pills.
- **FR-031**: Target detail MUST provide a "New project →" button that pre-fills target context in the project wizard.

**Projects**

- **FR-032**: Projects list MUST show a table with columns: name, lifecycle pill (7-state palette), verification, integration hours, on-disk size, cleanup eligibility, last updated, workflow profile.
- **FR-033**: Blocked projects MUST stay visible with ⚠ + reason text, never hidden.
- **FR-034**: Projects list footer MUST aggregate: total integration, on-disk size, cleanup-eligible across active projects.
- **FR-035**: Project detail MUST show a 3-way view toggle (Command center / Pipeline / Combined) that persists per project with a global fallback default.
- **FR-036**: Command center view MUST show a source map as a kit grid (4 columns: Lights/Darks/Flats/Bias) with cards for each session or master.
- **FR-037**: Pipeline view MUST show a horizontal flow: Sources → Source views → Processing → Outputs with state pills per stage.
- **FR-038**: Combined view MUST show compact kit grid on top with a visual "feeds into" connector, pipeline strip below, then lifecycle/cleanup/manifests row.

**Project Wizard**

- **FR-039**: Project wizard MUST implement 6 steps: Name & profile → Sources (lights) → Calibration → Source views → Naming & layout → Review plan & create.
- **FR-040**: Wizard MUST show a step rail at top (current filled-black, completed grey ✓, future outlined) and a persistent right-rail summary (selected counts, estimated footprint, coming-up list).
- **FR-041**: Step 3 (Calibration) MUST map flats per filter (each light filter gets its own master flat selection) with shared darks/bias/dark-flats below. Scores and soft mismatches visible inline.
- **FR-042**: Step 4 (Source views) MUST show the settings default strategy as a prominent chip with reasoning text. A "Use different strategy for this project" disclosure MUST expand to reveal the full strategy radio table.
- **FR-043**: Step 6 (Review plan) MUST show the complete filesystem plan with the 3-tier approval gate matching plan contents.
- **FR-044**: Wizard state MUST persist as a draft if closed mid-flow. Resume from projects list.

**Plan Review**

- **FR-045**: Plan review MUST provide Table and Diff views toggleable via header pill segments.

  > **Retired 2026-07-20** (issue #748): the Diff view and its toggle are
  > withdrawn; the Table view is the sole plan-review presentation. The
  > single-view table in `PlanReviewOverlay.tsx` satisfies the intent — see the
  > FR-047 note below for why the table carries strictly more information than
  > the specified diff.

- **FR-046**: Table view MUST show per row: action pill, source, destination, status pill (pending/protected/applied/failed/skipped), dry-run result (✓/✕), provenance origin.
- **FR-047**: Diff view MUST show two-column before/after filesystem with glyphs: − removed (red), + added (green), → archived (yellow), ✕ deleted (red), 🔒 protected (grey).

  > **Retired 2026-07-20** (issue #748): withdrawn together with FR-045. Every
  > `PlanItemAction` variant — `move`, `archive`, `delete`, `link`, `write`
  > (`apps/desktop/src/bindings/index.ts:7111`) — carries both `from` and `to`
  > on `PlanItemDetail_Serialize`, and `PlanReviewOverlay.tsx:369-379` renders
  > Item · Action · From · To · Protection · Link kind · Result · Reason ·
  > Linked. The From/To pair is the per-item before/after; the Action pill and
  > Protection pill carry what the glyph legend encoded. The destination column
  > landed in PR #492. The specified glyph set has no symbol for `link` or
  > `write`, so the retired Diff view would represent fewer actions than the
  > table it was meant to complement.
- **FR-048**: Summary bar MUST show: item count, reclaim bytes, trash count, archive count, permanent delete count, protected (skipped) count.
- **FR-049**: Approval MUST implement 3 tiers: (1) simple Approve for non-destructive plans, (2) Approve + confirmation dialog for trash/archive, (3) Approve + separate "I understand and accept" checkbox for permanent delete. Approve disabled until checkbox checked.
- **FR-050**: If any plan item dry-run fails, Approve button MUST be disabled with explanation.

**Artifacts & Outputs**

- **FR-051**: Artifacts page MUST be per-project (not global). Outputs section first, then artifacts grouped by type (registered/calibrated/drizzle/logs/etc.) showing counts and total sizes.
- **FR-052**: Each output row MUST show: filename, kind, size, date, verification pill (accepted/unreviewed/superseded), lock if protected, and a "Verify…" inline action.
- **FR-053**: "Observed, not owned" banner MUST appear reminding that the app doesn't modify processing artifacts.

**Audit Log**

- **FR-054**: Audit log MUST display an immutable table with columns: timestamp (mono), event type (dot-notation), entity, state change (from → to), actor (user/system), outcome (pill: applied/ok/refused/failed/paused), detail.
- **FR-055**: Refused transitions MUST be first-class rows with `outcome: refused` and blocking reason in detail.
- **FR-056**: Audit log MUST support filtering by event type, outcome, actor, date range.
- **FR-057**: Audit log MUST provide "Export JSONL" action producing one event per line.

**Settings**

- **FR-058**: Settings MUST use a left-rail category sidebar with 10 panes: Data Sources, Naming & Structure, Source View Strategy, Cleanup & Archive Policy, Root Recovery, Equipment & Trains, Tools, Logs, Catalogs, Protection.
- **FR-059**: Data Sources MUST show a roots table with DirPickers, category pills, online/offline state, file counts, scan actions.
- **FR-060**: Naming & Structure MUST implement a token + separator drag builder with live preview using recent metadata, per-frame-type override toggles.
- **FR-061**: Cleanup policy MUST show processing directory DirPickers (default "processing/") and a per-tool matrix (rows = data types, columns = PI/Siril/Planetary). Project's workflow profile selects the applicable column at plan-generation time.
- **FR-062**: Root Recovery MUST show a centered workflow with original mount info, DirPicker for new path, mandatory 4-sample verification, and a "what will change" list before apply.
- **FR-063**: Equipment & Trains MUST show auto-detected equipment from metadata with user-editable aliases and named optical train configurations.
- **FR-064**: All settings MUST auto-save (no global Save button) with lightweight status indicator.

**First-Run Setup**

- **FR-065**: When no library roots are registered, app MUST show a centered 4-step wizard: Welcome → Sources → Scan settings → Confirm.
- **FR-066**: Setup wizard is one-time only. After completion, all root management happens in Settings → Data Sources. No re-launch option.
- **FR-067**: Sources step MUST categorize directories before adding (Raw/Calibration/Project/Inbox) and show estimated file counts pre-scan.

**Guided Tour**

- **FR-068**: After first-run completion and initial scan, non-blocking overlay hints MUST anchor to real UI elements guiding: (1) confirm first session, (2) create first project, (3) open in processing tool.
- **FR-069**: Tour hints MUST be dismissible. Tour state MUST track completion of each step. Settings toggle MUST allow restarting the tour.

**Density & Global Preferences**

- **FR-070**: App MUST support a single global density setting: compact (24px row), comfortable (32px default), spacious (40px row). No per-page override.
- **FR-071**: All tables, toolbars, nav items, and data-display components MUST respect the active density setting.

### Key Entities

- **AppFrame**: Window chrome container (title bar, sidebar, content, status bar)
- **Sidebar**: Collapsible navigation rail with items, badges, and collapse state
- **LogPanel**: Expandable status bar showing operation progress and events
- **CommandPalette**: Global search + action launcher (cmdk)
- **Session**: Acquisition session displayed in lists, grouped views, calendar, and detail
- **CalibrationMaster**: Calibration artifact with fingerprint, provenance, usage tracking
- **Target**: Astronomical target with coverage data and linked entities
- **Project**: Processing envelope with source map, lifecycle, views, and artifacts
- **FilesystemPlan**: Reviewable set of filesystem operations with approval gates
- **AuditEntry**: Immutable log record with event type, entity, state change, outcome

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: All 16 specified pages render correctly with realistic mock data, matching wireframe layouts within reasonable tolerance.
- **SC-002**: Users can navigate between all pages via sidebar, command palette, and breadcrumbs without dead-end states.
- **SC-003**: The review queue keyboard workflow (J/K/Cmd+1/2/3) allows processing 10 sessions in under 60 seconds.
- **SC-004**: The project creation wizard can be completed end-to-end in under 5 minutes for a standard project (3 light sessions, matching calibration).
- **SC-005**: Plan review page correctly applies the 3-tier approval gate system: safe plans approve in one click, trash plans require confirmation, delete plans require explicit checkbox.
- **SC-006**: Sidebar collapse state, density preference, project view mode, and wizard draft state all persist across app restarts.
- **SC-007**: All provenance glyphs, confidence indicators, lifecycle pills, and status badges render correctly and are distinguishable without relying on color alone (text labels always present).
- **SC-008**: Command palette returns relevant results for session names, target names, project names, page names, and action names within 200ms of keystroke.
- **SC-009**: All DirPicker components invoke the native OS directory dialog — no text input for paths anywhere in the application.
  *(Reconciliation note, 2026-07-19, issue #764: conflicts with spec 018
  FR-011 — `features/settings/ProcessingTools.tsx` has a real raw
  `<input type="text">` for each tool's executable path, saved on
  blur/Enter. Route through speckit-sync to adjudicate which requirement
  wins; not corrected here since it's a genuine cross-spec conflict, not a
  one-sided drift.)*
- **SC-010**: The guided tour completes all 3 hint steps without blocking user interaction with the underlying UI.

## Assumptions

- Backend contracts from specs 002-026 are available as mock data providers during frontend development. Real Tauri commands will be wired later.
- The application runs in a Tauri window with access to native dialog APIs for DirPicker.
- React 19, Base UI, TanStack Router, TanStack Table, cmdk, react-resizable-panels, and lucide-react are the established dependency set (inherited from spec 022).
- CSS tokens follow the `alm-*` class naming convention with CSS custom properties in a central `tokens.css` file (inherited from spec 022 infrastructure).
- The visual design tokens (colors, spacing, typography, density) come exclusively from DESIGN.md §3 (canvas export), replacing any prior token definitions.
- Mosaic/multi-panel project UI is out of scope for this spec (v1 = single-panel projects only).
- Light/dark mode is out of scope — the app renders in the canvas-specified light theme only.
- The guided tour library (react-joyride v3, per spec 010 research) provides the overlay hint infrastructure.

## Out of Scope

- Mosaic/multi-panel project UI (follow-on spec)
- Light/dark mode toggle
- CLI interface
- Cloud sync or multi-user
- Image preview or thumbnails
- In-app FITS header editing
- Per-page density override
- DirPicker paste/dropdown/drag-drop modes
- Wizard re-launch from settings
- Brownfield project import UI
