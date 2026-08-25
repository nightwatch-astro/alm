// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Calibration matching engine (spec 007).
//!
//! Pure domain crate — no filesystem reads, no header parsing, no persistence.
//! All inputs arrive as already-extracted metadata.
//!
//! Public surface:
//! - [`CalibrationKind`] — dark / flat / bias (dark_flat reserved, not matched).
//! - [`Dimension`] / [`MatchingRuleConfig`] — rule configuration.
//! - [`CalibrationMatch`] — ranked suggestion with dimension breakdown.
//! - [`SessionInfo`] / [`MasterInfo`] — pure-data input types (no DB row types).
//! - [`suggest`] — fan-out dispatcher for single session.
//! - [`batch_suggest`] — multi-session dispatcher.
//! - [`assign`] module — override/assignment logic.
#![allow(
    clippy::doc_markdown, // spec/domain terminology
    clippy::must_use_candidate, // domain fns are often called for side-effects in tests
)]

pub mod assign;
pub mod candidate;
pub mod families;
pub mod ranking;
pub mod rotation;
pub mod rules;

pub use candidate::{CalibrationMatch, MatchedDim, MismatchReason, MismatchedDim, SelectionReason};
pub use ranking::MatchingRuleConfig;
pub use rotation::{flat_light_rotation_match, RotationMatch, RotationWarning};

// ── Domain enums ──────────────────────────────────────────────────────────────

use serde::{Deserialize, Serialize};

/// Kind of calibration frame.
///
/// `DarkFlat` is reserved for forward-compatibility (FR-001) but MUST NOT be
/// matched, suggested, or assigned in v1. Files landing with dark_flat IMAGETYP
/// are classified as `unclassified` at the inbox level (spec 005).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationKind {
    Dark,
    Flat,
    Bias,
    /// Reserved — not matched or exposed in v1 UI.
    DarkFlat,
}

/// A named matching dimension (metadata field).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dimension {
    Gain,
    Offset,
    Exposure,
    Temperature,
    Filter,
    Rotation,
    Binning,
    OpticTrain,
    /// Physical camera body, keyed on the `CAMERAID` serial rather than the
    /// `INSTRUME` model — two identical bodies share a model but not a serial.
    Camera,
    ObservingNightProximity,
    DateProximity,
}

impl CalibrationKind {
    /// Lowercase string name used in DB and contracts.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Flat => "flat",
            Self::Bias => "bias",
            Self::DarkFlat => "dark_flat",
        }
    }
}

/// Error returned when a string cannot be parsed into a [`CalibrationKind`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseCalibrationKindError(pub String);

impl std::fmt::Display for ParseCalibrationKindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown calibration kind: {}", self.0)
    }
}

impl std::error::Error for ParseCalibrationKindError {}

/// Single canonical, strict parser for [`CalibrationKind`].
///
/// Accepts the canonical serde strings plus the legacy `flat_dark` alias for
/// `DarkFlat`. Unknown values are rejected (no silent fallback); callers apply
/// any fallback explicitly (e.g. `.unwrap_or(CalibrationKind::Dark)` or
/// `.ok()`), keeping the fallback policy visible at each call site.
impl std::str::FromStr for CalibrationKind {
    type Err = ParseCalibrationKindError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "dark" => Ok(Self::Dark),
            "flat" => Ok(Self::Flat),
            "bias" => Ok(Self::Bias),
            "dark_flat" | "flat_dark" => Ok(Self::DarkFlat),
            other => Err(ParseCalibrationKindError(other.to_owned())),
        }
    }
}

impl TryFrom<&str> for CalibrationKind {
    type Error = ParseCalibrationKindError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl Dimension {
    /// Human-readable name (used in dimension breakdown responses).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Gain => "gain",
            Self::Offset => "offset",
            Self::Exposure => "exposure",
            Self::Temperature => "temperature",
            Self::Filter => "filter",
            Self::Rotation => "rotation",
            Self::Binning => "binning",
            Self::OpticTrain => "optic_train",
            Self::Camera => "camera",
            Self::ObservingNightProximity => "observing_night_proximity",
            Self::DateProximity => "date_proximity",
        }
    }
}

/// Metadata for a light session as input to the matcher.
///
/// All fields are optional because metadata extraction may be incomplete.
/// Missing required fields (hard dimensions) exclude candidates; missing soft
/// dimensions reduce confidence via `metadata_missing` mismatch entries.
#[derive(Clone, Debug, Default)]
pub struct SessionInfo {
    pub id: String,
    /// `"light"` | `"dark"` | `"flat"` | `"bias"` | `"mixed"`
    pub session_type: String,
    pub gain: Option<f64>,
    pub offset: Option<f64>,
    pub exposure_s: Option<f64>,
    pub temp_c: Option<f64>,
    pub filter: Option<String>,
    pub rotation_deg: Option<f64>,
    pub binning: Option<String>,
    pub optic_train: Option<String>,
    /// Physical camera body identifier, `None` whenever the headers carried no
    /// serial beyond the `INSTRUME` model. Absent on the majority of cameras,
    /// so it is never required for a match.
    pub camera_body_id: Option<String>,
    /// ISO-8601 date string of the observing night (local date, noon-to-noon).
    pub observing_night_date: Option<String>,
    /// Whether the session has a known observer_location (required for
    /// observing-night semantics; guards A6).
    pub has_observer_location: bool,
    /// Whether exposure_start_utc is populated (required for A6 guard).
    pub has_exposure_start_utc: bool,
}

impl SessionInfo {
    /// True when the session contains mixed frame types (guard E5).
    ///
    /// A mixed session must be split before calibration can be suggested or
    /// assigned; the `session_type == "mixed"` check is the single authoritative
    /// gate shared by `suggest` and `evaluate_assign`.
    #[must_use]
    pub fn is_mixed(&self) -> bool {
        self.session_type == "mixed"
    }
}

/// Metadata for a calibration master as input to the matcher.
///
/// All fingerprint fields optional; hard-rule missing fields cause `metadata_missing` mismatch.
#[derive(Clone, Debug)]
pub struct MasterInfo {
    pub id: String,
    pub kind: CalibrationKind,
    pub gain: Option<f64>,
    pub offset: Option<f64>,
    pub exposure_s: Option<f64>,
    pub temp_c: Option<f64>,
    pub filter: Option<String>,
    pub rotation_deg: Option<f64>,
    pub binning: Option<String>,
    pub optic_train: Option<String>,
    /// Physical camera body identifier; see [`SessionInfo::camera_body_id`].
    pub camera_body_id: Option<String>,
    /// Originating session id (used for same_session selection reason).
    pub source_session_id: Option<String>,
    /// ISO-8601 date string of the observing night of the calibration session.
    pub observing_night_date: Option<String>,
}

// ── Suggest dispatcher ────────────────────────────────────────────────────────

/// Suggest ranked calibration masters for a single light session.
///
/// Returns an error string for hard-guard failures (`session.mixed_state`
/// only — #867 removed the `match.observer_location_missing` hard guard, see
/// the doc comment below). Returns an empty vec when no masters match;
/// callers map that to `"no_match"` status.
///
/// # Errors
/// Returns `Err` with a contract error code string on hard-guard violations.
pub fn suggest(
    session: &SessionInfo,
    masters: &[MasterInfo],
    calibration_types: &[CalibrationKind],
    config: &MatchingRuleConfig,
) -> Result<Vec<CalibrationMatch>, String> {
    // Guard E5: mixed-session check.
    if session.is_mixed() {
        return Err("session.mixed_state".to_owned());
    }

    // Guard A6 (issue #867): missing observer_location/exposure_start_utc no
    // longer hard-blocks suggest. Neither field feeds scoring directly — only
    // `observing_night_date` does, and every rule (see rules::flat) already
    // degrades a missing night to a `metadata_missing` soft mismatch instead
    // of excluding the candidate. A hard reject here made calibration assign
    // end-to-end unreachable for any session without an acquisition
    // fingerprint row yet ("degraded-but-usable" path per #867).

    let types_to_run: Vec<CalibrationKind> = if calibration_types.is_empty() {
        vec![CalibrationKind::Dark, CalibrationKind::Flat, CalibrationKind::Bias]
    } else {
        calibration_types.to_vec()
    };

    let mut all_matches: Vec<CalibrationMatch> = Vec::new();

    for kind in &types_to_run {
        let type_masters: Vec<&MasterInfo> = masters.iter().filter(|m| m.kind == *kind).collect();

        let mut type_matches: Vec<CalibrationMatch> =
            type_masters.iter().filter_map(|m| evaluate_master(session, m, config)).collect();

        ranking::rank_matches(&mut type_matches);
        all_matches.extend(type_matches);
    }

    Ok(all_matches)
}

/// Batch suggest for multiple sessions.
///
/// Returns per-(session, calibration_type) results. Hard-guard failures for
/// individual sessions are represented as per-item status rather than top-level errors.
pub fn batch_suggest(
    sessions: &[SessionInfo],
    masters: &[MasterInfo],
    calibration_types: &[CalibrationKind],
    config: &MatchingRuleConfig,
) -> Vec<BatchSessionResult> {
    sessions
        .iter()
        .map(|session| {
            let result = suggest(session, masters, calibration_types, config);
            BatchSessionResult { session_id: session.id.clone(), result }
        })
        .collect()
}

/// Result for a single session in a batch suggest call.
#[derive(Debug)]
pub struct BatchSessionResult {
    pub session_id: String,
    /// `Ok(matches)` on success; `Err(error_code)` for hard-guard failures.
    pub result: Result<Vec<CalibrationMatch>, String>,
}

// ── Internal dispatch ─────────────────────────────────────────────────────────

fn evaluate_master(
    session: &SessionInfo,
    master: &MasterInfo,
    config: &MatchingRuleConfig,
) -> Option<CalibrationMatch> {
    match master.kind {
        CalibrationKind::Dark => rules::dark::evaluate(session, master, config),
        CalibrationKind::Flat => rules::flat::evaluate(session, master, config),
        CalibrationKind::Bias => rules::bias::evaluate(session, master, config),
        CalibrationKind::DarkFlat => None, // FR-001: never matched in v1
    }
}

pub const CRATE_NAME: &str = "calibration_core";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ranking::MatchingRuleConfig;

    fn default_session() -> SessionInfo {
        SessionInfo {
            id: "ses-001".to_owned(),
            session_type: "light".to_owned(),
            gain: Some(100.0),
            offset: Some(50.0),
            exposure_s: Some(300.0),
            temp_c: Some(-10.0),
            filter: Some("Ha".to_owned()),
            rotation_deg: Some(0.0),
            binning: Some("1x1".to_owned()),
            optic_train: Some("train-a".to_owned()),
            camera_body_id: None,
            observing_night_date: Some("2026-01-15".to_owned()),
            has_observer_location: true,
            has_exposure_start_utc: true,
        }
    }

    fn dark_master(id: &str, gain: f64, offset: f64, exposure_s: f64, temp_c: f64) -> MasterInfo {
        MasterInfo {
            id: id.to_owned(),
            kind: CalibrationKind::Dark,
            gain: Some(gain),
            offset: Some(offset),
            exposure_s: Some(exposure_s),
            temp_c: Some(temp_c),
            filter: None,
            rotation_deg: None,
            binning: None,
            optic_train: None,
            camera_body_id: None,
            source_session_id: None,
            observing_night_date: None,
        }
    }

    #[test]
    fn exposes_crate_name() {
        // Source of truth is Cargo.toml's package name, not a second hand-typed
        // literal in this file — catches CRATE_NAME drifting from the manifest.
        assert_eq!(CRATE_NAME, env!("CARGO_PKG_NAME"));
    }

    #[test]
    fn suggest_rejects_mixed_session() {
        let session = SessionInfo {
            session_type: "mixed".to_owned(),
            has_observer_location: true,
            has_exposure_start_utc: true,
            ..Default::default()
        };
        let result = suggest(&session, &[], &[], &MatchingRuleConfig::default());
        assert_eq!(result.unwrap_err(), "session.mixed_state");
    }

    #[test]
    fn suggest_no_longer_rejects_missing_observer_location() {
        // #867: missing fingerprint fields degrade scoring (via
        // observing_night_date metadata_missing), they no longer hard-block.
        let session = SessionInfo {
            session_type: "light".to_owned(),
            has_observer_location: false,
            has_exposure_start_utc: true,
            ..Default::default()
        };
        let result = suggest(&session, &[], &[], &MatchingRuleConfig::default());
        assert!(result.is_ok(), "missing observer_location must not block suggest");
    }

    #[test]
    fn suggest_no_longer_rejects_missing_exposure_start_utc() {
        let session = SessionInfo {
            session_type: "light".to_owned(),
            has_observer_location: true,
            has_exposure_start_utc: false,
            ..Default::default()
        };
        let result = suggest(&session, &[], &[], &MatchingRuleConfig::default());
        assert!(result.is_ok(), "missing exposure_start_utc must not block suggest");
    }

    #[test]
    fn suggest_finds_matches_without_observer_location() {
        // #867 regression: an unset observer location must not turn into
        // observer_location_missing for every session.
        //
        // This clears the guard flag only; the session keeps its dimensions.
        // A session with NO `acquisition_fingerprint` row at all is a
        // different shape, and no unit fixture can prove the production path
        // writes one — `ingested_session_matches_a_calibration_master` in
        // `app_core`'s `ingest_sessions_integration` covers that
        // (astro-plan-siyk).
        let session = SessionInfo { has_observer_location: false, ..default_session() };
        let master = dark_master("m-001", 100.0, 50.0, 300.0, -10.0);
        let matches = suggest(&session, &[master], &[], &MatchingRuleConfig::default()).unwrap();
        assert!(!matches.is_empty(), "must still find candidate matches");
    }

    #[test]
    fn suggest_returns_empty_when_no_masters() {
        let session = default_session();
        let matches = suggest(&session, &[], &[], &MatchingRuleConfig::default()).unwrap();
        assert!(matches.is_empty());
    }

    #[test]
    fn suggest_skips_dark_flat_masters() {
        let session = default_session();
        let dark_flat_master = MasterInfo {
            id: "m-df".to_owned(),
            kind: CalibrationKind::DarkFlat,
            gain: Some(100.0),
            offset: Some(50.0),
            exposure_s: Some(300.0),
            temp_c: Some(-10.0),
            filter: None,
            rotation_deg: None,
            binning: None,
            optic_train: None,
            camera_body_id: None,
            source_session_id: None,
            observing_night_date: None,
        };
        let matches =
            suggest(&session, &[dark_flat_master], &[], &MatchingRuleConfig::default()).unwrap();
        assert!(matches.is_empty());
    }

    #[test]
    fn suggest_dark_exact_match_returns_high_confidence() {
        let session = default_session();
        let master = dark_master("m-dark-001", 100.0, 50.0, 300.0, -10.0);
        let config = MatchingRuleConfig::default();
        let matches = suggest(&session, &[master], &[CalibrationKind::Dark], &config).unwrap();
        assert_eq!(matches.len(), 1);
        assert!((matches[0].confidence - 1.0).abs() < 1e-9, "exact match should be 1.0");
    }

    #[test]
    fn suggest_dark_gain_mismatch_excluded() {
        let session = default_session();
        let master = dark_master("m-dark-002", 200.0, 50.0, 300.0, -10.0);
        let config = MatchingRuleConfig::default();
        let matches = suggest(&session, &[master], &[CalibrationKind::Dark], &config).unwrap();
        assert!(matches.is_empty(), "gain hard-rule mismatch should exclude");
    }

    #[test]
    fn suggest_calibration_type_filter_respected() {
        let session = default_session();
        let dark = dark_master("m-dark-001", 100.0, 50.0, 300.0, -10.0);
        let config = MatchingRuleConfig::default();
        // Only request flat — dark master should not appear
        let matches = suggest(&session, &[dark], &[CalibrationKind::Flat], &config).unwrap();
        assert!(matches.is_empty());
    }

    // ── Camera-body dimension (astro-plan-ugux2) ──────────────────────────────

    /// Session and master fixtures that agree on every dimension the three
    /// evaluators compare, so only the camera body can change the outcome.
    fn body_pair(
        kind: CalibrationKind,
        session_body: Option<&str>,
        master_body: Option<&str>,
    ) -> (SessionInfo, MasterInfo) {
        let session =
            SessionInfo { camera_body_id: session_body.map(str::to_owned), ..default_session() };
        let master = MasterInfo {
            id: "m-body".to_owned(),
            kind,
            gain: session.gain,
            offset: session.offset,
            exposure_s: session.exposure_s,
            temp_c: session.temp_c,
            filter: session.filter.clone(),
            rotation_deg: session.rotation_deg,
            binning: session.binning.clone(),
            optic_train: session.optic_train.clone(),
            camera_body_id: master_body.map(str::to_owned),
            source_session_id: None,
            observing_night_date: session.observing_night_date.clone(),
        };
        (session, master)
    }

    fn suggest_one(session: &SessionInfo, master: &MasterInfo) -> Vec<CalibrationMatch> {
        suggest(
            session,
            std::slice::from_ref(master),
            &[master.kind],
            &MatchingRuleConfig::default(),
        )
        .unwrap()
    }

    const MATCHABLE_KINDS: [CalibrationKind; 3] =
        [CalibrationKind::Dark, CalibrationKind::Flat, CalibrationKind::Bias];

    /// The defect: a master from body A silently calibrated lights from body B.
    #[test]
    fn two_identical_bodies_with_different_camera_ids_do_not_match() {
        for kind in MATCHABLE_KINDS {
            let (session, master) =
                body_pair(kind, Some("player one_serial-a"), Some("player one_serial-b"));
            assert!(
                suggest_one(&session, &master).is_empty(),
                "{kind:?}: a master from a different body was offered as a candidate"
            );
        }
    }

    /// The majority case the owner called out: most vendors write no usable
    /// `CAMERAID`, so a frame without one must score exactly as it did before the
    /// dimension existed — no entry, no penalty.
    #[test]
    fn frames_without_a_camera_id_match_exactly_as_before() {
        for kind in MATCHABLE_KINDS {
            let (session, master) = body_pair(kind, None, None);
            let matches = suggest_one(&session, &master);
            assert_eq!(matches.len(), 1, "{kind:?}: an unidentified body stopped matching");
            let m = &matches[0];
            assert!(
                (m.confidence - 1.0).abs() < 1e-9,
                "{kind:?}: confidence dropped to {} without any camera evidence",
                m.confidence
            );
            assert!(
                !m.dimensions_matched.iter().any(|d| d.dimension == "camera")
                    && !m.dimensions_mismatched.iter().any(|d| d.dimension == "camera"),
                "{kind:?}: a skipped dimension was still reported"
            );
        }
    }

    /// Two agreeing body ids are recorded as a matched dimension at no cost.
    #[test]
    fn agreeing_camera_ids_match_at_full_confidence() {
        for kind in MATCHABLE_KINDS {
            let (session, master) =
                body_pair(kind, Some("player one_serial-a"), Some("player one_serial-a"));
            let matches = suggest_one(&session, &master);
            assert_eq!(matches.len(), 1, "{kind:?}: agreeing bodies failed to match");
            assert!(
                (matches[0].confidence - 1.0).abs() < 1e-9,
                "{kind:?}: agreement cost confidence"
            );
            assert!(
                matches[0].dimensions_matched.iter().any(|d| d.dimension == "camera"),
                "{kind:?}: the agreement was not recorded"
            );
        }
    }

    /// Present on one side only, both directions: the match stands (a mixed
    /// library must still calibrate) but carries the reduced confidence
    /// constitution II requires of an inferred match.
    #[test]
    fn a_camera_id_on_one_side_only_matches_at_reduced_confidence() {
        let expected = 1.0 - crate::rules::UNKNOWN_CAMERA_BODY_PENALTY;
        for kind in MATCHABLE_KINDS {
            for (session_body, master_body) in
                [(Some("player one_serial-a"), None), (None, Some("player one_serial-a"))]
            {
                let (session, master) = body_pair(kind, session_body, master_body);
                let matches = suggest_one(&session, &master);
                assert_eq!(
                    matches.len(),
                    1,
                    "{kind:?} {session_body:?}/{master_body:?}: a one-sided id must not exclude"
                );
                let m = &matches[0];
                assert!(
                    (m.confidence - expected).abs() < 1e-9,
                    "{kind:?} {session_body:?}/{master_body:?}: confidence {}, expected {expected}",
                    m.confidence
                );
                let entry = m
                    .dimensions_mismatched
                    .iter()
                    .find(|d| d.dimension == "camera")
                    .expect("unproven body agreement must be reported");
                assert_eq!(entry.reason, MismatchReason::MetadataMissing);
            }
        }
    }

    /// The two defects fixed in this area were a NaN delta scored as a match and
    /// a negative delta raising confidence. The camera dimension is string
    /// equality with a constant penalty, so it reports no delta at all and can
    /// only ever lower a score.
    #[test]
    fn the_camera_dimension_carries_no_delta_and_cannot_raise_confidence() {
        let penalty = crate::rules::UNKNOWN_CAMERA_BODY_PENALTY;
        assert!(penalty.is_finite() && penalty > 0.0, "penalty {penalty} cannot lower confidence");

        let (agreed_s, agreed_m) = body_pair(CalibrationKind::Dark, Some("body-a"), Some("body-a"));
        let (partial_s, partial_m) = body_pair(CalibrationKind::Dark, Some("body-a"), None);
        let agreed = suggest_one(&agreed_s, &agreed_m).remove(0);
        let partial = suggest_one(&partial_s, &partial_m).remove(0);

        assert!(
            partial.confidence < agreed.confidence,
            "an unproven body scored {} against a proven {}",
            partial.confidence,
            agreed.confidence
        );
        assert!(
            agreed
                .dimensions_matched
                .iter()
                .chain(partial.dimensions_matched.iter())
                .filter(|d| d.dimension == "camera")
                .all(|d| d.delta.is_none()),
            "a matched camera dimension reported a numeric delta"
        );
        assert!(
            partial
                .dimensions_mismatched
                .iter()
                .filter(|d| d.dimension == "camera")
                .all(|d| d.delta.is_none()),
            "a mismatched camera dimension reported a numeric delta"
        );
    }

    #[test]
    fn batch_suggest_mixed_state_returns_per_item_error() {
        let mixed = SessionInfo {
            id: "ses-mixed".to_owned(),
            session_type: "mixed".to_owned(),
            has_observer_location: true,
            has_exposure_start_utc: true,
            ..Default::default()
        };
        let normal = SessionInfo {
            id: "ses-001".to_owned(),
            session_type: "light".to_owned(),
            has_observer_location: true,
            has_exposure_start_utc: true,
            ..Default::default()
        };
        let results = batch_suggest(&[mixed, normal], &[], &[], &MatchingRuleConfig::default());
        assert_eq!(results.len(), 2);
        assert!(results[0].result.is_err());
        assert!(results[1].result.is_ok());
    }
}
