// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Application use cases for project manifests (spec 024).
//!
//! ## Entry points
//!
//! - [`list`]  — list manifest summaries for a project (newest first, paginated).
//! - [`get`]   — fetch one manifest with its full structured body.
//! - [`write`] — generate and persist a manifest snapshot (called by lifecycle
//!   triggers and the `workflow.run_completed` subscriber).
//!
//! ## Architecture
//!
//! Manifest files live in `<project_root>/notes/` on disk and are indexed in
//! the `manifests` DB table.  The DB row is the durable record; the file on
//! disk is the reproducible projection (Constitution V).
//!
//! Audit events flow through `crates/audit/`.
//!
//! `write` is the single producer for manifest rows.  It:
//!   1. Emits `manifest.write.attempt` audit event.
//!   2. Serialises the body to JSON (stored in the DB row).
//!   3. Renders markdown and writes the file via `project_structure::manifest`.
//!   4. Inserts the DB row.
//!   5. Emits `manifest.write.success` or `manifest.write.failure`.
//!
//! Constitution II: write never overwrites an existing file.  Retry produces a
//! new filename with a later timestamp.

use audit::bus::EventBus;
use audit::event_bus::Source;
use sqlx::SqlitePool;

use contracts_core::manifests::{
    ManifestBodyDto, ManifestDto, ManifestGetResponse, ManifestListRequest, ManifestListResponse,
    ManifestOpError, ManifestReason as DtoManifestReason, ManifestSummaryDto,
};
use persistence_plans::repositories::manifests::{
    get_manifest, insert_manifest, list_manifests_for_project, InsertManifest,
};
use persistence_plans::repositories::project_notes::get_note_content;
use project_structure::manifest::{
    manifest_relative_path, now_utc_iso, render_manifest_markdown, write_manifest_file,
    ManifestBody, ManifestReason, MANIFEST_VERSION,
};

use std::path::Path;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn new_uuid() -> String {
    domain_core::ids::new_id()
}

fn manifest_op_error(code: &str, message: &str) -> ManifestOpError {
    ManifestOpError { code: code.to_owned(), message: message.to_owned(), details: None }
}

fn dto_reason_to_struct(r: DtoManifestReason) -> ManifestReason {
    match r {
        DtoManifestReason::Created => ManifestReason::Created,
        DtoManifestReason::SourceChange => ManifestReason::SourceChange,
        DtoManifestReason::LifecycleTransition => ManifestReason::LifecycleTransition,
        DtoManifestReason::CleanupApplied => ManifestReason::CleanupApplied,
        DtoManifestReason::WorkflowRun => ManifestReason::WorkflowRun,
    }
}

fn str_to_dto_reason(s: &str) -> DtoManifestReason {
    match s {
        "source_change" => DtoManifestReason::SourceChange,
        "lifecycle_transition" => DtoManifestReason::LifecycleTransition,
        "cleanup_applied" => DtoManifestReason::CleanupApplied,
        "workflow_run" => DtoManifestReason::WorkflowRun,
        _ => DtoManifestReason::Created,
    }
}

// ── list ──────────────────────────────────────────────────────────────────────

/// List manifest summaries for a project, newest first, with cursor pagination.
///
/// Default limit is 50; max is 200 (A6).
///
/// # Errors
/// Returns [`ManifestOpError`] with `"project.not_found"` (future) or
/// `"internal"` on database failure.
pub async fn list(
    pool: &SqlitePool,
    req: ManifestListRequest,
) -> Result<ManifestListResponse, ManifestOpError> {
    let limit = req.limit.unwrap_or(50).clamp(1, 200);
    let (rows, next_cursor) =
        list_manifests_for_project(pool, &req.project_id, req.cursor.as_deref(), limit)
            .await
            .map_err(|e| manifest_op_error("internal", &format!("DB error: {e}")))?;

    let manifests = rows
        .into_iter()
        .map(|r| ManifestSummaryDto {
            id: r.id,
            reason: str_to_dto_reason(&r.reason),
            timestamp: r.timestamp,
            path: r.path,
            has_body: true,
        })
        .collect();

    Ok(ManifestListResponse { manifests, next_cursor })
}

// ── get ───────────────────────────────────────────────────────────────────────

/// Fetch one manifest with its full structured body.
///
/// # Errors
/// Returns [`ManifestOpError`] with `"manifest.not_found"` when the row does
/// not exist; `"internal"` on database failure.
pub async fn get(
    pool: &SqlitePool,
    manifest_id: &str,
) -> Result<ManifestGetResponse, ManifestOpError> {
    let row = get_manifest(pool, manifest_id).await.map_err(|e| {
        if matches!(e, persistence_core::DbError::NotFound(_)) {
            manifest_op_error("manifest.not_found", &format!("manifest {manifest_id} not found"))
        } else {
            manifest_op_error("internal", &format!("DB error: {e}"))
        }
    })?;

    // Deserialise body JSON; fall back to empty body on malformed data.
    let body: ManifestBodyDto = serde_json::from_str(&row.body_json).unwrap_or_else(|_| {
        ManifestBodyDto { lifecycle_state: "unknown".to_owned(), ..Default::default() }
    });

    let manifest = ManifestDto {
        id: row.id,
        project_id: row.project_id,
        reason: str_to_dto_reason(&row.reason),
        timestamp: row.timestamp,
        path: row.path,
        version: row.version,
        body,
    };

    Ok(ManifestGetResponse { manifest })
}

// ── write ─────────────────────────────────────────────────────────────────────

/// Parameters for a manifest write operation.
pub struct WriteManifestParams<'a> {
    pub project_id: &'a str,
    pub reason: DtoManifestReason,
    /// Absolute path to the project root (for resolving `notes/` folder).
    pub project_root: &'a Path,
    pub lifecycle_state: &'a str,
    /// Optional source map JSON value.
    pub source_map: Option<serde_json::Value>,
    /// Optional calibration snapshot JSON value.
    pub calibration: Option<serde_json::Value>,
    /// Optional workflow profile id.
    pub workflow_profile: Option<String>,
}

/// Generate and persist a manifest snapshot.
///
/// Steps:
/// 1. Emit `manifest.write.attempt` audit event (best-effort).
/// 2. Read current notes content from DB for embedding (A8 full text snapshot).
/// 3. Build and render the manifest markdown.
/// 4. Write the file to `<project_root>/notes/`.
/// 5. Insert the DB row.
/// 6. Emit `manifest.write.success` or `manifest.write.failure`.
///
/// Returns the project-relative path on success.
///
/// # Errors
/// Returns a descriptive string on file I/O or DB failure.
pub async fn write(
    pool: &SqlitePool,
    bus: &EventBus,
    params: WriteManifestParams<'_>,
) -> Result<String, String> {
    let manifest_id = new_uuid();
    let timestamp = now_utc_iso();
    let reason = dto_reason_to_struct(params.reason);
    let relative_path = manifest_relative_path(reason, &timestamp);

    // ── Audit: attempt ────────────────────────────────────────────────────────
    let _ = bus
        .publish(
            "manifest.write.attempt",
            Source::System,
            serde_json::json!({
                "projectId": params.project_id,
                "reason": reason.as_db_str(),
                "manifestId": manifest_id,
            }),
        )
        .await;

    // ── Read current notes for embedding (A8) ─────────────────────────────────
    let notes_snapshot = get_note_content(pool, params.project_id).await.unwrap_or(None);

    // ── Build body ────────────────────────────────────────────────────────────
    let body = ManifestBody {
        lifecycle_state: params.lifecycle_state.to_owned(),
        source_map: params.source_map.clone(),
        calibration: params.calibration.clone(),
        workflow_profile: params.workflow_profile.clone(),
        generated_views: vec![],
        notes: notes_snapshot,
    };

    let body_json = serde_json::to_string(&body)
        .map_err(|e| format!("failed to serialise manifest body: {e}"))?;

    // ── Render markdown ───────────────────────────────────────────────────────
    let markdown = render_manifest_markdown(params.project_id, reason, &timestamp, &body);
    let filename = relative_path.strip_prefix("notes/").unwrap_or(&relative_path).to_owned();

    let notes_dir = params.project_root.join("notes");

    // ── Write file ────────────────────────────────────────────────────────────
    let write_result = write_manifest_file(&notes_dir, &filename, &markdown).await;

    if let Err(ref e) = write_result {
        let _ = bus
            .publish(
                "manifest.write.failure",
                Source::System,
                serde_json::json!({
                    "projectId": params.project_id,
                    "reason": reason.as_db_str(),
                    "manifestId": manifest_id,
                    "error": e,
                }),
            )
            .await;
        return Err(format!("manifest file write failed: {e}"));
    }

    // ── Insert DB row ─────────────────────────────────────────────────────────
    insert_manifest(
        pool,
        InsertManifest {
            id: &manifest_id,
            project_id: params.project_id,
            reason: reason.as_db_str(),
            path: &relative_path,
            body_json: &body_json,
            version: MANIFEST_VERSION,
        },
    )
    .await
    .map_err(|e| format!("failed to insert manifest row: {e}"))?;

    // ── Audit: success ────────────────────────────────────────────────────────
    let _ = bus
        .publish(
            "manifest.write.success",
            Source::System,
            serde_json::json!({
                "projectId": params.project_id,
                "reason": reason.as_db_str(),
                "manifestId": manifest_id,
                "path": relative_path,
            }),
        )
        .await;

    Ok(relative_path)
}

// ── Source-map / calibration snapshot (#740) ───────────────────────────────────

/// Build the `source_map`/`calibration` snapshot for a manifest body from the
/// project's currently-linked sources and their calibration assignments.
///
/// FR-001 requires manifests to "document project source mappings [and]
/// calibration choices" — `spawn_workflow_run_subscriber` previously always
/// passed `None` for both, the only trigger a real user ever exercises
/// (#665 tracks the other 4 triggers being unwired).
///
/// `pub`: also called by `project_setup::add_source`/`remove_source` (this
/// crate, the `SourceChange` trigger) and `app_core::plan_apply`'s
/// `finalize_project_create_manifest` (the `Created` trigger, #665).
pub async fn build_source_calibration_snapshot(
    pool: &SqlitePool,
    project_id: &str,
) -> (Option<serde_json::Value>, Option<serde_json::Value>) {
    use persistence_calibration::repositories::calibration_assignment;
    use persistence_plans::repositories::projects as projects_repo;

    let sources = match projects_repo::list_project_sources(pool, project_id).await {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!(error = %e, "manifest snapshot: failed to list project sources");
            return (None, None);
        }
    };
    if sources.is_empty() {
        return (None, None);
    }

    let source_map = serde_json::Value::Array(
        sources
            .iter()
            .map(|s| {
                serde_json::json!({
                    "sourceId": s.id,
                    "inventorySessionId": s.inventory_session_id,
                    "name": s.name_snapshot,
                })
            })
            .collect(),
    );

    let mut calibration_entries = Vec::new();
    for s in &sources {
        match calibration_assignment::list_for_session(pool, &s.inventory_session_id).await {
            Ok(assignments) => {
                calibration_entries.extend(assignments.into_iter().map(|a| {
                    serde_json::json!({
                        "sessionId": a.session_id,
                        "calibrationType": a.calibration_type,
                        "masterId": a.master_id,
                        "confidence": a.confidence,
                        "wasOverride": a.was_override,
                    })
                }));
            }
            Err(e) => {
                tracing::debug!(
                    session_id = s.inventory_session_id,
                    error = %e,
                    "manifest snapshot: failed to list calibration for session"
                );
            }
        }
    }
    let calibration =
        (!calibration_entries.is_empty()).then_some(serde_json::Value::Array(calibration_entries));

    (Some(source_map), calibration)
}

/// Load the project's current path + lifecycle, build its source/calibration
/// snapshot, and write a manifest for `reason` (#665).
///
/// Best-effort: logs and swallows failure rather than propagating it — the
/// triggering mutation (source add/remove, lifecycle transition, project
/// create) has already succeeded by the time this runs; a manifest write
/// failure must never roll it back or surface as the mutation's own error.
///
/// Shared by every #665 trigger site so they can't drift on what a manifest
/// snapshot contains: `project_setup::add_source`/`remove_source`
/// (`SourceChange`), `app_core::plan_apply`'s project-create-plan completion
/// (`Created`), and the `lifecycle.transition.apply` Tauri command
/// (`LifecycleTransition`, `apps/desktop/src-tauri/src/commands/lifecycle.rs`).
///
/// Reads the project row fresh rather than taking a lifecycle string from
/// the caller: every call site here runs strictly after its own mutation
/// has already committed the new lifecycle to the DB, so a fresh read is
/// always the correct post-mutation value.
pub async fn write_lifecycle_manifest(
    pool: &SqlitePool,
    bus: &EventBus,
    project_id: &str,
    reason: DtoManifestReason,
) {
    let row = match persistence_plans::repositories::projects::get_project(pool, project_id).await {
        Ok(row) => row,
        Err(e) => {
            tracing::debug!(project_id, error = %e, "manifest snapshot: could not load project");
            return;
        }
    };
    let (source_map, calibration) = build_source_calibration_snapshot(pool, project_id).await;
    if let Err(e) = write(
        pool,
        bus,
        WriteManifestParams {
            project_id,
            reason,
            project_root: std::path::Path::new(&row.path),
            lifecycle_state: &row.lifecycle,
            source_map,
            calibration,
            workflow_profile: None,
        },
    )
    .await
    {
        tracing::warn!(project_id, error = %e, "manifest write failed");
    }
}

// ── workflow_run_completed subscriber ─────────────────────────────────────────

/// Spawn an event-bus subscriber that listens for `workflow.run_completed`
/// events and writes a `workflow_run` manifest for the named project.
///
/// The resolver performs an async DB lookup to turn a `project_id` into the
/// project's root path (`projects.path`), which is used as the base directory
/// for the manifest file write.
///
/// The subscriber uses the same idempotent write pattern as any other trigger;
/// a retry produces a new file with a later timestamp.
///
/// On broadcast lag, replays missed `workflow.run_completed` events from the
/// durable events table using a monotonic cursor (GF-19).
#[must_use]
pub fn spawn_workflow_run_subscriber(
    pool: SqlitePool,
    bus: EventBus,
) -> tokio::task::JoinHandle<()> {
    use audit::event_bus::TOPIC_WORKFLOW_RUN_COMPLETED;
    use tokio::sync::broadcast::error::RecvError;

    let mut rx = bus.subscribe();
    tokio::spawn(async move {
        // Cursor starts at 0, so the first lag replays from the beginning; every
        // lag thereafter resumes from the high-water mark the replay leaves
        // behind, and the pruner bounds how far back that can reach.
        //
        // Replay is SAFE but not free, and `write()` is not idempotent: it
        // inserts a `manifests` row with a fresh id per call, so a replayed
        // event adds a row the user can see in the manifest list. Same-second
        // replays collapse on disk (the path is second-granular and
        // `write_manifest_file` refuses to overwrite), but the DB rows do not.
        // Duplicate rows beat a silently missing manifest, which is what
        // advancing this cursor on the live path used to cause.
        let mut cursor: i64 = 0;

        loop {
            match rx.recv().await {
                Ok(env) if env.topic == TOPIC_WORKFLOW_RUN_COMPLETED => {
                    // The live path deliberately leaves `cursor` alone. Advancing it
                    // here to `max_event_id` would read the table-wide max rather
                    // than this event's row: any event inserted between the
                    // broadcast and that query gets jumped, so a later `Lagged`
                    // replay starts *after* it and it is lost rather than late.
                    // The envelope carries no `event_id`, so this branch cannot
                    // know its own row to advance to. Only the replay below moves
                    // the cursor, and it does so per row.
                    handle_workflow_run_event(&pool, &bus, &env.payload).await;
                }
                Ok(_) => {}
                Err(RecvError::Lagged(n)) => {
                    tracing::warn!(
                        missed = n,
                        cursor,
                        "workflow_run_subscriber: lagged — replaying from durable events"
                    );
                    cursor = replay_workflow_events_since(&pool, &bus, cursor).await;
                }
                Err(RecvError::Closed) => break,
            }
        }
    })
}

/// Process a single `workflow.run_completed` payload: look up the project,
/// build the manifest snapshot, and write it.
async fn handle_workflow_run_event(pool: &SqlitePool, bus: &EventBus, payload: &serde_json::Value) {
    use persistence_plans::repositories::projects::get_project;

    let project_id = payload.get("projectId").and_then(|v| v.as_str()).map(str::to_owned);
    let tool_id = payload.get("toolId").and_then(|v| v.as_str()).map(str::to_owned);

    if let Some(pid) = project_id {
        let project_row = match get_project(pool, &pid).await {
            Ok(row) => Some(row),
            Err(e) => {
                tracing::debug!(
                    "workflow.run_completed: could not look up project {pid}: {e}; skipping manifest"
                );
                None
            }
        };

        if let Some(row) = project_row {
            let project_root = std::path::PathBuf::from(&row.path);
            let (source_map, calibration) = build_source_calibration_snapshot(pool, &pid).await;
            let result = write(
                pool,
                bus,
                WriteManifestParams {
                    project_id: &pid,
                    reason: DtoManifestReason::WorkflowRun,
                    project_root: &project_root,
                    lifecycle_state: &row.lifecycle,
                    source_map,
                    calibration,
                    workflow_profile: tool_id,
                },
            )
            .await;
            if let Err(e) = result {
                tracing::warn!(project_id = pid, error = %e, "workflow_run manifest write failed");
            }
        }
    }
}

/// Replay `workflow.run_completed` events from the durable table since `cursor`.
/// Pages in chunks of [`REPLAY_PAGE_SIZE`] to bound memory. Returns the updated
/// cursor (highest processed `event_id`, or the incoming cursor on error).
async fn replay_workflow_events_since(pool: &SqlitePool, bus: &EventBus, mut cursor: i64) -> i64 {
    use audit::event_bus::TOPIC_WORKFLOW_RUN_COMPLETED;
    use persistence_lifecycle::repositories::events::{
        list_since_by_topic_paged, REPLAY_PAGE_SIZE,
    };

    loop {
        let rows = list_since_by_topic_paged(
            bus.pool(),
            cursor,
            TOPIC_WORKFLOW_RUN_COMPLETED,
            REPLAY_PAGE_SIZE,
        )
        .await;

        let rows = match rows {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(error = %e, "workflow_run_subscriber: replay query failed");
                break;
            }
        };

        if rows.is_empty() {
            break;
        }

        for row in &rows {
            if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&row.payload) {
                handle_workflow_run_event(pool, bus, &payload).await;
            }
            cursor = row.event_id;
        }
        tracing::info!(replayed = rows.len(), cursor, "workflow_run_subscriber: replay page");
        if rows.len() < usize::try_from(REPLAY_PAGE_SIZE).unwrap_or(usize::MAX) {
            break; // last page
        }
    }
    cursor
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    async fn setup() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("../../../crates/persistence/core/migrations").run(&pool).await.unwrap();
        pool
    }

    async fn insert_project(pool: &SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO projects (id, name, tool, lifecycle, path, notes, channel_drift, created_at, updated_at) \
             VALUES (?,?,?,?,?,?,?,?,?)",
        )
        .bind(id)
        .bind("Test")
        .bind("PixInsight")
        .bind("ready")
        .bind("projects/test")
        .bind::<Option<String>>(None)
        .bind(false)
        .bind("2026-01-01T00:00:00Z")
        .bind("2026-01-01T00:00:00Z")
        .execute(pool)
        .await
        .unwrap();
    }

    fn make_bus(pool: &SqlitePool) -> EventBus {
        EventBus::with_pool(pool.clone())
    }

    #[tokio::test]
    async fn write_creates_row_and_file() {
        let pool = setup().await;
        insert_project(&pool, "proj-w1").await;
        let bus = make_bus(&pool);
        let dir = tempfile::tempdir().unwrap();

        let path = write(
            &pool,
            &bus,
            WriteManifestParams {
                project_id: "proj-w1",
                reason: DtoManifestReason::Created,
                project_root: dir.path(),
                lifecycle_state: "setup_incomplete",
                source_map: None,
                calibration: None,
                workflow_profile: None,
            },
        )
        .await
        .unwrap();

        assert!(path.starts_with("notes/manifest-"));
        // Verify file exists on disk.
        let abs = dir.path().join(&path);
        assert!(abs.exists(), "manifest file should exist at {}", abs.display());
        // Verify DB row.
        let (rows, _) = list_manifests_for_project(&pool, "proj-w1", None, 10).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].reason, "created");
    }

    #[tokio::test]
    async fn list_returns_newest_first() {
        let pool = setup().await;
        insert_project(&pool, "proj-list").await;
        let bus = make_bus(&pool);
        let dir = tempfile::tempdir().unwrap();

        for _ in 0..3 {
            write(
                &pool,
                &bus,
                WriteManifestParams {
                    project_id: "proj-list",
                    reason: DtoManifestReason::SourceChange,
                    project_root: dir.path(),
                    lifecycle_state: "ready",
                    source_map: None,
                    calibration: None,
                    workflow_profile: None,
                },
            )
            .await
            .unwrap();
            // Tiny sleep to ensure timestamp ordering.
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }

        let resp = list(
            &pool,
            ManifestListRequest {
                project_id: "proj-list".to_owned(),
                cursor: None,
                limit: Some(10),
            },
        )
        .await
        .unwrap();
        assert_eq!(resp.manifests.len(), 3);
        // Newest first: timestamps should be descending.
        let ts: Vec<&str> = resp.manifests.iter().map(|m| m.timestamp.as_str()).collect();
        assert!(ts[0] >= ts[1] && ts[1] >= ts[2]);
    }

    #[tokio::test]
    async fn get_returns_manifest_not_found() {
        let pool = setup().await;
        let err = get(&pool, "no-such-id").await.unwrap_err();
        assert_eq!(err.code, "manifest.not_found");
    }

    #[tokio::test]
    async fn get_returns_manifest_body() {
        let pool = setup().await;
        insert_project(&pool, "proj-get").await;
        let bus = make_bus(&pool);
        let dir = tempfile::tempdir().unwrap();

        let path = write(
            &pool,
            &bus,
            WriteManifestParams {
                project_id: "proj-get",
                reason: DtoManifestReason::Created,
                project_root: dir.path(),
                lifecycle_state: "ready",
                source_map: None,
                calibration: None,
                workflow_profile: None,
            },
        )
        .await
        .unwrap();

        let (rows, _) = list_manifests_for_project(&pool, "proj-get", None, 1).await.unwrap();
        let manifest_id = rows[0].id.clone();
        let _ = path; // used above

        let resp = get(&pool, &manifest_id).await.unwrap();
        assert_eq!(resp.manifest.body.lifecycle_state, "ready");
    }

    #[tokio::test]
    async fn write_embeds_notes_snapshot() {
        let pool = setup().await;
        insert_project(&pool, "proj-notes").await;
        // Insert a note for the project.
        persistence_plans::repositories::project_notes::upsert_note(
            &pool,
            "note-001",
            "proj-notes",
            "My telescope notes",
        )
        .await
        .unwrap();

        let bus = make_bus(&pool);
        let dir = tempfile::tempdir().unwrap();

        write(
            &pool,
            &bus,
            WriteManifestParams {
                project_id: "proj-notes",
                reason: DtoManifestReason::Created,
                project_root: dir.path(),
                lifecycle_state: "ready",
                source_map: None,
                calibration: None,
                workflow_profile: None,
            },
        )
        .await
        .unwrap();

        let (rows, _) = list_manifests_for_project(&pool, "proj-notes", None, 1).await.unwrap();
        let body: ManifestBody = serde_json::from_str(&rows[0].body_json).unwrap();
        assert_eq!(body.notes.as_deref(), Some("My telescope notes"));
    }

    // ── T027: spawn_workflow_run_subscriber integration test (FR-008) ─────────

    #[tokio::test]
    async fn workflow_run_subscriber_generates_and_persists_manifest() {
        // Arrange: in-memory DB with a real project row.
        let pool = setup().await;
        let dir = tempfile::tempdir().unwrap();
        // Insert project whose path points to our temp dir.
        sqlx::query(
            "INSERT INTO projects (id, name, tool, lifecycle, path, notes, channel_drift, created_at, updated_at) \
             VALUES (?,?,?,?,?,?,?,?,?)",
        )
        .bind("proj-sub-1")
        .bind("SubTest")
        .bind("PixInsight")
        .bind("ready")
        .bind(dir.path().to_str().unwrap())
        .bind::<Option<String>>(None)
        .bind(false)
        .bind("2026-01-01T00:00:00Z")
        .bind("2026-01-01T00:00:00Z")
        .execute(&pool)
        .await
        .unwrap();

        let bus = make_bus(&pool);

        // Spawn the subscriber (it listens on the bus).
        let _handle = spawn_workflow_run_subscriber(pool.clone(), bus.clone());

        // Act: publish a workflow.run_completed event.
        let _ = bus
            .publish(
                "workflow.run_completed",
                audit::event_bus::Source::System,
                serde_json::json!({
                    "projectId": "proj-sub-1",
                    "toolId": "pixinsight",
                    "toolLaunchId": "tl-sub-1",
                    "completedAt": "2026-06-01T10:00:00Z",
                    "artifactIds": [],
                }),
            )
            .await;

        // Assert: manifest row appears within a short timeout.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let (rows, _) =
                list_manifests_for_project(&pool, "proj-sub-1", None, 10).await.unwrap();
            if !rows.is_empty() {
                // Verify reason and file existence.
                assert_eq!(rows[0].reason, "workflow_run");
                let abs_path = dir.path().join(&rows[0].path);
                assert!(abs_path.exists(), "manifest file should exist at {}", abs_path.display());
                // #740: lifecycle_state must be the project's REAL lifecycle
                // ("ready", from the inserted row), never the hardcoded "unknown".
                let resp = get(&pool, &rows[0].id).await.unwrap();
                assert_eq!(resp.manifest.body.lifecycle_state, "ready");
                return; // success
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "manifest row not found within 2 s after workflow.run_completed event"
            );
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    /// A `Lagged` replay must never skip a durable event the live path never
    /// delivered — regression test for the cursor-jump defect (astro-plan-hyk0).
    ///
    /// The live branch used to advance the cursor with `max_event_id`, the
    /// TABLE-WIDE max, rather than the row it had just handled. An event that
    /// reached the table without reaching this subscriber therefore sat *below*
    /// the new cursor and was never replayed, so its manifest was never written
    /// — lost, not merely late.
    ///
    /// Deterministic by ordering rather than by racing: the never-broadcast row
    /// is inserted FIRST, so it always holds the lower `event_id`.
    #[tokio::test]
    async fn lag_replay_writes_the_manifest_for_an_event_the_live_path_never_saw() {
        let pool = setup().await;
        let dir = tempfile::tempdir().unwrap();
        for id in ["proj-lost", "proj-live"] {
            // `projects.path` is UNIQUE, so each project needs its own directory.
            let project_dir = dir.path().join(id);
            std::fs::create_dir_all(&project_dir).unwrap();
            sqlx::query(
                "INSERT INTO projects (id, name, tool, lifecycle, path, notes, channel_drift, created_at, updated_at) \
                 VALUES (?,?,?,?,?,?,?,?,?)",
            )
            .bind(id)
            .bind(id)
            .bind("PixInsight")
            .bind("ready")
            .bind(project_dir.to_str().unwrap())
            .bind::<Option<String>>(None)
            .bind(false)
            .bind("2026-01-01T00:00:00Z")
            .bind("2026-01-01T00:00:00Z")
            .execute(&pool)
            .await
            .unwrap();
        }

        // Capacity 1 so a lag is reachable at all.
        let bus = EventBus::new(pool.clone(), 1);
        let _handle = spawn_workflow_run_subscriber(pool.clone(), bus.clone());

        // The event the subscriber can never see live: durable row, no broadcast.
        // Inserted first, so a jumped cursor would strand it.
        persistence_lifecycle::repositories::events::insert_event(
            &pool,
            "workflow.run_completed",
            "system",
            "2026-01-01T00:00:00Z",
            &serde_json::json!({
                "projectId": "proj-lost",
                "toolId": "pixinsight",
                "toolLaunchId": "tl-lost",
                "completedAt": "2026-06-01T10:00:00Z",
                "artifactIds": [],
            })
            .to_string(),
        )
        .await
        .unwrap();

        // A live event, which is what used to jump the cursor past the row above.
        let _ = bus
            .publish(
                "workflow.run_completed",
                audit::event_bus::Source::System,
                serde_json::json!({
                    "projectId": "proj-live",
                    "toolId": "pixinsight",
                    "toolLaunchId": "tl-live",
                    "completedAt": "2026-06-01T10:00:00Z",
                    "artifactIds": [],
                }),
            )
            .await;

        // Wait for the live one to be handled, so the cursor logic has run.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let (rows, _) = list_manifests_for_project(&pool, "proj-live", None, 10).await.unwrap();
            if !rows.is_empty() {
                break;
            }
            assert!(tokio::time::Instant::now() < deadline, "live manifest not written in time");
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }

        // Force a lag: two broadcasts with no await between them, so the
        // subscriber cannot drain the channel in between.
        let _ = bus.broadcast_only("tick.heartbeat");
        let _ = bus.broadcast_only("tick.heartbeat");

        // The replay must reach the stranded row and write ITS manifest.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let (rows, _) = list_manifests_for_project(&pool, "proj-lost", None, 10).await.unwrap();
            if !rows.is_empty() {
                return; // success
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "the lag replay never wrote a manifest for the event the live path \
                 never saw: the cursor jumped past it"
            );
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    }

    /// #740: a project with a linked source (and a calibration assignment for
    /// that source's session) must produce a manifest whose body actually
    /// documents them, not `source_map: null, calibration: null` forever.
    #[tokio::test]
    async fn workflow_run_subscriber_documents_source_map_and_calibration() {
        let pool = setup().await;
        let dir = tempfile::tempdir().unwrap();
        sqlx::query(
            "INSERT INTO projects (id, name, tool, lifecycle, path, notes, channel_drift, created_at, updated_at) \
             VALUES (?,?,?,?,?,?,?,?,?)",
        )
        .bind("proj-snap-1")
        .bind("SnapTest")
        .bind("PixInsight")
        .bind("ready")
        .bind(dir.path().to_str().unwrap())
        .bind::<Option<String>>(None)
        .bind(false)
        .bind("2026-01-01T00:00:00Z")
        .bind("2026-01-01T00:00:00Z")
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO project_sources (id, project_id, inventory_session_id, name_snapshot, frames_snapshot, filter_snapshot, exposure_snapshot, linked_at)
             VALUES ('src-1', 'proj-snap-1', 'sess-1', 'M31 Lum', 42, 'L', '300', '2026-01-01T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();

        persistence_calibration::repositories::calibration_assignment::upsert(
            &pool,
            persistence_calibration::repositories::calibration_assignment::UpsertParams {
                id: "calib-1",
                session_id: "sess-1",
                calibration_type: "dark",
                master_id: "master-1",
                confidence: 0.95,
                was_override: false,
                mismatched_dimensions: &[],
                assigned_at: None,
            },
        )
        .await
        .unwrap();

        let bus = make_bus(&pool);
        let _handle = spawn_workflow_run_subscriber(pool.clone(), bus.clone());
        let _ = bus
            .publish(
                "workflow.run_completed",
                audit::event_bus::Source::System,
                serde_json::json!({
                    "projectId": "proj-snap-1",
                    "toolId": "pixinsight",
                    "toolLaunchId": "tl-snap-1",
                    "completedAt": "2026-06-01T10:00:00Z",
                    "artifactIds": [],
                }),
            )
            .await;

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let (rows, _) =
                list_manifests_for_project(&pool, "proj-snap-1", None, 10).await.unwrap();
            if !rows.is_empty() {
                let resp = get(&pool, &rows[0].id).await.unwrap();
                let source_map =
                    resp.manifest.body.source_map.expect("source_map must be populated").0;
                assert_eq!(source_map[0]["name"], "M31 Lum");
                let calibration =
                    resp.manifest.body.calibration.expect("calibration must be populated").0;
                assert_eq!(calibration[0]["calibrationType"], "dark");
                assert_eq!(calibration[0]["masterId"], "master-1");
                return;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "manifest row not found within 2 s after workflow.run_completed event"
            );
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    #[tokio::test]
    async fn list_pagination_works() {
        let pool = setup().await;
        insert_project(&pool, "proj-pag").await;
        let bus = make_bus(&pool);
        let dir = tempfile::tempdir().unwrap();

        for i in 0..55u32 {
            write(
                &pool,
                &bus,
                WriteManifestParams {
                    project_id: "proj-pag",
                    reason: DtoManifestReason::SourceChange,
                    project_root: dir.path(),
                    lifecycle_state: "ready",
                    source_map: None,
                    calibration: None,
                    workflow_profile: None,
                },
            )
            .await
            .unwrap();
            let _ = i;
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }

        let resp1 = list(
            &pool,
            ManifestListRequest {
                project_id: "proj-pag".to_owned(),
                cursor: None,
                limit: Some(50),
            },
        )
        .await
        .unwrap();
        assert_eq!(resp1.manifests.len(), 50);
        assert!(resp1.next_cursor.is_some(), "should have next cursor");

        let resp2 = list(
            &pool,
            ManifestListRequest {
                project_id: "proj-pag".to_owned(),
                cursor: resp1.next_cursor.clone(),
                limit: Some(50),
            },
        )
        .await
        .unwrap();
        assert_eq!(resp2.manifests.len(), 5);
        assert!(resp2.next_cursor.is_none());
    }
}
