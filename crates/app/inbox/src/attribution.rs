// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Inbox-confirm attribution pass (spec 008 Q27, F-Framing-5/6/10,
//! FR-019/FR-020/FR-022).
//!
//! Matches an incoming light-frame Inbox item against existing framings/
//! projects by target + optic-train + pointing/rotation (tolerance) and
//! returns **ranked suggestions** — a suggestion surface only, never an
//! auto-merge (FR-020). The user's pick ([`ChosenAttributionDto`]) is applied
//! and persisted by [`apply_chosen_attribution`], called from
//! [`crate::confirm::confirm`] (FR-022).
//!
//! ## Composition point (Q22)
//!
//! This is the **first** pre-ingest pass at the confirm gate. The Q22
//! duplicate-detection sweep has no spec/code yet; when its iterate lands it
//! joins this same pass (calling [`compute_item_geometry`]'s per-file rows
//! before [`compute_candidates`] runs) rather than adding a second pass —
//! documented here as the composition point per F-Framing-5's task note.
//!
//! ## Geometry source
//!
//! Matching uses the item's **staged** (non-durable) `inbox_file_metadata`
//! rows — the only geometry available at confirm time, before any plan is
//! applied. Real `acquisition_session` rows (and their durable
//! `pointing_ra_deg`/`rotation_deg`/`optic_train_key` columns, migration
//! 0064) do not exist until the plan's light frames are folded into sessions
//! at apply completion (`app_core_targets::ingest_sessions`), which is also
//! where this pass's geometry-population counterpart populates them (F-Framing-5).
//!
//! ## Apply-path session binding
//!
//! A chosen attribution's target framing is recorded on the plan
//! (`plans.chosen_framing_id`, migration 0068) rather than applied to a
//! session immediately — no session exists yet. `ingest_sessions` reads it
//! back once the real session is created and adds the membership then
//! (F-Framing-10).

use std::collections::BTreeMap;

use audit::bus::EventBus;
use audit::event_bus::Source;
use audit::{AuditLogEntry, Outcome, Severity};
use contracts_core::error_code::ErrorCode;
use contracts_core::framing::{
    AttributionAppliedDto, ChosenAttributionDto, ChosenAttributionKind,
    IngestionAttributionCandidateDto, IngestionAttributionKind,
};
use contracts_core::lifecycle::{ProjectState, ProjectTransitionRequest, TransitionActor};
use contracts_core::{ContractError, ErrorSeverity};
use domain_core::ids::{new_id, EntityId};
use domain_core::lifecycle::data_asset::EntityType;
use domain_core::lifecycle::ProjectState as DomainProjectState;
use domain_core::project::framing::Pointing;
use persistence_inbox::repositories::inbox as inbox_repo;
use persistence_lifecycle::repositories::lifecycle::SqliteLifecycleRepository;
use persistence_plans::repositories::plans as plans_repo;
use persistence_plans::repositories::projects as projects_repo;
use persistence_targets::repositories::framing as framing_repo;
use persistence_targets::repositories::q_resolver;
use sessions::{
    angular_separation_deg, optic_train_key as compute_optic_train_key,
    rotation_axial_distance_deg, ToleranceParams,
};
use sqlx::SqlitePool;

// Import canonical mapper so `.map_err(db_err)` works identically to the
// previous local definition but now correctly routes NotFound to Blocking
// instead of Fatal (bd astro-plan-kyo7.88).
use app_core_errors::db_err;

/// Load the clustering tolerance tunables from Settings (F-Framing-11, R11a).
/// `SettingsState::default()` reproduces `ToleranceParams::defaults()`
/// bit-for-bit, so an empty settings table behaves exactly as before this
/// wiring landed.
#[allow(clippy::cast_possible_truncation)]
async fn tolerance_params(pool: &SqlitePool) -> Result<ToleranceParams, ContractError> {
    let settings =
        persistence_lifecycle::repositories::settings::load_settings(pool).await.map_err(db_err)?;
    Ok(ToleranceParams {
        pointing_fraction_of_fov: settings.framing_pointing_fraction_of_fov,
        pointing_fallback_deg: settings.framing_pointing_fallback_deg,
        rotation_tolerance_deg: settings.framing_rotation_tolerance_deg as f32,
        mosaic_envelope_fraction_of_fov: settings.framing_mosaic_envelope_fraction_of_fov,
    })
}

// ── Item geometry (staged, non-durable) ─────────────────────────────────────

/// Representative geometry for an Inbox item, computed from its staged
/// `inbox_file_metadata` rows (F-Framing-5).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemGeometry {
    pub optic_train_key: Option<String>,
    pub pointing: Option<Pointing>,
    pub rotation_deg: Option<f32>,
    pub fov_diagonal_deg: Option<f64>,
    /// Resolved via a cache-only normalized-alias lookup against the raw
    /// `OBJECT` header value (never a live resolver call — attribution is a
    /// suggestion surface; the user reviews every pick, so a cache-only best
    /// effort is honest and cheap). `None` when no file carries a resolvable
    /// `OBJECT`.
    pub resolved_target_id: Option<String>,
}

impl ItemGeometry {
    /// NULL-geometry exclusion (Q16/R11 precedent): a framing cannot be
    /// created without a representative pointing/rotation/optic-train.
    #[must_use]
    pub const fn has_framing_geometry(&self) -> bool {
        self.optic_train_key.is_some() && self.pointing.is_some() && self.rotation_deg.is_some()
    }
}

/// Compute an item's representative geometry from its staged per-file
/// metadata (F-Framing-5). Never errors on missing/partial data — an item
/// with no geometry simply yields an [`ItemGeometry::default`] (excluded from
/// matching, mirroring the NULL-geometry-sessions precedent).
///
/// # Errors
/// Returns [`ContractError`] (`internal.database`) on a query failure.
pub async fn compute_item_geometry(
    pool: &SqlitePool,
    inbox_item_id: &str,
) -> Result<ItemGeometry, ContractError> {
    let rows =
        inbox_repo::list_inbox_attribution_geometry(pool, inbox_item_id).await.map_err(db_err)?;

    let mut optic_train_key = None;
    let mut fov_diagonal_deg = None;
    let mut object_raw: Option<String> = None;
    let (mut sin_ra, mut cos_ra, mut dec_sum, mut pointing_n) = (0.0_f64, 0.0_f64, 0.0_f64, 0_u32);
    let (mut sin_rot, mut cos_rot, mut rot_n) = (0.0_f64, 0.0_f64, 0_u32);

    for row in &rows {
        if optic_train_key.is_none() {
            optic_train_key = compute_optic_train_key(
                row.telescop.as_deref(),
                row.instrume.as_deref(),
                row.focal_length_mm,
            );
        }
        if fov_diagonal_deg.is_none() {
            if let (Some(focal), Some(px), Some(x), Some(y)) =
                (row.focal_length_mm, row.pixel_size_um, row.naxis1, row.naxis2)
            {
                fov_diagonal_deg = sessions::fov_diagonal_deg(focal, px, x, y);
            }
        }
        if let (Some(ra), Some(dec)) = (row.ra_deg, row.dec_deg) {
            let ra_rad = ra.to_radians();
            sin_ra += ra_rad.sin();
            cos_ra += ra_rad.cos();
            dec_sum += dec;
            pointing_n += 1;
        }
        if let Some(rot) = row.rotator_angle_deg {
            let rot_rad = rot.to_radians();
            sin_rot += rot_rad.sin();
            cos_rot += rot_rad.cos();
            rot_n += 1;
        }
        if object_raw.is_none() {
            if let Some(o) = row.object.as_deref().map(str::trim).filter(|o| !o.is_empty()) {
                object_raw = Some(o.to_owned());
            }
        }
    }

    let pointing = (pointing_n > 0).then(|| Pointing {
        ra_deg: sin_ra.atan2(cos_ra).to_degrees().rem_euclid(360.0),
        dec_deg: dec_sum / f64::from(pointing_n),
    });
    // Angles live in [-180, 180]; an f64 -> f32 narrows precision, not range.
    #[allow(clippy::cast_possible_truncation)]
    let rotation_deg = (rot_n > 0).then(|| sin_rot.atan2(cos_rot).to_degrees() as f32);

    let resolved_target_id = match object_raw {
        Some(raw) => {
            let normalized = targeting::normalize::normalize(&raw);
            q_resolver::select_target_id_by_normalized_alias(pool, &normalized)
                .await
                .map_err(db_err)?
        }
        None => None,
    };

    Ok(ItemGeometry {
        optic_train_key,
        pointing,
        rotation_deg,
        fov_diagonal_deg,
        resolved_target_id,
    })
}

// ── Candidate ranking (F-Framing-5) ─────────────────────────────────────────

/// Compute ranked attribution candidates for an item's geometry (FR-019).
///
/// Prefilter: framings sharing the item's exact `optic_train_key` (a cheap,
/// coarse SQL cut per F-Framing-5's task note — tolerance math runs only over
/// this already-narrow candidate set, itself further partitioned per project
/// below, which is the "coarse sky bin" this prefilter provides in practice).
///
/// Always returns at least one candidate: a trailing zero-score `new_project`
/// candidate with no `projectId`, so the UI always has a pick even when
/// nothing matched.
///
/// # Errors
/// Returns [`ContractError`] (`internal.database`) on a query failure.
pub async fn compute_candidates(
    pool: &SqlitePool,
    geometry: &ItemGeometry,
) -> Result<Vec<IngestionAttributionCandidateDto>, ContractError> {
    let mut candidates = Vec::new();
    let mut matched_project_ids: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();

    if let (Some(optic_train_key), Some(item_pointing), Some(item_rotation)) =
        (&geometry.optic_train_key, geometry.pointing, geometry.rotation_deg)
    {
        same_optic_train_candidates(
            pool,
            optic_train_key,
            item_pointing,
            item_rotation,
            geometry.resolved_target_id.as_deref(),
            geometry.fov_diagonal_deg,
            &mut candidates,
            &mut matched_project_ids,
        )
        .await?;
    }

    if let Some(target_id) = &geometry.resolved_target_id {
        flag_optic_difference_candidates(pool, target_id, &matched_project_ids, &mut candidates)
            .await?;
    }

    candidates.sort_by(|a, b| {
        b.match_score.partial_cmp(&a.match_score).unwrap_or(std::cmp::Ordering::Equal)
    });

    // Trailing fallback: always at least one candidate.
    candidates.push(IngestionAttributionCandidateDto {
        kind: IngestionAttributionKind::NewProject,
        project_id: None,
        framing_id: None,
        target_id: geometry.resolved_target_id.clone(),
        match_score: 0.0,
        reopen: None,
        optic_mismatch: None,
    });

    Ok(candidates)
}

/// The `add_to_framing` / `new_framing` half of [`compute_candidates`]:
/// projects reachable via the optic-train prefilter (FR-019). Populates
/// `matched_project_ids` so the `flag_optic_difference` pass can skip
/// projects already covered here.
#[allow(clippy::too_many_arguments)]
async fn same_optic_train_candidates(
    pool: &SqlitePool,
    optic_train_key: &str,
    item_pointing: Pointing,
    item_rotation: f32,
    resolved_target_id: Option<&str>,
    fov_diagonal_deg: Option<f64>,
    candidates: &mut Vec<IngestionAttributionCandidateDto>,
    matched_project_ids: &mut std::collections::BTreeSet<String>,
) -> Result<(), ContractError> {
    let same_optic = framing_repo::list_framings_by_optic_train_key(pool, optic_train_key)
        .await
        .map_err(db_err)?;

    let mut by_project: BTreeMap<String, Vec<framing_repo::FramingRow>> = BTreeMap::new();
    for f in same_optic {
        by_project.entry(f.project_id.clone()).or_default().push(f);
    }

    let params = tolerance_params(pool).await?;
    for (project_id, framings) in &by_project {
        let Ok(project) = projects_repo::get_project(pool, project_id).await else {
            continue; // dangling reference — skip rather than fail the whole pass
        };
        let project_target_id = projects_repo::get_project_canonical_target_id(pool, project_id)
            .await
            .map_err(db_err)?;

        // Non-mosaic projects require target equality (FR-016); mosaic
        // projects replace it with the optic-train match already applied by
        // the prefilter above (FR-019 relaxation).
        if !project.is_mosaic {
            let target_matches =
                resolved_target_id.zip(project_target_id.as_deref()).is_some_and(|(a, b)| a == b);
            if !target_matches {
                continue;
            }
        }

        // No FOV fallback is defined for the mosaic envelope specifically
        // (R11a); both cases fall back to the same fixed absolute default.
        let pointing_tolerance_deg = fov_diagonal_deg.map_or(params.pointing_fallback_deg, |fov| {
            if project.is_mosaic {
                params.mosaic_envelope_fraction_of_fov * fov
            } else {
                params.pointing_fraction_of_fov * fov
            }
        });

        let best = best_matching_framing(
            framings,
            item_pointing,
            item_rotation,
            pointing_tolerance_deg,
            params.rotation_tolerance_deg,
        );

        matched_project_ids.insert(project_id.clone());
        candidates.push(match best {
            Some((f, dist)) => IngestionAttributionCandidateDto {
                kind: IngestionAttributionKind::AddToFraming,
                project_id: Some(project_id.clone()),
                framing_id: Some(f.id.clone()),
                target_id: f.target_id.clone(),
                match_score: match_score_from_distance(dist, pointing_tolerance_deg),
                reopen: Some(
                    DomainProjectState::parse_str(&project.lifecycle)
                        .is_some_and(DomainProjectState::reopens_on_new_data),
                ),
                optic_mismatch: None,
            },
            // US6 AS3: a mosaic project's first NEW panel (pointing matches
            // no existing framing) still suggests this project, via
            // add-as-new-framing.
            None => IngestionAttributionCandidateDto {
                kind: IngestionAttributionKind::NewFraming,
                project_id: Some(project_id.clone()),
                framing_id: None,
                target_id: project_target_id,
                match_score: 0.5,
                reopen: Some(
                    DomainProjectState::parse_str(&project.lifecycle)
                        .is_some_and(DomainProjectState::reopens_on_new_data),
                ),
                optic_mismatch: None,
            },
        });
    }
    Ok(())
}

/// The closest framing (by pointing distance, then rotation tolerance) among
/// `framings`, or `None` when nothing is within tolerance of either axis.
fn best_matching_framing(
    framings: &[framing_repo::FramingRow],
    item_pointing: Pointing,
    item_rotation: f32,
    pointing_tolerance_deg: f64,
    rotation_tolerance_deg: f32,
) -> Option<(&framing_repo::FramingRow, f64)> {
    let mut best: Option<(&framing_repo::FramingRow, f64)> = None;
    for f in framings {
        let f_pointing = Pointing { ra_deg: f.pointing_ra_deg, dec_deg: f.pointing_dec_deg };
        let dist = angular_separation_deg(item_pointing, f_pointing);
        if dist > pointing_tolerance_deg {
            continue;
        }
        #[allow(clippy::cast_possible_truncation)]
        let f_rotation = f.rotation_deg as f32;
        let rot_dist = rotation_axial_distance_deg(item_rotation, f_rotation);
        if rot_dist > f64::from(rotation_tolerance_deg) {
            continue;
        }
        if best.is_none_or(|(_, best_dist)| dist < best_dist) {
            best = Some((f, dist));
        }
    }
    best
}

/// The `flag_optic_difference` half of [`compute_candidates`]: projects
/// matched by target whose framings' optic-train did NOT already produce a
/// same-optic-train candidate (mosaic projects are excluded — the mosaic
/// relaxation already covers them via the optic-train prefilter).
async fn flag_optic_difference_candidates(
    pool: &SqlitePool,
    target_id: &str,
    matched_project_ids: &std::collections::BTreeSet<String>,
    candidates: &mut Vec<IngestionAttributionCandidateDto>,
) -> Result<(), ContractError> {
    let target_projects = projects_repo::list_projects_by_canonical_target_id(pool, target_id)
        .await
        .map_err(db_err)?;
    for project in target_projects {
        if project.is_mosaic || matched_project_ids.contains(&project.id) {
            continue;
        }
        candidates.push(IngestionAttributionCandidateDto {
            kind: IngestionAttributionKind::FlagOpticDifference,
            project_id: Some(project.id),
            framing_id: None,
            target_id: Some(target_id.to_owned()),
            match_score: 0.3,
            reopen: Some(
                DomainProjectState::parse_str(&project.lifecycle)
                    .is_some_and(DomainProjectState::reopens_on_new_data),
            ),
            optic_mismatch: Some(true),
        });
    }
    Ok(())
}

/// Ranking key: 1.0 at zero distance, linearly decaying to 0.0 at the
/// tolerance boundary. Clamped so a caller-side float edge case never yields
/// a negative score.
#[allow(clippy::cast_possible_truncation)]
fn match_score_from_distance(distance_deg: f64, tolerance_deg: f64) -> f32 {
    if tolerance_deg <= 0.0 {
        return 1.0;
    }
    (1.0 - (distance_deg / tolerance_deg).clamp(0.0, 1.0)) as f32
}

// ── Read-only suggest surface (F-Framing-5/10) ──────────────────────────────

/// Ranked attribution candidates for an Inbox item, read-only (FR-019).
///
/// This is the **suggest** half of FR-022: the UI calls it *before*
/// `inbox.confirm` so the user can pick, then sends the pick as the confirm
/// request's `chosenAttribution`. Reading candidates out of the confirm
/// response instead is unusable — that confirm has already created the plan
/// that [`crate::confirm::confirm`]'s open-plan guard then refuses to confirm
/// over, so the pick could never be sent (issue #943).
///
/// Returns an empty list for non-light items, mirroring
/// [`crate::confirm::confirm`]'s light-frame gate: attribution applies to
/// light frames only, so there is nothing to suggest.
///
/// # Errors
/// Returns [`ContractError`] (`internal.database`) on a query failure. Unlike
/// the confirm-time pass — which degrades to "no candidates" rather than lose
/// a user's confirm — a failure here is surfaced: nothing is at stake but the
/// suggestion itself, and a silent empty list would be indistinguishable from
/// an honest no-match.
pub async fn suggest_candidates(
    pool: &SqlitePool,
    inbox_item_id: &str,
) -> Result<Vec<IngestionAttributionCandidateDto>, ContractError> {
    let evidence_rows = inbox_repo::list_evidence(pool, inbox_item_id).await.map_err(db_err)?;
    if !crate::confirm::evidence_is_light(&evidence_rows) {
        return Ok(Vec::new());
    }
    let geometry = compute_item_geometry(pool, inbox_item_id).await?;
    compute_candidates(pool, &geometry).await
}

// ── Apply-path (F-Framing-10, F-Framing-6) ──────────────────────────────────

/// Result of applying a [`ChosenAttributionDto`] — see [`AttributionAppliedDto`].
pub type AppliedAttribution = AttributionAppliedDto;

/// Apply the user's attribution pick at confirm time (FR-022): create the
/// framing/project the kind requires, persist the pick on the plan
/// (`plans.chosen_framing_id`) for `ingest_sessions` to bind once the real
/// session exists, and honor the F-Framing-6 completed-project reopen.
///
/// # Errors
/// - `attribution.geometry_unavailable` — `new_framing` / `flag_optic_difference`
///   / `new_project` requires creating a framing, but `geometry` has no
///   pointing/rotation/optic-train (Q16 NULL-geometry precedent — no
///   fabricated snapshot).
/// - `framing.not_found` / `project.not_found` — a referenced id doesn't exist.
/// - `framing.project_mismatch` — `add_to_framing`'s framing belongs to a
///   different project than the request implies.
/// - `ContractError` on internal database/audit failure, or if the reopen
///   transition itself is refused.
pub async fn apply_chosen_attribution(
    pool: &SqlitePool,
    bus: &EventBus,
    plan_id: &str,
    geometry: &ItemGeometry,
    chosen: &ChosenAttributionDto,
) -> Result<Option<AttributionAppliedDto>, ContractError> {
    let (project_id, framing_id) = match chosen.kind {
        ChosenAttributionKind::Unassigned => return Ok(None),
        ChosenAttributionKind::AddToFraming => {
            let framing_id = chosen.framing_id.as_deref().ok_or_else(|| {
                validation_err("chosenAttribution.framingId is required for add_to_framing")
            })?;
            let framing =
                framing_repo::get_framing(pool, framing_id).await.map_err(map_db_not_found)?;
            (framing.project_id, framing_id.to_owned())
        }
        ChosenAttributionKind::NewFraming | ChosenAttributionKind::FlagOpticDifference => {
            let project_id = chosen.project_id.as_deref().ok_or_else(|| {
                validation_err(
                    "chosenAttribution.projectId is required for new_framing / flag_optic_difference",
                )
            })?;
            let project =
                projects_repo::get_project(pool, project_id).await.map_err(map_db_not_found)?;
            let framing_id = create_framing_for_project(pool, &project, geometry).await?;
            (project_id.to_owned(), framing_id)
        }
        ChosenAttributionKind::NewProject => {
            let project_id = create_project_for_attribution(pool, geometry).await?;
            let project =
                projects_repo::get_project(pool, &project_id).await.map_err(map_db_not_found)?;
            let framing_id = create_framing_for_project(pool, &project, geometry).await?;
            (project_id, framing_id)
        }
    };

    plans_repo::set_chosen_framing_id(pool, plan_id, &framing_id).await.map_err(db_err)?;

    write_attribution_audit(
        bus,
        &project_id,
        &framing_id,
        chosen,
        serde_json::json!({
            "planId": plan_id,
            "kind": chosen_kind_str(chosen.kind),
        }),
    )
    .await?;

    // F-Framing-6: completed-project match -> add + reopen (Q25 revoke/warn).
    let project = projects_repo::get_project(pool, &project_id).await.map_err(db_err)?;
    let (reopened, raw_subs_archived_warning) = if DomainProjectState::parse_str(&project.lifecycle)
        .is_some_and(DomainProjectState::reopens_on_new_data)
    {
        reopen_completed_project(pool, bus, &project_id).await?
    } else {
        (false, false)
    };

    Ok(Some(AttributionAppliedDto {
        project_id,
        framing_id: Some(framing_id),
        reopened,
        raw_subs_archived_warning,
    }))
}

fn validation_err(msg: &str) -> ContractError {
    ContractError::new(
        ErrorCode::AttributionGeometryUnavailable,
        msg,
        ErrorSeverity::Blocking,
        false,
    )
}

fn map_db_not_found(e: persistence_core::DbError) -> ContractError {
    match e {
        persistence_core::DbError::NotFound(msg) => {
            let code = if msg.contains("framing") {
                ErrorCode::FramingNotFound
            } else {
                ErrorCode::ProjectNotFound
            };
            ContractError::new(code, msg, ErrorSeverity::Blocking, false)
        }
        other => db_err(other),
    }
}

fn chosen_kind_str(kind: ChosenAttributionKind) -> &'static str {
    match kind {
        ChosenAttributionKind::AddToFraming => "add_to_framing",
        ChosenAttributionKind::NewFraming => "new_framing",
        ChosenAttributionKind::FlagOpticDifference => "flag_optic_difference",
        ChosenAttributionKind::NewProject => "new_project",
        ChosenAttributionKind::Unassigned => "unassigned",
    }
}

/// Create a new framing under `project` from `geometry`'s snapshot
/// (`new_framing` / `flag_optic_difference` / `new_project` kinds). Mosaic
/// projects inherit the project's declared target (FR-017); non-mosaic
/// projects use the resolved target (may be `None`).
async fn create_framing_for_project(
    pool: &SqlitePool,
    project: &projects_repo::ProjectRow,
    geometry: &ItemGeometry,
) -> Result<String, ContractError> {
    let (Some(optic_train_key), Some(pointing), Some(rotation_deg)) =
        (&geometry.optic_train_key, geometry.pointing, geometry.rotation_deg)
    else {
        return Err(ContractError::new(
            ErrorCode::AttributionGeometryUnavailable,
            "This item has no pointing/rotation/optic-train data staged from its FITS/XISF \
             headers, so a new framing cannot be created for it.",
            ErrorSeverity::Blocking,
            false,
        ));
    };

    let target_id = if project.is_mosaic {
        projects_repo::get_project_canonical_target_id(pool, &project.id).await.map_err(db_err)?
    } else {
        geometry.resolved_target_id.clone()
    };

    let params = tolerance_params(pool).await?;
    let (tolerance_pointing, _is_fallback) =
        geometry.fov_diagonal_deg.map_or((params.pointing_fallback_deg, true), |fov| {
            (params.pointing_fraction_of_fov * fov, false)
        });

    let framing_id = new_id();
    framing_repo::insert_framing(
        pool,
        &framing_repo::InsertFraming {
            id: &framing_id,
            project_id: &project.id,
            target_id: target_id.as_deref(),
            optic_train_key,
            pointing_ra_deg: pointing.ra_deg,
            pointing_dec_deg: pointing.dec_deg,
            rotation_deg: f64::from(rotation_deg),
            tolerance_pointing,
            tolerance_rotation_deg: f64::from(params.rotation_tolerance_deg),
            clustering: "suggested",
        },
    )
    .await
    .map_err(db_err)?;

    Ok(framing_id)
}

/// Create a minimal project for `new_project` attribution (F-Framing-10).
///
/// A deliberately minimal DB-metadata-only insert — no folder-structure
/// filesystem plan (unlike `project.create`, spec 008's full use case).
/// Framing membership is DB metadata (Q27 precedent: "no §II plan is
/// generated for it"), and this extends that precedent to the project row
/// itself when it is auto-created purely to receive an attribution pick: the
/// project starts `setup_incomplete` with a placeholder name/path, same as
/// any other freshly created project, and the user completes its name/path/
/// sources via the normal project edit flow.
async fn create_project_for_attribution(
    pool: &SqlitePool,
    geometry: &ItemGeometry,
) -> Result<String, ContractError> {
    let project_id = new_id();
    let short = project_id.chars().take(8).collect::<String>();
    projects_repo::insert_project(
        pool,
        &projects_repo::InsertProject {
            id: &project_id,
            name: &format!("Untitled Project {short}"),
            tool: "PixInsight",
            lifecycle: "setup_incomplete",
            path: &format!("projects/{project_id}"),
            notes: None,
            canonical_target_id: geometry.resolved_target_id.as_deref(),
            is_mosaic: false,
        },
    )
    .await
    .map_err(db_err)?;
    Ok(project_id)
}

/// F-Framing-6: `completed -> processing` reopen via the existing spec-009
/// edge, with the Q25 raw-subs-archived warning. Returns `(reopened, warning)`.
async fn reopen_completed_project(
    pool: &SqlitePool,
    bus: &EventBus,
    project_id: &str,
) -> Result<(bool, bool), ContractError> {
    let entity_id = uuid::Uuid::parse_str(project_id).map_err(|_| {
        ContractError::new(
            ErrorCode::ProjectNotFound,
            format!("project id {project_id} is not a valid uuid"),
            ErrorSeverity::Blocking,
            false,
        )
    })?;

    let warning = projects_repo::has_archived_raw_frames_for_project(pool, project_id)
        .await
        .map_err(db_err)?;

    let repo = SqliteLifecycleRepository::new(pool.clone(), bus.clone());
    let response = app_core_lifecycle::transition_use_case::apply_transition(
        &repo,
        bus,
        contracts_core::lifecycle::TransitionRequest::Project(ProjectTransitionRequest {
            contract_version: "2.0.0".to_owned(),
            request_id: uuid::Uuid::new_v4(),
            entity_id,
            current_state: ProjectState::Completed,
            next_state: ProjectState::Processing,
            action_label: Some("Inbox-confirm attribution: add + reopen".to_owned()),
            actor: TransitionActor::User,
        }),
    )
    .await;

    if let Some(err) = response.error {
        return Err(ContractError::new(
            ErrorCode::ProjectReadOnly,
            format!("reopen of completed project {project_id} was refused: {}", err.message),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    Ok((true, warning))
}

/// Deterministic `entity_id` for the attribution audit row: parses the
/// project id as a real UUID (every persisted project id is one), falling
/// back to a stable UUIDv5 derivation only if that ever fails — mirrors
/// `app_core::framing::framing_entity_id`.
fn attribution_entity_id(id: &str) -> EntityId {
    uuid::Uuid::parse_str(id).map_or_else(
        |_| {
            let ns =
                uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_DNS, b"astro-plan.audit.attribution");
            EntityId::from_uuid(uuid::Uuid::new_v5(&ns, id.as_bytes()))
        },
        EntityId::from_uuid,
    )
}

async fn write_attribution_audit(
    bus: &EventBus,
    project_id: &str,
    framing_id: &str,
    chosen: &ChosenAttributionDto,
    mut payload: serde_json::Value,
) -> Result<(), ContractError> {
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("projectId".to_owned(), serde_json::Value::String(project_id.to_owned()));
        obj.insert("framingId".to_owned(), serde_json::Value::String(framing_id.to_owned()));
        if let Some(source_framing) = chosen.framing_id.as_deref() {
            obj.insert(
                "chosenFramingId".to_owned(),
                serde_json::Value::String(source_framing.to_owned()),
            );
        }
    }
    let entry = AuditLogEntry::new(
        EntityType::Project,
        attribution_entity_id(project_id),
        "attribution.applied",
        "user",
        Outcome::Applied,
        Severity::Workflow,
        EntityId::new(),
    )
    .with_payload(payload.clone());
    bus.write_audit(entry, "attribution.applied", Source::User, payload).await.map(|_| ()).map_err(
        |e| {
            ContractError::new(
                ErrorCode::InternalAudit,
                e.to_string(),
                ErrorSeverity::Warning,
                false,
            )
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_score_is_one_at_zero_distance() {
        assert!((match_score_from_distance(0.0, 0.2) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn match_score_decays_to_zero_at_tolerance_boundary() {
        assert!((match_score_from_distance(0.2, 0.2) - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn match_score_clamps_beyond_tolerance() {
        assert!((match_score_from_distance(0.5, 0.2) - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn has_framing_geometry_requires_all_three_fields() {
        let complete = ItemGeometry {
            optic_train_key: Some("a|b|400".to_owned()),
            pointing: Some(Pointing { ra_deg: 10.0, dec_deg: 20.0 }),
            rotation_deg: Some(0.0),
            ..ItemGeometry::default()
        };
        assert!(complete.has_framing_geometry());

        let missing_rotation = ItemGeometry { rotation_deg: None, ..complete };
        assert!(!missing_rotation.has_framing_geometry());

        assert!(!ItemGeometry::default().has_framing_geometry());
    }

    // ── DB-backed integration tests ────────────────────────────────────────

    use persistence_core::Database;
    use persistence_inbox::repositories::inbox::UpsertFileMetadata;

    async fn test_db() -> persistence_core::Database {
        persistence_core::test_support::setup_db().await
    }

    async fn seed_inbox_item(pool: &SqlitePool, item_id: &str) {
        sqlx::query(
            "INSERT INTO inbox_items \
             (id, root_id, relative_path, group_key, discovered_at, last_scanned_at, state, lane) \
             VALUES (?, 'root-1', '2026-01-01/lights', '', \
                     '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 'pending_classification', 'fits')",
        )
        .bind(item_id)
        .execute(pool)
        .await
        .unwrap();
    }

    #[allow(clippy::too_many_arguments)]
    async fn seed_geometry_row(
        pool: &SqlitePool,
        item_id: &str,
        path: &str,
        ra_deg: f64,
        dec_deg: f64,
        rotator_angle_deg: f64,
        object: Option<&str>,
    ) {
        persistence_inbox::repositories::inbox::upsert_inbox_file_metadata(
            pool,
            &UpsertFileMetadata {
                inbox_item_id: item_id,
                relative_file_path: path,
                telescop: Some("RASA 8"),
                instrume: Some("ASI2600MM"),
                focal_length_mm: Some(400.0),
                pixel_size_um: Some(3.76),
                naxis1: Some(6248),
                naxis2: Some(4176),
                ra_deg: Some(ra_deg),
                dec_deg: Some(dec_deg),
                rotator_angle_deg: Some(rotator_angle_deg),
                object,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }

    async fn seed_canonical_target(pool: &SqlitePool, id: &str, alias: &str) {
        sqlx::query(
            "INSERT INTO canonical_target \
                (id, simbad_oid, primary_designation, object_type, ra_deg, dec_deg, source, resolved_at) \
             VALUES (?, NULL, ?, 'nebula', 83.633, 22.0145, 'resolved', '2026-01-01T00:00:00Z')",
        )
        .bind(id)
        .bind(alias)
        .execute(pool)
        .await
        .unwrap();
        let normalized = targeting::normalize::normalize(alias);
        sqlx::query(
            "INSERT INTO target_alias (id, target_id, alias, normalized, kind) \
             VALUES (?, ?, ?, ?, 'designation')",
        )
        .bind(format!("alias-{id}"))
        .bind(id)
        .bind(alias)
        .bind(normalized)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_project(pool: &SqlitePool, id: &str, lifecycle: &str, is_mosaic: bool) {
        projects_repo::insert_project(
            pool,
            &projects_repo::InsertProject {
                id,
                name: &format!("Project {id}"),
                tool: "PixInsight",
                lifecycle,
                path: &format!("projects/{id}"),
                notes: None,
                canonical_target_id: None,
                is_mosaic,
            },
        )
        .await
        .unwrap();
    }

    #[allow(clippy::too_many_arguments)]
    async fn seed_framing(
        pool: &SqlitePool,
        id: &str,
        project_id: &str,
        target_id: Option<&str>,
        ra_deg: f64,
        dec_deg: f64,
        rotation_deg: f64,
    ) {
        framing_repo::insert_framing(
            pool,
            &framing_repo::InsertFraming {
                id,
                project_id,
                target_id,
                optic_train_key: "rasa 8|asi2600mm|400",
                pointing_ra_deg: ra_deg,
                pointing_dec_deg: dec_deg,
                rotation_deg,
                tolerance_pointing: 0.1,
                tolerance_rotation_deg: 3.0,
                clustering: "suggested",
            },
        )
        .await
        .unwrap();
    }

    // ── compute_item_geometry ────────────────────────────────────────────────

    #[tokio::test]
    async fn compute_item_geometry_derives_optic_train_pointing_rotation_and_target() {
        let db = test_db().await;
        seed_inbox_item(db.pool(), "item-1").await;
        seed_canonical_target(db.pool(), "target-1", "M 42").await;
        seed_geometry_row(db.pool(), "item-1", "l1.fits", 83.63, 22.01, 1.0, Some("M 42")).await;
        seed_geometry_row(db.pool(), "item-1", "l2.fits", 83.64, 22.02, 1.5, Some("M 42")).await;

        let geo = compute_item_geometry(db.pool(), "item-1").await.unwrap();
        assert_eq!(geo.optic_train_key.as_deref(), Some("rasa 8|asi2600mm|400"));
        assert!(geo.pointing.is_some());
        assert!(geo.rotation_deg.is_some());
        assert!(geo.fov_diagonal_deg.is_some());
        assert_eq!(geo.resolved_target_id.as_deref(), Some("target-1"));
    }

    #[tokio::test]
    async fn compute_item_geometry_is_empty_for_an_item_with_no_staged_metadata() {
        let db = test_db().await;
        seed_inbox_item(db.pool(), "item-empty").await;
        let geo = compute_item_geometry(db.pool(), "item-empty").await.unwrap();
        assert!(!geo.has_framing_geometry());
        assert_eq!(geo.resolved_target_id, None);
    }

    // ── compute_candidates ────────────────────────────────────────────────────

    #[tokio::test]
    async fn compute_candidates_ranks_add_to_framing_top_when_within_tolerance() {
        let db = test_db().await;
        seed_project(db.pool(), "proj-1", "ready", false).await;
        seed_canonical_target(db.pool(), "target-1", "M 42").await;
        projects_repo::set_project_canonical_target_id(db.pool(), "proj-1", "target-1")
            .await
            .unwrap();
        seed_framing(db.pool(), "framing-1", "proj-1", Some("target-1"), 83.633, 22.0145, 1.0)
            .await;

        let geometry = ItemGeometry {
            optic_train_key: Some("rasa 8|asi2600mm|400".to_owned()),
            pointing: Some(Pointing { ra_deg: 83.634, dec_deg: 22.015 }),
            rotation_deg: Some(1.2),
            fov_diagonal_deg: Some(2.0),
            resolved_target_id: Some("target-1".to_owned()),
        };

        let candidates = compute_candidates(db.pool(), &geometry).await.unwrap();
        assert_eq!(candidates[0].kind, IngestionAttributionKind::AddToFraming);
        assert_eq!(candidates[0].framing_id.as_deref(), Some("framing-1"));
        assert!(candidates[0].match_score > 0.5);
        // Trailing new_project fallback is always present, lowest ranked.
        assert_eq!(candidates.last().unwrap().kind, IngestionAttributionKind::NewProject);
    }

    #[tokio::test]
    async fn compute_candidates_suggests_new_framing_when_pointing_is_out_of_tolerance() {
        let db = test_db().await;
        seed_project(db.pool(), "proj-2", "ready", false).await;
        seed_canonical_target(db.pool(), "target-2", "M 31").await;
        projects_repo::set_project_canonical_target_id(db.pool(), "proj-2", "target-2")
            .await
            .unwrap();
        // Existing framing far away (well beyond a 2deg-FOV*10% tolerance).
        seed_framing(db.pool(), "framing-2", "proj-2", Some("target-2"), 10.0, 20.0, 0.0).await;

        let geometry = ItemGeometry {
            optic_train_key: Some("rasa 8|asi2600mm|400".to_owned()),
            pointing: Some(Pointing { ra_deg: 83.633, dec_deg: 22.0145 }),
            rotation_deg: Some(0.0),
            fov_diagonal_deg: Some(2.0),
            resolved_target_id: Some("target-2".to_owned()),
        };

        let candidates = compute_candidates(db.pool(), &geometry).await.unwrap();
        let top = candidates.iter().find(|c| c.project_id.as_deref() == Some("proj-2")).unwrap();
        assert_eq!(top.kind, IngestionAttributionKind::NewFraming);
    }

    // ── tolerance_params (F-Framing-11: settings wiring, R11a) ─────────────────

    #[tokio::test]
    async fn tolerance_params_reads_r11a_defaults_when_unset() {
        let db = test_db().await;
        let params = tolerance_params(db.pool()).await.unwrap();
        assert_eq!(params, ToleranceParams::defaults());
    }

    #[tokio::test]
    async fn tolerance_params_honours_stored_settings_overrides() {
        let db = test_db().await;
        persistence_lifecycle::repositories::settings::set_raw(
            db.pool(),
            "framingPointingFractionOfFov",
            &serde_json::json!(0.25),
        )
        .await
        .unwrap();
        persistence_lifecycle::repositories::settings::set_raw(
            db.pool(),
            "framingRotationToleranceDeg",
            &serde_json::json!(7.5),
        )
        .await
        .unwrap();

        let params = tolerance_params(db.pool()).await.unwrap();
        assert!((params.pointing_fraction_of_fov - 0.25).abs() < f64::EPSILON);
        assert!((params.rotation_tolerance_deg - 7.5).abs() < f32::EPSILON);
        // Untouched keys keep the R11a defaults.
        let defaults = ToleranceParams::defaults();
        assert!(
            (params.pointing_fallback_deg - defaults.pointing_fallback_deg).abs() < f64::EPSILON
        );
        assert!(
            (params.mosaic_envelope_fraction_of_fov - defaults.mosaic_envelope_fraction_of_fov)
                .abs()
                < f64::EPSILON
        );
    }

    /// End-to-end proof that a stored `framingPointingFractionOfFov` override
    /// actually changes `compute_candidates`' ranking outcome, not just the
    /// parameter struct (F-Framing-11).
    #[tokio::test]
    async fn compute_candidates_honours_widened_pointing_tolerance_setting() {
        let db = test_db().await;
        seed_project(db.pool(), "proj-tol", "ready", false).await;
        seed_canonical_target(db.pool(), "target-tol", "M 99").await;
        projects_repo::set_project_canonical_target_id(db.pool(), "proj-tol", "target-tol")
            .await
            .unwrap();
        seed_framing(
            db.pool(),
            "framing-tol",
            "proj-tol",
            Some("target-tol"),
            83.633,
            22.0145,
            0.0,
        )
        .await;

        let geometry = ItemGeometry {
            optic_train_key: Some("rasa 8|asi2600mm|400".to_owned()),
            // ~0.5deg north of the framing's representative — outside the
            // R11a default 10%-of-FOV (0.2deg @ 2.0deg FOV) tolerance.
            pointing: Some(Pointing { ra_deg: 83.633, dec_deg: 22.5145 }),
            rotation_deg: Some(0.0),
            fov_diagonal_deg: Some(2.0),
            resolved_target_id: Some("target-tol".to_owned()),
        };

        // Default tolerance: too far, suggests a new framing instead.
        let candidates = compute_candidates(db.pool(), &geometry).await.unwrap();
        let top = candidates.iter().find(|c| c.project_id.as_deref() == Some("proj-tol")).unwrap();
        assert_eq!(top.kind, IngestionAttributionKind::NewFraming);

        // Widen the pointing tolerance via Settings — 0.5 * 2.0deg FOV =
        // 1.0deg envelope, now comfortably covers the 0.5deg offset.
        persistence_lifecycle::repositories::settings::set_raw(
            db.pool(),
            "framingPointingFractionOfFov",
            &serde_json::json!(0.5),
        )
        .await
        .unwrap();

        let candidates = compute_candidates(db.pool(), &geometry).await.unwrap();
        let top = candidates.iter().find(|c| c.project_id.as_deref() == Some("proj-tol")).unwrap();
        assert_eq!(top.kind, IngestionAttributionKind::AddToFraming);
        assert_eq!(top.framing_id.as_deref(), Some("framing-tol"));
    }

    #[tokio::test]
    async fn compute_candidates_flags_optic_difference_for_same_target_different_optic_train() {
        let db = test_db().await;
        seed_project(db.pool(), "proj-3", "ready", false).await;
        seed_canonical_target(db.pool(), "target-3", "M 33").await;
        projects_repo::set_project_canonical_target_id(db.pool(), "proj-3", "target-3")
            .await
            .unwrap();
        // A framing with a DIFFERENT optic-train key, same target/project.
        framing_repo::insert_framing(
            db.pool(),
            &framing_repo::InsertFraming {
                id: "framing-3",
                project_id: "proj-3",
                target_id: Some("target-3"),
                optic_train_key: "other-scope|other-cam|800",
                pointing_ra_deg: 83.633,
                pointing_dec_deg: 22.0145,
                rotation_deg: 0.0,
                tolerance_pointing: 0.1,
                tolerance_rotation_deg: 3.0,
                clustering: "suggested",
            },
        )
        .await
        .unwrap();

        let geometry = ItemGeometry {
            optic_train_key: Some("rasa 8|asi2600mm|400".to_owned()),
            pointing: Some(Pointing { ra_deg: 83.633, dec_deg: 22.0145 }),
            rotation_deg: Some(0.0),
            fov_diagonal_deg: Some(2.0),
            resolved_target_id: Some("target-3".to_owned()),
        };

        let candidates = compute_candidates(db.pool(), &geometry).await.unwrap();
        let flagged =
            candidates.iter().find(|c| c.project_id.as_deref() == Some("proj-3")).unwrap();
        assert_eq!(flagged.kind, IngestionAttributionKind::FlagOpticDifference);
        assert_eq!(flagged.optic_mismatch, Some(true));
    }

    /// US6 AS3: a mosaic project with an existing framing (a prior panel)
    /// still suggests add-as-new-framing for a pointing that matches no
    /// existing framing, via the FR-019 mosaic relaxation (optic-train match
    /// + envelope, not target equality — the mosaic's per-frame target
    /// resolution is suppressed so the new panel's `resolved_target_id` is
    /// `None`).
    #[tokio::test]
    async fn compute_candidates_mosaic_first_new_panel_suggests_new_framing() {
        let db = test_db().await;
        seed_project(db.pool(), "proj-mosaic", "ready", true).await;
        seed_canonical_target(db.pool(), "target-mosaic", "NGC 7000").await;
        projects_repo::set_project_canonical_target_id(db.pool(), "proj-mosaic", "target-mosaic")
            .await
            .unwrap();
        // Panel 1 at one pointing.
        seed_framing(db.pool(), "panel-1", "proj-mosaic", Some("target-mosaic"), 300.0, 40.0, 0.0)
            .await;

        // Panel 2's pointing is well outside a 2deg-FOV*1.0x envelope of
        // panel 1, and per-frame target resolution is suppressed (None).
        let geometry = ItemGeometry {
            optic_train_key: Some("rasa 8|asi2600mm|400".to_owned()),
            pointing: Some(Pointing { ra_deg: 305.0, dec_deg: 42.0 }),
            rotation_deg: Some(0.0),
            fov_diagonal_deg: Some(2.0),
            resolved_target_id: None,
        };

        let candidates = compute_candidates(db.pool(), &geometry).await.unwrap();
        let top =
            candidates.iter().find(|c| c.project_id.as_deref() == Some("proj-mosaic")).unwrap();
        assert_eq!(top.kind, IngestionAttributionKind::NewFraming);
    }

    #[tokio::test]
    async fn compute_candidates_returns_only_new_project_when_nothing_matches() {
        let db = test_db().await;
        let geometry = ItemGeometry {
            optic_train_key: Some("lonely|scope|400".to_owned()),
            pointing: Some(Pointing { ra_deg: 1.0, dec_deg: 1.0 }),
            rotation_deg: Some(0.0),
            fov_diagonal_deg: None,
            resolved_target_id: None,
        };
        let candidates = compute_candidates(db.pool(), &geometry).await.unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].kind, IngestionAttributionKind::NewProject);
        assert_eq!(candidates[0].project_id, None);
    }

    // ── suggest_candidates (read-only suggest surface, issue #943) ───────────

    async fn seed_evidence(pool: &SqlitePool, item_id: &str, frame_type: &str) {
        persistence_inbox::repositories::inbox::insert_evidence(
            pool,
            &persistence_inbox::repositories::inbox::InsertEvidence {
                id: &format!("ev-{item_id}"),
                inbox_item_id: item_id,
                relative_file_path: "sub_0001.fits",
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

    /// SC-008 read half: the candidates a user must pick from are readable
    /// *without* confirming, so the pick can ride a single confirm.
    #[tokio::test]
    async fn suggest_candidates_ranks_matching_framing_first() {
        let db = test_db().await;
        seed_project(db.pool(), "proj-s1", "ready", false).await;
        seed_canonical_target(db.pool(), "target-s1", "M 42").await;
        projects_repo::set_project_canonical_target_id(db.pool(), "proj-s1", "target-s1")
            .await
            .unwrap();
        seed_framing(db.pool(), "framing-s1", "proj-s1", Some("target-s1"), 83.633, 22.0145, 1.0)
            .await;

        seed_inbox_item(db.pool(), "item-s1").await;
        seed_evidence(db.pool(), "item-s1", "light").await;
        seed_geometry_row(db.pool(), "item-s1", "sub_0001.fits", 83.634, 22.015, 1.2, Some("M 42"))
            .await;

        let candidates = suggest_candidates(db.pool(), "item-s1").await.unwrap();

        assert_eq!(candidates[0].kind, IngestionAttributionKind::AddToFraming);
        assert_eq!(candidates[0].framing_id.as_deref(), Some("framing-s1"));
        assert!(candidates[0].match_score > 0.5);
        // Suggest-never-auto-merge (FR-020): the fallback is always offered, so
        // the user can always decline the top match.
        assert_eq!(candidates.last().unwrap().kind, IngestionAttributionKind::NewProject);
    }

    #[tokio::test]
    async fn suggest_candidates_returns_empty_for_non_light_item() {
        let db = test_db().await;
        seed_inbox_item(db.pool(), "item-s2").await;
        seed_evidence(db.pool(), "item-s2", "flat").await;
        seed_geometry_row(db.pool(), "item-s2", "flat_0001.fits", 83.634, 22.015, 1.2, None).await;

        assert!(suggest_candidates(db.pool(), "item-s2").await.unwrap().is_empty());
    }

    // ── apply_chosen_attribution ─────────────────────────────────────────────

    fn test_bus(db: &Database) -> EventBus {
        EventBus::with_pool(db.pool().clone())
    }

    async fn seed_plan(pool: &SqlitePool, plan_id: &str) {
        plans_repo::insert_plan(
            pool,
            &plans_repo::InsertPlan {
                id: plan_id,
                title: "test plan",
                origin: "inbox",
                origin_path: None,
                plan_type: "split",
                destructive_destination: "archive",
                parent_plan_id: None,
                total_bytes_required: 0,
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn apply_new_project_creates_project_framing_and_persists_plan_pick() {
        let db = test_db().await;
        let bus = test_bus(&db);
        seed_plan(db.pool(), "plan-1").await;

        let geometry = ItemGeometry {
            optic_train_key: Some("rasa 8|asi2600mm|400".to_owned()),
            pointing: Some(Pointing { ra_deg: 10.0, dec_deg: 20.0 }),
            rotation_deg: Some(0.0),
            fov_diagonal_deg: Some(2.0),
            resolved_target_id: None,
        };
        let chosen = ChosenAttributionDto {
            kind: ChosenAttributionKind::NewProject,
            project_id: None,
            framing_id: None,
        };

        let applied =
            apply_chosen_attribution(db.pool(), &bus, "plan-1", &geometry, &chosen).await.unwrap();
        let applied = applied.expect("new_project must apply");
        assert!(!applied.reopened);
        assert!(!applied.raw_subs_archived_warning);

        let project = projects_repo::get_project(db.pool(), &applied.project_id).await.unwrap();
        assert_eq!(project.lifecycle, "setup_incomplete");

        let framing_id = applied.framing_id.expect("new_project creates a framing");
        let framing = framing_repo::get_framing(db.pool(), &framing_id).await.unwrap();
        assert_eq!(framing.project_id, applied.project_id);

        assert_eq!(
            plans_repo::get_chosen_framing_id(db.pool(), "plan-1").await.unwrap().as_deref(),
            Some(framing_id.as_str())
        );
    }

    #[tokio::test]
    async fn apply_add_to_framing_links_existing_framing_without_creating_a_new_one() {
        let db = test_db().await;
        let bus = test_bus(&db);
        seed_plan(db.pool(), "plan-2").await;
        seed_project(db.pool(), "proj-existing", "ready", false).await;
        seed_framing(db.pool(), "framing-existing", "proj-existing", None, 10.0, 20.0, 0.0).await;

        let geometry = ItemGeometry::default();
        let chosen = ChosenAttributionDto {
            kind: ChosenAttributionKind::AddToFraming,
            project_id: None,
            framing_id: Some("framing-existing".to_owned()),
        };

        let applied =
            apply_chosen_attribution(db.pool(), &bus, "plan-2", &geometry, &chosen).await.unwrap();
        let applied = applied.unwrap();
        assert_eq!(applied.project_id, "proj-existing");
        assert_eq!(applied.framing_id.as_deref(), Some("framing-existing"));
        assert_eq!(
            plans_repo::get_chosen_framing_id(db.pool(), "plan-2").await.unwrap().as_deref(),
            Some("framing-existing")
        );
    }

    /// F-Framing-4/F-Framing-8: a mosaic project's new framing inherits the
    /// project's *declared* canonical target (FR-017) — it never adopts the
    /// item's OBJECT-resolved target, even when one was staged (no per-frame
    /// OBJECT/coordinate resolution for mosaic projects).
    #[tokio::test]
    async fn apply_new_framing_for_mosaic_project_inherits_declared_target_ignoring_resolved_object(
    ) {
        let db = test_db().await;
        let bus = test_bus(&db);
        seed_plan(db.pool(), "plan-mosaic").await;
        seed_canonical_target(db.pool(), "target-declared", "NGC 7000").await;
        seed_canonical_target(db.pool(), "target-object-decoy", "M 45").await;
        seed_project(db.pool(), "proj-mosaic", "ready", true).await;
        projects_repo::set_project_canonical_target_id(db.pool(), "proj-mosaic", "target-declared")
            .await
            .unwrap();

        let geometry = ItemGeometry {
            optic_train_key: Some("rasa 8|asi2600mm|400".to_owned()),
            pointing: Some(Pointing { ra_deg: 10.0, dec_deg: 20.0 }),
            rotation_deg: Some(0.0),
            fov_diagonal_deg: Some(2.0),
            // A misleading per-frame OBJECT resolution — must be ignored.
            resolved_target_id: Some("target-object-decoy".to_owned()),
        };
        let chosen = ChosenAttributionDto {
            kind: ChosenAttributionKind::NewFraming,
            project_id: Some("proj-mosaic".to_owned()),
            framing_id: None,
        };

        let applied = apply_chosen_attribution(db.pool(), &bus, "plan-mosaic", &geometry, &chosen)
            .await
            .unwrap()
            .expect("new_framing must apply");

        let framing_id = applied.framing_id.expect("new_framing creates a framing");
        let framing = framing_repo::get_framing(db.pool(), &framing_id).await.unwrap();
        assert_eq!(framing.target_id.as_deref(), Some("target-declared"));
    }

    #[tokio::test]
    async fn apply_unassigned_is_a_noop() {
        let db = test_db().await;
        let bus = test_bus(&db);
        seed_plan(db.pool(), "plan-3").await;

        let geometry = ItemGeometry::default();
        let chosen = ChosenAttributionDto {
            kind: ChosenAttributionKind::Unassigned,
            project_id: None,
            framing_id: None,
        };

        let applied =
            apply_chosen_attribution(db.pool(), &bus, "plan-3", &geometry, &chosen).await.unwrap();
        assert!(applied.is_none());
        assert_eq!(plans_repo::get_chosen_framing_id(db.pool(), "plan-3").await.unwrap(), None);
    }

    #[tokio::test]
    async fn apply_new_framing_without_geometry_is_rejected() {
        let db = test_db().await;
        let bus = test_bus(&db);
        seed_plan(db.pool(), "plan-4").await;
        seed_project(db.pool(), "proj-nogeo", "ready", false).await;

        let geometry = ItemGeometry::default(); // no pointing/rotation/optic-train
        let chosen = ChosenAttributionDto {
            kind: ChosenAttributionKind::NewFraming,
            project_id: Some("proj-nogeo".to_owned()),
            framing_id: None,
        };

        let err = apply_chosen_attribution(db.pool(), &bus, "plan-4", &geometry, &chosen)
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::AttributionGeometryUnavailable);
    }

    /// F-Framing-6: matching a `completed` project reopens it via the
    /// existing `completed -> processing` edge, honoring the Q25
    /// raw-subs-archived warning when the project's raw subs were already
    /// cleaned up.
    #[tokio::test]
    async fn apply_to_completed_project_reopens_and_surfaces_raw_subs_warning() {
        let db = test_db().await;
        let bus = test_bus(&db);
        seed_plan(db.pool(), "plan-5").await;
        // `reopen_completed_project` parses the project id as a `Uuid` (the
        // `TransitionRequest::Project.entity_id` contract field, matching
        // every real project id produced by `domain_core::ids::new_id`).
        let project_id = new_id();
        seed_project(db.pool(), &project_id, "completed", false).await;
        seed_framing(db.pool(), "framing-completed", &project_id, None, 10.0, 20.0, 0.0).await;

        // Raw-subs-archived history: an applied cleanup plan that archived a
        // raw frame belonging to a session linked to this project.
        projects_repo::insert_project_source(
            db.pool(),
            &projects_repo::InsertProjectSource {
                id: "src-1",
                project_id: &project_id,
                inventory_session_id: "sess-1",
                name_snapshot: "Ha",
                frames_snapshot: 10,
                filter_snapshot: "Ha",
                exposure_snapshot: "60s",
                linked_at: "2026-01-01T00:00:00Z",
            },
        )
        .await
        .unwrap();
        plans_repo::insert_plan(
            db.pool(),
            &plans_repo::InsertPlan {
                id: "cleanup-plan",
                title: "Raw sub-frame cleanup",
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
        plans_repo::insert_plan_item(
            db.pool(),
            &plans_repo::InsertPlanItem {
                id: "cleanup-item-0",
                plan_id: "cleanup-plan",
                item_index: 0,
                name: "light_001.fits",
                action: "archive",
                from_root_id: Some("root-1"),
                from_relative_path: "lights/light_001.fits",
                to_root_id: None,
                to_relative_path: "",
                reason: "raw_frame_cleanup",
                protection: "normal",
                linked_entity: None,
                provenance_json: None,
                archive_path: None,
                source_id: Some("sess-1"),
                category: Some("raw_frames"),
            },
        )
        .await
        .unwrap();
        sqlx::query("UPDATE plans SET state = 'applied' WHERE id = 'cleanup-plan'")
            .execute(db.pool())
            .await
            .unwrap();
        sqlx::query("UPDATE plan_items SET item_state = 'succeeded' WHERE id = 'cleanup-item-0'")
            .execute(db.pool())
            .await
            .unwrap();

        let geometry = ItemGeometry::default();
        let chosen = ChosenAttributionDto {
            kind: ChosenAttributionKind::AddToFraming,
            project_id: None,
            framing_id: Some("framing-completed".to_owned()),
        };

        let applied = apply_chosen_attribution(db.pool(), &bus, "plan-5", &geometry, &chosen)
            .await
            .unwrap()
            .unwrap();
        assert!(applied.reopened);
        assert!(applied.raw_subs_archived_warning);

        let project = projects_repo::get_project(db.pool(), &project_id).await.unwrap();
        assert_eq!(project.lifecycle, "processing");
    }
}
