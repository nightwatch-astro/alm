# Data Model: Settings Configuration Model

*(Reconciled 2026-06-23 against as-built code on `main`; file and crate paths
re-derived 2026-08-24.)*

## IPC Transport

The IPC surface uses a **scope/values** model. `settings.get { scope }` returns
a flat JSON bag; `settings.update { scope, values }` persists every key in
`values`. Empty scope = full bag. See `spec.md` Implementation Status for the
scope → key map.

## SettingsState v1

The canonical bag of v1 settings. Defined in
`crates/domain/core/src/settings.rs` and persisted in a SQLite `settings`
table (`crates/persistence/core/migrations/0001_initial_schema.sql`) in which each field maps to one row
keyed by field name. The localStorage path (`alm.settings.v1`) is superseded.

### Field Table

| Key                       | Type                                       | Section            | Section Title         | Notes                                                                 |
|---------------------------|--------------------------------------------|--------------------|-----------------------|-----------------------------------------------------------------------|
| `pattern`                 | `PatternPart[]`                            | Naming & Structure | Project folder        | Structured token list, not a string. Noisy.                           |
| `autoApplyPattern`        | `boolean`                                  | Naming & Structure | Auto-apply pattern    | Whether new projects adopt the pattern without confirmation.          |
| `alwaysPreviewBeforePlan` | `boolean`                                  | Ingestion & Review | Preview before plan   | Forces a preview step before any filesystem plan is generated.        |
| `followSymlinks`          | `boolean`                                  | Data Sources       | Follow symlinks       | Off by default per constitution. Overridable per source.              |
| `hashOnScan`              | `"lazy" \| "eager" \| "off"`               | Data Sources       | Hash on scan          | Overridable per source.                                               |
| `darkMatchTolerance`      | `"strict" \| "loose" \| "any"`             | Calibration        | Dark match            | Per calibration frame type (dark).                                    |
| `flatMatching`            | `"filter-rot" \| "filter" \| "manual"`     | Calibration        | Flat match            | Per calibration frame type (flat).                                    |
| `suggestCalibration`      | `boolean`                                  | Calibration        | Suggest calibration   | Whether to surface calibration suggestions.                           |
| `rowDensity`              | `"dense" \| "comfortable"`                 | Advanced           | ~~Row density~~ **REMOVED** | REMOVED: T032 (FR-006). Key was vestigial; no UI consumer. Removed 2026-06-23. |
| `logLevel`                | `"error" \| "warn" \| "info" \| "debug"`   | Application Log    | Log level             |                                                                       |
| `rememberFollowLogs`      | `boolean`                                  | Application Log    | Remember follow       | Whether the follow-tail toggle persists across restarts. See absorbed keys table for updated default. |
| `defaultProtection`       | `"protected" \| "normal" \| "unprotected"` | Source Protection  | Default protection    | Overridable per source.                                               |
| `blockPermanentDelete`    | `boolean`                                  | Source Protection  | Block permanent delete| Routes destructive operations to archive/trash workflows.             |
| `protectedCategories`     | `string[]`                                 | Source Protection  | Protected categories  | Array of category strings. Noisy. (R-Set-1)                          |

### Defaults

| Key                       | Default                                                                                                  |
|---------------------------|----------------------------------------------------------------------------------------------------------|
| `pattern`                 | `[{target}, /, {filter}, /, {date}, /, {frame_type}, /]`                                                 |
| `autoApplyPattern`        | `true`                                                                                                   |
| `alwaysPreviewBeforePlan` | `false`                                                                                                  |
| `followSymlinks`          | `false`                                                                                                  |
| `hashOnScan`              | `"lazy"`                                                                                                 |
| `darkMatchTolerance`      | `"strict"`                                                                                               |
| `flatMatching`            | `"filter-rot"`                                                                                           |
| `suggestCalibration`      | `true`                                                                                                   |
| `rowDensity`              | ~~`"dense"`~~ **REMOVED** (T032)                                                                                 |
| `logLevel`                | `"info"`                                                                                                 |
| `rememberFollowLogs`      | `false` (amended from `true` per spec 019 E-019-3)                                                       |
| `defaultProtection`       | `"protected"`                                                                                            |
| `blockPermanentDelete`    | `true`                                                                                                   |
| `protectedCategories`     | `["lights", "masters", "finals"]`                                                                        |

### Absorbed Keys (2026-05-22 ripple absorption; reconciled 2026-06-23)

All absorbed keys are **flat typed fields** on `SettingsState` except
`tools.<tool_id>.bundle_id`, which remains a per-tool structured key.
`target_lookup.active_catalogs` and the `workflow_profile.*` structured-path
keys are dropped (see notes).

| Key (field name on SettingsState) | Type | Default | Overridable per source? | Description | Noisy? |
|---|---|---|---|---|---|
| `current_library_id` | `String?` (uuid) | `null` | No | Currently-open library id; drives `?lib=` URL injection (spec 020 R-Lib-V1). | No |
| `devMode` | `bool` | `false` | No | Runtime developer-mode toggle. Only meaningful in `dev-tools` builds; release gating enforced via `#[cfg(not(feature = "dev-tools"))]` in `descriptors.rs` (T036 done). | No |
| ~~`plans_list_default_age_cutoff_days`~~ | — | — | — | **DROPPED** (2026-07-19, issue #624) — never had a UI consumer (no Plans list page ever shipped); removed as vestigial, re-addable if a Plans page is built. | — |
| `rememberFollowLogs` | `bool` | `false` | No | Persists log viewer "follow tail" state across restarts (spec 019 E-019-3). | Yes |
| ~~`target_lookup.active_catalogs`~~ | — | — | — | **DROPPED** — spec 014 catalog manifest superseded by spec 035 (SIMBAD). | — |
| `calibration_dark_temp_tolerance` | `f64` (°C) | `2.0` | No | Dark frame temperature matching tolerance (spec 007 A5). | No |
| `calibration_dark_override_penalty` | `f64` [0,1] | `0.3` | No | Confidence penalty when user overrides dark calibration suggestion (spec 007 R-OverridePenalty). **Flat field.** | No |
| `calibration_flat_override_penalty` | `f64` [0,1] | `0.3` | No | Confidence penalty when user overrides flat calibration suggestion (spec 007 R-OverridePenalty). **Flat field.** | No |
| `calibration_bias_override_penalty` | `f64` [0,1] | `0.3` | No | Confidence penalty when user overrides bias calibration suggestion (spec 007 R-OverridePenalty). **Flat field.** | No |
| `calibration_prefill_suggestion` | `bool` | `true` | No | Open assign dialog pre-filled with top candidate; user must confirm (spec 007 R-Prefill). | No |
| `calibration_aging_threshold_days` | `f64` | `90.0` | No | Threshold beyond which a calibration frame is considered aged; scoring input (spec 007/018 FR-023). | No |
| `tools.<tool_id>.bundle_id` | `String?` | `null` (seeded for known tools) | No | Per-tool macOS bundle id for `open -b` launching; user-editable (spec 011 R-BundleId). Validated against ToolProfile rows (T042, open). | No |
| ~~`workflow_profile.<profile_id>.watch_extensions`~~ | — | — | — | **DROPPED** — replaced by flat `tool_watch_extensions`. | — |
| ~~`workflow_profile.<profile_id>.launch_attribution_window_hours`~~ | — | — | — | **DROPPED** — replaced by flat `tool_attribution_window_hours`. | — |
| `tool_watch_extensions` | `Vec<String>` | see below | No | Global allow-list of extensions monitored by the artifact-observation watcher (spec 012 R-ExtAllow, T043). | No |
| `tool_attribution_window_hours` | `f64` | `6` | No | Global attribution window for matching artifacts to tool launches (spec 012 C3, T043). | No |
| `patterns_by_type` | `BTreeMap<String, String>` | `{}` | No | Per-frame-type destination pattern overrides (spec 041 FR-026/FR-026b). | No |
| `always_preview_before_plan` | `bool` | `false` | No | Forces a preview step before any filesystem plan is generated (also in main field table). | No |
| `imagetyp_normalization_user_mappings` | `Vec<ImageTypMapping>` | `[]` | No | User-extensible IMAGETYP normalization entries for niche capture software (spec 005 R-IMAGETYP-Norm). | No |

**Default for `tool_watch_extensions`**: `[".xisf", ".fits", ".fit", ".tif", ".tiff", ".png", ".jpg", ".ser", ".avi"]`

**Seed values for `tools.<tool_id>.bundle_id`** (stored only when tool is registered):
- `tools.pixinsight.bundle_id`: `"com.pixinsight.PixInsight"`
- `tools.siril.bundle_id`: `"org.siril.Siril"`

### Noisy Keys

`pattern`, `protectedCategories`, `plans.list.default_age_cutoff_days`,
`rememberFollowLogs`. Persisted on every change; audited as a snapshot rather
than per-change. See `research.md` R4. `plans.list.default_age_cutoff_days`
is noisy because users may sweep the slider frequently; `rememberFollowLogs`
is a toggle that changes on each session open but requires no discrete audit.

### Overridable Keys

`followSymlinks`, `hashOnScan`, `defaultProtection`. All other keys are
global-only in v1; attempts to override return `key.unoverridable`.
(`autoApplyPattern` was removed from the overridable set — A2.)

None of the newly absorbed keys are overridable per source. Structured-path
keys (`tools.*`, `workflow_profile.*`) encode their scope in their key path
and are not subject to the per-source override mechanism.

## SourceOverride

A per-source override of an overridable settings key.

| Field       | Type     | Notes                                                                |
|-------------|----------|----------------------------------------------------------------------|
| `source_id` | `string` | Identifier of the data source (root) the override applies to.        |
| `key`       | `string` | Must be one of the overridable keys.                                 |
| `value`     | `JSON`   | Must validate against the same JSON Schema as the global key value.  |
| `updated_at`| `string` | ISO-8601 timestamp set by the store on write.                        |

Resolution order is per-source override → global setting → in-code default
(`research.md` R6).

## Theme (separate)

Theme persists separately and is intentionally not part of `SettingsState`.

| Field  | Type                                | Storage                                 |
|--------|-------------------------------------|-----------------------------------------|
| `mode` | `"system" \| "light" \| "dark"`      | localStorage key `alm.theme`            |

Theme changes do not appear in the settings audit stream and do not
participate in `settings.get`, `settings.update`, or
`settings.restore-defaults`.

## Pattern Part

`PatternPart` is the token-builder fragment used by `pattern`.

| Field   | Type                          | Notes                                                  |
|---------|-------------------------------|--------------------------------------------------------|
| `id`    | `string`                      | Stable identifier for drag-reorder.                    |
| `kind`  | `"token" \| "separator"`     | Token resolves at materialization; separator is literal.|
| `value` | `string`                      | Token name or literal separator character.             |

## Audit Event (settings shape)

| Field          | Type     | Notes                                                          |
|----------------|----------|----------------------------------------------------------------|
| `source`       | `string` | Always `"settings"`.                                           |
| `level`        | `string` | `"info"` for changes; `"warn"` for schema repairs.             |
| `key`          | `string` | Settings key affected (omitted for snapshot events).           |
| `prior_value`  | `JSON`   | Previous value (omitted for snapshot events).                  |
| `new_value`    | `JSON`   | New value (omitted for snapshot events).                       |
| `snapshot`     | `JSON?`  | Noisy-key snapshot at session boundaries.                      |
| `at`           | `string` | ISO-8601 timestamp.                                            |
