// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `calibration_master_view` degrades a malformed `calibration_session.frame_ids`
//! to a NULL path instead of failing every read of the view.
//!
//! `frame_ids` is `TEXT NOT NULL DEFAULT '[]'` with no `json_valid` CHECK, so a
//! non-JSON value is storable. An unguarded `json_extract` in the view's
//! `LEFT JOIN file_record` raises "malformed JSON" and aborts the whole SELECT,
//! taking down `calibration.masters.list` and `.get`.

use sqlx::{Row, SqlitePool};

use persistence_core::Database;

/// The projection of `persistence_calibration::repositories::q_calibration`'s
/// `list_calibration_masters`. Reproduced rather than called because
/// `persistence_calibration` depends on this crate, not the reverse.
///
/// The column set is load-bearing: selecting `id` alone lets SQLite prune the
/// guarded `LEFT JOIN file_record` away, and the test then passes against an
/// unguarded view. `frame_relative_path` is the column that forces the join —
/// and with it the `json_extract` — to evaluate.
const MASTER_VIEW_COLUMNS: &str = "id, kind, created_at, size_bytes, \
     fp_gain, fp_exposure_s, fp_temp_c, fp_filter_name, fp_binning, \
     fp_optic_train, source_session_id, root_id, frame_relative_path, \
     archived_at, archived_via_plan_id";

async fn migrated() -> Database {
    let db = Database::in_memory().await.expect("in-memory database");
    db.migrate().await.expect("baseline applies cleanly");
    db
}

/// Seeds the frame the intact master resolves to. The view's `LEFT JOIN` scans
/// `calibration_session` either way, so this proves the intact row keeps its
/// real path rather than merely surviving.
async fn seed_frame(pool: &SqlitePool) {
    sqlx::query(
        "INSERT INTO library_root (id, label, current_path, kind, state, created_at)
         VALUES ('root-1', 'Main', '/lib', 'local', 'active', '2026-06-01T00:00:00Z')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO file_record
            (id, root_id, relative_path, size_bytes, mtime, state, first_seen_at, last_seen_at)
         VALUES ('file-1', 'root-1', 'darks/dark_001.fits', 2048, '2026-06-01T00:00:00Z',
                 'observed', '2026-06-01T00:00:00Z', '2026-06-01T00:00:00Z')",
    )
    .execute(pool)
    .await
    .unwrap();
}

/// `kind` must be one of `dark`/`flat`/`bias`; the view's `WHERE` clause drops
/// anything else and the test would assert over an empty result.
async fn seed_master(pool: &SqlitePool, id: &str, frame_ids: &str) {
    sqlx::query(
        "INSERT INTO calibration_session (id, session_key, frame_ids, kind, root_id, created_at)
         VALUES (?, ?, ?, 'dark', 'root-1', '2026-06-01T00:00:00Z')",
    )
    .bind(id)
    .bind(id)
    .bind(frame_ids)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn master_view_reads_survive_a_malformed_frame_ids_row() {
    let db = migrated().await;
    let pool = db.pool();
    seed_frame(pool).await;
    seed_master(pool, "master-corrupt", "oops").await;
    seed_master(pool, "master-ok", r#"["file-1"]"#).await;

    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {MASTER_VIEW_COLUMNS} FROM calibration_master_view \
         WHERE archived_at IS NULL ORDER BY id ASC"
    )))
    .fetch_all(pool)
    .await
    .expect("a malformed frame_ids row must not fail the whole view");

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get::<String, _>("id"), "master-corrupt");
    assert_eq!(rows[0].get::<String, _>("kind"), "dark");
    assert_eq!(rows[0].get::<Option<String>, _>("frame_relative_path"), None);
    assert_eq!(rows[1].get::<String, _>("id"), "master-ok");
    assert_eq!(rows[1].get::<String, _>("kind"), "dark");
    assert_eq!(
        rows[1].get::<Option<String>, _>("frame_relative_path"),
        Some("darks/dark_001.fits".to_owned())
    );
}
