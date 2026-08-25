# TinySpec: Canonical settings-key naming

**Branch**: 042-stdlib-adoption (implement after the in-progress main→042 merge is committed)
**Date**: 2026-06-21
**Status**: implemented 2026-08-24 (landed with PR #1171; verified, not re-implemented)
**Complexity**: small

## What

Settings key-strings mix three styles: camelCase that mirrors the wire field
name (`autoApplyPattern`, `hashOnScan`), dotted-snake that does **not** match the
field's camelCase wire name (`calibration.dark_temp_tolerance`,
`plans.list.default_age_cutoff_days`), and plain snake (`current_library_id`,
`patterns_by_type`). Normalize every settings key to one rule. Greenfield — no
persisted-data migration. The serde `rename_all = "camelCase"` struct-field wire
boundary is correct and stays untouched; this changes **key strings only**.

## Canonical rule (decision)

**A settings key string equals the serde-camelCased wire name of its
`SettingsState` field.** The majority already comply; only the dotted/snake
outliers below change. This makes keys derivable from the struct and
test-enforceable.

| Old key | New key |
|---------|---------|
| `current_library_id` | `currentLibraryId` |
| `plans.list.default_age_cutoff_days` | *(setting removed, not renamed — see below)* |
| `calibration.dark_temp_tolerance` | `calibrationDarkTempTolerance` |
| `calibration.prefill_suggestion` | `calibrationPrefillSuggestion` |
| `calibration.dark.override_penalty` | `calibrationDarkOverridePenalty` |
| `calibration.flat.override_penalty` | `calibrationFlatOverridePenalty` |
| `calibration.bias.override_penalty` | `calibrationBiasOverridePenalty` |
| `calibration.aging_threshold_days` | `calibrationAgingThresholdDays` |
| `imagetyp_normalization.user_mappings` | `imagetypNormalizationUserMappings` |
| `patterns_by_type` | `patternsByType` |

## Context

| File | Role |
|------|------|
| `crates/app/settings/src/descriptors.rs` | Modify — `key:` entries, `NOISY_KEYS`, `OVERRIDABLE_KEYS`, validation match arms |
| `crates/persistence/lifecycle/src/repositories/settings.rs` | Modify — read/write key match arms + `PATTERNS_BY_TYPE_KEY` const (this spec was written against the pre-split path `crates/persistence/db/src/repositories/settings.rs`) |
| `apps/desktop/src/features/settings/*` | Modify — any literal key references |
| (repo-wide consumers) | Modify — crates/use-cases reading old keys (e.g. calibration, plans) — grep each old string |
| `crates/app/settings/src/` tests | Add — guard test: every `SettingsState` wire field name ∈ key registry and vice versa |

## Requirements

1. Every key in the table above is renamed to its camelCase wire-field form at all definition sites.
2. Every **consumer** of a renamed key (repo-wide, not just settings) is updated; no old key string remains.
3. `NOISY_KEYS` / `OVERRIDABLE_KEYS` / validation arms reference the new keys.
4. The serde `rename_all` struct-field boundary is unchanged.
5. A guard test asserts the key set is exactly the set of `SettingsState` camelCase wire field names.

## Plan

1. Update `descriptors.rs` keys, NOISY/OVERRIDABLE lists, and validation arms.
2. Update `persistence/.../settings.rs` match arms + `PATTERNS_BY_TYPE_KEY`.
3. Repo-wide: `rg` each old key string; update every remaining consumer + frontend literal.
4. Add the key↔field guard test.

## Tasks

- [x] Rename keys in `descriptors.rs` (entries, NOISY_KEYS, OVERRIDABLE_KEYS, validation)
- [x] Rename keys in persistence `settings.rs` (match arms + `PATTERNS_BY_TYPE_KEY`)
- [x] `rg` old strings repo-wide; update all consumers + `features/settings/*` literals
- [x] Add key↔wire-field guard test
- [x] `cargo clippy`/`test` for touched crates + `tsc`/`vitest` for settings frontend

## Done When

- [x] All tasks checked off; no old key survives as a **wire key**. `rg` is
      deliberately **not** clean — the old strings remain in 5 files as Rust
      snake_case field identifiers and as negative-test literals, enumerated
      under Outcome below.
- [x] Guard test passes; touched-crate + settings-frontend gates green
- [x] No lint errors

## Outcome (verified 2026-08-24)

Every key in `DESCRIPTORS` is the serde-camelCase wire name of its
`SettingsState` field. Two independent guard tests bind the three sites that
must agree, so a future drift is a test failure rather than a silently
forgotten user setting:

- `crates/app/settings/src/tests.rs:69` — descriptor registry ↔ `SettingsState`
  wire field names, both directions.
- `crates/persistence/lifecycle/src/repositories/settings.rs:631` — the
  persistence hydration table (`APPLY_KEYS`) ↔ `SettingsState` wire field names.

### The 5 files that still match an old key string, and why each is correct

Grepping the 10 old keys across `crates/`, `apps/`, and `packages/` returns 5
files. None contains an old **wire key**; every hit is one of three legitimate
shapes, so a future grep should not refile this spec:

| File | Hits | Shape |
|------|------|-------|
| `crates/domain/core/src/settings.rs` | `:148`, `:182`, `:289`, `:298` | `SettingsState` field declarations and defaults. Rust identifiers are snake_case; serde `rename_all = "camelCase"` produces the wire key. |
| `crates/app/settings/src/descriptors.rs` | `:285-287`, `:414-420` | Field access inside `apply`/`default` closures — reading `s.current_library_id` / `s.patterns_by_type`, not a key string. |
| `crates/persistence/lifecycle/src/repositories/settings.rs` | `:206`, `:215` | `settings_key_table!` rows pairing the camelCase **key** with its snake_case **field**; `PATTERNS_BY_TYPE_KEY` is `"patternsByType"` at `:25`. |
| `crates/app/settings/src/ingestion.rs` | `:11` | A doc comment naming the `get_patterns_by_type` function. |
| `crates/app/settings/src/tests.rs` | `:633`, `:1035-1046` | Negative tests asserting the old dotted keys are rejected. These must keep the old literals to test anything. |

The compatibility decision is **reject, no shim**: an old dotted key is not
accepted by validation (`crates/app/settings/src/tests.rs:633`,
`:1035-1046`). `apply_key_to_state` ignores an unrecognised stored key rather
than erroring (`crates/persistence/lifecycle/src/repositories/settings.rs:178-181`),
which is what lets structured-path keys (`tools.*`, `workflow_profile.*`) live
outside the static `SettingsState` bag.

Two divergences from this spec as written:

1. `plans.list.default_age_cutoff_days` has no canonical successor. The setting
   was removed rather than renamed; neither the old key nor
   `plansListDefaultAgeCutoffDays` appears in `crates/`, `apps/`, or
   `packages/`.
2. Settings keys are not enumerated in `packages/contracts`. They cross the
   Tauri boundary as an opaque `key: String`, so the rename was not a schema
   change.
