// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `inbox.confirm` use case — the single confirm path (spec 005, T027/T028).
//!
//! Spec 041 FR-050/T071: the legacy "split" action and its mixed per-type
//! confirm branch are removed. `inbox.classify`'s T066 materialization already
//! splits a folder into single-type sub-items before confirm ever sees them,
//! so every confirmable item carries exactly one classification result
//! ("classified") and one chosen destination `rootId`.
//!
//! Creates a reviewable Plan in `ready_for_review` via
//! `persistence_plans::repositories::plans`. File list comes from
//! `InboxClassificationEvidence` rows (not `InboxItem.fileCount` — Ref: A9).
//!
//! TOCTOU guard: verifies `content_signature` before creating the plan (Ref: A8).
//!
//! Destination resolution (spec 041 destination model): each file selects a
//! per-frame-type pattern via `settings::effective_pattern_for(frame_type,
//! is_master)` and resolves it with `patterns::resolve_pattern_str` over a
//! metadata bundle built from extracted `RawFileMetadata` (FITS/XISF headers).
//! A pattern's token set defines that type's path-load-bearing attributes
//! (FR-033); any token that falls back to its default (`missing_tokens`) blocks
//! plan generation with `inbox.missing_path_attributes` (US9). Inbox sources
//! move into a chosen library root (`select_destination_root`, US8); non-inbox
//! sources stay under their own root. A structural failure returns
//! `pattern.unset`.
//!
//! Equipment resolution (#1342): the `camera` token resolves its raw
//! `INSTRUME` string against the registered camera aliases, so one physical
//! camera spelled several ways by different capture programs resolves to one
//! destination directory. Resolution is read-only. Ingest deliberately does
//! **not** call `find_or_create_camera_by_alias`: that function registers one
//! camera per distinct spelling (`name = alias`, `aliases = [alias]`), so
//! running it over a batch would mint sibling cameras for the very spellings
//! the alias array exists to collapse. Growing the registry belongs to an
//! explicit user action over the unclaimed strings, not to a batch write
//! inferred during confirm.
#![allow(clippy::doc_markdown)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use app_core_targets::metadata_cache::cached_extract;
use audit::bus::EventBus;
use audit::event_bus::{InventoryConfirmed, Source, TOPIC_INVENTORY_CONFIRMED};
use contracts_core::first_run::{OrganizationState, SourceKind};
use contracts_core::framing::{
    AttributionAppliedDto, ChosenAttributionDto, IngestionAttributionCandidateDto,
};
use contracts_core::lifecycle::PlanState;
use contracts_core::plans::ProvenanceEntry;
use contracts_core::settings::PatternPart as ContractPatternPart;
use metadata_core::v1_normalization_table;
use patterns::{classify_frame, resolve_pattern_str, FrameTypeClass, MetadataBundle, PatternPart};
use persistence_inbox::repositories::inbox::{self as inbox_repo};
use persistence_lifecycle::repositories::first_run as first_run_repo;
use persistence_lifecycle::repositories::settings as settings_repo;
use persistence_plans::repositories::plans as plans_repo;
use serde_json::json;
use sqlx::SqlitePool;
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

use app_core_errors::db_internal_ctx;
use contracts_core::error_code::ErrorCode;
use contracts_core::{ContractError, ErrorSeverity};

use crate::attribution;

// ── Request / Response ────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct ConfirmRequest {
    pub inbox_item_id: String,
    /// Folder content_signature from the most recent classify response (Ref: A8).
    pub content_signature: String,
    /// Required when plan includes destructive items.
    pub destructive_destination: Option<String>,
    /// Absolute path to the inbox root on disk (needed to resolve file paths
    /// to read FITS/XISF headers for the metadata bundle).
    pub root_absolute_path: PathBuf,
    /// Caller-selected destination library root (spec 041 US8/FR-029). Only
    /// consulted for inbox sources whose frame-type category has >1 candidate
    /// root; ignored otherwise.
    pub root_id: Option<String>,
    /// Attribution apply-path (spec 008 Q27, F-Framing-10, FR-022) — additive.
    /// The user's pick from a prior `attribution_candidates` list. Only
    /// meaningful for light-frame items (`attribution.not_light_frame`
    /// otherwise); omitting it leaves the confirmed session's framing
    /// membership unset.
    pub chosen_attribution: Option<ChosenAttributionDto>,
}

#[derive(Clone, Debug)]
pub struct ConfirmResponse {
    pub plan_id: String,
    pub plan_state: String,
    pub items_total: usize,
    /// Always `false` since spec 041 (US4): masters no longer register at
    /// confirm time. Registration is relocated to plan-apply completion
    /// (`plan_listener`), so confirming a master now produces a reviewable plan
    /// like any other item. The field is retained for DTO compatibility.
    pub registered_as_master: bool,
    /// Organization state of the source owning this inbox item (spec 041 R-7).
    pub organization_state: OrganizationState,
    /// Number of `move` plan items produced (unorganized provenance).
    pub move_count: usize,
    /// Number of `catalogue` plan items produced (organized provenance).
    pub catalogue_count: usize,
    /// Per-action absolute destination previews (spec 041 US8/FR-031).
    pub destinations: Vec<ResolvedDestination>,
    /// Inbox-confirm attribution pass (spec 008 Q27, F-Framing-5, FR-019).
    /// Ranked suggestions for where this item's light session belongs — a
    /// suggestion surface only, never auto-applied. Empty for non-light items
    /// or when no candidate matched.
    pub attribution_candidates: Vec<IngestionAttributionCandidateDto>,
    /// Present when the request carried a `chosen_attribution` that was
    /// successfully applied (F-Framing-10/6).
    pub attribution_applied: Option<AttributionAppliedDto>,
}

/// One resolved per-file destination (spec 041 US8/FR-031). The absolute path
/// is `root_path + "/" + to_relative_path`, computed for display only — the
/// plan_items table stores `to_root_id` + `to_relative_path`, not the absolute.
#[derive(Clone, Debug)]
pub struct ResolvedDestination {
    pub from_path: String,
    pub to_relative_path: String,
    pub to_absolute_path: String,
    pub to_root_id: String,
    pub action: &'static str,
}

// ── confirm ───────────────────────────────────────────────────────────────────

/// Generate a reviewable plan for an inbox item.
///
/// # Errors
///
/// - `inbox.item.not_found` — item does not exist or has no classification.
/// - `inbox.has.open.plan` — an open plan already exists.
/// - `classification.ambiguous` — action/classification mismatch or no classified files.
/// - `classification.stale` — signature drift detected.
/// - `pattern.unset` — naming pattern is unset or fails to resolve required tokens.
/// - `inbox.destination_collision` — two files resolve onto one destination path.
/// - `attribution.not_light_frame` — `chosen_attribution` was supplied for a
///   non-light item (spec 008 Q27 F-Framing-10).
/// - `attribution.geometry_unavailable` — `chosen_attribution` requires
///   creating a new framing, but this item has no staged pointing/rotation.
#[allow(clippy::too_many_lines)]
pub async fn confirm(
    pool: &SqlitePool,
    bus: &EventBus,
    req: ConfirmRequest,
) -> Result<ConfirmResponse, ContractError> {
    // 1. Load item
    let item = inbox_repo::get_inbox_item(pool, &req.inbox_item_id).await.map_err(|_| {
        ContractError::new(
            ErrorCode::InboxItemNotFound,
            format!("InboxItem not found: {}", req.inbox_item_id),
            ErrorSeverity::Blocking,
            false,
        )
    })?;

    // Master metadata carry-to-apply (spec 041 US4/T031): the calibration
    // master fields (`is_master_item`, `master_frame_type`, `master_exposure_s`,
    // `master_filter`) already live on the `inbox_items` row. We deliberately do
    // NOT stamp them onto the plan or plan items here. At apply completion the
    // plan listener reloads the inbox item via the `inbox_plan_links` row and
    // reads these fields directly to register the master (calibration_session +
    // calibration_fingerprint). This is the lowest-risk mechanism — no new
    // columns, no plan-item provenance encoding — and keeps masters on the exact
    // same move/catalogue plan path as every other item (Constitution §II).

    // 2–7. Pre-flight guards: no open plan, not needs-review, classification
    //      exists and is "classified", content signature fresh.
    let classification =
        check_confirm_preflight(pool, &req.inbox_item_id, &req.content_signature, &item).await?;

    // 9. Enumerate files from evidence (Ref: A9) — NOT from file_count
    let evidence_rows = inbox_repo::list_evidence(pool, &req.inbox_item_id)
        .await
        .map_err(|e| db_internal_ctx(e, "list inbox evidence"))?;

    // Only include files that have a frame type (classified or manually overridden)
    let plan_files: Vec<&persistence_inbox::repositories::inbox::InboxEvidenceRow> =
        evidence_rows.iter().filter(|ev| effective_frame_type(ev).is_some()).collect();

    if plan_files.is_empty() {
        return Err(ContractError::new(
            ErrorCode::ClassificationAmbiguous,
            "No classified files found. Re-classify or reclassify unclassified files.",
            ErrorSeverity::Blocking,
            false,
        ));
    }

    // 9b. Attribution pass (spec 008 Q27, F-Framing-5, FR-019) — the first
    // pre-ingest pass at the confirm gate.
    let is_light_item = evidence_is_light(&evidence_rows);

    if req.chosen_attribution.is_some() && !is_light_item {
        return Err(ContractError::new(
            ErrorCode::AttributionNotLightFrame,
            "chosen_attribution only applies to light-frame items.",
            ErrorSeverity::Blocking,
            false,
        ));
    }

    // `item_geometry` is consumed by the apply-path below once the plan exists.
    let (item_geometry, attribution_candidates) =
        compute_attribution_candidates(pool, &req.inbox_item_id, is_light_item).await;

    // Spec 041 FR-050/T071: per-type action grouping is retired along with the
    // "split" action — a confirmable item is single-type (classification.result
    // == "classified"), so no frame-type sort is needed. `list_evidence` already
    // orders rows by `relative_file_path`, which is deterministic order enough.

    // 8a. Look up the owning source's organization state (spec 041 US4, R-7).
    //
    // `Organized`   → catalogue-in-place (record where the file already is; no
    //                 filesystem move; from_path == to_path == current path).
    // `Unorganized` → move to the pattern-resolved destination.
    //
    // An inbox item shares one root, so this is uniform in practice; the loop
    // below still decides per file so it composes with future mixed-provenance
    // cases (R-8) without special-casing.
    let org_state = persistence_lifecycle::repositories::first_run::get_source_organization_state(
        pool,
        &item.root_id,
    )
    .await
    .map_err(|e| db_internal_ctx(e, "get source organization state"))?
    .unwrap_or(OrganizationState::Unorganized);

    // 8b. Destination-root resolution (spec 041 US8/FR-027–FR-031).
    //
    // Load the full source list once; both the inbox-source check and the
    // root-path map for FR-031 absolute previews draw from it.
    let sources = first_run_repo::list_sources(pool).await.map_err(|e| {
        ContractError::new(ErrorCode::InternalDatabase, e.to_string(), ErrorSeverity::Fatal, true)
    })?;
    let source_kind = sources.iter().find(|s| s.source_id == item.root_id).map(|s| s.kind);
    let item_is_inbox_source = source_kind == Some(SourceKind::Inbox);

    // 8c. Resolve destination paths for each file via its per-type pattern
    // (spec 041 T052/FR-026a). Collect per-file resolution rows. `action` is
    // "catalogue" for organized provenance, "move" otherwise.
    //
    // `resolve_pattern_str` returns `Ok(ResolveResult)` whose `missing_tokens`
    // lists every token that fell back to a registry default. Because a per-type
    // pattern's token set defines that type's path-load-bearing attributes
    // (FR-033), a non-empty `missing_tokens` IS the missing-attribute set — the
    // US9 gate (T056) is derived from it rather than a separate matrix. A hard
    // `Err(ResolveError)` signals a structural failure (traversal, length cap).
    let norm_table = v1_normalization_table();
    // Loaded once per confirm, not per file: the whole registry is small and
    // the resolve below runs for every plan file. Equipment being unreadable
    // degrades to raw header strings rather than failing the confirm.
    let cameras = app_core_calibration::equipment::list_cameras(pool).await.unwrap_or_default();

    let ctx = ResolveFilesCtx {
        root_absolute_path: &req.root_absolute_path,
        norm_table: &norm_table,
        cameras: &cameras,
        org_state,
        item_root_id: &item.root_id,
        item_is_inbox_source,
        selected_root_id: req.root_id.as_deref(),
    };
    let (resolved_items, missing_by_file) =
        resolve_per_file_plan_items(pool, &plan_files, &ctx).await?;

    // 8d. US9 gate: reject if any file is missing a path-load-bearing attribute.
    reject_if_missing_path_attrs(&missing_by_file)?;

    // 8e. Self-collision gate: refuse before any plan row exists.
    reject_if_destinations_collide(&resolved_items)?;

    // 10. Build the plan.
    // A move-only split is non-destructive from the user perspective but the
    // plans table CHECK constraint only accepts the canonical 'archive' | 'trash'
    // vocabulary (spec 033, migration 0040). Anything else (incl. the legacy
    // 'os_trash' / 'none') falls back to 'archive' so confirm can never schedule
    // a permanent delete without a recoverable step.
    let destructive_dest = req
        .destructive_destination
        .as_deref()
        .filter(|s| matches!(*s, "archive" | "trash"))
        .unwrap_or("archive");

    let plan_id = Uuid::new_v4().to_string();
    let title = format!("Inbox confirm: {} ({})", item.relative_path, classification.result);

    let insert_plan = plans_repo::InsertPlan {
        id: &plan_id,
        title: &title,
        origin: "inbox",
        origin_path: Some(&item.relative_path),
        // "split" is the stable `plans.plan_type` CHECK-constrained category for
        // every inbox-confirm-origin plan (see `crates/app/core/src/plans.rs`
        // `parse_plan_type` and `plan_listener.rs`); it predates and is
        // unrelated to the removed per-request "split" action, so FR-050/T071
        // does not touch it.
        plan_type: "split",
        destructive_destination: destructive_dest,
        parent_plan_id: None,
        total_bytes_required: 0,
    };

    plans_repo::insert_plan(pool, &insert_plan)
        .await
        .map_err(|e| db_internal_ctx(e, "insert plan"))?;

    // Map destination root id → absolute path for the FR-031 absolute preview.
    // (Catalogue actions and non-inbox moves keep their own root; inbox moves
    // use the chosen library root resolved above.)
    let root_paths: std::collections::HashMap<String, String> =
        sources.into_iter().map(|s| (s.source_id, s.path)).collect();

    // 11. Insert plan items — one per classified file, with resolved
    // destinations and per-item destination root (spec 041 US8/FR-027–FR-031).
    let items_total = resolved_items.len();
    let (move_count, catalogue_count, destinations) =
        insert_plan_items_batch(pool, &plan_id, &item.root_id, &resolved_items, &root_paths)
            .await?;

    // 12. Attribution apply-path (spec 008 Q27, F-Framing-10/6, FR-022): the
    // plan row and its items now exist, so the pick can be persisted
    // (`plans.chosen_framing_id`) for `ingest_sessions` to bind once the real
    // session is created. Creates the framing/project the kind requires and
    // honors the F-Framing-6 completed-project reopen.
    //
    // Ordered before the three writes below, not after: frame-to-framing
    // attribution is a Tier-1 record (Constitution V) that the filesystem cannot
    // re-derive, and those writes are what leave the plan ready to apply. Running them
    // first left a `ready_for_review` plan on a failed attribution, so the frames
    // could move with the user's decision missing. The plan stays in `draft` on
    // failure, which `plans.approve` refuses.
    let attribution_applied = match (&item_geometry, &req.chosen_attribution) {
        (Some(geometry), Some(chosen)) => {
            attribution::apply_chosen_attribution(pool, bus, &plan_id, geometry, chosen).await?
        }
        _ => None,
    };

    // 13. Transition plan to ready_for_review
    plans_repo::update_plan_state(pool, &plan_id, PlanState::ReadyForReview.as_str())
        .await
        .map_err(|e| db_internal_ctx(e, "transition plan to ready_for_review"))?;

    // 14. Create plan link and update item state
    inbox_repo::insert_plan_link(pool, &req.inbox_item_id, &plan_id)
        .await
        .map_err(|e| db_internal_ctx(e, "insert plan link"))?;

    // Load-bearing (#1101): the plan and its link were just committed above, so
    // an item left off `plan_open` contradicts them — same class as the two
    // propagating writes it follows.
    inbox_repo::update_inbox_item_state(pool, &req.inbox_item_id, "plan_open")
        .await
        .map_err(|e| db_internal_ctx(e, "mark inbox item plan_open"))?;

    // 15. Publish `inventory.confirmed` (best-effort; durable writes above already
    //     succeeded — a bus failure must not surface as Fatal).
    publish_inventory_confirmed(bus, &req.inbox_item_id, &plan_id).await;

    Ok(ConfirmResponse {
        plan_id,
        plan_state: PlanState::ReadyForReview.as_str().to_owned(),
        items_total,
        // spec 041 US4: masters no longer register at confirm; registration is
        // relocated to plan-apply completion. Always false now.
        registered_as_master: false,
        organization_state: org_state,
        move_count,
        catalogue_count,
        destinations,
        attribution_candidates,
        attribution_applied,
    })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Context threaded through [`resolve_per_file_plan_items`] to avoid a
/// >7-argument signature.
struct ResolveFilesCtx<'a> {
    root_absolute_path: &'a PathBuf,
    norm_table: &'a metadata_core::ImageTypNormalizationTable,
    cameras: &'a [contracts_core::equipment::Camera],
    org_state: contracts_core::first_run::OrganizationState,
    item_root_id: &'a str,
    item_is_inbox_source: bool,
    selected_root_id: Option<&'a str>,
}

/// Resolve per-file destination rows from evidence, returning
/// `(resolved_items, missing_by_file)`.
///
/// For organized sources: catalogue-in-place (dest == source). For
/// unorganized: pattern-resolve the destination; collect files whose tokens
/// are missing into `missing_by_file` for the US9 gate.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
async fn resolve_per_file_plan_items(
    pool: &SqlitePool,
    plan_files: &[&persistence_inbox::repositories::inbox::InboxEvidenceRow],
    ctx: &ResolveFilesCtx<'_>,
) -> Result<(Vec<ResolvedRow>, Vec<(String, Vec<String>)>), ContractError> {
    use contracts_core::first_run::OrganizationState;

    let mut resolved_items: Vec<ResolvedRow> = Vec::with_capacity(plan_files.len());
    let mut missing_by_file: Vec<(String, Vec<String>)> = Vec::new();
    // Cache the chosen destination root per category.
    let mut chosen_root_cache: std::collections::HashMap<&'static str, DestinationRoot> =
        std::collections::HashMap::new();
    // Cache the resolved per-type destination pattern by (frame_type, is_master).
    let mut pattern_cache: std::collections::HashMap<(String, bool), Option<String>> =
        std::collections::HashMap::new();

    for ev in plan_files {
        let ft = effective_frame_type(ev).unwrap_or("unknown");
        let is_master = ev.is_master != 0;
        let abs_path = ctx.root_absolute_path.join(&ev.relative_file_path);
        let filename = abs_path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown.fits");
        let basename = ev.relative_file_path.rsplit('/').next().unwrap_or(&ev.relative_file_path);
        let item_name = format!("[{}] {basename}", ft.to_uppercase());

        // Built once per file for both branches so a catalogued item carries the
        // same frozen context as a moved one.
        let bundle = build_metadata_bundle(&abs_path, ft, ctx.norm_table, ctx.cameras);

        match ctx.org_state {
            OrganizationState::Organized => {
                // Catalogue-in-place: dest == source; stays under its own root.
                resolved_items.push(ResolvedRow {
                    source_rel: ev.relative_file_path.clone(),
                    dest_rel: ev.relative_file_path.clone(),
                    item_name,
                    action: "catalogue",
                    to_root_id: ctx.item_root_id.to_owned(),
                    provenance: freeze_confirm_provenance(&bundle, is_master, None),
                    frame_type: Some(ft.to_owned()),
                });
            }
            OrganizationState::Unorganized => {
                // Select the per-type pattern. `None` → image type is missing.
                let cache_key = (ft.to_owned(), is_master);
                let pattern = if let Some(cached) = pattern_cache.get(&cache_key) {
                    cached.clone()
                } else {
                    let fetched = settings_repo::effective_pattern_for(pool, ft, is_master)
                        .await
                        .map_err(|e| {
                        ContractError::new(
                            ErrorCode::InternalDatabase,
                            e.to_string(),
                            ErrorSeverity::Fatal,
                            true,
                        )
                    })?;
                    pattern_cache.insert(cache_key, fetched.clone());
                    fetched
                };
                let Some(pattern) = pattern else {
                    // Unclassified frame: image type is the missing attribute.
                    missing_by_file
                        .push((ev.relative_file_path.clone(), vec!["image type".to_owned()]));
                    continue;
                };

                let result = match resolve_pattern_str(&pattern, &bundle) {
                    Ok(r) => r,
                    Err(e) => {
                        return Err(ContractError::new(
                            ErrorCode::PatternUnset,
                            format!(
                                "Pattern resolution failed for '{}': {e:?}",
                                ev.relative_file_path
                            ),
                            ErrorSeverity::Blocking,
                            false,
                        ));
                    }
                };

                // US9 gate (FR-032/FR-033): missing tokens = missing attributes.
                if !result.missing_tokens.is_empty() {
                    missing_by_file
                        .push((ev.relative_file_path.clone(), result.missing_tokens.clone()));
                    continue;
                }

                // Destination root: inbox sources move into a chosen library
                // root; non-inbox sources move within their own root.
                let dest_root = if ctx.item_is_inbox_source {
                    let class = classify_frame(ft, is_master).ok_or_else(|| {
                        ContractError::new(
                            ErrorCode::InboxNoDestinationRoot,
                            format!("Cannot route '{ft}': unknown frame-type category"),
                            ErrorSeverity::Blocking,
                            false,
                        )
                    })?;
                    if let Some(cached) = chosen_root_cache.get(class.as_str()) {
                        cached.clone()
                    } else {
                        let chosen =
                            select_destination_root(pool, class, ctx.selected_root_id).await?;
                        chosen_root_cache.insert(class.as_str(), chosen.clone());
                        chosen
                    }
                } else {
                    DestinationRoot { root_id: ctx.item_root_id.to_owned(), path: String::new() }
                };

                let dest_with_file = format!("{}/{filename}", result.relative_path);
                resolved_items.push(ResolvedRow {
                    source_rel: ev.relative_file_path.clone(),
                    dest_rel: dest_with_file,
                    item_name,
                    action: "move",
                    to_root_id: dest_root.root_id,
                    provenance: freeze_confirm_provenance(&bundle, is_master, Some(&pattern)),
                    frame_type: Some(ft.to_owned()),
                });
            }
        }
    }
    Ok((resolved_items, missing_by_file))
}

/// Insert one plan item per resolved row; return `(move_count, catalogue_count,
/// destinations)` (spec 041 US8/FR-027–FR-031, step 11).
async fn insert_plan_items_batch(
    pool: &SqlitePool,
    plan_id: &str,
    from_root_id: &str,
    resolved_items: &[ResolvedRow],
    root_paths: &std::collections::HashMap<String, String>,
) -> Result<(usize, usize, Vec<ResolvedDestination>), ContractError> {
    let mut move_count = 0usize;
    let mut catalogue_count = 0usize;
    let mut destinations: Vec<ResolvedDestination> = Vec::with_capacity(resolved_items.len());

    for (idx, row) in resolved_items.iter().enumerate() {
        let item_id = Uuid::new_v4().to_string();

        match row.action {
            "catalogue" => catalogue_count += 1,
            _ => move_count += 1,
        }

        let plan_item = plans_repo::InsertPlanItem {
            id: &item_id,
            plan_id,
            item_index: i64::try_from(idx).unwrap_or(i64::MAX),
            name: &row.item_name,
            action: row.action,
            from_root_id: Some(from_root_id),
            from_relative_path: &row.source_rel,
            to_root_id: Some(&row.to_root_id),
            to_relative_path: &row.dest_rel,
            reason: "inbox_confirm",
            protection: "normal",
            linked_entity: None,
            provenance_json: row.provenance.as_deref(),
            archive_path: None,
            source_id: None,
            // Store frame type in category so InboxPlanAction::frame_type is populated.
            category: row.frame_type.as_deref(),
        };

        plans_repo::insert_plan_item(pool, &plan_item)
            .await
            .map_err(|e| db_internal_ctx(e, "insert plan item"))?;

        // FR-031: absolute destination = root path + "/" + relative path.
        let to_absolute_path = root_paths.get(&row.to_root_id).map_or_else(
            || row.dest_rel.clone(),
            |root| format!("{}/{}", root.trim_end_matches('/'), row.dest_rel),
        );
        destinations.push(ResolvedDestination {
            from_path: row.source_rel.clone(),
            to_relative_path: row.dest_rel.clone(),
            to_absolute_path,
            to_root_id: row.to_root_id.clone(),
            action: row.action,
        });
    }
    Ok((move_count, catalogue_count, destinations))
}

/// One resolved per-file plan row before insertion. Carries the per-item
/// destination root (spec 041 US8) so inbox moves can target a chosen library
/// root while non-inbox files stay under their own root.
struct ResolvedRow {
    source_rel: String,
    dest_rel: String,
    item_name: String,
    action: &'static str,
    to_root_id: String,
    /// Frozen inferred context at approval time (`freeze_confirm_provenance`).
    provenance: Option<String>,
    /// Frame-type class string (e.g. `"light"`, `"dark_flat"`) stored in the
    /// `category` column so `InboxPlanAction::frame_type` can be populated
    /// without a new column.
    frame_type: Option<String>,
}

/// A chosen destination library root (id + absolute path) for inbox moves.
#[derive(Clone)]
struct DestinationRoot {
    root_id: String,
    /// Absolute path of the root; empty when the root is the item's own root
    /// (non-inbox), in which case the absolute preview is filled from the
    /// `root_paths` map at insert time.
    #[allow(dead_code)]
    path: String,
}

/// Run all pre-flight guard checks (steps 2–7 of `confirm`) and return the
/// classification row. Fails fast on the first violated guard.
async fn check_confirm_preflight(
    pool: &SqlitePool,
    inbox_item_id: &str,
    content_signature: &str,
    item: &persistence_inbox::repositories::inbox::InboxItemRow,
) -> Result<persistence_inbox::repositories::inbox::InboxClassificationRow, ContractError> {
    // 2. Dedupe open plan (Ref: E1)
    if let Some(link) = inbox_repo::get_plan_link(pool, inbox_item_id).await.unwrap_or(None) {
        return Err(ContractError::new(
            ErrorCode::InboxHasOpenPlan,
            format!("Inbox item already has an open plan: {}", link.plan_id),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    // 3. Reject needs-review items (T070/FR-049/SC-015/spec 058 FR-028).
    if item.needs_review != 0 {
        return Err(ContractError::new(
            ErrorCode::InboxMissingPathAttributes,
            "This item is in the needs-review bucket: one or more files are missing mandatory \
             attributes. Supply the missing values via inbox.reclassify before confirming.",
            ErrorSeverity::Blocking,
            false,
        ));
    }

    // 4. Load classification.
    let classification = inbox_repo::get_classification(pool, inbox_item_id)
        .await
        .unwrap_or(None)
        .ok_or_else(|| {
            ContractError::new(
                ErrorCode::InboxItemNotFound,
                "Classification not found — run inbox.classify first",
                ErrorSeverity::Blocking,
                false,
            )
        })?;

    // 5. TOCTOU content_signature guard (Ref: A8).
    if item.content_signature.as_deref() != Some(content_signature) {
        return Err(ContractError::new(
            ErrorCode::ClassificationStale,
            "Folder has changed since classification. Re-classify before confirming.",
            ErrorSeverity::Blocking,
            false,
        ));
    }

    // 7. Spec 041 FR-050/T071/T072: only "classified" result is confirmable.
    if classification.result != "classified" {
        return Err(ContractError::new(
            ErrorCode::ClassificationAmbiguous,
            format!("Classification result '{}' is not confirmable", classification.result),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    Ok(classification)
}

/// Fail with `InboxMissingPathAttributes` if any file is missing path-load-
/// bearing attributes (US9 gate, FR-032/FR-033).
fn reject_if_missing_path_attrs(
    missing_by_file: &[(String, Vec<String>)],
) -> Result<(), ContractError> {
    if missing_by_file.is_empty() {
        return Ok(());
    }
    let files: Vec<serde_json::Value> = missing_by_file
        .iter()
        .map(|(path, attrs)| json!({ "filePath": path, "missingPathAttributes": attrs }))
        .collect();
    let summary = missing_by_file
        .iter()
        .map(|(p, a)| format!("{p}: {}", a.join(", ")))
        .collect::<Vec<_>>()
        .join("; ");
    Err(ContractError::new(
        ErrorCode::InboxMissingPathAttributes,
        format!("Files are missing attributes their destination pattern requires: {summary}"),
        ErrorSeverity::Blocking,
        false,
    )
    .with_details(json!({ "files": files })))
}

/// Case-folded, NFC-normalized destination path, used as the collision key.
///
/// APFS and NTFS resolve two paths differing only in case to one file, and APFS
/// also compares NFC and NFD spellings as equal, so a byte comparison would pass
/// rows that name one file on the volumes this app targets.
fn destination_key(dest_rel: &str) -> String {
    dest_rel.nfc().collect::<String>().to_lowercase()
}

/// Fail with `InboxDestinationCollision` if two resolved rows claim one
/// destination root and [`destination_key`].
///
/// A per-item overwrite check at apply time cannot catch a same-plan collision:
/// nothing pre-existing is in the way when the first item runs. The executor
/// then refuses the second item (`fs_executor::ops::move_op`,
/// `conflict.destination_exists`), which leaves the item's frames half-moved
/// with the plan failed partway. Constitution II puts that decision in the
/// reviewable plan instead, so confirm refuses to build one.
fn reject_if_destinations_collide(resolved: &[ResolvedRow]) -> Result<(), ContractError> {
    let mut by_dest: BTreeMap<(&str, String), Vec<&ResolvedRow>> = BTreeMap::new();
    for row in resolved {
        by_dest
            .entry((row.to_root_id.as_str(), destination_key(&row.dest_rel)))
            .or_default()
            .push(row);
    }
    let collisions: Vec<((&str, String), Vec<&ResolvedRow>)> =
        by_dest.into_iter().filter(|(_, rows)| rows.len() > 1).collect();
    if collisions.is_empty() {
        return Ok(());
    }

    let details: Vec<serde_json::Value> = collisions
        .iter()
        .map(|((root_id, _), rows)| {
            let items: Vec<serde_json::Value> = rows
                .iter()
                .map(|r| json!({ "sourcePath": r.source_rel, "toRelativePath": r.dest_rel }))
                .collect();
            json!({ "toRootId": root_id, "items": items })
        })
        .collect();
    let summary = collisions
        .iter()
        .map(|(_, rows)| {
            rows.iter()
                .map(|r| format!("{} -> {}", r.source_rel, r.dest_rel))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .collect::<Vec<_>>()
        .join("; ");
    Err(ContractError::new(
        ErrorCode::InboxDestinationCollision,
        format!("These files claim one destination path, so the plan cannot be applied: {summary}"),
        ErrorSeverity::Blocking,
        false,
    )
    .with_details(json!({ "collisions": details })))
}

/// Publish `inventory.confirmed` to the event bus (spec 056).
///
/// Best-effort: the plan is already durably committed before this call, so a
/// transient bus failure must not surface as a Fatal error for a confirm that
/// already landed.
async fn publish_inventory_confirmed(bus: &EventBus, inbox_item_id: &str, plan_id: &str) {
    let confirmed_at = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned());
    if let Err(e) = bus
        .publish(
            TOPIC_INVENTORY_CONFIRMED,
            Source::User,
            InventoryConfirmed {
                inbox_item_id: inbox_item_id.to_owned(),
                plan_id: plan_id.to_owned(),
                at: confirmed_at,
            },
        )
        .await
    {
        tracing::warn!(
            inbox_item_id = %inbox_item_id,
            plan_id = %plan_id,
            error = %e,
            "audit bus publish failed for inventory.confirmed"
        );
    }
}

/// Compute item geometry and attribution candidates for a light-frame confirm.
///
/// Returns `(item_geometry, attribution_candidates)`.  For non-light items both
/// are empty/None.  Attribution is a suggestion surface (FR-019/FR-020): any
/// transient failure degrades to "no candidates" rather than aborting confirm.
async fn compute_attribution_candidates(
    pool: &SqlitePool,
    inbox_item_id: &str,
    is_light_item: bool,
) -> (Option<attribution::ItemGeometry>, Vec<IngestionAttributionCandidateDto>) {
    if !is_light_item {
        return (None, Vec::new());
    }
    let item_geometry = match attribution::compute_item_geometry(pool, inbox_item_id).await {
        Ok(geometry) => Some(geometry),
        Err(e) => {
            tracing::warn!(
                inbox_item_id = %inbox_item_id,
                "confirm: attribution geometry computation failed, degrading to no \
                 candidates: {e:?}"
            );
            Some(attribution::ItemGeometry::default())
        }
    };
    let candidates = match &item_geometry {
        Some(geometry) => {
            attribution::compute_candidates(pool, geometry).await.unwrap_or_else(|e| {
                tracing::warn!(
                    inbox_item_id = %inbox_item_id,
                    "confirm: attribution candidate ranking failed, degrading to no candidates: \
                     {e:?}"
                );
                Vec::new()
            })
        }
        None => Vec::new(),
    };
    (item_geometry, candidates)
}

/// Map a frame-type class to the source kind that can host it (spec 041 FR-027).
/// Lights live under `LightFrames`; every calibration class (raw or master)
/// lives under `Calibration`.
fn destination_kind_for(class: FrameTypeClass) -> SourceKind {
    match class {
        FrameTypeClass::Light => SourceKind::LightFrames,
        FrameTypeClass::Flat
        | FrameTypeClass::Dark
        | FrameTypeClass::Bias
        | FrameTypeClass::MasterFlat
        | FrameTypeClass::MasterDark
        | FrameTypeClass::MasterBias => SourceKind::Calibration,
    }
}

/// Select the destination library root for an inbox item's frame-type category
/// (spec 041 US8/FR-027–FR-030).
///
/// Candidates are registered, non-inbox sources whose kind matches the
/// category. Resolution:
/// - 0 candidates → `inbox.no_destination_root`.
/// - 1 candidate → auto-select (caller `root_id` ignored).
/// - 2+ candidates → require `selected`; it MUST be one of the candidates, else
///   `inbox.destination_root_required` (absent) / `inbox.invalid_destination_root`.
async fn select_destination_root(
    pool: &SqlitePool,
    class: FrameTypeClass,
    selected: Option<&str>,
) -> Result<DestinationRoot, ContractError> {
    let want_kind = destination_kind_for(class);
    let candidates: Vec<DestinationRoot> = first_run_repo::list_sources(pool)
        .await
        .map_err(|e| {
            ContractError::new(
                ErrorCode::InternalDatabase,
                e.to_string(),
                ErrorSeverity::Fatal,
                true,
            )
        })?
        .into_iter()
        .filter(|s| s.kind != SourceKind::Inbox && s.kind == want_kind)
        .map(|s| DestinationRoot { root_id: s.source_id, path: s.path })
        .collect();

    match candidates.len() {
        0 => Err(ContractError::new(
            ErrorCode::InboxNoDestinationRoot,
            format!("No registered library root for frame-type category '{}'", class.as_str()),
            ErrorSeverity::Blocking,
            false,
        )),
        1 => Ok(candidates.into_iter().next().expect("len == 1")),
        _ => match selected {
            None => {
                let roots: Vec<serde_json::Value> = candidates
                    .iter()
                    .map(|c| json!({ "rootId": c.root_id, "path": c.path, "kind": want_kind }))
                    .collect();
                Err(ContractError::new(
                    ErrorCode::InboxDestinationRootRequired,
                    format!(
                        "Multiple library roots can host '{}'; a destination root must be selected",
                        class.as_str()
                    ),
                    ErrorSeverity::Blocking,
                    false,
                )
                .with_details(json!({ "category": class.as_str(), "candidates": roots })))
            }
            Some(id) => candidates.into_iter().find(|c| c.root_id == id).ok_or_else(|| {
                ContractError::new(
                    ErrorCode::InboxInvalidDestinationRoot,
                    format!(
                        "Selected root '{id}' is not a valid destination for '{}'",
                        class.as_str()
                    ),
                    ErrorSeverity::Blocking,
                    false,
                )
            }),
        },
    }
}

/// Return the effective frame type for a file: `manual_override` if set, else
/// the durable group-keyed `frameType` override, else the extracted
/// `frame_type` (same priority chain as classify's split and the metadata
/// DTO — the durable middle layer survives evidence rebuilds, #854).
pub(crate) fn effective_frame_type(
    ev: &persistence_inbox::repositories::inbox::InboxEvidenceRow,
) -> Option<&str> {
    ev.manual_override.as_deref().or(ev.override_frame_type.as_deref()).or(ev.frame_type.as_deref())
}

/// Whether an item's classified evidence makes it a light-frame item — the
/// gate for the whole attribution surface (FR-019).
///
/// A confirmable item is single-type (FR-050), so the first classified file's
/// effective frame type determines the whole item's class. Shared by
/// [`confirm`] and [`crate::attribution::suggest_candidates`] so the
/// suggest-time and apply-time gates can never disagree.
pub(crate) fn evidence_is_light(
    evidence_rows: &[persistence_inbox::repositories::inbox::InboxEvidenceRow],
) -> bool {
    evidence_rows
        .iter()
        .find_map(|ev| effective_frame_type(ev).map(|ft| (ft, ev.is_master != 0)))
        .and_then(|(ft, is_master)| classify_frame(ft, is_master))
        == Some(FrameTypeClass::Light)
}

/// Load the active `pattern` from the settings table, or fall back to the
/// built-in default if no pattern has been configured yet.
///
/// # Errors
/// Returns `pattern.unset` when the stored pattern fails to deserialize.
pub(crate) async fn load_active_pattern(
    pool: &SqlitePool,
) -> Result<Vec<PatternPart>, ContractError> {
    // Try to read the stored pattern JSON.
    let raw_opt = settings_repo::get_raw(pool, "pattern").await.unwrap_or(None);

    if let Some(raw) = raw_opt {
        // The stored value is a JSON array of PatternPart objects.
        match serde_json::from_value::<Vec<ContractPatternPart>>(raw) {
            Ok(parts) => {
                return Ok(parts
                    .into_iter()
                    .map(|p| PatternPart { id: p.id, kind: p.kind, value: p.value })
                    .collect());
            }
            Err(e) => {
                return Err(ContractError::new(
                    ErrorCode::PatternUnset,
                    format!("Stored pattern is invalid: {e}"),
                    ErrorSeverity::Blocking,
                    false,
                ));
            }
        }
    }

    // Fall back to the default pattern defined in contracts_core::settings.
    let defaults = contracts_core::settings::SettingsState::default();
    Ok(defaults
        .pattern
        .into_iter()
        .map(|p| PatternPart { id: p.id, kind: p.kind, value: p.value })
        .collect())
}

/// Freeze the inferred context that produced this plan item's destination, as
/// the free-form `[{label,value}]` JSON the `plan_items.provenance` column
/// already carries (spec 002 FR-005, applied at the spec 041 Inbox confirm
/// gate).
///
/// The snapshot is self-contained by construction: a later rescan re-extracts
/// headers and overwrites the live metadata, so anything that referenced a
/// metadata row instead of copying it would silently answer with today's values
/// rather than the ones the approver saw.
///
/// `destination_pattern` is stored next to the metadata because the destination
/// is a function of both. Without it, a path that no longer matches cannot be
/// attributed to metadata drift as opposed to a changed pattern setting. It is
/// `None` for catalogue-in-place items, which resolve no pattern.
///
/// `BTreeMap` fixes entry order so two snapshots of the same context compare
/// equal byte-for-byte.
fn freeze_confirm_provenance(
    bundle: &MetadataBundle,
    is_master: bool,
    destination_pattern: Option<&str>,
) -> Option<String> {
    let mut frozen: BTreeMap<&str, String> =
        bundle.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    frozen.insert("is_master", is_master.to_string());
    if let Some(pattern) = destination_pattern {
        frozen.insert("destination_pattern", pattern.to_owned());
    }
    let entries: Vec<ProvenanceEntry> = frozen
        .into_iter()
        .map(|(label, value)| ProvenanceEntry { label: label.to_owned(), value })
        .collect();
    serde_json::to_string(&entries).ok()
}

/// Build a `MetadataBundle` for pattern resolution from extracted FITS/XISF
/// headers + the known `frame_type` from classification evidence.
///
/// Source fields follow the v1 registry in `crates/patterns/src/registry.rs`:
/// `target`, `filter`, `date`, `frame_type`, `camera`, `exposure`, `gain`,
/// `binning`, `set_temp`.
pub(crate) fn build_metadata_bundle(
    abs_path: &std::path::Path,
    frame_type: &str,
    norm_table: &metadata_core::ImageTypNormalizationTable,
    cameras: &[contracts_core::equipment::Camera],
) -> MetadataBundle {
    let mut bundle = MetadataBundle::new();

    // Extract raw metadata (F0 cached-extract: memoized by path/mtime/size;
    // dispatches by extension internally).
    let raw_meta = cached_extract(abs_path).ok();

    // frame_type (authoritative from classification)
    bundle.insert("frame_type".to_owned(), frame_type.to_owned());

    if let Some(meta) = raw_meta {
        // target / object
        if let Some(obj) = &meta.object {
            let cleaned = obj.trim();
            if !cleaned.is_empty() {
                bundle.insert("target".to_owned(), cleaned.to_owned());
            }
        }
        // filter
        if let Some(filter) = &meta.filter {
            let cleaned = filter.trim();
            if !cleaned.is_empty() {
                bundle.insert("filter".to_owned(), cleaned.to_owned());
            }
        }
        // date — use the DATE-OBS field; strip time component for the directory token
        if let Some(date_obs) = &meta.date_obs {
            let date_part = date_obs.split('T').next().unwrap_or(date_obs.as_str());
            if !date_part.is_empty() {
                bundle.insert("date".to_owned(), date_part.to_owned());
            }
        }
        // camera — a registered camera claims the raw INSTRUME string under
        // case/whitespace-insensitive alias matching, so every spelling one
        // capture program or another wrote for the same physical camera
        // resolves to a single destination directory. An unregistered string
        // stays raw; nothing is created here (see the module note on why
        // ingest resolves but never registers equipment).
        if let Some(instrume) = &meta.instrume {
            let cleaned = instrume.trim();
            if !cleaned.is_empty() {
                let resolved =
                    app_core_calibration::equipment::resolve_camera_display_name(cameras, cleaned)
                        .unwrap_or_else(|| cleaned.to_owned());
                bundle.insert("camera".to_owned(), resolved);
            }
        }
        // exposure
        if let Some(exp) = &meta.exposure {
            bundle.insert("exposure".to_owned(), exp.trim().to_owned());
        }
        // gain
        if let Some(gain) = &meta.gain {
            bundle.insert("gain".to_owned(), gain.trim().to_owned());
        }
        // binning — use xbinning x ybinning format
        if let (Some(xb), Some(yb)) = (&meta.x_binning, &meta.y_binning) {
            bundle.insert("binning".to_owned(), format!("{}x{}", xb.trim(), yb.trim()));
        }
        // telescope (not a standard token but included for completeness)
        if let Some(scope) = &meta.telescop {
            let cleaned = scope.trim();
            if !cleaned.is_empty() {
                bundle.insert("telescope".to_owned(), cleaned.to_owned());
            }
        }
    }

    // Ensure all required v1 tokens that the pattern resolver needs have
    // fallback-friendly entries (the registry has per-token fallbacks anyway).
    let _ = norm_table; // used in classify path; not needed here directly

    bundle
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use audit::bus::EventBus;
    use persistence_calibration::repositories::equipment as equipment_repo;
    use persistence_core::Database;
    use persistence_inbox::repositories::inbox::{
        InsertEvidence, InsertInboxItem, UpsertClassification, UpsertInboxSubItem,
        UpsertSourceGroup,
    };
    use std::io::Write;

    fn make_bus(db: &Database) -> EventBus {
        EventBus::with_pool(db.pool().clone())
    }

    async fn test_db() -> persistence_core::Database {
        persistence_core::test_support::setup_db().await
    }

    /// Count plans sitting in one lifecycle state. `ready_for_review` is the
    /// only state `plans.approve` accepts, so it is the appliability probe.
    async fn count_plans_in_state(pool: &SqlitePool, state: &str) -> i64 {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM plans WHERE state = ?")
            .bind(state)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    /// Write a minimal single-block FITS file from the optional cards the
    /// destination-pattern tests care about. The `write_fits*` helpers below
    /// are thin presets over this.
    fn write_fits_cards(
        dir: &std::path::Path,
        name: &str,
        imagetyp: &str,
        object: Option<&str>,
        filter: Option<&str>,
        date_obs: Option<&str>,
        exptime: Option<f64>,
        instrume: Option<&str>,
    ) {
        let path = dir.join(name);
        let mut block = vec![b' '; 2880];
        let mut idx = 0usize;
        let write_card = |block: &mut Vec<u8>, idx: &mut usize, card: &str| {
            let bytes = card.as_bytes();
            let len = bytes.len().min(80);
            block[*idx * 80..*idx * 80 + len].copy_from_slice(&bytes[..len]);
            *idx += 1;
        };
        let imagetyp_card = format!("IMAGETYP= '{imagetyp:<8}'");
        write_card(&mut block, &mut idx, &format!("{imagetyp_card:<80}"));
        if let Some(obj) = object {
            write_card(&mut block, &mut idx, &format!("{:<80}", format!("OBJECT  = '{obj}'")));
        }
        if let Some(f) = filter {
            write_card(&mut block, &mut idx, &format!("{:<80}", format!("FILTER  = '{f}'")));
        }
        if let Some(d) = date_obs {
            write_card(&mut block, &mut idx, &format!("{:<80}", format!("DATE-OBS= '{d}'")));
        }
        if let Some(e) = exptime {
            write_card(&mut block, &mut idx, &format!("{:<80}", format!("EXPTIME = {e}")));
        }
        if let Some(i) = instrume {
            write_card(&mut block, &mut idx, &format!("{:<80}", format!("INSTRUME= '{i}'")));
        }
        block[idx * 80..idx * 80 + 3].copy_from_slice(b"END");
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(&block).unwrap();
    }

    /// Write a minimal FITS file with a given IMAGETYP and optional OBJECT/FILTER/DATE-OBS.
    fn write_fits(
        dir: &std::path::Path,
        name: &str,
        imagetyp: &str,
        object: Option<&str>,
        filter: Option<&str>,
        date_obs: Option<&str>,
    ) {
        write_fits_cards(dir, name, imagetyp, object, filter, date_obs, None, None);
    }

    /// Like [`write_fits`] but also writes an `EXPTIME` card. Used by calibration
    /// (dark/flat) tests so their files carry the exposure their per-type
    /// destination pattern requires (spec 041 US9 gate).
    #[allow(clippy::too_many_arguments)]
    fn write_fits_exp(
        dir: &std::path::Path,
        name: &str,
        imagetyp: &str,
        object: Option<&str>,
        filter: Option<&str>,
        date_obs: Option<&str>,
        exptime: f64,
    ) {
        write_fits_cards(dir, name, imagetyp, object, filter, date_obs, Some(exptime), None);
    }

    async fn setup_classified_item(
        db: &Database,
        item_id: &str,
        result: &str,
        frame_type: Option<&str>,
        sig: &str,
        file_names: &[&str],
    ) {
        inbox_repo::insert_inbox_item(
            db.pool(),
            &InsertInboxItem {
                id: item_id,
                root_id: "root-1",
                relative_path: "",
                file_count: i64::try_from(file_names.len()).unwrap_or(i64::MAX),
                content_signature: Some(sig),
                lane: "fits",
            },
        )
        .await
        .unwrap();

        inbox_repo::upsert_classification(
            db.pool(),
            &UpsertClassification {
                inbox_item_id: item_id,
                result,
                frame_type,
                content_signature: sig,
                unclassified_file_count: 0,
            },
        )
        .await
        .unwrap();

        for (i, fname) in file_names.iter().enumerate() {
            let ev_id = format!("ev-{item_id}-{i}");
            inbox_repo::insert_evidence(
                db.pool(),
                &InsertEvidence {
                    id: &ev_id,
                    inbox_item_id: item_id,
                    relative_file_path: fname,
                    frame_type,
                    evidence_source: "imagetyp_header",
                    raw_value: Some("Light Frame"),
                    unclassified: false,
                    manual_override: None,
                    is_master: false,
                    master_detector: None,
                },
            )
            .await
            .unwrap();
        }
    }

    /// Like [`setup_classified_item`] but the item is owned by an explicit
    /// `root_id` (for the US8 inbox-source routing tests).
    async fn setup_classified_item_rooted(
        db: &Database,
        item_id: &str,
        root_id: &str,
        frame_type: Option<&str>,
        sig: &str,
        file_names: &[&str],
    ) {
        inbox_repo::insert_inbox_item(
            db.pool(),
            &InsertInboxItem {
                id: item_id,
                root_id,
                relative_path: "",
                file_count: i64::try_from(file_names.len()).unwrap_or(i64::MAX),
                content_signature: Some(sig),
                lane: "fits",
            },
        )
        .await
        .unwrap();
        inbox_repo::upsert_classification(
            db.pool(),
            &UpsertClassification {
                inbox_item_id: item_id,
                result: "classified",
                frame_type,
                content_signature: sig,
                unclassified_file_count: 0,
            },
        )
        .await
        .unwrap();
        for (i, fname) in file_names.iter().enumerate() {
            inbox_repo::insert_evidence(
                db.pool(),
                &InsertEvidence {
                    id: &format!("ev-{item_id}-{i}"),
                    inbox_item_id: item_id,
                    relative_file_path: fname,
                    frame_type,
                    evidence_source: "imagetyp_header",
                    raw_value: Some("Light Frame"),
                    unclassified: false,
                    manual_override: None,
                    is_master: false,
                    master_detector: None,
                },
            )
            .await
            .unwrap();
        }
    }

    #[tokio::test]
    async fn confirm_single_type_creates_plan() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits(
            tmp.path(),
            "light_001.fits",
            "Light Frame",
            Some("M42"),
            Some("Ha"),
            Some("2025-10-10T22:00:00"),
        );
        write_fits(
            tmp.path(),
            "light_002.fits",
            "Light Frame",
            Some("M42"),
            Some("Ha"),
            Some("2025-10-10T22:05:00"),
        );
        write_fits(
            tmp.path(),
            "light_003.fits",
            "Light Frame",
            Some("M42"),
            Some("Ha"),
            Some("2025-10-10T22:10:00"),
        );

        let db = test_db().await;

        let bus = make_bus(&db);
        setup_classified_item(
            &db,
            "item-c1",
            "classified",
            Some("light"),
            "sig-abc",
            &["light_001.fits", "light_002.fits", "light_003.fits"],
        )
        .await;

        let resp = confirm(
            db.pool(),
            &bus,
            ConfirmRequest {
                inbox_item_id: "item-c1".to_owned(),
                content_signature: "sig-abc".to_owned(),
                destructive_destination: None,
                root_absolute_path: tmp.path().to_owned(),
                root_id: None,
                chosen_attribution: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(resp.plan_state, "ready_for_review");
        assert_eq!(resp.items_total, 3);
    }

    /// Confirm one inbox item holding two light frames, one per session folder,
    /// under identical pattern tokens. Returns the confirm error and asserts
    /// that no plan was left ready to apply.
    async fn confirm_two_session_frames(item_id: &str, names: (&str, &str)) -> ContractError {
        let tmp = tempfile::tempdir().unwrap();
        for (session, name) in [("session-a", names.0), ("session-b", names.1)] {
            let dir = tmp.path().join(session);
            std::fs::create_dir_all(&dir).unwrap();
            write_fits(
                &dir,
                name,
                "Light Frame",
                Some("M42"),
                Some("Ha"),
                Some("2026-01-12T22:00:00"),
            );
        }

        let db = test_db().await;
        let files = [format!("session-a/{}", names.0), format!("session-b/{}", names.1)];
        setup_classified_item(
            &db,
            item_id,
            "classified",
            Some("light"),
            "sig-collide",
            &[files[0].as_str(), files[1].as_str()],
        )
        .await;

        let err = confirm(
            db.pool(),
            &make_bus(&db),
            ConfirmRequest {
                inbox_item_id: item_id.to_owned(),
                content_signature: "sig-collide".to_owned(),
                destructive_destination: None,
                root_absolute_path: tmp.path().to_owned(),
                root_id: None,
                chosen_attribution: None,
            },
        )
        .await
        .expect_err("two files on one destination must not produce a plan");

        assert_eq!(
            count_plans_in_state(db.pool(), "ready_for_review").await,
            0,
            "the collision must not reach apply through a reviewable plan"
        );
        assert!(inbox_repo::get_plan_link(db.pool(), item_id).await.unwrap().is_none());
        err
    }

    /// Two frames captured in different session folders share a basename and
    /// carry identical pattern tokens, so the rendered destination is the same
    /// path for both. Applying that plan fails partway: `move_op` refuses the
    /// second item with `conflict.destination_exists`, leaving the item's frames
    /// half-moved. Constitution II puts that decision in the reviewable plan, so
    /// confirm must refuse to build one.
    #[tokio::test]
    async fn confirm_refuses_two_files_that_resolve_onto_one_destination() {
        let err =
            confirm_two_session_frames("item-collide", ("light_001.fits", "light_001.fits")).await;
        assert_eq!(err.code, ErrorCode::InboxDestinationCollision);
    }

    /// The byte comparison this replaces treats each pair below as two paths.
    /// APFS resolves both pairs to one file, NTFS the first. Only the case pair
    /// is reachable end to end: the filesystem normalizes the Unicode pair
    /// before `file_name()` sees it.
    #[test]
    fn destination_key_folds_case_and_normalizes_unicode() {
        assert_ne!("m42/Light_001.fits", "m42/light_001.fits");
        assert_eq!(destination_key("m42/Light_001.fits"), destination_key("m42/light_001.fits"));
        assert_ne!("m42/cafe\u{301}.fits", "m42/caf\u{e9}.fits");
        assert_eq!(destination_key("m42/cafe\u{301}.fits"), destination_key("m42/caf\u{e9}.fits"));
    }

    /// APFS and NTFS resolve two destinations differing only in case to one
    /// file, so the gate compares case-folded keys rather than bytes.
    #[tokio::test]
    async fn confirm_refuses_two_files_whose_destinations_differ_only_in_case() {
        let err =
            confirm_two_session_frames("item-collide-case", ("light_001.fits", "LIGHT_001.fits"))
                .await;
        assert_eq!(err.code, ErrorCode::InboxDestinationCollision);
    }

    /// Build one classified sibling sub-item under `sg_id`.
    ///
    /// Deliberately not [`setup_classified_item`]: that writes a legacy row via
    /// `insert_inbox_item` with no `source_group_id` and an empty `group_key`,
    /// so two of them are not siblings in the sense SC-006 is about. Sibling
    /// identity is `(root_id, relative_path, group_key)` — same folder, same
    /// group, different classification identity.
    async fn setup_sibling_sub_item(
        db: &Database,
        item_id: &str,
        sg_id: &str,
        group_key: &str,
        frame_type: &str,
        sig: &str,
        file_names: &[&str],
    ) {
        inbox_repo::upsert_inbox_sub_item(
            db.pool(),
            &UpsertInboxSubItem {
                id: item_id,
                root_id: "root-1",
                relative_path: "",
                source_group_id: sg_id,
                group_key,
                group_label: group_key,
                frame_type: Some(frame_type),
                content_signature: sig,
                file_count: i64::try_from(file_names.len()).unwrap_or(i64::MAX),
                lane: "fits",
                needs_review: false,
            },
        )
        .await
        .unwrap();

        inbox_repo::upsert_classification(
            db.pool(),
            &UpsertClassification {
                inbox_item_id: item_id,
                result: "classified",
                frame_type: Some(frame_type),
                content_signature: sig,
                unclassified_file_count: 0,
            },
        )
        .await
        .unwrap();

        for (i, fname) in file_names.iter().enumerate() {
            inbox_repo::insert_evidence(
                db.pool(),
                &InsertEvidence {
                    id: &format!("ev-{item_id}-{i}"),
                    inbox_item_id: item_id,
                    relative_file_path: fname,
                    frame_type: Some(frame_type),
                    evidence_source: "imagetyp_header",
                    raw_value: Some(frame_type),
                    unclassified: false,
                    manual_override: None,
                    is_master: false,
                    master_detector: None,
                },
            )
            .await
            .unwrap();
        }
    }

    /// Spec 058 T026 / FR-010 / SC-006: `inbox.confirm` operates on exactly one
    /// `inbox_item_id` and alters no sibling.
    ///
    /// Contracts say this needs no code change — the point of the test is that
    /// dropping the parent row (FR-001) makes the N siblings of a split folder
    /// the *only* rows, so sibling isolation stops being incidental and becomes
    /// the whole correctness story. Asserted in both directions: the confirmed
    /// item must move to `plan_open` and gain a plan link, the sibling must be
    /// byte-identical to its pre-confirm snapshot and have no link.
    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn confirm_alters_exactly_one_item_and_leaves_its_sibling_untouched_sc006() {
        let tmp = tempfile::tempdir().unwrap();
        for (i, name) in ["light_001.fits", "light_002.fits"].iter().enumerate() {
            write_fits(
                tmp.path(),
                name,
                "Light Frame",
                Some("M42"),
                Some("Ha"),
                Some(&format!("2025-10-10T22:0{i}:00")),
            );
        }
        for (i, name) in ["flat_001.fits", "flat_002.fits"].iter().enumerate() {
            write_fits_exp(
                tmp.path(),
                name,
                "Flat Field",
                None,
                Some("Ha"),
                Some(&format!("2025-10-10T18:0{i}:00")),
                3.0,
            );
        }

        let db = test_db().await;
        let bus = make_bus(&db);

        inbox_repo::upsert_inbox_source_group(
            db.pool(),
            &UpsertSourceGroup {
                id: "sg-sc006",
                root_id: "root-1",
                relative_path: "",
                content_signature: Some("sig-folder"),
                format: Some("fits"),
                lane: Some("move"),
                file_count: 4,
            },
        )
        .await
        .unwrap();

        setup_sibling_sub_item(
            &db,
            "item-lights",
            "sg-sc006",
            "type=light",
            "light",
            "sig-lights",
            &["light_001.fits", "light_002.fits"],
        )
        .await;
        setup_sibling_sub_item(
            &db,
            "item-flats",
            "sg-sc006",
            "type=flat",
            "flat",
            "sig-flats",
            &["flat_001.fits", "flat_002.fits"],
        )
        .await;

        let sibling_before = inbox_repo::get_inbox_item(db.pool(), "item-flats").await.unwrap();

        confirm(
            db.pool(),
            &bus,
            ConfirmRequest {
                inbox_item_id: "item-lights".to_owned(),
                content_signature: "sig-lights".to_owned(),
                destructive_destination: None,
                root_absolute_path: tmp.path().to_owned(),
                root_id: None,
                chosen_attribution: None,
            },
        )
        .await
        .unwrap();

        // Positive direction — without this the assertions below pass on a
        // confirm that did nothing at all.
        let confirmed = inbox_repo::get_inbox_item(db.pool(), "item-lights").await.unwrap();
        assert_eq!(
            confirmed.state, "plan_open",
            "the confirmed item must have moved to `plan_open` — otherwise this test proves nothing"
        );
        assert!(
            inbox_repo::get_plan_link(db.pool(), "item-lights").await.unwrap().is_some(),
            "the confirmed item must own the new plan link"
        );

        let sibling_after = inbox_repo::get_inbox_item(db.pool(), "item-flats").await.unwrap();
        assert_eq!(
            (
                sibling_after.state.as_str(),
                sibling_after.frame_type.as_deref(),
                sibling_after.needs_review,
                sibling_after.content_signature.as_deref(),
                sibling_after.group_key.as_str(),
                sibling_after.file_count,
            ),
            (
                sibling_before.state.as_str(),
                sibling_before.frame_type.as_deref(),
                sibling_before.needs_review,
                sibling_before.content_signature.as_deref(),
                sibling_before.group_key.as_str(),
                sibling_before.file_count,
            ),
            "SC-006: confirming one item must not alter its sibling"
        );
        assert!(
            inbox_repo::get_plan_link(db.pool(), "item-flats").await.unwrap().is_none(),
            "SC-006: the sibling must not be bound to the confirmed item's plan"
        );

        let sibling_classification =
            inbox_repo::get_classification(db.pool(), "item-flats").await.unwrap().unwrap();
        assert_eq!(
            (sibling_classification.result.as_str(), sibling_classification.frame_type.as_deref()),
            ("classified", Some("flat")),
            "SC-006: the sibling's own classification must survive the confirm untouched"
        );
    }

    /// US7 / T042-T043: the chosen destructive destination must be persisted on
    /// the plan (the durable audit record), default to `archive` when unset, and
    /// coerce any non-recoverable value to `archive` so confirm can never schedule
    /// a permanent delete without a recoverable step.
    #[tokio::test]
    async fn confirm_persists_destructive_destination() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits(
            tmp.path(),
            "light_001.fits",
            "Light Frame",
            Some("M42"),
            Some("Ha"),
            Some("2025-10-10T22:00:00"),
        );

        // (input destructive_destination, expected persisted value).
        // Canonical vocabulary is `archive | trash` (spec 033, migration 0040).
        let cases = [
            (None, "archive"),
            (Some("archive"), "archive"),
            (Some("trash"), "trash"),
            // Legacy `os_trash` is no longer canonical → coerced to safe archive.
            (Some("os_trash"), "archive"),
            // Anything outside the recoverable set must fall back to archive —
            // never a permanent delete.
            (Some("delete"), "archive"),
            (Some(""), "archive"),
        ];

        for (dest, expected) in &cases {
            // Fresh DB per case: setup_classified_item hardcodes the
            // (root_id, relative_path) key, so it supports one item per DB.
            let db = test_db().await;
            let bus = make_bus(&db);
            setup_classified_item(
                &db,
                "item-dd",
                "classified",
                Some("light"),
                "sig-dd",
                &["light_001.fits"],
            )
            .await;

            let resp = confirm(
                db.pool(),
                &bus,
                ConfirmRequest {
                    inbox_item_id: "item-dd".to_owned(),
                    content_signature: "sig-dd".to_owned(),
                    destructive_destination: dest.map(str::to_owned),
                    root_absolute_path: tmp.path().to_owned(),
                    root_id: None,
                    chosen_attribution: None,
                },
            )
            .await
            .unwrap();

            let (persisted,): (String,) =
                sqlx::query_as("SELECT destructive_destination FROM plans WHERE id = ?")
                    .bind(&resp.plan_id)
                    .fetch_one(db.pool())
                    .await
                    .unwrap();

            assert_eq!(
                &persisted, expected,
                "input {dest:?} must persist as {expected}, never a permanent delete"
            );
        }
    }

    /// Spec 041 FR-050/T071: the "split" action is removed and a genuinely
    /// mixed folder (classification.result == "unclassified" because it still
    /// has 2+ distinct frame types) is no longer confirmable at all — not via
    /// "split" (gone) and not via "confirm" (requires "classified"). It must
    /// be re-split into single-type sub-items (T066 materialization) before
    /// any of its pieces can be confirmed.
    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn confirm_of_mixed_result_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits(
            tmp.path(),
            "light_001.fits",
            "Light Frame",
            Some("NGC7000"),
            Some("Ha"),
            Some("2025-10-10T22:00:00"),
        );
        write_fits(
            tmp.path(),
            "light_002.fits",
            "Light Frame",
            Some("NGC7000"),
            Some("Ha"),
            Some("2025-10-10T22:05:00"),
        );
        write_fits_exp(
            tmp.path(),
            "dark_001.fits",
            "Dark Frame",
            None,
            None,
            Some("2025-10-10T20:00:00"),
            300.0,
        );
        write_fits_exp(
            tmp.path(),
            "dark_002.fits",
            "Dark Frame",
            None,
            None,
            Some("2025-10-10T20:05:00"),
            300.0,
        );

        let db = test_db().await;

        let bus = make_bus(&db);
        let item_id = "item-mixed-split";
        let sig = "sig-mixed";

        inbox_repo::insert_inbox_item(
            db.pool(),
            &InsertInboxItem {
                id: item_id,
                root_id: "root-1",
                relative_path: "",
                file_count: 4,
                content_signature: Some(sig),
                lane: "fits",
            },
        )
        .await
        .unwrap();

        inbox_repo::upsert_classification(
            db.pool(),
            &UpsertClassification {
                inbox_item_id: item_id,
                result: "unclassified",
                frame_type: None,
                content_signature: sig,
                unclassified_file_count: 0,
            },
        )
        .await
        .unwrap();

        for (ft, fname) in [
            ("light", "light_001.fits"),
            ("light", "light_002.fits"),
            ("dark", "dark_001.fits"),
            ("dark", "dark_002.fits"),
        ] {
            let ev_id = format!("ev-m-{fname}");
            inbox_repo::insert_evidence(
                db.pool(),
                &InsertEvidence {
                    id: &ev_id,
                    inbox_item_id: item_id,
                    relative_file_path: fname,
                    frame_type: Some(ft),
                    evidence_source: "imagetyp_header",
                    raw_value: Some(if ft == "light" { "Light Frame" } else { "Dark Frame" }),
                    unclassified: false,
                    manual_override: None,
                    is_master: false,
                    master_detector: None,
                },
            )
            .await
            .unwrap();
        }

        let err = confirm(
            db.pool(),
            &bus,
            ConfirmRequest {
                inbox_item_id: item_id.to_owned(),
                content_signature: sig.to_owned(),
                destructive_destination: None,
                root_absolute_path: tmp.path().to_owned(),
                root_id: None,
                chosen_attribution: None,
            },
        )
        .await
        .unwrap_err();

        assert_eq!(err.code, ErrorCode::ClassificationAmbiguous);
    }

    /// Helper: insert a classified inbox item with explicit per-file frame-type
    /// evidence, returning nothing (panics on error). `files` is (frame_type,
    /// filename, imagetyp_raw).
    async fn setup_typed_item(
        db: &Database,
        item_id: &str,
        sig: &str,
        result: &str,
        files: &[(&str, &str, &str)],
    ) {
        inbox_repo::insert_inbox_item(
            db.pool(),
            &InsertInboxItem {
                id: item_id,
                root_id: "root-1",
                relative_path: "",
                file_count: i64::try_from(files.len()).unwrap(),
                content_signature: Some(sig),
                lane: "fits",
            },
        )
        .await
        .unwrap();

        inbox_repo::upsert_classification(
            db.pool(),
            &UpsertClassification {
                inbox_item_id: item_id,
                result,
                frame_type: None,
                content_signature: sig,
                unclassified_file_count: 0,
            },
        )
        .await
        .unwrap();

        for (ft, fname, raw) in files {
            let ev_id = format!("ev-{item_id}-{fname}");
            inbox_repo::insert_evidence(
                db.pool(),
                &InsertEvidence {
                    id: &ev_id,
                    inbox_item_id: item_id,
                    relative_file_path: fname,
                    frame_type: Some(ft),
                    evidence_source: "imagetyp_header",
                    raw_value: Some(raw),
                    unclassified: false,
                    manual_override: None,
                    is_master: false,
                    master_detector: None,
                },
            )
            .await
            .unwrap();
        }
    }

    fn dest_dir(it: &persistence_plans::repositories::plans::PlanItemRow) -> String {
        std::path::Path::new(&it.to_relative_path)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// US5 (T037): a single-type folder produces exactly one action group.
    #[tokio::test]
    async fn confirm_single_type_emits_one_action_group() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits(
            tmp.path(),
            "light_001.fits",
            "Light Frame",
            Some("NGC7000"),
            Some("Ha"),
            Some("2025-10-10T22:00:00"),
        );
        write_fits(
            tmp.path(),
            "light_002.fits",
            "Light Frame",
            Some("NGC7000"),
            Some("Ha"),
            Some("2025-10-10T22:05:00"),
        );

        let db = test_db().await;

        let bus = make_bus(&db);
        setup_typed_item(
            &db,
            "item-single",
            "sig-single",
            "classified",
            &[
                ("light", "light_001.fits", "Light Frame"),
                ("light", "light_002.fits", "Light Frame"),
            ],
        )
        .await;

        let resp = confirm(
            db.pool(),
            &bus,
            ConfirmRequest {
                inbox_item_id: "item-single".to_owned(),
                content_signature: "sig-single".to_owned(),
                destructive_destination: None,
                root_absolute_path: tmp.path().to_owned(),
                root_id: None,
                chosen_attribution: None,
            },
        )
        .await
        .unwrap();

        let items =
            persistence_plans::repositories::plans::list_plan_items(db.pool(), &resp.plan_id)
                .await
                .unwrap();
        let dirs: std::collections::BTreeSet<String> = items.iter().map(dest_dir).collect();
        assert_eq!(dirs.len(), 1, "single-type folder must yield exactly one action group");
    }

    #[tokio::test]
    async fn stale_signature_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits(tmp.path(), "frame_000.fits", "Light Frame", None, None, None);

        let db = test_db().await;

        let bus = make_bus(&db);
        setup_classified_item(
            &db,
            "item-stale",
            "classified",
            Some("light"),
            "sig-current",
            &["frame_000.fits"],
        )
        .await;

        let err = confirm(
            db.pool(),
            &bus,
            ConfirmRequest {
                inbox_item_id: "item-stale".to_owned(),
                content_signature: "sig-OLD".to_owned(),
                destructive_destination: None,
                root_absolute_path: tmp.path().to_owned(),
                root_id: None,
                chosen_attribution: None,
            },
        )
        .await
        .unwrap_err();

        assert_eq!(err.code, ErrorCode::ClassificationStale);
    }

    /// Spec 041 FR-050/T071/T072: `ConfirmRequest` no longer carries an
    /// `action` field at all — the only gate is `classification.result ==
    /// "classified"`. An item whose classification is still `"unclassified"`
    /// (e.g. a folder with multiple distinct frame types that has not yet
    /// been re-split into single-type sub-items) is rejected with the same
    /// ambiguous-classification error a legacy "split" action request used
    /// to hit.
    #[tokio::test]
    async fn unclassified_result_returns_ambiguous() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits(tmp.path(), "frame_000.fits", "Light Frame", None, None, None);

        let db = test_db().await;

        let bus = make_bus(&db);
        setup_classified_item(
            &db,
            "item-ambig",
            "unclassified",
            None,
            "sig-x",
            &["frame_000.fits"],
        )
        .await;

        let err = confirm(
            db.pool(),
            &bus,
            ConfirmRequest {
                inbox_item_id: "item-ambig".to_owned(),
                content_signature: "sig-x".to_owned(),
                destructive_destination: None,
                root_absolute_path: tmp.path().to_owned(),
                root_id: None,
                chosen_attribution: None,
            },
        )
        .await
        .unwrap_err();

        assert_eq!(err.code, ErrorCode::ClassificationAmbiguous);
    }

    #[tokio::test]
    async fn duplicate_confirm_returns_has_open_plan() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits(
            tmp.path(),
            "frame_000.fits",
            "Light Frame",
            Some("M42"),
            Some("Ha"),
            Some("2025-10-10T22:00:00"),
        );
        write_fits(
            tmp.path(),
            "frame_001.fits",
            "Light Frame",
            Some("M42"),
            Some("Ha"),
            Some("2025-10-10T22:05:00"),
        );

        let db = test_db().await;

        let bus = make_bus(&db);
        setup_classified_item(
            &db,
            "item-dup",
            "classified",
            Some("light"),
            "sig-dup",
            &["frame_000.fits", "frame_001.fits"],
        )
        .await;

        // First confirm
        confirm(
            db.pool(),
            &bus,
            ConfirmRequest {
                inbox_item_id: "item-dup".to_owned(),
                content_signature: "sig-dup".to_owned(),
                destructive_destination: None,
                root_absolute_path: tmp.path().to_owned(),
                root_id: None,
                chosen_attribution: None,
            },
        )
        .await
        .unwrap();

        // Second confirm should fail
        let err = confirm(
            db.pool(),
            &bus,
            ConfirmRequest {
                inbox_item_id: "item-dup".to_owned(),
                content_signature: "sig-dup".to_owned(),
                destructive_destination: None,
                root_absolute_path: tmp.path().to_owned(),
                root_id: None,
                chosen_attribution: None,
            },
        )
        .await
        .unwrap_err();

        assert_eq!(err.code, ErrorCode::InboxHasOpenPlan);
    }

    // ── spec 041 US4: per-source organization-state (move vs catalogue) ──────

    /// Register a `registered_sources` row with an explicit organization state
    /// so `get_source_organization_state` returns it for the inbox item's root.
    async fn register_source_org_state(db: &Database, root_id: &str, kind: &str, org_state: &str) {
        sqlx::query(
            "INSERT INTO registered_sources
                (id, kind, path, kind_subtype, scan_depth, created_at, created_via, organization_state)
             VALUES (?, ?, '/tmp/src', NULL, 'recursive', '2026-01-01T00:00:00Z', 'first_run', ?)",
        )
        .bind(root_id)
        .bind(kind)
        .bind(org_state)
        .execute(db.pool())
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn organized_source_emits_catalogue_actions() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits(
            tmp.path(),
            "light_001.fits",
            "Light Frame",
            Some("M42"),
            Some("Ha"),
            Some("2025-10-10T22:00:00"),
        );

        let db = test_db().await;

        let bus = make_bus(&db);
        register_source_org_state(&db, "root-1", "light_frames", "organized").await;
        setup_classified_item(
            &db,
            "item-org",
            "classified",
            Some("light"),
            "sig-org",
            &["light_001.fits"],
        )
        .await;

        let resp = confirm(
            db.pool(),
            &bus,
            ConfirmRequest {
                inbox_item_id: "item-org".to_owned(),
                content_signature: "sig-org".to_owned(),
                destructive_destination: None,
                root_absolute_path: tmp.path().to_owned(),
                root_id: None,
                chosen_attribution: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(resp.catalogue_count, 1, "organized source → catalogue");
        assert_eq!(resp.move_count, 0);
        assert!(matches!(resp.organization_state, OrganizationState::Organized));

        // Catalogue plan item: action == 'catalogue', from == to (no move).
        let rows = sqlx::query_as::<_, (String, String, String)>(
            "SELECT action, from_relative_path, to_relative_path FROM plan_items WHERE plan_id = ?",
        )
        .bind(&resp.plan_id)
        .fetch_all(db.pool())
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "catalogue");
        assert_eq!(rows[0].1, rows[0].2, "catalogue dest == source (in place)");
        assert_eq!(rows[0].1, "light_001.fits");
    }

    #[tokio::test]
    async fn unorganized_source_emits_move_actions() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits(
            tmp.path(),
            "light_001.fits",
            "Light Frame",
            Some("M42"),
            Some("Ha"),
            Some("2025-10-10T22:00:00"),
        );

        let db = test_db().await;

        let bus = make_bus(&db);
        // Non-inbox unorganized source → in-place move under its own root (no
        // destination-root selection). Inbox-source routing is covered by the
        // dedicated US8 tests below.
        register_source_org_state(&db, "root-1", "light_frames", "unorganized").await;
        setup_classified_item(
            &db,
            "item-unorg",
            "classified",
            Some("light"),
            "sig-unorg",
            &["light_001.fits"],
        )
        .await;

        let resp = confirm(
            db.pool(),
            &bus,
            ConfirmRequest {
                inbox_item_id: "item-unorg".to_owned(),
                content_signature: "sig-unorg".to_owned(),
                destructive_destination: None,
                root_absolute_path: tmp.path().to_owned(),
                root_id: None,
                chosen_attribution: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(resp.move_count, 1, "unorganized source → move");
        assert_eq!(resp.catalogue_count, 0);
        assert!(matches!(resp.organization_state, OrganizationState::Unorganized));

        let rows = sqlx::query_as::<_, (String, String, String)>(
            "SELECT action, from_relative_path, to_relative_path FROM plan_items WHERE plan_id = ?",
        )
        .bind(&resp.plan_id)
        .fetch_all(db.pool())
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "move");
        assert_ne!(rows[0].1, rows[0].2, "move dest != source (pattern-resolved)");
    }

    /// Absent source row → default Unorganized (conservative: never catalogue
    /// in place by accident). Mixed provenance composes because the per-file
    /// branch keys on the resolved org-state; an inbox item shares one root so
    /// the result is uniform per confirm today.
    #[tokio::test]
    async fn absent_source_defaults_to_move() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits(
            tmp.path(),
            "frame_000.fits",
            "Light Frame",
            Some("M42"),
            Some("Ha"),
            Some("2025-10-10T22:00:00"),
        );

        let db = test_db().await;

        let bus = make_bus(&db);
        // No registered_sources row inserted for root-1.
        setup_classified_item(
            &db,
            "item-absent",
            "classified",
            Some("light"),
            "sig-absent",
            &["frame_000.fits"],
        )
        .await;

        let resp = confirm(
            db.pool(),
            &bus,
            ConfirmRequest {
                inbox_item_id: "item-absent".to_owned(),
                content_signature: "sig-absent".to_owned(),
                destructive_destination: None,
                root_absolute_path: tmp.path().to_owned(),
                root_id: None,
                chosen_attribution: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(resp.move_count, 1);
        assert_eq!(resp.catalogue_count, 0);
        assert!(matches!(resp.organization_state, OrganizationState::Unorganized));
    }

    // ── spec 041 US8/US9: destination-root resolution + missing-attribute gate ──

    /// Register a source with an explicit id/kind/path/organization_state.
    async fn register_source_full(
        db: &Database,
        root_id: &str,
        kind: &str,
        path: &str,
        org_state: &str,
    ) {
        sqlx::query(
            "INSERT INTO registered_sources
                (id, kind, path, kind_subtype, scan_depth, created_at, created_via, organization_state)
             VALUES (?, ?, ?, NULL, 'recursive', '2026-01-01T00:00:00Z', 'first_run', ?)",
        )
        .bind(root_id)
        .bind(kind)
        .bind(path)
        .bind(org_state)
        .execute(db.pool())
        .await
        .unwrap();
    }

    /// US8/FR-027: a non-inbox source defaults to an in-place move under its own
    /// root — no destination-root selection, `to_root_id == from_root_id`.
    #[tokio::test]
    async fn non_inbox_source_moves_in_place() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits(
            tmp.path(),
            "light_001.fits",
            "Light Frame",
            Some("M42"),
            Some("Ha"),
            Some("2025-10-10T22:00:00"),
        );

        let db = test_db().await;

        let bus = make_bus(&db);
        register_source_full(&db, "root-1", "light_frames", "/lib/lights", "unorganized").await;
        setup_classified_item(
            &db,
            "item-np",
            "classified",
            Some("light"),
            "sig-np",
            &["light_001.fits"],
        )
        .await;

        let resp = confirm(
            db.pool(),
            &bus,
            ConfirmRequest {
                inbox_item_id: "item-np".to_owned(),
                content_signature: "sig-np".to_owned(),
                destructive_destination: None,
                root_absolute_path: tmp.path().to_owned(),
                root_id: None,
                chosen_attribution: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(resp.move_count, 1);
        let (to_root,): (String,) =
            sqlx::query_as("SELECT to_root_id FROM plan_items WHERE plan_id = ?")
                .bind(&resp.plan_id)
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(to_root, "root-1", "non-inbox source stays under its own root");
    }

    /// US8/FR-028: an inbox source with exactly one matching destination root
    /// auto-selects it (no caller `root_id` needed); the plan lands under that
    /// root and the absolute preview uses the root's path.
    #[tokio::test]
    async fn inbox_single_candidate_auto_selects() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits(
            tmp.path(),
            "light_001.fits",
            "Light Frame",
            Some("M42"),
            Some("Ha"),
            Some("2025-10-10T22:00:00"),
        );

        let db = test_db().await;

        let bus = make_bus(&db);
        register_source_full(&db, "root-inbox", "inbox", "/inbox", "unorganized").await;
        register_source_full(&db, "root-lights", "light_frames", "/lib/lights", "unorganized")
            .await;
        // The item lives on the inbox root.
        setup_classified_item_rooted(
            &db,
            "item-1cand",
            "root-inbox",
            Some("light"),
            "sig-1c",
            &["light_001.fits"],
        )
        .await;

        let resp = confirm(
            db.pool(),
            &bus,
            ConfirmRequest {
                inbox_item_id: "item-1cand".to_owned(),
                content_signature: "sig-1c".to_owned(),
                destructive_destination: None,
                root_absolute_path: tmp.path().to_owned(),
                root_id: None,
                chosen_attribution: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(resp.move_count, 1);
        assert_eq!(resp.destinations.len(), 1);
        let d = &resp.destinations[0];
        assert_eq!(d.to_root_id, "root-lights");
        assert!(
            d.to_absolute_path.starts_with("/lib/lights/"),
            "absolute preview uses the chosen root's path, got {}",
            d.to_absolute_path
        );
    }

    /// US8/FR-029/FR-030: an inbox source with >1 matching destination root
    /// requires the caller's `root_id`. Absent → `inbox.destination_root_required`
    /// listing the candidates; supplied & valid → succeeds under that root;
    /// supplied & invalid → `inbox.invalid_destination_root`.
    #[tokio::test]
    async fn inbox_multi_candidate_requires_selection() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits(
            tmp.path(),
            "light_001.fits",
            "Light Frame",
            Some("M42"),
            Some("Ha"),
            Some("2025-10-10T22:00:00"),
        );

        let db = test_db().await;

        let bus = make_bus(&db);
        register_source_full(&db, "root-inbox", "inbox", "/inbox", "unorganized").await;
        register_source_full(&db, "root-lib-a", "light_frames", "/a", "unorganized").await;
        register_source_full(&db, "root-lib-b", "light_frames", "/b", "unorganized").await;
        setup_classified_item_rooted(
            &db,
            "item-2cand",
            "root-inbox",
            Some("light"),
            "sig-2c",
            &["light_001.fits"],
        )
        .await;

        let mk = |root_id: Option<String>| ConfirmRequest {
            inbox_item_id: "item-2cand".to_owned(),
            content_signature: "sig-2c".to_owned(),
            destructive_destination: None,
            root_absolute_path: tmp.path().to_owned(),
            root_id,
            chosen_attribution: None,
        };

        // Absent selection → blocking error listing both candidates.
        let err = confirm(db.pool(), &bus, mk(None)).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::InboxDestinationRootRequired);
        let candidates = err.details.0.get("candidates").and_then(|c| c.as_array()).unwrap();
        assert_eq!(candidates.len(), 2, "both library roots offered as candidates");

        // Invalid selection (an id that is not a candidate) → rejected.
        let err = confirm(db.pool(), &bus, mk(Some("root-inbox".to_owned()))).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::InboxInvalidDestinationRoot);

        // Valid selection → plan lands under the chosen root.
        let resp = confirm(db.pool(), &bus, mk(Some("root-lib-b".to_owned()))).await.unwrap();
        assert_eq!(resp.destinations.len(), 1);
        assert_eq!(resp.destinations[0].to_root_id, "root-lib-b");
    }

    /// US8/FR-027: an inbox source whose frame-type category has no registered
    /// library root → `inbox.no_destination_root`.
    #[tokio::test]
    async fn inbox_no_candidate_blocks() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits(
            tmp.path(),
            "light_001.fits",
            "Light Frame",
            Some("M42"),
            Some("Ha"),
            Some("2025-10-10T22:00:00"),
        );

        let db = test_db().await;

        let bus = make_bus(&db);
        register_source_full(&db, "root-inbox", "inbox", "/inbox", "unorganized").await;
        // Only a calibration root exists; a light has no light_frames destination.
        register_source_full(&db, "root-cal", "calibration", "/cal", "unorganized").await;
        setup_classified_item_rooted(
            &db,
            "item-0cand",
            "root-inbox",
            Some("light"),
            "sig-0c",
            &["light_001.fits"],
        )
        .await;

        let err = confirm(
            db.pool(),
            &bus,
            ConfirmRequest {
                inbox_item_id: "item-0cand".to_owned(),
                content_signature: "sig-0c".to_owned(),
                destructive_destination: None,
                root_absolute_path: tmp.path().to_owned(),
                root_id: None,
                chosen_attribution: None,
            },
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::InboxNoDestinationRoot);
    }

    /// FR-026a: a raw dark lands under `darks/{exposure}/` with NO target/date
    /// segment.
    ///
    /// Spec 041 FR-050/T071: previously this and the master-flat case below
    /// were exercised in one multi-type "split" confirm over a single mixed
    /// item; that path is retired, so each frame type is now its own
    /// single-type "classified" item confirmed independently — matching what
    /// T066 materialization actually produces upstream of confirm.
    #[tokio::test]
    async fn calibration_dark_destination_omits_target() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits_exp(tmp.path(), "dark.fits", "Dark Frame", None, None, None, 300.0);

        let db = test_db().await;

        let bus = make_bus(&db);
        register_source_full(&db, "root-1", "calibration", "/cal", "unorganized").await;
        setup_classified_item(
            &db,
            "item-cal-dark",
            "classified",
            Some("dark"),
            "sig-cal-dark",
            &["dark.fits"],
        )
        .await;

        let resp = confirm(
            db.pool(),
            &bus,
            ConfirmRequest {
                inbox_item_id: "item-cal-dark".to_owned(),
                content_signature: "sig-cal-dark".to_owned(),
                destructive_destination: None,
                root_absolute_path: tmp.path().to_owned(),
                root_id: None,
                chosen_attribution: None,
            },
        )
        .await
        .unwrap();

        let dests: std::collections::HashMap<String, String> = resp
            .destinations
            .iter()
            .map(|d| (d.from_path.clone(), d.to_relative_path.clone()))
            .collect();
        assert_eq!(dests["dark.fits"], "darks/300/dark.fits");
    }

    /// FR-026a: a calibration master lands under its `masters/...` pattern with
    /// NO target/date segment. See `calibration_dark_destination_omits_target`
    /// for why this is now its own single-type confirm.
    #[tokio::test]
    async fn calibration_master_destination_omits_target() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits_exp(tmp.path(), "master_flat.fits", "Flat Frame", None, Some("Ha"), None, 5.0);

        let db = test_db().await;

        let bus = make_bus(&db);
        register_source_full(&db, "root-2", "calibration", "/cal", "unorganized").await;
        inbox_repo::insert_inbox_item(
            db.pool(),
            &InsertInboxItem {
                id: "item-cal-master",
                root_id: "root-2",
                relative_path: "",
                file_count: 1,
                content_signature: Some("sig-cal-master"),
                lane: "fits",
            },
        )
        .await
        .unwrap();
        inbox_repo::upsert_classification(
            db.pool(),
            &UpsertClassification {
                inbox_item_id: "item-cal-master",
                result: "classified",
                frame_type: Some("flat"),
                content_signature: "sig-cal-master",
                unclassified_file_count: 0,
            },
        )
        .await
        .unwrap();
        inbox_repo::insert_evidence(
            db.pool(),
            &InsertEvidence {
                id: "ev-cal-master-flat",
                inbox_item_id: "item-cal-master",
                relative_file_path: "master_flat.fits",
                frame_type: Some("flat"),
                evidence_source: "imagetyp_header",
                raw_value: None,
                unclassified: false,
                manual_override: None,
                is_master: true,
                master_detector: None,
            },
        )
        .await
        .unwrap();

        let resp = confirm(
            db.pool(),
            &bus,
            ConfirmRequest {
                inbox_item_id: "item-cal-master".to_owned(),
                content_signature: "sig-cal-master".to_owned(),
                destructive_destination: None,
                root_absolute_path: tmp.path().to_owned(),
                root_id: None,
                chosen_attribution: None,
            },
        )
        .await
        .unwrap();

        let dests: std::collections::HashMap<String, String> = resp
            .destinations
            .iter()
            .map(|d| (d.from_path.clone(), d.to_relative_path.clone()))
            .collect();
        assert_eq!(dests["master_flat.fits"], "masters/flats/Ha/master_flat.fits");
    }

    /// US9/FR-032/FR-033: a light missing its date blocks plan generation with
    /// `inbox.missing_path_attributes`, reporting the missing attribute.
    #[tokio::test]
    async fn missing_path_attribute_blocks_with_report() {
        let tmp = tempfile::tempdir().unwrap();
        // Light with target + filter but NO DATE-OBS → date is path-load-bearing.
        write_fits(tmp.path(), "light_001.fits", "Light Frame", Some("M42"), Some("Ha"), None);

        let db = test_db().await;

        let bus = make_bus(&db);
        register_source_full(&db, "root-1", "light_frames", "/lib", "unorganized").await;
        setup_classified_item(
            &db,
            "item-gate",
            "classified",
            Some("light"),
            "sig-gate",
            &["light_001.fits"],
        )
        .await;

        let err = confirm(
            db.pool(),
            &bus,
            ConfirmRequest {
                inbox_item_id: "item-gate".to_owned(),
                content_signature: "sig-gate".to_owned(),
                destructive_destination: None,
                root_absolute_path: tmp.path().to_owned(),
                root_id: None,
                chosen_attribution: None,
            },
        )
        .await
        .unwrap_err();

        assert_eq!(err.code, ErrorCode::InboxMissingPathAttributes);
        let files = err.details.0.get("files").and_then(|f| f.as_array()).unwrap();
        let attrs = files[0].get("missingPathAttributes").and_then(|a| a.as_array()).unwrap();
        assert!(
            attrs.iter().any(|a| a.as_str() == Some("date")),
            "missing 'date' must be reported, got {attrs:?}"
        );
    }

    // ── T070 tests — needs-review sentinel gate at confirm ───────────────────

    /// T070/FR-049/SC-015: confirm of an item whose group_key is the sentinel
    /// flagged `needs_review` must be rejected with InboxMissingPathAttributes.
    #[tokio::test]
    async fn t070_confirm_of_needs_review_item_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        // Write a light without mandatory attrs — doesn't matter here since
        // the gate fires on group_key before reaching file resolution.
        write_fits(tmp.path(), "light_001.fits", "Light Frame", Some("M42"), Some("Ha"), None);

        let db = test_db().await;

        let bus = make_bus(&db);
        register_source_full(&db, "root-1", "light_frames", "/lib", "unorganized").await;

        // Insert the inbox item flagged needs_review directly. The gate reads
        // `item.needs_review` (spec 058 FR-028), so we bypass
        // setup_classified_item and set the column explicitly via raw SQL.
        let item_id = "item-t070-sentinel";
        let sig = "sig-t070-sentinel";
        inbox_repo::insert_inbox_item(
            db.pool(),
            &InsertInboxItem {
                id: item_id,
                root_id: "root-1",
                relative_path: "",
                file_count: 1,
                content_signature: Some(sig),
                lane: "fits",
            },
        )
        .await
        .unwrap();
        sqlx::query("UPDATE inbox_items SET needs_review = 1 WHERE id = ?")
            .bind(item_id)
            .execute(db.pool())
            .await
            .unwrap();
        // Add a classification so the item doesn't fail on "no classification".
        inbox_repo::upsert_classification(
            db.pool(),
            &UpsertClassification {
                inbox_item_id: item_id,
                result: "classified",
                frame_type: None,
                content_signature: sig,
                unclassified_file_count: 1,
            },
        )
        .await
        .unwrap();

        let err = confirm(
            db.pool(),
            &bus,
            ConfirmRequest {
                inbox_item_id: item_id.to_owned(),
                content_signature: sig.to_owned(),
                destructive_destination: None,
                root_absolute_path: tmp.path().to_owned(),
                root_id: None,
                chosen_attribution: None,
            },
        )
        .await
        .unwrap_err();

        assert_eq!(
            err.code,
            ErrorCode::InboxMissingPathAttributes,
            "needs-review item must be rejected with InboxMissingPathAttributes"
        );
        assert!(
            err.message.contains("needs-review"),
            "error message must mention needs-review, got: {}",
            err.message
        );
    }

    /// T070: a fully-resolved item (with all mandatory attributes satisfied via
    /// the active pattern) confirms successfully and produces a plan.
    #[tokio::test]
    async fn t070_fully_resolved_item_confirms_successfully() {
        let tmp = tempfile::tempdir().unwrap();
        // Light with object + filter + date (no EXPTIME needed for the gate here;
        // the pattern gate fires on date/target in the existing test).
        write_fits(
            tmp.path(),
            "light_001.fits",
            "Light Frame",
            Some("M42"),
            Some("Ha"),
            Some("2024-11-01"),
        );

        let db = test_db().await;

        let bus = make_bus(&db);
        register_source_full(&db, "root-1", "light_frames", "/lib", "unorganized").await;
        setup_classified_item(
            &db,
            "item-t070-ok",
            "classified",
            Some("light"),
            "sig-t070-ok",
            &["light_001.fits"],
        )
        .await;

        // A fully-resolved item (needs_review defaults to 0 in the helper) must
        // pass the needs-review gate.
        // Same registered-source / classified-item shape as
        // `non_inbox_source_moves_in_place`, which deterministically succeeds —
        // a fully-resolved item must confirm cleanly, not merely "not fail on
        // the sentinel gate".
        let resp = confirm(
            db.pool(),
            &bus,
            ConfirmRequest {
                inbox_item_id: "item-t070-ok".to_owned(),
                content_signature: "sig-t070-ok".to_owned(),
                destructive_destination: None,
                root_absolute_path: tmp.path().to_owned(),
                root_id: None,
                chosen_attribution: None,
            },
        )
        .await
        .expect("fully-resolved item must confirm successfully and produce a plan");

        assert!(!resp.plan_id.is_empty(), "confirm must produce a plan id");
        assert_eq!(resp.items_total, 1, "one classified file must produce one plan item");
        assert_eq!(resp.move_count, 1, "unorganized source must produce a move item");
        assert_eq!(resp.catalogue_count, 0, "unorganized source must not catalogue in place");
    }

    // ── inventory.confirmed publish (review fix: swallow-on-failure) ─────────

    #[tokio::test]
    async fn confirm_publishes_inventory_confirmed_on_success() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits(
            tmp.path(),
            "light_001.fits",
            "Light Frame",
            Some("M42"),
            Some("Ha"),
            Some("2025-10-10T22:00:00"),
        );

        let db = test_db().await;
        let bus = make_bus(&db);
        let mut rx = bus.subscribe();

        setup_classified_item(
            &db,
            "item-evt1",
            "classified",
            Some("light"),
            "sig-evt1",
            &["light_001.fits"],
        )
        .await;

        let resp = confirm(
            db.pool(),
            &bus,
            ConfirmRequest {
                inbox_item_id: "item-evt1".to_owned(),
                content_signature: "sig-evt1".to_owned(),
                destructive_destination: None,
                root_absolute_path: tmp.path().to_owned(),
                root_id: None,
                chosen_attribution: None,
            },
        )
        .await
        .unwrap();

        let envelope =
            rx.try_recv().expect("inventory.confirmed must be published on a successful confirm");
        assert_eq!(envelope.topic, TOPIC_INVENTORY_CONFIRMED);
        assert_eq!(envelope.payload["inboxItemId"], "item-evt1");
        assert_eq!(envelope.payload["planId"], resp.plan_id);
    }

    #[tokio::test]
    async fn confirm_succeeds_even_when_publish_fails() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits(
            tmp.path(),
            "light_001.fits",
            "Light Frame",
            Some("M42"),
            Some("Ha"),
            Some("2025-10-10T22:00:00"),
        );

        let db = test_db().await;
        setup_classified_item(
            &db,
            "item-evt2",
            "classified",
            Some("light"),
            "sig-evt2",
            &["light_001.fits"],
        )
        .await;

        // A bus backed by a pool with no `events` table: `bus.publish` fails
        // durably (BusError::Database). confirm()'s own writes still go
        // through the properly-migrated `db.pool()`, isolating the publish
        // failure from confirm's transactional work.
        let bad_pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let bad_bus = EventBus::with_pool(bad_pool);

        let resp = confirm(
            db.pool(),
            &bad_bus,
            ConfirmRequest {
                inbox_item_id: "item-evt2".to_owned(),
                content_signature: "sig-evt2".to_owned(),
                destructive_destination: None,
                root_absolute_path: tmp.path().to_owned(),
                root_id: None,
                chosen_attribution: None,
            },
        )
        .await
        .expect("confirm must succeed even when the audit bus publish fails");

        assert!(!resp.plan_id.is_empty(), "plan must still be created");

        // The transactional work landed despite the publish failure.
        let link = inbox_repo::get_plan_link(db.pool(), "item-evt2").await.unwrap();
        assert!(link.is_some(), "plan link must be created despite publish failure");
    }

    // ── Attribution wiring (spec 008 Q27, F-Framing-5/10) ────────────────────

    #[tokio::test]
    async fn confirm_returns_new_project_fallback_candidate_for_a_light_item_with_no_geometry() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits(
            tmp.path(),
            "light_001.fits",
            "Light Frame",
            Some("M42"),
            Some("Ha"),
            Some("2025-10-10T22:00:00"),
        );
        let db = test_db().await;
        setup_classified_item(
            &db,
            "item-attr1",
            "classified",
            Some("light"),
            "sig-attr1",
            &["light_001.fits"],
        )
        .await;

        let resp = confirm(
            db.pool(),
            &make_bus(&db),
            ConfirmRequest {
                inbox_item_id: "item-attr1".to_owned(),
                content_signature: "sig-attr1".to_owned(),
                destructive_destination: None,
                root_absolute_path: tmp.path().to_owned(),
                root_id: None,
                chosen_attribution: None,
            },
        )
        .await
        .unwrap();

        // `write_fits` in this test module does not carry TELESCOP/INSTRUME/
        // FOCALLEN/RA/DEC — the item has no staged geometry, so the pass
        // yields only the trailing new_project fallback (never an error: an
        // ungeometried light item still confirms normally).
        assert_eq!(resp.attribution_candidates.len(), 1);
        assert_eq!(
            resp.attribution_candidates[0].kind,
            contracts_core::framing::IngestionAttributionKind::NewProject
        );
        assert!(resp.attribution_applied.is_none());
    }

    #[tokio::test]
    async fn confirm_rejects_chosen_attribution_for_a_non_light_item() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits(tmp.path(), "dark_001.fits", "Dark Frame", None, None, None);
        let db = test_db().await;
        setup_classified_item(
            &db,
            "item-attr-dark",
            "classified",
            Some("dark"),
            "sig-attr-dark",
            &["dark_001.fits"],
        )
        .await;

        let err = confirm(
            db.pool(),
            &make_bus(&db),
            ConfirmRequest {
                inbox_item_id: "item-attr-dark".to_owned(),
                content_signature: "sig-attr-dark".to_owned(),
                destructive_destination: None,
                root_absolute_path: tmp.path().to_owned(),
                root_id: None,
                chosen_attribution: Some(contracts_core::framing::ChosenAttributionDto {
                    kind: contracts_core::framing::ChosenAttributionKind::Unassigned,
                    project_id: None,
                    framing_id: None,
                }),
            },
        )
        .await
        .unwrap_err();

        assert_eq!(err.code, ErrorCode::AttributionNotLightFrame);
    }

    /// F-Framing-10 (FR-022): a `chosen_attribution` picking an existing
    /// framing is applied and persisted on the plan `confirm` itself created —
    /// the apply-path's whole point (the response's `attribution_applied` and
    /// the durable `plans.chosen_framing_id` must agree).
    #[tokio::test]
    async fn confirm_applies_add_to_framing_and_persists_the_pick_on_its_own_plan() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits(
            tmp.path(),
            "light_001.fits",
            "Light Frame",
            Some("M42"),
            Some("Ha"),
            Some("2025-10-10T22:00:00"),
        );
        let db = test_db().await;
        setup_classified_item(
            &db,
            "item-attr2",
            "classified",
            Some("light"),
            "sig-attr2",
            &["light_001.fits"],
        )
        .await;

        persistence_plans::repositories::projects::insert_project(
            db.pool(),
            &persistence_plans::repositories::projects::InsertProject {
                id: "proj-attr2",
                name: "M42 project",
                tool: "PixInsight",
                lifecycle: "ready",
                path: "projects/proj-attr2",
                notes: None,
                canonical_target_id: None,
                is_mosaic: false,
            },
        )
        .await
        .unwrap();
        persistence_targets::repositories::framing::insert_framing(
            db.pool(),
            &persistence_targets::repositories::framing::InsertFraming {
                id: "framing-attr2",
                project_id: "proj-attr2",
                target_id: None,
                optic_train_key: "scope|cam|400",
                pointing_ra_deg: 10.0,
                pointing_dec_deg: 20.0,
                rotation_deg: 0.0,
                tolerance_pointing: 0.1,
                tolerance_rotation_deg: 3.0,
                clustering: "suggested",
            },
        )
        .await
        .unwrap();

        let resp = confirm(
            db.pool(),
            &make_bus(&db),
            ConfirmRequest {
                inbox_item_id: "item-attr2".to_owned(),
                content_signature: "sig-attr2".to_owned(),
                destructive_destination: None,
                root_absolute_path: tmp.path().to_owned(),
                root_id: None,
                chosen_attribution: Some(contracts_core::framing::ChosenAttributionDto {
                    kind: contracts_core::framing::ChosenAttributionKind::AddToFraming,
                    project_id: None,
                    framing_id: Some("framing-attr2".to_owned()),
                }),
            },
        )
        .await
        .unwrap();

        let applied = resp.attribution_applied.expect("chosen_attribution must apply");
        assert_eq!(applied.project_id, "proj-attr2");
        assert_eq!(applied.framing_id.as_deref(), Some("framing-attr2"));
        assert!(!applied.reopened);

        assert_eq!(
            plans_repo::get_chosen_framing_id(db.pool(), &resp.plan_id).await.unwrap().as_deref(),
            Some("framing-attr2"),
            "the apply-path must persist the pick on the plan confirm() itself created"
        );
    }

    /// Frame-to-framing attribution is a Tier-1 record (Constitution V): it
    /// cannot be re-derived from the filesystem. A confirm whose attribution
    /// fails must therefore leave no plan ready to apply, or the frames move
    /// with the user's decision missing.
    #[tokio::test]
    async fn failed_attribution_leaves_no_plan_ready_to_apply() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits(
            tmp.path(),
            "light_001.fits",
            "Light Frame",
            Some("M42"),
            Some("Ha"),
            Some("2025-10-10T22:00:00"),
        );
        let db = test_db().await;
        setup_classified_item(
            &db,
            "item-attr-fail",
            "classified",
            Some("light"),
            "sig-attr-fail",
            &["light_001.fits"],
        )
        .await;

        let err = confirm(
            db.pool(),
            &make_bus(&db),
            ConfirmRequest {
                inbox_item_id: "item-attr-fail".to_owned(),
                content_signature: "sig-attr-fail".to_owned(),
                destructive_destination: None,
                root_absolute_path: tmp.path().to_owned(),
                root_id: None,
                chosen_attribution: Some(contracts_core::framing::ChosenAttributionDto {
                    kind: contracts_core::framing::ChosenAttributionKind::AddToFraming,
                    project_id: None,
                    framing_id: Some("framing-does-not-exist".to_owned()),
                }),
            },
        )
        .await
        .expect_err("a failed Tier-1 attribution must fail the confirm");

        assert_eq!(err.code, ErrorCode::FramingNotFound);
        assert_eq!(
            count_plans_in_state(db.pool(), "ready_for_review").await,
            0,
            "a failed attribution must leave no plan ready to apply"
        );
        assert!(inbox_repo::get_plan_link(db.pool(), "item-attr-fail").await.unwrap().is_none());
        let item = inbox_repo::get_inbox_item(db.pool(), "item-attr-fail").await.unwrap();
        assert_ne!(item.state, "plan_open");
    }

    // ── #1342: equipment resolution on the ingest path ────────────────────

    /// Confirm a one-light item whose header names `instrume`, under a
    /// `{camera}/light/` destination pattern, and return the destination
    /// directory the plan recorded.
    async fn confirm_camera_dest_dir(db: &Database, instrume: &str, item: &str) -> String {
        let tmp = tempfile::tempdir().unwrap();
        write_fits_cards(
            tmp.path(),
            "frame_000.fits",
            "Light Frame",
            Some("M42"),
            Some("Ha"),
            Some("2025-10-10T22:00:00"),
            None,
            Some(instrume),
        );
        settings_repo::set_raw(
            db.pool(),
            settings_repo::PATTERNS_BY_TYPE_KEY,
            &serde_json::json!({"light": "{camera}/light/"}),
        )
        .await
        .unwrap();

        let bus = make_bus(db);
        let sig = format!("sig-{item}");
        setup_classified_item(db, item, "classified", Some("light"), &sig, &["frame_000.fits"])
            .await;

        let resp = confirm(
            db.pool(),
            &bus,
            ConfirmRequest {
                inbox_item_id: item.to_owned(),
                content_signature: sig,
                destructive_destination: None,
                root_absolute_path: tmp.path().to_owned(),
                root_id: None,
                chosen_attribution: None,
            },
        )
        .await
        .unwrap();

        let items =
            persistence_plans::repositories::plans::list_plan_items(db.pool(), &resp.plan_id)
                .await
                .unwrap();
        dest_dir(items.first().expect("confirm must record a plan item"))
    }

    /// #1342: a header string claimed by a registered camera's alias resolves
    /// to that camera's name, so the destination directory carries the name
    /// the user chose rather than whatever the capture program wrote. Case and
    /// padding differ here to pin the normalized match.
    #[tokio::test]
    async fn confirm_resolves_a_registered_camera_alias_to_its_name() {
        let db = test_db().await;
        equipment_repo::create_camera(
            db.pool(),
            &contracts_core::equipment::CreateCamera {
                name: "Main Imaging Rig".to_owned(),
                aliases: vec!["ASI2600MM".to_owned()],
                sensor_type: None,
                passband: None,
                pixel_size_um: None,
                sensor_width_px: None,
                sensor_height_px: None,
            },
        )
        .await
        .unwrap();

        let dir = confirm_camera_dest_dir(&db, " asi2600mm ", "item-cam-hit").await;
        assert_eq!(
            dir, "Main Imaging Rig/light",
            "a registered alias must resolve to the camera's name in the destination path"
        );
    }

    /// #1342: an unregistered header string keeps its raw spelling AND does
    /// not register a camera. Ingest resolves equipment; it never creates it
    /// (`find_or_create_camera_by_alias` stays uncalled here) — auto-creating
    /// one camera per distinct spelling would defeat the alias model the
    /// registry exists to provide.
    #[tokio::test]
    async fn confirm_leaves_an_unregistered_camera_raw_and_registers_nothing() {
        let db = test_db().await;
        equipment_repo::create_camera(
            db.pool(),
            &contracts_core::equipment::CreateCamera {
                name: "Main Imaging Rig".to_owned(),
                aliases: vec!["ASI2600MM".to_owned()],
                sensor_type: None,
                passband: None,
                pixel_size_um: None,
                sensor_width_px: None,
                sensor_height_px: None,
            },
        )
        .await
        .unwrap();

        let dir = confirm_camera_dest_dir(&db, "ASI6200MM Pro", "item-cam-miss").await;
        assert_eq!(
            dir, "ASI6200MM Pro/light",
            "an unclaimed header string must stay raw rather than resolve to another camera"
        );

        let cameras = equipment_repo::list_cameras(db.pool()).await.unwrap();
        assert_eq!(
            cameras.len(),
            1,
            "confirm must not register a camera for an unclaimed header string"
        );
        assert!(
            !cameras.iter().any(|c| c.auto_detected),
            "confirm must not write auto-detected equipment rows"
        );
    }

    /// Read the single plan item's frozen provenance for `plan_id`.
    async fn read_item_provenance(db: &Database, plan_id: &str) -> Vec<ProvenanceEntry> {
        let raw = sqlx::query_as::<_, (Option<String>,)>(
            "SELECT provenance FROM plan_items WHERE plan_id = ?",
        )
        .bind(plan_id)
        .fetch_one(db.pool())
        .await
        .unwrap();
        let json = raw.0.expect("confirm must write plan_items.provenance");
        serde_json::from_str(&json).unwrap()
    }

    fn prov(entries: &[ProvenanceEntry], label: &str) -> Option<String> {
        entries.iter().find(|e| e.label == label).map(|e| e.value.clone())
    }

    async fn confirm_defaults(
        db: &Database,
        bus: &EventBus,
        item_id: &str,
        sig: &str,
        root_absolute_path: &std::path::Path,
    ) -> ConfirmResponse {
        confirm(
            db.pool(),
            bus,
            ConfirmRequest {
                inbox_item_id: item_id.to_owned(),
                content_signature: sig.to_owned(),
                destructive_destination: None,
                root_absolute_path: root_absolute_path.to_owned(),
                root_id: None,
                chosen_attribution: None,
            },
        )
        .await
        .unwrap()
    }

    /// Spec 002 FR-005 at the spec 041 Inbox confirm gate: the inferred context
    /// behind an approved destination is frozen on the plan item, so a later
    /// rescan that reads different headers cannot rewrite the record of what
    /// was known at approval time — while still producing a new snapshot of
    /// its own.
    #[tokio::test]
    async fn confirm_provenance_survives_later_metadata_change() {
        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("light_001.fits");
        write_fits(
            tmp.path(),
            "light_001.fits",
            "Light Frame",
            Some("M42"),
            Some("Ha"),
            Some("2025-10-10T22:00:00"),
        );

        let db = test_db().await;
        let bus = make_bus(&db);
        register_source_org_state(&db, "root-1", "light_frames", "unorganized").await;
        setup_classified_item(
            &db,
            "item-prov",
            "classified",
            Some("light"),
            "sig-prov",
            &["light_001.fits"],
        )
        .await;

        let first = confirm_defaults(&db, &bus, "item-prov", "sig-prov", tmp.path()).await;

        let approved = read_item_provenance(&db, &first.plan_id).await;
        assert_eq!(prov(&approved, "target").as_deref(), Some("M42"));
        assert_eq!(prov(&approved, "filter").as_deref(), Some("Ha"));
        assert_eq!(prov(&approved, "date").as_deref(), Some("2025-10-10"));
        assert_eq!(prov(&approved, "frame_type").as_deref(), Some("light"));
        assert_eq!(prov(&approved, "is_master").as_deref(), Some("false"));
        assert!(
            prov(&approved, "destination_pattern").is_some_and(|p| !p.is_empty()),
            "a moved item records the pattern its destination was resolved from"
        );

        // Rescan simulation: the same file now reports different headers.
        // The metadata cache is keyed by (path, mtime, size), so this rewrite
        // also changes the byte length — a same-second rewrite of identical
        // length would be a cache hit and would make the "the live value really
        // did change" leg below vacuous rather than failing loudly.
        write_fits(
            tmp.path(),
            "light_001.fits",
            "Light Frame",
            Some("NGC7000"),
            Some("Oiii"),
            Some("2025-12-31T21:00:00"),
        );
        std::fs::OpenOptions::new()
            .append(true)
            .open(&file_path)
            .unwrap()
            .write_all(&[b' '; 2880])
            .unwrap();
        assert_eq!(
            cached_extract(&file_path).unwrap().object.as_deref(),
            Some("NGC7000"),
            "precondition: the live metadata must actually have changed"
        );

        let after_rescan = read_item_provenance(&db, &first.plan_id).await;
        assert_eq!(
            prov(&after_rescan, "target").as_deref(),
            Some("M42"),
            "the approved snapshot must still report what was known at approval time"
        );
        assert_eq!(prov(&after_rescan, "filter").as_deref(), Some("Ha"));
        assert_eq!(prov(&after_rescan, "date").as_deref(), Some("2025-10-10"));

        // FR-005 also requires later rescans to produce their own snapshots.
        // A second root: `inbox_items` is unique on (root_id, relative_path,
        // group_key), so the rescanned item cannot reuse root-1.
        // `registered_sources` is unique on (kind, path), so root-2 needs its
        // own path rather than `register_source_org_state`'s fixed one.
        sqlx::query(
            "INSERT INTO registered_sources
                (id, kind, path, kind_subtype, scan_depth, created_at, created_via, organization_state)
             VALUES ('root-2', 'light_frames', '/tmp/src-2', NULL, 'recursive',
                     '2026-01-01T00:00:00Z', 'first_run', 'unorganized')",
        )
        .execute(db.pool())
        .await
        .unwrap();
        setup_classified_item_rooted(
            &db,
            "item-prov-2",
            "root-2",
            Some("light"),
            "sig-prov-2",
            &["light_001.fits"],
        )
        .await;
        let second = confirm_defaults(&db, &bus, "item-prov-2", "sig-prov-2", tmp.path()).await;
        let rescanned = read_item_provenance(&db, &second.plan_id).await;
        assert_eq!(prov(&rescanned, "target").as_deref(), Some("NGC7000"));
        assert_eq!(prov(&rescanned, "filter").as_deref(), Some("Oiii"));
    }
}
