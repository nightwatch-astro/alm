// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Migration 0002 integration tests for the Spec 062 `relation_evidence` envelope.

use persistence_core::Database;
use sqlx::{Acquire, Executor, SqlitePool};

async fn fresh_database() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().expect("temporary database directory");
    let path = dir.path().join("relation-evidence.db");
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let db = Database::connect(&url).await.expect("connect fresh database");
    db.migrate_uncached().await.expect("run complete migration chain");
    (dir, db)
}

/// The two rows every evidence envelope points at: the matching-settings revision
/// it was measured under, and the repository change that wrote it.
async fn seed_evidence_prerequisites(pool: &SqlitePool) {
    let mut connection = pool.acquire().await.expect("acquire seed connection");
    let mut tx = connection.begin().await.expect("begin seed transaction");
    tx.execute("INSERT INTO spec062_actor VALUES (1, '00000000-0000-7000-8000-000000000001', '2026-07-22T00:00:00.000000Z')")
        .await
        .unwrap();
    tx.execute("INSERT INTO spec062_config_revision VALUES (1, '00000000-0000-7000-8000-000000000002', 1, 'config-digest', '2026-07-22T00:00:00.000000Z')")
        .await
        .unwrap();
    tx.execute("INSERT INTO repository_change(command_row_id, created_at) VALUES (NULL, '2026-07-22T00:00:00.000000Z')")
        .await
        .unwrap();
    tx.commit().await.expect("commit seed data");
}

/// A minimal accepted envelope: no optional geometry, all three lists empty.
async fn insert_evidence(pool: &SqlitePool, row_id: i64) {
    sqlx::query(
        "INSERT INTO relation_evidence (
            row_id, public_id, subject_kind, subject_digest, target_compatibility,
            parity, acquisition_geometry, equipment, config_revision_row_id, input_digest,
            expected_measurement_count, expected_missing_code_count,
            expected_rotation_range_count, created_sequence, created_at
         ) VALUES (
            ?1, ?2, 'proposal', 'subject-digest', 'same_target',
            'match', 'compatible', 'compatible', 1, 'input-digest',
            0, 0, 0, 1, '2026-07-22T00:00:00.000000Z'
         )",
    )
    .bind(row_id)
    .bind(format!("00000000-0000-7000-8000-0000000001{row_id:02}"))
    .execute(pool)
    .await
    .expect("insert baseline evidence envelope");
}

#[tokio::test]
async fn evidence_envelope_and_its_three_lists_exist_after_migration() {
    let (_dir, db) = fresh_database().await;

    let objects: Vec<(String, String)> = sqlx::query_as(
        "SELECT name, type FROM sqlite_master
         WHERE name LIKE 'relation_evidence%' ORDER BY name",
    )
    .fetch_all(db.pool())
    .await
    .expect("inspect migrated objects");
    assert_eq!(
        objects,
        vec![
            ("relation_evidence".to_owned(), "table".to_owned()),
            ("relation_evidence_allowed_rotation".to_owned(), "table".to_owned()),
            ("relation_evidence_measurement".to_owned(), "table".to_owned()),
            ("relation_evidence_missing_code".to_owned(), "table".to_owned()),
            ("relation_evidence_subject_digest_idx".to_owned(), "index".to_owned()),
        ]
    );

    let non_strict: Vec<(String,)> = sqlx::query_as(
        "SELECT name FROM pragma_table_list
         WHERE name LIKE 'relation_evidence%' AND strict = 0",
    )
    .fetch_all(db.pool())
    .await
    .expect("inspect strictness");
    assert!(non_strict.is_empty(), "non-STRICT evidence tables: {non_strict:?}");

    let violations: Vec<(String, i64, String, i64)> = sqlx::query_as("PRAGMA foreign_key_check")
        .fetch_all(db.pool())
        .await
        .expect("foreign key check");
    assert!(violations.is_empty(), "foreign key violations: {violations:?}");
}

/// Overwrite one column of the row-1 envelope, to exercise that column's CHECK.
///
/// `AssertSqlSafe`: every caller passes `column` from a literal array in this file,
/// so no user string reaches the statement, and the value itself is bound.
async fn set_column<T>(pool: &SqlitePool, column: &str, value: T) -> Result<(), sqlx::Error>
where
    T: for<'a> sqlx::Encode<'a, sqlx::Sqlite> + sqlx::Type<sqlx::Sqlite> + Send + 'static,
{
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE relation_evidence SET {column} = ?1 WHERE row_id = 1"
    )))
    .bind(value)
    .execute(pool)
    .await
    .map(|_| ())
}

#[tokio::test]
async fn evidence_rejects_verdicts_outside_the_contract_vocabulary() {
    let (_dir, db) = fresh_database().await;
    seed_evidence_prerequisites(db.pool()).await;
    insert_evidence(db.pool(), 1).await;

    for (column, value) in [
        ("subject_kind", "panel_group"),
        ("target_compatibility", "cross_target"),
        ("parity", "compatible"),
        ("acquisition_geometry", "match"),
        ("equipment", "same"),
    ] {
        let error = set_column(db.pool(), column, value)
            .await
            .expect_err(&format!("{column} must reject {value}"));
        assert!(error.to_string().contains("CHECK"), "{column}: {error}");
    }
}

#[tokio::test]
async fn optional_geometry_is_null_or_inside_its_range() {
    let (_dir, db) = fresh_database().await;
    seed_evidence_prerequisites(db.pool()).await;
    insert_evidence(db.pool(), 1).await;

    // A committed light session's singleton panel revision carries none of the three.
    let stored: (Option<i64>, Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT footprint_coverage_ppm, centre_separation_ppm, residual_sky_rotation_udeg
         FROM relation_evidence WHERE row_id = 1",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(stored, (None, None, None));

    for (column, value) in [
        ("footprint_coverage_ppm", -1),
        ("footprint_coverage_ppm", 1_000_001),
        ("centre_separation_ppm", -1),
    ] {
        set_column(db.pool(), column, value)
            .await
            .expect_err(&format!("{column} must reject {value}"));
    }

    // Both endpoints of the coverage range are inside it, and a negative residual
    // rotation is a direction rather than an error.
    for (column, value) in [
        ("footprint_coverage_ppm", 0),
        ("footprint_coverage_ppm", 1_000_000),
        ("centre_separation_ppm", 0),
        ("residual_sky_rotation_udeg", -180_000_000),
    ] {
        set_column(db.pool(), column, value)
            .await
            .unwrap_or_else(|error| panic!("{column} must accept {value}: {error}"));
    }
}

#[tokio::test]
async fn density_targets_are_bounded_at_the_contract_list_sizes() {
    let (_dir, db) = fresh_database().await;
    seed_evidence_prerequisites(db.pool()).await;
    insert_evidence(db.pool(), 1).await;

    for (column, value) in [
        ("expected_measurement_count", 101),
        ("expected_measurement_count", -1),
        ("expected_missing_code_count", 101),
        ("expected_rotation_range_count", 17),
    ] {
        set_column(db.pool(), column, value)
            .await
            .expect_err(&format!("{column} must reject {value}"));
    }
}

#[tokio::test]
async fn missing_codes_are_ordered_once_and_named_once_per_envelope() {
    let (_dir, db) = fresh_database().await;
    seed_evidence_prerequisites(db.pool()).await;
    insert_evidence(db.pool(), 1).await;

    let insert = "INSERT INTO relation_evidence_missing_code
         (evidence_row_id, ordinal, code, created_sequence) VALUES (?1, ?2, ?3, 1)";
    sqlx::query(insert)
        .bind(1_i64)
        .bind(0_i64)
        .bind("footprint_unavailable")
        .execute(db.pool())
        .await
        .expect("first missing code");

    // Same ordinal twice, and the same code under a fresh ordinal, are both
    // corrupt lists rather than two readings of one reason.
    sqlx::query(insert)
        .bind(1_i64)
        .bind(0_i64)
        .bind("rotation_unavailable")
        .execute(db.pool())
        .await
        .unwrap_err();
    sqlx::query(insert)
        .bind(1_i64)
        .bind(1_i64)
        .bind("footprint_unavailable")
        .execute(db.pool())
        .await
        .unwrap_err();

    // Out of the contract bound, and — because SQLite evaluates a CHECK over a
    // null to null — the null that would otherwise slip past that bound.
    sqlx::query(insert)
        .bind(1_i64)
        .bind(100_i64)
        .bind("out_of_bound")
        .execute(db.pool())
        .await
        .unwrap_err();
    sqlx::query(insert)
        .bind(1_i64)
        .bind(None::<i64>)
        .bind("unordered")
        .execute(db.pool())
        .await
        .unwrap_err();

    // A second envelope restarts the ordinals and may reuse the same code.
    insert_evidence(db.pool(), 2).await;
    sqlx::query(insert)
        .bind(2_i64)
        .bind(0_i64)
        .bind("footprint_unavailable")
        .execute(db.pool())
        .await
        .expect("codes are keyed per envelope, not globally");

    // No parent, no list row.
    sqlx::query(insert)
        .bind(99_i64)
        .bind(0_i64)
        .bind("orphan")
        .execute(db.pool())
        .await
        .unwrap_err();
}

#[tokio::test]
async fn allowed_rotation_ranges_are_closed_and_ascending() {
    let (_dir, db) = fresh_database().await;
    seed_evidence_prerequisites(db.pool()).await;
    insert_evidence(db.pool(), 1).await;

    let insert = "INSERT INTO relation_evidence_allowed_rotation
         (evidence_row_id, ordinal, lower_udeg, upper_udeg, created_sequence)
         VALUES (?1, ?2, ?3, ?4, 1)";

    // A single-value range is a legitimate closed interval.
    sqlx::query(insert)
        .bind(1_i64)
        .bind(0_i64)
        .bind(90_000_000_i64)
        .bind(90_000_000_i64)
        .execute(db.pool())
        .await
        .expect("degenerate closed interval");
    sqlx::query(insert)
        .bind(1_i64)
        .bind(1_i64)
        .bind(-1_000_i64)
        .bind(1_000_i64)
        .execute(db.pool())
        .await
        .expect("interval spanning zero");

    // Inverted bounds would make minInclusive/maxInclusive meaningless.
    sqlx::query(insert)
        .bind(1_i64)
        .bind(2_i64)
        .bind(1_000_i64)
        .bind(-1_000_i64)
        .execute(db.pool())
        .await
        .unwrap_err();
    sqlx::query(insert)
        .bind(1_i64)
        .bind(16_i64)
        .bind(0_i64)
        .bind(1_i64)
        .execute(db.pool())
        .await
        .unwrap_err();
    sqlx::query(insert)
        .bind(1_i64)
        .bind(None::<i64>)
        .bind(0_i64)
        .bind(1_i64)
        .execute(db.pool())
        .await
        .unwrap_err();
}

#[tokio::test]
async fn measurements_are_keyed_once_per_envelope_and_carry_a_pass_or_fail() {
    let (_dir, db) = fresh_database().await;
    seed_evidence_prerequisites(db.pool()).await;
    insert_evidence(db.pool(), 1).await;

    let insert = "INSERT INTO relation_evidence_measurement (
            evidence_row_id, ordinal, measurement_key, measured_value_micro, unit,
            comparison, threshold_value_micro, outcome, source_evidence_digest,
            created_sequence
         ) VALUES (?1, ?2, ?3, 1, 'degree', ?4, 2, ?5, 'source-digest', 1)";

    sqlx::query(insert)
        .bind(1_i64)
        .bind(0_i64)
        .bind("footprint.coverage")
        .bind("lte")
        .bind("pass")
        .execute(db.pool())
        .await
        .expect("first measurement");

    // One key per envelope: a second reading of the same measurement is a
    // rewritten verdict, not an additional one.
    sqlx::query(insert)
        .bind(1_i64)
        .bind(1_i64)
        .bind("footprint.coverage")
        .bind("lte")
        .bind("fail")
        .execute(db.pool())
        .await
        .unwrap_err();

    for (comparison, outcome) in [("le", "pass"), ("lte", "unknown"), ("lte", "warn")] {
        sqlx::query(insert)
            .bind(1_i64)
            .bind(2_i64)
            .bind("centre.separation")
            .bind(comparison)
            .bind(outcome)
            .execute(db.pool())
            .await
            .expect_err(&format!("{comparison}/{outcome} must be rejected"));
    }

    sqlx::query(insert)
        .bind(1_i64)
        .bind(100_i64)
        .bind("centre.separation")
        .bind("gte")
        .bind("pass")
        .execute(db.pool())
        .await
        .unwrap_err();
}

#[tokio::test]
async fn subject_lookup_is_indexed_by_kind_and_digest() {
    let (_dir, db) = fresh_database().await;
    seed_evidence_prerequisites(db.pool()).await;
    insert_evidence(db.pool(), 1).await;

    // The mispairing check reads by (subject_kind, subject_digest); a scan here
    // would mean every proposal projection reads the whole envelope table.
    let plan: Vec<(i64, i64, i64, String)> = sqlx::query_as(
        "EXPLAIN QUERY PLAN SELECT row_id FROM relation_evidence
         WHERE subject_kind = 'proposal' AND subject_digest = 'subject-digest'",
    )
    .fetch_all(db.pool())
    .await
    .expect("explain subject lookup");
    assert!(
        plan.iter()
            .any(|(_, _, _, detail)| detail.contains("relation_evidence_subject_digest_idx")),
        "subject lookup does not use the index: {plan:?}"
    );
}
