// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use contracts_core::first_run::{OrganizationState, RegisterSourceRequest};

#[test]
fn validate_path_not_exists() {
    let result = validate_path("/nonexistent/path/that/does/not/exist");
    let err = result.unwrap_err();
    assert_eq!(err.code, ErrorCode::PathNotExists);
}

#[test]
fn validate_path_not_directory() {
    // Use a known file path that exists on all platforms.
    let path = if cfg!(unix) { "/etc/hostname" } else { "C:\\Windows\\System32\\cmd.exe" };
    // Only run this test if the path actually exists.
    if std::fs::metadata(path).is_ok() {
        let result = validate_path(path);
        let err = result.unwrap_err();
        assert_eq!(err.code, ErrorCode::PathNotDirectory);
    }
}

#[test]
fn validate_path_success_for_tmp() {
    // /tmp should exist and be a directory on Unix.
    if cfg!(unix) {
        let result = validate_path("/tmp");
        assert!(result.is_ok());
    }
}

#[tokio::test]
async fn check_duplicate_detects_same_kind() {
    let db = persistence_core::Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    let pool = db.pool().clone();

    let req = RegisterSourceRequest {
        kind: SourceKind::LightFrames,
        path: "/tmp".to_owned(),
        kind_subtype: None,
        scan_depth: contracts_core::first_run::ScanDepth::Recursive,
        organization_state: OrganizationState::Organized,
    };
    repo::register_source(&pool, &req).await.unwrap();

    let err = check_duplicate(&pool, "/tmp", SourceKind::LightFrames).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::PathAlreadyRegistered);
    // Issue #501: an exact duplicate must hard-stop registration, not be
    // a bypassable `Warning`.
    assert_eq!(err.severity, ErrorSeverity::Blocking);
}

#[tokio::test]
async fn check_duplicate_detects_different_kind() {
    let db = persistence_core::Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    let pool = db.pool().clone();

    let req = RegisterSourceRequest {
        kind: SourceKind::LightFrames,
        path: "/tmp".to_owned(),
        kind_subtype: None,
        scan_depth: contracts_core::first_run::ScanDepth::Recursive,
        organization_state: OrganizationState::Organized,
    };
    repo::register_source(&pool, &req).await.unwrap();

    let err = check_duplicate(&pool, "/tmp", SourceKind::Project).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::PathAlreadyRegisteredDifferentKind);
}

// ── Issue #501: overlapping root registration ────────────────────────────

#[tokio::test]
async fn register_source_rejects_nested_child_root() {
    let db = persistence_core::Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    let pool = db.pool().clone();
    let bus = EventBus::with_pool(pool.clone());

    let parent = tempfile::tempdir().expect("tempdir");
    let child = parent.path().join("nested");
    std::fs::create_dir(&child).unwrap();

    let parent_req = RegisterSourceRequest {
        kind: SourceKind::LightFrames,
        path: parent.path().to_str().unwrap().to_owned(),
        kind_subtype: None,
        scan_depth: contracts_core::first_run::ScanDepth::Recursive,
        organization_state: OrganizationState::Organized,
    };
    register_source(&pool, &bus, &parent_req).await.unwrap();

    // A root nested inside an already-registered root — even under a
    // DIFFERENT category — must be rejected (cross-cutting overlap).
    let child_req = RegisterSourceRequest {
        kind: SourceKind::Inbox,
        path: child.to_str().unwrap().to_owned(),
        kind_subtype: None,
        scan_depth: contracts_core::first_run::ScanDepth::Recursive,
        organization_state: OrganizationState::Unorganized,
    };
    let err = register_source(&pool, &bus, &child_req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::PathOverlapsExisting);
}

#[tokio::test]
async fn register_source_rejects_parent_of_existing_root() {
    let db = persistence_core::Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    let pool = db.pool().clone();
    let bus = EventBus::with_pool(pool.clone());

    let parent = tempfile::tempdir().expect("tempdir");
    let child = parent.path().join("nested");
    std::fs::create_dir(&child).unwrap();

    let child_req = RegisterSourceRequest {
        kind: SourceKind::LightFrames,
        path: child.to_str().unwrap().to_owned(),
        kind_subtype: None,
        scan_depth: contracts_core::first_run::ScanDepth::Recursive,
        organization_state: OrganizationState::Organized,
    };
    register_source(&pool, &bus, &child_req).await.unwrap();

    // The parent of an already-registered root must also be rejected.
    let parent_req = RegisterSourceRequest {
        kind: SourceKind::LightFrames,
        path: parent.path().to_str().unwrap().to_owned(),
        kind_subtype: None,
        scan_depth: contracts_core::first_run::ScanDepth::Recursive,
        organization_state: OrganizationState::Organized,
    };
    let err = register_source(&pool, &bus, &parent_req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::PathOverlapsExisting);
}

#[tokio::test]
async fn register_source_batch_rejects_intra_batch_overlap() {
    let db = persistence_core::Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    let pool = db.pool().clone();
    let bus = EventBus::with_pool(pool.clone());

    let parent = tempfile::tempdir().expect("tempdir");
    let child = parent.path().join("nested");
    std::fs::create_dir(&child).unwrap();

    // Neither path is registered yet — the overlap must still be caught
    // candidate-vs-candidate within the SAME batch request.
    let req = contracts_core::first_run::RegisterSourceBatchRequest {
        sources: vec![
            RegisterSourceRequest {
                kind: SourceKind::LightFrames,
                path: parent.path().to_str().unwrap().to_owned(),
                kind_subtype: None,
                scan_depth: contracts_core::first_run::ScanDepth::Recursive,
                organization_state: OrganizationState::Organized,
            },
            RegisterSourceRequest {
                kind: SourceKind::Inbox,
                path: child.to_str().unwrap().to_owned(),
                kind_subtype: None,
                scan_depth: contracts_core::first_run::ScanDepth::Recursive,
                organization_state: OrganizationState::Unorganized,
            },
        ],
    };

    let resp = register_source_batch(&pool, &bus, &req).await.unwrap();
    assert_eq!(resp.status, contracts_core::first_run::BatchStatus::Partial);
    assert_eq!(resp.items[0].status, contracts_core::first_run::ItemStatus::Success);
    assert_eq!(resp.items[1].status, contracts_core::first_run::ItemStatus::Failure);
    assert_eq!(resp.items[1].error.as_deref(), Some("path.overlaps_existing"));
}

/// A case-only variant of an already-registered root names the SAME directory
/// on a case-insensitive volume (and must be refused as an overlap), and a
/// DIFFERENT directory on a case-sensitive one (and must be accepted). Both
/// branches run against whichever volume `tempfile` lands on; `create_dir`
/// answers which volume that is, independently of the identity check under
/// test. Set `TMPDIR` to a case-sensitive volume to exercise the other branch.
#[tokio::test]
async fn register_source_case_variant_of_existing_root_follows_the_volume() {
    let db = persistence_core::Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    let pool = db.pool().clone();
    let bus = EventBus::with_pool(pool.clone());

    let parent = tempfile::tempdir().expect("tempdir");
    let lower = parent.path().join("Foo");
    let variant = parent.path().join("FOO");
    std::fs::create_dir(&lower).expect("create Foo");
    let volume_is_case_sensitive = match std::fs::create_dir(&variant) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => false,
        Err(e) => panic!("probing volume case sensitivity: {e}"),
    };

    let req = RegisterSourceRequest {
        kind: SourceKind::LightFrames,
        path: lower.to_str().expect("utf8 path").to_owned(),
        kind_subtype: None,
        scan_depth: contracts_core::first_run::ScanDepth::Recursive,
        organization_state: OrganizationState::Organized,
    };
    register_source(&pool, &bus, &req).await.unwrap();

    let variant_req = RegisterSourceRequest {
        kind: SourceKind::Inbox,
        path: variant.to_str().expect("utf8 path").to_owned(),
        kind_subtype: None,
        scan_depth: contracts_core::first_run::ScanDepth::Recursive,
        organization_state: OrganizationState::Unorganized,
    };
    let result = register_source(&pool, &bus, &variant_req).await;

    if volume_is_case_sensitive {
        result.expect("distinct directories on a case-sensitive volume register");
    } else {
        assert_eq!(
            result.expect_err("case variant of one directory is refused").code,
            ErrorCode::PathOverlapsExisting
        );
    }
}

/// T1-c: `db_to_contract`'s fallback arm now delegates to the canonical
/// `db_err` mapper, so a `NotFound` (missing row) is `Blocking`/
/// non-retryable rather than the hand-rolled `Fatal`/`retryable=true`
/// this used to apply to every `DbError` variant.
#[tokio::test]
async fn remove_source_not_found_is_blocking_not_fatal() {
    let db = persistence_core::Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    let pool = db.pool().clone();

    let err = remove_source(&pool, "does-not-exist").await.unwrap_err();
    assert_eq!(err.severity, ErrorSeverity::Blocking);
    assert!(!err.retryable);
}

/// T125/SC-009: a successful `register_source` writes a durable
/// `Outcome::Applied` `audit_log_entry` row tagged `EntityType::DataSource`.
#[tokio::test]
async fn register_source_writes_durable_applied_audit_row() {
    let db = persistence_core::Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    let pool = db.pool().clone();
    let bus = EventBus::with_pool(pool.clone());

    // CI FIX: `tempfile::tempdir()` (not a hardcoded "/tmp") — mirrors
    // `crates/app/core/tests/first_run_integration.rs`'s pattern for this
    // same function; a Unix-only literal path fails `validate_path` on
    // windows-latest.
    let dir = tempfile::tempdir().expect("tempdir");
    let req = RegisterSourceRequest {
        kind: SourceKind::LightFrames,
        path: dir.path().to_str().expect("utf8 path").to_owned(),
        kind_subtype: None,
        scan_depth: contracts_core::first_run::ScanDepth::Recursive,
        organization_state: OrganizationState::Organized,
    };
    register_source(&pool, &bus, &req).await.unwrap();

    let row: (String, String) = sqlx::query_as(
        "SELECT entity_type, outcome FROM audit_log_entry WHERE trigger = 'source.register'",
    )
    .fetch_one(&pool)
    .await
    .expect("register_source must write a durable audit row");
    assert_eq!(row.0, "data_source");
    assert_eq!(row.1, "applied");
}

/// T127: a refused `register_source` (duplicate path) writes a durable
/// `Outcome::Refused` row with a reason_code, per FR-130.
#[tokio::test]
async fn register_source_refused_duplicate_writes_durable_row() {
    let db = persistence_core::Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    let pool = db.pool().clone();
    let bus = EventBus::with_pool(pool.clone());

    // CI FIX: see `register_source_writes_durable_applied_audit_row` —
    // same "/tmp" → tempdir() Windows fix.
    let dir = tempfile::tempdir().expect("tempdir");
    let req = RegisterSourceRequest {
        kind: SourceKind::LightFrames,
        path: dir.path().to_str().expect("utf8 path").to_owned(),
        kind_subtype: None,
        scan_depth: contracts_core::first_run::ScanDepth::Recursive,
        organization_state: OrganizationState::Organized,
    };
    register_source(&pool, &bus, &req).await.unwrap();
    let err = register_source(&pool, &bus, &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::PathAlreadyRegistered);

    let row: (String, Option<String>) = sqlx::query_as(
        "SELECT outcome, reason_code FROM audit_log_entry WHERE trigger = 'source.register' AND outcome = 'refused'",
    )
    .fetch_one(&pool)
    .await
    .expect("refused register_source must write a durable audit row");
    assert_eq!(row.0, "refused");
    assert_eq!(row.1.as_deref(), Some("path.already_registered"));
}

#[tokio::test]
async fn complete_first_run_rejects_without_sources() {
    let db = persistence_core::Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    let pool = db.pool().clone();
    let bus = EventBus::with_pool(pool.clone());

    let err = complete_first_run(&pool, &bus).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::FirstrunIncomplete);
}

// ── P6a: root remap use cases ────────────────────────────────────────────

#[tokio::test]
async fn remap_root_missing_root_returns_not_found() {
    let db = persistence_core::Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    let pool = db.pool().clone();

    let err = remap_root(&pool, "nonexistent-root", "/tmp").await.unwrap_err();
    assert_eq!(err.code, ErrorCode::SourceNotFound);
}

#[tokio::test]
async fn remap_root_invalid_new_path_returns_path_not_exists() {
    let db = persistence_core::Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    let pool = db.pool().clone();

    let req = RegisterSourceRequest {
        kind: SourceKind::LightFrames,
        path: "/tmp".to_owned(),
        kind_subtype: None,
        scan_depth: contracts_core::first_run::ScanDepth::Recursive,
        organization_state: OrganizationState::Organized,
    };
    let resp = repo::register_source(&pool, &req).await.unwrap();

    let err = remap_root(&pool, &resp.source_id, "/nonexistent/path/that/does/not/exist")
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::PathNotExists);
}

#[tokio::test]
async fn remap_root_with_no_file_records_is_verified_by_path_existence_alone() {
    // Needs a real, existing directory to remap into; "/tmp" is Unix-only.
    if !cfg!(unix) {
        return;
    }
    let db = persistence_core::Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    let pool = db.pool().clone();

    let req = RegisterSourceRequest {
        kind: SourceKind::Calibration,
        path: "/tmp".to_owned(),
        kind_subtype: None,
        scan_depth: contracts_core::first_run::ScanDepth::Recursive,
        organization_state: OrganizationState::Organized,
    };
    let resp = repo::register_source(&pool, &req).await.unwrap();

    let preview = remap_root(&pool, &resp.source_id, "/tmp").await.unwrap();
    assert_eq!(preview.original_path, "/tmp");
    assert_eq!(preview.new_path, "/tmp");
    assert!(preview.samples.is_empty());
    assert!(preview.all_verified);
}

/// T127 "source op failed": an `apply_root_remap` attempted against a
/// missing root writes a durable `Outcome::Failed` row tagged
/// `EntityType::LibraryRoot`, with a reason_code (FR-130).
#[tokio::test]
async fn apply_root_remap_missing_root_returns_not_found() {
    let db = persistence_core::Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    let pool = db.pool().clone();
    let bus = EventBus::with_pool(pool.clone());

    let err = apply_root_remap(&pool, &bus, "nonexistent-root", "/tmp", true).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::SourceNotFound);

    let row: (String, String, Option<String>) = sqlx::query_as(
        "SELECT entity_type, outcome, reason_code FROM audit_log_entry WHERE trigger = 'root.remap.apply'",
    )
    .fetch_one(&pool)
    .await
    .expect("failed apply_root_remap must write a durable audit row");
    assert_eq!(row.0, "library_root");
    assert_eq!(row.1, "failed");
    assert_eq!(row.2.as_deref(), Some("source.not_found"));
}

#[tokio::test]
async fn apply_root_remap_invalid_new_path_returns_path_not_exists() {
    let db = persistence_core::Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    let pool = db.pool().clone();
    let bus = EventBus::with_pool(pool.clone());

    let req = RegisterSourceRequest {
        kind: SourceKind::LightFrames,
        path: "/tmp".to_owned(),
        kind_subtype: None,
        scan_depth: contracts_core::first_run::ScanDepth::Recursive,
        organization_state: OrganizationState::Organized,
    };
    let resp = repo::register_source(&pool, &req).await.unwrap();

    let err = apply_root_remap(
        &pool,
        &bus,
        &resp.source_id,
        "/nonexistent/path/that/does/not/exist",
        true,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::PathNotExists);

    // Apply-without-verify semantics: a failed apply must never mutate the
    // stored path — the root still reports its original location.
    let (_, path) = repo::get_source_kind_and_path(&pool, &resp.source_id).await.unwrap().unwrap();
    assert_eq!(path, "/tmp");
}

#[tokio::test]
async fn apply_root_remap_updates_path_and_publishes_audit_event() {
    // Needs two real, existing directories; "/tmp" and "/var/tmp" are Unix-only.
    if !cfg!(unix) {
        return;
    }
    let db = persistence_core::Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    let pool = db.pool().clone();
    let bus = EventBus::with_pool(pool.clone());

    let req = RegisterSourceRequest {
        kind: SourceKind::Project,
        path: "/tmp".to_owned(),
        kind_subtype: None,
        scan_depth: contracts_core::first_run::ScanDepth::Recursive,
        organization_state: OrganizationState::Organized,
    };
    let resp = repo::register_source(&pool, &req).await.unwrap();

    apply_root_remap(&pool, &bus, &resp.source_id, "/var/tmp", true).await.unwrap();

    let (_, path) = repo::get_source_kind_and_path(&pool, &resp.source_id).await.unwrap().unwrap();
    assert_eq!(path, "/var/tmp");

    // A durable `root.remapped` audit event was written (constitution §II).
    let row: (String,) = sqlx::query_as("SELECT payload FROM events WHERE topic = 'root.remapped'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(row.0.contains(&resp.source_id));
    assert!(row.0.contains("/tmp"));
    assert!(row.0.contains("/var/tmp"));
}

/// Issue #707: `verified: false` must refuse the mutation, not merely be
/// recorded as audit metadata — this is the core of the bug report.
#[tokio::test]
async fn apply_root_remap_rejects_when_not_verified() {
    if !cfg!(unix) {
        return;
    }
    let db = persistence_core::Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    let pool = db.pool().clone();
    let bus = EventBus::with_pool(pool.clone());

    let req = RegisterSourceRequest {
        kind: SourceKind::Calibration,
        path: "/tmp".to_owned(),
        kind_subtype: None,
        scan_depth: contracts_core::first_run::ScanDepth::Recursive,
        organization_state: OrganizationState::Organized,
    };
    let resp = repo::register_source(&pool, &req).await.unwrap();

    let err = apply_root_remap(&pool, &bus, &resp.source_id, "/var/tmp", false)
        .await
        .expect_err("apply with verified: false must be refused");
    assert_eq!(err.code, ErrorCode::RemapNotVerified);
    assert_eq!(err.severity, ErrorSeverity::Blocking);

    // The stored path must be untouched.
    let (_, path) = repo::get_source_kind_and_path(&pool, &resp.source_id).await.unwrap().unwrap();
    assert_eq!(path, "/tmp");

    let row: (String, String) = sqlx::query_as(
        "SELECT outcome, reason_code FROM audit_log_entry WHERE trigger = 'root.remap.apply' AND outcome = 'refused'",
    )
    .fetch_one(&pool)
    .await
    .expect("refused apply_root_remap must write a durable audit row");
    assert_eq!(row.0, "refused");
    assert_eq!(row.1, "remap.not_verified");
}

/// nJ01a review carry-over: a caller passing `verified: true` must not
/// bypass server-side re-verification. A recorded relative path that
/// does NOT resolve under `new_path` means the true state disagrees with
/// the caller's claim (stale preview, or a direct IPC bypass attempt) —
/// apply must still be refused.
#[tokio::test]
async fn apply_root_remap_rejects_stale_verified_true_claim() {
    if !cfg!(unix) {
        return;
    }
    let db = persistence_core::Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    let pool = db.pool().clone();
    let bus = EventBus::with_pool(pool.clone());

    let req = RegisterSourceRequest {
        kind: SourceKind::LightFrames,
        path: "/tmp".to_owned(),
        kind_subtype: None,
        scan_depth: contracts_core::first_run::ScanDepth::Recursive,
        organization_state: OrganizationState::Organized,
    };
    let resp = repo::register_source(&pool, &req).await.unwrap();

    // Mirror the registered_sources row into library_root so the
    // file_record FK holds — see persistence_db's
    // `relative_paths_for_root_is_exhaustive_and_ordered`.
    sqlx::query(
        "INSERT INTO library_root (id, label, current_path, kind, state, created_at) \
         VALUES (?, ?, ?, 'local', 'active', '2026-01-01T00:00:00Z')",
    )
    .bind(&resp.source_id)
    .bind(&resp.source_id)
    .bind(&resp.path)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO file_record \
         (id, root_id, relative_path, size_bytes, mtime, state, first_seen_at, last_seen_at) \
         VALUES (?, ?, 'nonexistent/light_001.fits', 0, '2026-01-01T00:00:00Z', 'observed', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .bind("fr-stale")
    .bind(&resp.source_id)
    .execute(&pool)
    .await
    .unwrap();

    // "/var/tmp" does not contain "nonexistent/light_001.fits" — the
    // caller's `verified: true` claim is stale/wrong, so the recompute
    // inside `apply_root_remap` must refuse regardless.
    let err = apply_root_remap(&pool, &bus, &resp.source_id, "/var/tmp", true)
        .await
        .expect_err("stale verified:true claim must be refused after server-side recompute");
    assert_eq!(err.code, ErrorCode::RemapNotVerified);

    let (_, path) = repo::get_source_kind_and_path(&pool, &resp.source_id).await.unwrap().unwrap();
    assert_eq!(path, "/tmp");
}

/// Issue #560: a root only scanned into the Inbox (no `file_record` rows,
/// but real `inbox_items` rows) must NOT report `all_verified: true` from
/// a vacuous empty sample when its actual content isn't found at the new
/// path.
#[tokio::test]
async fn remap_root_checks_inbox_items_not_just_file_record() {
    use persistence_inbox::repositories::inbox::{insert_inbox_item, InsertInboxItem};

    let db = persistence_core::Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    let pool = db.pool().clone();

    let req = RegisterSourceRequest {
        kind: SourceKind::Inbox,
        path: "/tmp".to_owned(),
        kind_subtype: None,
        scan_depth: contracts_core::first_run::ScanDepth::Recursive,
        organization_state: OrganizationState::Unorganized,
    };
    let resp = repo::register_source(&pool, &req).await.unwrap();

    insert_inbox_item(
        &pool,
        &InsertInboxItem {
            id: "item-1",
            root_id: &resp.source_id,
            relative_path: "M31/lights",
            file_count: 7,
            content_signature: None,
            lane: "fits",
        },
    )
    .await
    .unwrap();

    // A brand-new, genuinely empty candidate directory — none of the
    // root's recorded content lives there.
    let new_dir = tempfile::tempdir().expect("tempdir");
    let preview =
        remap_root(&pool, &resp.source_id, new_dir.path().to_str().unwrap()).await.unwrap();

    assert_eq!(preview.samples.len(), 1, "the inbox item must be sampled");
    assert!(!preview.samples[0].found);
    assert!(
        !preview.all_verified,
        "must not vacuously report all_verified against an empty candidate path"
    );
}

// ── P6b: root active toggle ────────────────────────────────────────────────

#[tokio::test]
async fn set_source_active_missing_root_returns_not_found() {
    let db = persistence_core::Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    let pool = db.pool().clone();
    let bus = EventBus::with_pool(pool.clone());

    let err = set_source_active(&pool, &bus, "nonexistent-id", false).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::SourceNotFound);
}

#[tokio::test]
async fn set_source_active_toggles_and_publishes_audit_event() {
    let db = persistence_core::Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    let pool = db.pool().clone();
    let bus = EventBus::with_pool(pool.clone());

    let req = RegisterSourceRequest {
        kind: SourceKind::LightFrames,
        path: "/astro/raw".to_owned(),
        kind_subtype: None,
        scan_depth: contracts_core::first_run::ScanDepth::Recursive,
        organization_state: OrganizationState::Organized,
    };
    let resp = repo::register_source(&pool, &req).await.unwrap();

    set_source_active(&pool, &bus, &resp.source_id, false).await.unwrap();

    let flags = repo::list_active_flags(&pool).await.unwrap();
    assert_eq!(flags.get(&resp.source_id), Some(&false));

    // A durable `root.active_changed` audit event was written (constitution §II).
    let row: (String,) =
        sqlx::query_as("SELECT payload FROM events WHERE topic = 'root.active_changed'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(row.0.contains(&resp.source_id));
    assert!(row.0.contains("false"));

    set_source_active(&pool, &bus, &resp.source_id, true).await.unwrap();
    let flags = repo::list_active_flags(&pool).await.unwrap();
    assert_eq!(flags.get(&resp.source_id), Some(&true));
}

// ── P6b: root delete ───────────────────────────────────────────────────────

#[tokio::test]
async fn delete_source_missing_root_returns_not_found() {
    let db = persistence_core::Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    let pool = db.pool().clone();
    let bus = EventBus::with_pool(pool.clone());

    let err = delete_source(&pool, &bus, "nonexistent-id").await.unwrap_err();
    assert_eq!(err.code, ErrorCode::SourceNotFound);
}

#[tokio::test]
async fn delete_source_without_dependents_succeeds_and_publishes_audit_event() {
    let db = persistence_core::Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    let pool = db.pool().clone();
    let bus = EventBus::with_pool(pool.clone());

    let req = RegisterSourceRequest {
        kind: SourceKind::Project,
        path: "/astro/projects".to_owned(),
        kind_subtype: None,
        scan_depth: contracts_core::first_run::ScanDepth::Recursive,
        organization_state: OrganizationState::Organized,
    };
    let resp = repo::register_source(&pool, &req).await.unwrap();

    delete_source(&pool, &bus, &resp.source_id).await.unwrap();

    let remaining = repo::list_sources(&pool).await.unwrap();
    assert!(remaining.iter().all(|s| s.source_id != resp.source_id));

    // A durable `root.deleted` audit event was written (constitution §II).
    let row: (String,) = sqlx::query_as("SELECT payload FROM events WHERE topic = 'root.deleted'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(row.0.contains(&resp.source_id));
    assert!(row.0.contains("/astro/projects"));
}

#[tokio::test]
async fn delete_source_blocks_when_dependents_exist() {
    use persistence_inbox::repositories::inbox::{insert_inbox_item, InsertInboxItem};

    let db = persistence_core::Database::in_memory().await.unwrap();
    db.migrate().await.unwrap();
    let pool = db.pool().clone();
    let bus = EventBus::with_pool(pool.clone());

    let req = RegisterSourceRequest {
        kind: SourceKind::Inbox,
        path: "/astro/inbox".to_owned(),
        kind_subtype: None,
        scan_depth: contracts_core::first_run::ScanDepth::Recursive,
        organization_state: OrganizationState::Unorganized,
    };
    let resp = repo::register_source(&pool, &req).await.unwrap();

    insert_inbox_item(
        &pool,
        &InsertInboxItem {
            id: "item-1",
            root_id: &resp.source_id,
            relative_path: "2026-01-01/lights",
            file_count: 3,
            content_signature: None,
            lane: "fits",
        },
    )
    .await
    .unwrap();

    let err = delete_source(&pool, &bus, &resp.source_id).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::RootHasDependents);
    // The typed counts are surfaced in `details` so the caller can explain
    // the block reason without a second round trip.
    assert_eq!(err.details.0["inboxItems"], serde_json::json!(1));

    // The source registration must NOT have been removed (no cascade,
    // no partial delete — constitution §II).
    let remaining = repo::list_sources(&pool).await.unwrap();
    assert!(remaining.iter().any(|s| s.source_id == resp.source_id));
}
