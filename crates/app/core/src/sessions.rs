// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Session use cases (spec 029 / spec 037).
//!
//! Real implementations (T037):
//!   `list_sessions` -- backed by `acquisition_session` + `acquisition_fingerprint`.
//!   `get_session`   -- backed by `acquisition_session` joined with related tables
//!                      for calibration_matches and audit history.
//!
//! # Architecture
//!
//! `acquisition_session` stores: id, session_key (pipe-delimited
//! `target|filter|binning|gain|night`, see `parse_session_key`), frame_ids
//! (JSON array), target_id, observer_location (JSON), created_at.
//! `acquisition_fingerprint` stores per-session metadata dimensions for
//! calibration matching (gain, filter, binning, optic_train, etc.).
//!
//! Many contract DTO fields (confidence, metadata, warnings, framesets) have
//! no column yet; defaulted with `// TODO(astro-plan-kyo7.88):` markers until later
//! columns/views are built. `total_integration_seconds` (#775) and
//! `total_size_bytes` are real sums over active frames, not TODOs.
//!
//! Spec 041 FR-051 (T076, Phase 13): sessions are derived, already-confirmed
//! inventory — the `state` column (and the review lifecycle it backed) was
//! removed. `confidence` is now a constant `Confirmed` rather than derived
//! from a review state.
//!
//! Extracted from `app_core` into its own crate (spec 042 / T253 O3b) as a pure
//! leaf: it has zero `crate::` references and nothing else in `app_core`
//! references it. `app_core` re-exports this crate at `app_core::sessions` so the
//! public surface stays byte-identical.
#![allow(clippy::doc_markdown)] // spec/domain terminology not appropriate for backticks

use contracts_core::calibration::CalibrationKind;
use contracts_core::sessions::{
    AcquisitionSession, ConfidenceLevel, Frameset, SessionCalibrationMatch, SessionDetail,
    SessionHistoryEntry, SessionKey,
};
use sqlx::SqlitePool;
use std::collections::HashMap;

// -- Public use-case functions ------------------------------------------------

/// `sessions.list` -- return all acquisition sessions from real DB rows.
///
/// Queries `acquisition_session` and joins `acquisition_fingerprint` for
/// supplementary metadata dimensions. Sessions are ordered by `created_at DESC`.
///
/// # Errors
/// Returns `Err(String)` on database failure.
pub async fn list_sessions(pool: &SqlitePool) -> Result<Vec<AcquisitionSession>, String> {
    list_sessions_paginated(pool, None, None).await
}

/// Paginated `sessions.list` — accepts optional `limit`/`offset`.
///
/// Uses set-based batch queries (fingerprints, frame summaries, exposure, project
/// links) instead of per-row N+1 loops.
///
/// # Errors
/// Returns `Err(String)` on database failure.
pub async fn list_sessions_paginated(
    pool: &SqlitePool,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<AcquisitionSession>, String> {
    use persistence_core::repositories::q_core;

    // 1) Fetch session rows (paginated).
    let rows = q_core::list_sessions_joined_paginated(pool, limit, offset)
        .await
        .map_err(|e| e.to_string())?;

    if rows.is_empty() {
        return Ok(Vec::new());
    }

    // 2) Collect session ids and parse frame_ids once per row.
    let session_ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
    let sessions_frame_ids: Vec<(String, Vec<String>)> = rows
        .iter()
        .map(|r| {
            let ids: Vec<String> = serde_json::from_str(&r.frame_ids).unwrap_or_default();
            (r.id.clone(), ids)
        })
        .collect();

    // 3) Batch-load fingerprints (1 query instead of N).
    let fp_rows =
        q_core::get_fingerprints_batch(pool, &session_ids).await.map_err(|e| e.to_string())?;
    let fingerprints: HashMap<String, Fingerprint> = fp_rows
        .into_iter()
        .map(|r| {
            (
                r.id.clone(),
                Fingerprint {
                    gain: r.gain,
                    filter_name: r.filter_name,
                    binning: r.binning,
                    optic_train: r.optic_train,
                    observing_night_date: r.observing_night_date,
                },
            )
        })
        .collect();

    // 4) Batch-load active frame summaries (1 query instead of N).
    let frame_summaries = q_core::active_frame_summaries_batch(pool, &sessions_frame_ids)
        .await
        .map_err(|e| e.to_string())?;

    // 5) Batch-load exposure seconds (1 query instead of N).
    let exposures = q_core::active_frame_exposure_seconds_batch(pool, &sessions_frame_ids)
        .await
        .map_err(|e| e.to_string())?;

    // 6) Batch-load project ids (1 query instead of N).
    let project_pairs = q_core::project_ids_for_sessions_batch(pool, &session_ids)
        .await
        .map_err(|e| e.to_string())?;
    let mut project_map: HashMap<String, Vec<String>> = HashMap::new();
    for (sid, pid) in project_pairs {
        project_map.entry(sid).or_default().push(pid);
    }

    // 7) Assemble results.
    let mut sessions = Vec::with_capacity(rows.len());
    for row in rows {
        let id = row.id;
        let fp = fingerprints.get(&id);
        let mut sk = parse_session_key(&row.session_key, fp);
        if let Some(name) = row.canonical_target_name.filter(|n| !n.is_empty()) {
            sk.target = name;
        }
        let confidence = ConfidenceLevel::Confirmed;
        let optical_train_id = fp.and_then(|f| f.optic_train.clone()).unwrap_or_default();
        let (count, total) = frame_summaries.get(&id).copied().unwrap_or((0, 0));
        let frame_count = u32::try_from(count.max(0)).unwrap_or(0);
        let total_size_bytes = u64::try_from(total.max(0)).unwrap_or(0);
        let total_integration_seconds = exposures.get(&id).copied().unwrap_or(0.0);
        let metadata = HashMap::new();
        let target_ids = q_core::resolve_session_target_id(row.target_id, row.canonical_target_id)
            .into_iter()
            .collect();
        let project_ids = project_map.remove(&id).unwrap_or_default();
        let warnings = Vec::new();
        sessions.push(AcquisitionSession {
            id,
            session_key: sk,
            confidence,
            optical_train_id,
            frame_count,
            total_integration_seconds,
            total_size_bytes,
            metadata,
            target_ids,
            project_ids,
            warnings,
        });
    }
    Ok(sessions)
}

/// `sessions.get` -- return detail for a single acquisition session.
///
/// Returns `Err("session.not_found: <id>")` when the session does not exist,
/// mirroring the `masters_get` not-found pattern.
///
/// # Errors
/// Returns `Err(String)` on database failure or when the session is absent.
pub async fn get_session(pool: &SqlitePool, id: &str) -> Result<SessionDetail, String> {
    let row = persistence_core::repositories::q_core::get_session_joined(pool, id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("session.not_found: {id}"))?;

    let id = row.id;
    let fp = load_fingerprint(pool, &id).await?;
    let mut sk = parse_session_key(&row.session_key, fp.as_ref());
    if let Some(name) = row.canonical_target_name.filter(|n| !n.is_empty()) {
        sk.target = name;
    }
    // Spec 041 FR-051: sessions are derived, already-confirmed inventory —
    // there is no review state left to derive a confidence level from.
    let confidence = ConfidenceLevel::Confirmed;
    // TODO(astro-plan-kyo7.88): optical_train_id -- fingerprint stores name, not UUID.
    let optical_train_id = fp.as_ref().and_then(|f| f.optic_train.clone()).unwrap_or_default();
    // spec 048 US1: active (non-missing) frame_count/total_size_bytes.
    let (frame_count, total_size_bytes) = active_frame_summary(pool, &row.frame_ids).await?;
    // #775: real sum of active frames' per-file exposure_s (inbox_file_metadata).
    let total_integration_seconds = active_frame_exposure_seconds(pool, &row.frame_ids).await?;
    // TODO(astro-plan-kyo7.88): metadata -- not stored as structured provenance rows yet.
    let metadata = HashMap::new();
    // Shared precedence with q_targets_mgmt::session_counts_by_target — see
    // resolve_session_target_id's doc (reviewer seq=277).
    let target_ids = persistence_core::repositories::q_core::resolve_session_target_id(
        row.target_id,
        row.canonical_target_id,
    )
    .into_iter()
    .collect();
    let project_ids = load_project_ids(pool, &id).await?;
    // TODO(astro-plan-kyo7.88): warnings -- not stored.
    let warnings = Vec::new();
    // Calibration matches from calibration_assignment (real DB rows).
    let calibration_matches = load_calibration_matches(pool, &id).await?;
    // Audit history from audit_log_entry (real DB rows).
    let history = load_history(pool, &id).await?;
    // TODO(astro-plan-kyo7.88): framesets -- requires frame-level data (frame_ids join).
    let framesets: Vec<Frameset> = Vec::new();

    Ok(SessionDetail {
        id,
        session_key: sk,
        confidence,
        optical_train_id,
        frame_count,
        total_integration_seconds,
        total_size_bytes,
        metadata,
        target_ids,
        project_ids,
        warnings,
        framesets,
        calibration_matches,
        history,
    })
}

// -- Private helpers ----------------------------------------------------------

/// Row from `acquisition_fingerprint` for supplementary metadata.
struct Fingerprint {
    gain: Option<f64>,
    filter_name: Option<String>,
    binning: Option<String>,
    optic_train: Option<String>,
    observing_night_date: Option<String>,
}

async fn load_fingerprint(pool: &SqlitePool, id: &str) -> Result<Option<Fingerprint>, String> {
    let row = persistence_core::repositories::q_core::get_fingerprint(pool, id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(row.map(|r| Fingerprint {
        gain: r.gain,
        filter_name: r.filter_name,
        binning: r.binning,
        optic_train: r.optic_train,
        observing_night_date: r.observing_night_date,
    }))
}

/// Parse `SessionKey` from the stored `session_key` string.
///
/// The real/production format (written by `crate::ingest_sessions::
/// derive_session_key` via `sessions::SessionKey::new`, spec 035 US4) is the
/// stable pipe-delimited tuple `target|filter|binning|gain|night` — see
/// `crates/sessions/src/key.rs`; `app_core_projects::source_view_generate::
/// session_night` already relies on this same shape. A handful of
/// pre-035/test-only call sites still write a JSON object
/// (`{"target":...,"filter":...}`); both are accepted so neither format
/// regresses. Missing/blank fields are supplemented from the fingerprint row
/// (legacy spec-006 path), then defaulted to empty string when absent from
/// both.
fn parse_session_key(key: &str, fp: Option<&Fingerprint>) -> SessionKey {
    let (target, filter, binning, gain, night) = if key.trim_start().starts_with('{') {
        parse_session_key_json_fields(key)
    } else {
        parse_session_key_delimited_fields(key)
    };

    let filter =
        non_empty(filter).or_else(|| fp.and_then(|f| f.filter_name.clone())).unwrap_or_default();
    let binning =
        non_empty(binning).or_else(|| fp.and_then(|f| f.binning.clone())).unwrap_or_default();
    let gain = non_empty(gain)
        .or_else(|| fp.and_then(|f| f.gain).map(|n| n.to_string()))
        .unwrap_or_default();
    let night = non_empty(night)
        .or_else(|| fp.and_then(|f| f.observing_night_date.clone()))
        .unwrap_or_default();

    SessionKey { target: target.unwrap_or_default(), filter, binning, gain, night }
}

type SessionKeyFields =
    (Option<String>, Option<String>, Option<String>, Option<String>, Option<String>);

/// Split the real pipe-delimited `target|filter|binning|gain|night` form,
/// via the format's owner (`crates/sessions`).
fn parse_session_key_delimited_fields(key: &str) -> SessionKeyFields {
    let p = sessions::SessionKey::parse(key);
    (p.target, p.filter, p.binning, p.gain, p.night)
}

/// Parse the legacy JSON-object form (`{"target":...}`), pre-035/test-only.
fn parse_session_key_json_fields(json: &str) -> SessionKeyFields {
    let v: serde_json::Value = serde_json::from_str(json).unwrap_or(serde_json::Value::Null);
    let str_field = |key: &str| v.get(key).and_then(|x| x.as_str()).map(ToOwned::to_owned);
    (
        str_field("target"),
        str_field("filter"),
        str_field("binning"),
        str_field("gain"),
        str_field("night"),
    )
}

/// `None` for an absent or blank field, so downstream `or_else` fallbacks
/// (fingerprint) kick in for empty pipe-segments the same way they do for a
/// missing JSON key.
fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|s| !s.is_empty())
}

/// Active `(frame_count, total_size_bytes)` for a session's `frame_ids` JSON
/// array (spec 048 US1, INV-5): only `file_record` rows whose `state !=
/// 'missing'` count toward membership/totals. A `frame_ids` entry with no
/// matching `file_record` (never written, or a future hard-delete) is simply
/// excluded rather than erroring — sessions must never fail to load.
///
/// # Errors
///
/// Returns `Err(String)` on database failure.
async fn active_frame_summary(
    pool: &SqlitePool,
    frame_ids_json: &str,
) -> Result<(u32, u64), String> {
    let ids: Vec<String> = serde_json::from_str(frame_ids_json).unwrap_or_default();
    let (count, total) = persistence_core::repositories::q_core::active_frame_summary(pool, &ids)
        .await
        .map_err(|e| e.to_string())?;
    Ok((u32::try_from(count.max(0)).unwrap_or(0), u64::try_from(total.max(0)).unwrap_or(0)))
}

/// Real total integration seconds for a session's `frame_ids` JSON array
/// (#775): sums each active frame's per-file `exposure_s` from
/// `inbox_file_metadata`. A frame with no reachable metadata row contributes
/// `0.0` rather than erroring — sessions must never fail to load.
///
/// # Errors
///
/// Returns `Err(String)` on database failure.
async fn active_frame_exposure_seconds(
    pool: &SqlitePool,
    frame_ids_json: &str,
) -> Result<f64, String> {
    let ids: Vec<String> = serde_json::from_str(frame_ids_json).unwrap_or_default();
    persistence_core::repositories::q_core::active_frame_exposure_seconds(pool, &ids)
        .await
        .map_err(|e| e.to_string())
}

/// Load project ids linked to a session via `project_sources`.
async fn load_project_ids(pool: &SqlitePool, session_id: &str) -> Result<Vec<String>, String> {
    Ok(persistence_core::repositories::q_core::project_ids_for_session(pool, session_id)
        .await
        .unwrap_or_default())
}

/// Load calibration matches for a session from `calibration_assignment`.
async fn load_calibration_matches(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Vec<SessionCalibrationMatch>, String> {
    let rows =
        persistence_core::repositories::q_core::calibration_matches_for_session(pool, session_id)
            .await
            .unwrap_or_default();

    Ok(rows
        .into_iter()
        .map(|r| {
            // DB CHECK constrains `calibration_type` to dark/flat/bias; unknown
            // values fall back to Dark, preserving prior behavior.
            let kind = r.calibration_type.parse().unwrap_or(CalibrationKind::Dark);
            let soft_mismatches: Vec<String> =
                serde_json::from_str(&r.mismatched_dimensions).unwrap_or_default();
            SessionCalibrationMatch {
                master_id: r.master_id,
                kind,
                score: r.confidence,
                soft_mismatches,
                was_override: r.was_override,
            }
        })
        .collect())
}

/// Load audit history entries for a session from `audit_log_entry`.
async fn load_history(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Vec<SessionHistoryEntry>, String> {
    let rows = persistence_core::repositories::q_core::session_history(pool, session_id)
        .await
        .unwrap_or_default();

    Ok(rows
        .into_iter()
        .map(|r| SessionHistoryEntry { timestamp: r.at, event: r.trigger, actor: r.actor })
        .collect())
}

// -- Tests --------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_session_key_falls_back_to_fingerprint() {
        let fp = Fingerprint {
            gain: Some(100.0),
            filter_name: Some("Ha".to_owned()),
            binning: Some("1x1".to_owned()),
            optic_train: Some("FSQ106".to_owned()),
            observing_night_date: Some("2026-06-01".to_owned()),
        };
        let sk = parse_session_key(r#"{"target":"NGC 7000"}"#, Some(&fp));
        assert_eq!(sk.target, "NGC 7000");
        assert_eq!(sk.filter, "Ha");
        assert_eq!(sk.binning, "1x1");
        assert_eq!(sk.gain, "100");
        assert_eq!(sk.night, "2026-06-01");
    }

    #[test]
    fn parse_session_key_prefers_json_over_fingerprint() {
        let fp = Fingerprint {
            gain: Some(200.0),
            filter_name: Some("OIII".to_owned()),
            binning: Some("2x2".to_owned()),
            optic_train: None,
            observing_night_date: Some("2026-05-01".to_owned()),
        };
        let json =
            r#"{"target":"M31","filter":"Ha","binning":"1x1","gain":"100","night":"2026-04-01"}"#;
        let sk = parse_session_key(json, Some(&fp));
        assert_eq!(sk.filter, "Ha");
        assert_eq!(sk.binning, "1x1");
        assert_eq!(sk.gain, "100");
        assert_eq!(sk.night, "2026-04-01");
    }

    #[test]
    fn parse_session_key_handles_invalid_json() {
        // A `{`-prefixed value that fails to parse as JSON still degrades to
        // empty fields rather than panicking.
        let sk = parse_session_key("{not-json", None);
        assert_eq!(sk.target, "");
        assert_eq!(sk.filter, "");
    }

    /// Real production format (spec 035 US4): `sessions::SessionKey::new` writes
    /// a stable pipe-delimited string, not JSON — see `ingest_sessions::
    /// derive_session_key`. This was the root cause of #564: sessions from
    /// catalogue-ingest had every field but `target` come back empty because
    /// this parser only understood JSON.
    #[test]
    fn parse_session_key_parses_real_pipe_delimited_format() {
        let sk = parse_session_key("M 51|L|1x1|100|2025-05-03", None);
        assert_eq!(sk.target, "M 51");
        assert_eq!(sk.filter, "L");
        assert_eq!(sk.binning, "1x1");
        assert_eq!(sk.gain, "100");
        assert_eq!(sk.night, "2025-05-03");
    }

    #[test]
    fn parse_session_key_delimited_falls_back_to_fingerprint_for_blank_segments() {
        // A resolved canonical-target key has an empty filter/binning/gain
        // segment when the frame's FITS header omitted them; fingerprint
        // supplementation must still kick in exactly as it does for JSON.
        let fp = Fingerprint {
            gain: Some(50.0),
            filter_name: Some("Ha".to_owned()),
            binning: Some("2x2".to_owned()),
            optic_train: None,
            observing_night_date: Some("2026-02-01".to_owned()),
        };
        let sk = parse_session_key("target-uuid-1||||", Some(&fp));
        assert_eq!(sk.target, "target-uuid-1");
        assert_eq!(sk.filter, "Ha");
        assert_eq!(sk.binning, "2x2");
        assert_eq!(sk.gain, "50");
    }
}
