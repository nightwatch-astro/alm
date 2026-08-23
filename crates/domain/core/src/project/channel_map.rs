// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Per-channel integration accounting (spec 008 US4 / task #56).
//!
//! A [`ChannelMap`] maps each filter/channel label to a [`ChannelIntegration`]
//! that aggregates the total frame count and total exposure time for that
//! channel across all linked acquisition sessions.
//!
//! Design rules:
//! - Case-sensitive labels (matches the existing `channels` module decision for
//!   PixInsight workflow compatibility — "Ha" and "HA" are distinct).
//! - `ChannelMap` is pure-domain (no I/O, no DB types).
//! - Building and updating a map is done through [`ChannelMap::add`] /
//!   [`ChannelMap::merge`]; callers supply frame batches as
//!   [`ChannelFrame`] slices.

use std::collections::BTreeMap;

// ── ChannelFrame ──────────────────────────────────────────────────────────────

/// A single frame record used as input to integration accounting.
///
/// Callers typically build these from the persistence layer's session row data
/// before passing a slice to [`ChannelMap::from_frames`] or
/// [`ChannelMap::add`].
#[derive(Clone, Debug, PartialEq)]
pub struct ChannelFrame {
    /// Filter/channel label for this frame (e.g. `"Ha"`, `"OIII"`, `"L"`).
    /// An empty or whitespace-only label is treated as an unfiltered / unknown
    /// channel and keyed as `""` to preserve the frame count.
    pub label: String,
    /// Integration time for this frame in seconds. Must be ≥ 0.0 and finite;
    /// negative and non-finite values contribute 0.0 on insertion, and the
    /// frame is still counted.
    pub exposure_s: f64,
}

impl ChannelFrame {
    /// Convenience constructor.
    #[must_use]
    pub fn new(label: impl Into<String>, exposure_s: f64) -> Self {
        Self { label: label.into(), exposure_s }
    }
}

// ── ChannelIntegration ────────────────────────────────────────────────────────

/// Aggregated integration totals for a single filter/channel.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChannelIntegration {
    /// Total number of frames accumulated for this channel.
    pub frame_count: u64,
    /// Total integration time in seconds across all accumulated frames.
    pub total_exposure_s: f64,
}

impl ChannelIntegration {
    /// Add one frame's exposure to the running totals.
    ///
    /// A non-finite exposure contributes 0.0 and the frame is still counted:
    /// `max(0.0)` neither filters `NaN` reliably nor filters `+inf` at all, so
    /// one such frame would otherwise poison the channel total permanently.
    fn accumulate(&mut self, exposure_s: f64) {
        self.frame_count += 1;
        if exposure_s.is_finite() {
            self.total_exposure_s += exposure_s.max(0.0);
        }
    }
}

// ── ChannelMap ────────────────────────────────────────────────────────────────

/// Per-channel integration map.
///
/// Keyed by filter/channel label. Uses [`BTreeMap`] so iteration order is
/// deterministic and alphabetical — convenient for display and snapshot tests.
///
/// # Examples
///
/// ```rust
/// use domain_core::project::channel_map::{ChannelFrame, ChannelMap};
///
/// let frames = vec![
///     ChannelFrame::new("Ha",   300.0),
///     ChannelFrame::new("Ha",   300.0),
///     ChannelFrame::new("OIII", 600.0),
/// ];
/// let map = ChannelMap::from_frames(&frames);
///
/// let ha = map.get("Ha").unwrap();
/// assert_eq!(ha.frame_count, 2);
/// assert!((ha.total_exposure_s - 600.0).abs() < 1e-9);
///
/// let oiii = map.get("OIII").unwrap();
/// assert_eq!(oiii.frame_count, 1);
/// assert!((oiii.total_exposure_s - 600.0).abs() < 1e-9);
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChannelMap {
    inner: BTreeMap<String, ChannelIntegration>,
}

impl ChannelMap {
    /// Create an empty map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a map from a slice of [`ChannelFrame`] records.
    ///
    /// All frames are accumulated in a single pass. Duplicate labels are merged.
    #[must_use]
    pub fn from_frames(frames: &[ChannelFrame]) -> Self {
        let mut map = Self::new();
        for frame in frames {
            map.add(frame);
        }
        map
    }

    /// Accumulate a single frame into the map.
    pub fn add(&mut self, frame: &ChannelFrame) {
        self.inner.entry(frame.label.clone()).or_default().accumulate(frame.exposure_s);
    }

    /// Look up integration totals for a specific channel label.
    #[must_use]
    pub fn get(&self, label: &str) -> Option<&ChannelIntegration> {
        self.inner.get(label)
    }

    /// Ordered channel labels present in the map.
    ///
    /// Returned in ascending alphabetical order (BTreeMap key order).
    #[must_use]
    pub fn labels(&self) -> Vec<&str> {
        self.inner.keys().map(String::as_str).collect()
    }

    /// Number of distinct channels tracked.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// True when the map contains no channels.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Iterator over `(label, integration)` pairs in ascending label order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &ChannelIntegration)> {
        self.inner.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Merge another map into this one, accumulating totals for shared labels.
    ///
    /// Labels present only in `other` are inserted; labels present in both have
    /// their frame counts and exposures summed.
    pub fn merge(&mut self, other: &Self) {
        for (label, integration) in &other.inner {
            let entry = self.inner.entry(label.clone()).or_default();
            entry.frame_count += integration.frame_count;
            entry.total_exposure_s += integration.total_exposure_s;
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_map_is_empty() {
        let m = ChannelMap::new();
        assert!(m.is_empty());
        assert_eq!(m.len(), 0);
    }

    #[test]
    fn from_frames_aggregates_same_label() {
        let frames = vec![
            ChannelFrame::new("Ha", 300.0),
            ChannelFrame::new("Ha", 300.0),
            ChannelFrame::new("Ha", 300.0),
        ];
        let m = ChannelMap::from_frames(&frames);
        assert_eq!(m.len(), 1);
        let ha = m.get("Ha").unwrap();
        assert_eq!(ha.frame_count, 3);
        assert!((ha.total_exposure_s - 900.0).abs() < 1e-9);
    }

    #[test]
    fn non_finite_exposures_contribute_zero_but_stay_counted() {
        let frames = vec![
            ChannelFrame::new("Ha", 300.0),
            ChannelFrame::new("Ha", f64::NAN),
            ChannelFrame::new("Ha", f64::INFINITY),
        ];
        let ha = ChannelMap::from_frames(&frames).get("Ha").unwrap().clone();
        assert_eq!(ha.frame_count, 3, "an unreadable exposure header must not lose the frame");
        assert!((ha.total_exposure_s - 300.0).abs() < 1e-9);
    }

    #[test]
    fn from_frames_multiple_labels() {
        let frames = vec![
            ChannelFrame::new("Ha", 300.0),
            ChannelFrame::new("OIII", 600.0),
            ChannelFrame::new("SII", 300.0),
            ChannelFrame::new("OIII", 600.0),
        ];
        let m = ChannelMap::from_frames(&frames);
        assert_eq!(m.len(), 3);

        assert_eq!(m.get("Ha").unwrap().frame_count, 1);
        assert!((m.get("Ha").unwrap().total_exposure_s - 300.0).abs() < 1e-9);

        assert_eq!(m.get("OIII").unwrap().frame_count, 2);
        assert!((m.get("OIII").unwrap().total_exposure_s - 1200.0).abs() < 1e-9);

        assert_eq!(m.get("SII").unwrap().frame_count, 1);
    }

    #[test]
    fn labels_are_alphabetically_ordered() {
        let frames = vec![
            ChannelFrame::new("SII", 300.0),
            ChannelFrame::new("Ha", 300.0),
            ChannelFrame::new("OIII", 300.0),
        ];
        let m = ChannelMap::from_frames(&frames);
        assert_eq!(m.labels(), vec!["Ha", "OIII", "SII"]);
    }

    #[test]
    fn case_sensitive_labels() {
        // "Ha" and "HA" are distinct channels (PixInsight workflow rule).
        let frames = vec![ChannelFrame::new("Ha", 300.0), ChannelFrame::new("HA", 300.0)];
        let m = ChannelMap::from_frames(&frames);
        assert_eq!(m.len(), 2);
        assert!(m.get("Ha").is_some());
        assert!(m.get("HA").is_some());
        assert!(m.get("ha").is_none());
    }

    #[test]
    fn get_missing_label_returns_none() {
        let m = ChannelMap::from_frames(&[ChannelFrame::new("Ha", 300.0)]);
        assert!(m.get("OIII").is_none());
    }

    #[test]
    fn negative_exposure_clamped_to_zero() {
        let frames = vec![ChannelFrame::new("Ha", -100.0)];
        let m = ChannelMap::from_frames(&frames);
        let ha = m.get("Ha").unwrap();
        assert_eq!(ha.frame_count, 1);
        assert!((ha.total_exposure_s - 0.0).abs() < 1e-9, "negative exposure should clamp to 0");
    }

    #[test]
    fn empty_label_tracked_as_unfiltered() {
        let frames = vec![ChannelFrame::new("", 300.0), ChannelFrame::new("", 300.0)];
        let m = ChannelMap::from_frames(&frames);
        // Empty label is a valid key (unfiltered/unknown channel).
        let unfiltered = m.get("").unwrap();
        assert_eq!(unfiltered.frame_count, 2);
        assert!((unfiltered.total_exposure_s - 600.0).abs() < 1e-9);
    }

    #[test]
    fn add_incrementally() {
        let mut m = ChannelMap::new();
        m.add(&ChannelFrame::new("Ha", 300.0));
        m.add(&ChannelFrame::new("Ha", 600.0));
        let ha = m.get("Ha").unwrap();
        assert_eq!(ha.frame_count, 2);
        assert!((ha.total_exposure_s - 900.0).abs() < 1e-9);
    }

    #[test]
    fn merge_disjoint_maps() {
        let mut a = ChannelMap::from_frames(&[ChannelFrame::new("Ha", 300.0)]);
        let b = ChannelMap::from_frames(&[ChannelFrame::new("OIII", 600.0)]);
        a.merge(&b);
        assert_eq!(a.len(), 2);
        assert_eq!(a.get("Ha").unwrap().frame_count, 1);
        assert_eq!(a.get("OIII").unwrap().frame_count, 1);
    }

    #[test]
    fn merge_overlapping_maps_sums_totals() {
        let mut a = ChannelMap::from_frames(&[ChannelFrame::new("Ha", 300.0)]);
        let b = ChannelMap::from_frames(&[
            ChannelFrame::new("Ha", 600.0),
            ChannelFrame::new("OIII", 900.0),
        ]);
        a.merge(&b);
        let ha = a.get("Ha").unwrap();
        assert_eq!(ha.frame_count, 2);
        assert!((ha.total_exposure_s - 900.0).abs() < 1e-9);

        let oiii = a.get("OIII").unwrap();
        assert_eq!(oiii.frame_count, 1);
        assert!((oiii.total_exposure_s - 900.0).abs() < 1e-9);
    }

    #[test]
    fn iter_yields_all_entries_in_order() {
        let frames = vec![ChannelFrame::new("SII", 300.0), ChannelFrame::new("Ha", 600.0)];
        let m = ChannelMap::from_frames(&frames);
        let entries: Vec<(&str, u64)> =
            m.iter().map(|(label, ci)| (label, ci.frame_count)).collect();
        // BTreeMap guarantees ascending key order.
        assert_eq!(entries, vec![("Ha", 1), ("SII", 1)]);
    }

    #[test]
    fn merge_self_doubles_totals() {
        let source = ChannelMap::from_frames(&[ChannelFrame::new("Ha", 300.0)]);
        let mut m = source.clone();
        m.merge(&source);
        let ha = m.get("Ha").unwrap();
        assert_eq!(ha.frame_count, 2);
        assert!((ha.total_exposure_s - 600.0).abs() < 1e-9);
    }
}
