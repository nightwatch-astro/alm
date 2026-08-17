// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `inbox.classify` use case (spec 005, T014/T016/T020).
//!
//! Reads cached classification when `content_signature` matches; falls back
//! to metadata adapters on miss or `force_rescan`. Normalizes IMAGETYP via
//! `ImageTypNormalizationTable`. Marks files `unclassified = true` when
//! IMAGETYP is absent or unmapped.
//!
//! This module is pure orchestration: DB reads/writes via
//! `persistence_inbox::repositories::inbox`; metadata reads via
//! `metadata_fits::FitsExtractor` / `metadata_xisf::XisfExtractor`.
//!
//! # Write-failure policy (issue #1101)
//!
//! Writes here split into two classes and are handled differently:
//!
//! - **Load-bearing** — the classification result itself (the stale-row wipe
//!   that the fresh rows assume, the evidence rows, the classification cache
//!   row, the item's scan columns and its `state = 'classified'`). A failure
//!   propagates: returning `Ok` would report a classification the database
//!   does not hold, and the `state` write in particular would leave a false
//!   statement in the DB (#711 badge disagreement).
//! - **Best-effort** — rows that are re-derivable from the next classify run
//!   (per-file metadata seeded for materialized sub-items, override
//!   restoration and staleness flags, breakdown cache rows, orphan sub-item
//!   cleanup, denormalized child counts). These keep going, but every failure
//!   is logged with the item id and path so a partial classify is diagnosable
//!   after the fact instead of surfacing as a data bug months later.
#![allow(clippy::doc_markdown)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use app_core_errors::db_internal_ctx;
use app_core_targets::metadata_cache::cached_extract;
use calibration_master_detect::{detect_master, DetectInput};
use camino::Utf8Path;
use metadata_core::{
    v1_normalization_table, EvidenceSource, FrameType, ImageTypNormalizationTable,
};

use super::grouping::{group_file, FrameMetadata, GroupingConfig};
use super::signature::folder_signature;
use persistence_inbox::repositories::inbox::{
    self as repo, InsertEvidence, UpsertClassification, UpsertFileMetadata, UpsertInboxSubItem,
};
use persistence_inbox::repositories::q_inbox;
use sqlx::{SqliteConnection, SqlitePool};
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

use contracts_core::error_code::ErrorCode;
use contracts_core::ContractError;
use contracts_core::ErrorSeverity;

// ── Response types ────────────────────────────────────────────────────────────

/// Per-frame-type breakdown entry.
#[derive(Clone, Debug, serde::Serialize)]
pub struct BreakdownEntry {
    pub kind: String,
    pub count: usize,
    pub destination_preview: Option<String>,
    pub sample_files: Vec<String>,
}

/// Response from the classify use case.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ClassifyResponse {
    pub inbox_item_id: String,
    /// API vocabulary (stable for frontend): "single_type" | "unclassified".
    /// Note: the DB stores "classified" / "unclassified" (migration 0049 CHECK);
    /// the API vocabulary is mapped from the DB value so the frontend contract
    /// stays unchanged until T071/T072 update the contracts.
    pub classification_type: String,
    /// Present when type == "single_type"
    pub frame_type: Option<String>,
    pub content_signature: String,
    pub breakdown: Vec<BreakdownEntry>,
    pub unclassified_files: Vec<String>,
    pub sample_files: Vec<String>,
    pub computed_at: String,
}

// ── Request ───────────────────────────────────────────────────────────────────

pub struct ClassifyRequest {
    pub inbox_item_id: String,
    /// Path to the root of the inbox root (needed to compute absolute file paths).
    pub root_absolute_path: PathBuf,
    pub force_rescan: bool,
}

// ── classify ──────────────────────────────────────────────────────────────────

/// Per-file intermediate state for the classify hot path: classification result
/// plus pre-collected file identity (stat done outside any DB transaction).
struct FileWork {
    fc: FileClassification,
    file_size_bytes: Option<i64>,
    file_mtime: Option<String>,
}

/// Run or retrieve a classification for an inbox item.
///
/// # Errors
/// Returns a `ContractError` with appropriate error codes.
#[allow(clippy::too_many_lines)]
pub async fn classify(
    pool: &SqlitePool,
    req: ClassifyRequest,
) -> Result<ClassifyResponse, ContractError> {
    // 1. Fetch the inbox item
    let item = repo::get_inbox_item(pool, &req.inbox_item_id).await.map_err(|_| {
        ContractError::new(
            ErrorCode::InboxItemNotFound,
            format!("InboxItem not found: {}", req.inbox_item_id),
            ErrorSeverity::Blocking,
            false,
        )
    })?;

    // 2. Check cache unless force_rescan
    if !req.force_rescan {
        if let Some(cached) =
            repo::get_classification(pool, &req.inbox_item_id).await.ok().flatten()
        {
            // If sig matches, return from cache
            if item.content_signature.as_deref() == Some(&cached.content_signature) {
                return build_response_from_cache(pool, &item, &cached).await;
            }
        }
    }

    // 3. Build absolute path for this item.  Sub-frame groups are folders; a
    //    detected calibration master (spec 040) is a single file.
    let folder_abs = req.root_absolute_path.join(&item.relative_path);

    // 4. Enumerate the FITS/XISF files to classify.  Master items are one file,
    //    not a directory, so reading them as a folder finds nothing — enumerate
    //    the file itself instead (this is why masters were stuck "Loading
    //    classification" forever).
    let file_paths = if item.is_master_item != 0 {
        if folder_abs.is_file() {
            vec![folder_abs.clone()]
        } else {
            Vec::new()
        }
    } else {
        enumerate_fits_files(&folder_abs)
    };
    if file_paths.is_empty() {
        return Err(ContractError::new(
            ErrorCode::MetadataUnreadable,
            format!("No FITS/XISF files found for item: {}", folder_abs.display()),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    // 5. Compute folder content signature
    let sig_refs: Vec<&Path> = file_paths.iter().map(PathBuf::as_path).collect();
    let content_signature = super::signature::compute_content_signature(&sig_refs);

    // 6. Run metadata extraction + classification
    let norm_table = v1_normalization_table();

    // spec 041 R-4 / T025: before wiping evidence, snapshot per-path override
    // values and detect which files have changed identity (size/mtime). After
    // the fresh evidence re-insert both are re-applied: overrides are
    // re-written for all paths still present in the scan, and the subset whose
    // identity changed is additionally marked override_stale = 1.
    let override_snapshot =
        snapshot_overrides(pool, &req.inbox_item_id, &file_paths, &req.root_absolute_path).await;

    // ── Collect all file-level data before opening any transaction ──────────
    //
    // Stat calls (inside `classify_one_file` via `cached_extract`, and the
    // explicit `fs::metadata` for file identity below) run here, outside the
    // write transaction, so the transaction never blocks on slow filesystem I/O
    // (constitution §II: "transaction must not span fs I/O unnecessarily long").

    let mut file_work: Vec<FileWork> = Vec::with_capacity(file_paths.len());
    for abs_path in &file_paths {
        let fc = classify_one_file(abs_path, &req.root_absolute_path, &norm_table);
        // #549/#1286: skip extracted masters when classifying the parent folder.
        if fc.is_master && item.is_master_item == 0 {
            continue;
        }
        // Collect file identity for metadata staleness (spec 041 R-4).
        let (file_size_bytes, file_mtime) = match std::fs::metadata(abs_path) {
            Ok(md) => {
                let size = i64::try_from(md.len()).ok();
                let mtime = md
                    .modified()
                    .ok()
                    .and_then(|t| time::OffsetDateTime::from(t).format(&Rfc3339).ok());
                (size, mtime)
            }
            Err(_) => (None, None),
        };
        file_work.push(FileWork { fc, file_size_bytes, file_mtime });
    }

    // ── Build batch payloads (owned strings) ─────────────────────────────────
    //
    // Binding lifetimes require owned storage for anything derived from `fc`;
    // IDs must outlive the batch slice passed to insert_evidence_batch.
    let ev_ids: Vec<String> = file_work.iter().map(|_| Uuid::new_v4().to_string()).collect();
    let ev_batch: Vec<InsertEvidence<'_>> = file_work
        .iter()
        .zip(ev_ids.iter())
        .map(|(w, id)| InsertEvidence {
            id: id.as_str(),
            inbox_item_id: &req.inbox_item_id,
            relative_file_path: &w.fc.relative_path,
            frame_type: w.fc.frame_type.map(FrameType::as_str),
            evidence_source: w.fc.evidence_source.as_str(),
            raw_value: w.fc.raw_value.as_deref(),
            unclassified: w.fc.is_unclassified,
            manual_override: None,
            is_master: w.fc.is_master,
            master_detector: w.fc.master_detector,
        })
        .collect();

    let meta_batch: Vec<UpsertFileMetadata<'_>> = file_work
        .iter()
        .map(|w| {
            build_file_metadata_upsert(
                &req.inbox_item_id,
                w.fc.raw_meta.as_deref(),
                &w.fc.relative_path,
                w.file_size_bytes,
                w.file_mtime.as_deref(),
            )
        })
        .collect();

    // ── T066: per-file records for sub-item grouping ──────────────────────────
    let mut file_records: Vec<(
        String,
        Option<FrameType>,
        Option<std::sync::Arc<metadata_core::RawFileMetadata>>,
    )> = file_work
        .iter()
        .map(|w| (w.fc.relative_path.clone(), w.fc.frame_type, w.fc.raw_meta.clone()))
        .collect();

    // ── Open a single write transaction for all load-bearing inserts ──────────
    let mut tx =
        pool.begin().await.map_err(|e| db_internal_ctx(e, "classify: begin transaction"))?;
    let conn: &mut SqliteConnection = &mut tx;

    // Delete stale evidence, breakdown, and per-file metadata.
    repo::delete_evidence_for_item_conn(conn, &req.inbox_item_id)
        .await
        .map_err(|e| db_internal_ctx(e, "classify: delete stale evidence"))?;
    repo::delete_breakdown_for_item_conn(conn, &req.inbox_item_id)
        .await
        .map_err(|e| db_internal_ctx(e, "classify: delete stale breakdown"))?;
    // spec 041 US2/T016: clear stale per-file metadata so removed files do not
    // leave orphaned rows behind after a re-scan.
    repo::delete_file_metadata_for_item_conn(conn, &req.inbox_item_id)
        .await
        .map_err(|e| db_internal_ctx(e, "classify: delete stale file metadata"))?;

    // Batch-insert all evidence rows in one (or a few chunked) statements.
    repo::insert_evidence_batch(conn, &ev_batch)
        .await
        .map_err(|e| db_internal_ctx(e, "classify: insert classification evidence"))?;

    // Batch-upsert all per-file metadata rows.
    repo::upsert_inbox_file_metadata_batch(conn, &meta_batch)
        .await
        .map_err(|e| db_internal_ctx(e, "classify: persist per-file metadata"))?;

    // Commit the first transaction (wipe + batch inserts). Subsequent
    // best-effort override re-application reads the committed evidence rows via
    // the pool, so they must be visible before those queries run. The
    // classification writes + sub-item materialization follow in a second
    // transaction below.
    tx.commit()
        .await
        .map_err(|e| db_internal_ctx(e, "classify: commit batch insert transaction"))?;

    // spec 041 R-4 / T025: re-apply snapshotted overrides to freshly-inserted
    // evidence rows, then mark stale the subset whose file identity changed.
    restore_overrides_after_wipe(pool, &req.inbox_item_id, &override_snapshot).await;

    // Layer the user's overrides onto the extraction-only records BEFORE the
    // folder tallies and the T066 split consume them (#854 CI race + the
    // rescan-reverts-overrides bug): without this, every classify run silently
    // reverted a reclassified file to its raw-header state — a file whose
    // frameType/exposureS only exist as overrides bounced back to the
    // needs-review sentinel on the very next classify. Priority per field
    // mirrors `reclassify_v2`'s own re-split layering:
    //   frame type — evidence `manual_override` (snapshot) → durable
    //                group-keyed 'frameType' override → extracted header;
    //   values     — group-keyed override → extracted header.
    let group_overrides = match item.source_group_id.as_deref() {
        Some(sg_id) => repo::list_file_overrides_for_group(pool, sg_id).await.unwrap_or_default(),
        None => Vec::new(),
    };
    let overrides_index: HashMap<(&str, &str), &str> = group_overrides
        .iter()
        .map(|o| ((o.relative_file_path.as_str(), o.property_key.as_str()), o.value.as_str()))
        .collect();
    let manual_by_path: HashMap<&str, &str> = override_snapshot
        .iter()
        .filter_map(|e| e.manual_override.as_deref().map(|m| (e.relative_file_path.as_str(), m)))
        .collect();

    for (rel, ft_opt, raw_opt) in &mut file_records {
        let fp = rel.clone();
        let fp = fp.as_str();

        let eff_ft_str = manual_by_path
            .get(fp)
            .copied()
            .or_else(|| overrides_index.get(&(fp, "frameType")).copied());
        if let Some(ft) = eff_ft_str.and_then(FrameType::from_str_ci) {
            *ft_opt = Some(ft);
        }

        // Value overrides feed the T070 mandatory gate and the grouping dims.
        if overrides_index.contains_key(&(fp, "exposureS"))
            || overrides_index.contains_key(&(fp, "filter"))
            || overrides_index.contains_key(&(fp, "gain"))
            || overrides_index.contains_key(&(fp, "target"))
            || overrides_index.contains_key(&(fp, "binning"))
            || overrides_index.contains_key(&(fp, "temperatureC"))
            || overrides_index.contains_key(&(fp, "offset"))
        {
            // Arc::make_mut clones only when the refcount > 1 (cache still holds
            // a handle); otherwise it is a no-op pointer cast.
            let raw = std::sync::Arc::make_mut(raw_opt.get_or_insert_with(|| {
                std::sync::Arc::new(metadata_core::RawFileMetadata::default())
            }));
            if let Some(v) = overrides_index.get(&(fp, "exposureS")) {
                raw.exposure = Some((*v).to_owned());
            }
            if let Some(v) = overrides_index.get(&(fp, "filter")) {
                raw.filter = Some((*v).to_owned());
            }
            if let Some(v) = overrides_index.get(&(fp, "gain")) {
                raw.gain = Some((*v).to_owned());
            }
            if let Some(v) = overrides_index.get(&(fp, "target")) {
                raw.object = Some((*v).to_owned());
            }
            if let Some(v) = overrides_index.get(&(fp, "binning")) {
                if let Some((bx, by)) = v.split_once('x') {
                    raw.x_binning = Some(bx.trim().to_owned());
                    raw.y_binning = Some(by.trim().to_owned());
                }
            }
            if let Some(v) = overrides_index.get(&(fp, "temperatureC")) {
                raw.set_temp_c = v.trim().parse::<f64>().ok().or(raw.set_temp_c);
            }
            if let Some(v) = overrides_index.get(&(fp, "offset")) {
                raw.offset = v.trim().parse::<i64>().ok().or(raw.offset);
            }
        }
    }

    // spec 041 FR-046 / R-4: detect `inbox_file_overrides` staleness — the
    // generic-property counterpart to the `mark_override_stale` snapshot pass
    // above (which only covers the fixed evidence columns).
    if let Some(sg_id) = item.source_group_id.as_deref() {
        check_generic_override_staleness(pool, sg_id, &group_overrides, &req.root_absolute_path)
            .await;
    }

    // Folder tallies keyed by FrameType (Copy enum) to avoid one String alloc
    // per file.
    let mut frame_type_files: HashMap<FrameType, Vec<String>> = HashMap::new();
    let mut unclassified_files: Vec<String> = Vec::new();
    for (rel, ft_opt, _) in &file_records {
        match ft_opt {
            Some(ft) => {
                frame_type_files.entry(*ft).or_default().push(rel.clone());
            }
            None => unclassified_files.push(rel.clone()),
        }
    }

    // 7. Determine folder-level classification result.
    //
    // Two distinct string spaces are used:
    //   db_result    — stored in inbox_classifications.result; must match the
    //                  CHECK constraint introduced in migration 0049:
    //                  ('classified', 'unclassified').  'single_type' and
    //                  'mixed' no longer exist at the DB level. A folder with
    //                  a single frame type → 'classified'; a folder with
    //                  multiple frame types → 'unclassified' (the mixed case
    //                  will be re-split into single-type sub-items in T066).
    //   api_result   — returned in ClassifyResponse.classification_type, on the
    //                  pre-0049 vocabulary ('single_type' / 'unclassified').
    //
    // Spec 058 T035 retires 'mixed'. FR-031 required it "for as long as
    // placeholder rows exist" and named this feature as the change that ends
    // that condition: 'mixed' was reachable ONLY on a pre-materialization
    // placeholder whose files spanned two or more frame types, and T012 stopped
    // creating those rows. The multi-type arm stays for exhaustiveness and
    // reports 'unclassified' — the value its DB result already carried — rather
    // than becoming `unreachable!()`, because a panic is a poor way to discover
    // that an assumption was wrong in a release build.
    let distinct_types: Vec<&str> = frame_type_files.keys().map(|ft| ft.as_str()).collect();
    let (db_result, api_result, single_frame_type) = match distinct_types.len() {
        1 => ("classified", "single_type", Some(distinct_types[0].to_owned())),
        // Zero readable frame types and two-or-more both mean "not one thing",
        // and with `mixed` retired they are the same answer. Keeping them as
        // separate arms with identical bodies would imply a distinction the
        // vocabulary no longer draws.
        _ => ("unclassified", "unclassified", None),
    };

    let unclassified_count = i64::try_from(unclassified_files.len()).unwrap_or(i64::MAX);

    // T066: Materialize single-type sub-items (R-9/R-11).
    //
    // For each file we build a FrameMetadata from extracted raw_meta, then call
    // group_file with the per-type GroupingConfig::default_for to get its
    // deterministic group_key. Files are partitioned by group_key; the T070 gate
    // sets `needs_review` on the resulting item without touching its identity.
    // For each group we upsert one inbox_items row with identity
    // (root_id, relative_path, group_key) and a per-sub-group content_signature.
    //
    // Only runs when the item has a source_group_id (i.e. was discovered via
    // T065 scan → source group). Legacy items without a source group are
    // skipped here (they continue to function as single folder-level items
    // until they are rescanned after T065 is in place).
    //
    // spec 041 T077/FR-054: a legacy item with an open plan (`state ==
    // "plan_open"`, read from the item fetched in step 1, before step 9 below
    // overwrites it) is NOT re-split even though it already carries a
    // `source_group_id` — migration 0049 assigns every pre-existing row a
    // `sg-migrate-*` source group unconditionally (see the doc comment on
    // `InboxItemRow::source_group_id`), so this state check is what keeps the
    // plan's 1:1 link to a single legacy sub-item intact until the plan
    // resolves or is discarded (`plan_listener::transition_via_plan_id`).
    // Re-derivation into proper single-type sub-items happens naturally the
    // next time classify runs on the item once it is no longer `plan_open`.
    // ── Second transaction: classification writes + sub-item materialization ──
    //
    // All load-bearing writes after the override re-application run in one
    // transaction so a partial write (e.g. a failed `update_inbox_item_state`)
    // cannot leave the DB in a partially-updated state.
    let mut tx2 = pool
        .begin()
        .await
        .map_err(|e| db_internal_ctx(e, "classify: begin classification transaction"))?;
    let conn2: &mut SqliteConnection = &mut tx2;

    let sg_id_for_split = item.source_group_id.as_deref().filter(|_| item.state != "plan_open");
    if let Some(sg_id) = sg_id_for_split {
        materialize_sub_items_tx(
            conn2,
            sg_id,
            &item.root_id,
            &item.relative_path,
            &item.lane,
            &file_paths,
            &file_records,
        )
        .await;
    }

    // 8. Persist classification (use db_result which satisfies migration 0049 CHECK).
    let classification = UpsertClassification {
        inbox_item_id: &req.inbox_item_id,
        result: db_result,
        frame_type: single_frame_type.as_deref(),
        content_signature: &content_signature,
        unclassified_file_count: unclassified_count,
    };
    repo::upsert_classification_conn(conn2, &classification)
        .await
        .map_err(|e| db_internal_ctx(e, "classify: upsert classification"))?;

    // 9. Update item state and signature.
    // #549: `file_records.len()` (not `file_paths.len()`) — extracted-master
    // files were filtered out of `file_records` above, so this is the
    // un-extracted remainder the placeholder is meant to represent, matching
    // the count `persist_folder_placeholder` (inbox.rs) writes at scan time.
    repo::update_inbox_item_scan_conn(
        conn2,
        &req.inbox_item_id,
        &content_signature,
        i64::try_from(file_records.len()).unwrap_or(i64::MAX),
    )
    .await
    .map_err(|e| db_internal_ctx(e, "classify: update item scan columns"))?;

    // spec 058 FR-007/SC-003: only a row that carries its own frame type may
    // report `classified`. A folder aggregate never gets one, so flipping it
    // unconditionally was the #711 "Classified badge on a row that knows no
    // frame type" defect. Kept across main's error-handling refactor, which
    // rewrote this call to propagate context but restored the unconditional
    // "classified" this feature exists to remove.
    //
    // Keyed on `item.frame_type` — the row's OWN type — deliberately, not on
    // the freshly computed `single_frame_type`. A uniform folder resolves to
    // one type, but `classify` writes that type onto the materialized
    // sub-items, never onto this row; keying on the computed value marks a
    // row `classified` whose own frame_type column is still NULL, which is
    // precisely the #711 badge defect (caught by
    // `no_item_reports_classified_without_a_frame_type_sc003`).
    let next_state =
        if item.frame_type.is_some() { "classified" } else { "pending_classification" };
    repo::update_inbox_item_state_conn(conn2, &req.inbox_item_id, next_state)
        .await
        .map_err(|e| db_internal_ctx(e, "classify: mark item classified"))?;

    tx2.commit()
        .await
        .map_err(|e| db_internal_ctx(e, "classify: commit classification transaction"))?;

    // 10. Build + persist breakdown with destination previews
    let breakdown =
        build_breakdown(pool, &req.inbox_item_id, &frame_type_files, &req.root_absolute_path).await;

    // 11. Sample files for top-level response
    let all_classified: Vec<String> =
        frame_type_files.values().flatten().take(10).cloned().collect();

    let computed_at = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned());

    Ok(ClassifyResponse {
        inbox_item_id: req.inbox_item_id,
        // api_result retains the pre-0049 vocabulary for frontend stability.
        classification_type: api_result.to_owned(),
        frame_type: single_frame_type,
        content_signature,
        breakdown,
        unclassified_files,
        sample_files: all_classified,
        computed_at,
    })
}

// ── classify_source_group ────────────────────────────────────────────────────

/// Response from the group-scoped classify entry point (spec 058 T012).
#[derive(Clone, Debug, serde::Serialize)]
pub struct ClassifySourceGroupResponse {
    pub source_group_id: String,
    pub materialized_sub_item_count: usize,
}

/// Classify a bare `inbox_source_groups` row directly — keyed on the group,
/// not an `inbox_items` row (spec 058 T012).
///
/// Spec 058 removes the scan-time folder placeholder item (T020), which until
/// now was the ONLY route to `materialize_sub_items`: `classify()` requires
/// an `inbox_item_id`, and `reclassify_v2` rebuilds its `file_records` from
/// `inbox_classification_evidence`/`inbox_file_metadata` rows that are
/// themselves only ever written against an item id. A bare source group has
/// none of that — no evidence, no overrides, no cache row — so this entry
/// point deliberately SKIPS every item-keyed step `classify()` performs:
/// `snapshot_overrides` (nothing to snapshot), the `delete_*_for_item` wipes
/// (nothing to delete), the `get_classification` cache-hit check, and the
/// classification cache write (all keyed on an item id that does not exist).
/// It goes straight from the source group's own file list to
/// `materialize_sub_items`, which is already fully group-keyed.
///
/// An empty folder returns `MetadataUnreadable`, mirroring `classify()`'s own
/// behaviour for the same condition — no caller needs a typed "empty" result
/// yet (YAGNI).
///
/// # Errors
/// Returns `ContractError` when the source group row is missing or the
/// folder has no FITS/XISF files.
pub async fn classify_source_group(
    pool: &SqlitePool,
    source_group_id: &str,
    root_absolute_path: &Path,
) -> Result<ClassifySourceGroupResponse, ContractError> {
    let sg = q_inbox::get_source_group_by_id(pool, source_group_id)
        .await
        .map_err(|e| ContractError::internal(e.to_string()))?
        .ok_or_else(|| {
            ContractError::new(
                ErrorCode::InboxItemNotFound,
                format!("Source group not found: {source_group_id}"),
                ErrorSeverity::Blocking,
                false,
            )
        })?;

    let folder_abs = root_absolute_path.join(&sg.relative_path);
    let file_paths = enumerate_fits_files(&folder_abs);
    if file_paths.is_empty() {
        return Err(ContractError::new(
            ErrorCode::MetadataUnreadable,
            format!("No FITS/XISF files found for source group: {}", folder_abs.display()),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    let file_records = build_file_records(&file_paths, root_absolute_path);

    // `inbox_source_groups.lane` is the move-vs-catalogue lane, NOT the
    // fits/video lane `inbox_items` requires (CHECK(lane IN ('fits',
    // 'video'))) — see `reclassify_v2`'s identical derivation and its #854
    // fix comment. Deriving from `format` mirrors scan's own assignment
    // (video-only folders → 'video', everything else → 'fits'); passing
    // `sg.lane` straight through would fail that CHECK for any 'move'/
    // 'catalogue'-lane group and silently drop every materialized sub-item.
    let lane = match sg.format.as_deref() {
        Some("video") => "video",
        _ => "fits",
    };

    materialize_sub_items(
        pool,
        &sg.id,
        &sg.root_id,
        &sg.relative_path,
        lane,
        &file_paths,
        &file_records,
    )
    .await;

    let materialized_sub_item_count =
        repo::list_inbox_sub_items(pool, &sg.id).await.map_or(0, |rows| rows.len());

    Ok(ClassifySourceGroupResponse { source_group_id: sg.id, materialized_sub_item_count })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_f64(s: Option<&String>) -> Option<f64> {
    s.and_then(|v| v.trim().parse::<f64>().ok())
}
fn parse_i64(s: Option<&String>) -> Option<i64> {
    s.and_then(|v| {
        let t = v.trim();
        t.parse::<i64>().ok().or_else(|| t.strip_suffix(".0").and_then(|i| i.parse::<i64>().ok()))
    })
}
fn parse_i32(s: Option<&String>) -> Option<i32> {
    s.and_then(|v| {
        let t = v.trim();
        t.parse::<i32>().ok().or_else(|| t.strip_suffix(".0").and_then(|i| i.parse::<i32>().ok()))
    })
}

/// Re-apply snapshotted overrides to freshly-inserted evidence rows, then mark
/// stale the subset whose file identity changed (spec 041 R-4 / T025).
///
/// Called after the first write transaction (evidence wipe + re-insert) commits,
/// because `set_overrides` and `mark_override_stale` read the committed rows via
/// the pool.  Failures are best-effort: logged but not propagated.
async fn restore_overrides_after_wipe(
    pool: &SqlitePool,
    inbox_item_id: &str,
    override_snapshot: &[OverrideSnapshot],
) {
    for entry in override_snapshot {
        if let Err(e) = repo::set_overrides(
            pool,
            inbox_item_id,
            &entry.relative_file_path,
            entry.manual_override.as_deref(),
            entry.override_filter.as_deref(),
            entry.override_exposure_s,
            entry.override_binning.as_deref(),
        )
        .await
        {
            tracing::warn!(
                item = %inbox_item_id,
                file = %entry.relative_file_path,
                "classify: restoring overrides failed, file reverts to its raw header state: {e}"
            );
        }
        if entry.stale {
            repo::mark_override_stale(pool, inbox_item_id, &entry.relative_file_path)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!(
                        item = %inbox_item_id,
                        file = %entry.relative_file_path,
                        "classify: marking override stale failed, file shows as fresh: {e}"
                    );
                });
        }
    }
}

/// Check `inbox_file_overrides` staleness for a source group (spec 041 FR-046
/// / R-4) — the generic-property counterpart to the `mark_override_stale`
/// snapshot pass.
///
/// Each override row carries the file identity recorded when it was set.
/// Compares it against the current on-disk stat; if they differ, marks the
/// override stale.  Deduplicates by path (property rows share identity).
/// Failures are best-effort.
async fn check_generic_override_staleness(
    pool: &SqlitePool,
    source_group_id: &str,
    group_overrides: &[persistence_inbox::repositories::inbox::FileOverrideRow],
    root_absolute_path: &std::path::Path,
) {
    let mut checked_paths: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for ov in group_overrides {
        let (Some(stored_size), Some(stored_mtime)) =
            (ov.file_size_bytes, ov.file_mtime.as_deref())
        else {
            continue; // no recorded identity yet — nothing to compare against
        };
        if !checked_paths.insert(ov.relative_file_path.as_str()) {
            continue;
        }
        let abs = root_absolute_path.join(&ov.relative_file_path);
        if let Ok(md) = std::fs::metadata(&abs) {
            let cur_size = i64::try_from(md.len()).ok();
            let cur_mtime = md
                .modified()
                .ok()
                .and_then(|t| time::OffsetDateTime::from(t).format(&Rfc3339).ok());
            if cur_size != Some(stored_size) || cur_mtime.as_deref() != Some(stored_mtime) {
                repo::mark_file_override_stale(pool, source_group_id, &ov.relative_file_path)
                    .await
                    .unwrap_or_else(|e| {
                        tracing::warn!(
                            source_group = %source_group_id,
                            file = %ov.relative_file_path,
                            "classify: marking file override stale failed, changed file shows as fresh: {e}"
                        );
                    });
            }
        }
    }
}

/// Per-file classification outcome, shared by `classify()`'s item-keyed
/// evidence-persist loop and `build_file_records` (spec 058 T012's
/// `classify_source_group`, which has no `inbox_item_id` to persist evidence
/// against). Carries every field `classify()` needs for `InsertEvidence`;
/// `build_file_records` keeps only `relative_path`/`frame_type`/`raw_meta`.
pub(crate) struct FileClassification {
    pub relative_path: String,
    pub frame_type: Option<FrameType>,
    pub raw_meta: Option<std::sync::Arc<metadata_core::RawFileMetadata>>,
    pub evidence_source: EvidenceSource,
    pub raw_value: Option<String>,
    pub is_unclassified: bool,
    pub is_master: bool,
    pub master_detector: Option<&'static str>,
}

/// Classify one file: master detection first (spec 040 FR-004), then
/// IMAGETYP normalization fallback (T014/T016). Pure — no DB writes; callers
/// persist evidence themselves (only `classify()` has an item id to persist
/// it against).
pub(crate) fn classify_one_file(
    abs_path: &Path,
    root_absolute_path: &Path,
    norm_table: &ImageTypNormalizationTable,
) -> FileClassification {
    // Lossless path → wire-string conversion (camino). `abs_path` descends
    // from a UTF-8 root supplied by the contract, so `Utf8Path::from_path`
    // succeeds; the `to_string_lossy` arms are defensive fallbacks only and
    // replace the previous always-lossy conversions.
    let rel = match abs_path.strip_prefix(root_absolute_path) {
        Ok(p) => fs_pathsafe::wire_path(p),
        Err(_) => Utf8Path::from_path(abs_path)
            .map_or_else(|| abs_path.display().to_string(), |u| u.as_str().to_owned()),
    };

    let ext = abs_path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();

    // Extract raw metadata (F0 cached-extract: memoized by path/mtime/size).
    let raw_meta = cached_extract(abs_path).ok();

    let image_typ_raw = raw_meta.as_ref().and_then(|m| m.image_typ.as_deref());
    let stack_count = raw_meta.as_ref().and_then(|m| m.stack_count);
    let file_name = abs_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // Run master detection first (spec 040 FR-004).
    // detect_master provides both frame_type and is_master when it matches.
    let detect_input =
        DetectInput { imagetyp: image_typ_raw, stack_count, file_name, rel_path: &rel };
    let master_result = detect_master(&detect_input);

    let (frame_type, evidence_source, raw_value, is_unclassified, is_master, master_detector) =
        if let Some(ref det) = master_result {
            // Detector produced a classification — use it directly.
            let src = if ext == "xisf" {
                EvidenceSource::XisfProperty
            } else {
                EvidenceSource::ImagetypHeader
            };
            (
                Some(det.frame_type),
                src,
                image_typ_raw.map(str::to_owned),
                false,
                det.is_master,
                Some(det.detector),
            )
        } else if let Some(raw) = image_typ_raw {
            // No master detector matched; fall back to the normalization table.
            match norm_table.normalize(raw) {
                Some(ft) => {
                    let src = if ext == "xisf" {
                        EvidenceSource::XisfProperty
                    } else {
                        EvidenceSource::ImagetypHeader
                    };
                    (Some(ft), src, Some(raw.to_owned()), false, false, None)
                }
                None => (None, EvidenceSource::None, Some(raw.to_owned()), true, false, None),
            }
        } else {
            (None, EvidenceSource::None, None, true, false, None)
        };

    FileClassification {
        relative_path: rel,
        frame_type,
        raw_meta,
        evidence_source,
        raw_value,
        is_unclassified,
        is_master,
        master_detector,
    }
}

/// Build `(relative_path, frame_type, raw_meta)` records for a set of files
/// with no item-keyed persistence (spec 058 T012). Used by
/// `classify_source_group`, which has no `inbox_item_id` to persist evidence
/// against; `classify()` calls `classify_one_file` directly instead, since it
/// also needs the fields this drops in order to write `InsertEvidence` rows.
pub(crate) fn build_file_records(
    file_paths: &[PathBuf],
    root_absolute_path: &Path,
) -> Vec<(String, Option<FrameType>, Option<std::sync::Arc<metadata_core::RawFileMetadata>>)> {
    let norm_table = v1_normalization_table();
    file_paths
        .iter()
        .map(|abs_path| {
            let fc = classify_one_file(abs_path, root_absolute_path, &norm_table);
            (fc.relative_path, fc.frame_type, fc.raw_meta)
        })
        .collect()
}

/// Map extracted `RawFileMetadata` → an `inbox_file_metadata` upsert and write
/// it (spec 041 US2/T016).
///
/// Numeric header fields arrive as `Option<String>` from the extractor; we
/// parse them to `i64`/`f64` here. `gain` is intentionally left as a string.
/// `file_size_bytes`/`file_mtime` are the cheap per-file identity used for
/// override staleness (R-4); a failed `stat` simply leaves them `None`.
/// Build a [`UpsertFileMetadata`] from already-collected file identity and raw
/// metadata — no DB write, no stat (caller supplies `file_size_bytes` /
/// `file_mtime` collected outside any transaction).
///
/// Extracted from the old `persist_file_metadata` body so the classify hot
/// path can build the full batch without opening a DB connection per file.
fn build_file_metadata_upsert<'a>(
    inbox_item_id: &'a str,
    raw_meta: Option<&'a metadata_core::RawFileMetadata>,
    rel: &'a str,
    file_size_bytes: Option<i64>,
    file_mtime: Option<&'a str>,
) -> UpsertFileMetadata<'a> {
    if let Some(meta) = raw_meta {
        UpsertFileMetadata {
            inbox_item_id,
            relative_file_path: rel,
            filter: meta.filter.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            exposure_s: parse_f64(meta.exposure.as_ref()),
            gain: meta.gain.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            binning_x: parse_i64(meta.x_binning.as_ref()),
            binning_y: parse_i64(meta.y_binning.as_ref()),
            // SET-TEMP is the default dark-grouping temperature source (R-18);
            // CCD-TEMP is deviation-only and not persisted to this single column.
            temperature_c: meta.set_temp_c,
            object: meta.object.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            date_obs: meta.date_obs.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            instrume: meta.instrume.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            telescop: meta.telescop.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            naxis1: parse_i64(meta.naxis1.as_ref()),
            naxis2: parse_i64(meta.naxis2.as_ref()),
            stack_count: meta.stack_count.map(i64::from),
            file_size_bytes,
            file_mtime,
            // spec 041 T072/FR-044: persist the T062 extended fields so
            // `inbox.item.metadata` (display) and `inbox.target_recommendations`
            // (T074) read real values instead of permanently-NULL columns.
            offset: meta.offset,
            set_temp_c: meta.set_temp_c,
            ccd_temp_c: meta.ccd_temp_c,
            ra_deg: meta.ra_deg,
            dec_deg: meta.dec_deg,
            rotator_angle_deg: meta.rotator_angle_deg,
            readout_mode: meta.readout_mode.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            focal_length_mm: meta.focal_length_mm,
            date_loc: meta.date_loc.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            // spec 052 P3: pixel_size_um/sky_rotation_deg columns predate this
            // spec (migration 0049) but were never wired here — without them
            // `inbox.target_recommendations` (R-17) and cone-search silently
            // always fell back to the fixed/axis-aligned case for real files.
            pixel_size_um: meta.pixel_size_um,
            sky_rotation_deg: meta.sky_rotation_deg,
            wcs_ra_deg: meta.wcs_ra_deg,
            wcs_dec_deg: meta.wcs_dec_deg,
            wcs_rotation_deg: meta.wcs_rotation_deg,
        }
    } else {
        // No header metadata — still record identity for staleness tracking.
        UpsertFileMetadata {
            inbox_item_id,
            relative_file_path: rel,
            file_size_bytes,
            file_mtime,
            ..Default::default()
        }
    }
}

/// Collect relative file paths that need to be marked as stale after the
/// rescan evidence/metadata wipe (spec 041 R-4).
///
/// A file is stale when ALL of:
/// - it has at least one override column set (manual_override / filter /
///   exposure_s / binning), AND
/// - its current on-disk size or mtime differs from what was stored in
///   `inbox_file_metadata` at the previous classify.
///
/// Called BEFORE `delete_evidence_for_item` / `delete_file_metadata_for_item`
/// because both tables are wiped in the rescan. The returned paths are then
/// used to call `mark_override_stale` on the freshly-inserted evidence rows.
///
/// Failures are silently ignored — the classify result is unaffected.
/// Snapshot of a single evidence row's override values, captured before the
/// evidence table is wiped on a rescan (spec 041 R-4 / T025).
struct OverrideSnapshot {
    relative_file_path: String,
    manual_override: Option<String>,
    override_filter: Option<String>,
    override_exposure_s: Option<f64>,
    override_binning: Option<String>,
    /// True when the file's current on-disk identity (size/mtime) differs from
    /// the stored `inbox_file_metadata` row, meaning the file changed since the
    /// override was recorded.
    stale: bool,
}

/// Before the evidence/metadata wipe on a rescan, snapshot all evidence rows
/// that carry any override, paired with a staleness flag (spec 041 R-4 / T025).
///
/// For each such row that also appears in the current scan:
/// - compare stored file identity (size/mtime in `inbox_file_metadata`) against
///   the current on-disk stat; set `stale = true` when they differ.
///
/// Called BEFORE `delete_evidence_for_item` / `delete_file_metadata_for_item`.
/// After the fresh evidence re-insert the caller must:
///   1. call `set_overrides` for every entry (preserves the user's decision), and
///   2. call `mark_override_stale` for entries where `stale == true`.
///
/// Failures are silently ignored — the classify result is unaffected.
async fn snapshot_overrides(
    pool: &SqlitePool,
    inbox_item_id: &str,
    file_paths: &[PathBuf],
    root_absolute_path: &Path,
) -> Vec<OverrideSnapshot> {
    let mut snapshots = Vec::new();

    let evidence = repo::list_evidence(pool, inbox_item_id).await.unwrap_or_default();

    for ev in evidence {
        let has_override = ev.manual_override.is_some()
            || ev.override_filter.is_some()
            || ev.override_exposure_s.is_some()
            || ev.override_binning.is_some();

        if !has_override {
            continue;
        }

        // Only re-apply for files that appear in the current scan.
        let in_scan = file_paths.iter().any(|p| {
            p.strip_prefix(root_absolute_path)
                .is_ok_and(|r| fs_pathsafe::wire_path(r) == ev.relative_file_path)
        });
        if !in_scan {
            continue;
        }

        // Compare stored identity against current on-disk stat.
        let stale = 'stale: {
            let prior = repo::get_file_metadata(pool, inbox_item_id, &ev.relative_file_path)
                .await
                .ok()
                .flatten();
            let Some(prior) = prior else { break 'stale false };

            let abs = root_absolute_path.join(&ev.relative_file_path);
            let Ok(md) = std::fs::metadata(&abs) else { break 'stale false };
            let new_size = i64::try_from(md.len()).ok();
            let new_mtime = md
                .modified()
                .ok()
                .and_then(|t| time::OffsetDateTime::from(t).format(&Rfc3339).ok());

            prior.file_size_bytes != new_size || prior.file_mtime.as_deref() != new_mtime.as_deref()
        };

        snapshots.push(OverrideSnapshot {
            relative_file_path: ev.relative_file_path,
            manual_override: ev.manual_override,
            override_filter: ev.override_filter,
            override_exposure_s: ev.override_exposure_s,
            override_binning: ev.override_binning,
            stale,
        });
    }

    snapshots
}

/// Classification identity for a file whose frame type could not be determined
/// at all (spec 058 FR-028, T007).
///
/// This is an identity *value* ("type undetermined"), not a needs-review flag:
/// the review verdict lives in `inbox_items.needs_review`. No code branches on
/// this string. `FrameType::as_str` never yields `"unknown"`, so it cannot
/// collide with a real classification key.
pub const GROUP_KEY_TYPE_UNKNOWN: &str = "type=unknown";

// ── Mandatory-attribute gate (T070 / FR-047 / R-14) ──────────────────────────

/// Derive the **mandatory attribute set** for a frame type (R-14).
///
/// Returns the registry key names (camelCase, matching `property_registry()`)
/// that **must** be present for a file of this type to form a valid single-type
/// destination.  The set is derived from the `GroupingConfig` default dimensions
/// (enabled grouping dims whose absence is meaningful) plus hard per-type keys
/// that are always mandatory regardless of pattern tokens:
///
/// | frame_type | Hard mandatory keys                   |
/// |------------|---------------------------------------|
/// | light      | `frameType`, `target`, `filter`, `exposureS` |
/// | dark       | `frameType`, `exposureS`, `gain`      |
/// | bias       | `frameType`, `gain`                   |
/// | flat       | `frameType`, `filter`                 |
///
/// `target` for lights is a special hard key: it is satisfied by coordinate
/// auto-resolution (R-17) **or** an explicit user pick, but if neither has
/// produced a value the file routes to needs-review (FR-047 note).
///
/// The returned list is deduplicated and stable.
#[must_use]
pub fn mandatory_set_for(ft: FrameType) -> Vec<&'static str> {
    // Hard mandatory keys per R-14 table.
    let hard: &[&str] = match ft {
        FrameType::Light => &["frameType", "target", "filter", "exposureS"],
        FrameType::Dark | FrameType::DarkFlat => &["frameType", "exposureS", "gain"],
        FrameType::Bias => &["frameType", "gain"],
        FrameType::Flat => &["frameType", "filter"],
    };
    // Deduplicate (preserving order) — grouping dims may overlap with hard keys.
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for &k in hard {
        if seen.insert(k) {
            out.push(k);
        }
    }
    out
}

/// Whether the `target` hard-mandatory key (R-14) counts as absent.
///
/// Single shared formula for the `target` arm of both mandatory-attribute
/// gates — [`check_mandatory_missing`] here and
/// `crate::metadata::compute_missing_mandatory` — so the two consumers
/// agree on "target missing" **by construction**, not because every caller
/// happens to pass the same `target_resolved` constant (issue #1132).
///
/// Satisfied by an explicit target resolution (`target_resolved`: coordinate
/// auto-resolution or a user pick, R-17) **or** a non-empty (trimmed) `OBJECT`
/// header/override. Every current call site pins `target_resolved` to
/// `false` — R-17's coordinate resolver (`target_recommendations`,
/// `cone_search`) is implemented and reachable from the UI, but nothing yet
/// threads a resolved-target signal back into this gate; that wiring is the
/// open product decision tracked in #1132.
#[must_use]
pub fn target_missing(object: Option<&str>, target_resolved: bool) -> bool {
    !target_resolved && object.map_or("", str::trim).is_empty()
}

/// Check which mandatory attributes are absent for a classified file.
///
/// `raw_meta` is `None` when FITS extraction failed entirely (→ all mandatory
/// attributes except `frameType` are considered missing).  `target_resolved`
/// indicates whether a target has been resolved by coordinate lookup or user
/// pick for light frames.
///
/// Returns the sorted list of registry key names that are missing.
#[must_use]
pub fn check_mandatory_missing(
    ft: FrameType,
    raw_meta: Option<&metadata_core::RawFileMetadata>,
    target_resolved: bool,
) -> Vec<String> {
    let mandatory = mandatory_set_for(ft);
    let mut missing = Vec::new();

    for key in mandatory {
        let absent = match key {
            "target" => target_missing(raw_meta.and_then(|m| m.object.as_deref()), target_resolved),
            "filter" => raw_meta.and_then(|m| m.filter.as_deref()).map_or("", str::trim).is_empty(),
            "exposureS" => !raw_meta
                .and_then(|m| m.exposure.as_deref())
                .is_some_and(|s| s.trim().parse::<f64>().is_ok_and(|v| v > 0.0)),
            "gain" => raw_meta.and_then(|m| m.gain.as_deref()).map_or("", str::trim).is_empty(),
            // "frameType" is always present when ft is known; unknown keys never absent.
            _ => false,
        };
        if absent {
            missing.push(key.to_owned());
        }
    }
    missing
}

/// Mandatory attributes absent for one file record, as the needs-review bucket
/// judges it (T070 / FR-047 / R-14).
///
/// An unresolved frame type reports `["frameType"]`: the frame type is itself
/// the first mandatory attribute, and until it is known no per-type set can be
/// derived. Empty means the file can leave the needs-review bucket.
///
/// `target_resolved` is pinned to `false` here because coordinate resolution
/// (FR-052) needs the sub-group's persisted pointing rows, which this pass is
/// still producing — so at classify time the OBJECT header remains the proxy.
/// A light with pointing but no OBJECT therefore starts in needs-review and
/// leaves it on the next reclassify, where
/// [`crate::target_recommendations::auto_resolve_target`] can run against the
/// stored pointing.
#[must_use]
pub(crate) fn missing_mandatory_for_file(
    frame_type: Option<FrameType>,
    raw_meta: Option<&metadata_core::RawFileMetadata>,
) -> Vec<String> {
    match frame_type {
        Some(ft) => check_mandatory_missing(ft, raw_meta, false),
        None => vec!["frameType".to_owned()],
    }
}

/// Build a [`FrameMetadata`] from a [`metadata_core::RawFileMetadata`] for use
/// with the grouping engine (T066). All extended fields (set_temp, pointing,
/// rotation, optic-train, observing-night) are sourced from the core
/// extractor's T062 extraction (spec 041 T081); a field is `None` only when
/// the source FITS header was absent, in which case that grouping dimension
/// gracefully falls back to the [`crate::grouping::SENTINEL_MISSING`] bucket
/// (R-9 best-effort).
pub(crate) fn build_frame_metadata(
    frame_type: FrameType,
    raw: &metadata_core::RawFileMetadata,
) -> FrameMetadata {
    FrameMetadata {
        frame_type,
        filter: raw.filter.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned),
        exposure_s: parse_f64(raw.exposure.as_ref()),
        gain: raw.gain.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned),
        offset: raw.offset,
        binning_x: parse_i32(raw.x_binning.as_ref()),
        binning_y: parse_i32(raw.y_binning.as_ref()),
        set_temp_c: raw.set_temp_c,
        ccd_temp_c: raw.ccd_temp_c,
        ra_deg: raw.ra_deg,
        dec_deg: raw.dec_deg,
        rotator_angle_deg: raw.rotator_angle_deg,
        readout_mode: raw
            .readout_mode
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
        telescop: raw
            .telescop
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
        instrume: raw
            .instrume
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
        focal_length_mm: raw.focal_length_mm,
        date_loc: raw
            .date_loc
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
        date_obs: raw
            .date_obs
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
    }
}

/// Materialize one single-type `inbox_items` sub-item per homogeneous group
/// within a source group (spec 041 T066/T070, R-9/R-10/R-11/R-12/R-14).
///
/// # Algorithm
/// 1. Build a [`FrameMetadata`] for each file from its extracted raw metadata.
/// 2. Call [`group_file`] with [`GroupingConfig::default_for`] the file's frame
///    type to get a deterministic `(group_key, group_label)`.
/// 3. Files missing a mandatory attribute (T070 / FR-047/FR-048) keep their
///    classification identity and are flagged `needs_review`; files with no
///    frame type at all key on [`GROUP_KEY_TYPE_UNKNOWN`] (spec 058 FR-028).
/// 4. Per group: compute a per-sub-group `content_signature` =
///    `folder_signature(sorted per-file sigs of files in that group)`, then
///    upsert an `inbox_items` row with identity `(root_id, relative_path,
///    group_key)` — stable across rescans of unchanged content (FR-042).
/// 5. Update the source group's `child_count`.
///
/// Failures are silently ignored — classify's primary evidence/classification
/// result is unaffected.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn materialize_sub_items(
    pool: &sqlx::SqlitePool,
    source_group_id: &str,
    root_id: &str,
    relative_path: &str,
    lane: &str,
    file_paths: &[PathBuf],
    file_records: &[(
        String,
        Option<FrameType>,
        Option<std::sync::Arc<metadata_core::RawFileMetadata>>,
    )],
) {
    let Ok(mut conn) = pool.acquire().await else { return };
    materialize_sub_items_tx(
        &mut conn,
        source_group_id,
        root_id,
        relative_path,
        lane,
        file_paths,
        file_records,
    )
    .await;
}

/// Per-group seed context built during sub-item materialization.
struct SubItemSeed {
    persisted_id: String,
    is_needs_review: bool,
    frame_type_str: Option<String>,
    sub_sig: String,
    /// Index range into the flat `all_files` Vec: `[start, end)`.
    files_idx: (usize, usize),
}

/// Connection-level variant of [`materialize_sub_items`]: routes all writes
/// through `conn` (part of the caller's transaction) while reads use `pool`.
///
/// Used by `classify()` so the sub-item seed writes are included in the same
/// transaction as the classification and item-state writes.
/// `materialize_sub_items` (pool-only) is kept for `reclassify.rs`.
// CCN justified: 6 sequential pipeline stages (partition → upsert → delete-stale →
// evidence → metadata → breakdown+classification) each contribute ~2-3 branches.
// No branching alternatives exist — each stage is mandatory and order-dependent.
// Splitting across functions would require threading conn + source_group_id + seeds
// through every call, replacing structural clarity with parameter noise.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(crate) async fn materialize_sub_items_tx(
    conn: &mut SqliteConnection,
    source_group_id: &str,
    root_id: &str,
    relative_path: &str,
    lane: &str,
    file_paths: &[PathBuf],
    file_records: &[(
        String,
        Option<FrameType>,
        Option<std::sync::Arc<metadata_core::RawFileMetadata>>,
    )],
) {
    // Step 1 + 2 + T070 gate: partition files by group_key.
    #[allow(clippy::type_complexity)]
    let mut groups: std::collections::HashMap<
        String,
        (
            String,
            bool,
            Vec<(String, Option<PathBuf>, Option<std::sync::Arc<metadata_core::RawFileMetadata>>)>,
        ),
    > = std::collections::HashMap::new();

    for (i, (rel, frame_type_opt, raw_meta_opt)) in file_records.iter().enumerate() {
        let abs_path = file_paths.get(i).cloned();
        let missing = missing_mandatory_for_file(*frame_type_opt, raw_meta_opt.as_deref());
        let needs_review = !missing.is_empty();
        let (group_key, group_label) = if let Some(ft) = *frame_type_opt {
            let meta = raw_meta_opt.as_ref().map_or_else(
                || FrameMetadata { frame_type: ft, ..Default::default() },
                |r| build_frame_metadata(ft, r),
            );
            let config = GroupingConfig::default_for(ft);
            let result = group_file(&meta, &config);
            (result.key.0, result.label.0)
        } else {
            (GROUP_KEY_TYPE_UNKNOWN.to_owned(), "(root) · needs review".to_owned())
        };
        let entry =
            groups.entry(group_key).or_insert_with(|| (group_label, needs_review, Vec::new()));
        entry.1 |= needs_review;
        entry.2.push((rel.clone(), abs_path, raw_meta_opt.clone()));
    }

    let child_count = i64::try_from(groups.len()).unwrap_or(i64::MAX);

    // ── Step 1: upsert sub-items; collect persisted ids ───────────────────────
    //
    // Each upsert returns the id that ACTUALLY persisted (ON CONFLICT keeps the
    // pre-existing row's id). All subsequent batch writes key on persisted_id,
    // not on the freshly-generated candidate id (issue #854).
    // Collect all files into a flat Vec so we can build batch payloads with
    // contiguous index ranges.
    #[allow(clippy::type_complexity)]
    let mut all_files: Vec<(
        String,
        Option<PathBuf>,
        Option<std::sync::Arc<metadata_core::RawFileMetadata>>,
    )> = Vec::new();
    // Produce the sub-item seeds in deterministic key order.
    let mut sorted_groups: Vec<(&String, &(String, bool, Vec<_>))> = groups.iter().collect();
    sorted_groups.sort_by_key(|(k, _)| k.as_str());

    let mut seeds: Vec<SubItemSeed> = Vec::with_capacity(sorted_groups.len());

    for (group_key, (group_label, is_needs_review, files)) in &sorted_groups {
        let is_needs_review = *is_needs_review;
        let file_sigs: Vec<[u8; 32]> = files
            .iter()
            .filter_map(|(_, abs, _)| abs.as_deref())
            .filter_map(super::signature::file_signature)
            .collect();
        let sub_sig = folder_signature(file_sigs);
        let frame_type_str: Option<String> = if is_needs_review {
            None
        } else {
            group_key
                .strip_prefix("type=")
                .and_then(|rest| rest.split('·').next())
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        };

        let file_count = i64::try_from(files.len()).unwrap_or(i64::MAX);
        let sub_id = Uuid::new_v4().to_string();
        let sub_item = UpsertInboxSubItem {
            id: &sub_id,
            root_id,
            relative_path,
            source_group_id,
            group_key,
            group_label,
            frame_type: frame_type_str.as_deref(),
            content_signature: &sub_sig,
            file_count,
            lane,
            needs_review: is_needs_review,
        };
        let Ok(persisted_id) = repo::upsert_inbox_sub_item_conn(conn, &sub_item).await else {
            continue;
        };

        let start = all_files.len();
        all_files.extend(files.iter().cloned());
        let end = all_files.len();

        seeds.push(SubItemSeed {
            persisted_id,
            is_needs_review,
            frame_type_str,
            sub_sig,
            files_idx: (start, end),
        });
    }

    if seeds.is_empty() {
        // No groups succeeded — update child_count and return.
        repo::update_source_group_child_count_conn(conn, source_group_id, child_count)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(
                    source_group = %source_group_id,
                    child_count,
                    "classify: updating source-group child count failed, badge count is stale: {e}"
                );
            });
        return;
    }

    // ── Step 2: batch-delete stale cache rows for all persisted ids ───────────
    //
    // Best-effort: a failure leaves stale rows, not corrupt ones.
    let id_refs: Vec<&str> = seeds.iter().map(|s| s.persisted_id.as_str()).collect();
    repo::delete_evidence_for_items(conn, &id_refs).await.unwrap_or_else(|e| {
        tracing::warn!(
            source_group = %source_group_id,
            "classify: batch delete evidence failed, cache may be stale: {e}"
        );
    });
    repo::delete_breakdown_for_items(conn, &id_refs).await.unwrap_or_else(|e| {
        tracing::warn!(
            source_group = %source_group_id,
            "classify: batch delete breakdown failed, cache may be stale: {e}"
        );
    });
    repo::delete_file_metadata_for_items(conn, &id_refs).await.unwrap_or_else(|e| {
        tracing::warn!(
            source_group = %source_group_id,
            "classify: batch delete metadata failed, cache may be stale: {e}"
        );
    });

    // ── Step 3: batch-insert evidence across all groups ───────────────────────
    let ev_ids: Vec<String> = all_files.iter().map(|_| Uuid::new_v4().to_string()).collect();
    let ev_batch: Vec<repo::InsertEvidence<'_>> = seeds
        .iter()
        .flat_map(|s| {
            let slice = &all_files[s.files_idx.0..s.files_idx.1];
            let id_slice = &ev_ids[s.files_idx.0..s.files_idx.1];
            slice.iter().zip(id_slice.iter()).map(|((rel, _, _), id)| repo::InsertEvidence {
                id: id.as_str(),
                inbox_item_id: s.persisted_id.as_str(),
                relative_file_path: rel.as_str(),
                frame_type: if s.is_needs_review { None } else { s.frame_type_str.as_deref() },
                evidence_source: EvidenceSource::ImagetypHeader.as_str(),
                raw_value: None,
                unclassified: s.is_needs_review,
                manual_override: None,
                is_master: false,
                master_detector: None,
            })
        })
        .collect();

    repo::insert_evidence_batch(conn, &ev_batch).await.unwrap_or_else(|e| {
        tracing::warn!(
            source_group = %source_group_id,
            "classify: batch insert evidence failed, cache may be stale: {e}"
        );
    });

    // ── Step 4: batch-insert metadata across all groups ───────────────────────
    //
    // Stat calls happen here, outside any lock contention bottleneck.
    let meta_storage: Vec<(Option<i64>, Option<String>)> = all_files
        .iter()
        .map(|(_, abs_opt, _)| {
            let abs_for_stat = abs_opt.as_deref().unwrap_or_else(|| Path::new(""));
            match std::fs::metadata(abs_for_stat) {
                Ok(md) => {
                    let size = i64::try_from(md.len()).ok();
                    let mtime = md
                        .modified()
                        .ok()
                        .and_then(|t| time::OffsetDateTime::from(t).format(&Rfc3339).ok());
                    (size, mtime)
                }
                Err(_) => (None, None),
            }
        })
        .collect();

    let meta_batch: Vec<repo::UpsertFileMetadata<'_>> = seeds
        .iter()
        .flat_map(|s| {
            let slice = &all_files[s.files_idx.0..s.files_idx.1];
            let stat_slice = &meta_storage[s.files_idx.0..s.files_idx.1];
            slice.iter().zip(stat_slice.iter()).map(|((rel, _, raw_meta_opt), (sz, mt))| {
                build_file_metadata_upsert(
                    s.persisted_id.as_str(),
                    raw_meta_opt.as_deref(),
                    rel.as_str(),
                    *sz,
                    mt.as_deref(),
                )
            })
        })
        .collect();

    repo::upsert_inbox_file_metadata_batch(conn, &meta_batch).await.unwrap_or_else(|e| {
        tracing::warn!(
            source_group = %source_group_id,
            "classify: batch insert metadata failed, cache may be stale: {e}"
        );
    });

    // ── Step 5: batch-upsert breakdown and classification for all groups ──────
    let bd_ids: Vec<String> = seeds.iter().map(|_| Uuid::new_v4().to_string()).collect();
    let classification_batch: Vec<repo::UpsertClassificationRow<'_>> = seeds
        .iter()
        .map(|s| {
            let (db_result, unclassified_count) = if s.is_needs_review {
                ("unclassified", i64::try_from(s.files_idx.1 - s.files_idx.0).unwrap_or(i64::MAX))
            } else {
                ("classified", 0)
            };
            repo::UpsertClassificationRow {
                inbox_item_id: s.persisted_id.as_str(),
                result: db_result,
                frame_type: if s.is_needs_review { None } else { s.frame_type_str.as_deref() },
                content_signature: s.sub_sig.as_str(),
                unclassified_file_count: unclassified_count,
            }
        })
        .collect();

    repo::upsert_classification_batch(conn, &classification_batch).await.unwrap_or_else(|e| {
        tracing::warn!(
            source_group = %source_group_id,
            "classify: batch upsert classification failed, cache may be stale: {e}"
        );
    });

    // Breakdown rows (one per group, only for classified groups).
    for (seed, bd_id) in seeds.iter().zip(bd_ids.iter()) {
        if seed.is_needs_review {
            continue;
        }
        let files_slice = &all_files[seed.files_idx.0..seed.files_idx.1];
        let samples: Vec<&str> = files_slice.iter().take(10).map(|(r, _, _)| r.as_str()).collect();
        let sample_json = serde_json::to_string(&samples).unwrap_or_else(|_| "[]".to_owned());
        repo::upsert_breakdown_row_conn(
            conn,
            bd_id,
            &seed.persisted_id,
            seed.frame_type_str.as_deref().unwrap_or("unknown"),
            i64::try_from(files_slice.len()).unwrap_or(i64::MAX),
            None,
            &sample_json,
        )
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                source_group = %source_group_id,
                sub_item = %seed.persisted_id,
                "classify: upsert breakdown failed, cache may be stale: {e}"
            );
        });
    }

    // ── Step 6: orphan cleanup ────────────────────────────────────────────────
    let current_keys: std::collections::HashSet<&str> = groups.keys().map(String::as_str).collect();
    let existing = repo::list_inbox_sub_items_conn(conn, source_group_id).await.unwrap_or_default();
    for row in existing {
        if !current_keys.contains(row.group_key.as_str()) {
            repo::delete_sub_item_if_unlinked_conn(conn, &row.id).await.unwrap_or_else(|e| {
                tracing::warn!(
                    source_group = %source_group_id,
                    sub_item = %row.id,
                    "classify: deleting orphaned sub-item failed, stale group remains: {e}"
                );
            });
        }
    }

    repo::update_source_group_child_count_conn(conn, source_group_id, child_count)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                source_group = %source_group_id,
                child_count,
                "classify: updating source-group child count failed, badge count is stale: {e}"
            );
        });
}

/// Enumerate FITS/XISF files directly inside a folder (non-recursive).
///
/// Skips symlinks and Windows junctions. `folder` is user-supplied, and
/// `is_file()` resolves links, so without this gate a link planted in an
/// inbox folder would pull an out-of-root file into classification — the
/// do-not-follow-links rule the scanner already enforces (issue #1233,
/// constitution product constraints).
fn enumerate_fits_files(folder: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(folder) else {
        return files;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if fs_pathsafe::is_link_or_junction(&path) {
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        if matches!(ext.as_str(), "fits" | "fit" | "fts" | "xisf") {
            files.push(path);
        }
    }
    files.sort();
    files
}

/// Build breakdown entries and persist them.
///
/// spec 041 US3/T018: each frame-type group gets a `destination_preview` —
/// the directory the active Naming & Structure pattern resolves to for that
/// group's first file. This reuses the SAME resolve path `confirm.rs` uses
/// (`load_active_pattern` + `build_metadata_bundle` + `resolve_v1`) so the
/// inbox surface previews exactly where a confirm would move the files. When
/// the pattern is unset/invalid or resolution fails, the preview is left
/// `None` (the surface shows a dash).
async fn build_breakdown(
    pool: &SqlitePool,
    inbox_item_id: &str,
    frame_type_files: &HashMap<FrameType, Vec<String>>,
    root_absolute_path: &Path,
) -> Vec<BreakdownEntry> {
    // Load the active pattern once; if it is unset/invalid every preview is None.
    let active_pattern = super::confirm::load_active_pattern(pool).await.ok();
    let norm_table = v1_normalization_table();
    // The same registry confirm resolves against, so a preview shows the
    // directory a confirm would actually produce.
    let cameras = app_core_calibration::equipment::list_cameras(pool).await.unwrap_or_default();

    let mut entries = Vec::new();

    for (ft, files) in frame_type_files {
        let kind = ft.as_str();
        let count = files.len();
        let sample: Vec<String> = files.iter().take(10).cloned().collect();
        let sample_json = serde_json::to_string(&sample).unwrap_or_else(|_| "[]".to_owned());

        // Resolve a destination-directory preview from the group's first file.
        let destination_preview = active_pattern.as_ref().and_then(|pattern| {
            let first_rel = files.first()?;
            let abs_path = root_absolute_path.join(first_rel);
            let bundle =
                super::confirm::build_metadata_bundle(&abs_path, kind, &norm_table, &cameras);
            patterns::resolve_v1(pattern, &bundle).ok().map(|r| r.relative_path)
        });

        let bd_id = Uuid::new_v4().to_string();
        let count_i64 = i64::try_from(count).unwrap_or(i64::MAX);
        repo::upsert_breakdown_row(
            pool,
            &bd_id,
            inbox_item_id,
            kind,
            count_i64,
            destination_preview.as_deref(),
            &sample_json,
        )
        .await
        .unwrap_or_else(|e| {
            // The returned `entries` are built in memory regardless, so the
            // response stays correct; only the persisted cache row is lost.
            tracing::warn!(
                item = %inbox_item_id,
                kind,
                "classify: persisting breakdown row failed, cached breakdown is partial: {e}"
            );
        });

        entries.push(BreakdownEntry {
            kind: kind.to_owned(),
            count,
            destination_preview,
            sample_files: sample,
        });
    }

    entries.sort_by(|a, b| a.kind.cmp(&b.kind));
    entries
}

/// Build a ClassifyResponse from cached DB rows.
async fn build_response_from_cache(
    pool: &SqlitePool,
    item: &persistence_inbox::repositories::inbox::InboxItemRow,
    cached: &persistence_inbox::repositories::inbox::InboxClassificationRow,
) -> Result<ClassifyResponse, ContractError> {
    let breakdown_rows = repo::list_breakdown(pool, &item.id).await.unwrap_or_default();
    let evidence_rows = repo::list_evidence(pool, &item.id).await.unwrap_or_default();

    let breakdown: Vec<BreakdownEntry> = breakdown_rows
        .into_iter()
        .map(|row| {
            let samples: Vec<String> = serde_json::from_str(&row.sample_files).unwrap_or_default();
            BreakdownEntry {
                kind: row.kind,
                count: usize::try_from(row.count).unwrap_or(0),
                destination_preview: row.destination_preview,
                sample_files: samples,
            }
        })
        .collect();

    let unclassified_files: Vec<String> = evidence_rows
        .iter()
        .filter(|ev| ev.unclassified != 0 && ev.manual_override.is_none())
        .map(|ev| ev.relative_file_path.clone())
        .collect();

    let sample_files: Vec<String> = evidence_rows
        .iter()
        .filter(|ev| ev.frame_type.is_some())
        .take(10)
        .map(|ev| ev.relative_file_path.clone())
        .collect();

    let computed_at = cached.computed_at.clone();

    // `inbox_classifications.result` stores the DB vocabulary introduced by
    // migration 0049 ('classified' / 'unclassified'), but
    // `ClassifyResponse.classification_type` is contractually the stable API
    // vocabulary ('single_type' / 'mixed' / 'unclassified') — see step 7 of
    // `classify`. Returning `cached.result` verbatim leaked 'classified' to
    // the frontend on every cache hit (re-selecting an item, or the classify
    // refetch after `inbox.reclassify`), where `canConfirm` requires exactly
    // 'single_type' — permanently disabling Confirm for already-classified
    // items (caught by the spec 037 Layer-2 Inbox journeys, PR #457). Map DB
    // → API here: 'classified' is single-type by the 0049 CHECK's definition,
    // and everything else is 'unclassified'.
    //
    // Spec 058 T035: this used to distinguish a 'mixed' case by counting
    // distinct effective frame types across the evidence rows. That count could
    // only reach two on a pre-materialization placeholder, which T012 no longer
    // creates, so the branch reported a state the app can no longer be in. The
    // cached path now mirrors the compute path above.
    let classification_type = match cached.result.as_str() {
        "classified" => "single_type".to_owned(),
        "unclassified" => "unclassified".to_owned(),
        // Pre-0049 rows can still carry the API vocabulary — pass through.
        other => other.to_owned(),
    };

    Ok(ClassifyResponse {
        inbox_item_id: item.id.clone(),
        classification_type,
        frame_type: cached.frame_type.clone(),
        content_signature: cached.content_signature.clone(),
        breakdown,
        unclassified_files,
        sample_files,
        computed_at,
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_core::Database;
    use persistence_inbox::repositories::inbox as inbox_repo;
    use persistence_inbox::repositories::inbox::InsertInboxItem;
    use std::io::Write;

    async fn test_db() -> persistence_core::Database {
        persistence_core::test_support::setup_db().await
    }

    fn write_fits_with_imagetyp(dir: &Path, name: &str, imagetyp: &str) {
        let path = dir.join(name);
        // Write minimal valid FITS-like content with an IMAGETYP card
        // (not a real FITS file but sufficient for the header extractor to try)
        let mut data = vec![b' '; 2880];
        let card = format!("IMAGETYP= '{imagetyp:<8}'");
        let bytes = card.as_bytes();
        let len = bytes.len().min(80);
        data[0..len].copy_from_slice(&bytes[..len]);
        // END card
        data[80..83].copy_from_slice(b"END");
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(&data).unwrap();
    }

    #[tokio::test]
    async fn classify_single_type_light_folder() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits_with_imagetyp(tmp.path(), "light_001.fits", "Light Frame");
        write_fits_with_imagetyp(tmp.path(), "light_002.fits", "Light Frame");

        let db = test_db().await;

        // Insert the item
        let item_id = "item-classify-1";
        repo::insert_inbox_item(
            db.pool(),
            &InsertInboxItem {
                id: item_id,
                root_id: "root-1",
                relative_path: "",
                file_count: 0,
                content_signature: None,
                lane: "fits",
            },
        )
        .await
        .unwrap();

        let req = ClassifyRequest {
            inbox_item_id: item_id.to_owned(),
            root_absolute_path: tmp.path().to_owned(),
            force_rescan: false,
        };

        let resp = classify(db.pool(), req).await.unwrap();
        assert_eq!(resp.classification_type, "single_type");
        assert_eq!(resp.frame_type, Some("light".to_owned()));
        assert!(!resp.content_signature.is_empty());
    }

    /// Build the standard one-light-frame fixture used by the #1101 regression
    /// tests: a temp dir holding one classifiable FITS file plus its inbox row.
    async fn single_light_fixture(db: &Database, item_id: &str) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        write_fits_with_imagetyp(tmp.path(), "light_001.fits", "Light Frame");
        repo::insert_inbox_item(
            db.pool(),
            &InsertInboxItem {
                id: item_id,
                root_id: "root-1",
                relative_path: "",
                file_count: 0,
                content_signature: None,
                lane: "fits",
            },
        )
        .await
        .unwrap();
        tmp
    }

    /// Regression (#1101, headline site): when the `state = 'classified'` write
    /// fails, classify MUST NOT return `Ok`. Previously the result was dropped
    /// with `.await.ok()`, so the caller was told the item was classified while
    /// the row still said otherwise — a false statement in the DB and the root
    /// of the #711 badge disagreement.
    ///
    /// The failure is injected with a trigger scoped to exactly that write, so
    /// every earlier write in the run still succeeds and only the state
    /// transition breaks.
    #[tokio::test]
    async fn classify_surfaces_failed_item_state_write() {
        let db = test_db().await;
        let item_id = "item-1101-state";
        let tmp = single_light_fixture(&db, item_id).await;

        // Spec 058 SC-003: `classify` only writes `state = 'classified'` for a
        // row that carries its OWN frame type — a folder row does not, and
        // marking it classified is the #711 badge defect. This test is about
        // error PROPAGATION, not about which state value is chosen, so give the
        // row a frame type: that is the case where the write this trigger
        // blocks is legitimately attempted.
        sqlx::query("UPDATE inbox_items SET frame_type = 'light' WHERE id = ?")
            .bind(item_id)
            .execute(db.pool())
            .await
            .unwrap();

        sqlx::query(
            "CREATE TRIGGER block_classified BEFORE UPDATE ON inbox_items \
             WHEN NEW.state = 'classified' \
             BEGIN SELECT RAISE(ABORT, 'injected: state write failed'); END",
        )
        .execute(db.pool())
        .await
        .unwrap();

        let err = classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: item_id.to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: false,
            },
        )
        .await
        .expect_err("classify must not report success when the state write fails");

        assert_eq!(err.code, ErrorCode::InternalDatabase);
        assert!(
            err.message.contains("mark item classified"),
            "error must name the failed operation, got: {}",
            err.message
        );

        // The DB must not claim the item is classified either.
        let state: String = sqlx::query_scalar("SELECT state FROM inbox_items WHERE id = ?")
            .bind(item_id)
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_ne!(state, "classified");
    }

    /// Regression (#1101): evidence rows ARE the classification result, so a
    /// failed evidence write must surface rather than yielding a response
    /// describing rows that were never persisted.
    #[tokio::test]
    async fn classify_surfaces_failed_evidence_write() {
        let db = test_db().await;
        let item_id = "item-1101-evidence";
        let tmp = single_light_fixture(&db, item_id).await;

        sqlx::query("DROP TABLE inbox_classification_evidence").execute(db.pool()).await.unwrap();

        let err = classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: item_id.to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: false,
            },
        )
        .await
        .expect_err("classify must not report success when the evidence write fails");

        assert_eq!(err.code, ErrorCode::InternalDatabase);
    }

    /// Regression (PR #457 Layer-2 Inbox journeys): a SECOND classify of an
    /// unchanged item hits `build_response_from_cache`, which must translate
    /// the persisted DB vocabulary ('classified', migration 0049) back to the
    /// API vocabulary ('single_type'). Leaking 'classified' permanently
    /// disabled the frontend's Confirm gate (`canConfirm` requires exactly
    /// 'single_type') for any already-classified item.
    #[tokio::test]
    async fn classify_cache_hit_keeps_api_vocabulary() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits_with_imagetyp(tmp.path(), "light_001.fits", "Light Frame");

        let db = test_db().await;
        let item_id = "item-classify-cache-vocab";
        repo::insert_inbox_item(
            db.pool(),
            &InsertInboxItem {
                id: item_id,
                root_id: "root-1",
                relative_path: "",
                file_count: 0,
                content_signature: None,
                lane: "fits",
            },
        )
        .await
        .unwrap();

        let req = || ClassifyRequest {
            inbox_item_id: item_id.to_owned(),
            root_absolute_path: tmp.path().to_owned(),
            force_rescan: false,
        };

        let first = classify(db.pool(), req()).await.unwrap();
        assert_eq!(first.classification_type, "single_type");

        // Unchanged content → this is the cache path.
        let second = classify(db.pool(), req()).await.unwrap();
        assert_eq!(
            second.classification_type, "single_type",
            "cache-hit classify must return the API vocabulary, not the DB 'classified' value"
        );
        assert_eq!(second.frame_type, Some("light".to_owned()));
        assert_eq!(second.content_signature, first.content_signature);
    }

    /// Spec 058 T035: a folder spanning two frame types reports `unclassified`,
    /// not `mixed`.
    ///
    /// The vocabulary is retired, not the behaviour. The breakdown still
    /// carries both types — that is what the UI needs to explain why the folder
    /// cannot be confirmed as one thing — and the DB result is unchanged, since
    /// it stored `unclassified` for this case all along. Only the API label
    /// collapses, because `mixed` could only ever be observed on a
    /// pre-materialization placeholder and T012 stopped creating those.
    #[tokio::test]
    async fn classify_multi_type_folder_reports_unclassified() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits_with_imagetyp(tmp.path(), "light.fits", "Light Frame");
        write_fits_with_imagetyp(tmp.path(), "dark.fits", "Dark Frame");

        let db = test_db().await;
        let item_id = "item-classify-mixed";
        repo::insert_inbox_item(
            db.pool(),
            &InsertInboxItem {
                id: item_id,
                root_id: "root-1",
                relative_path: "",
                file_count: 0,
                content_signature: None,
                lane: "fits",
            },
        )
        .await
        .unwrap();

        let req = ClassifyRequest {
            inbox_item_id: item_id.to_owned(),
            root_absolute_path: tmp.path().to_owned(),
            force_rescan: false,
        };

        let resp = classify(db.pool(), req).await.unwrap();
        assert_eq!(resp.classification_type, "unclassified");
        assert!(resp.frame_type.is_none());
        assert_eq!(resp.breakdown.len(), 2);
    }

    /// Regression (#549): a detected calibration master is extracted into its
    /// own `inbox_items` row at scan time, but the file itself is never moved
    /// off disk. Classifying the folder PLACEHOLDER (this item, is_master_item
    /// = 0) must not re-tally that master into the breakdown or file_count —
    /// the placeholder represents only the un-extracted remainder. Before the
    /// fix `classify` walked every FITS/XISF file still in the folder, so a
    /// folder of 2 un-extracted lights + 1 already-extracted master dark
    /// reported "mixed"/3 files instead of "single_type light"/2 files.
    #[tokio::test]
    async fn classify_excludes_already_extracted_master_from_placeholder_tally() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits_with_imagetyp(tmp.path(), "light_001.fits", "Light Frame");
        write_fits_with_imagetyp(tmp.path(), "light_002.fits", "Light Frame");
        write_fits_with_imagetyp(tmp.path(), "master_dark.fits", "Master Dark");

        let db = test_db().await;
        let item_id = "item-classify-placeholder-with-master";
        repo::insert_inbox_item(
            db.pool(),
            &InsertInboxItem {
                id: item_id,
                root_id: "root-1",
                relative_path: "",
                file_count: 0,
                content_signature: None,
                lane: "fits",
            },
        )
        .await
        .unwrap();

        let req = ClassifyRequest {
            inbox_item_id: item_id.to_owned(),
            root_absolute_path: tmp.path().to_owned(),
            force_rescan: false,
        };

        let resp = classify(db.pool(), req).await.unwrap();

        assert_eq!(
            resp.classification_type, "single_type",
            "the master must not turn this into a mixed folder: {:?}",
            resp.breakdown
        );
        assert_eq!(resp.breakdown.len(), 1);
        assert_eq!(resp.breakdown[0].kind, "light");
        assert_eq!(
            resp.breakdown[0].count, 2,
            "breakdown must not double-count the already-extracted master (#549)"
        );

        let item = repo::get_inbox_item(db.pool(), item_id).await.unwrap();
        assert_eq!(
            item.file_count, 2,
            "placeholder file_count must be the un-extracted remainder (#549), not the full folder"
        );
    }

    #[tokio::test]
    async fn classify_no_imagetyp_returns_unclassified() {
        let tmp = tempfile::tempdir().unwrap();
        // Write a file with no IMAGETYP card
        let path = tmp.path().join("mystery.fits");
        let mut data = vec![b' '; 2880];
        data[0..3].copy_from_slice(b"END");
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(&data).unwrap();

        let db = test_db().await;
        let item_id = "item-unclassified";
        repo::insert_inbox_item(
            db.pool(),
            &InsertInboxItem {
                id: item_id,
                root_id: "root-1",
                relative_path: "",
                file_count: 0,
                content_signature: None,
                lane: "fits",
            },
        )
        .await
        .unwrap();

        let req = ClassifyRequest {
            inbox_item_id: item_id.to_owned(),
            root_absolute_path: tmp.path().to_owned(),
            force_rescan: false,
        };

        let resp = classify(db.pool(), req).await.unwrap();
        assert_eq!(resp.classification_type, "unclassified");
        assert_eq!(resp.unclassified_files.len(), 1);
    }

    /// spec 041 US2/T016: per-file metadata rows are persisted during classify
    /// and carry the parsed header fields (here: OBJECT/FILTER/DATE-OBS).
    #[tokio::test]
    async fn classify_persists_per_file_metadata() {
        use persistence_inbox::repositories::inbox as inbox_repo;

        let tmp = tempfile::tempdir().unwrap();
        // Reuse the richer FITS writer from the confirm tests to embed headers.
        let path = tmp.path().join("light_001.fits");
        let mut block = vec![b' '; 2880];
        let mut idx = 0usize;
        let mut write_card = |block: &mut Vec<u8>, card: &str| {
            let bytes = card.as_bytes();
            let len = bytes.len().min(80);
            block[idx * 80..idx * 80 + len].copy_from_slice(&bytes[..len]);
            idx += 1;
        };
        write_card(&mut block, &format!("{:<80}", "IMAGETYP= 'Light Frame'"));
        write_card(&mut block, &format!("{:<80}", "OBJECT  = 'M42'"));
        write_card(&mut block, &format!("{:<80}", "FILTER  = 'Ha'"));
        write_card(&mut block, &format!("{:<80}", "DATE-OBS= '2025-10-10T22:00:00'"));
        block[idx * 80..idx * 80 + 3].copy_from_slice(b"END");
        std::fs::File::create(&path).unwrap().write_all(&block).unwrap();

        let db = test_db().await;
        let item_id = "item-meta-persist";
        repo::insert_inbox_item(
            db.pool(),
            &InsertInboxItem {
                id: item_id,
                root_id: "root-1",
                relative_path: "",
                file_count: 0,
                content_signature: None,
                lane: "fits",
            },
        )
        .await
        .unwrap();

        classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: item_id.to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: false,
            },
        )
        .await
        .unwrap();

        let rows = inbox_repo::list_inbox_file_metadata(db.pool(), item_id).await.unwrap();
        assert_eq!(rows.len(), 1, "one metadata row per classified file");
        assert_eq!(rows[0].relative_file_path, "light_001.fits");
        assert_eq!(rows[0].object.as_deref(), Some("M42"));
        assert_eq!(rows[0].filter.as_deref(), Some("Ha"));
        assert_eq!(rows[0].date_obs.as_deref(), Some("2025-10-10T22:00:00"));
        // File identity is recorded for override-staleness tracking (R-4).
        assert!(rows[0].file_size_bytes.is_some());

        // Re-classify (force) must REPLACE rows, not duplicate them.
        classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: item_id.to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: true,
            },
        )
        .await
        .unwrap();
        let rows2 = inbox_repo::list_inbox_file_metadata(db.pool(), item_id).await.unwrap();
        assert_eq!(rows2.len(), 1, "re-classify replaces, does not duplicate");
    }

    /// spec 041 US3/T018: the breakdown resolves a `destination_preview` for the
    /// light group via the default Naming & Structure pattern.
    #[tokio::test]
    async fn classify_breakdown_has_destination_preview() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("light_001.fits");
        let mut block = vec![b' '; 2880];
        let mut idx = 0usize;
        let mut write_card = |block: &mut Vec<u8>, card: &str| {
            let bytes = card.as_bytes();
            let len = bytes.len().min(80);
            block[idx * 80..idx * 80 + len].copy_from_slice(&bytes[..len]);
            idx += 1;
        };
        write_card(&mut block, &format!("{:<80}", "IMAGETYP= 'Light Frame'"));
        write_card(&mut block, &format!("{:<80}", "OBJECT  = 'M42'"));
        write_card(&mut block, &format!("{:<80}", "FILTER  = 'Ha'"));
        write_card(&mut block, &format!("{:<80}", "DATE-OBS= '2025-10-10T22:00:00'"));
        block[idx * 80..idx * 80 + 3].copy_from_slice(b"END");
        std::fs::File::create(&path).unwrap().write_all(&block).unwrap();

        let db = test_db().await;
        let item_id = "item-dest-preview";
        repo::insert_inbox_item(
            db.pool(),
            &InsertInboxItem {
                id: item_id,
                root_id: "root-1",
                relative_path: "",
                file_count: 0,
                content_signature: None,
                lane: "fits",
            },
        )
        .await
        .unwrap();

        let resp = classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: item_id.to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: false,
            },
        )
        .await
        .unwrap();

        let light = resp.breakdown.iter().find(|b| b.kind == "light").expect("light group");
        let preview = light.destination_preview.as_deref().expect("destination preview resolved");
        assert!(!preview.is_empty(), "preview path is non-empty: {preview}");
    }

    /// spec 041 R-4: when a file's size changes between two classify runs AND
    /// it had an override set, the second classify must mark override_stale = 1
    /// on the freshly-inserted evidence row.
    #[tokio::test]
    async fn classify_rescan_marks_override_stale_on_changed_file() {
        use persistence_inbox::repositories::inbox as inbox_repo;
        use std::io::Write;

        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("light_001.fits");

        // Write initial file content.
        write_fits_with_imagetyp(tmp.path(), "light_001.fits", "Light Frame");

        let db = test_db().await;
        let item_id = "item-r4-stale";

        repo::insert_inbox_item(
            db.pool(),
            &InsertInboxItem {
                id: item_id,
                root_id: "root-1",
                relative_path: "",
                file_count: 0,
                content_signature: None,
                lane: "fits",
            },
        )
        .await
        .unwrap();

        // First classify: no overrides yet.
        classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: item_id.to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: false,
            },
        )
        .await
        .unwrap();

        // Apply a non-type override.
        inbox_repo::set_overrides(
            db.pool(),
            item_id,
            "light_001.fits",
            None,
            Some("Ha"),
            Some(300.0),
            Some("2x2"),
        )
        .await
        .unwrap();

        // Verify not stale yet.
        let ev_before = inbox_repo::list_evidence(db.pool(), item_id).await.unwrap();
        let ev0 = ev_before.iter().find(|e| e.relative_file_path == "light_001.fits").unwrap();
        assert_eq!(ev0.override_stale, 0, "override freshly set, not yet stale");

        // Mutate the file so its size changes.
        {
            let mut f = std::fs::OpenOptions::new().append(true).open(&file_path).unwrap();
            f.write_all(b"extra bytes that change size").unwrap();
        }

        // Second classify (force rescan): should detect size change and mark stale.
        classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: item_id.to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: true,
            },
        )
        .await
        .unwrap();

        // After rescan, override_stale should be 1 AND the override VALUE must survive.
        let ev_after = inbox_repo::list_evidence(db.pool(), item_id).await.unwrap();
        let ev1 = ev_after.iter().find(|e| e.relative_file_path == "light_001.fits").unwrap();
        assert_eq!(ev1.override_stale, 1, "override_stale must be 1 after file size changed");
        // The override values must survive the rescan even when marked stale.
        assert_eq!(
            ev1.override_filter.as_deref(),
            Some("Ha"),
            "override_filter must survive rescan"
        );
        assert_eq!(ev1.override_exposure_s, Some(300.0), "override_exposure_s must survive rescan");
        assert_eq!(
            ev1.override_binning.as_deref(),
            Some("2x2"),
            "override_binning must survive rescan"
        );
    }

    /// R-4 negative guard: a force rescan that observes the SAME size/mtime must
    /// NOT mark an override stale. Protects against the size/mtime identity being
    /// stored and compared in mismatched formats (which would make every rescan
    /// falsely report staleness).
    #[tokio::test]
    async fn classify_rescan_keeps_override_fresh_when_file_unchanged() {
        use persistence_inbox::repositories::inbox as inbox_repo;

        let tmp = tempfile::tempdir().unwrap();
        write_fits_with_imagetyp(tmp.path(), "light_001.fits", "Light Frame");

        let db = test_db().await;
        let item_id = "item-r4-fresh";

        repo::insert_inbox_item(
            db.pool(),
            &InsertInboxItem {
                id: item_id,
                root_id: "root-1",
                relative_path: "",
                file_count: 0,
                content_signature: None,
                lane: "fits",
            },
        )
        .await
        .unwrap();

        classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: item_id.to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: false,
            },
        )
        .await
        .unwrap();

        inbox_repo::set_overrides(
            db.pool(),
            item_id,
            "light_001.fits",
            None,
            Some("Ha"),
            Some(300.0),
            Some("2x2"),
        )
        .await
        .unwrap();

        // Force rescan WITHOUT touching the file.
        classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: item_id.to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: true,
            },
        )
        .await
        .unwrap();

        let ev = inbox_repo::list_evidence(db.pool(), item_id).await.unwrap();
        let row = ev.iter().find(|e| e.relative_file_path == "light_001.fits").unwrap();
        assert_eq!(
            row.override_stale, 0,
            "unchanged file must not mark the override stale on rescan"
        );
        // The override values must also survive the rescan.
        assert_eq!(row.override_filter.as_deref(), Some("Ha"));
    }

    /// spec 041 FR-046: a generic `inbox_file_overrides` entry (set via
    /// `set_file_override`, the reclassify_v2/cone_search path — not the
    /// legacy `set_overrides` evidence columns exercised above) must be
    /// flagged `override_stale` once classify observes the file's on-disk
    /// identity has drifted from what was recorded when the override was set.
    #[tokio::test]
    async fn classify_marks_generic_file_override_stale_on_changed_file() {
        use persistence_inbox::repositories::inbox as inbox_repo;
        use std::io::Write;

        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("light_001.fits");
        write_fits_with_imagetyp(tmp.path(), "light_001.fits", "Light Frame");

        let db = test_db().await;
        insert_source_group_with_item(&db, "sg-fovr-stale", "item-fovr-stale", "root-fovr", "")
            .await;

        // First classify records the file's identity in inbox_file_metadata.
        classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: "item-fovr-stale".to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: false,
            },
        )
        .await
        .unwrap();

        let identity =
            inbox_repo::list_inbox_file_metadata(db.pool(), "item-fovr-stale").await.unwrap();
        let (size, mtime) = identity
            .iter()
            .find(|m| m.relative_file_path == "light_001.fits")
            .map(|m| (m.file_size_bytes, m.file_mtime.clone()))
            .expect("identity recorded by first classify");

        // Set a generic override carrying that identity (mirrors what
        // reclassify_v2/cone_search now do).
        inbox_repo::set_file_override(
            db.pool(),
            "sg-fovr-stale",
            "light_001.fits",
            "gain",
            "100",
            size,
            mtime.as_deref(),
        )
        .await
        .unwrap();

        let before =
            inbox_repo::list_file_overrides_for_group(db.pool(), "sg-fovr-stale").await.unwrap();
        assert_eq!(before[0].override_stale, 0, "override freshly set, not yet stale");

        // Mutate the file so its size changes.
        {
            let mut f = std::fs::OpenOptions::new().append(true).open(&file_path).unwrap();
            f.write_all(b"extra bytes that change size").unwrap();
        }

        classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: "item-fovr-stale".to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: true,
            },
        )
        .await
        .unwrap();

        let after =
            inbox_repo::list_file_overrides_for_group(db.pool(), "sg-fovr-stale").await.unwrap();
        let ov = after.iter().find(|o| o.relative_file_path == "light_001.fits").unwrap();
        assert_eq!(ov.override_stale, 1, "override_stale must be 1 after file size changed");
        assert_eq!(ov.value, "100", "override value must survive being marked stale");
    }

    // ── T066: sub-item materialization tests ─────────────────────────────────

    /// Insert a source group + inbox item with source_group_id set.
    /// Returns (source_group_id, inbox_item_id).
    async fn insert_source_group_with_item(
        db: &Database,
        sg_id: &str,
        item_id: &str,
        root_id: &str,
        relative_path: &str,
    ) {
        let pool = db.pool();
        // Insert registered_sources row (FK required by inbox_source_groups).
        //
        // This INSERT must succeed: any query under test that JOINs
        // `registered_sources` measures an empty table otherwise, and
        // assertions of *absence* ("no rows", "item hidden") then pass
        // vacuously — for the wrong reason, and they would keep passing if the
        // production query broke (#1252).
        //
        // The old statement violated the 0006 schema four ways at once
        // (missing NOT NULL `created_at` and `created_via`, `scan_depth` given
        // `1` against `CHECK (IN ('recursive','single'))`, and a non-existent
        // `organization_state` column), so it had never once inserted a row.
        //
        // What HID that was `INSERT OR IGNORE`, not the `.ok()` it was paired
        // with: `OR IGNORE` makes SQLite swallow the constraint violation
        // itself, so no error ever reaches sqlx and an `.expect()` here would
        // have been equally silent. Verified both ways — `OR IGNORE` with
        // `.expect()` still passes; the form below panics with
        // `NOT NULL constraint failed: registered_sources.created_at`.
        //
        // So idempotency is expressed as a bare `ON CONFLICT DO NOTHING`,
        // which suppresses only genuine uniqueness conflicts (the PK and
        // `UNIQUE(kind, path)`, both expected when this helper runs more than
        // once per test) while letting NOT NULL and CHECK violations surface.
        // That, not the `.expect()`, is what keeps this from drifting again.
        sqlx::query(
            "INSERT INTO registered_sources \
             (id, path, kind, scan_depth, created_at, created_via) \
             VALUES (?, '/test/root', 'inbox', 'recursive', \
             '2026-01-01T00:00:00Z', 'first_run') \
             ON CONFLICT DO NOTHING",
        )
        .bind(root_id)
        .execute(pool)
        .await
        .expect("test fixture: registered_sources INSERT must succeed");

        // Insert source group.
        sqlx::query(
            "INSERT INTO inbox_source_groups \
             (id, root_id, relative_path, discovered_at, last_scanned_at, child_count) \
             VALUES (?, ?, ?, '2025-10-10T20:00:00Z', '2025-10-10T20:00:00Z', 0)",
        )
        .bind(sg_id)
        .bind(root_id)
        .bind(relative_path)
        .execute(pool)
        .await
        .unwrap();

        // Insert inbox_item with source_group_id.
        sqlx::query(
            "INSERT INTO inbox_items \
             (id, root_id, relative_path, source_group_id, group_key, \
              discovered_at, last_scanned_at, state, lane) \
             VALUES (?, ?, ?, ?, '', \
                     '2025-10-10T20:00:00Z', '2025-10-10T20:00:00Z', \
                     'pending_classification', 'fits')",
        )
        .bind(item_id)
        .bind(root_id)
        .bind(relative_path)
        .bind(sg_id)
        .execute(pool)
        .await
        .unwrap();
    }

    /// #1252: guard the fixture's own precondition.
    ///
    /// `insert_source_group_with_item` seeds `registered_sources` because
    /// queries under test JOIN it. That INSERT silently inserted nothing for
    /// its whole existence, so every such JOIN was measuring an empty table
    /// and any assertion of absence passed vacuously.
    ///
    /// Asserting the row is present — rather than merely that the statement
    /// did not error — is the part that survives future schema drift, since
    /// a conflict-suppressing INSERT can succeed while writing nothing.
    #[tokio::test]
    async fn fixture_actually_seeds_registered_sources_1252() {
        let db = test_db().await;
        insert_source_group_with_item(&db, "sg-fixture", "item-fixture", "root-fixture", "a/b")
            .await;

        let (count, path): (i64, String) = sqlx::query_as(
            "SELECT COUNT(*), COALESCE(MAX(path), '') FROM registered_sources WHERE id = ?",
        )
        .bind("root-fixture")
        .fetch_one(db.pool())
        .await
        .unwrap();

        assert_eq!(count, 1, "the fixture must actually create its registered_sources row");
        assert_eq!(path, "/test/root", "and it must be the row the helper claims to insert");
    }

    #[tokio::test]
    async fn t066_single_type_folder_produces_one_sub_item() {
        // A folder with only light frames → one single-type sub-item.
        // T070: lights need OBJECT+FILTER+EXPTIME to pass the mandatory-attr gate.
        let tmp = tempfile::tempdir().unwrap();
        write_fits_full(
            tmp.path(),
            "light_001.fits",
            "Light Frame",
            Some("M42"),
            Some("Ha"),
            Some(300.0),
            None,
        );
        write_fits_full(
            tmp.path(),
            "light_002.fits",
            "Light Frame",
            Some("M42"),
            Some("Ha"),
            Some(300.0),
            None,
        );

        let db = test_db().await;
        insert_source_group_with_item(&db, "sg-t066-single", "item-t066-single", "root-sg1", "")
            .await;

        classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: "item-t066-single".to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: false,
            },
        )
        .await
        .unwrap();

        // Exactly one classified sub-item in the source group.
        let sub_items =
            inbox_repo::list_inbox_sub_items(db.pool(), "sg-t066-single").await.unwrap();
        assert_eq!(sub_items.len(), 1, "single-type folder must produce exactly one sub-item");
        let si = &sub_items[0];
        assert_eq!(si.frame_type.as_deref(), Some("light"), "sub-item frame_type must be 'light'");
        assert!(si.group_key.starts_with("type=light"), "group_key must start with type=light");
        assert!(si.content_signature.is_some(), "sub-item must have a content_signature");
        assert_eq!(si.file_count, 2, "sub-item file_count must match files in the group");

        // Source group child_count updated.
        let sg =
            q_inbox::get_source_group_by_id(db.pool(), "sg-t066-single").await.unwrap().unwrap();
        assert_eq!(sg.child_count, 1, "source group child_count must be 1");
    }

    /// Spec 058 T031 / FR-003 / SC-002: a folder with N distinct groups yields
    /// exactly N items **and no aggregate**.
    ///
    /// The existing `t066_*` tests pin the N half, but they were written while
    /// the folder placeholder still existed — they counted sub-items beside an
    /// aggregate rather than instead of one. This asserts both halves together,
    /// through the post-FR-015 entry point: a bare source group with no item
    /// row, classified via `classify_source_group`.
    ///
    /// The no-aggregate half is asserted by comparing the UNFILTERED item count
    /// for the group against the sub-item count. `list_inbox_sub_items` filters
    /// `group_key != ''` in SQL, so asserting "no returned row has an empty
    /// group_key" against it would be vacuous — it cannot return one. Only the
    /// unfiltered `list_item_ids_for_source_group` can observe a resurrected
    /// parent, so the two counts must agree.
    #[tokio::test]
    async fn t031_two_frame_types_yield_two_items_and_no_aggregate() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits_with_imagetyp(tmp.path(), "light.fits", "Light Frame");
        write_fits_with_imagetyp(tmp.path(), "dark.fits", "Dark Frame");

        let db = test_db().await;
        inbox_repo::upsert_inbox_source_group(
            db.pool(),
            &inbox_repo::UpsertSourceGroup {
                id: "sg-t031",
                root_id: "root-t031",
                relative_path: "",
                content_signature: Some("sig-t031"),
                format: Some("fits"),
                lane: Some("move"),
                file_count: 2,
            },
        )
        .await
        .unwrap();

        // No inbox_items row exists — the post-FR-015 shape scan now produces.
        let resp = classify_source_group(db.pool(), "sg-t031", tmp.path()).await.unwrap();
        assert_eq!(
            resp.materialized_sub_item_count, 2,
            "a folder holding lights and darks must materialize exactly two items"
        );

        let sub_items = inbox_repo::list_inbox_sub_items(db.pool(), "sg-t031").await.unwrap();
        assert_eq!(sub_items.len(), 2, "expected exactly two sub-items for this source group");

        let all_items =
            inbox_repo::list_item_ids_for_source_group(db.pool(), "sg-t031").await.unwrap();
        assert_eq!(
            all_items.len(),
            sub_items.len(),
            "every inbox_items row for this group must BE one of its sub-items; \
             a surplus row is an aggregate (group_key = ''), which FR-003 forbids"
        );
    }

    #[tokio::test]
    async fn t066_mixed_folder_produces_n_sub_items() {
        // A folder with lights + darks → two single-type sub-items.
        // T070: lights need OBJECT+FILTER+EXPTIME; darks need EXPTIME+GAIN.
        let tmp = tempfile::tempdir().unwrap();
        write_fits_full(
            tmp.path(),
            "light_ha.fits",
            "Light Frame",
            Some("M42"),
            Some("Ha"),
            Some(300.0),
            None,
        );
        write_fits_full(
            tmp.path(),
            "dark_1.fits",
            "Dark Frame",
            None,
            None,
            Some(300.0),
            Some("100"),
        );
        write_fits_full(
            tmp.path(),
            "dark_2.fits",
            "Dark Frame",
            None,
            None,
            Some(300.0),
            Some("100"),
        );

        let db = test_db().await;
        insert_source_group_with_item(&db, "sg-t066-mixed", "item-t066-mixed", "root-sg2", "")
            .await;

        classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: "item-t066-mixed".to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: false,
            },
        )
        .await
        .unwrap();

        let sub_items = inbox_repo::list_inbox_sub_items(db.pool(), "sg-t066-mixed").await.unwrap();
        assert_eq!(sub_items.len(), 2, "mixed folder must produce one sub-item per frame type");

        let types: Vec<_> = sub_items.iter().filter_map(|s| s.frame_type.as_deref()).collect();
        assert!(types.contains(&"light"), "must have a light sub-item");
        assert!(types.contains(&"dark"), "must have a dark sub-item");

        // Each sub-item has its own group_key and content_signature.
        let keys: std::collections::HashSet<_> =
            sub_items.iter().map(|s| s.group_key.as_str()).collect();
        assert_eq!(keys.len(), 2, "each sub-item must have a distinct group_key");

        for si in &sub_items {
            assert!(si.content_signature.is_some(), "each sub-item must have a content_signature");
            // file_count per group (1 light, 2 darks).
            if si.frame_type.as_deref() == Some("light") {
                assert_eq!(si.file_count, 1);
            } else {
                assert_eq!(si.file_count, 2);
            }
        }

        // Source group child_count updated.
        let sg =
            q_inbox::get_source_group_by_id(db.pool(), "sg-t066-mixed").await.unwrap().unwrap();
        assert_eq!(sg.child_count, 2, "source group child_count must be 2");
    }

    #[tokio::test]
    async fn t066_rescan_determinism() {
        // Classifying unchanged content twice must produce identical group keys
        // and no duplicated sub-items (FR-042).
        // T070: lights need OBJECT+FILTER+EXPTIME; darks need EXPTIME+GAIN.
        let tmp = tempfile::tempdir().unwrap();
        write_fits_full(
            tmp.path(),
            "light_001.fits",
            "Light Frame",
            Some("M42"),
            Some("Ha"),
            Some(300.0),
            None,
        );
        write_fits_full(
            tmp.path(),
            "dark_001.fits",
            "Dark Frame",
            None,
            None,
            Some(300.0),
            Some("100"),
        );

        let db = test_db().await;
        insert_source_group_with_item(&db, "sg-t066-determ", "item-t066-determ", "root-sg3", "")
            .await;

        // First classify.
        classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: "item-t066-determ".to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: false,
            },
        )
        .await
        .unwrap();

        let items_first =
            inbox_repo::list_inbox_sub_items(db.pool(), "sg-t066-determ").await.unwrap();
        let keys_first: Vec<String> = items_first.iter().map(|i| i.group_key.clone()).collect();
        let sigs_first: Vec<Option<String>> =
            items_first.iter().map(|i| i.content_signature.clone()).collect();

        // Second classify (force_rescan = true on unchanged content).
        classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: "item-t066-determ".to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: true,
            },
        )
        .await
        .unwrap();

        let items_second =
            inbox_repo::list_inbox_sub_items(db.pool(), "sg-t066-determ").await.unwrap();
        let keys_second: Vec<String> = items_second.iter().map(|i| i.group_key.clone()).collect();
        let sigs_second: Vec<Option<String>> =
            items_second.iter().map(|i| i.content_signature.clone()).collect();

        // Same group_keys.
        assert_eq!(
            keys_first, keys_second,
            "rescan of unchanged content must produce identical group_keys (FR-042)"
        );
        // Same signatures.
        assert_eq!(
            sigs_first, sigs_second,
            "rescan of unchanged content must produce identical content_signatures (FR-042)"
        );
        // No duplicates — still exactly 2 sub-items.
        assert_eq!(items_second.len(), 2, "rescan must not create duplicate sub-items");
    }

    #[tokio::test]
    async fn t066_unclassifiable_file_goes_to_sentinel_bucket() {
        // A file with no IMAGETYP → one needs-review sub-item (spec 058 FR-028).
        let tmp = tempfile::tempdir().unwrap();
        // No IMAGETYP card.
        let path = tmp.path().join("mystery.fits");
        let mut data = vec![b' '; 2880];
        data[0..3].copy_from_slice(b"END");
        std::fs::write(&path, &data).unwrap();

        let db = test_db().await;
        insert_source_group_with_item(
            &db,
            "sg-t066-sentinel",
            "item-t066-sentinel",
            "root-sg4",
            "",
        )
        .await;

        classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: "item-t066-sentinel".to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: false,
            },
        )
        .await
        .unwrap();

        let sub_items =
            inbox_repo::list_inbox_sub_items(db.pool(), "sg-t066-sentinel").await.unwrap();
        assert_eq!(
            sub_items.len(),
            1,
            "unclassifiable file must produce one needs-review sub-item"
        );
        let si = &sub_items[0];
        assert_eq!(si.needs_review, 1, "unclassifiable file must be flagged needs_review");
        assert_eq!(
            si.group_key, GROUP_KEY_TYPE_UNKNOWN,
            "a file with no determinable frame type keys on the undetermined-type identity"
        );
        assert!(si.frame_type.is_none(), "needs-review sub-item must have no frame_type");
    }

    // ── T067: composite identity + signature stability (FR-042) ──────────────

    /// Write a minimal FITS file whose IMAGETYP + FILTER headers are embedded,
    /// allowing the grouping engine to distinguish the filter dimension.
    ///
    /// Unlike `write_fits_with_imagetyp`, this also embeds a FILTER card so that
    /// two files written with different filter values end up in different sub-groups.
    fn write_fits_with_filter(dir: &std::path::Path, name: &str, imagetyp: &str, filter: &str) {
        use std::io::Write as _;

        let path = dir.join(name);
        let mut data = vec![b' '; 2880];
        let mut idx = 0usize;

        let mut write_card = |card: &str| {
            let bytes = card.as_bytes();
            let len = bytes.len().min(80);
            data[idx * 80..idx * 80 + len].copy_from_slice(&bytes[..len]);
            idx += 1;
        };

        write_card(&format!("{:<80}", format!("IMAGETYP= '{imagetyp:<8}'")));
        write_card(&format!("{:<80}", format!("FILTER  = '{filter:<8}'")));
        data[idx * 80..idx * 80 + 3].copy_from_slice(b"END");

        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(&data).unwrap();
    }

    /// T067-1 — Composite identity uniqueness.
    ///
    /// Two flat files that differ ONLY in the `FILTER` header (a grouping
    /// dimension) must materialise as **two distinct sub-items** with distinct
    /// `group_key`s. The identity triple `(root_id, relative_path, group_key)`
    /// must be unique across the pair (R-11).
    #[tokio::test]
    async fn t067_composite_identity_differs_by_grouping_dimension() {
        let tmp = tempfile::tempdir().unwrap();
        // Two flat files — same folder, same type, but different filters.
        // The flat recipe includes Filter as a dimension, so they split.
        write_fits_with_filter(tmp.path(), "flat_ha.fits", "Flat Frame", "Ha");
        write_fits_with_filter(tmp.path(), "flat_oiii.fits", "Flat Frame", "OIII");

        let db = test_db().await;
        insert_source_group_with_item(&db, "sg-t067-id", "item-t067-id", "root-t067a", "").await;

        classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: "item-t067-id".to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: false,
            },
        )
        .await
        .unwrap();

        let sub_items = inbox_repo::list_inbox_sub_items(db.pool(), "sg-t067-id").await.unwrap();

        // Two files in different filter groups → two sub-items.
        assert_eq!(
            sub_items.len(),
            2,
            "flat files differing only in FILTER must produce two distinct sub-items; got {sub_items:?}"
        );

        let keys: std::collections::HashSet<_> =
            sub_items.iter().map(|s| s.group_key.as_str()).collect();
        assert_eq!(
            keys.len(),
            2,
            "group_key must differ between the two filter groups — identity is not unique: {sub_items:?}"
        );

        // Both sub-items share the same (root_id, relative_path) — only group_key differs.
        for si in &sub_items {
            assert_eq!(si.root_id, "root-t067a");
            assert_eq!(si.relative_path, "");
            assert!(
                si.group_key.contains("filter="),
                "group_key must embed the filter dimension: {}",
                si.group_key
            );
        }

        // The two group_keys contain different filter values.
        let filter_values: Vec<_> = sub_items
            .iter()
            .map(|s| {
                s.group_key
                    .split('·')
                    .find(|seg| seg.starts_with("filter="))
                    .unwrap_or("filter=<missing>")
                    .to_owned()
            })
            .collect();
        assert_ne!(
            filter_values[0], filter_values[1],
            "the two group_keys must embed different filter tokens; both read: {}",
            filter_values[0]
        );
    }

    /// T067-2 — Per-sub-group signature correctness.
    ///
    /// Each sub-item's `content_signature` must equal
    /// `folder_signature(sorted(per-file sigs of only the files in that group))`
    /// (R-11). Two sub-groups in the same folder must therefore have **different**
    /// signatures because they cover different file sets.
    #[tokio::test]
    async fn t067_per_subgroup_signatures_differ_and_match_formula() {
        let tmp = tempfile::tempdir().unwrap();
        // One Ha flat and one OIII flat → two distinct groups.
        write_fits_with_filter(tmp.path(), "flat_ha.fits", "Flat Frame", "Ha");
        write_fits_with_filter(tmp.path(), "flat_oiii.fits", "Flat Frame", "OIII");

        let db = test_db().await;
        insert_source_group_with_item(&db, "sg-t067-sig", "item-t067-sig", "root-t067b", "").await;

        classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: "item-t067-sig".to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: false,
            },
        )
        .await
        .unwrap();

        let sub_items = inbox_repo::list_inbox_sub_items(db.pool(), "sg-t067-sig").await.unwrap();
        assert_eq!(sub_items.len(), 2, "expected two sub-items");

        // Both must have a non-empty content_signature.
        for si in &sub_items {
            assert!(
                si.content_signature.as_deref().is_some_and(|s| !s.is_empty()),
                "sub-item {} must have a non-empty content_signature",
                si.group_key
            );
        }

        // The two sub-groups share the same folder but different file sets →
        // their per-sub-group signatures must be distinct.
        let sig_a = sub_items[0].content_signature.as_deref().unwrap_or("");
        let sig_b = sub_items[1].content_signature.as_deref().unwrap_or("");
        assert_ne!(
            sig_a, sig_b,
            "two sub-groups covering different files must have distinct content_signatures"
        );

        // Verify each signature matches the formula: folder_signature(sorted per-file sigs).
        // We can do this by computing the expected signature for each group ourselves
        // and comparing — we know which file belongs to which group from the group_key.
        let ha_abs = tmp.path().join("flat_ha.fits");
        let oiii_abs = tmp.path().join("flat_oiii.fits");

        // Access signature primitives through the crate root (sibling module of classify).
        let ha_file_sig =
            crate::signature::file_signature(&ha_abs).expect("flat_ha.fits must be stat-able");
        let oiii_file_sig =
            crate::signature::file_signature(&oiii_abs).expect("flat_oiii.fits must be stat-able");

        let expected_ha_sig = crate::signature::folder_signature(vec![ha_file_sig]);
        let expected_oiii_sig = crate::signature::folder_signature(vec![oiii_file_sig]);

        // Locate which sub-item corresponds to which filter.
        let ha_item = sub_items
            .iter()
            .find(|s| s.group_key.contains("filter=ha"))
            .expect("must find Ha sub-item by group_key");
        let oiii_item = sub_items
            .iter()
            .find(|s| s.group_key.contains("filter=oiii"))
            .expect("must find OIII sub-item by group_key");

        assert_eq!(
            ha_item.content_signature.as_deref().unwrap_or(""),
            expected_ha_sig,
            "Ha sub-item signature must equal folder_signature(sorted per-file sigs of Ha group)"
        );
        assert_eq!(
            oiii_item.content_signature.as_deref().unwrap_or(""),
            expected_oiii_sig,
            "OIII sub-item signature must equal folder_signature(sorted per-file sigs of OIII group)"
        );
    }

    /// T067-3 — Rescan determinism (the key FR-042 property).
    ///
    /// Classifying identical content twice must yield:
    /// - identical `group_key` for every sub-item,
    /// - identical `content_signature` for every sub-item, and
    /// - **no new rows** (the UNIQUE constraint absorbs the upsert).
    ///
    /// This extends the existing `t066_rescan_determinism` test with an explicit
    /// filter-split scenario (two sub-items) and asserts the count stays at two.
    #[tokio::test]
    async fn t067_rescan_determinism_no_item_churn() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits_with_filter(tmp.path(), "flat_ha.fits", "Flat Frame", "Ha");
        write_fits_with_filter(tmp.path(), "flat_oiii.fits", "Flat Frame", "OIII");

        let db = test_db().await;
        insert_source_group_with_item(&db, "sg-t067-redet", "item-t067-redet", "root-t067c", "")
            .await;

        // First classify.
        classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: "item-t067-redet".to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: false,
            },
        )
        .await
        .unwrap();

        let first = inbox_repo::list_inbox_sub_items(db.pool(), "sg-t067-redet").await.unwrap();
        let first_keys: Vec<_> = first.iter().map(|s| s.group_key.clone()).collect();
        let first_sigs: Vec<_> = first.iter().map(|s| s.content_signature.clone()).collect();

        // Second classify — force rescan, same files on disk.
        classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: "item-t067-redet".to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: true,
            },
        )
        .await
        .unwrap();

        let second = inbox_repo::list_inbox_sub_items(db.pool(), "sg-t067-redet").await.unwrap();
        let second_keys: Vec<_> = second.iter().map(|s| s.group_key.clone()).collect();
        let second_sigs: Vec<_> = second.iter().map(|s| s.content_signature.clone()).collect();

        // Count must not grow.
        assert_eq!(
            second.len(),
            2,
            "rescan must not create duplicate sub-items; expected 2, got {}",
            second.len()
        );

        // Keys and signatures must be bitwise identical.
        assert_eq!(
            first_keys, second_keys,
            "rescan of unchanged content must produce identical group_keys (FR-042)"
        );
        assert_eq!(
            first_sigs, second_sigs,
            "rescan of unchanged content must produce identical content_signatures (FR-042)"
        );
    }

    /// T067-4 — Correct churn on change.
    ///
    /// When a file's metadata moves it to a different sub-group (here: we replace
    /// a file on disk with a different FILTER header so the extractor sees a new
    /// value), the affected sub-group signatures must change after the next
    /// classify.  Specifically:
    ///
    /// - The old sub-group (Ha) now has zero files → it must be absent from the
    ///   live sub-item list (not left as a stale row).
    /// - The new sub-group (SII) appears with the moved file.
    /// - The untouched group (OIII, flat_b) has a stable signature.
    ///
    /// This models the "file whose metadata/override moves it to a different group
    /// changes both its old and new sub-group signatures" requirement (R-11).
    ///
    /// # KNOWN BUG (production code)
    ///
    /// `materialize_sub_items` (classify.rs) uses `upsert_inbox_sub_item` which
    /// runs `ON CONFLICT … DO UPDATE` for groups that still exist after the rescan,
    /// but **never deletes rows for groups that have become empty**.  After flat_a
    /// moves from Ha→SII, the Ha row persists in `inbox_items` with its old
    /// `file_count = 1` — it is never cleaned up.
    ///
    /// Expected result: 2 sub-items (OIII + SII) after the rescan.
    /// Actual result:   3 sub-items (Ha stale + OIII + SII) — the Ha row survives.
    ///
    /// The failing assertion below (`ha_after.is_none()`) documents this bug.
    /// Fix required: after upserting the current groups, delete `inbox_items` rows
    /// belonging to this `source_group_id` whose `group_key` is NOT in the current
    /// set (i.e. `DELETE … WHERE source_group_id = ? AND group_key NOT IN (…)`).
    #[tokio::test]
    async fn t067_churn_on_metadata_change_updates_group_signatures() {
        let tmp = tempfile::tempdir().unwrap();
        // Initial state: Ha flat + OIII flat → two sub-items.
        write_fits_with_filter(tmp.path(), "flat_a.fits", "Flat Frame", "Ha");
        write_fits_with_filter(tmp.path(), "flat_b.fits", "Flat Frame", "OIII");

        let db = test_db().await;
        insert_source_group_with_item(&db, "sg-t067-churn", "item-t067-churn", "root-t067d", "")
            .await;

        // First classify: Ha group (flat_a) + OIII group (flat_b).
        classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: "item-t067-churn".to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: false,
            },
        )
        .await
        .unwrap();

        let first = inbox_repo::list_inbox_sub_items(db.pool(), "sg-t067-churn").await.unwrap();
        assert_eq!(first.len(), 2, "initial classify: two filter groups");

        let ha_sig_before = first
            .iter()
            .find(|s| s.group_key.contains("filter=ha"))
            .expect("Ha sub-item must exist after first classify")
            .content_signature
            .clone();

        // Change flat_a on disk: overwrite with SII filter header so it moves groups.
        write_fits_with_filter(tmp.path(), "flat_a.fits", "Flat Frame", "SII");
        // app_core_targets::metadata_cache::cached_extract keys on (path, mtime,
        // size); this rewrite keeps flat_a.fits at the same fixed 2880-byte
        // length, so mtime is the only thing that can bust the cache. Its key
        // truncates mtime to whole seconds (metadata_cache.rs's documented,
        // accepted same-second/same-size collision risk), and a fast test can
        // complete both writes within one wall-clock second on some CI
        // runners — a real re-scan is never this fast, so explicitly advance
        // the mtime to model realistic elapsed time and make the assertion
        // deterministic across platforms rather than racing the clock.
        // A read-only handle (`File::open`) is enough for `set_modified` on
        // POSIX, but Windows requires FILE_WRITE_ATTRIBUTES access, which
        // only a writable handle carries — `write(true)` opens for write
        // without truncating (the file's freshly-written SII content is left
        // intact) and gives a handle valid on all platforms.
        let flat_a_path = tmp.path().join("flat_a.fits");
        std::fs::OpenOptions::new()
            .write(true)
            .open(&flat_a_path)
            .unwrap()
            .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(2))
            .unwrap();

        // Second classify: now Ha is gone, OIII remains, SII appears.
        classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: "item-t067-churn".to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: true,
            },
        )
        .await
        .unwrap();

        let second = inbox_repo::list_inbox_sub_items(db.pool(), "sg-t067-churn").await.unwrap();

        // SII sub-item must now exist (new group from the moved file).
        let sii_after = second.iter().find(|s| s.group_key.contains("filter=sii"));
        assert!(
            sii_after.is_some(),
            "SII sub-item must appear after flat_a was rewritten with FILTER=SII; \
             got sub-items: {second:?}"
        );

        // OIII sub-item must still exist and its signature must be unchanged
        // (flat_b was not touched).
        let oiii_before = first
            .iter()
            .find(|s| s.group_key.contains("filter=oiii"))
            .expect("OIII sub-item must exist after first classify");
        let oiii_after = second
            .iter()
            .find(|s| s.group_key.contains("filter=oiii"))
            .expect("OIII sub-item must still exist after second classify");
        assert_eq!(
            oiii_before.content_signature, oiii_after.content_signature,
            "OIII sub-item signature must be stable when its files are unchanged"
        );

        // The new SII sub-item's signature must differ from the old Ha signature
        // because the file was rewritten on disk (different content → different file sig).
        let sii_sig = sii_after.unwrap().content_signature.as_deref().unwrap_or("");
        assert_ne!(
            ha_sig_before.as_deref().unwrap_or(""),
            sii_sig,
            "SII sub-item signature must differ from the old Ha signature after the file changed"
        );

        // ── BUG ASSERTION (will fail until production code is fixed) ───────────
        //
        // After flat_a moved from Ha→SII, the Ha row must be purged.
        // Currently `materialize_sub_items` never deletes stale group rows, so
        // the Ha row survives.  Expected: 2 sub-items.  Actual (buggy): 3.
        let ha_after = second.iter().find(|s| s.group_key.contains("filter=ha"));
        assert!(
            ha_after.is_none(),
            "BUG: Ha sub-item must be absent after flat_a moved to SII (R-11 correct churn). \
             materialize_sub_items must delete inbox_items rows for groups no longer in \
             the current scan. Got stale row: {ha_after:?}. \
             All sub-items after rescan: {second:?}"
        );
    }

    #[tokio::test]
    async fn t066_no_source_group_does_not_create_sub_items() {
        // Items without a source_group_id (legacy or pre-T065 scan) must not
        // have sub-items created — the materialization is skipped gracefully.
        let tmp = tempfile::tempdir().unwrap();
        write_fits_with_imagetyp(tmp.path(), "light_001.fits", "Light Frame");

        let db = test_db().await;
        // Insert WITHOUT a source_group_id (legacy path via insert_inbox_item).
        let item_id = "item-t066-legacy";
        repo::insert_inbox_item(
            db.pool(),
            &InsertInboxItem {
                id: item_id,
                root_id: "root-legacy",
                relative_path: "",
                file_count: 0,
                content_signature: None,
                lane: "fits",
            },
        )
        .await
        .unwrap();

        classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: item_id.to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: false,
            },
        )
        .await
        .unwrap();

        // No source group → no sub-items (can't list, so just verify classify
        // returned normally without error; no assertion on sub-items needed).
        // The classify response should still show single_type.
        let cached = repo::get_classification(db.pool(), item_id).await.unwrap();
        assert!(cached.is_some(), "classification must be persisted");
    }

    // ── T070 tests — mandatory-attribute gate ────────────────────────────────

    /// Write a FITS file with optional header cards (imagetyp, object, filter,
    /// exptime, gain).  Used for the T070 mandatory-attribute gate tests.
    fn write_fits_full(
        dir: &Path,
        name: &str,
        imagetyp: &str,
        object: Option<&str>,
        filter: Option<&str>,
        exptime: Option<f64>,
        gain: Option<&str>,
    ) {
        use std::io::Write as _;

        let path = dir.join(name);
        let mut block = vec![b' '; 2880];
        let mut idx = 0usize;
        let mut write_card = |card: String| {
            let bytes = card.as_bytes();
            let len = bytes.len().min(80);
            block[idx * 80..idx * 80 + len].copy_from_slice(&bytes[..len]);
            idx += 1;
        };
        write_card(format!("{:<80}", format!("IMAGETYP= '{imagetyp:<8}'")));
        if let Some(v) = object {
            write_card(format!("{:<80}", format!("OBJECT  = '{v}'")));
        }
        if let Some(v) = filter {
            write_card(format!("{:<80}", format!("FILTER  = '{v}'")));
        }
        if let Some(v) = exptime {
            write_card(format!("{:<80}", format!("EXPTIME = {v}")));
        }
        if let Some(v) = gain {
            write_card(format!("{:<80}", format!("GAIN    = {v}")));
        }
        block[idx * 80..idx * 80 + 3].copy_from_slice(b"END");
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(&block).unwrap();
    }

    /// T070/FR-047/FR-048: a light frame missing `target` (no OBJECT header)
    /// must be flagged needs_review.
    #[tokio::test]
    async fn t070_light_missing_target_goes_to_sentinel() {
        let tmp = tempfile::tempdir().unwrap();
        // Light with filter + exptime but NO OBJECT → target is missing.
        write_fits_full(
            tmp.path(),
            "light_no_target.fits",
            "Light Frame",
            None, // no OBJECT
            Some("Ha"),
            Some(300.0),
            Some("100"),
        );

        let db = test_db().await;
        insert_source_group_with_item(
            &db,
            "sg-t070-light-no-target",
            "item-t070-light-no-target",
            "root-t070a",
            "",
        )
        .await;

        classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: "item-t070-light-no-target".to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: false,
            },
        )
        .await
        .unwrap();

        let sub_items =
            inbox_repo::list_inbox_sub_items(db.pool(), "sg-t070-light-no-target").await.unwrap();
        assert_eq!(
            sub_items.len(),
            1,
            "light missing target must produce one needs-review sub-item"
        );
        assert_eq!(
            sub_items[0].needs_review, 1,
            "light missing target must be flagged needs_review"
        );
        assert!(
            sub_items[0].group_key.starts_with("type=light"),
            "needs-review does not erase classification identity: {}",
            sub_items[0].group_key
        );
        assert!(sub_items[0].frame_type.is_none(), "needs-review sub-item must have no frame_type");
    }

    /// FR-047/FR-049: `target` is mandatory for lights but is NOT a grouping
    /// dimension (`Dimension` has no Target variant), so a light with OBJECT
    /// and one without collapse into a single `group_key`. The group's verdict
    /// must therefore be the OR across its files, not whichever file the
    /// scanner enumerated first — otherwise a file missing a mandatory
    /// attribute rides a resolved sibling's row past the confirm gate.
    ///
    /// Asserted in both enumeration orders because the defect this pins was
    /// filename-order-dependent.
    ///
    /// The cardinality is asserted deliberately. On `main` the sentinel
    /// `__needs_review__` key made this folder TWO sub-items — the resolved
    /// sibling stayed independently confirmable, and only the offender was
    /// gated. Folding onto the shared natural key makes it ONE, which gates
    /// the resolved frames too and drops the folder out of
    /// `exclude_split_placeholder!`'s `COUNT(DISTINCT group_key) > 1` bound.
    /// That is a real behaviour change, accepted because the sentinel's
    /// uniqueness-discriminator role is what let a file missing a mandatory
    /// attribute become confirmable on filename order alone. Separating the two
    /// again needs a grouping dimension for the missing attribute, which is a
    /// spec decision (CHK003), not a local fix — so pin the number here and
    /// make any future change to it deliberate.
    #[tokio::test]
    async fn light_missing_target_keeps_needs_review_beside_a_resolved_sibling() {
        for (ok_name, bad_name) in
            [("a_ok.fits", "b_no_object.fits"), ("z_ok.fits", "a_no_object.fits")]
        {
            let tmp = tempfile::tempdir().unwrap();
            // Identical on every grouping dimension; differ only in OBJECT.
            write_fits_full(
                tmp.path(),
                ok_name,
                "Light Frame",
                Some("M31"),
                Some("Ha"),
                Some(300.0),
                Some("100"),
            );
            write_fits_full(
                tmp.path(),
                bad_name,
                "Light Frame",
                None,
                Some("Ha"),
                Some(300.0),
                Some("100"),
            );

            let db = test_db().await;
            insert_source_group_with_item(
                &db,
                "sg-mixed-obj",
                "item-mixed-obj",
                "root-mixed-obj",
                "",
            )
            .await;

            classify(
                db.pool(),
                ClassifyRequest {
                    inbox_item_id: "item-mixed-obj".to_owned(),
                    root_absolute_path: tmp.path().to_owned(),
                    force_rescan: false,
                },
            )
            .await
            .unwrap();

            let sub_items =
                inbox_repo::list_inbox_sub_items(db.pool(), "sg-mixed-obj").await.unwrap();
            assert!(
                sub_items.iter().any(|s| s.needs_review == 1),
                "a light missing its mandatory OBJECT must stay flagged when a resolved sibling \
                 shares its group_key (order: {ok_name} / {bad_name}); got {:?}",
                sub_items.iter().map(|s| (&s.group_key, s.needs_review)).collect::<Vec<_>>()
            );
            assert_eq!(
                sub_items.len(),
                1,
                "the two files share one classification identity, so they are ONE sub-item — \
                 changing this changes whether the folder counts as split (order: {ok_name} / \
                 {bad_name}); got {:?}",
                sub_items.iter().map(|s| (&s.group_key, s.needs_review)).collect::<Vec<_>>()
            );
        }
    }

    /// T070/FR-047/FR-048: a dark frame missing `exposureS` must route to the
    /// needs-review sub-item.
    #[tokio::test]
    async fn t070_dark_missing_exposure_goes_to_sentinel() {
        let tmp = tempfile::tempdir().unwrap();
        // Dark with gain but NO EXPTIME → exposureS is missing.
        write_fits_full(
            tmp.path(),
            "dark_no_exp.fits",
            "Dark Frame",
            None,
            None,
            None, // no EXPTIME
            Some("100"),
        );

        let db = test_db().await;
        insert_source_group_with_item(
            &db,
            "sg-t070-dark-no-exp",
            "item-t070-dark-no-exp",
            "root-t070b",
            "",
        )
        .await;

        classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: "item-t070-dark-no-exp".to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: false,
            },
        )
        .await
        .unwrap();

        let sub_items =
            inbox_repo::list_inbox_sub_items(db.pool(), "sg-t070-dark-no-exp").await.unwrap();
        assert_eq!(
            sub_items.len(),
            1,
            "dark missing exposure must produce one needs-review sub-item"
        );
        assert_eq!(
            sub_items[0].needs_review, 1,
            "dark missing exposure must be flagged needs_review"
        );
    }

    /// T070: check_mandatory_missing for various frame types.
    #[test]
    fn t070_mandatory_set_for_frame_types() {
        use metadata_core::FrameType;

        // light: frameType + target + filter + exposureS
        let set = mandatory_set_for(FrameType::Light);
        assert!(set.contains(&"target"), "light must require target");
        assert!(set.contains(&"filter"), "light must require filter");
        assert!(set.contains(&"exposureS"), "light must require exposureS");
        assert!(set.contains(&"frameType"), "light must require frameType");

        // dark: frameType + exposureS + gain (no target/filter)
        let set = mandatory_set_for(FrameType::Dark);
        assert!(set.contains(&"exposureS"), "dark must require exposureS");
        assert!(set.contains(&"gain"), "dark must require gain");
        assert!(!set.contains(&"target"), "dark must NOT require target");
        assert!(!set.contains(&"filter"), "dark must NOT require filter");

        // bias: frameType + gain (no exposure/filter/target)
        let set = mandatory_set_for(FrameType::Bias);
        assert!(set.contains(&"gain"), "bias must require gain");
        assert!(!set.contains(&"exposureS"), "bias must NOT require exposureS");

        // flat: frameType + filter
        let set = mandatory_set_for(FrameType::Flat);
        assert!(set.contains(&"filter"), "flat must require filter");
        assert!(!set.contains(&"gain"), "flat must NOT require gain by default");
    }

    /// Issue #1132: `target_missing` is the single formula both mandatory
    /// gates share for the `target` key (R-14/R-17). Pins the semantics:
    /// resolution (coordinate or user pick) OR a non-empty, trimmed OBJECT
    /// satisfies the requirement; neither does not.
    #[test]
    fn target_missing_r17_semantics() {
        assert!(target_missing(None, false), "unresolved + no OBJECT is missing");
        assert!(target_missing(Some(""), false), "unresolved + empty OBJECT is missing");
        assert!(
            target_missing(Some("   "), false),
            "unresolved + whitespace-only OBJECT is missing"
        );
        assert!(
            !target_missing(Some("M 42"), false),
            "unresolved + non-empty OBJECT is not missing"
        );
        // R-17: coordinate/user resolution alone satisfies the requirement, even
        // with no OBJECT header at all — the case #1131's equivalence test
        // cannot exercise and #1132 flags as currently unreachable in production.
        assert!(!target_missing(None, true), "resolved + no OBJECT is not missing");
        assert!(!target_missing(Some(""), true), "resolved + empty OBJECT is not missing");
    }

    /// T070: check_mandatory_missing correctly flags absent attributes.
    #[test]
    fn t070_check_mandatory_missing_light() {
        use metadata_core::{FrameType, RawFileMetadata};

        // Light with no OBJECT, no filter, no exposure → target+filter+exposureS missing.
        let raw = RawFileMetadata::default();
        let missing = check_mandatory_missing(FrameType::Light, Some(&raw), false);
        assert!(missing.contains(&"target".to_owned()), "target must be missing: {missing:?}");
        assert!(missing.contains(&"filter".to_owned()), "filter must be missing: {missing:?}");
        assert!(
            missing.contains(&"exposureS".to_owned()),
            "exposureS must be missing: {missing:?}"
        );
        assert!(!missing.contains(&"frameType".to_owned()), "frameType must not be missing");

        // Light with all mandatory attrs present.
        let raw_full = RawFileMetadata {
            object: Some("M42".to_owned()),
            filter: Some("Ha".to_owned()),
            exposure: Some("300".to_owned()),
            ..Default::default()
        };
        let missing_full = check_mandatory_missing(FrameType::Light, Some(&raw_full), false);
        assert!(
            missing_full.is_empty(),
            "fully-attributed light must have no missing attrs: {missing_full:?}"
        );
    }

    /// Write a dark-frame fixture with mandatory (EXPTIME/GAIN) plus a
    /// `SET-TEMP` card (spec 041 T081/T062 — sensor set-temperature).
    fn write_fits_dark_with_temp(
        dir: &Path,
        name: &str,
        exptime: f64,
        gain: &str,
        set_temp_c: f64,
    ) {
        let path = dir.join(name);
        let mut block = vec![b' '; 2880];
        let mut idx = 0usize;
        let mut write_card = |card: String| {
            let bytes = card.as_bytes();
            let len = bytes.len().min(80);
            block[idx * 80..idx * 80 + len].copy_from_slice(&bytes[..len]);
            idx += 1;
        };
        write_card(format!("{:<80}", "IMAGETYP= 'Dark Frame'"));
        write_card(format!("{:<80}", format!("EXPTIME = {exptime}")));
        write_card(format!("{:<80}", format!("GAIN    = {gain}")));
        write_card(format!("{:<80}", format!("SET-TEMP= {set_temp_c}")));
        block[idx * 80..idx * 80 + 3].copy_from_slice(b"END");
        let mut f = std::fs::File::create(path).unwrap();
        std::io::Write::write_all(&mut f, &block).unwrap();
    }

    /// Write a light-frame fixture with mandatory (OBJECT/FILTER/EXPTIME) plus
    /// decimal `RA`/`DEC` cards (spec 041 T081/T062 — pointing).
    fn write_fits_light_with_pointing(
        dir: &Path,
        name: &str,
        object: &str,
        filter: &str,
        exptime: f64,
        ra_deg: f64,
        dec_deg: f64,
    ) {
        let path = dir.join(name);
        let mut block = vec![b' '; 2880];
        let mut idx = 0usize;
        let mut write_card = |card: String| {
            let bytes = card.as_bytes();
            let len = bytes.len().min(80);
            block[idx * 80..idx * 80 + len].copy_from_slice(&bytes[..len]);
            idx += 1;
        };
        write_card(format!("{:<80}", "IMAGETYP= 'Light Frame'"));
        write_card(format!("{:<80}", format!("OBJECT  = '{object}'")));
        write_card(format!("{:<80}", format!("FILTER  = '{filter}'")));
        write_card(format!("{:<80}", format!("EXPTIME = {exptime}")));
        write_card(format!("{:<80}", format!("RA      = {ra_deg}")));
        write_card(format!("{:<80}", format!("DEC     = {dec_deg}")));
        block[idx * 80..idx * 80 + 3].copy_from_slice(b"END");
        let mut f = std::fs::File::create(path).unwrap();
        std::io::Write::write_all(&mut f, &block).unwrap();
    }

    /// T081 (spec 041 FR-035–FR-040): darks that differ only by `SET-TEMP`
    /// must split into distinct grouping sub-items. Before T081,
    /// `build_frame_metadata` hardcoded `set_temp_c: None`, so the `SetTemp`
    /// grouping dimension always collapsed to the same sentinel bucket and
    /// these two darks would have merged into one sub-item.
    #[tokio::test]
    async fn t081_darks_differing_by_set_temp_produce_two_sub_items() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits_dark_with_temp(tmp.path(), "dark_cold.fits", 300.0, "100", -10.0);
        write_fits_dark_with_temp(tmp.path(), "dark_warm.fits", 300.0, "100", -20.0);

        let db = test_db().await;
        insert_source_group_with_item(
            &db,
            "sg-t081-dark-temp",
            "item-t081-dark-temp",
            "root-t081a",
            "",
        )
        .await;

        classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: "item-t081-dark-temp".to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: false,
            },
        )
        .await
        .unwrap();

        let sub_items =
            inbox_repo::list_inbox_sub_items(db.pool(), "sg-t081-dark-temp").await.unwrap();
        assert_eq!(
            sub_items.len(),
            2,
            "darks at two SET-TEMP values must produce two distinct sub-items; got {sub_items:?}"
        );
        let keys: std::collections::HashSet<_> =
            sub_items.iter().map(|s| s.group_key.as_str()).collect();
        assert_eq!(keys.len(), 2, "group_key must differ between the two SET-TEMP groups");
        for si in &sub_items {
            assert_eq!(si.needs_review, 0, "darks must classify, not need review");
            assert!(
                si.group_key.contains("set_temp="),
                "group_key must embed the set_temp dimension: {}",
                si.group_key
            );
        }
    }

    /// T081 (spec 041 FR-035–FR-040): lights that differ only by pointing
    /// (RA/DEC) must split into distinct grouping sub-items. Before T081,
    /// `build_frame_metadata` hardcoded `ra_deg`/`dec_deg: None`, so the
    /// `Pointing` grouping dimension always collapsed to the same sentinel
    /// bucket and these two lights would have merged into one sub-item.
    #[tokio::test]
    async fn t081_lights_differing_by_pointing_produce_two_sub_items() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits_light_with_pointing(tmp.path(), "light_a.fits", "M42", "Ha", 300.0, 10.0, 5.0);
        write_fits_light_with_pointing(tmp.path(), "light_b.fits", "M42", "Ha", 300.0, 50.0, -5.0);

        let db = test_db().await;
        insert_source_group_with_item(
            &db,
            "sg-t081-light-ptg",
            "item-t081-light-ptg",
            "root-t081b",
            "",
        )
        .await;

        classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: "item-t081-light-ptg".to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: false,
            },
        )
        .await
        .unwrap();

        let sub_items =
            inbox_repo::list_inbox_sub_items(db.pool(), "sg-t081-light-ptg").await.unwrap();
        assert_eq!(
            sub_items.len(),
            2,
            "lights at two pointings must produce two distinct sub-items; got {sub_items:?}"
        );
        let keys: std::collections::HashSet<_> =
            sub_items.iter().map(|s| s.group_key.as_str()).collect();
        assert_eq!(keys.len(), 2, "group_key must differ between the two pointing groups");
        for si in &sub_items {
            assert_eq!(si.needs_review, 0, "lights must classify, not need review");
            assert!(
                si.group_key.contains("pointing="),
                "group_key must embed the pointing dimension: {}",
                si.group_key
            );
        }
    }

    // ── T077 / FR-054: plan_open legacy items are not re-split ─────────────────

    /// A legacy item (already carrying a migration-assigned `source_group_id`,
    /// as migration 0049 gives every pre-existing row — see
    /// `InboxItemRow::source_group_id`) that is currently `plan_open` must NOT
    /// be split into single-type sub-items when classify runs again. This
    /// protects the plan's 1:1 link to the single legacy sub-item (FR-054):
    /// splitting mid-plan would leave the open plan pointing at a folder-level
    /// item while new sibling sub-items appeared underneath it.
    #[tokio::test]
    async fn t077_plan_open_legacy_item_is_not_split_while_plan_open() {
        let tmp = tempfile::tempdir().unwrap();
        // A mixed folder (light + dark) so a real split would be observable
        // (two sub-items) if the guard were missing.
        write_fits_full(
            tmp.path(),
            "light_001.fits",
            "Light Frame",
            Some("M42"),
            Some("Ha"),
            Some(300.0),
            None,
        );
        write_fits_full(
            tmp.path(),
            "dark_001.fits",
            "Dark Frame",
            None,
            None,
            Some(300.0),
            Some("100"),
        );

        let db = test_db().await;
        insert_source_group_with_item(&db, "sg-t077-open", "item-t077-open", "root-t077-open", "")
            .await;

        // Simulate the migration 0049 legacy state: item already carries a
        // source_group_id (set above) but is currently plan_open.
        inbox_repo::update_inbox_item_state(db.pool(), "item-t077-open", "plan_open")
            .await
            .unwrap();

        classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: "item-t077-open".to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: false,
            },
        )
        .await
        .unwrap();

        let sub_items = inbox_repo::list_inbox_sub_items(db.pool(), "sg-t077-open").await.unwrap();
        assert!(
            sub_items.is_empty(),
            "plan_open legacy item must not be split into sub-items while its plan is open; \
             got {sub_items:?}"
        );

        // The parent item itself must still exist and stay in whatever state
        // classify leaves non-split items in (it is not force-kept at
        // plan_open by classify — the plan_listener owns that transition —
        // but no sub-items must exist underneath it).
        let item = inbox_repo::get_inbox_item(db.pool(), "item-t077-open").await.unwrap();
        assert_eq!(item.source_group_id.as_deref(), Some("sg-t077-open"));
    }

    /// Once a plan_open item's plan resolves or is discarded (simulated here by
    /// directly flipping the state the way `plan_listener::transition_via_plan_id`
    /// does), the next classify call must re-derive it into proper single-type
    /// sub-items — filesystem-free in the sense that no fresh disk scan or
    /// migration re-run is required beyond the item's own already-persisted
    /// `source_group_id` link; classify reads the same on-disk files it always
    /// does, it just now applies the T066 split it skipped while plan_open.
    #[tokio::test]
    async fn t077_plan_open_item_re_derives_into_sub_items_after_plan_closes() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits_full(
            tmp.path(),
            "light_001.fits",
            "Light Frame",
            Some("M42"),
            Some("Ha"),
            Some(300.0),
            None,
        );
        write_fits_full(
            tmp.path(),
            "dark_001.fits",
            "Dark Frame",
            None,
            None,
            Some(300.0),
            Some("100"),
        );

        let db = test_db().await;
        insert_source_group_with_item(
            &db,
            "sg-t077-closed",
            "item-t077-closed",
            "root-t077-closed",
            "",
        )
        .await;

        inbox_repo::update_inbox_item_state(db.pool(), "item-t077-closed", "plan_open")
            .await
            .unwrap();

        // First classify while plan_open: must not split (guard under test).
        classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: "item-t077-closed".to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: false,
            },
        )
        .await
        .unwrap();
        assert!(
            inbox_repo::list_inbox_sub_items(db.pool(), "sg-t077-closed").await.unwrap().is_empty(),
            "must not split while plan_open"
        );

        // Simulate the plan closing (discard or non-applied terminal state):
        // plan_listener::transition_via_plan_id flips the item back to
        // "classified" and deletes the inbox_plan_links row.
        inbox_repo::update_inbox_item_state(db.pool(), "item-t077-closed", "classified")
            .await
            .unwrap();

        // Next classify (force_rescan to bypass the content_signature cache,
        // which would otherwise short-circuit before reaching the T066 split
        // step since the folder contents did not change).
        classify(
            db.pool(),
            ClassifyRequest {
                inbox_item_id: "item-t077-closed".to_owned(),
                root_absolute_path: tmp.path().to_owned(),
                force_rescan: true,
            },
        )
        .await
        .unwrap();

        let sub_items =
            inbox_repo::list_inbox_sub_items(db.pool(), "sg-t077-closed").await.unwrap();
        assert_eq!(
            sub_items.len(),
            2,
            "once the plan closes, the next classify must re-derive the legacy item into \
             single-type sub-items (light + dark); got {sub_items:?}"
        );
        let frame_types: std::collections::HashSet<_> =
            sub_items.iter().filter_map(|s| s.frame_type.as_deref()).collect();
        assert_eq!(
            frame_types,
            std::collections::HashSet::from(["light", "dark"]),
            "re-derived sub-items must cover both frame types present in the folder"
        );
    }

    /// Baseline for the two link tests below: a real FITS file in the folder
    /// IS enumerated, so a later assertion of "not enumerated" is evidence of
    /// the link gate rather than of a broken walker.
    #[test]
    fn real_fits_file_is_enumerated() {
        let tmp = tempfile::tempdir().unwrap();
        write_fits_with_imagetyp(tmp.path(), "real.fits", "LIGHT");

        let found = enumerate_fits_files(tmp.path());
        assert_eq!(found.len(), 1, "a real FITS file in the folder must be enumerated");
    }

    /// Issue #1233: a symlink to a FITS file outside the inbox folder must not
    /// be pulled into classification. `is_file()` resolves the link, so the
    /// explicit link gate is what refuses it.
    #[cfg(unix)]
    #[test]
    fn symlinked_fits_file_is_not_enumerated() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        write_fits_with_imagetyp(&outside, "elsewhere.fits", "LIGHT");

        let folder = tmp.path().join("inbox_folder");
        std::fs::create_dir_all(&folder).unwrap();
        std::os::unix::fs::symlink(outside.join("elsewhere.fits"), folder.join("linked.fits"))
            .unwrap();

        let found = enumerate_fits_files(&folder);
        assert!(found.is_empty(), "a symlinked FITS file must not be enumerated: {found:?}");
    }

    /// Windows counterpart: files reached through a junction must not be
    /// enumerated either. Junctions are directory-only, so the link sits on
    /// the folder the walker would descend into.
    #[cfg(windows)]
    #[test]
    fn fits_file_behind_junction_is_not_enumerated() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        write_fits_with_imagetyp(&outside, "elsewhere.fits", "LIGHT");

        let folder = tmp.path().join("inbox_folder");
        std::fs::create_dir_all(&folder).unwrap();
        let junction = folder.join("junction_to_outside");
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J", junction.to_str().unwrap(), outside.to_str().unwrap()])
            .status()
            .expect("mklink invocation failed");
        assert!(status.success(), "mklink /J failed to create the test junction");

        // The walker is non-recursive, so the junction itself is the entry it
        // must refuse; enumerating it as a directory would be a regression.
        let found = enumerate_fits_files(&folder);
        assert!(found.is_empty(), "nothing behind a junction may be enumerated: {found:?}");
    }
}
