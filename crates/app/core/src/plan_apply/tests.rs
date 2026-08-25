use super::*;
use audit::EventBus;
use fs_executor::failure::{FailureCode, PlanItemFailure};
use fs_executor::RollbackOutcome;
use persistence_core::Database;
use persistence_lifecycle::repositories::audit::{
    count_audit_entries, list_audit_entries, AuditLogFilter,
};
use persistence_plans::repositories::plans as repo;
use uuid::Uuid;

async fn setup() -> (Database, EventBus) {
    let db = Database::in_memory().await.expect("in-memory DB");
    db.migrate().await.expect("migrations");
    let bus = EventBus::with_pool(db.pool().clone());
    (db, bus)
}

async fn insert_approved_plan_with_items(db: &Database, plan_id: &str, item_count: usize) {
    repo::insert_plan(
        db.pool(),
        &repo::InsertPlan {
            id: plan_id,
            title: "Test",
            origin: "cleanup",
            origin_path: None,
            plan_type: "cleanup",
            destructive_destination: "archive",
            parent_plan_id: None,
            total_bytes_required: 0,
        },
    )
    .await
    .unwrap();

    for i in 0..item_count {
        repo::insert_plan_item(
            db.pool(),
            &repo::InsertPlanItem {
                id: &format!("{plan_id}-item-{i}"),
                plan_id,
                item_index: i64::try_from(i + 1).unwrap(),
                name: "file.fits",
                action: "move",
                from_root_id: None,
                // Plan-scoped paths: tests share the process-global
                // ACTIVE_RUNS registry and run in parallel, so identical
                // relative paths across tests would trip the FR-017
                // overlap guard non-deterministically.
                from_relative_path: &format!("{plan_id}/raw/file-{i}.fits"),
                to_root_id: None,
                to_relative_path: &format!("{plan_id}/archive/file-{i}.fits"),
                reason: "test",
                protection: "normal",
                linked_entity: None,
                provenance_json: None,
                archive_path: None,
                source_id: None,
                category: None,
            },
        )
        .await
        .unwrap();
    }

    repo::update_plan_state(db.pool(), plan_id, "ready_for_review").await.unwrap();
    repo::set_approved(db.pool(), plan_id, "2026-06-01T00:00:00Z", "test-token").await.unwrap();
}

#[tokio::test]
async fn plan_apply_callbacks_persist_every_producible_failure_code() {
    const PRODUCIBLE_CODES: [FailureCode; 19] = [
        FailureCode::PermissionDenied,
        FailureCode::ConflictDestinationExists,
        FailureCode::SourceMissing,
        FailureCode::SourceLocked,
        FailureCode::VolumeUnavailable,
        FailureCode::DiskFull,
        FailureCode::PathInvalid,
        FailureCode::RootEscape,
        FailureCode::SymlinkComponent,
        FailureCode::DestructiveUnconfirmed,
        FailureCode::ProtectedSource,
        FailureCode::TrashUnavailable,
        FailureCode::CopySucceededDeleteFailed,
        FailureCode::CopySucceededDeleteFailedRollbackFailed,
        FailureCode::ItemStale,
        FailureCode::OsTrashFull,
        FailureCode::OsTrashPermissionDenied,
        FailureCode::MaterializationUnsupported,
        FailureCode::Unknown,
    ];

    let (db, bus) = setup().await;
    let plan_id = "p-all-failure-codes";
    let run_id = "run-all-failure-codes";
    insert_approved_plan_with_items(&db, plan_id, PRODUCIBLE_CODES.len()).await;
    let item_count = i64::try_from(PRODUCIBLE_CODES.len()).unwrap();
    apply_repo::cas_approved_to_applying(
        db.pool(),
        plan_id,
        run_id,
        "test-token",
        item_count,
        item_count,
    )
    .await
    .unwrap();

    let callbacks = PlanApplyCallbacks::new(
        db.pool().clone(),
        bus,
        plan_id.to_owned(),
        run_id.to_owned(),
        None,
    );

    // Buffer all events (group-commit design: rows aren't written until flush).
    for (index, code) in PRODUCIBLE_CODES.into_iter().enumerate() {
        let item_id = format!("{plan_id}-item-{index}");
        callbacks.on_item_start(&item_id).await;
        callbacks
            .on_item_progress(ItemProgressEvent {
                item_id: item_id.clone(),
                prior_state: "applying".to_owned(),
                new_state: "failed".to_owned(),
                at: Timestamp::now_iso(),
                failure: Some(PlanItemFailure::with_code(code, "natural-seam failure")),
                rollback_attempted: false,
                rollback_outcome: RollbackOutcome::NotApplicable,
                rollback_message: None,
                audit_reason: None,
            })
            .await;
    }

    // Mandatory flush: drains the buffer into the DB in one tx.
    callbacks.flush().await;

    // Verify every failure code was persisted.
    for (index, code) in PRODUCIBLE_CODES.into_iter().enumerate() {
        let item_id = format!("{plan_id}-item-{index}");
        let persisted: Option<String> = sqlx::query_scalar(
            "SELECT failure_code FROM plan_apply_events WHERE plan_id = ? AND item_id = ?",
        )
        .bind(plan_id)
        .bind(&item_id)
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(persisted.as_deref(), Some(code.as_str()), "failed for {code:?}");
    }
}

/// Regression (FIX review, priority-check #2): `resolve_root_path`'s
/// `registered_sources` read-through must never resurface a
/// pre-remap path after `apply_root_remap` commits the new one.
#[tokio::test]
async fn resolve_root_path_reflects_remap_not_stale_cache() {
    use contracts_core::first_run::{
        OrganizationState, RegisterSourceRequest, ScanDepth, SourceKind,
    };

    // Needs two real, existing directories; "/tmp" and "/var/tmp" are Unix-only.
    if !cfg!(unix) {
        return;
    }

    let (db, bus) = setup().await;

    let reg = crate::first_run::register_source(
        db.pool(),
        &bus,
        &RegisterSourceRequest {
            kind: SourceKind::Project,
            path: "/tmp".to_owned(),
            kind_subtype: None,
            scan_depth: ScanDepth::Recursive,
            organization_state: OrganizationState::Organized,
        },
    )
    .await
    .unwrap();

    // Populate the cache via the same registered_sources fallback branch
    // apply_plan's root_map build resolves through.
    let resolved = resolve_root_path(db.pool(), &reg.source_id).await;
    assert_eq!(resolved.as_deref(), Some("/tmp"), "must resolve the registered path");

    // Remap must invalidate the cache entry after its DB write commits.
    crate::first_run::apply_root_remap(db.pool(), &bus, &reg.source_id, "/var/tmp", true)
        .await
        .unwrap();

    let after_remap = resolve_root_path(db.pool(), &reg.source_id).await;
    assert_eq!(
        after_remap.as_deref(),
        Some("/var/tmp"),
        "resolve_root_path must return the remapped path, not a stale cached one"
    );
}

#[tokio::test]
async fn apply_plan_rejects_wrong_state() {
    let (db, bus) = setup().await;
    repo::insert_plan(
        db.pool(),
        &repo::InsertPlan {
            id: "p-draft",
            title: "Test",
            origin: "cleanup",
            origin_path: None,
            plan_type: "cleanup",
            destructive_destination: "archive",
            parent_plan_id: None,
            total_bytes_required: 0,
        },
    )
    .await
    .unwrap();

    let err = apply_plan(db.pool(), &bus, "p-draft", "tok", None).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::PlanInvalidState);
}

#[tokio::test]
async fn apply_plan_rejects_wrong_token() {
    let (db, bus) = setup().await;
    insert_approved_plan_with_items(&db, "p1", 1).await;

    let err = apply_plan(db.pool(), &bus, "p1", "wrong-token", None).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::PlanApprovalStale);
}

#[tokio::test]
async fn apply_plan_starts_successfully() {
    let (db, bus) = setup().await;
    insert_approved_plan_with_items(&db, "p1", 1).await;

    let resp = apply_plan(db.pool(), &bus, "p1", "test-token", None).await.unwrap();
    assert_eq!(resp.plan_id, "p1");
    assert_eq!(resp.new_state, "applying");
    assert!(!resp.run_id.is_empty());

    // The background executor is spawned via `tokio::spawn`, and the
    // `#[tokio::test]` current-thread runtime only gives it a chance to
    // run at the next `.await` yield point — which is the `get_plan`
    // call right below. On a fast/loaded runner the executor can win that
    // race and finish (this test's item has no real file on disk, so it
    // resolves to a terminal `failed` state) before this read, which is
    // not a bug in `apply_plan` (the CAS to "applying" already succeeded,
    // per `resp.new_state` above) — it's a timing artifact of reading
    // back a state the caller does not otherwise synchronize on. Accept
    // either the transient "applying" state or a terminal state the
    // now-raced-ahead executor already reached.
    let plan = repo::get_plan(db.pool(), "p1", false).await.unwrap();
    assert!(
        matches!(plan.state.as_str(), "applying" | "completed" | "failed"),
        "unexpected plan state after apply_plan: {}",
        plan.state
    );

    // Wait briefly for the background task to complete.
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
}

/// T240 (spec 042 US16): a subscribed sink receives the long-op lifecycle —
/// a `Started` (ItemStarted carrying the running handle), per-item events,
/// then a terminal `Completed`/`Failed` carrying a terminal handle, with a
/// strictly increasing `sequence`. The durable audit rows are still written
/// (asserted separately) — the sink is an additive live projection (§II).
#[tokio::test]
async fn apply_plan_streams_operation_events() {
    use std::sync::Mutex;

    let (db, bus) = setup().await;
    insert_approved_plan_with_items(&db, "p-evt", 1).await;

    let captured: Arc<Mutex<Vec<OperationEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink_store = captured.clone();
    let sink: OperationEventSink = Arc::new(move |event: OperationEvent| {
        sink_store.lock().unwrap().push(event);
    });

    let resp = apply_plan(db.pool(), &bus, "p-evt", "test-token", Some(sink)).await.unwrap();

    // Let the background executor run to completion.
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let events = captured.lock().unwrap().clone();
    assert!(!events.is_empty(), "sink must receive long-op events");

    // First event is the Started projection carrying a Running handle.
    let first = &events[0];
    assert_eq!(first.event_type, OperationEventType::ItemStarted);
    assert_eq!(first.operation_id, OperationId(resp.run_id.clone()));
    assert_eq!(first.sequence, 0);

    // Sequence is strictly increasing across the run.
    for window in events.windows(2) {
        assert!(window[1].sequence > window[0].sequence, "sequence must be monotonic");
    }

    // The run terminates with a Completed (or Failed) event carrying a
    // terminal handle.
    let last = events.last().unwrap();
    assert!(
        matches!(last.event_type, OperationEventType::Completed | OperationEventType::Failed),
        "last event must be a terminal Completed/Failed, got {:?}",
        last.event_type
    );

    // Durable audit trail is retained: the DB still holds run events.
    let plan = repo::get_plan(db.pool(), "p-evt", false).await.unwrap();
    assert_ne!(plan.state, "approved", "plan must have progressed past approved in the DB");
}

// ── Spec 017 C5: archive lifecycle closure ──────────────────────────────

/// The finalize helper drives a completed project into `archived` and records
/// the owning plan id — the legitimate closure of the requires-plan gate.
#[tokio::test]
async fn finalize_archive_lifecycle_archives_completed_project() {
    use persistence_plans::repositories::projects as projects_repo;

    let (db, bus) = setup().await;
    let project_id = Uuid::new_v4().to_string();
    projects_repo::insert_project(
        db.pool(),
        &projects_repo::InsertProject {
            id: &project_id,
            name: "M31 LRGB",
            tool: "PixInsight",
            lifecycle: "completed",
            path: "projects/M31_LRGB",
            notes: None,
            canonical_target_id: None,
            is_mosaic: false,
        },
    )
    .await
    .unwrap();

    finalize_archive_lifecycle(db.pool(), &bus, "plan-arch-1", &project_id).await;

    let project = projects_repo::get_project(db.pool(), &project_id).await.unwrap();
    assert_eq!(project.lifecycle, "archived", "project must be driven to archived");

    // The link is recorded so archive-management commands act O(1).
    let archived = projects_repo::list_archived_projects(db.pool()).await.unwrap();
    assert_eq!(archived.len(), 1);
    assert_eq!(archived[0].archived_via_plan_id.as_deref(), Some("plan-arch-1"));
}

/// #665: a fully-applied `project_create` plan must fire the `Created`
/// manifest trigger — previously there was no emitter at all for it.
#[tokio::test]
async fn finalize_project_create_manifest_writes_created_manifest() {
    use persistence_plans::repositories::manifests::list_manifests_for_project;
    use persistence_plans::repositories::projects as projects_repo;

    let (db, bus) = setup().await;
    let dir = tempfile::tempdir().unwrap();
    let project_id = Uuid::new_v4().to_string();
    projects_repo::insert_project(
        db.pool(),
        &projects_repo::InsertProject {
            id: &project_id,
            name: "M31 LRGB",
            tool: "PixInsight",
            lifecycle: "setup_incomplete",
            path: dir.path().to_str().unwrap(),
            notes: None,
            canonical_target_id: None,
            is_mosaic: false,
        },
    )
    .await
    .unwrap();

    finalize_project_create_manifest(db.pool(), &bus, dir.path().to_str().unwrap()).await;

    let (rows, _) = list_manifests_for_project(db.pool(), &project_id, None, 10).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].reason, "created");
    let manifest = app_core_projects::project_manifests::get(db.pool(), &rows[0].id).await.unwrap();
    assert_eq!(manifest.manifest.body.lifecycle_state, "setup_incomplete");
}

/// An already-archived project is idempotent: the closure only (re)records
/// the plan link and never errors.
#[tokio::test]
async fn finalize_archive_lifecycle_is_idempotent_for_archived_project() {
    use persistence_plans::repositories::projects as projects_repo;

    let (db, bus) = setup().await;
    let project_id = Uuid::new_v4().to_string();
    projects_repo::insert_project(
        db.pool(),
        &projects_repo::InsertProject {
            id: &project_id,
            name: "M31",
            tool: "PixInsight",
            lifecycle: "archived",
            path: "projects/M31",
            notes: None,
            canonical_target_id: None,
            is_mosaic: false,
        },
    )
    .await
    .unwrap();

    finalize_archive_lifecycle(db.pool(), &bus, "plan-arch-2", &project_id).await;

    let project = projects_repo::get_project(db.pool(), &project_id).await.unwrap();
    assert_eq!(project.lifecycle, "archived");
    let archived = projects_repo::list_archived_projects(db.pool()).await.unwrap();
    assert_eq!(archived[0].archived_via_plan_id.as_deref(), Some("plan-arch-2"));
}

/// A non-UUID project id must not panic (best-effort logging only).
#[tokio::test]
async fn finalize_archive_lifecycle_non_uuid_is_noop() {
    let (db, bus) = setup().await;
    finalize_archive_lifecycle(db.pool(), &bus, "plan-x", "not-a-uuid").await;
    // No panic, no rows.
    let archived =
        persistence_plans::repositories::projects::list_archived_projects(db.pool()).await.unwrap();
    assert!(archived.is_empty());
}

/// Edge-legality guard (Constitution §II): if an archive plan somehow targets
/// a project that is NOT in a legal `* → archived` source state
/// (`completed`/`blocked`), the closure must refuse — leaving the lifecycle
/// unchanged and recording no archive link — rather than CAS an illegal edge
/// into `archived`.
#[tokio::test]
async fn finalize_archive_lifecycle_refuses_illegal_source_state() {
    use persistence_plans::repositories::projects as projects_repo;

    let (db, bus) = setup().await;
    let project_id = Uuid::new_v4().to_string();
    projects_repo::insert_project(
        db.pool(),
        &projects_repo::InsertProject {
            id: &project_id,
            name: "M31 Ready",
            tool: "PixInsight",
            lifecycle: "ready",
            path: "projects/M31_Ready",
            notes: None,
            canonical_target_id: None,
            is_mosaic: false,
        },
    )
    .await
    .unwrap();

    finalize_archive_lifecycle(db.pool(), &bus, "plan-arch-bad", &project_id).await;

    // Lifecycle untouched — no illegal edge recorded.
    let project = projects_repo::get_project(db.pool(), &project_id).await.unwrap();
    assert_eq!(project.lifecycle, "ready", "illegal archive source must leave lifecycle unchanged");
    // No archive link recorded.
    let archived = projects_repo::list_archived_projects(db.pool()).await.unwrap();
    assert!(archived.is_empty(), "no archive link may be recorded for a refused closure");
}

// ── #885: restore lifecycle closure ──────────────────────────────────────

/// Happy path: an archived project's finalize_restore_lifecycle drives it
/// back to `ready` and clears `archived_via_plan_id` (also exercises
/// `clear_archived_via_plan_id`, persistence_db repositories/projects.rs).
#[tokio::test]
async fn finalize_restore_lifecycle_restores_archived_project() {
    use persistence_plans::repositories::projects as projects_repo;

    let (db, bus) = setup().await;
    let project_id = Uuid::new_v4().to_string();
    projects_repo::insert_project(
        db.pool(),
        &projects_repo::InsertProject {
            id: &project_id,
            name: "M31 LRGB",
            tool: "PixInsight",
            lifecycle: "archived",
            path: "projects/M31_LRGB",
            notes: None,
            canonical_target_id: None,
            is_mosaic: false,
        },
    )
    .await
    .unwrap();
    projects_repo::set_archived_via_plan_id(db.pool(), &project_id, "plan-arch-1").await.unwrap();

    finalize_restore_lifecycle(db.pool(), &bus, &project_id).await;

    let project = projects_repo::get_project(db.pool(), &project_id).await.unwrap();
    assert_eq!(project.lifecycle, "ready", "project must be driven to ready (R-Unarchive)");

    let link: Option<String> =
        sqlx::query_scalar("SELECT archived_via_plan_id FROM projects WHERE id = ?")
            .bind(&project_id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(link, None, "archived_via_plan_id must be cleared on restore");
}

/// Edge-legality guard: the only legal source for R-Unarchive is `archived`
/// itself — a project in any other state must be left unchanged.
#[tokio::test]
async fn finalize_restore_lifecycle_refuses_non_archived_source_state() {
    use persistence_plans::repositories::projects as projects_repo;

    let (db, bus) = setup().await;
    let project_id = Uuid::new_v4().to_string();
    projects_repo::insert_project(
        db.pool(),
        &projects_repo::InsertProject {
            id: &project_id,
            name: "M31 Completed",
            tool: "PixInsight",
            lifecycle: "completed",
            path: "projects/M31_Completed",
            notes: None,
            canonical_target_id: None,
            is_mosaic: false,
        },
    )
    .await
    .unwrap();

    finalize_restore_lifecycle(db.pool(), &bus, &project_id).await;

    let project = projects_repo::get_project(db.pool(), &project_id).await.unwrap();
    assert_eq!(
        project.lifecycle, "completed",
        "illegal restore source must leave lifecycle unchanged"
    );
}

/// A non-UUID project id must not panic (best-effort logging only).
#[tokio::test]
async fn finalize_restore_lifecycle_non_uuid_is_noop() {
    let (db, bus) = setup().await;
    // No panic; nothing to assert beyond "returns".
    finalize_restore_lifecycle(db.pool(), &bus, "not-a-uuid").await;
}

// ── #886: calibration master archive lifecycle closure ──────────────────

async fn seed_calibration_master(db: &Database, id: &str) {
    sqlx::query(
        "INSERT INTO calibration_session (id, session_key, kind, created_at) \
         VALUES (?, 'k', 'dark', '2026-06-01T00:00:00Z')",
    )
    .bind(id)
    .execute(db.pool())
    .await
    .unwrap();
}

#[tokio::test]
async fn finalize_calibration_master_archive_records_flag_and_plan_link() {
    use persistence_calibration::repositories::q_calibration;

    let (db, _bus) = setup().await;
    seed_calibration_master(&db, "m-arch-1").await;

    finalize_calibration_master_archive(db.pool(), "plan-m-arch-1", "m-arch-1").await;

    let row = q_calibration::get_calibration_master(db.pool(), "m-arch-1").await.unwrap().unwrap();
    assert!(row.archived_at.is_some());
    assert_eq!(row.archived_via_plan_id.as_deref(), Some("plan-m-arch-1"));
}

#[tokio::test]
async fn finalize_calibration_master_restore_clears_flag() {
    use persistence_calibration::repositories::q_calibration;

    let (db, _bus) = setup().await;
    seed_calibration_master(&db, "m-rest-1").await;
    finalize_calibration_master_archive(db.pool(), "plan-m-rest-1", "m-rest-1").await;

    finalize_calibration_master_restore(db.pool(), "m-rest-1").await;

    let row = q_calibration::get_calibration_master(db.pool(), "m-rest-1").await.unwrap().unwrap();
    assert_eq!(row.archived_at, None);
    assert_eq!(row.archived_via_plan_id, None);
}

/// Regression: `calibration.masters.list` reads through
/// `app_core_calibration`'s process-global no-TTL snapshot cache
/// (`crates/app/cache/src/lib.rs` F0 contract — callers MUST invalidate
/// at write sites). The two tests above assert the DB write via
/// `q_calibration` directly, which bypasses the cache entirely and would
/// pass even if the finalize closures never invalidated it. This test
/// goes through the actual cache-backed read path
/// (`crate::calibration::masters_list`) both before and after
/// each closure, so a missing `invalidate_calibration_masters()` call
/// fails it (an archived master would incorrectly stay visible; a
/// restored one would incorrectly stay hidden).
#[tokio::test]
async fn finalize_calibration_master_archive_and_restore_invalidate_the_masters_cache() {
    let (db, _bus) = setup().await;
    seed_calibration_master(&db, "m-cache-1").await;

    // Defensive: this test is the only app_core (as opposed to
    // app_core_calibration) test touching the process-global cache
    // static today, but start from a known-clean slate regardless.
    crate::calibration::caches::invalidate_calibration_masters();

    // Prime the cache with the pre-archive snapshot (master visible).
    let before = crate::calibration::masters_list(db.pool()).await.unwrap();
    assert!(before.iter().any(|m| m.id == "m-cache-1"));

    finalize_calibration_master_archive(db.pool(), "plan-m-cache-1", "m-cache-1").await;

    let after_archive = crate::calibration::masters_list(db.pool()).await.unwrap();
    assert!(
        !after_archive.iter().any(|m| m.id == "m-cache-1"),
        "archived master must disappear from the CACHED masters.list read, not just the \
         direct q_calibration read — missing invalidate_calibration_masters() call"
    );

    finalize_calibration_master_restore(db.pool(), "m-cache-1").await;

    let after_restore = crate::calibration::masters_list(db.pool()).await.unwrap();
    assert!(
        after_restore.iter().any(|m| m.id == "m-cache-1"),
        "restored master must reappear in the CACHED masters.list read"
    );
}

#[tokio::test]
async fn cancel_plan_rejects_non_applying() {
    let (db, bus) = setup().await;
    repo::insert_plan(
        db.pool(),
        &repo::InsertPlan {
            id: "p2",
            title: "Test",
            origin: "cleanup",
            origin_path: None,
            plan_type: "cleanup",
            destructive_destination: "archive",
            parent_plan_id: None,
            total_bytes_required: 0,
        },
    )
    .await
    .unwrap();

    let err = cancel_plan(db.pool(), &bus, "p2").await.unwrap_err();
    assert_eq!(err.code, ErrorCode::PlanNotInApply);
}

#[tokio::test]
async fn skip_item_rejects_when_not_applying() {
    let (db, _bus) = setup().await;
    insert_approved_plan_with_items(&db, "p3", 1).await;

    let err = skip_plan_item(db.pool(), "p3", "p3-item-0").await.unwrap_err();
    assert_eq!(err.code, ErrorCode::PlanNotInApply);
}

/// Register a minimal `ActiveRun` directly in the process-global
/// registry, bypassing `apply_plan`/`resume_plan`'s executor spawn.
/// `retry_plan_item` requires a live entry before it will mutate any DB
/// state (review fix — see its doc comment); tests that exercise the
/// success path without driving a real executor need one of these.
/// Callers own removing it (or rely on process exit — the registry is a
/// `static`, so a leaked test entry cannot affect other plan ids).
fn register_fake_active_run(plan_id: &str) {
    active_runs().insert(
        plan_id.to_owned(),
        ActiveRun {
            cancel_token: CancellationToken::new(),
            skip_set: SkipSet::new(),
            retry_queue: RetryQueue::new(),
            run_id: "fake-run".to_owned(),
            path_set: crate::path_set::PlanPathSet::new(),
        },
    );
}

/// T038 gap-fill: `retry_plan_item`'s success path had zero coverage at
/// any level prior to this test (only the not-applying rejection was
/// tested). Drives the item failed -> applying transition directly
/// (bypassing the real executor, but with a fake `ActiveRun` registered
/// so the review-fix "run must be active" gate passes) and asserts both
/// the response and the persisted item state.
#[tokio::test]
async fn retry_plan_item_transitions_failed_item_to_applying() {
    let (db, _bus) = setup().await;
    insert_approved_plan_with_items(&db, "p-retry", 1).await;
    plans_repo::update_plan_state(db.pool(), "p-retry", "applying").await.unwrap();
    {
        let mut conn = db.pool().acquire().await.unwrap();
        apply_repo::batch_flush_item_states(
            &mut conn,
            "p-retry",
            &[apply_repo::BatchItemState {
                item_id: "p-retry-item-0",
                new_state: "failed",
                failure_reason: Some("permission.denied"),
                is_stale: false,
            }],
            0,
            1,
            0,
        )
        .await
        .unwrap();
    }
    register_fake_active_run("p-retry");

    let resp = retry_plan_item(db.pool(), "p-retry", "p-retry-item-0").await.unwrap();
    assert_eq!(resp.item_id, "p-retry-item-0");
    assert_eq!(resp.new_state, "applying");

    let items = plans_repo::list_plan_items(db.pool(), "p-retry").await.unwrap();
    let item = items.iter().find(|i| i.id == "p-retry-item-0").unwrap();
    assert_eq!(item.item_state, "applying", "retried item must move failed -> applying in DB");
}

/// Review fix: a retry attempted after the run has already finished
/// (no `ActiveRun` registered) must be rejected outright, not silently
/// flip the item to `applying` with nothing left to ever resolve it.
#[tokio::test]
async fn retry_plan_item_rejects_when_no_active_run() {
    let (db, _bus) = setup().await;
    insert_approved_plan_with_items(&db, "p-retry-no-run", 1).await;
    plans_repo::update_plan_state(db.pool(), "p-retry-no-run", "applying").await.unwrap();
    {
        let mut conn = db.pool().acquire().await.unwrap();
        apply_repo::batch_flush_item_states(
            &mut conn,
            "p-retry-no-run",
            &[apply_repo::BatchItemState {
                item_id: "p-retry-no-run-item-0",
                new_state: "failed",
                failure_reason: Some("permission.denied"),
                is_stale: false,
            }],
            0,
            1,
            0,
        )
        .await
        .unwrap();
    }
    // Deliberately NOT registering an ActiveRun.

    let err =
        retry_plan_item(db.pool(), "p-retry-no-run", "p-retry-no-run-item-0").await.unwrap_err();
    assert_eq!(err.code, ErrorCode::RunNotFound);

    // The DB write must never have happened — item stays failed, not
    // stuck applying with nothing to resolve it.
    let items = plans_repo::list_plan_items(db.pool(), "p-retry-no-run").await.unwrap();
    let item = items.iter().find(|i| i.id == "p-retry-no-run-item-0").unwrap();
    assert_eq!(item.item_state, "failed", "rejected retry must not mutate item state");
}

/// astro-plan-ts1z: the rollback that makes losing the retry race honest.
///
/// `retry_plan_item` flips the item to `applying` and only then enqueues it. If
/// the run disappears in that window, the enqueue finds no run and the item is
/// left `applying` with nothing that will ever execute it — and run completion
/// sweeps orphaned `applying` items to `cancelled` (`terminal.rs`). The user's
/// retry then reads as a cancellation they asked for: file unmoved, audit trail
/// saying cancelled. A Tier-1 custody failure (constitution II).
///
/// The window itself lives inside one function and cannot be opened from a test
/// without adding a production seam purely for testing, which is a worse trade.
/// So this drives the repair path directly: after a DB flip to `applying` with
/// no run to receive it, the rollback must restore `failed` AND the plan's
/// `items_failed` count, leaving the item genuinely retryable.
#[tokio::test]
async fn losing_the_retry_race_restores_the_item_to_failed_and_retryable() {
    let (db, _bus) = setup().await;
    insert_approved_plan_with_items(&db, "p-retry-race", 1).await;
    plans_repo::update_plan_state(db.pool(), "p-retry-race", "applying").await.unwrap();
    {
        let mut conn = db.pool().acquire().await.unwrap();
        apply_repo::batch_flush_item_states(
            &mut conn,
            "p-retry-race",
            &[apply_repo::BatchItemState {
                item_id: "p-retry-race-item-0",
                new_state: "failed",
                failure_reason: Some("permission.denied"),
                is_stale: false,
            }],
            0,
            1,
            0,
        )
        .await
        .unwrap();
    }

    let failed_before =
        plans_repo::get_plan(db.pool(), "p-retry-race", false).await.unwrap().items_failed;

    // The accepted-then-orphaned state: item flipped to `applying`, no run.
    apply_repo::item_retry_applying(db.pool(), "p-retry-race-item-0", "p-retry-race")
        .await
        .unwrap();
    let items = plans_repo::list_plan_items(db.pool(), "p-retry-race").await.unwrap();
    assert_eq!(
        items.iter().find(|i| i.id == "p-retry-race-item-0").unwrap().item_state,
        "applying",
        "precondition: the item is stranded `applying`, which is what the sweep would cancel"
    );

    apply_repo::item_retry_rollback_to_failed(db.pool(), "p-retry-race-item-0", "p-retry-race")
        .await
        .unwrap();

    let items = plans_repo::list_plan_items(db.pool(), "p-retry-race").await.unwrap();
    let item = items.iter().find(|i| i.id == "p-retry-race-item-0").unwrap();
    assert_eq!(
        item.item_state, "failed",
        "a retry that could not be handed to an executor must leave the item FAILED — \
         retryable and honest — not stranded `applying` for the sweep to cancel"
    );

    let failed_after =
        plans_repo::get_plan(db.pool(), "p-retry-race", false).await.unwrap().items_failed;
    assert_eq!(
        failed_after, failed_before,
        "items_failed must return to its pre-retry value, or the plan's terminal state is \
         computed from a wrong count"
    );

    // Idempotent: a second rollback must not inflate the counter. This matters
    // because the rollback is best-effort and could be reached twice.
    apply_repo::item_retry_rollback_to_failed(db.pool(), "p-retry-race-item-0", "p-retry-race")
        .await
        .unwrap();
    let failed_twice =
        plans_repo::get_plan(db.pool(), "p-retry-race", false).await.unwrap().items_failed;
    assert_eq!(
        failed_twice, failed_before,
        "rollback is guarded on item_state='applying', so repeating it must be a no-op"
    );
}

/// astro-plan-ts1z window (c): a retry the run accepted but never executed
/// must not be recorded as `cancelled`.
///
/// The completion sweep (`cancel_orphaned_applying_items`) turns any item left
/// `applying` into `cancelled`, which reads identically to a cancellation the
/// user asked for — the UI has nothing to distinguish them by and presents the
/// item as done. `restore_unexecuted_retries` runs first and puts the item back
/// to `failed` with a `retry.not_executed` audit event, so the item stays
/// retryable and the reason is on the record.
#[tokio::test]
async fn an_accepted_but_unexecuted_retry_is_restored_to_failed_not_cancelled() {
    let (db, _bus) = setup().await;
    insert_approved_plan_with_items(&db, "p-orphan", 1).await;
    // A real run row: the audit event's `run_id` is a foreign key, so a fake id
    // would make the append silently fail and the reason never be recorded.
    plans_repo::update_plan_state(db.pool(), "p-orphan", "ready_for_review").await.unwrap();
    plans_repo::set_approved(db.pool(), "p-orphan", "2026-06-01T00:00:00Z", "tok").await.unwrap();
    apply_repo::cas_approved_to_applying(db.pool(), "p-orphan", "run-orphan", "tok", 1, 1)
        .await
        .unwrap();
    {
        let mut conn = db.pool().acquire().await.unwrap();
        apply_repo::batch_flush_item_states(
            &mut conn,
            "p-orphan",
            &[apply_repo::BatchItemState {
                item_id: "p-orphan-item-0",
                new_state: "failed",
                failure_reason: Some("permission.denied"),
                is_stale: false,
            }],
            0,
            1,
            0,
        )
        .await
        .unwrap();
    }

    // The state `execute_plan` leaves behind when it halts over an accepted
    // retry: the item's row is `applying` and the queue reports it as an orphan.
    let retry_queue = RetryQueue::new();
    assert!(retry_queue.push("p-orphan-item-0"));
    retry_queue.close();
    apply_repo::item_retry_applying(db.pool(), "p-orphan-item-0", "p-orphan").await.unwrap();

    super::apply::restore_unexecuted_retries(db.pool(), "p-orphan", "run-orphan", &retry_queue)
        .await;

    let items = plans_repo::list_plan_items(db.pool(), "p-orphan").await.unwrap();
    let item = items.iter().find(|i| i.id == "p-orphan-item-0").unwrap();
    assert_eq!(
        item.item_state, "failed",
        "an accepted retry that never ran must be FAILED — retryable — not left `applying` for \
         the completion sweep to record as `cancelled`"
    );

    let reason: Option<String> = sqlx::query_scalar(
        "SELECT failure_code FROM plan_apply_events \
         WHERE plan_id = ? AND item_id = ? AND new_state = 'failed' \
         ORDER BY rowid DESC LIMIT 1",
    )
    .bind("p-orphan")
    .bind("p-orphan-item-0")
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(
        reason.as_deref(),
        Some(super::apply::RETRY_NOT_EXECUTED_REASON),
        "the audit record must name WHY the retry did not run, so a surface can offer it again \
         instead of presenting a cancellation the user never asked for"
    );

    // Now the sweep the terminal handlers run: it must find nothing, because
    // the restore already happened.
    let swept = apply_repo::cancel_orphaned_applying_items(db.pool(), "p-orphan").await.unwrap();
    assert!(
        swept.is_empty(),
        "the restore runs before the sweep, so the sweep has no `applying` item to cancel"
    );
}

#[tokio::test]
async fn retry_plan_item_rejects_non_failed_item() {
    let (db, _bus) = setup().await;
    insert_approved_plan_with_items(&db, "p-retry2", 1).await;
    plans_repo::update_plan_state(db.pool(), "p-retry2", "applying").await.unwrap();

    // Item is still `pending` (never failed) — retry must reject it
    // before reaching the active-run check (which runs after).
    let err = retry_plan_item(db.pool(), "p-retry2", "p-retry2-item-0").await.unwrap_err();
    assert_eq!(err.code, ErrorCode::ItemNotFailed);
}

#[tokio::test]
async fn confirm_plan_destructive_items_rejects_unknown_plan() {
    let (db, _bus) = setup().await;
    let err = confirm_plan_destructive_items(db.pool(), "missing-plan").await.unwrap_err();
    assert_eq!(err.code, ErrorCode::PlanNotFound);
}

#[tokio::test]
async fn confirm_plan_destructive_items_persists_flag() {
    let (db, _bus) = setup().await;
    repo::insert_plan(
        db.pool(),
        &repo::InsertPlan {
            id: "p-del",
            title: "Test",
            origin: "cleanup",
            origin_path: None,
            plan_type: "cleanup",
            destructive_destination: "trash",
            parent_plan_id: None,
            total_bytes_required: 0,
        },
    )
    .await
    .unwrap();
    repo::insert_plan_item(
        db.pool(),
        &repo::InsertPlanItem {
            id: "p-del-item-0",
            plan_id: "p-del",
            item_index: 1,
            name: "junk.fits",
            action: "delete",
            from_root_id: None,
            from_relative_path: "p-del/raw/junk.fits",
            to_root_id: None,
            to_relative_path: "",
            reason: "test",
            protection: "normal",
            linked_entity: None,
            provenance_json: None,
            archive_path: None,
            source_id: None,
            category: None,
        },
    )
    .await
    .unwrap();

    let before = repo::list_plan_items(db.pool(), "p-del").await.unwrap();
    assert_eq!(before[0].destructive_confirmed, 0);

    let confirmed = confirm_plan_destructive_items(db.pool(), "p-del").await.unwrap();
    assert_eq!(confirmed, 1);

    let after = repo::list_plan_items(db.pool(), "p-del").await.unwrap();
    assert_eq!(after[0].destructive_confirmed, 1);

    // Idempotent second call.
    let confirmed_again = confirm_plan_destructive_items(db.pool(), "p-del").await.unwrap();
    assert_eq!(confirmed_again, 0);
}

/// End-to-end regression for issue #741: before this fix, a delete item
/// was refused *permanently* at apply time (`destructive_confirmed` had
/// no writer anywhere). Confirming via the new write path must let a
/// subsequent apply actually delete the file on disk.
#[tokio::test]
async fn confirm_then_apply_executes_previously_refused_delete_item() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("junk.fits");
    std::fs::write(&file_path, b"data").unwrap();
    let abs = file_path.to_str().unwrap();

    let (db, bus) = setup().await;
    repo::insert_plan(
        db.pool(),
        &repo::InsertPlan {
            id: "p-e2e",
            title: "Test",
            origin: "cleanup",
            origin_path: None,
            plan_type: "cleanup",
            destructive_destination: "trash",
            parent_plan_id: None,
            total_bytes_required: 0,
        },
    )
    .await
    .unwrap();
    repo::insert_plan_item(
        db.pool(),
        &repo::InsertPlanItem {
            id: "p-e2e-item-0",
            plan_id: "p-e2e",
            item_index: 1,
            name: "junk.fits",
            action: "delete",
            // No from_root_id: item_row_to_executor_item leaves
            // library_root None, so `from_relative_path` is used as-is —
            // an absolute temp-file path works (mirrors the executor
            // crate's own "legacy" no-root test items).
            from_root_id: None,
            from_relative_path: abs,
            to_root_id: None,
            to_relative_path: "",
            reason: "test",
            protection: "normal",
            linked_entity: None,
            provenance_json: None,
            archive_path: None,
            source_id: None,
            category: None,
        },
    )
    .await
    .unwrap();

    confirm_plan_destructive_items(db.pool(), "p-e2e").await.unwrap();

    repo::update_plan_state(db.pool(), "p-e2e", "ready_for_review").await.unwrap();
    repo::set_approved(db.pool(), "p-e2e", "2026-06-01T00:00:00Z", "test-token").await.unwrap();

    apply_plan(db.pool(), &bus, "p-e2e", "test-token", None).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

    assert!(!file_path.exists(), "confirmed delete item must actually execute");
    let plan = repo::get_plan(db.pool(), "p-e2e", false).await.unwrap();
    assert_eq!(plan.state, "applied");

    // #766: a real, successful plan apply must write a durable
    // audit_log_entry row per succeeded plan_item — not just the
    // separate plan-apply run-events table.
    let audit_count = count_audit_entries(db.pool(), &AuditLogFilter::default()).await.unwrap();
    assert!(audit_count > 0, "apply_plan must write at least one durable audit_log_entry row");
}

/// Regression for the "trash destination is dead code" finding: both
/// `cleanup_generator` and `archive_generator` always store
/// `action = "archive"` for a destructive-but-reversible item; the
/// user's plan-level "System trash" choice (`plans.destructive_destination`)
/// was never consulted at apply time, so it silently archived into
/// `.astro-plan-archive` regardless of what the user picked in review.
/// `fs_executor::ops::trash_op::fake` (headless-safe OS-trash double, added for
/// the e2e harness) makes the OS-trash outcome deterministic here too.
#[tokio::test]
async fn archive_action_item_with_trash_destination_really_trashes() {
    // Restores real OS trash on drop, including on panic unwind, so a failed
    // assertion below cannot divert the trash path of the rest of this binary.
    let _fake_trash = fs_executor::ops::trash_op::fake::FakeTrashGuard::new();

    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("intermediate.fits");
    std::fs::write(&file_path, b"data").unwrap();
    let abs = file_path.to_str().unwrap();

    let (db, bus) = setup().await;
    repo::insert_plan(
        db.pool(),
        &repo::InsertPlan {
            id: "p-trash-e2e",
            title: "Test",
            origin: "cleanup",
            origin_path: None,
            plan_type: "cleanup",
            destructive_destination: "trash",
            parent_plan_id: None,
            total_bytes_required: 0,
        },
    )
    .await
    .unwrap();
    repo::insert_plan_item(
        db.pool(),
        &repo::InsertPlanItem {
            id: "p-trash-e2e-item-0",
            plan_id: "p-trash-e2e",
            item_index: 1,
            name: "intermediate.fits",
            action: "archive",
            from_root_id: None,
            from_relative_path: abs,
            to_root_id: None,
            to_relative_path: "",
            reason: "test",
            protection: "normal",
            linked_entity: None,
            provenance_json: None,
            archive_path: None,
            source_id: None,
            category: None,
        },
    )
    .await
    .unwrap();

    // The trash reroute makes this item destructive, so it now has to clear the
    // executor's confirm gate like any other trash item.
    let confirmed = confirm_plan_destructive_items(db.pool(), "p-trash-e2e").await.unwrap();
    assert_eq!(confirmed, 1, "the archive-on-trash item must be confirmable");

    repo::update_plan_state(db.pool(), "p-trash-e2e", "ready_for_review").await.unwrap();
    repo::set_approved(db.pool(), "p-trash-e2e", "2026-06-01T00:00:00Z", "test-token")
        .await
        .unwrap();

    apply_plan(db.pool(), &bus, "p-trash-e2e", "test-token", None).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

    assert!(
        !file_path.exists(),
        "an archive-action item under a trash-destination plan must actually be removed via trash"
    );
    assert!(
        !dir.path().join(".astro-plan-archive").exists(),
        "a trash-destination item must not fall through to the app archive folder"
    );
    let plan = repo::get_plan(db.pool(), "p-trash-e2e", false).await.unwrap();
    assert_eq!(plan.state, "applied");
    let items = repo::list_plan_items(db.pool(), "p-trash-e2e").await.unwrap();
    assert_eq!(items[0].item_state, "succeeded");
}

/// Sibling of the trash-routing regression above, guarding the inverse:
/// a plan whose `destructive_destination` stays `"archive"` must still
/// route its `action = "archive"` item through `ExecutorItemAction::Archive`
/// (file lands under the archive path, never removed). Without this, a
/// guard bug matching plain `"archive"` (routing every archive item to
/// Trash regardless of `destructive_destination`) would pass the trash
/// test above undetected — no existing `item_row_to_executor_item` test
/// asserts on `item.action`.
#[tokio::test]
async fn archive_action_item_with_archive_destination_stays_archived() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("intermediate.fits");
    std::fs::write(&file_path, b"data").unwrap();
    let abs = file_path.to_str().unwrap();
    let archive_dest_path = dir.path().join(".astro-plan-archive/p-archive-e2e-item-0.fits");
    let archive_dest = archive_dest_path.to_str().unwrap();

    let (db, bus) = setup().await;
    repo::insert_plan(
        db.pool(),
        &repo::InsertPlan {
            id: "p-archive-e2e",
            title: "Test",
            origin: "cleanup",
            origin_path: None,
            plan_type: "cleanup",
            destructive_destination: "archive",
            parent_plan_id: None,
            total_bytes_required: 0,
        },
    )
    .await
    .unwrap();
    repo::insert_plan_item(
        db.pool(),
        &repo::InsertPlanItem {
            id: "p-archive-e2e-item-0",
            plan_id: "p-archive-e2e",
            item_index: 1,
            name: "intermediate.fits",
            action: "archive",
            from_root_id: None,
            from_relative_path: abs,
            to_root_id: None,
            to_relative_path: "",
            reason: "test",
            protection: "normal",
            linked_entity: None,
            provenance_json: None,
            archive_path: Some(archive_dest),
            source_id: None,
            category: None,
        },
    )
    .await
    .unwrap();

    repo::update_plan_state(db.pool(), "p-archive-e2e", "ready_for_review").await.unwrap();
    repo::set_approved(db.pool(), "p-archive-e2e", "2026-06-01T00:00:00Z", "test-token")
        .await
        .unwrap();

    apply_plan(db.pool(), &bus, "p-archive-e2e", "test-token", None).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

    assert!(!file_path.exists(), "source must be gone after a successful archive move");
    assert!(
        archive_dest_path.exists(),
        "an archive-destination plan's archive-action item must land at the archive path, not be trashed/deleted"
    );
    let plan = repo::get_plan(db.pool(), "p-archive-e2e", false).await.unwrap();
    assert_eq!(plan.state, "applied");
    let items = repo::list_plan_items(db.pool(), "p-archive-e2e").await.unwrap();
    assert_eq!(items[0].item_state, "succeeded");
}

/// Builds an `action = "archive"` row, the only shape a generator ever stores
/// for a destructive-but-reversible item.
fn archive_row(id: &str) -> plans_repo::PlanItemRow {
    plans_repo::PlanItemRow {
        id: id.to_owned(),
        plan_id: "plan-confirm-derivation".to_owned(),
        item_index: 1,
        name: "intermediate.fits".to_owned(),
        action: "archive".to_owned(),
        from_root_id: None,
        from_relative_path: "raw/intermediate.fits".to_owned(),
        to_root_id: None,
        to_relative_path: String::new(),
        reason: "test".to_owned(),
        protection: "normal".to_owned(),
        linked_entity: None,
        item_state: "pending".to_owned(),
        failure_reason: None,
        provenance: None,
        approved_mtime: None,
        approved_size_bytes: None,
        archive_path: None,
        created_at: "2026-08-23T00:00:00Z".to_owned(),
        source_id: None,
        category: None,
        requires_destructive_confirm: None,
        resolved_pattern: None,
        destructive_confirmed: 0,
    }
}

/// The confirm-gate half of the trash-reroute fix: `requires_destructive_confirm`
/// must follow the EFFECTIVE executor action, so an `action = "archive"` item on
/// a `destructive_destination = "trash"` plan is gated, while the same row on an
/// `"archive"` plan is not (the archive arm is a real move, not a removal).
#[test]
fn archive_item_requires_confirm_only_under_a_trash_destination() {
    let root_map: HashMap<String, Utf8PathBuf> = HashMap::new();

    let trashed = item_row_to_executor_item(&archive_row("item-trash"), &root_map, "trash", None);
    assert!(
        matches!(trashed.action, ExecutorItemAction::Trash { .. }),
        "sanity: a trash-destination plan reroutes the archive item to Trash"
    );
    assert!(
        trashed.requires_destructive_confirm,
        "an archive item rerouted to OS trash removes the file and must be gated on confirmation"
    );

    let archived =
        item_row_to_executor_item(&archive_row("item-archive"), &root_map, "archive", None);
    assert!(
        matches!(archived.action, ExecutorItemAction::Archive { .. }),
        "sanity: an archive-destination plan keeps the archive action"
    );
    assert!(
        !archived.requires_destructive_confirm,
        "archiving is a reversible move — gating it would demand confirmation for every archive \
         plan"
    );
}

/// The writer half, which must stay at least as wide as the refuser above: a
/// user who confirms a trash plan has to actually clear the gate for its
/// archive-action items, and confirming an archive plan must still flip nothing.
#[tokio::test]
async fn confirm_covers_archive_items_only_under_a_trash_destination() {
    let (db, _bus) = setup().await;

    for (plan_id, destination) in [("p-cw-trash", "trash"), ("p-cw-archive", "archive")] {
        repo::insert_plan(
            db.pool(),
            &repo::InsertPlan {
                id: plan_id,
                title: "Test",
                origin: "cleanup",
                origin_path: None,
                plan_type: "cleanup",
                destructive_destination: destination,
                parent_plan_id: None,
                total_bytes_required: 0,
            },
        )
        .await
        .unwrap();
        repo::insert_plan_item(
            db.pool(),
            &repo::InsertPlanItem {
                id: &format!("{plan_id}-item-0"),
                plan_id,
                item_index: 1,
                name: "intermediate.fits",
                action: "archive",
                from_root_id: None,
                from_relative_path: &format!("{plan_id}/raw/intermediate.fits"),
                to_root_id: None,
                to_relative_path: &format!("{plan_id}/archive/intermediate.fits"),
                reason: "test",
                protection: "normal",
                linked_entity: None,
                provenance_json: None,
                archive_path: None,
                source_id: None,
                category: None,
            },
        )
        .await
        .unwrap();
    }

    let trash_confirmed = confirm_plan_destructive_items(db.pool(), "p-cw-trash").await.unwrap();
    assert_eq!(
        trash_confirmed, 1,
        "the confirm writer must cover every item the executor gates, or a confirmed trash plan \
         refuses all of its items"
    );
    let trash_items = repo::list_plan_items(db.pool(), "p-cw-trash").await.unwrap();
    assert_eq!(trash_items[0].destructive_confirmed, 1);

    let archive_confirmed =
        confirm_plan_destructive_items(db.pool(), "p-cw-archive").await.unwrap();
    assert_eq!(
        archive_confirmed, 0,
        "an archive-destination plan has nothing destructive to confirm"
    );
    let archive_items = repo::list_plan_items(db.pool(), "p-cw-archive").await.unwrap();
    assert_eq!(archive_items[0].destructive_confirmed, 0);

    // Idempotent: the widened predicate must not re-count a confirmed item.
    let again = confirm_plan_destructive_items(db.pool(), "p-cw-trash").await.unwrap();
    assert_eq!(again, 0);
}

/// End-to-end sibling of `archive_action_item_with_trash_destination_really_trashes`:
/// the same plan shape WITHOUT a confirm call must be refused before
/// `trash::delete` is reached, so the file is still on disk afterwards. The fake
/// trash guard turns a leaked removal into a deleted file rather than a real
/// OS-Trash move, keeping the assertion observable in a headless run.
#[tokio::test]
async fn unconfirmed_archive_item_under_trash_destination_is_refused_before_trashing() {
    let _fake_trash = fs_executor::ops::trash_op::fake::FakeTrashGuard::new();

    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("intermediate.fits");
    std::fs::write(&file_path, b"data").unwrap();
    let abs = file_path.to_str().unwrap();

    let (db, bus) = setup().await;
    repo::insert_plan(
        db.pool(),
        &repo::InsertPlan {
            id: "p-trash-unconfirmed",
            title: "Test",
            origin: "cleanup",
            origin_path: None,
            plan_type: "cleanup",
            destructive_destination: "trash",
            parent_plan_id: None,
            total_bytes_required: 0,
        },
    )
    .await
    .unwrap();
    repo::insert_plan_item(
        db.pool(),
        &repo::InsertPlanItem {
            id: "p-trash-unconfirmed-item-0",
            plan_id: "p-trash-unconfirmed",
            item_index: 1,
            name: "intermediate.fits",
            action: "archive",
            from_root_id: None,
            from_relative_path: abs,
            to_root_id: None,
            to_relative_path: "",
            reason: "test",
            protection: "normal",
            linked_entity: None,
            provenance_json: None,
            archive_path: None,
            source_id: None,
            category: None,
        },
    )
    .await
    .unwrap();

    repo::update_plan_state(db.pool(), "p-trash-unconfirmed", "ready_for_review").await.unwrap();
    repo::set_approved(db.pool(), "p-trash-unconfirmed", "2026-06-01T00:00:00Z", "test-token")
        .await
        .unwrap();

    apply_plan(db.pool(), &bus, "p-trash-unconfirmed", "test-token", None).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

    assert!(
        file_path.exists(),
        "an unconfirmed archive-on-trash item must never reach trash::delete"
    );

    let failure_code: Option<String> = sqlx::query_scalar(
        "SELECT failure_code FROM plan_apply_events \
         WHERE plan_id = ? AND item_id = ? AND failure_code IS NOT NULL \
         ORDER BY rowid DESC LIMIT 1",
    )
    .bind("p-trash-unconfirmed")
    .bind("p-trash-unconfirmed-item-0")
    .fetch_optional(db.pool())
    .await
    .unwrap()
    .flatten();
    assert_eq!(
        failure_code.as_deref(),
        Some(FailureCode::DestructiveUnconfirmed.as_str()),
        "the refusal must be recorded as destructive_unconfirmed"
    );
}

/// #766: one durable `audit_log_entry` row per succeeded plan_item
/// (query DB, not the live EventBus) — the exact SUCCESS criterion from
/// the issue repro.
#[tokio::test]
async fn n766_apply_writes_one_durable_audit_row_per_succeeded_item() {
    let (db, bus) = setup().await;
    insert_approved_plan_with_items(&db, "p-audit", 2).await;

    apply_plan(db.pool(), &bus, "p-audit", "test-token", None).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

    let plan = repo::get_plan(db.pool(), "p-audit", false).await.unwrap();
    // Items have no real file on disk (from_root_id: None, relative path
    // used as-is) so they resolve to a terminal `failed` state — still a
    // real "attempted action and outcome" that must be audited (§II).
    assert_eq!(plan.items_total, 2);

    let audit_count = count_audit_entries(db.pool(), &AuditLogFilter::default()).await.unwrap();
    assert!(
        i64::from(audit_count) >= plan.items_total,
        "expected at least one audit_log_entry row per plan item ({} items), got {audit_count}",
        plan.items_total
    );

    let entries = list_audit_entries(
        db.pool(),
        &AuditLogFilter { entity_type: Some("filesystem_plan".to_owned()), ..Default::default() },
    )
    .await
    .unwrap();
    assert!(
        entries.iter().any(|e| e.trigger.starts_with("plan_item.")),
        "expected a plan_item.* durable audit trigger"
    );
}

/// #750: `audit_item_cancelled` (the per-item write both bulk-cancel
/// paths — happy-path pending list and orphaned-`applying` sweep — funnel
/// through) must write a durable `audit_log_entry` row, not just a
/// run-events row, for each cancelled item.
#[tokio::test]
async fn n750_audit_item_cancelled_writes_durable_audit_row() {
    let (db, bus) = setup().await;
    insert_approved_plan_with_items(&db, "p-cancel", 1).await;
    repo::update_plan_state(db.pool(), "p-cancel", "applying").await.unwrap();

    audit_item_cancelled(
        db.pool(),
        &bus,
        "run-cancel",
        "p-cancel",
        "p-cancel-item-0",
        "pending",
        "2026-06-01T00:00:00Z",
    )
    .await;

    let audit_count = count_audit_entries(db.pool(), &AuditLogFilter::default()).await.unwrap();
    assert_eq!(audit_count, 1, "one durable audit_log_entry row per cancelled item");

    let entries = list_audit_entries(db.pool(), &AuditLogFilter::default()).await.unwrap();
    assert_eq!(entries[0].trigger, "plan_item.cancelled");
    assert_eq!(entries[0].outcome, "refused");
    assert_eq!(entries[0].to_state.as_deref(), Some("cancelled"));
}

#[tokio::test]
async fn get_apply_status_returns_plan_state() {
    let (db, _bus) = setup().await;
    insert_approved_plan_with_items(&db, "p4", 2).await;

    let status = get_apply_status(db.pool(), "p4").await.unwrap();
    assert_eq!(status.plan_id, "p4");
    assert_eq!(status.plan_state, "approved");
    assert_eq!(status.items_total, 2);
    assert!(status.run_id.is_none());
}

#[tokio::test]
async fn verify_approval_token_rejects_mismatched_token() {
    let result = verify_approval_token(Some("stored-token"), "different-token");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code, ErrorCode::PlanApprovalStale);
}

#[tokio::test]
async fn verify_approval_token_rejects_missing_token() {
    let result = verify_approval_token(None, "any-token");
    assert!(result.is_err());
}

#[tokio::test]
async fn verify_approval_token_accepts_matching_token() {
    let result = verify_approval_token(Some("tok-abc"), "tok-abc");
    assert!(result.is_ok());
}

// ── T023a tests ───────────────────────────────────────────────────────────

/// T023a: item_row_to_executor_item sets library_root from the root_map
/// so the path-gate fires on real plan items.
#[test]
fn t023a_library_root_resolved_from_map() {
    let row = plans_repo::PlanItemRow {
        id: "item-1".to_owned(),
        plan_id: "plan-1".to_owned(),
        item_index: 1,
        name: "file.fits".to_owned(),
        action: "move".to_owned(),
        from_root_id: Some("root-001".to_owned()),
        from_relative_path: "raw/file.fits".to_owned(),
        to_root_id: Some("root-001".to_owned()),
        to_relative_path: "archive/file.fits".to_owned(),
        reason: "test".to_owned(),
        protection: "normal".to_owned(),
        linked_entity: None,
        item_state: "pending".to_owned(),
        failure_reason: None,
        provenance: None,
        approved_mtime: None,
        approved_size_bytes: None,
        archive_path: None,
        created_at: "2026-06-17T00:00:00Z".to_owned(),
        source_id: None,
        category: None,
        requires_destructive_confirm: Some(0),
        resolved_pattern: None,
        destructive_confirmed: 0,
    };

    let mut root_map = HashMap::new();
    root_map.insert("root-001".to_owned(), Utf8PathBuf::from("/mnt/library"));

    let item = item_row_to_executor_item(&row, &root_map, "archive", None);
    assert_eq!(
        item.library_root,
        Some(Utf8PathBuf::from("/mnt/library")),
        "library_root must be populated from the root_map so the path gate fires"
    );
}

/// T023a: item without from_root_id gets library_root = None (legacy/unknown mode).
#[test]
fn t023a_no_root_id_gives_none_library_root() {
    let row = plans_repo::PlanItemRow {
        id: "item-2".to_owned(),
        plan_id: "plan-1".to_owned(),
        item_index: 1,
        name: "file.fits".to_owned(),
        action: "move".to_owned(),
        from_root_id: None,
        from_relative_path: "raw/file.fits".to_owned(),
        to_root_id: None,
        to_relative_path: "archive/file.fits".to_owned(),
        reason: "test".to_owned(),
        protection: "normal".to_owned(),
        linked_entity: None,
        item_state: "pending".to_owned(),
        failure_reason: None,
        provenance: None,
        approved_mtime: None,
        approved_size_bytes: None,
        archive_path: None,
        created_at: "2026-06-17T00:00:00Z".to_owned(),
        source_id: None,
        category: None,
        requires_destructive_confirm: Some(0),
        resolved_pattern: None,
        destructive_confirmed: 0,
    };

    let root_map: HashMap<String, Utf8PathBuf> = HashMap::new();
    let item = item_row_to_executor_item(&row, &root_map, "archive", None);
    assert_eq!(item.library_root, None);
}

/// #765: a cross-root item (`to_root_id != from_root_id`) must resolve
/// `destination_root` from `to_root_id`, independent of `library_root`
/// (which stays resolved from `from_root_id`) — otherwise the executor
/// joins the destination path against the wrong (source) root.
#[test]
fn n765_destination_root_resolves_independently_from_to_root_id() {
    let row = plans_repo::PlanItemRow {
        id: "item-cross-root".to_owned(),
        plan_id: "plan-1".to_owned(),
        item_index: 1,
        name: "file.fits".to_owned(),
        action: "move".to_owned(),
        from_root_id: Some("inbox-root".to_owned()),
        from_relative_path: "M51/LUM/file.fits".to_owned(),
        to_root_id: Some("lights-root".to_owned()),
        to_relative_path: "M51/LUM/file.fits".to_owned(),
        reason: "test".to_owned(),
        protection: "normal".to_owned(),
        linked_entity: None,
        item_state: "pending".to_owned(),
        failure_reason: None,
        provenance: None,
        approved_mtime: None,
        approved_size_bytes: None,
        archive_path: None,
        created_at: "2026-06-17T00:00:00Z".to_owned(),
        source_id: None,
        category: None,
        requires_destructive_confirm: Some(0),
        resolved_pattern: None,
        destructive_confirmed: 0,
    };

    let mut root_map = HashMap::new();
    root_map.insert("inbox-root".to_owned(), Utf8PathBuf::from("/mnt/inbox"));
    root_map.insert("lights-root".to_owned(), Utf8PathBuf::from("/mnt/lights/1"));

    let item = item_row_to_executor_item(&row, &root_map, "archive", None);
    assert_eq!(
        item.library_root,
        Some(Utf8PathBuf::from("/mnt/inbox")),
        "library_root (source) must resolve from from_root_id"
    );
    assert_eq!(
        item.destination_root,
        Some(Utf8PathBuf::from("/mnt/lights/1")),
        "destination_root must resolve from to_root_id, not from_root_id"
    );
}

/// #765: when `to_root_id` is absent or unresolvable, `destination_root`
/// falls back to `library_root` (same-root actions: archive/trash/
/// catalogue, or legacy rows without a recorded destination root).
#[test]
fn n765_destination_root_falls_back_to_library_root_when_to_root_id_absent() {
    let row = plans_repo::PlanItemRow {
        id: "item-same-root".to_owned(),
        plan_id: "plan-1".to_owned(),
        item_index: 1,
        name: "file.fits".to_owned(),
        action: "archive".to_owned(),
        from_root_id: Some("root-001".to_owned()),
        from_relative_path: "raw/file.fits".to_owned(),
        to_root_id: None,
        to_relative_path: "archive/file.fits".to_owned(),
        reason: "test".to_owned(),
        protection: "normal".to_owned(),
        linked_entity: None,
        item_state: "pending".to_owned(),
        failure_reason: None,
        provenance: None,
        approved_mtime: None,
        approved_size_bytes: None,
        archive_path: None,
        created_at: "2026-06-17T00:00:00Z".to_owned(),
        source_id: None,
        category: None,
        requires_destructive_confirm: Some(0),
        resolved_pattern: None,
        destructive_confirmed: 0,
    };

    let mut root_map = HashMap::new();
    root_map.insert("root-001".to_owned(), Utf8PathBuf::from("/mnt/library"));

    let item = item_row_to_executor_item(&row, &root_map, "archive", None);
    assert_eq!(item.destination_root, item.library_root);
    assert_eq!(item.destination_root, Some(Utf8PathBuf::from("/mnt/library")));
}

/// A path that `Utf8Path::is_absolute` accepts on the platform running the test.
///
/// A POSIX literal such as `/mnt/x` is root-relative on Windows, where
/// absoluteness needs a drive or UNC prefix. Hardcoding one makes an
/// absolute-destination test silently exercise the relative branch there
/// (astro-plan-d8cyr round 4: run 32580340909 job 97048640912).
/// `std::env::temp_dir` is prefixed on every platform and touches no disk,
/// which keeps these row-mapping tests pure.
fn absolute_test_path(tail: &str) -> Utf8PathBuf {
    let base =
        Utf8PathBuf::from_path_buf(std::env::temp_dir()).expect("temp dir path is UTF-8 in tests");
    let path = base.join(tail);
    assert!(path.is_absolute(), "test fixture must be absolute on this platform: {path}");
    path
}

fn source_view_item_row(to_relative_path: &str) -> plans_repo::PlanItemRow {
    plans_repo::PlanItemRow {
        id: "item-abs-dest".to_owned(),
        plan_id: "plan-pre-0003".to_owned(),
        item_index: 1,
        name: "frame.fits".to_owned(),
        action: "link".to_owned(),
        from_root_id: Some("root-001".to_owned()),
        from_relative_path: "raw/frame.fits".to_owned(),
        to_root_id: None,
        to_relative_path: to_relative_path.to_owned(),
        reason: "view_generation".to_owned(),
        protection: "normal".to_owned(),
        linked_entity: None,
        item_state: "pending".to_owned(),
        failure_reason: None,
        provenance: None,
        approved_mtime: None,
        approved_size_bytes: None,
        archive_path: None,
        created_at: "2026-06-17T00:00:00Z".to_owned(),
        source_id: None,
        category: None,
        requires_destructive_confirm: Some(0),
        resolved_pattern: None,
        destructive_confirmed: 0,
    }
}

/// astro-plan-d8cyr FIX 1: a `source_view_generation` plan row written before
/// migration 0003 has `to_root_id=None`, `from_root_id=Some` and an ABSOLUTE
/// `to_relative_path`. Falling back to `library_root` (the SOURCE root) gates
/// that destination against a root it never belonged to, so every item of the
/// user's existing plan is refused `root_escape`, which is non-retryable.
/// `destination_root` must stay `None` so the destination gate is inactive and
/// the plan applies as it did before the column existed.
#[test]
fn absolute_destination_takes_no_library_root_fallback() {
    let view_root = absolute_test_path("projects/m101/source-views/plan-pre-0003");
    let row = source_view_item_row(view_root.join("lights/frame.fits").as_str());

    let mut root_map = HashMap::new();
    root_map.insert("root-001".to_owned(), absolute_test_path("library"));

    let item = item_row_to_executor_item(&row, &root_map, "archive", None);
    assert_eq!(item.library_root, Some(absolute_test_path("library")));
    assert_eq!(
        item.destination_root, None,
        "an absolute destination must not inherit the source root"
    );

    // The same row in a plan that DOES record its destination root is gated.
    let gated = item_row_to_executor_item(&row, &root_map, "archive", Some(&view_root));
    assert_eq!(gated.destination_root, Some(view_root));
}

/// The sibling of the case above: a RELATIVE destination still takes the #765
/// `library_root` fallback, so the no-fallback rule is scoped to absolute
/// destinations rather than having disabled the fallback outright. Both cases
/// run on every platform.
#[test]
fn relative_destination_still_takes_library_root_fallback() {
    let library = absolute_test_path("library");
    let row = source_view_item_row("archive/frame.fits");

    let mut root_map = HashMap::new();
    root_map.insert("root-001".to_owned(), library.clone());

    let item = item_row_to_executor_item(&row, &root_map, "archive", None);
    assert_eq!(item.destination_root, Some(library));
}

/// T023a: root-escaping relative path is refused by the gate when library_root is set.
/// This proves the gate is active on real plan items (not inert).
#[test]
fn t023a_root_escape_gate_fires_when_library_root_is_set() {
    use fs_executor::ops::path_gate;

    let root = Utf8PathBuf::from(fs_pathsafe::test_support::abs("/mnt/library"));
    // A path that escapes the root via ".." — must be refused.
    let escaping_relative = Utf8PathBuf::from("../../etc/passwd");

    let result = path_gate::resolve_and_validate(&root, &escaping_relative);
    assert!(result.is_err(), "root-escaping path must be refused when library_root is set");
    let failure = result.unwrap_err();
    assert_eq!(failure.code.as_str(), "root_escape", "failure code must be root_escape");
}

/// T023a: destructive_confirmed is now a real DB column (migration 0033),
/// read directly (not defaulted via #[sqlx(default)]).
#[test]
fn t023a_destructive_confirmed_reads_from_db_column() {
    let row = plans_repo::PlanItemRow {
        id: "item-3".to_owned(),
        plan_id: "plan-1".to_owned(),
        item_index: 1,
        name: "file.fits".to_owned(),
        action: "delete".to_owned(),
        from_root_id: None,
        from_relative_path: "raw/file.fits".to_owned(),
        to_root_id: None,
        to_relative_path: String::new(),
        reason: "test".to_owned(),
        protection: "normal".to_owned(),
        linked_entity: None,
        item_state: "pending".to_owned(),
        failure_reason: None,
        provenance: None,
        approved_mtime: None,
        approved_size_bytes: None,
        archive_path: None,
        created_at: "2026-06-17T00:00:00Z".to_owned(),
        source_id: None,
        category: None,
        requires_destructive_confirm: Some(1),
        resolved_pattern: None,
        destructive_confirmed: 1, // user confirmed
    };

    let root_map: HashMap<String, Utf8PathBuf> = HashMap::new();
    let item = item_row_to_executor_item(&row, &root_map, "archive", None);
    assert!(item.destructive_confirmed, "destructive_confirmed=1 in DB must be read as true");
    assert!(item.requires_destructive_confirm, "delete action must require destructive confirm");
}

// ── FR-017: panic-safe registry removal (US12) ──────────────────────────────

/// Build an [`ActiveRun`] with no control wiring of consequence — the guard
/// test only cares about presence/absence of the entry by key.
fn dummy_active_run() -> ActiveRun {
    ActiveRun {
        cancel_token: CancellationToken::new(),
        skip_set: SkipSet::new(),
        retry_queue: RetryQueue::new(),
        run_id: "run-guard-test".to_owned(),
        path_set: PlanPathSet::new(),
    }
}

/// FR-017: on a *normal* scope exit the guard's `Drop` removes the entry
/// exactly once. This is the Completed / Cancelled / Paused path.
#[test]
fn active_run_guard_removes_entry_on_normal_drop() {
    let registry: Arc<DashMap<String, ActiveRun>> = Arc::new(DashMap::new());
    let plan_id = "plan-guard-normal";
    registry.insert(plan_id.to_owned(), dummy_active_run());
    assert!(registry.contains_key(plan_id), "entry present after insert");

    {
        let _guard = ActiveRunGuard { registry: registry.clone(), plan_id: plan_id.to_owned() };
        // entry still present while the guard is held
        assert!(registry.contains_key(plan_id), "entry present while guard held");
    } // guard drops here

    assert!(
        !registry.contains_key(plan_id),
        "guard Drop must remove the entry on normal scope exit"
    );
}

/// FR-017 acceptance scenario 2: a plan run that panics mid-apply must still
/// have its registry entry removed. The guard is owned by the same scope
/// that runs `execute_plan`; a panic there unwinds that scope, running the
/// guard's `Drop`. We model that scope with `catch_unwind` around a panic
/// that occurs *after* the guard is constructed and the entry inserted —
/// exactly the shape of `tokio::spawn(async move { let _g = guard; execute_plan().await })`
/// when `execute_plan` panics.
#[test]
fn active_run_guard_removes_entry_when_scope_panics() {
    let registry: Arc<DashMap<String, ActiveRun>> = Arc::new(DashMap::new());
    let plan_id = "plan-guard-panic";
    registry.insert(plan_id.to_owned(), dummy_active_run());
    assert!(registry.contains_key(plan_id), "entry present after insert");

    let registry_for_scope = registry.clone();
    let plan_id_owned = plan_id.to_owned();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        // Guard is owned by this scope, mirroring the spawned task.
        let _guard = ActiveRunGuard { registry: registry_for_scope, plan_id: plan_id_owned };
        // Stand-in for `execute_plan(...).await` panicking mid-apply.
        panic!("execute_plan panicked mid-apply");
    }));

    assert!(result.is_err(), "the scope must have panicked");
    assert!(
        !registry.contains_key(plan_id),
        "FR-017: guard Drop must remove the registry entry even when the scope unwinds from a panic"
    );
}

// ── FR-017: cross-plan path-set overlap guard (R-Concur-1) ──────────────────

/// Build a fake active run claiming the given path prefixes.
fn fake_active_run(run_id: &str, prefixes: &[&str]) -> ActiveRun {
    ActiveRun {
        cancel_token: CancellationToken::new(),
        skip_set: SkipSet::new(),
        retry_queue: RetryQueue::new(),
        run_id: run_id.to_owned(),
        path_set: prefixes.iter().map(Utf8PathBuf::from).collect(),
    }
}

/// FR-017: a pending apply whose (source ∪ destination) path set overlaps
/// an active run's path set is rejected with `plan.conflict.overlap`,
/// the state CAS never runs (plan stays `approved`), and no registry
/// entry is leaked for the rejected plan.
#[tokio::test]
async fn apply_plan_rejects_overlapping_active_plan() {
    let (db, bus) = setup().await;
    // Items claim "p-ovl-b/raw/file-0.fits" + "p-ovl-b/archive/file-0.fits"
    // (unrooted).
    insert_approved_plan_with_items(&db, "p-ovl-b", 1).await;

    // Another plan's active run claims the "p-ovl-b/raw" subtree — an
    // ancestor of this plan's source path at subtree-prefix granularity.
    let registry = active_runs();
    registry.insert("p-ovl-a".to_owned(), fake_active_run("run-ovl-a", &["p-ovl-b/raw"]));

    let result = apply_plan(db.pool(), &bus, "p-ovl-b", "test-token", None).await;
    registry.remove("p-ovl-a");

    let err = result.unwrap_err();
    assert_eq!(err.code, ErrorCode::PlanConflictOverlap);
    assert!(!registry.contains_key("p-ovl-b"), "rejected plan must not leak a registry entry");

    // The CAS never ran: the plan is untouched and can be applied later.
    let plan = repo::get_plan(db.pool(), "p-ovl-b", false).await.unwrap();
    assert_eq!(plan.state, "approved");
}

/// FR-017: disjoint path sets may apply concurrently — the guard only
/// rejects overlap, not concurrency itself.
#[tokio::test]
async fn apply_plan_allows_disjoint_active_plan() {
    let (db, bus) = setup().await;
    insert_approved_plan_with_items(&db, "p-dis-b", 1).await;

    let registry = active_runs();
    registry.insert("p-dis-a".to_owned(), fake_active_run("run-dis-a", &["/somewhere/else"]));

    let result = apply_plan(db.pool(), &bus, "p-dis-b", "test-token", None).await;
    registry.remove("p-dis-a");

    let resp = result.unwrap();
    assert_eq!(resp.new_state, "applying");

    // Let the background executor finish so the run's own registry entry
    // is dropped before other tests run.
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
}

/// FR-017: the claimed path set resolves item paths against the root map
/// the same way the executor does, and claims absolute archive paths
/// verbatim.
#[test]
fn compute_plan_path_set_resolves_roots_and_archive() {
    let row = plans_repo::PlanItemRow {
        id: "item-ps".to_owned(),
        plan_id: "plan-ps".to_owned(),
        item_index: 1,
        name: "file.fits".to_owned(),
        action: "archive".to_owned(),
        from_root_id: Some("root-001".to_owned()),
        from_relative_path: "raw/./file.fits".to_owned(),
        to_root_id: None,
        to_relative_path: "sorted/file.fits".to_owned(),
        reason: "test".to_owned(),
        protection: "normal".to_owned(),
        linked_entity: None,
        item_state: "pending".to_owned(),
        failure_reason: None,
        provenance: None,
        approved_mtime: None,
        approved_size_bytes: None,
        archive_path: Some("/vault/archive/file.fits".to_owned()),
        created_at: "2026-06-17T00:00:00Z".to_owned(),
        source_id: None,
        category: None,
        requires_destructive_confirm: Some(0),
        resolved_pattern: None,
        destructive_confirmed: 0,
    };

    let mut root_map = HashMap::new();
    root_map.insert("root-001".to_owned(), Utf8PathBuf::from("/mnt/library"));

    let set = compute_plan_path_set(std::slice::from_ref(&row), &root_map);
    assert_eq!(set.len(), 3);

    // Source: rooted + lexically normalized. Destination: falls back to
    // the source root (over-claiming, the safe direction). Archive:
    // absolute, claimed verbatim.
    let source: PlanPathSet =
        [Utf8PathBuf::from("/mnt/library/raw/file.fits")].into_iter().collect();
    let dest: PlanPathSet =
        [Utf8PathBuf::from("/mnt/library/sorted/file.fits")].into_iter().collect();
    let archive: PlanPathSet = [Utf8PathBuf::from("/vault/archive")].into_iter().collect();
    assert!(set.overlaps(&source), "source path must be claimed under its root");
    assert!(set.overlaps(&dest), "destination must fall back to the source root");
    assert!(set.overlaps(&archive), "absolute archive path must be claimed verbatim");

    let disjoint: PlanPathSet = [Utf8PathBuf::from("/elsewhere")].into_iter().collect();
    assert!(!set.overlaps(&disjoint));
}

/// Group-commit correctness: applying N items via the buffered
/// `PlanApplyCallbacks` produces exactly N `plan_apply_events` rows + 1
/// plan-level started row, and the `plans` counter ends at `items_applied = N`.
///
/// This also functions as a commit-reduction acceptance check: the design
/// replaces ~5N individual autocommit writes with ceil(N/100) flush txs +
/// plan-level bookends. For N=200 the former path issues ~1000 commits; the
/// buffered path issues at most ceil(200/100)+2 = 4 commits.
///
/// Commit count is not directly measurable in a unit test (SQLite
/// statement-count APIs are not exposed by sqlx), but row-count + counter
/// correctness proves the flush logic is correct without relying on timing.
#[tokio::test]
async fn group_commit_produces_correct_row_count_and_counters() {
    const N: usize = 200;

    let (db, bus) = setup().await;
    let plan_id = "gc-correctness";
    let run_id = "gc-run-1";
    insert_approved_plan_with_items(&db, plan_id, N).await;
    let n = i64::try_from(N).unwrap();
    apply_repo::cas_approved_to_applying(db.pool(), plan_id, run_id, "test-token", n, n)
        .await
        .unwrap();

    let callbacks = PlanApplyCallbacks::new(
        db.pool().clone(),
        bus,
        plan_id.to_owned(),
        run_id.to_owned(),
        None,
    );

    // Emit N succeeded events — mix in a few failures to test delta accounting.
    let mut expected_applied = 0i64;
    let mut expected_failed = 0i64;
    for i in 0..N {
        let item_id = format!("{plan_id}-item-{i}");
        // Fail every 10th item so we exercise both counter paths.
        let (new_state, failure) = if i % 10 == 9 {
            expected_failed += 1;
            ("failed", Some(PlanItemFailure::with_code(FailureCode::SourceMissing, "test")))
        } else {
            expected_applied += 1;
            ("succeeded", None)
        };

        callbacks
            .on_item_progress(ItemProgressEvent {
                item_id,
                prior_state: "pending".to_owned(),
                new_state: new_state.to_owned(),
                at: domain_core::ids::Timestamp::now_iso(),
                failure,
                rollback_attempted: false,
                rollback_outcome: RollbackOutcome::NotApplicable,
                rollback_message: None,
                audit_reason: None,
            })
            .await;
    }

    // Final mandatory flush (mirrors what spawn_executor_run does before
    // complete_run).
    callbacks.flush().await;

    // Verify plan_apply_events has exactly N item rows.
    let event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM plan_apply_events WHERE plan_id = ? AND item_id IS NOT NULL",
    )
    .bind(plan_id)
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(event_count, n, "must have one plan_apply_events row per item");

    // Verify plans counters.
    let plan =
        persistence_plans::repositories::plans::get_plan(db.pool(), plan_id, false).await.unwrap();
    assert_eq!(plan.items_applied, expected_applied, "items_applied counter");
    assert_eq!(plan.items_failed, expected_failed, "items_failed counter");

    // Verify audit_log_entry has N rows for this plan's items (one per item,
    // written in the flush batch; entity_type = 'filesystem_plan').
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log_entry WHERE entity_type = 'filesystem_plan'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert!(
        audit_count >= expected_applied + expected_failed,
        "must have at least one audit row per item; got {audit_count}, want >= {}",
        expected_applied + expected_failed
    );
}

/// Flush-on-timeout: items accumulated for longer than 250 ms trigger a flush
/// at the next item boundary even when the 100-item count threshold is not met.
#[tokio::test]
async fn group_commit_flush_on_timeout() {
    let (db, bus) = setup().await;
    let plan_id = "gc-timeout";
    let run_id = "gc-timeout-run";
    insert_approved_plan_with_items(&db, plan_id, 5).await;
    apply_repo::cas_approved_to_applying(db.pool(), plan_id, run_id, "test-token", 5, 5)
        .await
        .unwrap();

    let callbacks = PlanApplyCallbacks::new(
        db.pool().clone(),
        bus,
        plan_id.to_owned(),
        run_id.to_owned(),
        None,
    );

    // Emit one item — well below the 100-item threshold.
    callbacks
        .on_item_progress(ItemProgressEvent {
            item_id: format!("{plan_id}-item-0"),
            prior_state: "pending".to_owned(),
            new_state: "succeeded".to_owned(),
            at: domain_core::ids::Timestamp::now_iso(),
            failure: None,
            rollback_attempted: false,
            rollback_outcome: RollbackOutcome::NotApplicable,
            rollback_message: None,
            audit_reason: None,
        })
        .await;

    // Before the 250 ms window expires the row must not be in the DB yet.
    let count_before: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM plan_apply_events WHERE plan_id = ?")
            .bind(plan_id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(count_before, 0, "row must be buffered, not yet flushed");

    // Advance past the 250 ms window.
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

    // A second item triggers the boundary check — the 250 ms elapsed flag
    // causes the buffer to flush (the second item is then also flushed).
    callbacks
        .on_item_progress(ItemProgressEvent {
            item_id: format!("{plan_id}-item-1"),
            prior_state: "pending".to_owned(),
            new_state: "succeeded".to_owned(),
            at: domain_core::ids::Timestamp::now_iso(),
            failure: None,
            rollback_attempted: false,
            rollback_outcome: RollbackOutcome::NotApplicable,
            rollback_message: None,
            audit_reason: None,
        })
        .await;

    let count_after: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM plan_apply_events WHERE plan_id = ?")
            .bind(plan_id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(count_after, 2, "both items must be flushed after the 250 ms window expires");
}

/// Crash-window recovery: items from a lost flush window (simulated by
/// executing the fs op but not flushing) re-execute on resume and land as
/// `source.missing` terminal failures (CAS gate refuses them), never
/// double-applying the mutation.
///
/// This validates the crash-safety claim in the batching design: fs ops for
/// items in the lost window already ran; re-execution hits the
/// `check_cas`/destination-exists gates and produces a reviewable failure
/// rather than a silent duplicate mutation.
#[tokio::test]
async fn crash_window_recovery_produces_source_missing_not_double_apply() {
    use std::fs;
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("light.fits");
    let dst = tmp.path().join("archive").join("light.fits");
    fs::write(&src, "fits-data").unwrap();

    let (db, bus) = setup().await;
    // Register a root so item_row_to_executor_item resolves the path.
    let root_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO registered_sources \
         (id, kind, path, scan_depth, created_at, created_via, organization_state) \
         VALUES (?, 'light_frames', ?, 'recursive', '2026-01-01T00:00:00Z', 'first_run', 'organized')",
    )
    .bind(&root_id)
    .bind(tmp.path().to_str().unwrap())
    .execute(db.pool())
    .await
    .unwrap();

    // Insert a plan item that moves src → archive/light.fits.
    let plan_id = "crash-recovery";
    let run_id = "crash-recovery-run";
    repo::insert_plan(
        db.pool(),
        &repo::InsertPlan {
            id: plan_id,
            title: "Crash recovery test",
            origin: "cleanup",
            origin_path: None,
            plan_type: "cleanup",
            destructive_destination: "archive",
            parent_plan_id: None,
            total_bytes_required: 0,
        },
    )
    .await
    .unwrap();
    repo::insert_plan_item(
        db.pool(),
        &repo::InsertPlanItem {
            id: &format!("{plan_id}-item-0"),
            plan_id,
            item_index: 1,
            name: "light.fits",
            action: "move",
            from_root_id: Some(&root_id),
            from_relative_path: "light.fits",
            to_root_id: Some(&root_id),
            to_relative_path: "archive/light.fits",
            reason: "test",
            protection: "normal",
            linked_entity: None,
            provenance_json: None,
            archive_path: None,
            source_id: None,
            category: None,
        },
    )
    .await
    .unwrap();
    repo::update_plan_state(db.pool(), plan_id, "ready_for_review").await.unwrap();
    repo::set_approved(db.pool(), plan_id, "2026-06-01T00:00:00Z", "test-token").await.unwrap();

    // Simulate the crash-window scenario: the fs op ran (rename succeeded)
    // but the flush tx never committed — move the file manually to represent
    // a mutation that happened in a lost flush window.
    fs::create_dir_all(dst.parent().unwrap()).unwrap();
    fs::rename(&src, &dst).unwrap();
    assert!(!src.exists(), "source must be gone after simulated move");
    assert!(dst.exists(), "destination must exist after simulated move");

    // Apply the plan — the executor runs the move on a missing source. The
    // check_cas gate returns SourceMissing → terminal failed, no double-apply.
    let resp = apply_plan(db.pool(), &bus, plan_id, "test-token", None).await.unwrap();
    let _ = run_id; // not used after removing the manual CAS
    assert_eq!(resp.new_state, "applying");

    // Wait for the background executor to complete.
    for _ in 0..50 {
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        let state: String = sqlx::query_scalar("SELECT state FROM plans WHERE id = ?")
            .bind(plan_id)
            .fetch_one(db.pool())
            .await
            .unwrap();
        if state != "applying" {
            break;
        }
    }

    // The plan must be terminal (failed or partially_applied, not applying).
    let final_state: String = sqlx::query_scalar("SELECT state FROM plans WHERE id = ?")
        .bind(plan_id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert!(
        matches!(final_state.as_str(), "failed" | "partially_applied"),
        "plan must be terminal after re-apply of already-moved item; got {final_state}"
    );

    // The item must be in a failed state (not succeeded — it didn't move again).
    let item_state: String = sqlx::query_scalar("SELECT item_state FROM plan_items WHERE id = ?")
        .bind(format!("{plan_id}-item-0"))
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(
        item_state, "failed",
        "re-applied item must be failed (source.missing), not succeeded"
    );

    // The destination must not have been touched a second time.
    assert!(dst.exists(), "destination file must still exist — no double-apply");
}

// ── GF-5: cancel_plan on paused plan transitions DB directly ─────────────────

/// GF-5 regression: cancel_plan on a plan in state "paused" with NO live
/// ActiveRun must transition the DB to "cancelled" directly, not silently
/// no-op. The old code only signalled the cancel token (which has no receiver
/// when the executor's ActiveRunGuard has already dropped).
#[tokio::test]
async fn gf5_cancel_paused_plan_transitions_db_directly() {
    let (db, bus) = setup().await;
    let plan_id = "p-gf5-cancel-paused";
    insert_approved_plan_with_items(&db, plan_id, 3).await;

    // Simulate a plan that reached "paused": CAS to applying (creates run
    // row), then pause_run (sets terminal_state=paused on that run).
    apply_repo::cas_approved_to_applying(db.pool(), plan_id, "run-gf5", "test-token", 3, 3)
        .await
        .unwrap();
    apply_repo::pause_run(db.pool(), plan_id, "run-gf5", "item.stale", 0, 0, 0, 0, 3)
        .await
        .unwrap();

    // Crucially, do NOT register an ActiveRun in the process-global registry.
    // This mirrors the real state: executor's ActiveRunGuard dropped on pause.

    let response = cancel_plan(db.pool(), &bus, plan_id).await.unwrap();
    assert_eq!(response.plan_id, plan_id);
    assert_eq!(response.items_cancelled, 3);

    // The plan must now be in state "cancelled" in the DB.
    let row = repo::get_plan(db.pool(), plan_id, false).await.unwrap();
    assert_eq!(row.state, "cancelled", "paused plan must transition to cancelled in DB");
}

// ── GF-29: skip_plan_item rejects when no active run ─────────────────────────

/// GF-29 regression: skip_plan_item on a plan that is "applying" in DB but
/// has NO live ActiveRun registered must return run.not_found, not fabricate
/// a success response with new_state=skipped.
#[tokio::test]
async fn gf29_skip_plan_item_rejects_when_no_active_run() {
    let (db, _bus) = setup().await;
    let plan_id = "p-gf29-skip-no-run";
    insert_approved_plan_with_items(&db, plan_id, 1).await;

    // Set plan to "applying" in DB without registering an ActiveRun.
    sqlx::query("UPDATE plans SET state = 'applying' WHERE id = ?")
        .bind(plan_id)
        .execute(db.pool())
        .await
        .unwrap();

    let err = skip_plan_item(db.pool(), plan_id, &format!("{plan_id}-item-0")).await.unwrap_err();
    assert_eq!(
        err.code,
        ErrorCode::RunNotFound,
        "skip with no ActiveRun must return run.not_found, not fabricate success"
    );
}

// ── GFD-2 regression: orphaned-applying sweep on Completed path ───────────────

/// GFD-2 regression (PR #1527 symmetric): handle_completed must sweep items
/// stuck in `applying` — items whose retry DB flip landed but whose
/// re-execution never started before run completion. Without the sweep, those
/// items are permanently stuck in `applying` with no terminal audit record.
///
/// Mirrors the Cancelled-path sweep; both now call
/// `cancel_orphaned_applying_items` before `cumulative_counts` so swept items
/// are included in the terminal counters.
#[tokio::test]
async fn gfd2_completed_path_sweeps_orphaned_applying_items() {
    let (db, bus) = setup().await;
    let plan_id = "p-gfd2-completed";
    let run_id = "run-gfd2-completed";

    insert_approved_plan_with_items(&db, plan_id, 2).await;

    // Drive plan to applying + create a run (the token matches insert_approved helper).
    apply_repo::cas_approved_to_applying(db.pool(), plan_id, run_id, "test-token", 2, 2)
        .await
        .unwrap();

    // item-0 succeeded, item-1 stuck in `applying` (the GFD-2 race window:
    // retry flip landed but executor never picked it up before completion).
    sqlx::query("UPDATE plan_items SET item_state = 'succeeded' WHERE id = ?")
        .bind(format!("{plan_id}-item-0"))
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE plan_items SET item_state = 'applying' WHERE id = ?")
        .bind(format!("{plan_id}-item-1"))
        .execute(db.pool())
        .await
        .unwrap();

    // Call handle_completed — it should sweep item-1 and emit a durable audit row.
    super::terminal::handle_completed(
        db.pool(),
        &bus,
        plan_id,
        run_id,
        "cleanup",
        None,
        None,
        TerminalCounts { succeeded: 1, failed: 0, skipped: 0, cancelled: 0 },
    )
    .await;

    // item-1 must now be `cancelled` (swept by GFD-2 path).
    let item_state: String = sqlx::query_scalar("SELECT item_state FROM plan_items WHERE id = ?")
        .bind(format!("{plan_id}-item-1"))
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(
        item_state, "cancelled",
        "orphaned applying item must be swept to cancelled on Completed path"
    );

    // A durable audit row for the sweep must exist.
    let entries = list_audit_entries(
        db.pool(),
        &AuditLogFilter { entity_type: Some("filesystem_plan".to_owned()), ..Default::default() },
    )
    .await
    .unwrap();
    assert!(
        entries.iter().any(|e| e.trigger == "plan_item.cancelled"),
        "GFD-2 sweep on Completed path must emit a durable audit row for the swept item"
    );
}

/// Boot reconciliation classifies each crashed-plan item against filesystem
/// reality: a completed move heals to `succeeded`, an untouched move is left
/// for resume, and an ambiguous (both endpoints present) move is flagged for
/// user review. Exercises the real `resolve_root_path` + fs-probe path.
#[tokio::test]
async fn reconcile_crashed_plans_classifies_all_three_verdicts() {
    use contracts_core::first_run::{
        OrganizationState, RegisterSourceRequest, ScanDepth, SourceKind,
    };

    async fn item_state(db: &Database, id: &str) -> String {
        sqlx::query_scalar::<_, String>("SELECT item_state FROM plan_items WHERE id = ?")
            .bind(id)
            .fetch_one(db.pool())
            .await
            .unwrap()
    }

    let (db, bus) = setup().await;
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path().to_str().unwrap().to_owned();

    let reg = crate::first_run::register_source(
        db.pool(),
        &bus,
        &RegisterSourceRequest {
            kind: SourceKind::Project,
            path: root_path.clone(),
            kind_subtype: None,
            scan_depth: ScanDepth::Recursive,
            organization_state: OrganizationState::Organized,
        },
    )
    .await
    .unwrap();
    let root_id = reg.source_id;

    repo::insert_plan(
        db.pool(),
        &repo::InsertPlan {
            id: "prc",
            title: "Recover",
            origin: "cleanup",
            origin_path: None,
            plan_type: "cleanup",
            destructive_destination: "archive",
            parent_plan_id: None,
            total_bytes_required: 0,
        },
    )
    .await
    .unwrap();

    // Three move items rooted at the tempdir. src/dst are root-relative.
    let cases = [
        ("done", "raw/done.fits", "out/done.fits"),
        ("todo", "raw/todo.fits", "out/todo.fits"),
        ("ambig", "raw/ambig.fits", "out/ambig.fits"),
    ];
    for (i, (name, from, to)) in cases.iter().enumerate() {
        repo::insert_plan_item(
            db.pool(),
            &repo::InsertPlanItem {
                id: &format!("prc-{name}"),
                plan_id: "prc",
                item_index: i64::try_from(i + 1).unwrap(),
                name: "f.fits",
                action: "move",
                from_root_id: Some(&root_id),
                from_relative_path: from,
                to_root_id: Some(&root_id),
                to_relative_path: to,
                reason: "test",
                protection: "normal",
                linked_entity: None,
                provenance_json: None,
                archive_path: None,
                source_id: None,
                category: None,
            },
        )
        .await
        .unwrap();
    }
    repo::update_plan_state(db.pool(), "prc", "ready_for_review").await.unwrap();
    repo::set_approved(db.pool(), "prc", "2026-06-01T00:00:00Z", "tok").await.unwrap();

    // Drive to applying + simulate crash sweep.
    let run_id = new_id();
    apply_repo::cas_approved_to_applying(db.pool(), "prc", &run_id, "tok", 3, 3).await.unwrap();
    let swept = sweep_crashed_applying_plans(db.pool()).await.unwrap();
    assert_eq!(swept, vec!["prc".to_owned()]);

    // Materialize filesystem states matching each verdict.
    let mk = |rel: &str| {
        let p = root.path().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, b"x").unwrap();
    };
    // done: source gone, destination present -> Completed
    mk("out/done.fits");
    // todo: source present, destination absent -> NotStarted
    mk("raw/todo.fits");
    // ambig: both present -> Ambiguous
    mk("raw/ambig.fits");
    mk("out/ambig.fits");

    let report = reconcile_crashed_plans(db.pool(), &swept).await.unwrap();
    assert_eq!(report.healed, 1, "the completed move heals to succeeded");
    assert_eq!(report.left_for_resume, 1, "the untouched move is left for resume");
    assert_eq!(report.ambiguous_plan_ids, vec!["prc".to_owned()]);
    assert!(report.needs_user_review());

    assert_eq!(item_state(&db, "prc-done").await, "succeeded");
    assert_eq!(
        item_state(&db, "prc-todo").await,
        "pending",
        "not-started item stays pending for resume"
    );
    assert_eq!(
        item_state(&db, "prc-ambig").await,
        "failed",
        "ambiguous item is flagged failed for review"
    );
}

// ── G-RUNSTATE-TRUTH: a reported lifecycle state is a written one ────────────

/// `pause_run`'s last argument is `items_pending`. It received the settled sum
/// (succeeded + failed + skipped + cancelled), so a pause recorded the count of
/// items that had finished as the count still to run.
#[tokio::test]
async fn a_pause_records_the_items_still_to_run_not_the_settled_ones() {
    let (db, bus) = setup().await;
    insert_approved_plan_with_items(&db, "p-pause-count", 3).await;
    apply_repo::cas_approved_to_applying(
        db.pool(),
        "p-pause-count",
        "run-pause-count",
        "test-token",
        3,
        3,
    )
    .await
    .unwrap();
    {
        let mut conn = db.pool().acquire().await.unwrap();
        apply_repo::batch_flush_item_states(
            &mut conn,
            "p-pause-count",
            &[apply_repo::BatchItemState {
                item_id: "p-pause-count-item-0",
                new_state: "succeeded",
                failure_reason: None,
                is_stale: false,
            }],
            1,
            0,
            0,
        )
        .await
        .unwrap();
    }

    terminal::handle_paused(
        db.pool(),
        &bus,
        "p-pause-count",
        "run-pause-count",
        "item.stale",
        None,
        TerminalCounts { succeeded: 1, ..TerminalCounts::default() },
    )
    .await;

    let pending: i64 = sqlx::query_scalar("SELECT items_pending FROM plan_apply_runs WHERE id = ?")
        .bind("run-pause-count")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(pending, 2, "1 of 3 items settled, so 2 remain pending");

    let state: String = sqlx::query_scalar("SELECT state FROM plans WHERE id = ?")
        .bind("p-pause-count")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(state, "paused");
}

/// A pause whose write fails leaves the plan `applying` for boot recovery, so
/// the long-op stream carries the failure rather than a pause the row denies.
#[tokio::test]
async fn a_pause_that_cannot_be_written_is_not_reported_as_paused() {
    use std::sync::Mutex;

    let (db, bus) = setup().await;
    insert_approved_plan_with_items(&db, "p-pause-fail", 1).await;
    apply_repo::cas_approved_to_applying(
        db.pool(),
        "p-pause-fail",
        "run-pause-fail",
        "test-token",
        1,
        1,
    )
    .await
    .unwrap();
    sqlx::query("DROP TABLE plan_apply_runs").execute(db.pool()).await.unwrap();

    let captured: Arc<Mutex<Vec<OperationEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink_store = captured.clone();
    let sink: OperationEventSink = Arc::new(move |event: OperationEvent| {
        sink_store.lock().unwrap().push(event);
    });
    let emitter = OpEventEmitter::new(OperationId("run-pause-fail".to_owned()), sink);

    terminal::handle_paused(
        db.pool(),
        &bus,
        "p-pause-fail",
        "run-pause-fail",
        "disk.full",
        Some(&emitter),
        TerminalCounts::default(),
    )
    .await;

    let events = captured.lock().unwrap().clone();
    assert_eq!(events.len(), 1, "exactly one long-op event for a failed pause");
    assert_eq!(events[0].event_type, OperationEventType::Failed);

    let state: String = sqlx::query_scalar("SELECT state FROM plans WHERE id = ?")
        .bind("p-pause-fail")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(state, "applying", "the plan is left for the boot sweep to classify");
}

/// Cancelling a paused plan whose item write fails is refused. The plan stays
/// `paused` with its items `pending` instead of reporting a cancellation the
/// items contradict.
#[tokio::test]
async fn a_paused_cancel_whose_item_write_fails_is_refused() {
    let (db, bus) = setup().await;
    insert_approved_plan_with_items(&db, "p-cancel-refuse", 2).await;
    apply_repo::cas_approved_to_applying(
        db.pool(),
        "p-cancel-refuse",
        "run-cancel-refuse",
        "test-token",
        2,
        2,
    )
    .await
    .unwrap();
    apply_repo::pause_run(
        db.pool(),
        "p-cancel-refuse",
        "run-cancel-refuse",
        "disk.full",
        0,
        0,
        0,
        0,
        2,
    )
    .await
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER block_item_cancel BEFORE UPDATE OF item_state ON plan_items \
         WHEN NEW.item_state = 'cancelled' BEGIN SELECT RAISE(ABORT, 'write refused'); END",
    )
    .execute(db.pool())
    .await
    .unwrap();

    let err = cancel_plan(db.pool(), &bus, "p-cancel-refuse").await.unwrap_err();
    assert_eq!(err.code, ErrorCode::InternalDatabase);

    let state: String = sqlx::query_scalar("SELECT state FROM plans WHERE id = ?")
        .bind("p-cancel-refuse")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(state, "paused", "a refused cancel leaves the plan cancellable again");

    let pending = apply_repo::list_pending_items(db.pool(), "p-cancel-refuse").await.unwrap();
    assert_eq!(pending.len(), 2, "both items are still pending");
}

/// The user's skip is committed before the item stops being eligible to run, so
/// a pause or crash cannot put a deliberately skipped item back on the forward
/// pass (`resume_plan` re-reads `pending`/`failed` rows only).
#[tokio::test]
async fn a_skip_is_persisted_before_the_item_is_bypassed() {
    let (db, _bus) = setup().await;
    insert_approved_plan_with_items(&db, "p-skip-persist", 2).await;
    plans_repo::update_plan_state(db.pool(), "p-skip-persist", "applying").await.unwrap();
    register_fake_active_run("p-skip-persist");

    let resp = skip_plan_item(db.pool(), "p-skip-persist", "p-skip-persist-item-0").await.unwrap();
    assert_eq!(resp.new_state, "skipped");

    let state: String = sqlx::query_scalar("SELECT item_state FROM plan_items WHERE id = ?")
        .bind("p-skip-persist-item-0")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(state, "skipped", "the reported state is the stored state");

    let pending = apply_repo::list_pending_items(db.pool(), "p-skip-persist").await.unwrap();
    assert_eq!(pending, vec!["p-skip-persist-item-1".to_owned()]);

    active_runs().remove("p-skip-persist");
}

/// A skip whose write fails is refused with the item left `pending`, rather
/// than answered `skipped` from an in-memory set the next resume discards.
#[tokio::test]
async fn a_skip_that_cannot_be_written_is_refused() {
    let (db, _bus) = setup().await;
    insert_approved_plan_with_items(&db, "p-skip-refuse", 1).await;
    plans_repo::update_plan_state(db.pool(), "p-skip-refuse", "applying").await.unwrap();
    register_fake_active_run("p-skip-refuse");
    sqlx::query(
        "CREATE TRIGGER block_item_skip BEFORE UPDATE OF item_state ON plan_items \
         WHEN NEW.item_state = 'skipped' BEGIN SELECT RAISE(ABORT, 'write refused'); END",
    )
    .execute(db.pool())
    .await
    .unwrap();

    let err = skip_plan_item(db.pool(), "p-skip-refuse", "p-skip-refuse-item-0").await.unwrap_err();
    assert_eq!(err.code, ErrorCode::InternalDatabase);

    let state: String = sqlx::query_scalar("SELECT item_state FROM plan_items WHERE id = ?")
        .bind("p-skip-refuse-item-0")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(state, "pending", "a refused skip leaves the item eligible to run");

    active_runs().remove("p-skip-refuse");
}

/// A cancelled run whose terminal write fails is not announced as cancelled:
/// finding .5.17 on the live-executor branch, where `handle_cancelled` performs
/// the write (review FIX 1).
#[tokio::test]
async fn a_cancelled_run_whose_terminal_write_fails_is_not_reported_as_cancelled() {
    use std::sync::Mutex;

    let (db, bus) = setup().await;
    insert_approved_plan_with_items(&db, "p-cancel-write", 1).await;
    apply_repo::cas_approved_to_applying(
        db.pool(),
        "p-cancel-write",
        "run-cancel-write",
        "test-token",
        1,
        1,
    )
    .await
    .unwrap();
    sqlx::query("DROP TABLE plan_apply_runs").execute(db.pool()).await.unwrap();

    let captured: Arc<Mutex<Vec<OperationEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink_store = captured.clone();
    let sink: OperationEventSink = Arc::new(move |event: OperationEvent| {
        sink_store.lock().unwrap().push(event);
    });
    let emitter = OpEventEmitter::new(OperationId("run-cancel-write".to_owned()), sink);

    terminal::handle_cancelled(
        db.pool(),
        &bus,
        "p-cancel-write",
        "run-cancel-write",
        Some(&emitter),
        TerminalCounts::default(),
    )
    .await;

    let events = captured.lock().unwrap().clone();
    assert_eq!(events.len(), 1, "exactly one long-op event for a failed terminal write");
    assert_eq!(events[0].event_type, OperationEventType::Failed);

    let state: String = sqlx::query_scalar("SELECT state FROM plans WHERE id = ?")
        .bind("p-cancel-write")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(state, "applying", "the plan is left for the boot sweep to classify");
}

/// The same gate on the completed path: a terminal state the plan row denies is
/// reported as a failure, not as the terminal the run computed.
#[tokio::test]
async fn a_completed_run_whose_terminal_write_fails_is_not_reported_as_terminal() {
    use std::sync::Mutex;

    let (db, bus) = setup().await;
    insert_approved_plan_with_items(&db, "p-complete-write", 1).await;
    apply_repo::cas_approved_to_applying(
        db.pool(),
        "p-complete-write",
        "run-complete-write",
        "test-token",
        1,
        1,
    )
    .await
    .unwrap();
    // The item is flushed succeeded so the computed terminal is `applied`. An
    // all-zero counter set computes `failed`, which the emitter reports as a
    // Failed event of its own accord.
    {
        let mut conn = db.pool().acquire().await.unwrap();
        apply_repo::batch_flush_item_states(
            &mut conn,
            "p-complete-write",
            &[apply_repo::BatchItemState {
                item_id: "p-complete-write-item-0",
                new_state: "succeeded",
                failure_reason: None,
                is_stale: false,
            }],
            1,
            0,
            0,
        )
        .await
        .unwrap();
    }
    sqlx::query("DROP TABLE plan_apply_runs").execute(db.pool()).await.unwrap();

    let captured: Arc<Mutex<Vec<OperationEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink_store = captured.clone();
    let sink: OperationEventSink = Arc::new(move |event: OperationEvent| {
        sink_store.lock().unwrap().push(event);
    });
    let emitter = OpEventEmitter::new(OperationId("run-complete-write".to_owned()), sink);

    terminal::handle_completed(
        db.pool(),
        &bus,
        "p-complete-write",
        "run-complete-write",
        "cleanup",
        None,
        Some(&emitter),
        TerminalCounts { succeeded: 1, ..TerminalCounts::default() },
    )
    .await;

    let events = captured.lock().unwrap().clone();
    assert_eq!(events.len(), 1, "exactly one long-op event for a failed terminal write");
    assert_eq!(events[0].event_type, OperationEventType::Failed);

    let state: String = sqlx::query_scalar("SELECT state FROM plans WHERE id = ?")
        .bind("p-complete-write")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(state, "applying");
}

/// The pause record counts the item rows still `pending`, so a skip the run
/// never reached cannot inflate it through the lagging plan counter
/// (astro-plan-ajy4v).
#[tokio::test]
async fn a_pause_after_a_skip_counts_the_rows_not_the_lagging_plan_counter() {
    let (db, bus) = setup().await;
    insert_approved_plan_with_items(&db, "p-pause-skip", 3).await;
    apply_repo::cas_approved_to_applying(
        db.pool(),
        "p-pause-skip",
        "run-pause-skip",
        "test-token",
        3,
        3,
    )
    .await
    .unwrap();
    register_fake_active_run("p-pause-skip");
    skip_plan_item(db.pool(), "p-pause-skip", "p-pause-skip-item-0").await.unwrap();

    let counter: i64 = sqlx::query_scalar("SELECT items_pending FROM plans WHERE id = ?")
        .bind("p-pause-skip")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(counter, 3, "the plan counter still carries the skipped item");

    terminal::handle_paused(
        db.pool(),
        &bus,
        "p-pause-skip",
        "run-pause-skip",
        "disk.full",
        None,
        TerminalCounts::default(),
    )
    .await;

    let pending: i64 = sqlx::query_scalar("SELECT items_pending FROM plan_apply_runs WHERE id = ?")
        .bind("run-pause-skip")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(pending, 2, "the skipped item is not part of the work still to run");

    active_runs().remove("p-pause-skip");
}

// ── astro-plan-krqge: prepared lifecycle closure ─────────────────────────────

async fn insert_ready_project(db: &Database, project_id: &str) {
    use persistence_plans::repositories::projects as projects_repo;

    projects_repo::insert_project(
        db.pool(),
        &projects_repo::InsertProject {
            id: project_id,
            name: "JV Archive Prepared",
            tool: "PixInsight",
            lifecycle: "ready",
            path: "projects/JV_Archive_Prepared",
            notes: None,
            canonical_target_id: None,
            is_mosaic: false,
        },
    )
    .await
    .unwrap();
}

/// A clean `prepared_view_generation` apply closes the requires-plan gate on
/// `ready → prepared`: `apply_transition` refuses that edge unconditionally, so
/// without this closure the project can never leave `ready` and stays unlinked.
#[tokio::test]
async fn completed_view_generation_apply_prepares_the_project() {
    use persistence_plans::repositories::projects as projects_repo;

    let (db, bus) = setup().await;
    let plan_id = "p-krqge-prepared";
    let run_id = "run-krqge-prepared";
    let project_id = Uuid::new_v4().to_string();
    insert_ready_project(&db, &project_id).await;
    insert_approved_plan_with_items(&db, plan_id, 1).await;
    apply_repo::cas_approved_to_applying(db.pool(), plan_id, run_id, "test-token", 1, 1)
        .await
        .unwrap();
    sqlx::query("UPDATE plan_items SET item_state = 'succeeded' WHERE plan_id = ?")
        .bind(plan_id)
        .execute(db.pool())
        .await
        .unwrap();
    // The terminal state is derived from the plan row's cumulative counters
    // (`cumulative_counts`), not from item states.
    sqlx::query("UPDATE plans SET items_applied = 1, items_failed = 0 WHERE id = ?")
        .bind(plan_id)
        .execute(db.pool())
        .await
        .unwrap();

    terminal::handle_completed(
        db.pool(),
        &bus,
        plan_id,
        run_id,
        "prepared_view_generation",
        Some(&project_id),
        None,
        TerminalCounts { succeeded: 1, failed: 0, skipped: 0, cancelled: 0 },
    )
    .await;

    let project = projects_repo::get_project(db.pool(), &project_id).await.unwrap();
    assert_eq!(
        project.lifecycle, "prepared",
        "applying the source-view plan must drive the project to prepared"
    );

    let entries = list_audit_entries(
        db.pool(),
        &AuditLogFilter { entity_id: Some(project_id.clone()), ..AuditLogFilter::default() },
    )
    .await
    .unwrap();
    assert!(
        entries.iter().any(|e| e.trigger == "sourceview.plan.applied"),
        "the lifecycle closure must leave a durable audit record: {entries:?}"
    );
}

/// A `partially_applied` terminal leaves the lifecycle alone — only a clean
/// apply materialises every planned link (mirrors the archive closure, which
/// runs on `applied` only).
#[tokio::test]
async fn partially_applied_view_generation_leaves_lifecycle_ready() {
    use persistence_plans::repositories::projects as projects_repo;

    let (db, bus) = setup().await;
    let plan_id = "p-krqge-partial";
    let run_id = "run-krqge-partial";
    let project_id = Uuid::new_v4().to_string();
    insert_ready_project(&db, &project_id).await;
    insert_approved_plan_with_items(&db, plan_id, 2).await;
    apply_repo::cas_approved_to_applying(db.pool(), plan_id, run_id, "test-token", 2, 2)
        .await
        .unwrap();
    sqlx::query("UPDATE plan_items SET item_state = 'succeeded' WHERE id = ?")
        .bind(format!("{plan_id}-item-0"))
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE plan_items SET item_state = 'failed' WHERE id = ?")
        .bind(format!("{plan_id}-item-1"))
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE plans SET items_applied = 1, items_failed = 1 WHERE id = ?")
        .bind(plan_id)
        .execute(db.pool())
        .await
        .unwrap();

    terminal::handle_completed(
        db.pool(),
        &bus,
        plan_id,
        run_id,
        "prepared_view_generation",
        Some(&project_id),
        None,
        TerminalCounts { succeeded: 1, failed: 1, skipped: 0, cancelled: 0 },
    )
    .await;

    let project = projects_repo::get_project(db.pool(), &project_id).await.unwrap();
    assert_eq!(
        project.lifecycle, "ready",
        "a partial apply must not claim the project is prepared"
    );
}
