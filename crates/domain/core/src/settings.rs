// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Stored settings domain types (spec 042 T254).
//!
//! These types are the durable, on-disk representation of the v1 settings bag
//! and per-source overrides. They live in `domain_core` (the lowest layer) so
//! the persistence layer can depend on them without importing the transport
//! crate (`contracts_core`). `contracts_core` re-exports them so the IPC /
//! binding surface is byte-identical to before the move.
//!
//! The serde derives (`rename_all = "camelCase"`, field shapes,
//! `skip_serializing_if`) are preserved verbatim from the former
//! `contracts_core::settings` definitions so the persisted JSON and the
//! generated TypeScript bindings are unchanged.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::JsonAny;

// ── PatternPart (data-model.md §Pattern Part) ─────────────────────────────

/// One token or separator in the project folder naming pattern.
///
/// `kind` is `"token"` (resolves at materialization) or `"separator"` (literal).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PatternPart {
    /// Stable identifier for drag-reorder.
    pub id: String,
    /// `"token"` or `"separator"`.
    pub kind: String,
    /// Token name (e.g. `"target"`) or literal separator character.
    pub value: String,
}

// ── ImageTypMapping (data-model.md absorbed keys §F) ─────────────────────

/// User-extensible IMAGETYP normalization entry (spec 005 R-IMAGETYP-Norm).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImageTypMapping {
    pub imagetyp_string: String,
    pub frame_type: String,
}

// ── ObserverSite (spec 044 Track B, data-model.md §1) ─────────────────────

/// One named observing location the planner computes observability against.
///
/// Persisted in the spec-018 settings store (frontend astronomy consumes these
/// values, ADR-0001). `latitude_deg`/`longitude_deg` are manual entry (no online
/// lookup); `timezone` is an IANA id drawn from a bundled offline list and drives
/// local-time rendering + DST. `elevation_m` is optional.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ObserverSite {
    /// Stable, immutable identity; referenced by default/active site pointers.
    pub id: String,
    /// User label (e.g. "Home", "Dark site"); non-empty.
    pub name: String,
    /// Latitude in decimal degrees, `[-90, 90]`.
    pub latitude_deg: f64,
    /// Longitude in decimal degrees, `[-180, 180]`; east-positive.
    pub longitude_deg: f64,
    /// Elevation in metres; optional.
    #[serde(default)]
    pub elevation_m: Option<f64>,
    /// IANA timezone id (e.g. `Europe/Amsterdam`).
    pub timezone: String,
    /// Per-site night definition: `"astronomical"` (Sun −18°) or `"nautical"` (−12°).
    pub twilight: String,
    /// Local-obstruction floor in degrees, `[0, 90]`; standard refraction still
    /// applied at the true horizon.
    pub min_horizon_alt_deg: f64,
}

// ── MoonAvoidanceBand (spec 047 plannerMoonAvoidance) ─────────────────────

/// Per-band Lorentzian Moon-avoidance parameters (spec 047 data-model.md).
///
/// `distance_deg` is the required target↔Moon separation at full Moon (deg);
/// `width_days` is the Lorentzian half-width in Moon-age days. The frontend
/// planner derives per-band filter viability from these; they are never used
/// for durable/audited decisions (ADR-0001).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MoonAvoidanceBand {
    /// Required separation at full Moon, degrees in [0, 180].
    pub distance_deg: f64,
    /// Lorentzian half-width in Moon-age days, in [0.5, 30].
    pub width_days: f64,
}

// ── SettingsState v1 ──────────────────────────────────────────────────────

/// Complete v1 settings bag (data-model.md §`SettingsState` v1).
///
/// Fields that hold structured-path key values (`tools.*`, `workflow_profile.*`)
/// that cannot be statically typed are captured in `extra`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
// This is a flat data-transfer bag; bools are intentional per the spec data-model.
#[allow(clippy::struct_excessive_bools)]
pub struct SettingsState {
    // ── Naming & Structure ──────────────────────────────────────────────
    /// Project folder token pattern. Noisy key.
    pub pattern: Vec<PatternPart>,
    /// Whether new projects adopt the pattern without confirmation.
    pub auto_apply_pattern: bool,

    // ── Ingestion & Review ──────────────────────────────────────────────
    /// Forces a preview step before any filesystem plan is generated.
    pub always_preview_before_plan: bool,

    // ── Data Sources ────────────────────────────────────────────────────
    /// Follow symlinks during scan. Off by default (constitution §I).
    pub follow_symlinks: bool,
    /// Hashing strategy: `"lazy"` | `"eager"` | `"off"`.
    pub hash_on_scan: String,

    // ── Calibration ─────────────────────────────────────────────────────
    /// Dark frame match tolerance: `"strict"` | `"loose"` | `"any"`.
    pub dark_match_tolerance: String,
    /// Flat matching: `"filter-rot"` | `"filter"` | `"manual"`.
    pub flat_matching: String,
    /// Whether to surface calibration suggestions.
    pub suggest_calibration: bool,

    // ── Application Log ──────────────────────────────────────────────────
    /// Log level: `"error"` | `"warn"` | `"info"` | `"debug"`.
    pub log_level: String,
    /// Whether follow-tail toggle persists across restarts. Noisy.
    pub remember_follow_logs: bool,

    // ── Source Protection ────────────────────────────────────────────────
    /// Default protection level: `"protected"` | `"unprotected"`.
    pub default_protection: String,
    /// Routes destructive operations to archive/trash workflows.
    pub block_permanent_delete: bool,
    /// Protected category strings. Noisy.
    pub protected_categories: Vec<String>,

    // ── Absorbed keys (spec 018 Phase 9) ────────────────────────────────
    /// Currently-open library id (spec 020 R-Lib-V1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_library_id: Option<String>,

    /// Runtime developer-mode toggle. Always `false` in release builds.
    pub dev_mode: bool,

    /// Dark frame temperature matching tolerance in °C (spec 007 A5).
    pub calibration_dark_temp_tolerance: f64,

    /// Pre-fill the calibration assign dialog with the top candidate (spec 007 R-Prefill).
    pub calibration_prefill_suggestion: bool,

    /// Confidence penalty when user overrides dark suggestion \[0,1\] (spec 007).
    pub calibration_dark_override_penalty: f64,

    /// Confidence penalty when user overrides flat suggestion \[0,1\] (spec 007).
    pub calibration_flat_override_penalty: f64,

    /// Confidence penalty when user overrides bias suggestion \[0,1\] (spec 007).
    pub calibration_bias_override_penalty: f64,

    /// Days after which a calibration master is considered aging (spec 007/018 FR-023).
    /// Consumers compare `master.age_days` against this value instead of hardcoding 90.
    pub calibration_aging_threshold_days: f64,

    /// User-extensible IMAGETYP normalization entries (spec 005 R-IMAGETYP-Norm).
    pub imagetyp_normalization_user_mappings: Vec<ImageTypMapping>,

    /// Per-frame-type destination pattern overrides (spec 041 FR-026/FR-026b).
    ///
    /// Maps a frame-type class name (`light`, `flat`, `dark`, `bias`,
    /// `master_flat`, `master_dark`, `master_bias`) to a destination pattern
    /// string. Missing/empty/invalid entries fall back to the built-in default
    /// from `crates/patterns::default_pattern` at read time; this map carries
    /// only the user's explicit overrides. Empty by default.
    pub patterns_by_type: std::collections::BTreeMap<String, String>,

    // ── Tool watch / attribution (spec 018 T043) ─────────────────────────
    /// File extensions the tool-launch watcher monitors (spec 018 T043).
    ///
    /// Default covers all common astrophotography formats (XISF, FITS, TIFF,
    /// PNG, JPEG, SER, AVI). Stored as a flat global; no per-profile override.
    pub tool_watch_extensions: Vec<String>,

    /// Hours after a tool launch during which output files are attributed to
    /// that session (spec 018 T043). Default: 6.0 hours.
    pub tool_attribution_window_hours: f64,

    // ── Observing sites (spec 044 Track B, data-model.md §1) ─────────────
    /// Named observing-site collection. Empty = no-site state (US6). Site ids
    /// MUST be unique. Consumed frontend-only by the planner astronomy engine
    /// (ADR-0001); persisted so the user's sites survive relaunch.
    pub observing_sites: Vec<ObserverSite>,

    /// Which site is the default (referenced `ObserverSite.id` or `null`). At
    /// most one default; must stay valid across edits/deletes.
    pub observing_default_site_id: Option<String>,

    /// Which site the planner currently computes for (referenced
    /// `ObserverSite.id` or `null`); **persists across relaunch**.
    pub observing_active_site_id: Option<String>,

    /// Global lowest-worthwhile altitude in degrees, `[0, 90]`. Default 30.
    /// Replaces the localStorage `ALTITUDE_THRESHOLD_KEY` / `USABLE_ALT_DEG`
    /// constant (spec 044 FR-004); durable so it survives relaunch.
    pub usable_altitude_deg: f64,

    // ── Target planner (spec 047 FR-010) ────────────────────────────────
    /// Per-band Moon-avoidance Lorentzian parameters for the Targets planner.
    ///
    /// Maps each of the seven fixed filter bands (`L`, `R`, `G`, `B`, `Ha`,
    /// `SII`, `OIII`) to its `{ distanceDeg, widthDays }`. Consumed frontend-only
    /// by the spec 047 filter-guidance rule (ADR-0001); persisted so the user's
    /// tuning survives, never used for a durable/audited decision. Defaults:
    /// LRGB 120°/14d · Ha/SII 60°/7d · OIII 110°/10d.
    pub planner_moon_avoidance: std::collections::BTreeMap<String, MoonAvoidanceBand>,

    // ── Source Views (spec 049) ──────────────────────────────────────────
    /// Default link kind when a source and the generated view destination
    /// share a volume: `"hardlink"` | `"symlink"` | `"junction"` (spec 049
    /// FR-004). Resolved deterministically per drive-scope at plan time.
    pub source_view_link_kind_intra_drive: String,

    /// Default link kind when a source and the generated view destination
    /// are on different volumes: `"symlink"` | `"junction"` (spec 049
    /// FR-004a — `"hardlink"` is never a valid cross-drive value because
    /// hardlinks cannot cross volumes).
    pub source_view_link_kind_cross_drive: String,

    // ── Cleanup overrides (spec 051 US3) ─────────────────────────────────
    /// Per-data-type cleanup action overrides, keyed by the stable numeric
    /// data-type id (stringified, JSON object keys are always strings) from
    /// the frontend `CLEANUP_TYPES` fixture, valued `"Keep"` | `"Archive"` |
    /// `"Delete"` (data-model.md §E2). Absent id ⇒ that type's built-in
    /// default action applies. Empty by default (no overrides).
    pub cleanup_type_overrides: std::collections::BTreeMap<String, String>,

    // ── Appearance ─────────────────────────────────────────────────────
    /// UI theme choice: one of the four fixed theme ids (`warm-clay`,
    /// `warm-slate`, `observatory-dark`, `espresso-dark`) or `"system"` to
    /// follow the OS light/dark preference (`apps/desktop/src/data/theme.ts`
    /// `ThemeChoice`). This is the durable source of truth; the frontend
    /// mirrors it into a synchronous localStorage boot cache to avoid a
    /// flash of the wrong theme, since WebView2 only flushes localStorage on
    /// graceful shutdown and loses it on a forced kill.
    pub theme: String,

    // ── Framing clustering tunables (spec 008 Q27 F-Framing-11, R11a) ────
    /// Fraction of the optic-train FOV diagonal used as the pointing
    /// tolerance when grouping sessions into a framing. R11a default 0.10.
    pub framing_pointing_fraction_of_fov: f64,
    /// Absolute pointing tolerance in degrees used when the optic-train has
    /// no FOV data (R11a no-equipment fallback). Default 0.2.
    pub framing_pointing_fallback_deg: f64,
    /// Rotation tolerance in degrees for framing membership. Default 3.0.
    pub framing_rotation_tolerance_deg: f64,
    /// Mosaic candidate envelope (FR-019 relaxation): fraction of the FOV
    /// diagonal used as the pointing envelope around an existing framing's
    /// representative for `isMosaic` projects. Default 1.0.
    pub framing_mosaic_envelope_fraction_of_fov: f64,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            pattern: default_pattern(),
            auto_apply_pattern: true,
            always_preview_before_plan: false,
            follow_symlinks: false,
            hash_on_scan: "lazy".to_owned(),
            dark_match_tolerance: "strict".to_owned(),
            flat_matching: "filter-rot".to_owned(),
            suggest_calibration: true,
            log_level: "info".to_owned(),
            remember_follow_logs: false,
            default_protection: "protected".to_owned(),
            block_permanent_delete: true,
            protected_categories: vec![
                "lights".to_owned(),
                "masters".to_owned(),
                "finals".to_owned(),
            ],
            current_library_id: None,
            dev_mode: false,
            calibration_dark_temp_tolerance: 2.0,
            calibration_prefill_suggestion: true,
            calibration_dark_override_penalty: 0.3,
            calibration_flat_override_penalty: 0.3,
            calibration_bias_override_penalty: 0.3,
            calibration_aging_threshold_days: 90.0,
            imagetyp_normalization_user_mappings: vec![],
            patterns_by_type: std::collections::BTreeMap::new(),
            tool_watch_extensions: vec![
                ".xisf".to_owned(),
                ".fits".to_owned(),
                ".fit".to_owned(),
                ".tif".to_owned(),
                ".tiff".to_owned(),
                ".png".to_owned(),
                ".jpg".to_owned(),
                ".ser".to_owned(),
                ".avi".to_owned(),
            ],
            tool_attribution_window_hours: 6.0,
            observing_sites: vec![],
            observing_default_site_id: None,
            observing_active_site_id: None,
            usable_altitude_deg: 30.0,
            planner_moon_avoidance: default_planner_moon_avoidance(),
            source_view_link_kind_intra_drive: "hardlink".to_owned(),
            source_view_link_kind_cross_drive: "symlink".to_owned(),
            cleanup_type_overrides: std::collections::BTreeMap::new(),
            theme: "system".to_owned(),
            // R11a shipped defaults — must stay bit-identical to
            // `sessions::ToleranceParams::defaults()`.
            framing_pointing_fraction_of_fov: 0.10,
            framing_pointing_fallback_deg: 0.2,
            framing_rotation_tolerance_deg: 3.0,
            framing_mosaic_envelope_fraction_of_fov: 1.0,
        }
    }
}

/// Shipped default per-band Moon-avoidance parameters (spec 047 data-model.md).
///
/// LRGB 120°/14d · Ha/SII 60°/7d · OIII 110°/10d. Keys are the seven fixed
/// filter bands; `BTreeMap` keeps the serialised order stable.
#[must_use]
pub fn default_planner_moon_avoidance() -> std::collections::BTreeMap<String, MoonAvoidanceBand> {
    use std::collections::BTreeMap;
    let mut m = BTreeMap::new();
    let broadband = MoonAvoidanceBand { distance_deg: 120.0, width_days: 14.0 };
    let narrowband = MoonAvoidanceBand { distance_deg: 60.0, width_days: 7.0 };
    let oiii = MoonAvoidanceBand { distance_deg: 110.0, width_days: 10.0 };
    for band in ["L", "R", "G", "B"] {
        m.insert(band.to_owned(), broadband.clone());
    }
    m.insert("Ha".to_owned(), narrowband.clone());
    m.insert("SII".to_owned(), narrowband);
    m.insert("OIII".to_owned(), oiii);
    m
}

fn default_pattern() -> Vec<PatternPart> {
    vec![
        PatternPart { id: "p0".to_owned(), kind: "token".to_owned(), value: "target".to_owned() },
        PatternPart { id: "p1".to_owned(), kind: "separator".to_owned(), value: "/".to_owned() },
        PatternPart { id: "p2".to_owned(), kind: "token".to_owned(), value: "filter".to_owned() },
        PatternPart { id: "p3".to_owned(), kind: "separator".to_owned(), value: "/".to_owned() },
        PatternPart { id: "p4".to_owned(), kind: "token".to_owned(), value: "date".to_owned() },
        PatternPart { id: "p5".to_owned(), kind: "separator".to_owned(), value: "/".to_owned() },
        PatternPart {
            id: "p6".to_owned(),
            kind: "token".to_owned(),
            value: "frame_type".to_owned(),
        },
        PatternPart { id: "p7".to_owned(), kind: "separator".to_owned(), value: "/".to_owned() },
    ]
}

// ── SourceOverride ────────────────────────────────────────────────────────

/// Per-source override of an overridable settings key (data-model.md §`SourceOverride`).
///
/// Overridable keys: `followSymlinks`, `hashOnScan`, `defaultProtection`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SourceOverride {
    pub source_id: String,
    pub key: String,
    pub value: JsonAny,
    pub updated_at: String,
}
