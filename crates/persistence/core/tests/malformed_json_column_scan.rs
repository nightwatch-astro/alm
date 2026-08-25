// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `q_core::list_malformed_json_columns` separates an unreadable JSON column
//! from an empty one.
//!
//! Every other query over these columns wraps them in
//! `CASE WHEN json_valid(col) THEN col ELSE '[]' END`, which makes a corrupt
//! row read exactly like a row holding `[]`. This scan is the only place the
//! two are distinguishable, so it is asserted over all four guarded columns and
//! over valid rows in the same database — a scan that reported every row, or
//! none, would satisfy neither half.

use sqlx::SqlitePool;

use persistence_core::repositories::q_core::{list_malformed_json_columns, MalformedJsonColumnRow};
use persistence_core::Database;

async fn migrated() -> Database {
    let db = Database::in_memory().await.expect("in-memory database");
    db.migrate().await.expect("migrations apply");
    db
}

async fn seed_acquisition(pool: &SqlitePool, id: &str, frame_ids: &str) {
    sqlx::query(
        "INSERT INTO acquisition_session (id, session_key, frame_ids, created_at)
         VALUES (?, ?, ?, '2026-06-01T00:00:00Z')",
    )
    .bind(id)
    .bind(id)
    .bind(frame_ids)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_calibration(pool: &SqlitePool, id: &str, frame_ids: &str) {
    sqlx::query(
        "INSERT INTO calibration_session (id, session_key, frame_ids, kind, created_at)
         VALUES (?, ?, ?, 'dark', '2026-06-01T00:00:00Z')",
    )
    .bind(id)
    .bind(id)
    .bind(frame_ids)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_camera(pool: &SqlitePool, id: &str, aliases: &str) {
    sqlx::query(
        "INSERT INTO cameras (id, name, aliases, created_at)
         VALUES (?, ?, ?, '2026-06-01T00:00:00Z')",
    )
    .bind(id)
    .bind(id)
    .bind(aliases)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_telescope(pool: &SqlitePool, id: &str, aliases: &str) {
    sqlx::query(
        "INSERT INTO telescopes (id, name, aliases, created_at)
         VALUES (?, ?, ?, '2026-06-01T00:00:00Z')",
    )
    .bind(id)
    .bind(id)
    .bind(aliases)
    .execute(pool)
    .await
    .unwrap();
}

fn located(rows: &[MalformedJsonColumnRow]) -> Vec<(&str, &str, &str)> {
    rows.iter()
        .map(|r| (r.table_name.as_str(), r.column_name.as_str(), r.row_id.as_str()))
        .collect()
}

#[tokio::test]
async fn the_scan_reports_a_corrupt_row_in_each_guarded_column_and_no_valid_row() {
    let db = migrated().await;
    let pool = db.pool();

    seed_acquisition(pool, "acq-corrupt", "{").await;
    seed_acquisition(pool, "acq-empty", "[]").await;
    seed_acquisition(pool, "acq-populated", r#"["file-1"]"#).await;
    seed_calibration(pool, "cal-corrupt", "not json").await;
    seed_calibration(pool, "cal-empty", "[]").await;
    seed_camera(pool, "cam-corrupt", "[").await;
    seed_camera(pool, "cam-empty", "[]").await;
    seed_telescope(pool, "tel-corrupt", r#"{"a":}"#).await;
    seed_telescope(pool, "tel-empty", "[]").await;

    let rows = list_malformed_json_columns(pool).await.unwrap();

    assert_eq!(
        located(&rows),
        vec![
            ("acquisition_session", "frame_ids", "acq-corrupt"),
            ("calibration_session", "frame_ids", "cal-corrupt"),
            ("cameras", "aliases", "cam-corrupt"),
            ("telescopes", "aliases", "tel-corrupt"),
        ],
        "every guarded column reports its corrupt row, and no valid or empty row is reported"
    );
}

#[tokio::test]
async fn the_scan_is_empty_when_every_row_holds_valid_json() {
    let db = migrated().await;
    let pool = db.pool();

    seed_acquisition(pool, "acq-empty", "[]").await;
    seed_acquisition(pool, "acq-populated", r#"["file-1","file-2"]"#).await;
    seed_calibration(pool, "cal-empty", "[]").await;
    seed_camera(pool, "cam-ok", r#"["ASI2600"]"#).await;
    seed_telescope(pool, "tel-ok", "[]").await;

    let rows = list_malformed_json_columns(pool).await.unwrap();

    assert_eq!(located(&rows), Vec::new(), "an empty array is not a corrupt value");
}
