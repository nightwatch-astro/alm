# Implementation Plan: UI Audit & Revision

**Branch**: `030-ui-audit-revision` | **Date**: 2026-05-26 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/030-ui-audit-revision/spec.md`

## Summary

Comprehensive UI/UX overhaul of every screen in the Astro Library Manager
desktop app. Redesigns the setup wizard from 8 steps to 4, renames Review
Queue to Inbox with a new session review workflow, establishes a hybrid layout
model (sidebars for Inbox/Projects, top action bars elsewhere), simplifies
navigation from 9 to 7 items, reorganizes Settings from 12 to 11 logically
grouped panes, and adds an operational status bar. All changes are
frontend-only unless a UI change requires a backend contract adjustment.

## Technical Context

**Language/Version**: TypeScript 5.x (React frontend), Rust (backend only if
new commands needed)

**Primary Dependencies**: React, TanStack Router, Tauri 2.x, custom design
tokens (alm-* CSS vars), no component library

**Storage**: SQLite via Tauri backend (existing), localStorage for preferences

**Testing**: Vitest (component tests), Playwright (E2E scripts)

**Target Platform**: Windows/macOS/Linux desktop via Tauri 2.x

**Project Type**: Desktop app (Tauri + React)

**Performance Goals**: UI transitions < 100ms, list rendering smooth at 1000+
sessions

**Constraints**: No component library — all components are custom. Mock data
via `VITE_USE_MOCKS=true`. Native Tauri builds required for OS file pickers.

**Scale/Scope**: ~50 React components to modify or create, 7 feature
directories, 11 settings panes, 1 app shell rewrite

**Audit architecture (iteration 2026-07-14, Q15 / #647)**: Two disjoint
stores exist today. The `EventBus` (`crates/audit/src/bus.rs:37-40`) is a
hybrid tokio broadcast (live UI) + durable `events` topic stream
(`crates/persistence/core/migrations/0001_initial_schema.sql:720`) — a topic+payload
stream without outcome/refused semantics, not an audit record. The durable
`audit_log_entry` table
(`crates/persistence/core/migrations/0001_initial_schema.sql:130-143`) is
lifecycle-transition-shaped (`crates/audit-types/src/event.rs:106+`) and is
written only by lifecycle transitions
(`crates/persistence/db/src/repositories/lifecycle.rs:423,511`) and the
audit repository insert
(`crates/persistence/db/src/repositories/audit.rs:216`). Bus-only mutation
emitters: protection (`crates/app/core/src/protection.rs:227-228,404-419` —
returns an `auditId` with no durable row), settings
(`crates/app/settings/src/lib.rs:481,500,601,615,768`), source ops
(`crates/app/core/src/first_run.rs:503,542,597`). Equipment CRUD
(`crates/app/calibration/src/equipment.rs`) emits no audit at all. Phase G
unifies these per FR-130–FR-134.

**Metadata value path (iteration 2026-07-14, Q16 / #620)**: absence is
lost mid-path today. Extraction preserves it
(`crates/metadata/core/src/lib.rs:221,223` — `Option`-typed exposure/gain,
~25 more optional fields) and so does persistence
(`crates/persistence/db/src/repositories/q_calibration.rs:93-94` —
`fp_gain`/`fp_exposure_s: Option<f64>`), but the application layer
collapses missing to 0
(`crates/app/calibration/src/matching.rs:739,741,794,796` —
`unwrap_or(0.0)`) and the contract cannot carry absence
(`crates/contracts/core/src/calibration.rs:96,99` — `exposure_s`/`gain`
non-optional `f64`). Master size is absent one layer deeper: the SQL view supplies
no real size (as of 2026-08-24 `CAST(NULL AS INTEGER) AS size_bytes` at
`crates/persistence/core/migrations/0001_initial_schema.sql:3186`; it hardcoded `0 AS size_bytes` when this was written)
through a non-nullable row (`q_calibration.rs:92`) into non-optional
`CalibrationMaster`/`MasterDetail.size_bytes: u64`; `matching.rs:748,803`
is only a sign-conversion fallback, so fixing size requires a
view-redefinition migration (see data-model.md). UI null-checks are therefore dead
code (`apps/desktop/src/features/calibration/MastersTable.tsx:116,126`),
and the shared `PropertyTable` renders `null` as `—` for both missing and
not-applicable while showing the source badge regardless of value presence
(`apps/desktop/src/components/PropertyTable.tsx:44-48,181-197`). Phase H
fixes the model first (contract optionality, remove zero-defaulting), then
routes all rendering through one shared renderer per FR-135–FR-140.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Local-First File Custody | PASS | UI-only changes; file custody model unchanged |
| II. Reviewable Filesystem Mutation | PASS | Cleanup plans remain reviewable (FR-088). Confirmation overlay shows target directory (FR-041) |
| III. PixInsight Boundary | PASS | No processing features added. Tool config is path/directory only (FR-107) |
| IV. Research-Led Domain Modeling | PASS | Session naming, filter lists, calibration matching display — all decisions documented in spec with user input |
| V. Portable Contracts | PASS | UI changes go through existing Tauri commands. New commands (equipment CRUD, status bar data) will use same contract pattern |

| Product Constraint | Status | Notes |
|-------------------|--------|-------|
| Support messy libraries | PASS | Inbox workflow handles unstructured input; empty-folder warnings don't block |
| Cross-platform paths | PASS | Source view strategy dropdown handles platform differences |
| Optional hashing | PASS | Hashing mode configurable in Ingestion settings |
| No symlink following | PASS | Follow symlinks/junctions toggles in Ingestion settings (default off) |
| Protected categories before cleanup | PASS | Cleanup policy per-type table defines actions before any plan runs |

## Project Structure

### Documentation (this feature)

```text
specs/030-ui-audit-revision/
├── plan.md              # This file
├── research.md          # Phase 0 — component research
├── data-model.md        # Phase 1 — settings/equipment model changes
├── contracts/           # Phase 1 — new/modified Tauri commands
└── tasks.md             # Phase 2 — task breakdown
```

### Source Code (repository root)

```text
apps/desktop/src/
├── app/
│   ├── router.tsx              # Route changes: rename paths, remove Plans/Audit
│   ├── Shell.tsx               # Layout model: sidebar vs top-bar routing
│   ├── Sidebar.tsx             # Nav items: 7 items, sidebar footer
│   ├── StatusBar.tsx           # Complete rewrite: inbox/stats/cleanup/storage
│   └── LogPanel.tsx            # May be merged into status bar
├── features/
│   ├── setup/                  # REWRITE: 4-step wizard, unified folder picker
│   │   ├── SetupPage.tsx
│   │   ├── SetupWizard.tsx
│   │   ├── sources-store.ts
│   │   └── steps/              # Replace 8 steps with 4
│   ├── review/ → inbox/        # RENAME + REWRITE: session review workflow
│   │   ├── InboxPage.tsx
│   │   ├── InboxList.tsx
│   │   ├── SessionReview.tsx   # Property table, conflict detection
│   │   ├── ActionSidebar.tsx   # Confirm/reject/split/merge/edit
│   │   └── ConfirmOverlay.tsx  # Token pattern preview
│   ├── sessions/               # MODIFY: top action bar, calendar scroll
│   │   ├── SessionsPage.tsx
│   │   ├── SessionDetail.tsx   # Unified property table, read-only
│   │   ├── CalendarScroll.tsx  # NEW: vertical timeline view
│   │   └── TopActionBar.tsx    # NEW: replaces right sidebar
│   ├── calibration/            # MODIFY: masters+subs, fingerprint section
│   │   ├── CalibrationPage.tsx
│   │   ├── CalibrationDetail.tsx
│   │   └── MatchingFingerprint.tsx  # NEW
│   ├── targets/                # MODIFY: coverage by train, top action bar
│   │   ├── TargetsPage.tsx
│   │   └── TargetDetail.tsx
│   ├── projects/               # REWRITE: lifecycle sidebar, consolidated view
│   │   ├── ProjectsPage.tsx
│   │   ├── ProjectDetail.tsx   # Single view (no tabs)
│   │   ├── SourceMap.tsx       # Kept, cleaned up
│   │   ├── LifecycleSidebar.tsx # NEW: phase-contextual actions
│   │   └── MarkdownEditor.tsx  # NEW: inline note editing
│   ├── archive/                # NEW: top-level archive screen
│   │   ├── ArchivePage.tsx
│   │   └── ArchiveList.tsx
│   ├── settings/               # REWRITE: 11 panes, reorganized
│   │   ├── SettingsPage.tsx
│   │   ├── DataSources.tsx     # Add/edit/remove roots
│   │   ├── Equipment.tsx       # Optical trains, cameras, telescopes, filters
│   │   ├── Ingestion.tsx       # NEW: watcher, scan defaults, tolerances
│   │   ├── NamingStructure.tsx # Fix patterns, token dropdown, per-type defaults
│   │   ├── SourceViewStrategy.tsx  # Simplify to dropdown
│   │   ├── ProcessingTools.tsx # NEW: replaces Tools.tsx, add dir structure
│   │   ├── CalibrationMatching.tsx # NEW: tolerance settings
│   │   ├── Catalogs.tsx        # Add enable/disable, Download All
│   │   ├── Cleanup.tsx         # NEW: per-type table, auto/manual toggle
│   │   ├── General.tsx         # NEW: replaces DisplayPane.tsx
│   │   ├── Advanced.tsx        # NEW: merge app log + debug
│   │   └── AuditLog.tsx        # MOVED from features/audit/
│   ├── audit/ → removed        # Moved to settings/AuditLog.tsx
│   ├── plans/ → removed        # Dropped from nav
│   └── tour/ → removed         # Guided tour removed from Appearance
├── components/                  # NEW: shared components
│   ├── ListSidebar.tsx         # Standardized list controls
│   ├── TopActionBar.tsx        # Shared top action bar
│   ├── PropertyTable.tsx       # Shared property table (editable/read-only)
│   └── FilterBar.tsx           # Shared search/group/sort/filter
└── data/
    └── preferences.ts          # Add new preference keys
```

**Structure Decision**: Primarily frontend changes within the existing
`apps/desktop/` structure. A new `components/` directory is created for shared
components extracted from the layout standardization work. Backend changes are
limited to new Tauri commands for equipment CRUD, cleanup policy, and status
bar data.

## Complexity Tracking

No constitution violations to justify.

---

## Phase 0: Research

### R1: Markdown Editor Component

**Decision**: Use a lightweight markdown editor for inline project notes
(FR-086). Candidates: `@uiw/react-md-editor`, `react-simplemde-editor`,
`milkdown`, or a minimal textarea + markdown preview toggle.

**Research needed**: Which library is smallest, works in Tauri/Electron
context, supports inline editing with preview, and has no heavy dependencies?

### R2: Calendar Scroll Component

**Decision**: The Calendar Scroll view (FR-053) is a vertical scrolling
timeline with sticky month headers. This is a custom component — no library
needed. Implementation approach: virtualized list (e.g.,
`@tanstack/react-virtual`) with date-grouped sections and sticky headers.

**Rationale**: Standard calendar libraries render month grids. The scroll
timeline is a custom layout better built from primitives.

### R3: Per-Tool Directory Structure Defaults

**Decision**: Processing Tools settings (FR-107) need vendor-default directory
structure templates for PixInsight/WBPP and Siril.

**Research needed**: What folder structure does WBPP create by default? What
does Siril's processing workflow expect? Document the default folder names and
what goes in each.

### R4: Token Pattern Defaults Per Frame Type

**Decision**: Naming patterns must differ per frame type (FR-105). Light frames
use `{object}`, calibration frames don't.

**Research needed**: What are sensible default naming patterns for each frame
type (Light, Dark, Flat, Bias) that align with common astrophotography
conventions?

### R5: Shared List Component Design

**Decision**: All list screens must share identical controls (FR-005). This
requires extracting a shared `ListSidebar` / `FilterBar` component from the
existing per-screen implementations.

**Research needed**: Audit the current filter/sort/group implementations across
Sessions, Calibration, Targets, Projects to identify the common interface and
per-screen variations.

---

*Research items R1, R3, and R4 require external investigation. R2 and R5 are
internal codebase decisions. All will be resolved in `research.md`.*

---

## Phase 1: Design

Phase 1 artifacts (data-model.md, contracts/) will document:

### Data Model Changes

- **Equipment entities**: Camera, Telescope, OpticalTrain, Filter — separate
  from the existing detected-equipment table in calibration matching. These
  need their own tables/CRUD.
- **Cleanup policy model**: Per-type action table (Keep/Archive/Delete) instead
  of per-tool policy matrix. New settings schema.
- **Ingestion settings**: Scan defaults (follow symlinks, hashing mode,
  metadata extraction) move from data sources to ingestion config.
- **Source folder types**: Expand from current (raw, calibration, project,
  inbox) to (light_frames, dark, flat, bias, project, inbox).
- **Project lifecycle**: Remove "Prepared" state, simplify to 5 phases.

### Contract Changes

- **Equipment CRUD**: New commands for camera/telescope/filter/optical-train
  management.
- **Cleanup policy**: New command to read/write per-type cleanup actions.
- **Status bar data**: New command aggregating inbox count, library stats,
  cleanup available, storage health per volume.
- **Source type expansion**: Modify `roots.register` to accept the expanded
  type enum.
- **Settings restructure**: Commands for ingestion config, calibration matching
  tolerances.

### Implementation Phases

The implementation is organized into 8 phases (A–H), ordered by dependency:

| Phase | Scope | Key Deliverables |
|-------|-------|-----------------|
| **A. Shared Components** | Extract and build reusable components | ListSidebar, FilterBar, TopActionBar, PropertyTable |
| **B. App Shell** | Navigation, router, sidebar, status bar | 7 nav items, sidebar footer, status bar rewrite |
| **C. Setup Wizard** | 4-step wizard rewrite | Unified folder picker, tool config, catalogs, confirm |
| **D. Inbox & Sessions** | Core review workflow + sessions | Inbox rename/rewrite, session review, calendar scroll |
| **E. Calibration, Targets, Projects** | Remaining detail screens | Fingerprint section, coverage by train, lifecycle sidebar |
| **F. Settings & Archive** | Configuration + archive | 11 panes, archive screen, audit log move |
| **G. Audit Unification** *(iteration 2026-07-14, Q15 / #647)* | Durable audit coverage & store unification | Generalized audit entry model, durable writes for all bus-only mutation emitters (settings, protection, equipment, sources/roots), Activity/log panel reads durable audit + live event bus |
| **H. Missing-Value Semantics & Detail-as-Delta** *(iteration 2026-07-14, Q16 / #620, #619)* | Three-state value model + shared renderer + detail-panel content model | Contract/model optionality sweep (no sentinel zeros), shared `renderValue` renderer with unresolved chip and presence-coupled source pills, adoption across all metadata surfaces, detail panels reworked to lead with non-row information |

Each phase is independently testable and deployable. Phase A must come first
as it provides the shared components used by all subsequent phases. Phase G
depends only on existing audit plumbing and is independent of B–F. Phase H
depends on the shipped metadata/contract plumbing and the shared components
from Phase A only, and is independent of B–G *as applied to the shipped
UI*: T133 reworks detail panels that Phases D/E build, so if any B–F panel
work is still in flight, T133 lands after it.

---

**Plan complete.** Next step: `/speckit-tasks` to generate the task breakdown,
or `/speckit-critique` for a quality gate first.
