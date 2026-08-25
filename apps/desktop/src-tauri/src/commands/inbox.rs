// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Inbox Tauri commands (spec 005, spec 041).
//!
//! Provides `inbox.classify`, `inbox.confirm`, `inbox.reclassify`,
//! `inbox.scan.folder`, and the spec 041 plan surface commands:
//! `inbox.plan`, `inbox.plan.apply`, `inbox.plan.apply_all`, `inbox.plan.cancel`.
//!
//! Legacy `inbox.scan` is retained for backward compatibility.

use app_core::inbox::attribution::suggest_candidates;
use app_core::inbox::classify::{classify, classify_source_group, ClassifyRequest};
use app_core::inbox::confirm::{confirm, ConfirmRequest};
use app_core::inbox::metadata::get_inbox_item_metadata;
use app_core::inbox::property_registry::property_registry as get_property_registry;
use app_core::inbox::reclassify::{
    reclassify, reclassify_v2, ReclassifyOverride, ReclassifyRequest,
};
use app_core::inbox::scan::{scan_root, ScannedMasterFile};
use app_core::inbox::stats::inbox_stats as inbox_stats_uc;
use app_core::inbox::target_recommendations::{
    target_recommendations as target_recommendations_uc, DEFAULT_FIXED_RADIUS_DEG,
};
use app_core::inbox_plan::{
    apply_all_inbox_plans, apply_inbox_plan, apply_selected_inbox_plans, cancel_inbox_plan,
    get_inbox_plan, list_open_inbox_plans,
};
use app_core::inbox_scan::resolve_scan_options;
use contracts_core::framing::IngestionAttributionCandidateDto;
use contracts_core::inbox::{
    InboxApplyAllResponse, InboxApplySelectedRequest, InboxBreakdownEntry, InboxClassifyRequest,
    InboxClassifyResponse, InboxClassifySourceGroupRequest, InboxClassifySourceGroupResponse,
    InboxConfirmRequest, InboxConfirmResponse, InboxFileEntry, InboxItemMetadataRequest,
    InboxItemMetadataResponse, InboxItemSummary, InboxListItem, InboxListResponse,
    InboxOpenPlansResponse, InboxPlanCancelResponse, InboxPlanView, InboxPropertyRegistryResponse,
    InboxReclassifyRequest, InboxReclassifyResponse, InboxReclassifyV2Request,
    InboxReclassifyV2Response, InboxScanFolderRequest, InboxScanFolderResponse, InboxScanResult,
    InboxSourceGroupListItem, InboxStatsResponse, InboxTargetRecommendationsRequest,
    InboxTargetRecommendationsResponse, ScanDirWarning,
};
use contracts_core::plan_apply::PlanApplyResponse;
use contracts_core::ContractError;
use domain_core::first_run::OrganizationState;
use persistence_inbox::repositories::inbox::{
    grouping_keys_for_items, list_unacknowledged_across_roots, list_unclassified_source_groups,
    upsert_inbox_source_group, UpsertSourceGroup,
};
use persistence_lifecycle::repositories::first_run::get_source_organization_state;
use persistence_targets::repositories::q_desktop::{
    get_inbox_master_item_row, insert_inbox_master_item,
};
use sqlx::SqlitePool;
use std::path::PathBuf;
use uuid::Uuid;

use crate::commands::lifecycle::AppState;

/// Cap on cross-root listing (FR-006 — no unbounded loads).
const INBOX_LIST_LIMIT: i64 = 500;

// ── inbox.classify ────────────────────────────────────────────────────────────

/// `inbox.classify` — classify an Inbox folder using IMAGETYP-only evidence.
/// Idempotent unless `force_rescan: true`. Returns `contentSignature` for use
/// with `inbox.confirm`.
///
/// # Errors
/// `inbox.item.not_found` | `metadata.unreadable`
#[tauri::command]
#[specta::specta]
pub async fn inbox_classify(
    req: InboxClassifyRequest,
    pool: tauri::State<'_, SqlitePool>,
) -> Result<InboxClassifyResponse, ContractError> {
    let use_case_req = ClassifyRequest {
        inbox_item_id: req.inbox_item_id,
        root_absolute_path: PathBuf::from(&req.root_absolute_path),
        force_rescan: req.force_rescan,
    };

    let resp = classify(&pool, use_case_req).await?;

    Ok(InboxClassifyResponse {
        inbox_item_id: resp.inbox_item_id,
        classification_type: resp.classification_type,
        frame_type: resp.frame_type,
        content_signature: resp.content_signature,
        breakdown: resp
            .breakdown
            .into_iter()
            .map(|b| InboxBreakdownEntry {
                kind: b.kind,
                count: u32::try_from(b.count).unwrap_or(u32::MAX),
                destination_preview: b.destination_preview,
                sample_files: b.sample_files,
            })
            .collect(),
        unclassified_files: resp.unclassified_files,
        sample_files: resp.sample_files,
        computed_at: resp.computed_at,
    })
}

// ── inbox.classify.sourceGroup ────────────────────────────────────────────────

/// `inbox.classify.sourceGroup` — classify a scanned folder that has no
/// `inbox_items` row yet, materializing its sub-items directly from the group
/// (spec 058 FR-015/FR-016, T012).
///
/// This is the entry point that makes a source-group row actionable. Without
/// it, removing the scan-time placeholder (T020) leaves every scanned folder
/// permanently unclassifiable: `inbox.classify` is keyed on `inboxItemId` and
/// fails `inbox.item.not_found` without one, and `inbox.reclassify` v2 rebuilds
/// its file records from evidence rows that are only ever written against an
/// item id. No item ⇒ no evidence ⇒ no item.
///
/// Mints nothing confirmable — it returns a count, not a plan and not an id.
/// Confirmation continues to happen only against the materialized item rows,
/// which preserves the source-group row's structural non-confirmability.
///
/// # Errors
/// `inbox.item.not_found` (no such source group) | `metadata.unreadable` (the
/// folder holds no readable FITS/XISF files)
#[tauri::command]
#[specta::specta]
pub async fn inbox_classify_source_group(
    req: InboxClassifySourceGroupRequest,
    pool: tauri::State<'_, SqlitePool>,
) -> Result<InboxClassifySourceGroupResponse, ContractError> {
    let resp = classify_source_group(
        &pool,
        &req.source_group_id,
        std::path::Path::new(&req.root_absolute_path),
    )
    .await?;

    Ok(InboxClassifySourceGroupResponse {
        source_group_id: resp.source_group_id,
        materialized_sub_item_count: u32::try_from(resp.materialized_sub_item_count)
            .unwrap_or(u32::MAX),
    })
}

// ── inbox.confirm ─────────────────────────────────────────────────────────────

/// `inbox.confirm` — generate a reviewable plan from a classified Inbox item.
///
/// # Errors
/// `inbox.item.not_found` | `inbox.has.open.plan` | `classification.ambiguous`
/// | `classification.stale` | `pattern.unset`
#[tauri::command]
#[specta::specta]
pub async fn inbox_confirm(
    req: InboxConfirmRequest,
    state: tauri::State<'_, AppState>,
) -> Result<InboxConfirmResponse, ContractError> {
    let use_case_req = ConfirmRequest {
        inbox_item_id: req.inbox_item_id,
        content_signature: req.content_signature,
        destructive_destination: req.destructive_destination,
        root_absolute_path: PathBuf::from(&req.root_absolute_path),
        // Spec 041 US8/US9: caller-selected destination root (optional).
        root_id: req.root_id,
        // Spec 008 Q27 F-Framing-10 (FR-022) attribution apply-path.
        chosen_attribution: req.chosen_attribution,
    };

    // Spec 041 US8/US9: confirm can block on a destination-root choice
    // (`inbox.destination_root_required` / `inbox.no_destination_root` /
    // `inbox.invalid_destination_root`) or a missing path attribute
    // (`inbox.missing_path_attributes`). The ContractError carries code +
    // details so the UI can branch on the error code.
    let resp = confirm(state.repo.pool(), &state.bus, use_case_req).await?;

    let organization_state = match resp.organization_state {
        contracts_core::first_run::OrganizationState::Organized => "organized",
        contracts_core::first_run::OrganizationState::Unorganized => "unorganized",
    };

    let destinations = resp
        .destinations
        .into_iter()
        .map(|d| contracts_core::inbox::InboxConfirmDestination {
            from_path: d.from_path,
            to_relative_path: d.to_relative_path,
            to_absolute_path: d.to_absolute_path,
            to_root_id: d.to_root_id,
            action: d.action.to_owned(),
        })
        .collect();

    Ok(InboxConfirmResponse {
        plan_id: resp.plan_id,
        plan_state: resp.plan_state,
        items_total: u32::try_from(resp.items_total).unwrap_or(u32::MAX),
        registered_as_master: resp.registered_as_master,
        // spec 041 US4: per-source move-vs-catalogue breakdown.
        actions_summary: Some(contracts_core::inbox::InboxConfirmActionsSummary {
            move_count: u32::try_from(resp.move_count).unwrap_or(u32::MAX),
            catalogue_count: u32::try_from(resp.catalogue_count).unwrap_or(u32::MAX),
        }),
        organization_state: Some(organization_state.to_owned()),
        destinations,
        attribution_candidates: resp.attribution_candidates,
        attribution_applied: resp.attribution_applied,
    })
}

// ── inbox.reclassify ──────────────────────────────────────────────────────────

/// `inbox.reclassify` — write manual frame-type overrides and re-aggregate.
///
/// # Errors
/// Returns `"inbox.item.not_found"`, `"inbox.has.open.plan"`, `"file.not_found"`,
/// or `"internal.database"` — the re-aggregation's writes (classification,
/// needs-review sentinel, breakdown rows) surface a persistence failure rather
/// than returning a response that describes state which was never saved.
#[tauri::command]
#[specta::specta]
pub async fn inbox_reclassify(
    req: InboxReclassifyRequest,
    pool: tauri::State<'_, SqlitePool>,
) -> Result<InboxReclassifyResponse, ContractError> {
    let use_case_req = ReclassifyRequest {
        inbox_item_id: req.inbox_item_id,
        overrides: req
            .overrides
            .into_iter()
            .map(|o| ReclassifyOverride {
                file_path: o.file_path,
                frame_type: o.frame_type.unwrap_or_default(),
                // spec 041 US3/T026 (R-3): carry the non-type overrides through to
                // the use case instead of dropping them.
                filter: o.filter,
                exposure_s: o.exposure_s,
                binning: o.binning,
            })
            .collect(),
    };

    let resp = reclassify(&pool, use_case_req).await?;

    Ok(InboxReclassifyResponse {
        inbox_item_id: resp.inbox_item_id,
        updated_type: resp.updated_type,
        frame_type: resp.frame_type,
        remaining_unclassified: u32::try_from(resp.remaining_unclassified).unwrap_or(u32::MAX),
        applied_count: u32::try_from(resp.applied_count).unwrap_or(u32::MAX),
        // spec 041 — breakdown populated in phase 3+ when use case returns it
        breakdown: vec![],
    })
}

// ── inbox.reclassify v2 (spec 041 T068/T072 — field-agnostic + bulk) ──────────

/// `inbox.reclassify` v2 — field-agnostic property-map + bulk reclassify,
/// scoped to a source group (spec 041 T068/T072 / FR-044/FR-045/FR-049).
///
/// Unlike [`inbox_reclassify`] (fixed `frame_type`/`filter`/`exposure_s`/`binning`,
/// single item), this accepts an open property map validated against
/// `inbox.property_registry`, plus bulk "set all" entries, and re-splits the
/// source group's files into re-materialized single-type sub-items.
///
/// # Errors
/// `inbox.item.not_found` | `inbox.has.open.plan` | `file.not_found` |
/// `inbox.reclassify.unknown_property` | `inbox.reclassify.non_overridable_property`
#[tauri::command]
#[specta::specta]
pub async fn inbox_reclassify_v2(
    req: InboxReclassifyV2Request,
    pool: tauri::State<'_, SqlitePool>,
) -> Result<InboxReclassifyV2Response, ContractError> {
    reclassify_v2(&pool, req).await
}

// ── inbox.item.metadata ───────────────────────────────────────────────────────

/// `inbox.item.metadata` — assemble per-file extracted metadata for an inbox
/// item (spec 041 US2/FR-010).
///
/// # Errors
/// Returns a string error if the item is missing or a query fails.
#[tauri::command]
#[specta::specta]
pub async fn inbox_item_metadata(
    req: InboxItemMetadataRequest,
    pool: tauri::State<'_, SqlitePool>,
) -> Result<InboxItemMetadataResponse, String> {
    let files = get_inbox_item_metadata(&pool, &req.inbox_item_id).await.map_err(|e| e.message)?;
    Ok(InboxItemMetadataResponse { inbox_item_id: req.inbox_item_id, files })
}

// ── inbox.attribution.suggest ─────────────────────────────────────────────────

/// `inbox.attribution.suggest` — ranked framing/project attribution candidates
/// for a light-frame Inbox item (spec 008 US7/FR-019, F-Framing-5).
///
/// Read-only: suggests, never merges (FR-020). The user picks one and the pick
/// travels as `inbox.confirm`'s `chosenAttribution` (FR-022) on a **single**
/// confirm — the candidates must be readable before that confirm, because
/// confirm creates the plan that blocks any second confirm on the item.
///
/// Returns an empty list for non-light items.
///
/// # Errors
/// `internal.database` — a query failed.
#[tauri::command]
#[specta::specta]
pub async fn inbox_attribution_suggest(
    inbox_item_id: String,
    pool: tauri::State<'_, SqlitePool>,
) -> Result<Vec<IngestionAttributionCandidateDto>, ContractError> {
    suggest_candidates(&pool, &inbox_item_id).await
}

// ── inbox.target_recommendations ──────────────────────────────────────────────

/// `inbox.target_recommendations` — recommend canonical targets for a light
/// sub-group by sky-coordinate proximity (spec 041 R-17 / FR-052).
///
/// Ranks catalog targets by great-circle separation from the sub-group's
/// pointing within a FOV-aware (or configurable fixed) radius. The `OBJECT`
/// header is returned only as a display hint, never used for matching. The
/// chosen target is written separately via `inbox.reclassify` (T068).
///
/// Identify the sub-group by `inboxItemId`. The request's legacy
/// `sourceGroupId` is no longer accepted (spec 058 D-002).
///
/// # Errors
/// `inbox.item.not_found` — no resolvable inbox item; `internal.database` — query failed.
#[tauri::command]
#[specta::specta]
pub async fn inbox_target_recommendations(
    req: InboxTargetRecommendationsRequest,
    pool: tauri::State<'_, SqlitePool>,
) -> Result<InboxTargetRecommendationsResponse, ContractError> {
    // Spec 058 D-002: recommendations belong to exactly one inbox item, so a
    // `sourceGroupId`-only request is no longer serviceable.
    let Some(item_id) = req.inbox_item_id else {
        return Err(ContractError::new(
            contracts_core::error_code::ErrorCode::InboxItemNotFound,
            "target_recommendations requires inboxItemId".to_owned(),
            contracts_core::ErrorSeverity::Blocking,
            false,
        ));
    };

    target_recommendations_uc(&pool, &item_id, DEFAULT_FIXED_RADIUS_DEG).await
}

// ── inbox.scan.folder ─────────────────────────────────────────────────────────

/// `inbox.scan.folder` — recursively scan a root directory, discover leaf
/// FITS/video folders, upsert `InboxItem`s, and return a summary list.
///
/// The directory walk and per-file I/O (64 KB signature reads + FITS header
/// parses for master detection) run inside [`tokio::task::spawn_blocking`] so
/// the scan never occupies a tokio async worker. Per-file work is parallelised
/// across a bounded thread pool inside `scan_root` (see `crates/app/inbox/src/scan.rs`).
/// All source-group and master-item upserts are committed in a single
/// transaction to avoid one autocommit (WAL fsync) per leaf folder.
///
/// # Errors
/// Returns a string error if the root is not accessible.
#[tauri::command]
#[specta::specta]
pub async fn inbox_scan_folder(
    req: InboxScanFolderRequest,
    pool: tauri::State<'_, SqlitePool>,
) -> Result<InboxScanFolderResponse, ContractError> {
    let root_path = PathBuf::from(&req.root_absolute_path);
    let opts = resolve_scan_options(&pool).await?;

    // Run the blocking directory walk + per-file I/O off the async executor.
    let scan_out = tokio::task::spawn_blocking(move || scan_root(&root_path, &opts))
        .await
        .map_err(|e| ContractError::internal(e.to_string()))?
        .map_err(ContractError::internal)?;
    let scanned = scan_out.items;
    let scan_warnings: Vec<ScanDirWarning> = scan_out
        .warnings
        .into_iter()
        .map(|w| ScanDirWarning { path: w.path.to_string_lossy().into_owned(), reason: w.reason })
        .collect();

    // Derive the move-vs-catalogue lane for source groups from the root's
    // organization_state (spec 041 R-12, data-model §lane column).
    // organized → "catalogue" (files are already in place; no move needed).
    // unorganized / unknown → "move" (default for inbox sources).
    let org_state = get_source_organization_state(&pool, &req.root_id)
        .await
        .map_err(|e| ContractError::internal(e.to_string()))?;
    let group_lane = match org_state {
        Some(OrganizationState::Organized) => "catalogue",
        _ => "move",
    };

    let mut items: Vec<InboxItemSummary> = Vec::new();

    // Wrap all upserts in a single transaction: one WAL commit instead of
    // one per leaf folder (N autocommits → 1).
    let mut tx = pool.begin().await.map_err(|e| ContractError::internal(e.to_string()))?;

    for scanned_item in &scanned {
        // ── Source-group upsert (T065, R-10/R-12) ────────────────────────────
        //
        // One `inbox_source_groups` row per leaf folder — written at scan time,
        // refreshed on rescan. child_count stays 0 here; classify (T066) sets it.
        // No per-file header reads; reuses the cheap folder content_signature
        // already computed by scan_root (Constitution §I, lazy hashing).
        //
        // Spec 058 T013 stores the sub-frame count on the group because the
        // Inbox list row for a scanned-but-unclassified folder is the group
        // itself, and a 0 marks a masters-only folder as having nothing left
        // to classify (`list_unclassified_source_groups`). The FR-015 carve-out
        // arithmetic lives on the scan record, next to the masters it excludes.
        let sub_count = scanned_item.sub_frame_count();

        let sg_id = Uuid::new_v4().to_string();
        upsert_inbox_source_group(
            &mut *tx,
            &UpsertSourceGroup {
                id: &sg_id,
                root_id: &req.root_id,
                relative_path: &scanned_item.relative_path,
                content_signature: Some(&scanned_item.content_signature),
                format: Some(scanned_item.format.as_str()),
                lane: Some(group_lane),
                file_count: i64::try_from(sub_count).unwrap_or(i64::MAX),
            },
        )
        .await
        .map_err(|e| ContractError::internal(e.to_string()))?;
        // ── A. Individual rows for detected calibration masters ────────────────
        for master in &scanned_item.masters {
            if let Some(summary) =
                persist_master_item(&mut tx, &req.root_id, scanned_item.lane.as_str(), master)
                    .await?
            {
                items.push(summary);
            }
        }

        // ── B. No grouped row ────────────────────────────────────────────────
        //
        // Spec 058 FR-015/T012: scan creates the source group (above) and NO
        // inbox item for the folder's sub-frames. The folder-level placeholder
        // written here previously carried no frame type and no classification,
        // yet it was the row the user selected and confirmed — the false
        // statement this feature exists to delete.
        //
        // The folder is now represented by its `inbox_source_groups` row alone
        // until classification materialises real single-type items. It stays
        // visible in the queue through `list_unclassified_source_groups`, and
        // `inbox.classify.sourceGroup` is what turns it into items.
        //
        // Masters keep their own item rows (block A): the FR-015 carve-out is
        // untouched, because a detected master IS a real, classified,
        // confirmable thing. `sub_count` remains the source group's file count.
    }

    tx.commit().await.map_err(|e| ContractError::internal(e.to_string()))?;

    Ok(InboxScanFolderResponse { root_id: req.root_id, items, scan_warnings })
}

/// Insert (or reuse) the individual `inbox_items` row for a single detected
/// calibration master and return its summary, if the row is present.
///
/// Operates on `tx` so insert and read-back are within the same transaction.
async fn persist_master_item(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    root_id: &str,
    lane: &str,
    master: &ScannedMasterFile,
) -> Result<Option<InboxItemSummary>, ContractError> {
    let master_item_id = Uuid::new_v4().to_string();
    let frame_type_str = format!("{:?}", master.detection.frame_type).to_ascii_lowercase();
    let format_str = master.format.as_str();

    insert_inbox_master_item(
        &mut **tx,
        &master_item_id,
        root_id,
        &master.relative_path,
        lane,
        format_str,
        &frame_type_str,
        master.filter.as_deref(),
        master.exposure_s,
    )
    .await
    .map_err(|e| ContractError::internal(e.to_string()))?;

    // Fetch authoritative row within the same transaction (may have existed
    // from a prior scan, written by INSERT OR IGNORE above).
    let row = get_inbox_master_item_row(&mut **tx, root_id, &master.relative_path)
        .await
        .map_err(|e| ContractError::internal(e.to_string()))?;

    Ok(row.map(|r| InboxItemSummary {
        inbox_item_id: r.id,
        relative_path: master.relative_path.clone(),
        file_count: u32::try_from(r.file_count).unwrap_or(u32::MAX),
        lane: r.lane,
        format: format_str.to_owned(),
        state: r.state,
        content_signature: r.content_signature.unwrap_or_default(),
        is_master: true,
        master_frame_type: r.master_frame_type,
        master_filter: r.master_filter,
        master_exposure_s: r.master_exposure_s,
    }))
}

// ── inbox.list (spec 039) ─────────────────────────────────────────────────────

/// `inbox.list` — return all unacknowledged inbox items across all registered
/// roots (states `pending_classification` and `classified`).
///
/// Each of the two result arrays is capped at 500 rows (FR-006); `capped` is
/// true when either hit its limit, so the UI never silently drops folders.
/// Each item carries its root's absolute path so the UI can group/label by
/// root without a second call.
///
/// # Errors
/// Returns a string error on database failure.
#[tauri::command]
#[specta::specta]
pub async fn inbox_list(
    pool: tauri::State<'_, SqlitePool>,
) -> Result<InboxListResponse, ContractError> {
    let rows = list_unacknowledged_across_roots(&pool, INBOX_LIST_LIMIT)
        .await
        .map_err(|e| ContractError::internal(e.to_string()))?;

    let total = rows.len();

    // Per-item grouping aggregates for the multi-level grouping UI (spec 041).
    // Single GROUP BY pass over the items we're about to return — no N+1.
    let item_ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
    let mut grouping = grouping_keys_for_items(&pool, &item_ids)
        .await
        .map_err(|e| ContractError::internal(e.to_string()))?;

    let items = rows
        .into_iter()
        .map(|r| {
            let g = grouping.remove(&r.id).unwrap_or_default();
            InboxListItem {
                // spec 041 Phase 12 (T072/FR-043): the sub-item's own identity,
                // restated as its "group" id for symmetry with group_key/label.
                group_id: r.id.clone(),
                inbox_item_id: r.id,
                root_id: r.root_id,
                root_absolute_path: r.root_path,
                relative_path: r.relative_path,
                file_count: u32::try_from(r.file_count).unwrap_or(u32::MAX),
                lane: r.lane,
                format: r.format.unwrap_or_else(|| "fits".to_owned()),
                state: r.state,
                content_signature: r.content_signature.unwrap_or_default(),
                is_master: r.is_master != 0,
                master_frame_type: r.master_frame_type,
                master_filter: r.master_filter,
                master_exposure_s: r.master_exposure_s,
                // spec 041 — real org-state from the owning registered source,
                // joined in list_unacknowledged_across_roots (no N+1).
                organization_state: r.organization_state,
                group_target: g.group_target,
                group_frame_type: g.group_frame_type,
                group_date: g.group_date,
                group_filter: g.group_filter,
                group_exposure: g.group_exposure,
                group_instrument: g.group_instrument,
                // T070: per-item rollup populated as empty; the gate is enforced
                // at confirm time (spec 058: `inbox_items.needs_review`) and the
                // per-file detail is surfaced via inbox.item.metadata.
                missing_mandatory: Vec::new(),
                // Spec 058 FR-028 (T008): the list reads the persisted verdict
                // rather than guessing it from `group_key`.
                needs_review: r.needs_review != 0,
                // spec 041 Phase 12 (T072/FR-043): single-type sub-item identity,
                // sourced directly from the inbox_items row (no aggregation).
                source_group_id: r.source_group_id,
                group_key: r.group_key,
                group_label: r.group_label,
                frame_type: r.frame_type,
                classification_result: g.classification_result,
            }
        })
        .collect();

    // Spec 058 FR-016 (T013): folders scan has discovered but classification
    // has not yet split. These rows expose no item id and are therefore not
    // confirmable.
    let source_groups: Vec<InboxSourceGroupListItem> =
        list_unclassified_source_groups(&pool, INBOX_LIST_LIMIT)
            .await
            .map_err(|e| ContractError::internal(e.to_string()))?
            .into_iter()
            .map(|g| InboxSourceGroupListItem {
                source_group_id: g.id,
                root_id: g.root_id,
                root_absolute_path: g.root_path,
                relative_path: g.relative_path,
                file_count: u32::try_from(g.file_count).unwrap_or(u32::MAX),
                format: g.format.unwrap_or_else(|| "fits".to_owned()),
                lane: g.lane.unwrap_or_else(|| "move".to_owned()),
                content_signature: g.content_signature.unwrap_or_default(),
                discovered_at: g.discovered_at,
            })
            .collect();

    let limit = usize::try_from(INBOX_LIST_LIMIT).unwrap_or(usize::MAX);
    let capped = total >= limit || source_groups.len() >= limit;

    Ok(InboxListResponse {
        items,
        source_groups,
        capped,
        limit: u32::try_from(INBOX_LIST_LIMIT).unwrap_or(u32::MAX),
    })
}

// ── Legacy inbox.scan (retained for spec 030 compatibility) ──────────────────

/// `inbox.scan` — legacy stub returning fixture data.
///
/// Kept for backward compat; real scanning uses `inbox.scan.folder`.
///
/// # Errors
/// Never fails; always returns `Ok`.
#[tauri::command]
#[specta::specta]
pub async fn inbox_scan(root_id: Option<String>) -> Result<InboxScanResult, ContractError> {
    let root = root_id.unwrap_or_else(|| "root-inbox-001".to_owned());
    tracing::debug!("stub: inbox.scan root_id={root}");
    Ok(InboxScanResult {
        root_id: root,
        entries: vec![
            InboxFileEntry {
                path: "/astro/inbox/NGC7000_Ha_001.fits".to_owned(),
                file_name: "NGC7000_Ha_001.fits".to_owned(),
                size_bytes: 67_108_864,
                extension: "fits".to_owned(),
            },
            InboxFileEntry {
                path: "/astro/inbox/M31_L_001.fits".to_owned(),
                file_name: "M31_L_001.fits".to_owned(),
                size_bytes: 67_108_864,
                extension: "fits".to_owned(),
            },
            InboxFileEntry {
                path: "/astro/inbox/IC1396_SII_001.xisf".to_owned(),
                file_name: "IC1396_SII_001.xisf".to_owned(),
                size_bytes: 134_217_728,
                extension: "xisf".to_owned(),
            },
        ],
        total_count: 3,
        total_size_bytes: 268_435_456,
    })
}

// ── spec 041: inbox plan surface ──────────────────────────────────────────────

/// `inbox.plan` — fetch the open plan for an inbox item.
///
/// Returns the [`InboxPlanView`] when a plan link exists for this item, or an
/// error with code `inbox.item.no_plan` when the item has no open plan.
///
/// # Errors
/// - `inbox.item.not_found` — item does not exist.
/// - `inbox.item.no_plan`   — item exists but has no linked plan.
/// - `plan.not_found`       — link is present but plan row missing.
#[tauri::command]
#[specta::specta]
pub async fn inbox_plan(
    inbox_item_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<InboxPlanView, String> {
    get_inbox_plan(state.repo.pool(), &inbox_item_id).await.map_err(|e| e.message)
}

/// `inbox.plan.apply` — apply the approved plan for a single inbox item.
///
/// `inbox.confirm` leaves the plan at `ready_for_review`; the caller must record
/// the user's approval with `plans.approve` first. The plan listener transitions
/// the inbox item state once the executor completes.
///
/// # Errors
/// Returns a string error on failure, including `plan.approval_required` when the
/// plan carries no approval and `plan.stale` when per-item CAS detects a file
/// changed since the plan was created.
#[tauri::command]
#[specta::specta]
pub async fn inbox_plan_apply(
    inbox_item_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<PlanApplyResponse, String> {
    apply_inbox_plan(state.repo.pool(), &state.bus, &inbox_item_id).await.map_err(|e| e.message)
}

/// `inbox.plan.apply_all` — apply all plans currently in `plan_open` state.
///
/// Iterates items in `plan_open` state and applies each sequentially.
/// Returns a per-item result list so the UI can report partial failures.
///
/// # Errors
/// Returns a string error only if the list query itself fails; per-plan
/// errors are captured inside `InboxApplyAllResponse.results`.
#[tauri::command]
#[specta::specta]
pub async fn inbox_plan_apply_all(
    state: tauri::State<'_, AppState>,
) -> Result<InboxApplyAllResponse, String> {
    apply_all_inbox_plans(state.repo.pool(), &state.bus).await.map_err(|e| e.message)
}

/// `inbox.plan.list_open` — return every open plan across all roots (spec 041, US2).
///
/// Aggregate surface so the UI can show every active planned action at once,
/// each with its actions, without selecting inbox items one at a time.
///
/// # Errors
/// Returns a string error only if the underlying list/plan queries fail.
#[tauri::command]
#[specta::specta]
pub async fn inbox_plan_list_open(
    state: tauri::State<'_, AppState>,
) -> Result<InboxOpenPlansResponse, String> {
    list_open_inbox_plans(state.repo.pool()).await.map_err(|e| e.message)
}

/// `inbox.plan.apply_selected` — apply a caller-chosen subset of inbox plans
/// (spec 041, US2).
///
/// Selection is plan-level (per inbox item / ingestion group). Returns a
/// per-item result list so the UI can report partial failures; ids that are not
/// in `plan_open` state are reported as per-item errors rather than failing the
/// whole call.
///
/// # Errors
/// Returns a string error only if the membership query itself fails; per-plan
/// errors are captured inside `InboxApplyAllResponse.results`.
#[tauri::command]
#[specta::specta]
pub async fn inbox_plan_apply_selected(
    request: InboxApplySelectedRequest,
    state: tauri::State<'_, AppState>,
) -> Result<InboxApplyAllResponse, String> {
    apply_selected_inbox_plans(state.repo.pool(), &state.bus, &request.inbox_item_ids)
        .await
        .map_err(|e| e.message)
}

/// `inbox.plan.cancel` — discard the open plan and reset the item to `classified`.
///
/// The plan listener handles async cleanup; the use-case also eagerly resets
/// the inbox item state so the UI can reflect the change immediately.
///
/// # Errors
/// Returns a string error on database failure.
#[tauri::command]
#[specta::specta]
pub async fn inbox_plan_cancel(
    inbox_item_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<InboxPlanCancelResponse, String> {
    cancel_inbox_plan(state.repo.pool(), &state.bus, &inbox_item_id).await.map_err(|e| e.message)
}

/// `inbox.stats` — aggregate per-type frame counts across all active inbox items
/// (spec 041, US6 T038).
///
/// Returns counts of folders, masters, and images broken down by effective frame
/// type. The effective type is `manual_override` when set, otherwise
/// `frame_type` from classification evidence.
///
/// # Errors
/// Returns a string error on database failure.
#[tauri::command]
#[specta::specta]
pub async fn inbox_stats(pool: tauri::State<'_, SqlitePool>) -> Result<InboxStatsResponse, String> {
    inbox_stats_uc(&pool).await.map_err(|e| e.message)
}

// ── spec 041: property registry (FR-044) ──────────────────────────────────────

/// `inbox.property_registry` — return the typed property registry.
///
/// The registry lists every per-file property that the field-agnostic
/// reclassifier (spec 041 R-13) understands: its key, value kind, physical
/// unit, source FITS/XISF header(s), whether it is user-overridable, the frame
/// types it applies to, and an optional validation hint.
///
/// The UI uses this registry to render a generic metadata editor without
/// hard-coding field names, so future properties can be added without frontend
/// changes (FR-044).
///
/// # Errors
/// Never fails; always returns `Ok`.
#[tauri::command]
#[specta::specta]
pub async fn inbox_property_registry() -> Result<InboxPropertyRegistryResponse, String> {
    Ok(get_property_registry())
}
