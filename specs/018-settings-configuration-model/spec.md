# Feature Specification: Settings Configuration Model

> **⚠ Partial supersession (2026-06-18)**: any catalog settings here (manifest URL / signing key /
> downloaded-catalog files) are superseded by [Spec 035 — SIMBAD Target Resolution](../035-simbad-target-resolution/spec.md);
> the catalog settings surface becomes the SIMBAD resolver settings (endpoint, enable/disable, cache).
> All other settings are unaffected.

> **See Spec 030**: UI implementation of this feature must follow
> [Spec 030 — UI Audit & Revision](../030-ui-audit-revision/spec.md)
> for layout, navigation, and component patterns.

**Feature Branch**: `018-settings-configuration-model`  
**Created**: 2026-05-09  
**Status**: Completed (archived 2026-06-23; PRs #348/#350/#352)  
**Input**: User description: "Specify the settings model after the UI review: plain labels, one setting per line, hover information, auto-save, no internal technical controls, and grouped submenus that match user workflows."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Understand Every Setting (Priority: P1)

As a user, I want each setting to have a plain label, a single-line control, and an information affordance so that configuration does not feel like internal implementation leakage.

**Why this priority**: The user identified multiple settings whose names and controls were unclear.

**Independent Test**: Open Settings and inspect each setting row; confirm each row has one control line, an information icon, clear label text, and no save button.

**Acceptance Scenarios**:

1. **Given** Settings is open, **When** a setting row is displayed, **Then** it has one primary line with label, info affordance, and control.
2. **Given** a user hovers the information affordance, **When** help opens, **Then** it explains the setting in user language.
3. **Given** a setting changes, **When** the control value changes, **Then** the app saves automatically and shows lightweight status.

---

### User Story 2 - Configure Workflow-Relevant Rules (Priority: P2)

As a user, I want settings grouped by workflow domain so that source behavior, calibration matching, project naming, tools, catalogs, logs, and safety are easy to find.

**Why this priority**: Settings should match product workflows, not code modules.

**Independent Test**: Navigate each settings section and confirm controls are grouped under Sources, Calibration, Projects, Tools, Catalogs, Safety, Logs, and Appearance or equivalent final names.

**Acceptance Scenarios**:

1. **Given** a user needs calibration matching, **When** they open Settings, **Then** matching rules are grouped under Calibration.
2. **Given** a user needs project folder naming, **When** they open Settings, **Then** pattern builder controls are grouped under Projects.
3. **Given** a user needs log display preferences, **When** they open Settings, **Then** only workflow-relevant log display controls are shown.

### Edge Cases

- User changes a setting while an operation is running.
- Auto-save fails.
- Setting requires native path selection.
- A setting is only relevant when another feature is enabled.
- A setting has invalid values after version upgrade.

### Domain Questions To Resolve

- Final menu names for Settings sections.
- Which settings are project-level overrides versus global defaults.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Settings MUST use plain user-facing labels and avoid internal terms such as marker write queue, project envelope, prepared sources layout, structure normalization, material kinds, reuse scope, or metadata repair unless reworked into clear user concepts.
- **FR-002**: Settings MUST show one setting per line.
- **FR-003**: Each setting row MUST include an information affordance that explains what the setting changes, how the app uses it, what the options mean, and any workflow or safety consequence; it MUST NOT merely restate the label.
- **FR-004**: Settings MUST auto-save changes and MUST NOT require a global Save button.
- **FR-005**: Destructive or high-risk setting changes MUST surface a prominent
  warning (e.g. the Cleanup danger banner). *(Reconciled 2026-06-23 to the
  auto-save model: settings persist immediately, so the binding *confirmation*
  for an actual destructive filesystem operation is enforced at plan-application
  time — the reviewable-plan gate, constitution §II — not at the auto-saved
  toggle. A settings toggle sets policy; it does not itself mutate the
  filesystem.)*
- **FR-006**: Display density is a user-selectable **Appearance preference**, owned
  by the appearance/theme system (spec 043), persisted via `prefs.density` and
  applied app-wide by the shell. The legacy per-table `rowDensity` *settings* key
  is removed (T032). *(Amended 2026-06-23: the original prohibition — "Settings
  MUST NOT expose density controls" — is relaxed to keep density as an Appearance
  preference per the design-v4/043 redesign. The constraint that density is not a
  per-table or internal control is preserved; it is a single global Appearance
  choice.)*
- **FR-007**: Light/dark mode MUST be available as an icon control in the app shell and as a persisted appearance setting if exposed.
  *(Reconciliation note, 2026-07-19, issue #764: spec 043's redesign shipped
  4 named themes + system (`General.tsx`'s swatch-card picker: system,
  warm-clay, warm-slate, observatory-dark, espresso-dark), not a binary
  light/dark icon toggle. The persisted-setting half of this FR still holds
  (`theme` in the "general" settings scope); the "icon control" shape does
  not.)*
- **FR-008**: Project folder and archive location patterns MUST use the token pattern builder, not freeform text.
- **FR-009**: Calibration matching settings MUST be per calibration frame type.
  *(Reconciled 2026-06-23 to as-built: per-frame-type lives in the backend model
  — `darkMatchTolerance`/`flatMatching`, per-type dark/flat/bias override
  penalties, and per-frame-type destination patterns (`patternsByType`). The
  visible **Calibration Matching** pane presents per-criterion match-required
  toggles (camera/binning/gain/offset/temp/aging) backed by the spec-007
  `calibrationTolerances` store; per-frame-type granularity is in the keys it
  consumes, not as separate dark/flat/bias rows.)*
- **FR-010**: Catalog/target-lookup settings configure target resolution behavior.
  *(Reconciled 2026-06-23: catalog-**family** selection (manifest/`active_catalogs`)
  is superseded by [Spec 035 — SIMBAD Target Resolution](../035-simbad-target-resolution/spec.md);
  T039 is OBSOLETE. The **Target Resolution** settings pane configures the SIMBAD
  resolver — endpoint, enable/disable, and cache — in place of family selection.)*
- **FR-011**: Tool settings MUST configure executable paths for each supported processing tool.
- **FR-012**: API contract settings MUST NOT appear as a normal user settings section.
- **FR-013**: Log settings MUST not expose export format or request/entity metadata toggles; request/entity metadata is always present and export is JSON when offered.

### Key Entities

- **Setting Section**: Workflow-oriented group of settings.
- **Setting Row**: Label, information affordance, control, validation, and auto-save state.
- **Setting Help**: Short explanation shown on hover/focus.
- **Setting Change Event**: Auditable configuration change.
- **Project Override**: Project-specific configuration that overrides global defaults.

### Absorbed Settings Keys (2026-05-22 ripple absorption)

The following keys were flagged across multiple prior ratification passes and
are now absorbed into the v1 settings model.

**Library context**

- `current_library_id` (`string?`, uuid): tracks which library is currently
  open; drives `?lib=<library_id>` URL injection across all `<Link>` components
  (spec 020 R-Lib-V1). Single-library v1 always has one value; slot is reserved.
- `devMode` (`boolean`, default `false`): runtime toggle for the developer-mode
  surface (recording proxy, `/dev/contracts` route). Only meaningful when the
  binary is compiled with the Cargo feature `dev-tools`; in release builds this
  key is read-only and hidden from Settings UI (spec 021 R-DevFeature).

**Plans**

- `plans.list.default_age_cutoff_days` (`number`, default `90`, `0` = show all):
  UI hides terminal plans older than this threshold by default (spec 017 R-Ret-1).

**Log viewer**

- `rememberFollowLogs` (`boolean`, default `false`): persists the "follow tail"
  toggle state in the log viewer across app restarts (spec 019 E-019-3).

**Target lookup**

- ~~`target_lookup.active_catalogs`~~ **DROPPED** — referenced spec 014 catalog
  manifest, which is superseded by spec 035 (SIMBAD resolve-on-demand). No
  manifest exists; this key is not implemented.

**Calibration matching**

- `calibration.dark_temp_tolerance` (`number`, default `2.0` °C): dark frame
  temperature matching tolerance (spec 007 A5).
- `calibration_dark_override_penalty` / `calibration_flat_override_penalty` /
  `calibration_bias_override_penalty` (`number`, default `0.3`, range `[0, 1]`):
  per-frame-type confidence penalty applied when a user overrides the
  auto-suggested calibration match (spec 007 R-OverridePenalty). These are
  **flat typed fields** on `SettingsState`, not structured-path keys.
- `calibration.prefill_suggestion` (`boolean`, default `true`): when true, the
  assign dialog opens pre-filled with the top candidate; user must confirm
  (spec 007 R-Prefill).

**Tool launching**

- `tools.<tool_id>.bundle_id` (`string?`, macOS only): per-tool macOS bundle
  identifier used for `open -b` launching; seed values for known tools
  (PixInsight, Siril); user-editable for custom installs (spec 011 R-BundleId).

**Artifact watcher (spec 012)**

- `tool_watch_extensions` (`string[]`, default `[".xisf",".fits",".fit",".tif",
  ".tiff",".png",".jpg",".ser",".avi"]`): global allow-list of file extensions
  monitored by the artifact-observation watcher (spec 012 R-ExtAllow). **Flat
  global field** — replaces the former per-profile structured-path key.
- `tool_attribution_window_hours` (`number`, default `6`): global attribution
  window for matching artifacts to tool launches (spec 012 C3). **Flat global
  field** — replaces the former per-profile structured-path key.

**Inbox destination (spec 041)**

- `patterns_by_type` (`BTreeMap<String, String>`): per-frame-type destination
  pattern overrides; maps frame type strings to pattern tokens. Drives spec 041
  FR-026/FR-026b single-type item destination resolution.

**Ingestion gate**

- `always_preview_before_plan` (`boolean`, default `false`): forces a preview
  step before any filesystem plan is generated (also present in main field table).

**Calibration aging (FR-023)**

- `calibration_aging_threshold_days` (`f64`): threshold beyond which a
  calibration frame is considered aged; used in calibration suggestion scoring
  (spec 007/018 FR-023).

**FITS classifier**

- `imagetyp_normalization_user_mappings` (`Vec<{ imagetyp_string: string,
  frame_type: FrameType }>`): user-extensible table for IMAGETYP strings not
  covered by the built-in normalization table; empty array default (spec 005
  R-IMAGETYP-Norm).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Every visible setting can be explained without referring to internal implementation names.
- **SC-002**: Users can configure common setup, calibration, project naming, tool path, catalog, safety, log, and appearance preferences from Settings.
- **SC-003**: Settings changes persist without a global save action.
- **SC-004**: No visible settings section is named API Contracts.

## Assumptions

- Project-specific configuration lives in the Edit project pane when it only applies to one project.
- Developer diagnostics can expose contract references outside normal user settings.

## Out of Scope

- Implementing persistence migrations.
- Designing every final tooltip copy string.

## Implementation Status

*(Reconciled 2026-06-23 against as-built code on `main`; file and crate paths
re-derived 2026-08-24.)*

The backend settings subsystem is built and on `main`. The desktop UI panes are
wired via `useAutoSave` and `apps/desktop/src/features/settings/settingsIpc.ts`. The **localStorage
mockup path is replaced** by a Tauri backend. Remaining work is in UI polish,
US5 migration, dev_mode release gating, and two reframed key tasks (T042/T043).

### IPC Transport (scope/values)

The IPC surface uses a **scope/values** model:

- `settings.get { scope } → { scope, values }` — returns all keys owned by
  `scope` as a flat JSON object; empty scope = full bag.
- `settings.update { scope, values }` — persists every key in `values`; unknown
  keys are silently skipped (best-effort write); per-key validation, no-op guard,
  and audit emission happen in the repository layer.
- `settings.restore-defaults` — restore one, several, or all keys to defaults.
- `settings.source-override.set` — set a per-source override for an overridable key.

Scope → key mapping:

| Scope | Keys |
|-------|------|
| `advanced` | `logLevel`, `rememberFollowLogs`, `devMode` |
| `general` | *(empty — `rowDensity` removed by T032; display density is an Appearance preference)* |
| `cleanup` | `blockPermanentDelete`, `defaultProtection`, `protectedCategories` |
| `naming` | `pattern`, `autoApplyPattern`, `patternsByType` |
| `sources` | `followSymlinks`, `hashOnScan`, `alwaysPreviewBeforePlan` |
| `calibration` | `darkMatchTolerance`, `flatMatching`, `suggestCalibration`, `calibrationDarkTempTolerance`, `calibrationPrefillSuggestion`, `calibrationDarkOverridePenalty`, `calibrationFlatOverridePenalty`, `calibrationBiasOverridePenalty`, `calibrationAgingThresholdDays` |

### As-Built Reality

- **Domain types**: `crates/domain/core/src/settings.rs` (`PatternPart`,
  `ImageTypMapping`, `SettingsState`, `SourceOverride`), re-exported by
  `crates/contracts/core/src/settings.rs` (spec 042 T254).
- **Use-case layer**: `crates/app/settings/src/lib.rs` (crate `app_core_settings`,
  re-exported as `app_core::settings`): `get_settings`, `update_setting`,
  `restore_defaults`, `set_source_override`, `resolve_setting`, `emit_snapshot`.
  Key metadata is descriptor-driven from `crates/app/settings/src/descriptors.rs`
  (`DESCRIPTORS` table, 29 keys — single source for key set, noisy, overridable,
  defaults, devMode cfg gate).
- **Persistence**: the `settings` + `source_overrides` tables in
  `crates/persistence/core/migrations/0001_initial_schema.sql` (originally
  migration `0013_settings.sql`, since folded into the pre-1.0 baseline).
  Low-level repo:
  `crates/persistence/lifecycle/src/repositories/settings.rs` (get_raw/set_raw/load_settings/
  patterns_by_type helpers).
- **Tauri commands**: `apps/desktop/src-tauri/src/commands/settings.rs`
  (`settings_get`, `settings_update`, `settings_restore_defaults`,
  `settings_source_override_set`).
- **Desktop binding**: `apps/desktop/src/features/settings/settingsIpc.ts`
  (`getSettings` / `updateSettings`, the latter re-exported from
  `apps/desktop/src/data/settingsWrite.ts`) over the `apps/desktop/src/api/ipc.ts`
  switcher and the generated `packages/contracts/src/generated/` types; wired
  through `apps/desktop/src/features/settings/useAutoSave.ts` in section panes.
  NOTE: `apps/desktop/src/data/settings.ts` does NOT exist, and neither does the
  former single-file binding `apps/desktop/src/api/commands.ts`.
- **Audit variants**: `SettingsChanged`, `SettingsSnapshot`, `SettingsRepair`
  in `crates/audit/src/event_bus.rs`.
- **No `crates/app/core/usecases/settings.rs`**: this path does not exist.
- **Wired settings sections**: advanced / general / cleanup / naming / sources /
  calibration. API Contracts is not a user section (FR-012).
- **Auto-save**: section panes use `useAutoSave.ts`; no per-key save button.
- **Theme**: separate `pv.theme` localStorage key in
  `apps/desktop/src/data/theme.ts`; not part of `SettingsState`.
