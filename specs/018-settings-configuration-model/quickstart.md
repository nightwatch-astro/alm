# Quickstart: Settings Configuration Model

> **T034 quickstart pass** — exercises the scope/values panes and auto-save
> via `useAutoSave`. Updated 2026-06-23 to reflect as-built reality; file and
> crate paths re-derived 2026-08-24.

## Overview

Settings are organized into six panes (scopes), each auto-saving on change
via `useAutoSave.ts` + `apps/desktop/src/features/settings/settingsIpc.ts`. There is **no
global Save button** and no localStorage path.

## Pane Reference

| Pane | Scope key | Example keys |
|------|-----------|--------------|
| Advanced | `advanced` | `logLevel`, `rememberFollowLogs`, `devMode` |
| General | `general` | `rowDensity` |
| Cleanup | `cleanup` | `blockPermanentDelete`, `defaultProtection`, `protectedCategories` |
| Naming | `naming` | `pattern`, `autoApplyPattern`, `patternsByType` |
| Sources | `sources` | `followSymlinks`, `hashOnScan`, `alwaysPreviewBeforePlan` |
| Calibration | `calibration` | `darkMatchTolerance`, `flatMatching`, `suggestCalibration`, `calibrationDarkTempTolerance`, `calibrationPrefillSuggestion`, `calibrationDarkOverridePenalty`, `calibrationFlatOverridePenalty`, `calibrationBiasOverridePenalty`, `calibrationAgingThresholdDays` |

## Walkthrough

### 1. Open Settings

Navigate to **Settings** in the app shell. The page renders section panes for
each scope. Each row shows a plain label, an info affordance (hover for help),
and a single control.

### 2. Change a control in each pane

Work through each pane and change at least one control:

- **Advanced** — change `Log level` to `debug`.
- **General** — toggle row density (note: FR-006 plans to remove this control).
- **Cleanup** — toggle `Block permanent delete` off, then back on.
- **Naming** — drag a token in the pattern builder to reorder it.
- **Sources** — toggle `Follow symlinks` on.
- **Calibration** — adjust `Dark temperature tolerance` to `1.0`.

### 3. Observe auto-save

Each change is dispatched immediately through `useAutoSave` →
`settingsUpdate` (scope/values). There is no save button. A lightweight
status indicator confirms persistence.

### 4. Verify no-op guard

Change a value to its current value (e.g. toggle a boolean twice quickly).
Confirm that the second change produces no audit entry — the no-op guard
short-circuits before dispatch when the new value equals the prior value.

### 5. Check noisy-key behavior

Edit the `pattern` token builder repeatedly. Confirm that the application log
does **not** accumulate one entry per keystroke. Instead, a `settings.snapshot`
audit event appears after 5 minutes of inactivity (or at session start).

### 6. Restore defaults

In any section header, trigger **Restore defaults** (when T028 is wired). Confirm
the section returns to default values and one audit entry per restored key appears
in the log.

### 7. Verify persistence across restart

Close and reopen the library. Confirm all changed values are still present.
The source of truth is the SQLite `settings` table
(defined in `crates/persistence/core/migrations/0001_initial_schema.sql`), not
localStorage.

## IPC Shape (for developers)

```json
// settings.get request
{ "scope": "calibration" }

// settings.get response
{ "scope": "calibration", "values": { "darkMatchTolerance": "strict", ... } }

// settings.update request
{ "scope": "calibration", "values": { "darkMatchTolerance": "loose" } }
```

Tauri commands: `settings_get`, `settings_update`, `settings_restore_defaults`,
`settings_source_override_set` in
`apps/desktop/src-tauri/src/commands/settings.rs`.

Desktop binding: `getSettings` / `updateSettings` in
`apps/desktop/src/features/settings/settingsIpc.ts`.
