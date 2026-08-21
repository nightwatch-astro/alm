// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Masters list/get + suggest/config-cache tests (T032, T037).

use super::loaders::load_config;
use super::*;
use persistence_core::Database;

use crate::caches;
use crate::caches::cache_test_lock;

async fn test_db() -> Database {
    let db = Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    db
}

/// `masters_list`/`load_config` read through process-global snapshot
/// caches (`caches::calibration_masters`/`calibration_config`), which are
/// also touched by `caches::tests`. Tests that exercise them run
/// concurrently by default under `cargo test`, so without serialization one
/// test's `invalidate`+prime can race another test's assertions on the same
/// static slot — hence the shared `crate::caches::cache_test_lock` (#988;
/// previously a module-private lock here only serialized against sibling
/// tests in *this* module, not `caches::tests`, which is the actual race
/// that made `load_config_reads_require_same_offset_from_tolerances_table`
/// flaky).
async fn lock_cache_tests() -> tokio::sync::MutexGuard<'static, ()> {
    cache_test_lock::lock().await
}

/// T032 / T037: masters_list returns real rows from calibration_master_view.
#[tokio::test]
async fn masters_list_returns_real_rows_not_fixtures() {
    let _guard = lock_cache_tests().await;
    caches::invalidate_calibration_masters();
    let db = test_db().await;

    sqlx::query(
        "INSERT INTO calibration_session (id, session_key, kind, created_at) \
         VALUES ('cal-t1', 'dark-300s', 'dark', '2026-06-01T00:00:00Z')",
    )
    .execute(db.pool())
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO calibration_fingerprint \
         (id, calibration_type, gain, exposure_s, temp_c, binning, optic_train) \
         VALUES ('cal-t1', 'dark', 100.0, 300.0, -10.0, '1x1', 'ASI2600MM')",
    )
    .execute(db.pool())
    .await
    .unwrap();

    let masters = masters_list(db.pool()).await.unwrap();
    assert_eq!(masters.len(), 1, "must return exactly 1 real master from DB");
    assert_eq!(masters[0].id, "cal-t1");
    assert_eq!(masters[0].kind, contracts_core::calibration::CalibrationKind::Dark);
    assert!((masters[0].fingerprint.gain.unwrap() - 100.0).abs() < f64::EPSILON);
    assert_eq!(masters[0].fingerprint.camera.as_deref(), Some("ASI2600MM"));
}

/// T129 (Q16 / FR-136): absent fingerprint/size metadata round-trips as
/// `None`, never a synthesized sentinel (0.0, "", "1x1", or the master's
/// own id standing in for a missing source session).
#[tokio::test]
async fn masters_list_carries_absent_metadata_as_none_not_sentinels() {
    let _guard = lock_cache_tests().await;
    caches::invalidate_calibration_masters();
    let db = test_db().await;

    // Session with NO calibration_fingerprint row at all: every
    // fingerprint field and size_bytes must resolve to None, and
    // source_session_id must not fall back to the master's own id.
    sqlx::query(
        "INSERT INTO calibration_session (id, session_key, kind, created_at) \
         VALUES ('cal-none', 'bias-none', 'bias', '2026-06-02T00:00:00Z')",
    )
    .execute(db.pool())
    .await
    .unwrap();

    let masters = masters_list(db.pool()).await.unwrap();
    assert_eq!(masters.len(), 1);
    let m = &masters[0];
    assert_eq!(m.fingerprint.camera, None);
    assert_eq!(m.fingerprint.gain, None);
    assert_eq!(m.fingerprint.exposure_s, None);
    assert_eq!(m.fingerprint.binning, None);
    assert_eq!(m.source_session_id, None, "must never default to the master's own id");
    assert_eq!(m.size_bytes, None, "must never default to 0");
}

/// #879: a registered camera's user-facing name replaces the raw
/// `optic_train` header string in the master fingerprint. Before this
/// wiring, registered equipment was consumed nowhere outside Settings and
/// the list rendered the FITS header spelling verbatim.
#[tokio::test]
async fn masters_list_renders_registered_camera_name_not_raw_header() {
    let _guard = lock_cache_tests().await;
    caches::invalidate_calibration_masters();
    let db = test_db().await;
    let bus = audit::bus::EventBus::with_pool(db.pool().clone());

    crate::equipment::create_camera(
        db.pool(),
        &bus,
        &contracts_core::equipment::CreateCamera {
            name: "Main Imaging Rig".to_owned(),
            aliases: vec!["ASI2600MM".to_owned()],
            sensor_type: None,
            passband: None,
            pixel_size_um: None,
            sensor_width_px: None,
            sensor_height_px: None,
        },
    )
    .await
    .unwrap();

    insert_master(&db, "cal-named", "ASI2600MM").await;

    let masters = masters_list(db.pool()).await.unwrap();
    assert_eq!(
        masters[0].fingerprint.camera.as_deref(),
        Some("Main Imaging Rig"),
        "registered camera name must replace the raw optic_train header string"
    );
}

/// #879: header spelling varies by capture program, so alias matching
/// ignores case and surrounding whitespace.
#[tokio::test]
async fn masters_list_resolves_camera_name_ignoring_case_and_whitespace() {
    let _guard = lock_cache_tests().await;
    caches::invalidate_calibration_masters();
    let db = test_db().await;
    let bus = audit::bus::EventBus::with_pool(db.pool().clone());

    crate::equipment::create_camera(
        db.pool(),
        &bus,
        &contracts_core::equipment::CreateCamera {
            name: "Main Imaging Rig".to_owned(),
            aliases: vec!["ASI2600MM".to_owned()],
            sensor_type: None,
            passband: None,
            pixel_size_um: None,
            sensor_width_px: None,
            sensor_height_px: None,
        },
    )
    .await
    .unwrap();

    insert_master(&db, "cal-case", "  asi2600mm ").await;

    let masters = masters_list(db.pool()).await.unwrap();
    assert_eq!(masters[0].fingerprint.camera.as_deref(), Some("Main Imaging Rig"));
}

/// #879: an unregistered camera keeps rendering the raw header string —
/// resolution adds names, it never blanks out unknown equipment.
#[tokio::test]
async fn masters_list_falls_back_to_raw_header_for_unregistered_camera() {
    let _guard = lock_cache_tests().await;
    caches::invalidate_calibration_masters();
    let db = test_db().await;

    insert_master(&db, "cal-unreg", "Unregistered Cam").await;

    let masters = masters_list(db.pool()).await.unwrap();
    assert_eq!(masters[0].fingerprint.camera.as_deref(), Some("Unregistered Cam"));
}

/// #879: `masters_get` resolves names on the same rule as `masters_list`,
/// so the detail panel and the table never disagree.
#[tokio::test]
async fn masters_get_renders_registered_camera_name_not_raw_header() {
    let _guard = lock_cache_tests().await;
    caches::invalidate_calibration_masters();
    let db = test_db().await;
    let bus = audit::bus::EventBus::with_pool(db.pool().clone());

    crate::equipment::create_camera(
        db.pool(),
        &bus,
        &contracts_core::equipment::CreateCamera {
            name: "Main Imaging Rig".to_owned(),
            aliases: vec!["ASI2600MM".to_owned()],
            sensor_type: None,
            passband: None,
            pixel_size_um: None,
            sensor_width_px: None,
            sensor_height_px: None,
        },
    )
    .await
    .unwrap();

    insert_master(&db, "cal-detail", "ASI2600MM").await;

    let detail = masters_get(db.pool(), "cal-detail").await.unwrap();
    assert_eq!(detail.fingerprint.camera.as_deref(), Some("Main Imaging Rig"));
}

/// #879: renaming a camera must not serve the previous name out of the
/// process-global masters snapshot.
#[tokio::test]
async fn renaming_a_camera_invalidates_the_cached_master_names() {
    let _guard = lock_cache_tests().await;
    caches::invalidate_calibration_masters();
    let db = test_db().await;
    let bus = audit::bus::EventBus::with_pool(db.pool().clone());

    let camera = crate::equipment::create_camera(
        db.pool(),
        &bus,
        &contracts_core::equipment::CreateCamera {
            name: "Old Name".to_owned(),
            aliases: vec!["ASI2600MM".to_owned()],
            sensor_type: None,
            passband: None,
            pixel_size_um: None,
            sensor_width_px: None,
            sensor_height_px: None,
        },
    )
    .await
    .unwrap();

    insert_master(&db, "cal-rename", "ASI2600MM").await;

    // Prime the snapshot with the pre-rename name.
    let before = masters_list(db.pool()).await.unwrap();
    assert_eq!(before[0].fingerprint.camera.as_deref(), Some("Old Name"));

    crate::equipment::update_camera(
        db.pool(),
        &bus,
        &contracts_core::equipment::UpdateCamera {
            id: camera.id,
            name: "New Name".to_owned(),
            aliases: vec!["ASI2600MM".to_owned()],
            sensor_type: None,
            passband: None,
            pixel_size_um: None,
            sensor_width_px: None,
            sensor_height_px: None,
        },
    )
    .await
    .unwrap();

    let after = masters_list(db.pool()).await.unwrap();
    assert_eq!(
        after[0].fingerprint.camera.as_deref(),
        Some("New Name"),
        "rename must invalidate the cached masters snapshot"
    );
}

/// Insert a dark master whose fingerprint carries `optic_train`.
async fn insert_master(db: &Database, id: &str, optic_train: &str) {
    sqlx::query(
        "INSERT INTO calibration_session (id, session_key, kind, created_at) \
         VALUES (?, 'dark-300s', 'dark', '2026-06-01T00:00:00Z')",
    )
    .bind(id)
    .execute(db.pool())
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO calibration_fingerprint \
         (id, calibration_type, gain, exposure_s, temp_c, binning, optic_train) \
         VALUES (?, 'dark', 100.0, 300.0, -10.0, '1x1', ?)",
    )
    .bind(id)
    .bind(optic_train)
    .execute(db.pool())
    .await
    .unwrap();
}

/// T032 / T037: masters_list returns empty on a fresh DB (no fixtures).
#[tokio::test]
async fn masters_list_returns_empty_on_fresh_db() {
    let _guard = lock_cache_tests().await;
    caches::invalidate_calibration_masters();
    let db = test_db().await;
    let masters = masters_list(db.pool()).await.unwrap();
    assert!(masters.is_empty(), "fresh DB must have no masters — not fixtures");
}

/// T032 / T037: masters_get returns the correct row.
#[tokio::test]
async fn masters_get_returns_correct_row() {
    let _guard = lock_cache_tests().await;
    let db = test_db().await;

    sqlx::query(
        "INSERT INTO calibration_session (id, session_key, kind, created_at) \
         VALUES ('cal-t2', 'flat-2s-Ha', 'flat', '2026-05-15T00:00:00Z')",
    )
    .execute(db.pool())
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO calibration_fingerprint \
         (id, calibration_type, gain, exposure_s, filter_name, binning) \
         VALUES ('cal-t2', 'flat', 100.0, 2.0, 'Ha', '1x1')",
    )
    .execute(db.pool())
    .await
    .unwrap();

    let detail = masters_get(db.pool(), "cal-t2").await.unwrap();
    assert_eq!(detail.id, "cal-t2");
    assert_eq!(detail.kind, contracts_core::calibration::CalibrationKind::Flat);
    assert_eq!(detail.fingerprint.filter, Some("Ha".to_owned()));
}

/// #642: masters_list/masters_get expose `root_id`/`relative_path`
/// resolved from `calibration_session.frame_ids[0]` → `file_record`, the
/// master's own applied frame file written at master-confirm time
/// (`crates/app/inbox/src/plan_listener.rs`).
#[tokio::test]
async fn masters_list_and_get_resolve_frame_path_from_file_record() {
    let _guard = lock_cache_tests().await;
    caches::invalidate_calibration_masters();
    let db = test_db().await;

    sqlx::query(
        "INSERT INTO library_root (id, label, current_path, kind, state, created_at) \
         VALUES ('root-1', 'Library', '/data/lib', 'local', 'active', '2026-06-01T00:00:00Z')",
    )
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO file_record \
         (id, root_id, relative_path, size_bytes, mtime, state, first_seen_at, last_seen_at) \
         VALUES ('fr-1', 'root-1', 'masters/masterDark_300s.xisf', 1000, \
                 '2026-06-01T00:00:00Z', 'observed', '2026-06-01T00:00:00Z', \
                 '2026-06-01T00:00:00Z')",
    )
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO calibration_session (id, session_key, kind, frame_ids, root_id, created_at) \
         VALUES ('cal-path', 'dark-300s', 'dark', '[\"fr-1\"]', 'root-1', '2026-06-01T00:00:00Z')",
    )
    .execute(db.pool())
    .await
    .unwrap();

    let masters = masters_list(db.pool()).await.unwrap();
    assert_eq!(masters.len(), 1);
    assert_eq!(masters[0].root_id.as_deref(), Some("root-1"));
    assert_eq!(masters[0].relative_path.as_deref(), Some("masters/masterDark_300s.xisf"));

    let detail = masters_get(db.pool(), "cal-path").await.unwrap();
    assert_eq!(detail.root_id.as_deref(), Some("root-1"));
    assert_eq!(detail.relative_path.as_deref(), Some("masters/masterDark_300s.xisf"));
}

/// #642: an unresolved master frame (`frame_ids = '[]'`, the common case
/// before spec 048 US1 wired real file-record writes) must leave both
/// fields `None` — never a guessed/empty-string path.
#[tokio::test]
async fn masters_list_leaves_path_none_when_frame_unresolved() {
    let _guard = lock_cache_tests().await;
    caches::invalidate_calibration_masters();
    let db = test_db().await;

    sqlx::query(
        "INSERT INTO calibration_session (id, session_key, kind, created_at) \
         VALUES ('cal-unresolved', 'bias-none', 'bias', '2026-06-02T00:00:00Z')",
    )
    .execute(db.pool())
    .await
    .unwrap();

    let masters = masters_list(db.pool()).await.unwrap();
    assert_eq!(masters.len(), 1);
    assert_eq!(masters[0].root_id, None);
    assert_eq!(masters[0].relative_path, None);
}

/// T032 / T037: masters_get returns error for unknown id.
#[tokio::test]
async fn masters_get_returns_error_for_unknown_id() {
    let _guard = lock_cache_tests().await;
    let db = test_db().await;
    let err = masters_get(db.pool(), "nonexistent").await.unwrap_err();
    assert!(err.contains("master.not_found"), "expected master.not_found error, got: {err}");
}

/// #868: masters_get.compatible_sessions is populated from a real
/// domain-matcher pass over light sessions, not hardcoded to empty.
#[tokio::test]
async fn masters_get_populates_compatible_sessions() {
    let _guard = lock_cache_tests().await;
    let db = test_db().await;

    sqlx::query(
        "INSERT INTO calibration_session (id, session_key, kind, created_at) \
         VALUES ('cal-t3', 'dark-300s', 'dark', '2026-05-15T00:00:00Z')",
    )
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO calibration_fingerprint \
         (id, calibration_type, gain, offset_val, exposure_s, temp_c, binning) \
         VALUES ('cal-t3', 'dark', 100.0, 50.0, 300.0, -10.0, '1x1')",
    )
    .execute(db.pool())
    .await
    .unwrap();

    // Compatible light session: matches every hard-rule dimension.
    sqlx::query(
        "INSERT INTO acquisition_session (id, session_key, created_at) \
         VALUES ('acq-t3', 'M31/L/2026-05-15/300/1x1', '2026-05-15T00:00:00Z')",
    )
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO acquisition_fingerprint \
         (id, session_type, gain, offset_val, exposure_s, temp_c, binning, \
          has_observer_location, has_exposure_start_utc) \
         VALUES ('acq-t3', 'light', 100.0, 50.0, 300.0, -10.0, '1x1', 1, 1)",
    )
    .execute(db.pool())
    .await
    .unwrap();

    // Incompatible light session: gain hard-rule mismatch (dark's hard
    // dimensions are gain + offset, so this must exclude the candidate).
    sqlx::query(
        "INSERT INTO acquisition_session (id, session_key, created_at) \
         VALUES ('acq-t4', 'M31/L/2026-05-16/300/1x1', '2026-05-16T00:00:00Z')",
    )
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO acquisition_fingerprint \
         (id, session_type, gain, offset_val, exposure_s, temp_c, binning) \
         VALUES ('acq-t4', 'light', 200.0, 50.0, 300.0, -10.0, '1x1')",
    )
    .execute(db.pool())
    .await
    .unwrap();

    let detail = masters_get(db.pool(), "cal-t3").await.unwrap();
    let ids: Vec<&str> = detail.compatible_sessions.iter().map(|e| e.session_id.as_str()).collect();
    assert_eq!(ids, vec!["acq-t3"], "only the matching light session should be compatible");
}

/// T032 / T037: calibration suggest finds real masters from populated fingerprints.
#[tokio::test]
async fn suggest_uses_real_fingerprint_rows() {
    let _guard = lock_cache_tests().await;
    caches::invalidate_calibration_masters();
    let db = test_db().await;

    // Insert acquisition session + fingerprint.
    sqlx::query(
        "INSERT INTO acquisition_session (id, session_key, created_at) \
         VALUES ('acq-t1', 'M31/L/2026-03-01/100/1x1', '2026-03-01T00:00:00Z')",
    )
    .execute(db.pool())
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO acquisition_fingerprint \
         (id, session_type, gain, exposure_s, binning, \
          has_observer_location, has_exposure_start_utc) \
         VALUES ('acq-t1', 'light', 100.0, 300.0, '1x1', 0, 0)",
    )
    .execute(db.pool())
    .await
    .unwrap();

    // Insert calibration master fingerprint.
    sqlx::query(
        "INSERT INTO calibration_session (id, session_key, kind, created_at) \
         VALUES ('cal-t3', 'dark-300s-gain100', 'dark', '2026-03-01T00:00:00Z')",
    )
    .execute(db.pool())
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO calibration_fingerprint \
         (id, calibration_type, gain, exposure_s, binning) \
         VALUES ('cal-t3', 'dark', 100.0, 300.0, '1x1')",
    )
    .execute(db.pool())
    .await
    .unwrap();

    // masters_list must return the real row.
    let masters = masters_list(db.pool()).await.unwrap();
    assert_eq!(masters.len(), 1);
    assert_eq!(masters[0].id, "cal-t3");
}

/// In-memory caching layer (F0 follow-up): a `load_config` cache hit must
/// skip the DB entirely, so a `calibration_tolerances` update after the
/// first call is invisible until `invalidate_calibration_config` is
/// called.
#[tokio::test]
async fn load_config_cache_hit_skips_db_until_invalidated() {
    let _guard = lock_cache_tests().await;
    caches::invalidate_calibration_config();
    let db = test_db().await;

    let first = load_config(db.pool()).await;
    assert!(first.require_same_offset, "fresh DB primes the cache with the default (true)");

    let row =
        persistence_calibration::repositories::calibration_tolerances::CalibrationTolerancesRow {
            temperature_tolerance_c: 5.0,
            exposure_tolerance_s: 2.0,
            aging_limit_days: 365,
            require_same_camera: true,
            require_same_gain: true,
            require_same_binning: true,
            require_same_offset: false,
        };
    persistence_calibration::repositories::calibration_tolerances::update(db.pool(), &row)
        .await
        .unwrap();

    let cached = load_config(db.pool()).await;
    assert!(cached.require_same_offset, "cache hit must not see the post-priming update");

    caches::invalidate_calibration_config();
    let fresh = load_config(db.pool()).await;
    assert!(!fresh.require_same_offset, "after invalidation, the update must be visible");
}

/// Spec 043 P8: `load_config` defaults `require_same_offset` to true on a
/// fresh DB, matching `MatchingRuleConfig::default()` (migration 0008/0051
/// seed row).
#[tokio::test]
async fn load_config_defaults_require_same_offset_true() {
    let _guard = lock_cache_tests().await;
    caches::invalidate_calibration_config();
    let db = test_db().await;
    let config = load_config(db.pool()).await;
    assert!(config.require_same_offset);
}

/// In-memory caching layer (F0 follow-up): a `masters_list` cache hit
/// must skip the DB entirely, so a row inserted after the first call is
/// invisible until `invalidate_calibration_masters` is called.
#[tokio::test]
async fn masters_list_cache_hit_skips_db_until_invalidated() {
    let _guard = lock_cache_tests().await;
    caches::invalidate_calibration_masters();
    let db = test_db().await;

    let first = masters_list(db.pool()).await.unwrap();
    assert!(first.is_empty(), "fresh DB primes the cache with an empty snapshot");

    sqlx::query(
        "INSERT INTO calibration_session (id, session_key, kind, created_at) \
         VALUES ('cal-t4', 'dark-60s', 'dark', '2026-06-01T00:00:00Z')",
    )
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO calibration_fingerprint (id, calibration_type, gain, exposure_s, binning) \
         VALUES ('cal-t4', 'dark', 100.0, 60.0, '1x1')",
    )
    .execute(db.pool())
    .await
    .unwrap();

    let cached = masters_list(db.pool()).await.unwrap();
    assert!(cached.is_empty(), "cache hit must not see the row inserted after priming");

    caches::invalidate_calibration_masters();
    let fresh = masters_list(db.pool()).await.unwrap();
    assert_eq!(fresh.len(), 1, "after invalidation, the new row must be visible");
}

/// Spec 043 P8: the Settings > Calibration Matching "Offset match
/// required" toggle persists via `calibration_tolerances` and must feed
/// `MatchingRuleConfig::require_same_offset` on the next `load_config`
/// call — this is the engine-side half of closing the STUB-OFFSET-REQUIRED
/// gap.
///
/// #988: was flaky under `cargo test` before the cache-test lock was shared
/// with `caches::tests` (see [`lock_cache_tests`]) — a concurrently running
/// `caches::tests` round-trip test could invalidate/store the same
/// `CALIBRATION_CONFIG` slot between this test's `invalidate` and its final
/// `load_config` read.
#[tokio::test]
async fn load_config_reads_require_same_offset_from_tolerances_table() {
    let _guard = lock_cache_tests().await;
    caches::invalidate_calibration_config();
    let db = test_db().await;

    let row =
        persistence_calibration::repositories::calibration_tolerances::CalibrationTolerancesRow {
            temperature_tolerance_c: 5.0,
            exposure_tolerance_s: 2.0,
            aging_limit_days: 365,
            require_same_camera: true,
            require_same_gain: true,
            require_same_binning: true,
            require_same_offset: false,
        };
    persistence_calibration::repositories::calibration_tolerances::update(db.pool(), &row)
        .await
        .unwrap();

    let config = load_config(db.pool()).await;
    assert!(!config.require_same_offset, "toggling off must reach MatchingRuleConfig");
}

/// astro-plan-qgyu: the sensor-temperature tolerance the user sets in Settings >
/// Calibration Matching must actually reach the matching engine.
///
/// It did not. The UI wrote `temperature_tolerance_c` to the
/// `calibration_tolerances` row while `load_config_from_db` read only
/// `require_same_offset` from that row and took its temperature from the older
/// `calibrationDarkTempTolerance` settings key. So the row value round-tripped
/// through the DTO — list and update both worked — and never influenced a match.
/// The engine kept its own 2.0 default while the control displayed 5.0.
///
/// A round-trip test cannot catch this: reading back what you wrote passes
/// either way. The assertion has to be against `MatchingRuleConfig`, which is
/// what the ranking rules actually consume.
#[tokio::test]
async fn load_config_reads_temperature_tolerance_from_tolerances_table() {
    let _guard = lock_cache_tests().await;
    caches::invalidate_calibration_config();
    let db = test_db().await;

    // Deliberately not 5.0 (the column default) and not 2.0 (the engine
    // default), so passing cannot be explained by either default surviving.
    let row =
        persistence_calibration::repositories::calibration_tolerances::CalibrationTolerancesRow {
            temperature_tolerance_c: 7.5,
            exposure_tolerance_s: 2.0,
            aging_limit_days: 365,
            require_same_camera: true,
            require_same_gain: true,
            require_same_binning: true,
            require_same_offset: true,
        };
    persistence_calibration::repositories::calibration_tolerances::update(db.pool(), &row)
        .await
        .unwrap();

    let config = load_config(db.pool()).await;
    assert!(
        (config.dark_temp_tolerance_c - 7.5).abs() < f64::EPSILON,
        "the Settings temperature tolerance must reach MatchingRuleConfig; got {} \
         (2.0 means the engine default won, 5.0 means the column default did)",
        config.dark_temp_tolerance_c
    );
}

/// astro-plan-vj6x: a persisted tolerance that is not a usable measurement must
/// not reach the engine.
///
/// `calibration_tolerances::update` binds whatever the caller passed, so a
/// negative or infinite value persists and used to land in `MatchingRuleConfig`
/// unexamined. With `SoftDimConfig::penalty` now rejecting such a tolerance,
/// keeping the value would have quietly turned the temperature dimension into a
/// reject-everything rule, so the loader falls back to the engine default instead.
///
/// A NaN is not in this list: the column is `REAL NOT NULL`, SQLite stores a NaN
/// bind as NULL, and the insert fails on the NOT NULL constraint. That one value
/// cannot reach the loader through this path.
#[tokio::test]
async fn an_unusable_persisted_tolerance_falls_back_to_the_engine_default() {
    for bad in [-3.0, f64::INFINITY, f64::NEG_INFINITY] {
        let _guard = lock_cache_tests().await;
        caches::invalidate_calibration_config();
        let db = test_db().await;

        let row = persistence_calibration::repositories::calibration_tolerances::CalibrationTolerancesRow {
            temperature_tolerance_c: bad,
            exposure_tolerance_s: 2.0,
            aging_limit_days: 365,
            require_same_camera: true,
            require_same_gain: true,
            require_same_binning: true,
            require_same_offset: true,
        };
        persistence_calibration::repositories::calibration_tolerances::update(db.pool(), &row)
            .await
            .unwrap();

        let config = load_config(db.pool()).await;
        assert!(
            (config.dark_temp_tolerance_c - 2.0).abs() < f64::EPSILON,
            "tolerance {bad} reached the engine; got {}",
            config.dark_temp_tolerance_c
        );
    }
}

/// astro-plan-vj6x: an out-of-range override penalty must not reach the engine.
///
/// A penalty is subtracted from a confidence. JSON cannot carry a NaN, so the
/// settings store can only hold a finite number, but nothing stopped it holding
/// `5.0` (confidence below zero) or `-1.0` (confidence above one).
#[tokio::test]
async fn an_out_of_range_persisted_penalty_falls_back_to_the_engine_default() {
    for bad in [-1.0_f64, 5.0] {
        let _guard = lock_cache_tests().await;
        caches::invalidate_calibration_config();
        let db = test_db().await;

        for key in [KEY_DARK_OVERRIDE, KEY_FLAT_OVERRIDE, KEY_BIAS_OVERRIDE] {
            persistence_lifecycle::repositories::settings::set_raw(
                db.pool(),
                key,
                &serde_json::json!(bad),
            )
            .await
            .unwrap();
        }
        persistence_lifecycle::repositories::settings::set_raw(
            db.pool(),
            KEY_DARK_TEMP,
            &serde_json::json!(-1.0),
        )
        .await
        .unwrap();

        let config = load_config(db.pool()).await;
        for (name, got) in [
            ("dark", config.dark_override_penalty),
            ("flat", config.flat_override_penalty),
            ("bias", config.bias_override_penalty),
        ] {
            assert!(
                (got - 0.3).abs() < f64::EPSILON,
                "{name} override penalty {bad} reached the engine; got {got}"
            );
        }
        // 5.0, not the engine's 2.0: the `calibration_tolerances` singleton row
        // always exists, its column default is 5.0, and it is read before this
        // key. Rejecting the key leaves the row value standing.
        assert!(
            (config.dark_temp_tolerance_c - 5.0).abs() < f64::EPSILON,
            "a negative settings-key tolerance reached the engine; got {}",
            config.dark_temp_tolerance_c
        );
    }
}

/// `aging_limit_days` is the second dead-config field on this row
/// (astro-plan-rcvr): editable in Settings > Calibration Matching, read by no
/// rule. Asserting against `MatchingRuleConfig` rather than a round-trip is
/// what distinguishes "persisted" from "consumed".
#[tokio::test]
async fn load_config_reads_aging_limit_days_from_tolerances_table() {
    let _guard = lock_cache_tests().await;
    let db = test_db().await;

    // Neither the column default (365) nor any engine default.
    let mut row =
        persistence_calibration::repositories::calibration_tolerances::CalibrationTolerancesRow {
            temperature_tolerance_c: 5.0,
            exposure_tolerance_s: 2.0,
            aging_limit_days: 30,
            require_same_camera: true,
            require_same_gain: true,
            require_same_binning: true,
            require_same_offset: true,
        };
    persistence_calibration::repositories::calibration_tolerances::update(db.pool(), &row)
        .await
        .unwrap();
    caches::invalidate_calibration_config();
    let config = load_config(db.pool()).await;
    assert!(
        (config.age_limit_days - 30.0).abs() < f64::EPSILON,
        "the Settings age limit must reach MatchingRuleConfig; got {}",
        config.age_limit_days
    );

    row.aging_limit_days = 0;
    persistence_calibration::repositories::calibration_tolerances::update(db.pool(), &row)
        .await
        .unwrap();
    caches::invalidate_calibration_config();
    let config = load_config(db.pool()).await;
    assert!(
        (config.age_limit_days - 365.0).abs() < f64::EPSILON,
        "a zero limit must fall back to the engine default; got {}",
        config.age_limit_days
    );
}

/// astro-plan-rcvr: the "Gain match required" and "Binning match required"
/// toggles must change which masters the engine keeps, not just what the row
/// stores. Both were persisted and read by no rule.
///
/// The assertion runs the rules with the loaded config so a value that reaches
/// `MatchingRuleConfig` but is ignored by the rules still fails.
#[tokio::test]
async fn load_config_relaxes_gain_and_binning_hard_rules_from_tolerances_table() {
    let _guard = lock_cache_tests().await;
    let db = test_db().await;

    let row =
        persistence_calibration::repositories::calibration_tolerances::CalibrationTolerancesRow {
            temperature_tolerance_c: 5.0,
            exposure_tolerance_s: 2.0,
            aging_limit_days: 365,
            require_same_camera: true,
            require_same_gain: false,
            require_same_binning: false,
            require_same_offset: true,
        };
    persistence_calibration::repositories::calibration_tolerances::update(db.pool(), &row)
        .await
        .unwrap();
    caches::invalidate_calibration_config();
    let config = load_config(db.pool()).await;

    let session = calibration_core::SessionInfo {
        id: "ses-rcvr".to_owned(),
        session_type: "light".to_owned(),
        gain: Some(100.0),
        offset: Some(50.0),
        exposure_s: Some(300.0),
        temp_c: Some(-10.0),
        filter: Some("Ha".to_owned()),
        rotation_deg: Some(0.0),
        binning: Some("1x1".to_owned()),
        optic_train: Some("train-a".to_owned()),
        ..Default::default()
    };
    let dark = calibration_core::MasterInfo {
        id: "m-dark-rcvr".to_owned(),
        kind: calibration_core::CalibrationKind::Dark,
        gain: Some(200.0),
        offset: Some(50.0),
        exposure_s: Some(300.0),
        temp_c: Some(-10.0),
        filter: None,
        rotation_deg: None,
        binning: None,
        optic_train: None,
        source_session_id: None,
        observing_night_date: None,
    };
    let flat = calibration_core::MasterInfo {
        id: "m-flat-rcvr".to_owned(),
        kind: calibration_core::CalibrationKind::Flat,
        gain: Some(100.0),
        offset: None,
        exposure_s: None,
        temp_c: None,
        filter: Some("Ha".to_owned()),
        rotation_deg: Some(0.0),
        binning: Some("2x2".to_owned()),
        optic_train: Some("train-a".to_owned()),
        source_session_id: None,
        observing_night_date: None,
    };

    let strict = calibration_core::ranking::MatchingRuleConfig::default();
    assert!(
        calibration_core::rules::dark::evaluate(&session, &dark, &strict).is_none(),
        "the gain mismatch must be excluded while the toggle is set"
    );
    assert!(
        calibration_core::rules::dark::evaluate(&session, &dark, &config).is_some(),
        "clearing Gain match required must keep the mismatched dark"
    );
    assert!(
        calibration_core::rules::flat::evaluate(&session, &flat, &strict).is_none(),
        "the binning mismatch must be excluded while the toggle is set"
    );
    assert!(
        calibration_core::rules::flat::evaluate(&session, &flat, &config).is_some(),
        "clearing Binning match required must keep the mismatched flat"
    );

    // The config cache is process-global: leaving a relaxed config in it makes
    // any later test that reads `load_config` without priming its own row see
    // relaxed hard rules (#988).
    caches::invalidate_calibration_config();
}
