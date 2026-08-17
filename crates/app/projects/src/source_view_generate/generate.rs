// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `sourceview.generate` — the plan-build pipeline itself.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use camino::{Utf8Path, Utf8PathBuf};
use contracts_core::source_view_generate::{
    GenerationWarning, GenerationWarningCode, SourceViewGenerateRequest, SourceViewGenerateResponse,
};
use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use domain_core::ids::new_id;
use domain_core::source_view::Materialization;
use patterns::{resolve_pattern_str, MetadataBundle};
use persistence_plans::repositories::plans as plans_repo;
use persistence_plans::repositories::projects as projects_repo;
use persistence_plans::repositories::q_projects;
use sqlx::SqlitePool;

use app_core_errors::db_internal_ctx;

use crate::prepared_views::{check_project_lifecycle, project_db_err};

use super::{
    exceeds_windows_long_path_limit, frames_for_ids, get_destination_override, join_portable,
    layout_resolve_err, parse_frame_ids, session_night,
};

/// A single planned link: canonical source (root + relative path) → the
/// view-relative destination path, plus the inventory reference to carry
/// into `PreparedSourceViewItem.inventory_item_id` on successful apply.
struct PlannedLink {
    inventory_item_id: String,
    source_root_id: String,
    source_relative_path: String,
    dest_relative: Utf8PathBuf,
}

/// Intermediate collection from the source-planning phase.
struct SourcePlanResult {
    planned: Vec<PlannedLink>,
    warnings: Vec<GenerationWarning>,
}

/// Build a `prepared_view_generation` plan for `req.project_id`.
///
/// Validates:
/// 1. Project exists and its lifecycle permits view operations (not `archived`).
/// 2. At least one selected light frame resolves (`no_selection` otherwise).
/// 3. Every planned item has an achievable link kind, or `copyOptIn` is set
///    (`no_link_kind` otherwise, FR-003/FR-004b).
/// 4. No two planned items collide on the same destination path
///    (`destination.collision`, FR-009a) and no destination path already
///    exists as a user file/folder (`destination.exists`, FR-016).
///
/// # Errors
///
/// Returns `project.not_found`, `lifecycle.read_only`, `no_selection`,
/// `no_link_kind`, `destination.collision`, `destination.exists`, or an
/// `internal.*` error on failure.
// CCN 11 tolerated: linear pipeline orchestrator with early-return gates.
pub async fn generate_source_view(
    pool: &SqlitePool,
    req: &SourceViewGenerateRequest,
) -> Result<SourceViewGenerateResponse, ContractError> {
    // 1. Project + lifecycle gate.
    let project =
        projects_repo::get_project(pool, &req.project_id).await.map_err(project_db_err)?;
    check_project_lifecycle(pool, &req.project_id).await?;

    // 2. Resolve project-linked sessions.
    let sources = projects_repo::list_project_sources(pool, &req.project_id)
        .await
        .map_err(|e| db_internal_ctx(e, "list project sources"))?;

    // 3. Profile-driven layout + destination resolution.
    let layout = workflow_profiles::seed::resolve_source_view_layout(
        req.profile_id.as_deref().or(Some(project.tool.as_str())),
    );
    let plan_id = new_id();
    let destination_root = resolve_destination_root(pool, req, &project.path, &plan_id).await?;

    // 4. Plan source frames and calibration.
    let SourcePlanResult { planned, mut warnings } =
        plan_source_frames(pool, &sources, &layout, req.strict).await?;

    if planned.is_empty() {
        return Err(ContractError::new(
            ErrorCode::NoSelection,
            "project has no selected light frames to generate a source view from",
            ErrorSeverity::Blocking,
            false,
        ));
    }

    // 5. Guards and warnings.
    guard_collisions(&planned)?;
    guard_destinations_exist(&planned, &destination_root)?;
    warnings.extend(check_long_paths(&planned, &destination_root));

    // 6. Resolve link kinds.
    let (resolved_kinds, drift_warning) =
        resolve_link_kinds(pool, &planned, &destination_root, &project.path, req.copy_opt_in)
            .await?;
    warnings.extend(drift_warning);

    let used_copy_fallback = resolved_kinds.values().any(|kind| *kind == Materialization::Copy);

    // 7. Persist the plan.
    persist_plan(pool, &plan_id, &req.project_id, &planned, &resolved_kinds, &destination_root)
        .await?;

    Ok(SourceViewGenerateResponse { plan_id, warnings, used_copy_fallback })
}

/// T041 precedence: per-generation override > per-project persisted override > default.
async fn resolve_destination_root(
    pool: &SqlitePool,
    req: &SourceViewGenerateRequest,
    project_path: &str,
    plan_id: &str,
) -> Result<Utf8PathBuf, ContractError> {
    if let Some(dest) = req.destination_override.as_deref() {
        return Ok(Utf8PathBuf::from(dest));
    }
    if let Some(dest) = get_destination_override(pool, &req.project_id).await? {
        return Ok(Utf8PathBuf::from(dest));
    }
    let root = Utf8PathBuf::from(project_path);
    Ok(join_portable(&join_portable(&root, "source-views"), plan_id))
}

// ── Phase: source frame planning ──────────────────────────────────────────────

/// Iterate project sources and matched calibration, collecting planned links
/// and surfacing unresolved/partial-coverage warnings.
async fn plan_source_frames(
    pool: &SqlitePool,
    sources: &[projects_repo::ProjectSourceRow],
    layout: &workflow_profiles::SourceViewLayout,
    strict: bool,
) -> Result<SourcePlanResult, ContractError> {
    let mut planned: Vec<PlannedLink> = Vec::new();
    let mut unresolved_refs: Vec<String> = Vec::new();
    let mut sessions_without_calibration: Vec<String> = Vec::new();
    let mut session_calibration_types: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for src in sources {
        let Some(session) =
            q_projects::get_acquisition_session_view(pool, &src.inventory_session_id)
                .await
                .map_err(|e| db_internal_ctx(e, "load acquisition session"))?
        else {
            unresolved_refs.push(src.inventory_session_id.clone());
            continue;
        };

        let any_light = plan_light_frames_for_session(
            pool,
            src,
            &session,
            layout,
            &mut planned,
            &mut unresolved_refs,
        )
        .await?;
        if !any_light {
            continue;
        }

        plan_calibration_for_session(
            pool,
            src,
            layout,
            &session.root_id,
            &mut planned,
            &mut unresolved_refs,
            &mut sessions_without_calibration,
            &mut session_calibration_types,
        )
        .await?;
    }

    detect_partial_calibration_coverage(
        &session_calibration_types,
        &mut sessions_without_calibration,
    );

    let warnings = assemble_source_warnings(strict, unresolved_refs, sessions_without_calibration)?;

    Ok(SourcePlanResult { planned, warnings })
}

/// Resolve and plan light frames for a single acquisition session.
/// Returns whether any present light frame was found.
async fn plan_light_frames_for_session(
    pool: &SqlitePool,
    src: &projects_repo::ProjectSourceRow,
    session: &q_projects::AcquisitionSessionViewRow,
    layout: &workflow_profiles::SourceViewLayout,
    planned: &mut Vec<PlannedLink>,
    unresolved_refs: &mut Vec<String>,
) -> Result<bool, ContractError> {
    let mut light_bundle: MetadataBundle = HashMap::new();
    light_bundle.insert("filter".to_owned(), src.filter_snapshot.clone());
    light_bundle.insert("exposure".to_owned(), src.exposure_snapshot.clone());
    light_bundle.insert("date".to_owned(), session_night(&session.session_key));
    let light_dir = Utf8PathBuf::from(
        resolve_pattern_str(layout.light_pattern, &light_bundle)
            .map_err(|e| layout_resolve_err(&e, &src.inventory_session_id))?
            .relative_path,
    );

    let frame_ids = parse_frame_ids(&session.frame_ids);
    let frames = frames_for_ids(pool, &frame_ids).await;

    let mut any_light_present = false;
    for frame in &frames {
        if frame.state == "missing" || frame.state == "rejected" {
            unresolved_refs.push(frame.id.clone());
            continue;
        }
        any_light_present = true;
        let basename = Utf8Path::new(&frame.relative_path)
            .file_name()
            .unwrap_or(&frame.relative_path)
            .to_owned();
        planned.push(PlannedLink {
            inventory_item_id: frame.id.clone(),
            source_root_id: session.root_id.clone(),
            source_relative_path: frame.relative_path.clone(),
            dest_relative: join_portable(&light_dir, basename.as_str()),
        });
    }
    Ok(any_light_present)
}

/// Assemble warnings from accumulated unresolved refs and calibration gaps.
fn assemble_source_warnings(
    strict: bool,
    unresolved_refs: Vec<String>,
    sessions_without_calibration: Vec<String>,
) -> Result<Vec<GenerationWarning>, ContractError> {
    let mut warnings: Vec<GenerationWarning> = Vec::new();

    if !unresolved_refs.is_empty() {
        if strict {
            return Err(ContractError::new(
                ErrorCode::NoSelection,
                format!(
                    "strict mode: {} source(s) could not be resolved: {}",
                    unresolved_refs.len(),
                    unresolved_refs.join(", ")
                ),
                ErrorSeverity::Blocking,
                false,
            ));
        }
        warnings.push(GenerationWarning {
            code: GenerationWarningCode::UnresolvedSource,
            message: format!(
                "{} source(s) could not be resolved and were skipped",
                unresolved_refs.len()
            ),
            items: unresolved_refs,
        });
    }

    if !sessions_without_calibration.is_empty() {
        warnings.push(GenerationWarning {
            code: GenerationWarningCode::NoCalibrationApplied,
            message: "generated without matched calibration (missing or partial coverage) \
                      for one or more light groups"
                .to_owned(),
            items: sessions_without_calibration,
        });
    }

    Ok(warnings)
}

/// Plan calibration links for a single source session.
#[allow(clippy::too_many_arguments)]
async fn plan_calibration_for_session(
    pool: &SqlitePool,
    src: &projects_repo::ProjectSourceRow,
    layout: &workflow_profiles::SourceViewLayout,
    _root_id: &str,
    planned: &mut Vec<PlannedLink>,
    unresolved_refs: &mut Vec<String>,
    sessions_without_calibration: &mut Vec<String>,
    session_calibration_types: &mut BTreeMap<String, BTreeSet<String>>,
) -> Result<(), ContractError> {
    let assignments: Vec<(String, String)> =
        q_projects::list_calibration_assignment_types(pool, &src.inventory_session_id)
            .await
            .unwrap_or_default();

    if assignments.is_empty() {
        sessions_without_calibration.push(src.inventory_session_id.clone());
        return Ok(());
    }

    session_calibration_types.insert(
        src.inventory_session_id.clone(),
        assignments.iter().map(|(t, _)| t.clone()).collect(),
    );

    for (cal_type, master_id) in assignments {
        let Some((cal_root_id, cal_frame_ids_json)) =
            q_projects::get_calibration_session_view(pool, &master_id).await.unwrap_or(None)
        else {
            unresolved_refs.push(master_id.clone());
            continue;
        };

        // Calibration goes to the profile's expected calibration location
        // (FR-010); every matched set still gets its own `master_id`
        // subdirectory so two masters of the same type never collide
        // (FR-009a/CL-5).
        let mut cal_bundle: MetadataBundle = HashMap::new();
        cal_bundle.insert("frame_type".to_owned(), cal_type.clone());
        let cal_dir = join_portable(
            &Utf8PathBuf::from(
                resolve_pattern_str(layout.calibration_pattern, &cal_bundle)
                    .map_err(|e| layout_resolve_err(&e, &master_id))?
                    .relative_path,
            ),
            &master_id,
        );

        let cal_frame_ids = parse_frame_ids(&cal_frame_ids_json);
        let cal_frames = frames_for_ids(pool, &cal_frame_ids).await;
        for frame in &cal_frames {
            if frame.state == "missing" || frame.state == "rejected" {
                unresolved_refs.push(frame.id.clone());
                continue;
            }
            let basename = Utf8Path::new(&frame.relative_path)
                .file_name()
                .unwrap_or(&frame.relative_path)
                .to_owned();
            planned.push(PlannedLink {
                inventory_item_id: frame.id.clone(),
                source_root_id: cal_root_id.clone(),
                source_relative_path: frame.relative_path.clone(),
                dest_relative: join_portable(&cal_dir, basename.as_str()),
            });
        }
    }
    Ok(())
}

/// T028: a session that matched *some* but not all of the calibration types
/// seen elsewhere in this project gets the "no calibration applied" warning.
fn detect_partial_calibration_coverage(
    session_calibration_types: &BTreeMap<String, BTreeSet<String>>,
    sessions_without_calibration: &mut Vec<String>,
) {
    let all_project_calibration_types: BTreeSet<String> =
        session_calibration_types.values().flatten().cloned().collect();
    for (session_id, types) in session_calibration_types {
        if !types.is_empty() && types != &all_project_calibration_types {
            sessions_without_calibration.push(session_id.clone());
        }
    }
}

// ── Phase: guards ─────────────────────────────────────────────────────────────

/// FR-009a/FR-017: case-insensitive collision guard.
fn guard_collisions(planned: &[PlannedLink]) -> Result<(), ContractError> {
    let mut seen_dest: BTreeSet<String> = BTreeSet::new();
    for item in planned {
        let key = item.dest_relative.as_str().to_lowercase();
        if !seen_dest.insert(key) {
            return Err(ContractError::new(
                ErrorCode::DestinationCollision,
                format!("two sources resolve to the same destination path: {}", item.dest_relative),
                ErrorSeverity::Blocking,
                false,
            ));
        }
    }
    Ok(())
}

/// FR-016: never silently overwrite a path that already exists.
fn guard_destinations_exist(
    planned: &[PlannedLink],
    destination_root: &Utf8Path,
) -> Result<(), ContractError> {
    for item in planned {
        let abs = join_portable(destination_root, item.dest_relative.as_str());
        if abs.exists() {
            return Err(ContractError::new(
                ErrorCode::DestinationExists,
                format!("destination already exists: {abs}"),
                ErrorSeverity::Blocking,
                false,
            ));
        }
    }
    Ok(())
}

/// T042/FR-018: on Windows, surface long-path warnings.
fn check_long_paths(
    planned: &[PlannedLink],
    destination_root: &Utf8Path,
) -> Option<GenerationWarning> {
    if !cfg!(windows) {
        return None;
    }
    let mut long_paths: BTreeSet<String> = BTreeSet::new();
    for item in planned {
        let abs = join_portable(destination_root, item.dest_relative.as_str());
        if exceeds_windows_long_path_limit(abs.as_str()) {
            long_paths.insert(abs.into_string());
        }
    }
    if long_paths.is_empty() {
        return None;
    }
    Some(GenerationWarning {
        code: GenerationWarningCode::LongPath,
        message: "one or more destination paths exceed the Windows 260-character limit".to_owned(),
        items: long_paths.into_iter().collect(),
    })
}

// ── Phase: link kind resolution ───────────────────────────────────────────────

/// Resolve link kind per item (FR-004/FR-022): capability probed once against
/// the project root.
async fn resolve_link_kinds(
    pool: &SqlitePool,
    planned: &[PlannedLink],
    destination_root: &Utf8Path,
    project_path: &str,
    copy_opt_in: bool,
) -> Result<(BTreeMap<usize, Materialization>, Option<GenerationWarning>), ContractError> {
    let settings = persistence_lifecycle::repositories::settings::load_settings(pool)
        .await
        .map_err(|e| db_internal_ctx(e, "load settings"))?;
    let intra_default = domain_core::source_view::Materialization::from_str_opt(
        &settings.source_view_link_kind_intra_drive,
    )
    .unwrap_or(Materialization::Hardlink);
    let cross_default = domain_core::source_view::Materialization::from_str_opt(
        &settings.source_view_link_kind_cross_drive,
    )
    .unwrap_or(Materialization::Symlink);
    let capability = fs_inventory::capability::probe(Utf8Path::new(project_path));

    let mut drift_notices: BTreeSet<String> = BTreeSet::new();
    let mut resolved_kinds: BTreeMap<usize, Materialization> = BTreeMap::new();

    for (idx, item) in planned.iter().enumerate() {
        let source_root_path = persistence_targets::repositories::inventory::get_library_root_path(
            pool,
            &item.source_root_id,
        )
        .await
        .unwrap_or(None);
        let Some(source_root_path) = source_root_path else {
            return Err(ContractError::new(
                ErrorCode::NoLinkKind,
                format!("source root {} could not be resolved", item.source_root_id),
                ErrorSeverity::Blocking,
                false,
            ));
        };
        let source_abs = Utf8PathBuf::from(source_root_path).join(&item.source_relative_path);
        let scope = fs_inventory::drive_scope::classify(&source_abs, destination_root);

        let resolved = domain_core::source_view::resolve_link_kind(
            scope,
            intra_default,
            cross_default,
            capability,
            copy_opt_in,
        )
        .map_err(|_| {
            ContractError::new(
                ErrorCode::NoLinkKind,
                format!(
                    "no achievable link kind for '{}' (drive-scope {scope:?}); \
                     enable copyOptIn to allow a fallback copy",
                    item.dest_relative
                ),
                ErrorSeverity::Blocking,
                false,
            )
        })?;

        if let Some(requested) = resolved.capability_drift {
            drift_notices.insert(format!(
                "{} (requested {}, applied {})",
                item.dest_relative,
                requested.as_str(),
                resolved.kind.as_str()
            ));
        }
        resolved_kinds.insert(idx, resolved.kind);
    }

    let warning = if drift_notices.is_empty() {
        None
    } else {
        Some(GenerationWarning {
            code: GenerationWarningCode::CapabilityDrift,
            message: "a saved link kind was not achievable and a documented fallback was applied"
                .to_owned(),
            items: drift_notices.into_iter().collect(),
        })
    };

    Ok((resolved_kinds, warning))
}

// ── Phase: plan persistence ───────────────────────────────────────────────────

/// Persist the generation plan (origin `prepared_view_generation`, plan_type
/// `source_view_generation` — FR-021a) and advance to `ready_for_review`.
async fn persist_plan(
    pool: &SqlitePool,
    plan_id: &str,
    project_id: &str,
    planned: &[PlannedLink],
    resolved_kinds: &BTreeMap<usize, Materialization>,
    destination_root: &Utf8Path,
) -> Result<(), ContractError> {
    let title = format!("Generate source view for project {project_id}");
    plans_repo::insert_plan(
        pool,
        &plans_repo::InsertPlan {
            id: plan_id,
            title: &title,
            origin: "prepared_view_generation",
            origin_path: Some(project_id),
            plan_type: "source_view_generation",
            destructive_destination: "archive",
            parent_plan_id: None,
            total_bytes_required: 0,
        },
    )
    .await
    .map_err(|e| db_internal_ctx(e, "insert source view generation plan"))?;

    // Mkdir actions for each distinct destination directory.
    let mut item_index: i64 = 0;
    let mut mkdir_dirs: BTreeSet<Utf8PathBuf> = BTreeSet::new();
    mkdir_dirs.insert(destination_root.to_path_buf());
    for item in planned {
        if let Some(parent) = join_portable(destination_root, item.dest_relative.as_str()).parent()
        {
            mkdir_dirs.insert(parent.to_path_buf());
        }
    }
    for dir in &mkdir_dirs {
        item_index += 1;
        let item_id = new_id();
        plans_repo::insert_plan_item(
            pool,
            &plans_repo::InsertPlanItem {
                id: &item_id,
                plan_id,
                item_index,
                name: dir.as_str(),
                action: "mkdir",
                from_root_id: None,
                from_relative_path: "",
                to_root_id: None,
                to_relative_path: dir.as_str(),
                reason: "view_generation",
                protection: "normal",
                linked_entity: None,
                provenance_json: None,
                archive_path: None,
                source_id: None,
                category: None,
            },
        )
        .await
        .map_err(|e| db_internal_ctx(e, "insert generation mkdir item"))?;
    }

    // Link actions for each planned frame.
    for (idx, item) in planned.iter().enumerate() {
        item_index += 1;
        let item_id = new_id();
        let kind = resolved_kinds.get(&idx).copied().unwrap_or(Materialization::Symlink);
        let dest_abs = join_portable(destination_root, item.dest_relative.as_str());
        let provenance = serde_json::to_string(&serde_json::json!([
            {"label": "materialization", "value": kind.as_str()}
        ]))
        .ok();

        plans_repo::insert_plan_item(
            pool,
            &plans_repo::InsertPlanItem {
                id: &item_id,
                plan_id,
                item_index,
                name: item.dest_relative.as_str(),
                action: "link",
                from_root_id: Some(&item.source_root_id),
                from_relative_path: &item.source_relative_path,
                to_root_id: None,
                to_relative_path: dest_abs.as_str(),
                reason: "view_generation",
                protection: "normal",
                linked_entity: Some(&item.inventory_item_id),
                provenance_json: provenance.as_deref(),
                archive_path: None,
                source_id: None,
                category: None,
            },
        )
        .await
        .map_err(|e| db_internal_ctx(e, "insert generation link item"))?;
    }

    // Advance to ready_for_review.
    plans_repo::update_plan_state(pool, plan_id, "ready_for_review")
        .await
        .map_err(|e| db_internal_ctx(e, "advance generation plan to ready_for_review"))?;

    Ok(())
}
