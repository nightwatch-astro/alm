// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Seeded processing-tool profiles (spec 011 T002).
//!
//! Includes PixInsight and Siril with per-OS bundle IDs (R-BundleId) and
//! args templates (R3).
//!
//! `all()` returns a fixed-size array of owned `ToolProfile` values.
//! `validate_seeds()` asserts seed integrity. The seeds are compile-time
//! constants, so it is a test-time gate (`all_seeds_pass_validation`), not a
//! runtime one; it has no caller outside this module's tests.

use crate::{ArgsToken, DetachStrategy, ToolProfile, DEFAULT_SOURCE_VIEW_LAYOUT};

fn pixinsight_profile() -> ToolProfile {
    ToolProfile {
        id: "pixinsight",
        name: "PixInsight",
        bundle_id: Some("com.pixinsight.PixInsight"),
        // PixInsight cannot open a bare folder path (bug #778): the only
        // robust path argument is a `.xosm` project file we never generate,
        // and running WBPP's automationMode would violate the PixInsight
        // processing boundary (Constitution III). Launch bare instead — the
        // user opens their own files/project inside PixInsight, which is what
        // the one-time launch hint says (supports_open_folder=false). The
        // working folder reaches the tool only where the platform arm applies
        // it, and `bundle_id` is set here, so on macOS `open -b` runs and
        // Launch Services drops it (`launch::spawn_platform`).
        args_template: vec![],
        supports_open_folder: false,
        detach_strategy: DetachStrategy::OpenBundleId,
        // WBPP: session/night → filter → exposure grouping (spec 049 US2 T025).
        source_view_layout: Some(DEFAULT_SOURCE_VIEW_LAYOUT),
    }
}

fn siril_profile() -> ToolProfile {
    ToolProfile {
        id: "siril",
        name: "Siril",
        bundle_id: Some("org.free-astro.siril"),
        args_template: vec![ArgsToken::Folder],
        supports_open_folder: true,
        detach_strategy: DetachStrategy::OpenBundleId,
        // No Siril-specific layout yet — resolve_source_view_layout() falls
        // back to DEFAULT_SOURCE_VIEW_LAYOUT (spec 049 US2 T025 note).
        source_view_layout: None,
    }
}

fn planetary_suite_profile() -> ToolProfile {
    ToolProfile {
        id: "planetary_suite",
        name: "Planetary Suite",
        // No stable macOS bundle id is known for AutoStakkert! (research.md R2:
        // "if present; else surface not found") — `open -b` dispatch stays
        // unavailable until one is confirmed; Windows/Linux launch by path only.
        bundle_id: None,
        // AutoStakkert! opens its own dialog; cwd anchors it (spec 011 R3).
        args_template: vec![],
        supports_open_folder: false,
        detach_strategy: DetachStrategy::Setsid,
        // No Planetary-Suite-specific layout yet — falls back to
        // DEFAULT_SOURCE_VIEW_LAYOUT like Siril.
        source_view_layout: None,
    }
}

/// Return all seeded processing-tool profiles as an owned `Vec`.
#[must_use]
pub fn all() -> Vec<ToolProfile> {
    vec![pixinsight_profile(), siril_profile(), planetary_suite_profile()]
}

/// Validate all seed profiles.
///
/// # Errors
///
/// Returns the first validation error encountered, if any.
pub fn validate_seeds() -> Result<(), String> {
    for p in all() {
        p.validate()?;
    }
    Ok(())
}

/// Find a seed profile by `tool_id`, returning an owned clone.
#[must_use]
pub fn find(tool_id: &str) -> Option<ToolProfile> {
    all().into_iter().find(|p| p.id == tool_id)
}

/// Find a seed profile by its display `name` (the string stored in the
/// `projects.tool` column, e.g. `"PixInsight"`), returning an owned clone.
#[must_use]
pub fn find_by_name(name: &str) -> Option<ToolProfile> {
    all().into_iter().find(|p| p.name == name)
}

/// Resolve the effective source-view layout for a project's active tool
/// profile (spec 049 US2 T025).
///
/// `profile_ref` may be either a stable profile `id` (e.g. `"pixinsight"`) or
/// the display `name`/DB string (e.g. `"PixInsight"`, as stored in the
/// `projects.tool` column) — both are tried. Falls back to
/// [`crate::DEFAULT_SOURCE_VIEW_LAYOUT`] when `profile_ref` is `None`, matches
/// no seeded profile (e.g. `"Planetary Suite"`, which has none yet), or the
/// matched profile has no explicit layout of its own.
#[must_use]
pub fn resolve_source_view_layout(profile_ref: Option<&str>) -> crate::SourceViewLayout {
    profile_ref
        .and_then(|s| find(s).or_else(|| find_by_name(s)))
        .and_then(|p| p.source_view_layout)
        .unwrap_or(DEFAULT_SOURCE_VIEW_LAYOUT)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_seeds_pass_validation() {
        validate_seeds().expect("all seed profiles must be valid");
    }

    #[test]
    fn seed_ids_match_c2_invariant() {
        for p in all() {
            assert!(ToolProfile::id_is_valid(p.id), "seed id '{}' violates C2", p.id);
        }
    }

    #[test]
    fn pixinsight_launches_bare_with_no_folder_arg() {
        // PixInsight cannot open a bare folder path (bug #778): launch with
        // no args and rely on `cwd` anchoring instead of {folder} (spec 011
        // R-CwdContain, supports_open_folder=false path).
        let p = find("pixinsight").expect("pixinsight must be seeded");
        assert!(!p.supports_open_folder);
        assert!(p.args_template.is_empty());
        assert_eq!(p.bundle_id, Some("com.pixinsight.PixInsight"));
    }

    #[test]
    fn siril_has_correct_bundle_id() {
        let p = find("siril").expect("siril must be seeded");
        assert_eq!(p.bundle_id, Some("org.free-astro.siril"));
    }

    #[test]
    fn find_returns_none_for_unknown_id() {
        assert!(find("photoshop").is_none());
    }

    #[test]
    fn three_seeds_are_registered() {
        assert_eq!(all().len(), 3);
    }

    #[test]
    fn planetary_suite_is_seeded_and_launches_bare() {
        let p = find("planetary_suite").expect("planetary_suite must be seeded");
        assert_eq!(p.name, "Planetary Suite");
        assert!(!p.supports_open_folder);
        assert!(p.args_template.is_empty());
    }

    // ── Spec 049 US2 T023: WBPP layout groups by session/night → filter →
    // exposure, and calibration lands at the profile's expected location. ──

    #[test]
    fn pixinsight_layout_groups_lights_by_night_filter_exposure() {
        let p = find("pixinsight").expect("pixinsight must be seeded");
        let layout = p.source_view_layout.expect("pixinsight must have a source-view layout");
        assert_eq!(layout.light_pattern, "{date}/{filter}/{exposure}/");
        assert_eq!(layout.calibration_pattern, "calibration/{frame_type}/");
    }

    #[test]
    fn resolve_source_view_layout_matches_by_id_or_name() {
        let by_id = resolve_source_view_layout(Some("pixinsight"));
        let by_name = resolve_source_view_layout(Some("PixInsight"));
        assert_eq!(by_id, by_name);
        assert_eq!(by_id.light_pattern, "{date}/{filter}/{exposure}/");
    }

    #[test]
    fn resolve_source_view_layout_falls_back_to_default() {
        // No profile, an unmatched profile ref, and seeded profiles without an
        // explicit layout of their own (siril, planetary_suite) all fall back
        // to the WBPP/PixInsight default.
        assert_eq!(resolve_source_view_layout(None), crate::DEFAULT_SOURCE_VIEW_LAYOUT);
        assert_eq!(
            resolve_source_view_layout(Some("photoshop")),
            crate::DEFAULT_SOURCE_VIEW_LAYOUT
        );
        assert_eq!(resolve_source_view_layout(Some("siril")), crate::DEFAULT_SOURCE_VIEW_LAYOUT);
        assert_eq!(
            resolve_source_view_layout(Some("Planetary Suite")),
            crate::DEFAULT_SOURCE_VIEW_LAYOUT
        );
    }

    // ── Spec 049 US2 T024: changing the layout pattern only changes the
    // *destination path* produced downstream — this crate has no DB/plan
    // access, so this asserts the pure contract the builder relies on:
    // different profile refs resolve to independently-comparable pattern
    // strings with no shared mutable state (spec 049 US2 AS2).

    #[test]
    fn different_profiles_can_resolve_to_different_layouts_independently() {
        let default_layout = resolve_source_view_layout(None);
        let pixinsight_layout = resolve_source_view_layout(Some("pixinsight"));
        // Today these happen to be equal (pixinsight IS the default), but the
        // resolution is independent per call — no shared/cached state that
        // would prevent a future per-profile override from taking effect.
        assert_eq!(default_layout, pixinsight_layout);
        assert_eq!(pixinsight_layout.light_pattern, "{date}/{filter}/{exposure}/");
    }
}
