// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Raw sub-frame candidates (spec 048 US3 T027-T031).

use contracts_core::cleanup::{
    GenerateCleanupPlanResult, RawFrameCleanupCandidate, RawFrameCleanupGenerateRequest,
    RawFrameCleanupScanRequest, RawFrameCleanupScanResponse,
};
use contracts_core::inventory_frame::{
    FramePresenceState, InventoryFrameListRequest, InventoryFrameListScope, RawFrameType,
};
use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use domain_core::ids::new_id;
use persistence_core::repositories::q_core;
use persistence_plans::repositories::source_protection as prot_repo;
use sqlx::SqlitePool;

use crate::errors::db_err;
use crate::frame_inventory;
use crate::protection::{self, CleanupPlanItem, GenerateCleanupPlanRequest};

use super::file_name;

// ── Raw sub-frame candidates (spec 048 US3 T027-T031) ───────────────────────

/// Protection category a raw sub-frame resolves under, keyed by its own
/// `frame_type` (distinct from the artifact `DataType` categories
/// `intermediate`/`masters`/`finals` above).
///
/// Issue #731: this used to be a single fixed pseudo-category
/// (`"raw_frames"`) for every frame regardless of type. Global
/// `protected_categories` (default `["lights","masters","finals"]`) elevates a
/// source to `protected` only when its category is a member of that list —
/// `"raw_frames"` never is, so a light frame could never be category-elevated
/// even when the configured categories say lights should be protected.
/// Mapping to the frame's real type name lets that elevation actually apply.
fn raw_frame_protection_category(frame_type: RawFrameType) -> &'static str {
    match frame_type {
        RawFrameType::Light => "lights",
        RawFrameType::Dark => "darks",
        RawFrameType::Flat => "flats",
        RawFrameType::Bias => "bias",
    }
}

/// Refuse raw-frame cleanup while any session's `frame_ids` is unreadable.
///
/// The `CASE WHEN json_valid(frame_ids)` guards substitute an empty array, so a
/// corrupt session contributes zero frames and its frames resolve to no owning
/// session — indistinguishable from a frame that is genuinely unattributed, and
/// an unattributed frame is a cleanup candidate whose protection resolves under
/// its root instead of its session. Constitution II forbids letting a
/// destructive plan be built on that, and Constitution V makes
/// frame-to-session attribution Tier 1, so the failure is raised rather than
/// degraded (the same choice as
/// `persistence_plans::repositories::projects::find_blockable_missing_sources`).
///
/// The check is deliberately not narrowed to the requested scope: deciding
/// whether a corrupt session owns a selected frame requires reading the
/// membership that is precisely what cannot be read.
///
/// A malformed `aliases` list is logged but does not refuse — equipment naming
/// is not attribution and cannot misdirect a plan item.
async fn refuse_unreadable_frame_attribution(pool: &SqlitePool) -> Result<(), ContractError> {
    let malformed = q_core::list_malformed_json_columns(pool).await.map_err(db_err)?;
    for row in &malformed {
        tracing::warn!(
            table = %row.table_name,
            column = %row.column_name,
            row_id = %row.row_id,
            "stored JSON column is not valid JSON; queries over it read as empty"
        );
    }

    let unreadable: Vec<&str> = malformed
        .iter()
        .filter(|row| row.column_name == q_core::FRAME_IDS_COLUMN)
        .map(|row| row.row_id.as_str())
        .collect();
    if unreadable.is_empty() {
        return Ok(());
    }

    Err(ContractError::new(
        ErrorCode::InternalData,
        format!(
            "frame-to-session attribution is unreadable for {} session(s) ({}), \
             so raw sub-frame cleanup cannot tell an unattributed frame from one \
             of theirs. Repair or re-import those sessions first.",
            unreadable.len(),
            unreadable.join(", ")
        ),
        ErrorSeverity::Blocking,
        false,
    ))
}

/// Pick the protection source id for a raw frame (issue #563): the owning
/// session when a per-session override row exists (future surface), otherwise
/// the frame's root — the only override unit the UI ships today. Resolving
/// under the root also covers the no-override case (plain global
/// inheritance), so no third branch is needed.
async fn frame_protection_source(
    pool: &SqlitePool,
    session_id: Option<&str>,
    root_id: &str,
) -> Result<String, ContractError> {
    if let Some(sid) = session_id {
        if prot_repo::get_source_protection_row(pool, sid).await.map_err(db_err)?.is_some() {
            return Ok(sid.to_owned());
        }
    }
    Ok(root_id.to_owned())
}

/// Pure, read-only raw sub-frame cleanup preview for a root or session
/// (spec 048 US3 T029/T030 — the headline "biggest disk win" this feature
/// exists to unblock, per spec.md).
///
/// Enumerates present, non-protected per-frame inventory entries for the
/// scope via [`frame_inventory::list_frames`] (already excludes `missing`,
/// FR-022), resolves protection per candidate (FR-021), and sums reclaimable
/// bytes over what remains (FR-020). Classification is deterministic (T014's
/// session-kind derivation, not inference), so `confidence` is always `1.0`
/// (FR-023).
///
/// DECISION NOTE: protection is resolved keyed by the frame's owning session
/// id when a per-session override row exists (no shipped surface creates one
/// today; if that surface lands, this is where it takes effect), otherwise by
/// the frame's root id — the ONLY override unit the UI ships (the Data
/// Sources card, keyed by root id). Issue #563: the previous
/// session-id-or-bust keying meant a per-root "Unprotected" override was
/// cosmetic for every session-attributed frame — enforcement resolved under
/// the session id, found no row, and inherited the global default instead.
///
/// # Errors
///
/// Returns `ContractError` on database failure, an invalid/empty scope, or
/// `internal.data` when any session's `frame_ids` is unreadable (see
/// [`refuse_unreadable_frame_attribution`]) — the preview refuses on the same
/// condition as the plan so a frame cannot be absent from one surface and
/// present as a candidate in the other.
pub async fn scan_raw_frames(
    pool: &SqlitePool,
    req: &RawFrameCleanupScanRequest,
) -> Result<RawFrameCleanupScanResponse, ContractError> {
    refuse_unreadable_frame_attribution(pool).await?;

    let listed = frame_inventory::list_frames(
        pool,
        &InventoryFrameListRequest {
            scope: InventoryFrameListScope {
                session_id: req.scope.session_id.clone(),
                root_id: req.scope.root_id.clone(),
            },
            include_missing: Some(false), // FR-022: missing frames are never candidates
        },
    )
    .await?;

    let global = protection::load_global_protection(pool).await?;

    let mut candidates = Vec::new();
    let mut total_reclaimable_bytes: i64 = 0;

    for frame in listed.frames {
        if frame.state == FramePresenceState::Protected {
            continue; // FR-021
        }
        if let Some(kinds) = &req.kinds {
            if !kinds.contains(&frame.frame_type) {
                continue;
            }
        }

        let source_id =
            frame_protection_source(pool, frame.session_id.as_deref(), &frame.root_id).await?;
        let resolved = prot_repo::resolve_protection(
            pool,
            &source_id,
            Some(raw_frame_protection_category(frame.frame_type)),
            &global.level,
            global.block_permanent_delete,
            &global.categories,
        )
        .await
        .map_err(db_err)?;

        total_reclaimable_bytes = total_reclaimable_bytes.saturating_add(frame.size_bytes);

        candidates.push(RawFrameCleanupCandidate {
            frame_id: frame.frame_id,
            session_id: frame.session_id,
            root_id: frame.root_id,
            relative_path: frame.relative_path,
            frame_type: frame.frame_type,
            size_bytes: frame.size_bytes,
            protection: resolved.level,
            confidence: 1.0,
        });
    }

    Ok(RawFrameCleanupScanResponse { candidates, total_reclaimable_bytes })
}

/// Materialise a reviewable cleanup plan for a set of user-selected raw
/// sub-frames (spec 048 US3 T031). Reuses the SAME
/// [`crate::protection::generate_cleanup_plan`] tail as the project-scoped
/// [`super::generate`] above, so it inherits the PR #408 cross-plan overlap
/// guard and the `.astro-plan-archive/<planId>/` destination for free. Every
/// selected item defaults to the `"archive"` action (constitution II prefers
/// archive/trash over permanent delete) — there is no raw-frame equivalent
/// of the artifact `CleanupPolicy` (Keep/Archive/Delete per type) since
/// selection here is always an explicit, per-frame user choice.
///
/// Generating a plan performs NO filesystem mutation (FR-019).
///
/// # Errors
///
/// Returns `ContractError` on database failure, when no selected frame id
/// resolves to a present `file_record` row, or `internal.data` when any
/// session's `frame_ids` is unreadable (see
/// [`refuse_unreadable_frame_attribution`]).
pub async fn generate_raw_frame_plan(
    pool: &SqlitePool,
    req: &RawFrameCleanupGenerateRequest,
) -> Result<GenerateCleanupPlanResult, ContractError> {
    refuse_unreadable_frame_attribution(pool).await?;

    let rows = frame_inventory::rows_by_ids(pool, &req.selected_frame_ids).await?;

    let plan_id = new_id();
    let mut items: Vec<CleanupPlanItem> = Vec::new();
    let mut total_bytes_required: i64 = 0;

    for row in &rows {
        if row.state == "missing" {
            continue; // FR-022, even if a stale selection still names it
        }
        let (session_id, frame_type) =
            frame_inventory::owning_session_frame_type(pool, &row.id).await?;
        let source_id = frame_protection_source(pool, session_id.as_deref(), &row.root_id).await?;
        let size = row.size_bytes.max(0);
        total_bytes_required = total_bytes_required.saturating_add(size);

        items.push(CleanupPlanItem {
            id: format!("{plan_id}-item-{}", items.len()),
            name: file_name(&row.relative_path).to_owned(),
            action: "archive".to_owned(),
            source_id,
            category: raw_frame_protection_category(frame_type).to_owned(),
            from_relative_path: row.relative_path.clone(),
            from_root_id: Some(row.root_id.clone()),
            to_relative_path: String::new(),
        });
    }

    if items.is_empty() {
        return Err(ContractError::new(
            ErrorCode::InternalError,
            "no selected frame resolved to a present file_record row".to_owned(),
            ErrorSeverity::Warning,
            false,
        ));
    }

    let item_count = u32::try_from(items.len()).unwrap_or(u32::MAX);
    let resolved_title = req.title.clone().unwrap_or_else(|| "Raw sub-frame cleanup".to_owned());
    let destination = req.destructive_destination.clone().unwrap_or_else(|| "archive".to_owned());

    let gen_req = GenerateCleanupPlanRequest {
        plan_id: plan_id.clone(),
        title: resolved_title,
        destructive_destination: destination,
        total_bytes_required,
        items,
    };
    let resp = protection::generate_cleanup_plan(pool, &gen_req).await?;

    Ok(GenerateCleanupPlanResult {
        plan_id: resp.plan_id,
        item_count,
        protected_item_count: u32::try_from(resp.protected_item_count).unwrap_or(u32::MAX),
    })
}
