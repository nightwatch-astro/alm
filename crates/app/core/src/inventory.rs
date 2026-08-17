// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Inventory use cases (spec 006).
//!
//! - `list`: produce the `InventorySource[]` projection with optional filters.
//!
//! # Architecture
//!
//! `list` is a read-only projection: it joins `library_root`,
//! `acquisition_session`, `calibration_session`, and `project_sources` to
//! produce the contract DTOs without storing any new rows.
//!
//! Spec 041 FR-051 (T076, Phase 13): sessions are derived, already-confirmed
//! inventory. The `review_session` use case that wrapped the spec-002
//! `apply_transition` machinery (and its `inventory.session.review` review
//! affordance) was removed along with the review-state column it mutated.
//!
//! Extracted from `app_core` into its own crate (spec 042 / T253 O3b). Its only
//! cross-module dependency is on the now-extracted `app_core_lifecycle` crate
//! (`lifecycle_use_case`, `transition_use_case`); nothing else in `app_core`
//! references it. `app_core` re-exports this crate at `app_core::inventory` so
//! the public surface stays byte-identical.
#![allow(clippy::doc_markdown)] // spec/domain terminology not appropriate for backticks

use std::collections::HashMap;

use app_core_calibration::equipment::resolve_camera_display_name;
use contracts_core::calibration::CalibrationKind;
use contracts_core::equipment::Camera;
use contracts_core::inventory::{
    InventoryFrameType, InventoryLinkedRefs, InventoryListFilters, InventoryProvenanceSummary,
    InventorySession, InventorySource, InventorySourceKind, InventorySourceState, LinkedProjectRef,
    SessionNotesUpdateRequest, SessionNotesUpdateResult,
};
use contracts_core::sessions::SessionCalibrationMatch;
use persistence_core::repositories::q_core::file_records_by_ids;
use persistence_targets::repositories::inventory::{
    list_calibration_matches_for_sessions, list_project_links_for_sessions,
    list_roots_with_sessions, list_session_cameras, list_sessions_for_root, set_session_notes,
    InventoryFilters, SessionCalibrationLinkRow, SessionProjectionRow,
};
use sqlx::SqlitePool;

/// Maximum UTF-8 byte length for a session note (mirrors
/// `target_management::MAX_NOTE_BYTES`, spec 023 US4's FR-004 precedent).
const MAX_NOTE_BYTES: usize = 16_384;

// ── list ─────────────────────────────────────────────────────────────────────

/// Produce the inventory ledger as `InventorySource[]`, applying optional
/// filters server-side.
///
/// # Errors
/// Returns a descriptive `String` on database error.
pub async fn list(
    pool: &SqlitePool,
    filters: Option<InventoryListFilters>,
) -> Result<Vec<InventorySource>, String> {
    let db_filters = filters_to_db(filters.as_ref());

    let roots = list_roots_with_sessions(pool).await.map_err(|e| e.to_string())?;

    // Registered equipment, loaded once for the whole ledger: every root's
    // rows resolve their raw header camera against the same set.
    let cameras =
        app_core_calibration::equipment::list_cameras(pool).await.map_err(|e| e.message)?;

    let mut sources: Vec<InventorySource> = Vec::new();

    for root in &roots {
        // Skip roots not matching a source_filter when set.
        if let Some(ref sf) = db_filters.source_id {
            if &root.id != sf {
                continue;
            }
        }

        let (sessions, has_more) =
            list_sessions_for_root(pool, &root.id, &db_filters).await.map_err(|e| e.to_string())?;

        if sessions.is_empty() {
            // Omit sources that have no visible sessions after filtering.
            continue;
        }

        // Collect session ids for batch project-link + calibration-match lookup.
        let session_ids: Vec<String> = sessions.iter().map(|s| s.id.clone()).collect();
        let project_links =
            list_project_links_for_sessions(pool, &session_ids).await.map_err(|e| e.to_string())?;
        let calibration_links = list_calibration_matches_for_sessions(pool, &session_ids)
            .await
            .map_err(|e| e.to_string())?;

        // Build a map: session_id → Vec<(project_id, project_name)>
        let mut proj_map: HashMap<String, Vec<(String, String)>> = HashMap::new();
        for link in project_links {
            proj_map.entry(link.session_id).or_default().push((link.project_id, link.project_name));
        }

        // Build a map: session_id → Vec<SessionCalibrationMatch> (#772).
        let mut cal_map: HashMap<String, Vec<SessionCalibrationMatch>> = HashMap::new();
        for link in calibration_links {
            cal_map
                .entry(link.session_id.clone())
                .or_default()
                .push(calibration_link_to_match(link));
        }

        // Build a map: session_id → relative frame folder (#567). A session's
        // frames live under one folder; the reveal action joins the root path
        // with this so it opens that folder rather than the library root.
        // Batch-load the first frame of every session in one query (no N+1).
        let folder_map = build_folder_map(pool, &sessions).await?;

        // Build a map: session_id → camera display name (#1343).
        let camera_map = build_camera_map(pool, &session_ids, &cameras).await?;

        let inventory_sessions: Vec<InventorySession> = sessions
            .into_iter()
            .map(|row| project_row_to_session(row, &proj_map, &cal_map, &folder_map, &camera_map))
            .collect();

        sources.push(InventorySource {
            id: root.id.clone(),
            path: root.current_path.clone(),
            kind: map_source_kind(&root.kind),
            state: map_source_state(&root.state),
            sessions: inventory_sessions,
            has_more,
        });
    }

    Ok(sources)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Default per-source session cap applied server-side when the caller omits
/// `limit`. Each session type (acquisition, calibration) is capped
/// independently at the SQL level; see `list_sessions_for_root`.
const DEFAULT_SESSION_LIMIT: u32 = 1_000;

fn filters_to_db(filters: Option<&InventoryListFilters>) -> InventoryFilters {
    let Some(f) = filters else {
        return InventoryFilters { limit: Some(DEFAULT_SESSION_LIMIT), ..Default::default() };
    };
    // Treat limit=0 as unset: the schema says minimum:1 but Rust accepts
    // Some(0) via deserialisation, which would silently empty every page.
    let limit = f.limit.filter(|&n| n > 0).or(Some(DEFAULT_SESSION_LIMIT));
    InventoryFilters {
        source_id: f.source_filter.clone(),
        frame_type: f.frame_filter.map(frame_type_to_str).map(ToOwned::to_owned),
        limit,
        offset: f.offset,
    }
}

fn frame_type_to_str(ft: InventoryFrameType) -> &'static str {
    match ft {
        InventoryFrameType::Light => "light",
        InventoryFrameType::Dark => "dark",
        InventoryFrameType::Flat => "flat",
        InventoryFrameType::Bias => "bias",
    }
}

fn map_source_kind(kind: &str) -> InventorySourceKind {
    match kind {
        "external" => InventorySourceKind::ExternalDisk,
        "network" => InventorySourceKind::NetworkShare,
        _ => InventorySourceKind::LocalDisk,
    }
}

fn map_source_state(state: &str) -> InventorySourceState {
    match state {
        "missing" => InventorySourceState::Missing,
        "disabled" => InventorySourceState::Disabled,
        "reconnect_required" => InventorySourceState::ReconnectRequired,
        _ => InventorySourceState::Active,
    }
}

fn map_frame_type(db_kind: &str) -> InventoryFrameType {
    match db_kind {
        "dark" => InventoryFrameType::Dark,
        "flat" => InventoryFrameType::Flat,
        "bias" => InventoryFrameType::Bias,
        _ => InventoryFrameType::Light,
    }
}

/// Parent folder of a root-relative frame path, or `None` when the frame sits
/// directly at the root (no separator). Handles both `/` and `\` so a
/// Windows-captured relative path resolves the same as a POSIX one.
fn frame_folder(relative_path: &str) -> Option<String> {
    let idx = relative_path.rfind(['/', '\\'])?;
    Some(relative_path[..idx].to_owned())
}

/// Map each session id to its relative frame folder (#567), derived from the
/// parent folder of the session's first frame `file_record`. Sessions whose
/// first frame resolves no `file_record`, or whose frame sits at the root,
/// are simply absent from the map (the UI falls back to the root path).
async fn build_folder_map(
    pool: &SqlitePool,
    sessions: &[SessionProjectionRow],
) -> Result<HashMap<String, String>, String> {
    let first_frames: Vec<(String, String)> = sessions
        .iter()
        .filter_map(|s| s.first_frame_id.clone().map(|fid| (s.id.clone(), fid)))
        .collect();
    let frame_ids: Vec<String> = first_frames.iter().map(|(_, fid)| fid.clone()).collect();

    let rel_by_frame: HashMap<String, String> = file_records_by_ids(pool, &frame_ids)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|r| (r.id, r.relative_path))
        .collect();

    let mut folder_map = HashMap::new();
    for (session_id, fid) in first_frames {
        if let Some(folder) = rel_by_frame.get(&fid).and_then(|rel| frame_folder(rel)) {
            folder_map.insert(session_id, folder);
        }
    }
    Ok(folder_map)
}

/// Map each session id to the camera name shown on its inventory row
/// (#1343): the registered camera's user-facing name when the header string
/// matches one of its aliases, else the raw header string, so an unregistered
/// camera still identifies the gear instead of rendering blank.
///
/// Sessions whose frames carry no camera string are absent from the map.
async fn build_camera_map(
    pool: &SqlitePool,
    session_ids: &[String],
    cameras: &[Camera],
) -> Result<HashMap<String, String>, String> {
    let rows = list_session_cameras(pool, session_ids).await.map_err(|e| e.to_string())?;

    let mut camera_map: HashMap<String, String> = HashMap::new();
    for row in rows {
        // Rows arrive winner-first per session; a later row is a losing
        // camera string for a session already decided.
        if camera_map.contains_key(&row.session_id) {
            continue;
        }
        let display =
            resolve_camera_display_name(cameras, &row.camera).unwrap_or_else(|| row.camera.clone());
        camera_map.insert(row.session_id, display);
    }
    Ok(camera_map)
}

/// `(target, filter, binning, gain, night)`, each `None` for an absent or
/// blank field.
type SessionKeyFields =
    (Option<String>, Option<String>, Option<String>, Option<String>, Option<String>);

/// Parse the stored `session_key` string.
///
/// The real/production format (written by `sessions::SessionKey::new`, spec 035
/// US4) is the stable pipe-delimited tuple `target|filter|binning|gain|night`
/// — see `crates/sessions/src/key.rs`. A handful of pre-035/test-only call
/// sites still write a legacy JSON object (`{"target":...,"filter":...}`);
/// both are accepted so neither format regresses. Mirrors `sessions::
/// SessionKey::parse`'s dual-format handling (#564) — this projection had the
/// exact same JSON-only bug, silently discarding filter/binning/gain/night
/// for every real catalogue-ingested session (every `Sessions` page row
/// showed a generic "Session — <date>" label with blank fields).
fn parse_session_key_fields(key: &str) -> SessionKeyFields {
    // Both branches parse raw; empty→None normalization is applied uniformly
    // below (matching `sessions::SessionKey::parse`'s `non_empty` pass) so a
    // legacy JSON `""` and a blank pipe segment mean the same absent field.
    let (target, filter, binning, gain, night) = if key.trim_start().starts_with('{') {
        let v: serde_json::Value = serde_json::from_str(key).unwrap_or(serde_json::Value::Null);
        let str_field = |k: &str| v.get(k).and_then(|x| x.as_str()).map(ToOwned::to_owned);
        (
            str_field("target"),
            str_field("filter"),
            str_field("binning"),
            str_field("gain"),
            str_field("night"),
        )
    } else {
        let mut parts = key.splitn(5, '|').map(ToOwned::to_owned);
        (parts.next(), parts.next(), parts.next(), parts.next(), parts.next())
    };
    let non_empty = |s: Option<String>| s.filter(|s| !s.is_empty());
    (non_empty(target), non_empty(filter), non_empty(binning), non_empty(gain), non_empty(night))
}

/// Parse one `calibration_assignment` row into the contract's match DTO.
/// The DB `CHECK` constrains `calibration_type` to dark/flat/bias; an
/// unrecognized value falls back to `Dark`, matching
/// `sessions::load_calibration_matches`'s existing tolerance.
fn calibration_link_to_match(row: SessionCalibrationLinkRow) -> SessionCalibrationMatch {
    let kind = row.calibration_type.parse().unwrap_or(CalibrationKind::Dark);
    let soft_mismatches: Vec<String> =
        serde_json::from_str(&row.mismatched_dimensions).unwrap_or_default();
    SessionCalibrationMatch {
        master_id: row.master_id,
        kind,
        score: row.confidence,
        soft_mismatches,
        was_override: row.was_override,
    }
}

fn project_row_to_session(
    row: SessionProjectionRow,
    proj_map: &HashMap<String, Vec<(String, String)>>,
    cal_map: &HashMap<String, Vec<SessionCalibrationMatch>>,
    folder_map: &HashMap<String, String>,
    camera_map: &HashMap<String, String>,
) -> InventorySession {
    let frames = u32::try_from(row.frame_count).unwrap_or(0);
    let relative_path = folder_map.get(&row.id).cloned();
    let frame_type = map_frame_type(&row.frame_type);

    // Parse session_key once — replaces 3 redundant parses via
    // effective_target + derive_session_name + inline destructure.
    let (parsed_target, filter, binning, gain, night) = parse_session_key_fields(&row.session_key);
    let target = row.target_name.clone().or(parsed_target);

    let date = &row.created_at[..10.min(row.created_at.len())];
    let name = if row.session_kind == "calibration" {
        format!("{} calibration — {date}", row.frame_type)
    } else {
        match target.as_deref() {
            Some(t) => {
                let f = filter.as_deref().unwrap_or("?");
                let n = night.as_deref().unwrap_or(date);
                format!("{t} · {f} — {n}")
            }
            None => format!("Session — {date}"),
        }
    };

    let linked = proj_map.get(&row.id).map(|projs| InventoryLinkedRefs {
        projects: Some(
            projs
                .iter()
                .map(|(id, name)| LinkedProjectRef { id: id.clone(), name: name.clone() })
                .collect(),
        ),
        session: None,
        calibration: None,
    });

    // Provenance summary: derive from session_key metadata where available.
    let provenance = if target.is_some() || filter.is_some() {
        Some(InventoryProvenanceSummary {
            target: target.clone(),
            filter: filter.clone(),
            ..Default::default()
        })
    } else {
        None
    };

    // Night (#564/#567): the observing night parsed from session_key —
    // computed by `sessions::observing_night` at ingest time, distinct from
    // `created_at` (when the row was inserted, e.g. a later re-scan). Falls
    // back to the ingest date only when the key carries no night segment
    // (legacy rows / calibration sessions, which have no observing night).
    let captured_on = night.or_else(|| {
        if row.created_at.len() >= 10 {
            Some(row.created_at[..10].to_owned())
        } else {
            None
        }
    });

    // No exposure in session_key; would come from the fingerprint/provenance
    // join in a full implementation (TODO(astro-plan-kyo7.88)).
    let exposure = None;

    let camera = camera_map.get(&row.id).cloned();

    let calibration_matches = cal_map.get(&row.id).cloned().unwrap_or_default();

    InventorySession {
        id: row.id,
        name,
        source_id: row.root_id,
        frames,
        frame_type,
        target,
        filter,
        exposure,
        camera,
        gain,
        binning,
        set_temp: None,
        captured_on,
        provenance,
        linked,
        relative_path,
        notes: row.notes,
        calibration_matches,
    }
}

// ── session.notes.update ─────────────────────────────────────────────────────

/// `inventory.session.notes.update` — write post-hoc notes for an inventory
/// session (#773). Empty/whitespace-only `notes` clears the field (stores
/// `NULL`), mirroring `target_management::note_update`'s FR-004 precedent:
/// notes exceeding 16 384 UTF-8 bytes (after trimming) are rejected.
///
/// # Errors
/// Returns `Err("note.content_too_large: ...")` when the trimmed note
/// exceeds the byte limit, `Err("session.not_found: <id>")` when
/// `session_id` matches neither session table, or a database error string.
pub async fn update_session_notes(
    pool: &SqlitePool,
    req: &SessionNotesUpdateRequest,
) -> Result<SessionNotesUpdateResult, String> {
    let trimmed = req.notes.trim();
    if trimmed.len() > MAX_NOTE_BYTES {
        return Err(format!(
            "note.content_too_large: note body exceeds the {MAX_NOTE_BYTES}-byte limit \
             ({} bytes supplied)",
            trimmed.len()
        ));
    }
    let stored: Option<&str> = if trimmed.is_empty() { None } else { Some(trimmed) };

    let updated =
        set_session_notes(pool, &req.session_id, stored).await.map_err(|e| e.to_string())?;
    if !updated {
        return Err(format!("session.not_found: {}", req.session_id));
    }

    Ok(SessionNotesUpdateResult { notes: stored.map(ToOwned::to_owned) })
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_source_kind_local() {
        assert!(matches!(map_source_kind("local"), InventorySourceKind::LocalDisk));
    }

    #[test]
    fn map_source_kind_external() {
        assert!(matches!(map_source_kind("external"), InventorySourceKind::ExternalDisk));
    }

    #[test]
    fn map_source_kind_network() {
        assert!(matches!(map_source_kind("network"), InventorySourceKind::NetworkShare));
    }

    #[test]
    fn map_source_kind_unknown_defaults_to_local() {
        assert!(matches!(map_source_kind("usb"), InventorySourceKind::LocalDisk));
    }

    #[test]
    fn map_source_state_variants() {
        assert!(matches!(map_source_state("active"), InventorySourceState::Active));
        assert!(matches!(map_source_state("missing"), InventorySourceState::Missing));
        assert!(matches!(map_source_state("disabled"), InventorySourceState::Disabled));
        assert!(matches!(
            map_source_state("reconnect_required"),
            InventorySourceState::ReconnectRequired
        ));
    }

    #[test]
    fn map_frame_type_variants() {
        assert!(matches!(map_frame_type("light"), InventoryFrameType::Light));
        assert!(matches!(map_frame_type("dark"), InventoryFrameType::Dark));
        assert!(matches!(map_frame_type("flat"), InventoryFrameType::Flat));
        assert!(matches!(map_frame_type("bias"), InventoryFrameType::Bias));
        // "mixed" no longer maps to a dedicated variant (removed 2026-07-03);
        // any unknown db_kind, including a legacy "mixed", falls back to Light.
        assert!(matches!(map_frame_type("mixed"), InventoryFrameType::Light));
    }

    // count_frames / first_frame_id Rust helpers were removed: frame_count and
    // first_frame_id are now computed by SQLite (json_array_length /
    // json_extract) and returned as typed columns in SessionProjectionRow.
    // Parity is verified by the DB-level integration tests in persistence_db.

    // ── parse_session_key_fields (#564 backend half) ──────────────────────────

    #[test]
    fn parse_session_key_fields_parses_real_pipe_delimited_format() {
        let (target, filter, binning, gain, night) =
            parse_session_key_fields("M 51|L|1x1|100|2025-05-03");
        assert_eq!(target.as_deref(), Some("M 51"));
        assert_eq!(filter.as_deref(), Some("L"));
        assert_eq!(binning.as_deref(), Some("1x1"));
        assert_eq!(gain.as_deref(), Some("100"));
        assert_eq!(night.as_deref(), Some("2025-05-03"));
    }

    #[test]
    fn parse_session_key_fields_blank_segments_become_none() {
        let (target, filter, binning, gain, night) = parse_session_key_fields("M 51||||");
        assert_eq!(target.as_deref(), Some("M 51"));
        assert!(filter.is_none());
        assert!(binning.is_none());
        assert!(gain.is_none());
        assert!(night.is_none());
    }

    #[test]
    fn parse_session_key_fields_accepts_legacy_json_form() {
        let (target, filter, ..) =
            parse_session_key_fields(r#"{"target":"NGC 7000","filter":"Ha"}"#);
        assert_eq!(target.as_deref(), Some("NGC 7000"));
        assert_eq!(filter.as_deref(), Some("Ha"));
    }

    #[test]
    fn parse_session_key_fields_invalid_json_degrades_to_none() {
        let (target, filter, ..) = parse_session_key_fields("{not-json");
        assert!(target.is_none());
        assert!(filter.is_none());
    }

    #[test]
    fn parse_session_key_fields_json_empty_strings_become_none_like_pipe_form() {
        let (target, filter, binning, gain, night) =
            parse_session_key_fields(r#"{"target":"M 51","filter":"","gain":""}"#);
        assert_eq!(target.as_deref(), Some("M 51"));
        assert!(filter.is_none(), "JSON \"\" must normalize like a blank pipe segment");
        assert!(binning.is_none());
        assert!(gain.is_none());
        assert!(night.is_none());
    }

    // ── list() session_key wiring + notes + calibration matches ──────────────

    async fn setup() -> persistence_core::Database {
        let db = persistence_core::Database::in_memory().await.expect("in-memory DB");
        db.migrate().await.expect("migrations");
        db
    }

    #[tokio::test]
    async fn list_resolves_target_filter_binning_gain_night_from_pipe_delimited_key() {
        let db = setup().await;
        let pool = db.pool();
        sqlx::query(
            "INSERT INTO library_root (id, label, kind, current_path, state, created_at) \
             VALUES ('root-1', 'Lib', 'local', '/lib', 'active', '2026-07-14T00:00:00Z')",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO acquisition_session (id, session_key, root_id, frame_ids, created_at) \
             VALUES ('acq-1', 'M 51|L|1x1|100|2025-05-03', 'root-1', '[\"f1\",\"f2\"]', \
                     '2026-07-14T00:00:00Z')",
        )
        .execute(pool)
        .await
        .unwrap();

        let sources = list(pool, None).await.unwrap();
        let session = &sources[0].sessions[0];

        assert_eq!(session.target.as_deref(), Some("M 51"));
        assert_eq!(session.filter.as_deref(), Some("L"));
        assert_eq!(session.binning.as_deref(), Some("1x1"));
        assert_eq!(session.gain.as_deref(), Some("100"));
        // Night from session_key wins over the ingest created_at date (#564/#567).
        assert_eq!(session.captured_on.as_deref(), Some("2025-05-03"));
    }

    #[tokio::test]
    async fn list_surfaces_notes_and_calibration_matches() {
        let db = setup().await;
        let pool = db.pool();
        sqlx::query(
            "INSERT INTO library_root (id, label, kind, current_path, state, created_at) \
             VALUES ('root-2', 'Lib', 'local', '/lib', 'active', '2026-07-14T00:00:00Z')",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO acquisition_session (id, session_key, root_id, frame_ids, created_at, notes) \
             VALUES ('acq-2', 'M 31|L|1x1|100|2026-01-01', 'root-2', '[\"f1\"]', \
                     '2026-07-14T00:00:00Z', 'Great seeing.')",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO calibration_assignment \
                (id, session_id, calibration_type, master_id, confidence, mismatched_dimensions, assigned_at) \
             VALUES ('ca-1', 'acq-2', 'dark', 'master-dark-1', 0.9, '[\"gain\"]', '2026-07-14T00:00:00Z')",
        )
        .execute(pool)
        .await
        .unwrap();

        let sources = list(pool, None).await.unwrap();
        let session = &sources[0].sessions[0];

        assert_eq!(session.notes.as_deref(), Some("Great seeing."));
        assert_eq!(session.calibration_matches.len(), 1);
        assert_eq!(session.calibration_matches[0].master_id, "master-dark-1");
        assert!(matches!(session.calibration_matches[0].kind, CalibrationKind::Dark));
        assert_eq!(session.calibration_matches[0].soft_mismatches, vec!["gain".to_owned()]);
    }

    #[tokio::test]
    async fn list_session_with_no_calibration_assignment_has_empty_matches() {
        let db = setup().await;
        let pool = db.pool();
        sqlx::query(
            "INSERT INTO library_root (id, label, kind, current_path, state, created_at) \
             VALUES ('root-3', 'Lib', 'local', '/lib', 'active', '2026-07-14T00:00:00Z')",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO acquisition_session (id, session_key, root_id, frame_ids, created_at) \
             VALUES ('acq-3', 'M 42|Ha|1x1|100|2026-01-01', 'root-3', '[\"f1\"]', \
                     '2026-07-14T00:00:00Z')",
        )
        .execute(pool)
        .await
        .unwrap();

        let sources = list(pool, None).await.unwrap();
        let session = &sources[0].sessions[0];
        assert!(session.calibration_matches.is_empty());
        assert!(session.notes.is_none());
    }

    // ── provenance summary (spec 006 T204) ───────────────────────────────────

    /// A session key carrying neither target nor filter yields no provenance
    /// summary at all, rather than an all-`None` object. The UI relies on the
    /// absent object to skip the provenance fact rows entirely.
    #[tokio::test]
    async fn list_session_without_target_or_filter_has_no_provenance_summary() {
        let db = setup().await;
        let pool = db.pool();
        sqlx::query(
            "INSERT INTO library_root (id, label, kind, current_path, state, created_at) \
             VALUES ('root-np', 'Lib', 'local', '/lib', 'active', '2026-07-14T00:00:00Z')",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO calibration_session (id, session_key, kind, root_id, frame_ids, created_at) \
             VALUES ('cal-np', '||1x1|100|2026-01-01', 'dark', 'root-np', '[\"f1\"]', \
                     '2026-07-14T00:00:00Z')",
        )
        .execute(pool)
        .await
        .unwrap();

        let sources = list(pool, None).await.unwrap();
        let session = &sources[0].sessions[0];
        assert!(session.target.is_none());
        assert!(session.filter.is_none());
        assert!(
            session.provenance.is_none(),
            "provenance must be absent when neither target nor filter is known"
        );
        // Binning/gain are still projected — they are not provenance inputs.
        assert_eq!(session.binning.as_deref(), Some("1x1"));
    }

    /// The populated branch mirrors the same rule: `target`/`filter` are the
    /// only fields the projection can derive. `inferred`/`confirmed_by` have no
    /// source of truth (no writer populates `provenance_history_archive`, and
    /// spec 041 FR-051 dropped the session review lifecycle), so they stay
    /// `None` — see the T203 follow-up.
    #[tokio::test]
    async fn list_provenance_summary_carries_target_and_filter_only() {
        let db = setup().await;
        let pool = db.pool();
        sqlx::query(
            "INSERT INTO library_root (id, label, kind, current_path, state, created_at) \
             VALUES ('root-pv', 'Lib', 'local', '/lib', 'active', '2026-07-14T00:00:00Z')",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO acquisition_session (id, session_key, root_id, frame_ids, created_at) \
             VALUES ('acq-pv', 'M 51|L|1x1|100|2025-05-03', 'root-pv', '[\"f1\"]', \
                     '2026-07-14T00:00:00Z')",
        )
        .execute(pool)
        .await
        .unwrap();

        let sources = list(pool, None).await.unwrap();
        let prov = sources[0].sessions[0].provenance.as_ref().expect("provenance summary");
        assert_eq!(prov.target.as_deref(), Some("M 51"));
        assert_eq!(prov.filter.as_deref(), Some("L"));
        assert!(prov.inferred.is_none());
        assert!(prov.confirmed_by.is_none());
    }

    #[tokio::test]
    async fn list_resolves_relative_frame_folder_from_first_frame() {
        let db = setup().await;
        let pool = db.pool();
        sqlx::query(
            "INSERT INTO library_root (id, label, kind, current_path, state, created_at) \
             VALUES ('root-rp', 'Lib', 'local', '/lib', 'active', '2026-07-14T00:00:00Z')",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO file_record \
                (id, root_id, relative_path, size_bytes, mtime, state, first_seen_at, last_seen_at) \
             VALUES ('f1', 'root-rp', 'M 51/2025-05-03/light_001.fits', 100, '2026-07-14T00:00:00Z', \
                     'observed', '2026-07-14T00:00:00Z', '2026-07-14T00:00:00Z')",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO acquisition_session (id, session_key, root_id, frame_ids, created_at) \
             VALUES ('acq-rp', 'M 51|L|1x1|100|2025-05-03', 'root-rp', '[\"f1\"]', \
                     '2026-07-14T00:00:00Z')",
        )
        .execute(pool)
        .await
        .unwrap();

        let sources = list(pool, None).await.unwrap();
        let session = &sources[0].sessions[0];
        assert_eq!(session.relative_path.as_deref(), Some("M 51/2025-05-03"));
    }

    #[test]
    fn frame_folder_derives_parent_and_handles_root_level() {
        assert_eq!(frame_folder("a/b/c.fits").as_deref(), Some("a/b"));
        assert_eq!(frame_folder("a\\b\\c.fits").as_deref(), Some("a\\b"));
        assert!(frame_folder("c.fits").is_none());
    }

    /// Documents the first-frame-folder heuristic: when a session's frames
    /// span multiple folders, `relativePath` is the FIRST frame's folder —
    /// good enough for the reveal action (#567), which needs one anchor
    /// folder, not a spanning set.
    #[tokio::test]
    async fn list_relative_folder_uses_first_frame_when_frames_span_folders() {
        let db = setup().await;
        let pool = db.pool();
        sqlx::query(
            "INSERT INTO library_root (id, label, kind, current_path, state, created_at) \
             VALUES ('root-mf', 'Lib', 'local', '/lib', 'active', '2026-07-14T00:00:00Z')",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO file_record \
                (id, root_id, relative_path, size_bytes, mtime, state, first_seen_at, last_seen_at) \
             VALUES \
                ('mf1', 'root-mf', 'night1\\M 42\\light_001.fits', 100, '2026-07-14T00:00:00Z', \
                 'observed', '2026-07-14T00:00:00Z', '2026-07-14T00:00:00Z'), \
                ('mf2', 'root-mf', 'night2/M 42/light_002.fits', 100, '2026-07-14T00:00:00Z', \
                 'observed', '2026-07-14T00:00:00Z', '2026-07-14T00:00:00Z')",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO acquisition_session (id, session_key, root_id, frame_ids, created_at) \
             VALUES ('acq-mf', 'M 42|Ha|1x1|100|2026-01-01', 'root-mf', '[\"mf1\",\"mf2\"]', \
                     '2026-07-14T00:00:00Z')",
        )
        .execute(pool)
        .await
        .unwrap();

        let sources = list(pool, None).await.unwrap();
        let session = &sources[0].sessions[0];
        // First frame wins (mf1, a Windows-style path) — mf2's folder is ignored.
        assert_eq!(session.relative_path.as_deref(), Some("night1\\M 42"));
    }

    // ── update_session_notes (#773) ───────────────────────────────────────────

    #[tokio::test]
    async fn update_session_notes_roundtrips() {
        let db = setup().await;
        let pool = db.pool();
        sqlx::query(
            "INSERT INTO acquisition_session (id, session_key, frame_ids, created_at) \
             VALUES ('acq-notes', 'k', '[]', '2026-01-01T00:00:00Z')",
        )
        .execute(pool)
        .await
        .unwrap();

        let req = SessionNotesUpdateRequest {
            session_id: "acq-notes".to_owned(),
            notes: "  Great seeing.  ".to_owned(),
        };
        let result = update_session_notes(pool, &req).await.unwrap();
        // Stored value is trimmed.
        assert_eq!(result.notes.as_deref(), Some("Great seeing."));
    }

    #[tokio::test]
    async fn update_session_notes_blank_clears() {
        let db = setup().await;
        let pool = db.pool();
        sqlx::query(
            "INSERT INTO acquisition_session (id, session_key, frame_ids, created_at, notes) \
             VALUES ('acq-clear', 'k', '[]', '2026-01-01T00:00:00Z', 'old')",
        )
        .execute(pool)
        .await
        .unwrap();

        let req = SessionNotesUpdateRequest {
            session_id: "acq-clear".to_owned(),
            notes: "   ".to_owned(),
        };
        let result = update_session_notes(pool, &req).await.unwrap();
        assert!(result.notes.is_none());
    }

    #[tokio::test]
    async fn update_session_notes_unknown_session_errors() {
        let db = setup().await;
        let pool = db.pool();
        let req = SessionNotesUpdateRequest {
            session_id: "no-such-id".to_owned(),
            notes: "x".to_owned(),
        };
        let err = update_session_notes(pool, &req).await.unwrap_err();
        assert!(err.starts_with("session.not_found:"), "got: {err}");
    }

    #[tokio::test]
    async fn update_session_notes_rejects_oversized_body() {
        let db = setup().await;
        let pool = db.pool();
        sqlx::query(
            "INSERT INTO acquisition_session (id, session_key, frame_ids, created_at) \
             VALUES ('acq-big', 'k', '[]', '2026-01-01T00:00:00Z')",
        )
        .execute(pool)
        .await
        .unwrap();

        let oversized = "x".repeat(MAX_NOTE_BYTES + 1);
        let req = SessionNotesUpdateRequest { session_id: "acq-big".to_owned(), notes: oversized };
        let err = update_session_notes(pool, &req).await.unwrap_err();
        assert!(err.starts_with("note.content_too_large:"), "got: {err}");
    }
}
