// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Tool-launch attribution logic (spec 012 T022/T022b, data-model §Tool Launch Attribution).
//!
//! Attributes a detected artifact to the nearest preceding `ToolLaunch` row
//! for the same project and tool, within a configurable time window.
//!
//! Attribution uses the **app clock** (the `detected_at` timestamp recorded
//! when the artifact arrived at the app), NOT the filesystem `mtime`.
//! This guards against NAS clock skew (R-AppClock).
//!
//! Re-attribution (A7): when a new `tool.launch` event fires, this module
//! back-fills `tool_launch_id` for artifacts whose `detected_at` is within
//! the window of the new launch's `launched_at` AND whose current
//! `tool_launch_id` is null OR points to an earlier launch.

use std::time::Duration;

/// A minimal view of a `ToolLaunch` row needed for attribution.
#[derive(Clone, Debug)]
pub struct LaunchRef {
    pub id: String,
    pub tool_id: String,
    pub launched_at: time::OffsetDateTime,
}

/// Default attribution window: 6 hours (C3).
pub const DEFAULT_ATTRIBUTION_WINDOW: Duration = Duration::from_hours(6);

/// Select the best `ToolLaunch` for a detected artifact.
///
/// Returns the `id` of the nearest preceding launch (same tool, within
/// `window` before `detected_at`) or `None` if no match exists.
#[must_use]
pub fn attribute(
    tool: &str,
    detected_at: time::OffsetDateTime,
    launches: &[LaunchRef],
    window: Duration,
) -> Option<String> {
    let window_secs = window.as_secs_f64();
    launches
        .iter()
        .filter(|l| {
            l.tool_id == tool && {
                let diff = (detected_at - l.launched_at).as_seconds_f64();
                diff >= 0.0 && diff <= window_secs
            }
        })
        .max_by(|a, b| {
            a.launched_at.partial_cmp(&b.launched_at).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|l| l.id.clone())
}

/// Determine which artifact rows should be re-attributed to `new_launch`.
///
/// Returns the ids of artifacts produced by `new_launch`'s tool whose
/// `detected_at` falls within `window` of `new_launch.launched_at` AND whose
/// current `tool_launch_id` is either `None` (unattributed) or points to a
/// launch earlier than `new_launch`.
///
/// The caller is responsible for updating the DB rows.
#[must_use]
pub fn reattribute_candidates<'a>(
    new_launch: &LaunchRef,
    // (artifact_id, artifact_tool, detected_at, current_launch_id)
    artifacts: &'a [(String, String, time::OffsetDateTime, Option<String>)],
    existing_launches: &[LaunchRef],
    window: Duration,
) -> Vec<&'a String> {
    let window_secs = window.as_secs_f64();
    artifacts
        .iter()
        .filter(|(_, artifact_tool, detected_at, current_launch_id)| {
            // Must be same tool and within window.
            if *artifact_tool != new_launch.tool_id {
                return false;
            }
            let diff = (*detected_at - new_launch.launched_at).as_seconds_f64();
            if diff < 0.0 || diff > window_secs {
                return false;
            }
            // Accept if unattributed.
            let Some(current_id) = current_launch_id else { return true };
            // Accept if current launch is earlier than new launch.
            let current_launch_time =
                existing_launches.iter().find(|l| &l.id == current_id).map(|l| l.launched_at);
            match current_launch_time {
                Some(t) => t < new_launch.launched_at,
                None => true, // orphaned reference — re-attribute
            }
        })
        .map(|(id, _, _, _)| id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn launch(id: &str, tool: &str, launched_at: time::OffsetDateTime) -> LaunchRef {
        LaunchRef { id: id.to_owned(), tool_id: tool.to_owned(), launched_at }
    }

    #[test]
    fn nearest_preceding_launch_selected() {
        let launches = vec![
            launch("launch-1", "pixinsight", datetime!(2026-06-01 08:00 UTC)),
            launch("launch-2", "pixinsight", datetime!(2026-06-01 10:00 UTC)),
            launch("launch-3", "pixinsight", datetime!(2026-06-01 14:00 UTC)),
        ];
        let detected_at = datetime!(2026-06-01 11:30 UTC);
        let result = attribute("pixinsight", detected_at, &launches, DEFAULT_ATTRIBUTION_WINDOW);
        assert_eq!(result, Some("launch-2".to_owned()));
    }

    #[test]
    fn no_match_when_all_launches_after_detection() {
        let launches = vec![launch("launch-1", "pixinsight", datetime!(2026-06-01 20:00 UTC))];
        let detected_at = datetime!(2026-06-01 11:00 UTC);
        let result = attribute("pixinsight", detected_at, &launches, DEFAULT_ATTRIBUTION_WINDOW);
        assert!(result.is_none());
    }

    #[test]
    fn no_match_when_launch_outside_window() {
        let launches = vec![launch("launch-1", "pixinsight", datetime!(2026-06-01 00:00 UTC))];
        // Detected 10 hours after launch — outside the 6h window.
        let detected_at = datetime!(2026-06-01 10:00 UTC);
        let result = attribute("pixinsight", detected_at, &launches, DEFAULT_ATTRIBUTION_WINDOW);
        assert!(result.is_none());
    }

    #[test]
    fn wrong_tool_not_matched() {
        let launches = vec![launch("launch-1", "siril", datetime!(2026-06-01 09:00 UTC))];
        let detected_at = datetime!(2026-06-01 10:00 UTC);
        let result = attribute("pixinsight", detected_at, &launches, DEFAULT_ATTRIBUTION_WINDOW);
        assert!(result.is_none());
    }

    #[test]
    fn reattribute_candidates_selects_unattributed_in_window() {
        let new_launch = launch("launch-new", "pixinsight", datetime!(2026-06-01 10:00 UTC));
        let artifacts = vec![
            ("art-1".to_owned(), "pixinsight".to_owned(), datetime!(2026-06-01 11:00 UTC), None),
            // outside window
            ("art-2".to_owned(), "pixinsight".to_owned(), datetime!(2026-06-01 18:00 UTC), None),
        ];
        let candidates =
            reattribute_candidates(&new_launch, &artifacts, &[], DEFAULT_ATTRIBUTION_WINDOW);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0], "art-1");
    }

    #[test]
    fn reattribute_candidates_upgrades_earlier_launch() {
        let old_launch = launch("launch-old", "pixinsight", datetime!(2026-06-01 08:00 UTC));
        let new_launch = launch("launch-new", "pixinsight", datetime!(2026-06-01 10:00 UTC));
        let artifacts = vec![(
            "art-1".to_owned(),
            "pixinsight".to_owned(),
            datetime!(2026-06-01 11:00 UTC),
            Some("launch-old".to_owned()),
        )];
        let existing_launches = vec![old_launch];
        let candidates = reattribute_candidates(
            &new_launch,
            &artifacts,
            &existing_launches,
            DEFAULT_ATTRIBUTION_WINDOW,
        );
        assert_eq!(candidates.len(), 1);
    }

    #[test]
    fn reattribute_candidates_skips_already_newer_attribution() {
        // artifact already points to a launch newer than new_launch — do not re-attribute.
        let new_launch = launch("launch-new", "pixinsight", datetime!(2026-06-01 10:00 UTC));
        let newer = launch("launch-newer", "pixinsight", datetime!(2026-06-01 12:00 UTC));
        let artifacts = vec![(
            "art-1".to_owned(),
            "pixinsight".to_owned(),
            datetime!(2026-06-01 13:00 UTC),
            Some("launch-newer".to_owned()),
        )];
        let existing_launches = vec![newer];
        let candidates = reattribute_candidates(
            &new_launch,
            &artifacts,
            &existing_launches,
            DEFAULT_ATTRIBUTION_WINDOW,
        );
        assert!(candidates.is_empty());
    }

    /// An artifact recorded as produced by another tool is not a candidate for
    /// rebinding to this tool's launch, even inside the window.
    #[test]
    fn a_foreign_tool_artifact_is_not_a_candidate() {
        let new_launch = launch("launch-new", "pixinsight", datetime!(2026-06-01 10:00 UTC));
        let artifacts =
            vec![("art-1".to_owned(), "siril".to_owned(), datetime!(2026-06-01 11:00 UTC), None)];
        let candidates =
            reattribute_candidates(&new_launch, &artifacts, &[], DEFAULT_ATTRIBUTION_WINDOW);
        assert!(candidates.is_empty(), "a siril artifact must not rebind to a pixinsight launch");
    }
}
