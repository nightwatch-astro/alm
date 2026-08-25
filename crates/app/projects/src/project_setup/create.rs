// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `project.create` — validate, anchor path, build the Constitution II
//! folder-structure plan, persist, infer channels, emit audit.

use audit::bus::EventBus;
use audit::event_bus::Source;
use contracts_core::projects_v2::{ProjectChannelDto, ProjectCreateRequest, ProjectCreateResult};
use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use domain_core::first_run::SourceKind;
use domain_core::ids::{new_id, Timestamp};
use domain_core::project::validate::{validate_name, validate_tool};
use persistence_lifecycle::repositories::first_run as first_run_repo;
use persistence_plans::repositories::plans as plans_repo;
use persistence_plans::repositories::projects as repo;
use project_structure::{required_folders, ProcessingTool as StructureTool, MARKER_FILENAME};
use sqlx::SqlitePool;

use app_core_errors::bus_err;

use super::{
    channel_dto_from_domain, channel_totals_by_filter, db_err, infer_from_sources,
    maybe_auto_ready, source_snapshot, str_to_error_code, SourceSnapshot,
};

// ── Path anchoring (Constitution I) ───────────────────────────────────────────

/// Resolve the requested project path to an unambiguous absolute location.
///
/// `projects.path` is consumed as an absolute path by every downstream reader
/// (scaffolding `mkdir` executor, spec-011 tool-launch cwd, artifact watcher,
/// spec-024 manifests). The creation wizard historically submitted a
/// library-relative path (`projects/<slug>`) which was stored verbatim and
/// silently resolved against the process CWD at each consumer. Constitution I
/// models roots separately from relative paths, so a relative request path is
/// anchored here — at creation — to the registered project folder
/// (`registered_sources.kind = 'project'`, earliest registration wins).
///
/// Rules:
/// - `..` components are rejected (`path.invalid`) — a project path must not
///   escape its anchor;
/// - a path with no `Normal` component is rejected (`path.invalid`) — a
///   filesystem or device root cannot be a destination root;
/// - an absolute path is used as-is (validated downstream by containment);
/// - a relative path is joined onto the registered project folder;
/// - a relative path with no registered project folder is rejected
///   (`path.invalid`) — there is no unambiguous location to anchor to.
async fn anchor_project_path(pool: &SqlitePool, raw: &str) -> Result<String, ContractError> {
    let trimmed = raw.trim();
    let candidate = std::path::Path::new(trimmed);

    if candidate.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        let message = "Project path must not contain '..' components.";
        return Err(ContractError::new(
            ErrorCode::PathInvalid,
            message,
            ErrorSeverity::Blocking,
            false,
        )
        .with_field_error(super::field_error("path", ErrorCode::PathInvalid, message)));
    }

    // A path whose components are only a root and/or a device prefix (`/`,
    // `C:\`, `\\server\share`) or only `.` names no folder, so downstream
    // readers would take the filesystem root itself as the project's
    // destination root and scaffold directly into it.
    if !candidate.components().any(|c| matches!(c, std::path::Component::Normal(_))) {
        let message = "Project path must name a folder, not a filesystem or device root.";
        return Err(ContractError::new(
            ErrorCode::PathInvalid,
            message,
            ErrorSeverity::Blocking,
            false,
        )
        .with_field_error(super::field_error("path", ErrorCode::PathInvalid, message)));
    }

    if candidate.is_absolute() {
        return Ok(trimmed.to_owned());
    }

    let project_root = first_run_repo::list_sources(pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .find(|s| s.kind == SourceKind::Project)
        .map(|s| s.path);

    let Some(root) = project_root else {
        let message = "Project path is relative and no project folder is registered; \
             register a project folder in setup or provide an absolute path.";
        return Err(ContractError::new(
            ErrorCode::PathInvalid,
            message,
            ErrorSeverity::Blocking,
            false,
        )
        .with_field_error(super::field_error("path", ErrorCode::PathInvalid, message)));
    };

    // Join with '/' (valid on all supported platforms); strip trailing
    // separators from the root so the stored path has a single separator.
    Ok(format!("{}/{}", root.trim_end_matches(['/', '\\']), trimmed))
}

// ── Folder plan builder (Constitution II) ─────────────────────────────────────

/// One resolved (not yet persisted) folder-plan item destination.
struct FolderPlanItemData {
    id: String,
    name: String,
    action: &'static str,
    to_relative_path: String,
    reason: &'static str,
}

/// Resolved (not yet persisted) folder-structure plan for a project
/// (Constitution II): one `mkdir` item per sub-folder required by the tool
/// (`project_structure::required_folders`) plus one `write_manifest` item for
/// the app-owned project marker.
struct FolderPlanData {
    plan_id: String,
    plan_title: String,
    items: Vec<FolderPlanItemData>,
}

/// Resolve the folder-structure plan for `project_path`/`tool_str` without
/// touching the database (pure). The caller borrows these owned strings into
/// `persistence_plans::repositories::plans::{InsertPlan, InsertPlanItem}` for
/// [`repo::create_project_tx`], which persists the plan (in `draft`, then
/// advanced to `ready_for_review`) atomically with the rest of project
/// creation.
fn build_folder_plan_data(project_path: &str, tool_str: &str) -> FolderPlanData {
    let plan_id = new_id();
    let tool = StructureTool::parse(tool_str).unwrap_or(StructureTool::PixInsight);
    let folders = required_folders(tool);
    let plan_title = format!("Create project folder structure: {project_path}");

    let mut items = Vec::with_capacity(folders.len() + 1);
    for folder in &folders {
        items.push(FolderPlanItemData {
            id: new_id(),
            name: folder.0.clone(),
            action: "mkdir",
            to_relative_path: format!("{project_path}/{}", folder.0),
            reason: "Create project sub-folder for tool workflow",
        });
    }
    // One write_manifest item for the project marker file.
    items.push(FolderPlanItemData {
        id: new_id(),
        name: MARKER_FILENAME.to_owned(),
        action: "write_manifest",
        to_relative_path: format!("{project_path}/{MARKER_FILENAME}"),
        reason: "Write app-owned project marker file",
    });

    FolderPlanData { plan_id, plan_title, items }
}

// ── project.create ────────────────────────────────────────────────────────────

/// Create a new project.
///
/// Validates name (non-empty, ≤120 chars, unique), tool (canonical value),
/// path (unique within library). A relative request path is anchored to the
/// registered project folder before storage so `projects.path` is always an
/// unambiguous absolute location (Constitution I; see `anchor_project_path`).
/// Persists the project in `setup_incomplete`,
/// links any `initial_sources`, infers channels, checks the auto-ready trigger,
/// and emits a `project.created` audit event.
///
/// Constitution II: folder structure creation goes through a persisted,
/// auditable FilesystemPlan returned as `plan_id`. Application is driven by
/// the caller: the app_core `project_create` orchestration auto-applies the
/// plan when every action is directory creation (user decision 2026-07-04,
/// supersedes handover D16) and falls back to the manual review flow
/// otherwise.
///
/// # Errors
///
/// Returns `ContractError` on validation failure or database error.
#[allow(clippy::too_many_lines)]
pub async fn create(
    pool: &SqlitePool,
    bus: &EventBus,
    redb_cache: &dyn simbad_resolver::Cache,
    req: &ProjectCreateRequest,
) -> Result<ProjectCreateResult, ContractError> {
    // 1. Validate name.
    validate_name(&req.name).map_err(|code| {
        let error_code = str_to_error_code(code);
        let message = format!("Project name error: {code}");
        ContractError::new(error_code, message.clone(), ErrorSeverity::Blocking, false)
            .with_field_error(super::field_error("name", error_code, message))
    })?;
    validate_tool(req.tool.as_db_str()).map_err(|code| {
        let error_code = str_to_error_code(code);
        let message = format!("Processing tool error: {code}");
        ContractError::new(error_code, message.clone(), ErrorSeverity::Blocking, false)
            .with_field_error(super::field_error("tool", error_code, message))
    })?;

    // 2. Check name uniqueness.
    if let Some(conflict_id) = repo::name_exists(pool, &req.name, None).await.map_err(db_err)? {
        let message = "A project with this name already exists.";
        return Err(ContractError::new(
            ErrorCode::NameDuplicate,
            message,
            ErrorSeverity::Blocking,
            false,
        )
        .with_field_error(super::field_error("name", ErrorCode::NameDuplicate, message))
        .with_details(serde_json::json!({ "conflictingProjectId": conflict_id })));
    }

    // 3. Validate path non-empty.
    if req.path.trim().is_empty() {
        let message = "Project path must not be empty.";
        return Err(ContractError::new(
            ErrorCode::PathInvalid,
            message,
            ErrorSeverity::Blocking,
            false,
        )
        .with_field_error(super::field_error("path", ErrorCode::PathInvalid, message)));
    }

    // 3b. Anchor a relative path to the registered project folder so the
    //     stored path is an unambiguous absolute location (Constitution I).
    let project_path = anchor_project_path(pool, &req.path).await?;

    // 4. Check path uniqueness (on the anchored path).
    if let Some(collide_id) = repo::path_exists(pool, &project_path, None).await.map_err(db_err)? {
        let message = "Another project already uses this path.";
        return Err(ContractError::new(
            ErrorCode::PathCollision,
            message,
            ErrorSeverity::Blocking,
            false,
        )
        .with_field_error(super::field_error("path", ErrorCode::PathCollision, message))
        .with_details(serde_json::json!({ "collidingProjectId": collide_id })));
    }

    let project_id = new_id();
    let now = Timestamp::now_iso();

    // Promote the optional canonical target into the durable table if it
    // isn't already there (spec 052 P1 FR-004: adding a target to a project
    // is an in-use commit). `target.resolve`/`target.search` no longer write
    // `canonical_target` themselves, so `ctid` is typically a redb-cache-only
    // id at this point; a dangling id (unknown to both stores) is still
    // rejected rather than silently stored.
    if let Some(ctid) = req.canonical_target_id.as_deref() {
        let uuid = uuid::Uuid::parse_str(ctid).map_err(|_| {
            ContractError::new(
                ErrorCode::CanonicalTargetNotFound,
                "The selected target was not found.",
                ErrorSeverity::Blocking,
                false,
            )
            .with_details(serde_json::json!({ "canonicalTargetId": ctid }))
        })?;
        let promoted = app_core_targets::target_resolve::promote_by_id(
            pool,
            redb_cache,
            uuid,
            &req.request_id,
        )
        .await?;
        if !promoted {
            return Err(ContractError::new(
                ErrorCode::CanonicalTargetNotFound,
                "The selected target was not found.",
                ErrorSeverity::Blocking,
                false,
            )
            .with_details(serde_json::json!({ "canonicalTargetId": ctid })));
        }
    }

    // 5. Build the project row, initial source links, inferred channels, and
    //    the Constitution II folder-structure plan (+ items) — every write
    //    this use case needs — then persist them as ONE atomic unit via
    //    `create_project_tx`. A mid-sequence database failure previously left
    //    a half-built project (project row with no sources/channels/plan);
    //    see `docs/development/duplication-and-abstraction-audit.md` T2-a.
    let insert = repo::InsertProject {
        id: &project_id,
        name: &req.name,
        tool: req.tool.as_db_str(),
        lifecycle: "setup_incomplete",
        path: &project_path,
        notes: req.notes.as_deref(),
        canonical_target_id: req.canonical_target_id.as_deref(),
        is_mosaic: req.is_mosaic,
    };

    // Link initial sources (best-effort: an id with no acquisition session is
    // linked with an empty snapshot rather than failing the create). Ids are
    // pre-generated so `source_rows` (used for channel inference and the
    // response DTOs) and `source_data` (borrowed into the composite insert)
    // can share them without re-querying the DB.
    //
    // Snapshots are resolved HERE, before `create_project_tx`, because that
    // call must stay one atomic unit (see the comment above `insert`).
    let source_ids: Vec<String> = req.initial_sources.iter().map(|_| new_id()).collect();
    let mut snapshots: Vec<SourceSnapshot> = Vec::with_capacity(req.initial_sources.len());
    for inv_id in &req.initial_sources {
        snapshots.push(source_snapshot(pool, inv_id).await?);
    }
    let source_rows: Vec<repo::ProjectSourceRow> = req
        .initial_sources
        .iter()
        .zip(source_ids.iter())
        .zip(snapshots.iter())
        .map(|((inv_id, src_id), snap)| repo::ProjectSourceRow {
            id: src_id.clone(),
            project_id: project_id.clone(),
            inventory_session_id: inv_id.clone(),
            name_snapshot: snap.name.clone(),
            frames_snapshot: snap.frames,
            filter_snapshot: snap.filter.clone(),
            exposure_snapshot: snap.exposure.clone(),
            linked_at: now.clone(),
        })
        .collect();
    let source_data: Vec<repo::InsertProjectSource> = req
        .initial_sources
        .iter()
        .zip(source_ids.iter())
        .zip(snapshots.iter())
        .map(|((inv_id, src_id), snap)| repo::InsertProjectSource {
            id: src_id,
            project_id: &project_id,
            inventory_session_id: inv_id,
            name_snapshot: &snap.name,
            frames_snapshot: snap.frames,
            filter_snapshot: &snap.filter,
            exposure_snapshot: &snap.exposure,
            linked_at: &now,
        })
        .collect();

    // Infer channels from initial sources.
    let channels = infer_from_sources(&source_rows);
    let channel_pairs: Vec<(&str, &str)> =
        channels.iter().map(|c| (c.label.as_str(), c.source.as_str())).collect();

    // Constitution II folder-structure plan.
    let folder_plan = build_folder_plan_data(&project_path, req.tool.as_db_str());
    let plan_items: Vec<plans_repo::InsertPlanItem> = folder_plan
        .items
        .iter()
        .enumerate()
        .map(|(idx, item)| plans_repo::InsertPlanItem {
            id: &item.id,
            plan_id: &folder_plan.plan_id,
            item_index: i64::try_from(idx).unwrap_or(0),
            name: &item.name,
            action: item.action,
            from_root_id: None,
            from_relative_path: "",
            to_root_id: None,
            to_relative_path: &item.to_relative_path,
            reason: item.reason,
            protection: "normal",
            linked_entity: Some(&project_id),
            provenance_json: None,
            archive_path: None,
            // Project setup items create app-managed folders/files; source
            // protection does not apply.
            source_id: None,
            category: None,
        })
        .collect();
    let plan_data = plans_repo::InsertPlan {
        id: &folder_plan.plan_id,
        title: &folder_plan.plan_title,
        origin: "project",
        origin_path: Some(&project_path),
        plan_type: "project_create",
        destructive_destination: "archive",
        parent_plan_id: None,
        total_bytes_required: 0,
    };

    let create_input = repo::CreateProjectInput {
        project: insert,
        sources: &source_data,
        channels: &channel_pairs,
        channels_added_at: &now,
        plan: plan_data,
        plan_items: &plan_items,
    };
    repo::create_project_tx(pool, &create_input).await.map_err(db_err)?;
    let plan_id = folder_plan.plan_id;

    // 6. Auto-transition setup_incomplete → ready if sources are present.
    let final_lifecycle = maybe_auto_ready(pool, bus, &project_id, "setup_incomplete")
        .await
        .map_err(db_err)?
        .unwrap_or_else(|| "setup_incomplete".to_owned());

    // 7. Audit.
    let audit_id = new_id();
    bus.publish(
        "project.created",
        Source::User,
        serde_json::json!({
            "auditId": audit_id,
            "projectId": project_id,
            "name": req.name,
            "tool": req.tool.as_db_str(),
            "lifecycle": final_lifecycle,
            "sourceCount": source_rows.len(),
            "isMosaic": req.is_mosaic,
        }),
    )
    .await
    .map_err(bus_err)?;

    let channel_totals = channel_totals_by_filter(&source_rows);
    let channel_dtos: Vec<ProjectChannelDto> =
        channels.into_iter().map(|c| channel_dto_from_domain(c, &now, &channel_totals)).collect();

    Ok(ProjectCreateResult {
        project_id,
        is_mosaic: req.is_mosaic,
        lifecycle: final_lifecycle,
        plan_id: Some(plan_id),
        channels: channel_dtos,
        audit_id,
        created_at: now,
        // Set by the app_core `project_create` orchestration when the
        // scaffolding plan qualifies for mkdir-only auto-apply (user decision
        // 2026-07-04). This module only persists the reviewable plan.
        scaffold_applied: None,
    })
}
