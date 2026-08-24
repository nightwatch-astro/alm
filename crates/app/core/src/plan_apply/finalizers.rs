use super::{materialization_from_provenance, new_id, plans_repo, EventBus, SqlitePool, Timestamp};

// ── Source view generation finalization (spec 049) ───────────────────────────

/// Terminal step of a `project_create` plan apply: fire the `Created`
/// manifest trigger (#665 — this and the source add/remove triggers had no
/// emitters at all; only the unrelated `workflow.run_completed` trigger was
/// wired). The project's folder structure (including `notes/`) only exists
/// on disk once this plan applies, so this is the earliest point the write
/// can succeed.
///
/// `origin_path` on a `project_create` plan is the project's filesystem
/// path, not its id (unlike every other plan origin) — recovered via
/// `projects::path_exists`.
///
/// Best-effort: the project already exists, so a manifest failure here must
/// NOT fail the apply. Every failure is logged for an external watchdog (§II).
pub(super) async fn finalize_project_create_manifest(
    pool: &SqlitePool,
    bus: &EventBus,
    project_path: &str,
) {
    use contracts_core::manifests::ManifestReason as DtoManifestReason;
    use persistence_plans::repositories::projects as projects_repo;

    let project_id = match projects_repo::path_exists(pool, project_path, None).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            tracing::warn!(%project_path, "project_create manifest: no project at this path");
            return;
        }
        Err(e) => {
            tracing::warn!(%project_path, error=%e, "project_create manifest: path lookup failed");
            return;
        }
    };
    app_core_projects::project_manifests::write_lifecycle_manifest(
        pool,
        bus,
        &project_id,
        DtoManifestReason::Created,
    )
    .await;
}

/// Terminal step of a `prepared_view_generation` plan apply: write the
/// first-materialization `PreparedSourceView` (state `current`) plus one
/// `PreparedSourceViewItem` per successfully-applied `link` item.
///
/// Best-effort: the links are already on disk, so a failure here must NOT
/// fail the apply. Every failure is logged for an external watchdog (§II).
/// Idempotent: a re-entrant call (e.g. a retried terminal transition) skips
/// item rows that already exist for this view id.
#[expect(
    clippy::cognitive_complexity,
    reason = "view-generation finalize: per-item materialization dispatch with skip paths"
)]
pub(super) async fn finalize_view_generation(pool: &SqlitePool, plan_id: &str, project_id: &str) {
    use domain_core::source_view::Materialization;
    use persistence_plans::repositories::prepared_source_views as views_repo;

    let items = match plans_repo::list_plan_items(pool, plan_id).await {
        Ok(items) => items,
        Err(e) => {
            tracing::error!(%plan_id, error=%e, "generation finalize: failed to load plan items");
            return;
        }
    };

    let succeeded: Vec<_> =
        items.iter().filter(|i| i.action == "link" && i.item_state == "succeeded").collect();

    if succeeded.is_empty() {
        tracing::warn!(%plan_id, "generation finalize: no succeeded link items; no view recorded");
        return;
    }

    // The view's display `kind` is the dominant per-item materialization
    // (spec 026 FR-008 amended, CL-2) — the first succeeded item's kind is a
    // reasonable representative; per-item kind remains authoritative.
    let dominant_kind = succeeded
        .first()
        .map_or(Materialization::Symlink, |row| materialization_from_provenance(row));

    let view_id = new_id();
    if let Err(e) = views_repo::insert_view(
        pool,
        &views_repo::InsertPreparedSourceView {
            id: &view_id,
            project_id,
            kind: dominant_kind.as_str(),
        },
    )
    .await
    {
        tracing::error!(%plan_id, %view_id, error=%e, "generation finalize: failed to insert view");
        return;
    }

    for item in succeeded {
        let Some(inventory_item_id) = item.linked_entity.as_deref() else {
            tracing::warn!(
                %plan_id, item_id = %item.id,
                "generation finalize: link item missing linked_entity (inventory reference); skipped"
            );
            continue;
        };
        let materialization = materialization_from_provenance(item);
        let view_item_id = new_id();
        if let Err(e) = views_repo::insert_view_item(
            pool,
            &views_repo::InsertPreparedSourceViewItem {
                id: &view_item_id,
                view_id: &view_id,
                inventory_item_id,
                view_relative_path: &item.to_relative_path,
                materialization: materialization.as_str(),
            },
        )
        .await
        {
            tracing::error!(
                %plan_id, %view_id, item_id = %item.id, error=%e,
                "generation finalize: failed to insert view item"
            );
        }
    }
}

// ── Source view removal/regeneration finalization (spec 026 T017/T018) ───────

/// Look up the `PreparedSourceView` id a `prepared_view_removal`/
/// `prepared_view_regeneration` plan targets, from any item's
/// `linked_entity` (every item in these plans is linked to the same view —
/// `prepared_views::remove_prepared_view`/`regenerate_prepared_view`).
pub(super) async fn view_id_for_plan(pool: &SqlitePool, plan_id: &str) -> Option<String> {
    match plans_repo::list_plan_items(pool, plan_id).await {
        Ok(items) => items.into_iter().find_map(|i| i.linked_entity),
        Err(e) => {
            tracing::error!(%plan_id, error=%e, "view finalize: failed to load plan items");
            None
        }
    }
}

/// Terminal step of a `prepared_view_removal` plan apply (T017/T018).
///
/// A clean `applied` terminal means every item was archived away, so the
/// view's on-disk representation is fully gone — recorded explicitly via
/// `mark_view_removed` (A4: membership preserved indefinitely for later
/// regeneration; this is not derivable from a staleness sweep, which cannot
/// distinguish "removed by this plan" from "some items independently went
/// missing").
///
/// A partial apply leaves a genuinely mixed on-disk state; rather than guess,
/// this rides the stale-detection sweep (T014) to recompute real per-item
/// state from disk, same as any other spec-026 US3 sweep.
///
/// Best-effort: failures are logged only, never fail the apply (§II).
#[expect(
    clippy::cognitive_complexity,
    reason = "view-removal finalize with per-precondition early return"
)]
pub(super) async fn finalize_view_removal(pool: &SqlitePool, plan_id: &str, terminal: &str) {
    use persistence_plans::repositories::prepared_source_views as views_repo;

    let Some(view_id) = view_id_for_plan(pool, plan_id).await else {
        tracing::warn!(%plan_id, "removal finalize: no linked view id on plan items; skipped");
        return;
    };

    if terminal == "applied" {
        if let Err(e) = views_repo::mark_view_removed(pool, &view_id).await {
            tracing::error!(%plan_id, %view_id, error=%e, "removal finalize: failed to mark view removed");
        }
    } else if let Err(e) =
        app_core_projects::source_view_verify::sweep_view_staleness(pool, &view_id).await
    {
        tracing::error!(%plan_id, %view_id, error=?e, "removal finalize: sweep failed after partial apply");
    }
}

/// Terminal step of a `prepared_view_regeneration` plan apply (T017/T018).
///
/// Unlike removal, a successful regeneration doesn't have a single new
/// terminal DB state to write — the freshly-created links are just real
/// files again. Rides the same stale-detection sweep (T014) used for
/// on-demand staleness checks, so the recorded `state`/`last_observed_state`
/// reflect the actual outcome (including any items a partial apply left
/// broken) rather than a hand-maintained approximation.
///
/// A successful regeneration is the one legitimate way out of the terminal
/// `removed` state (A4) — but `sweep_view_staleness` intentionally skips
/// `removed`/`kind_diverged` views (they have nothing meaningful to sweep in
/// the general list-load path). So a `removed` view is first cleared to a
/// neutral non-terminal state here, purely so the sweep actually runs and
/// re-evaluates the freshly-recreated links, rather than leaving the view
/// stuck `removed` forever after a successful regeneration.
///
/// Best-effort: failures are logged only, never fail the apply (§II).
#[expect(
    clippy::cognitive_complexity,
    reason = "view-regeneration finalize with per-precondition early return"
)]
pub(super) async fn finalize_view_regeneration(pool: &SqlitePool, plan_id: &str) {
    use persistence_plans::repositories::prepared_source_views as views_repo;

    let Some(view_id) = view_id_for_plan(pool, plan_id).await else {
        tracing::warn!(%plan_id, "regeneration finalize: no linked view id on plan items; skipped");
        return;
    };

    if let Ok(view) = views_repo::get_view(pool, &view_id).await {
        if view.state == "removed" {
            if let Err(e) = views_repo::update_view_state(pool, &view_id, "stale").await {
                tracing::error!(%plan_id, %view_id, error=%e, "regeneration finalize: failed to clear removed state pre-sweep");
            }
        }
    }

    if let Err(e) =
        app_core_projects::source_view_verify::sweep_view_staleness(pool, &view_id).await
    {
        tracing::error!(%plan_id, %view_id, error=?e, "regeneration finalize: sweep failed");
    }
}

// ── Archive lifecycle closure (spec 017 C5) ──────────────────────────────────

/// Terminal step of a successful `origin = archive` plan apply: drive the owning
/// project into the `archived` lifecycle state (C5). This is the ONE legitimate
/// closure of the requires-plan gate — the plan was reviewed, approved, and just
/// applied, so the filesystem move that `completed → archived` requires has
/// happened. We call the low-level [`transition_lifecycle`] directly (which does
/// not re-run the requires-plan gate that `apply_transition` enforces) and then
/// record `archived_via_plan_id` so the archive-management commands can act on
/// this plan.
///
/// Best-effort: the files are already archived, so a failure here must NOT fail
/// the apply. Every failure is logged for an external watchdog (§II).
#[expect(
    clippy::cognitive_complexity,
    reason = "archive finalize: lifecycle transition per project entity with per-step logging"
)]
pub(super) async fn finalize_archive_lifecycle(
    pool: &SqlitePool,
    bus: &EventBus,
    plan_id: &str,
    project_id: &str,
) {
    use crate::lifecycle::lifecycle_use_case::{transition_lifecycle, TransitionCommand};
    use domain_core::ids::EntityId;
    use domain_core::lifecycle::data_asset::EntityType;
    use persistence_lifecycle::repositories::lifecycle::SqliteLifecycleRepository;
    use persistence_plans::repositories::projects as projects_repo;

    // The lifecycle repo keys entities on their UUID id.
    let uuid = match uuid::Uuid::parse_str(project_id) {
        Ok(u) => u,
        Err(e) => {
            tracing::error!(%project_id, error=%e, "archive lifecycle closure: project id is not a uuid");
            return;
        }
    };

    // Read the current lifecycle so the transition CAS matches whatever the
    // project is in (typically `completed` or `blocked`).
    let current = match projects_repo::get_project(pool, project_id).await {
        Ok(p) => p.lifecycle,
        Err(e) => {
            tracing::error!(%project_id, error=%e, "archive lifecycle closure: project not found");
            return;
        }
    };

    // Idempotent: an already-archived project just needs the plan link recorded.
    if current == "archived" {
        if let Err(e) = projects_repo::set_archived_via_plan_id(pool, project_id, plan_id).await {
            tracing::error!(%project_id, error=%e, "archive lifecycle closure: failed to record archived_via_plan_id");
        }
        return;
    }

    // Edge-legality guard (Constitution §II). `transition_lifecycle` is un-gated
    // and `record_transition` only CAS-checks `from_state`, so this closure would
    // otherwise CAS `<any state> → archived`. Per the domain edge table
    // (`domain_core::lifecycle::project::is_allowed`) the ONLY legal edges into
    // `archived` are `completed → archived` and `blocked → archived`. Archive
    // plans should only ever target completed/blocked projects; if we somehow
    // reach here from another state, refuse to record an illegal edge and log
    // for an external watchdog rather than corrupt lifecycle history.
    if !matches!(current.as_str(), "completed" | "blocked") {
        tracing::error!(
            %project_id, %plan_id, from_state = %current,
            "archive lifecycle closure: refusing illegal edge into 'archived' (legal sources: completed, blocked); leaving lifecycle unchanged"
        );
        return;
    }

    let repo = SqliteLifecycleRepository::new(pool.clone(), bus.clone());
    let cmd = TransitionCommand {
        entity_id: EntityId::from_uuid(uuid),
        entity_type: EntityType::Project,
        from_state: current,
        to_state: "archived".to_owned(),
        trigger: "archive.plan.applied".to_owned(),
        actor: "user".to_owned(),
        request_id: EntityId::new(),
    };

    match transition_lifecycle(&repo, bus, cmd).await {
        Ok(_) => {
            if let Err(e) = projects_repo::set_archived_via_plan_id(pool, project_id, plan_id).await
            {
                tracing::error!(%project_id, error=%e, "archive lifecycle closure: transition succeeded but recording archived_via_plan_id failed");
            }
        }
        Err(e) => {
            tracing::error!(%project_id, %plan_id, error=%e, "archive lifecycle closure: transition to archived failed");
        }
    }
}

/// Terminal step of a successful `origin = restore` plan apply (#885): drive
/// the owning project back out of `archived` (R-Unarchive, `archived → ready`)
/// and clear `archived_via_plan_id` so the Archive listing (which filters on
/// `lifecycle = 'archived'`) drops the row. Mirrors
/// [`finalize_archive_lifecycle`]'s shape; the only legal source state here is
/// `archived` itself, so this closure is never idempotent-on-already-restored
/// the way the archive closure is idempotent-on-already-archived.
///
/// Best-effort: the files are already moved back, so a failure here must NOT
/// fail the apply. Every failure is logged for an external watchdog (§II).
#[expect(
    clippy::cognitive_complexity,
    reason = "restore finalize: lifecycle transition per project entity with per-step logging"
)]
pub(super) async fn finalize_restore_lifecycle(
    pool: &SqlitePool,
    bus: &EventBus,
    project_id: &str,
) {
    use crate::lifecycle::lifecycle_use_case::{transition_lifecycle, TransitionCommand};
    use domain_core::ids::EntityId;
    use domain_core::lifecycle::data_asset::EntityType;
    use persistence_lifecycle::repositories::lifecycle::SqliteLifecycleRepository;
    use persistence_plans::repositories::projects as projects_repo;

    let uuid = match uuid::Uuid::parse_str(project_id) {
        Ok(u) => u,
        Err(e) => {
            tracing::error!(%project_id, error=%e, "restore lifecycle closure: project id is not a uuid");
            return;
        }
    };

    let current = match projects_repo::get_project(pool, project_id).await {
        Ok(p) => p.lifecycle,
        Err(e) => {
            tracing::error!(%project_id, error=%e, "restore lifecycle closure: project not found");
            return;
        }
    };

    // Edge-legality guard (Constitution §II): the only legal source for
    // R-Unarchive is `archived` itself (domain_core::lifecycle::project).
    if current != "archived" {
        tracing::error!(
            %project_id, from_state = %current,
            "restore lifecycle closure: refusing illegal edge out of a non-archived state; leaving lifecycle unchanged"
        );
        return;
    }

    let repo = SqliteLifecycleRepository::new(pool.clone(), bus.clone());
    let cmd = TransitionCommand {
        entity_id: EntityId::from_uuid(uuid),
        entity_type: EntityType::Project,
        from_state: current,
        to_state: "ready".to_owned(),
        trigger: "archive.plan.restore.applied".to_owned(),
        actor: "user".to_owned(),
        request_id: EntityId::new(),
    };

    match transition_lifecycle(&repo, bus, cmd).await {
        Ok(_) => {
            if let Err(e) = projects_repo::clear_archived_via_plan_id(pool, project_id).await {
                tracing::error!(%project_id, error=%e, "restore lifecycle closure: transition succeeded but clearing archived_via_plan_id failed");
            }
        }
        Err(e) => {
            tracing::error!(%project_id, error=%e, "restore lifecycle closure: transition to ready failed");
        }
    }
}

/// Terminal step of a clean `origin = prepared_view_generation` apply: drive the
/// owning project `ready → prepared`. This is the closure of the requires-plan
/// gate on that edge (spec 009 research §Which edges trigger filesystem work:
/// side effect "PreparedSource artifact creation", gating "approved plan
/// required") — the artifact the edge requires has just been materialised, and
/// `apply_transition` refuses the edge unconditionally, so without this closure
/// no project can ever reach `prepared`.
///
/// Only `ready → prepared` is legal into `prepared` for a project
/// (`domain_core::lifecycle::project::is_allowed`); an already-`prepared`
/// project is a no-op, so re-applying a second generation plan is harmless.
///
/// Best-effort: the links already exist on disk, so a failure here must NOT
/// fail the apply. Every failure is logged for an external watchdog (§II).
#[expect(
    clippy::cognitive_complexity,
    reason = "prepared finalize: lifecycle transition per project entity with per-step logging"
)]
pub(super) async fn finalize_prepared_lifecycle(
    pool: &SqlitePool,
    bus: &EventBus,
    project_id: &str,
) {
    use crate::lifecycle::lifecycle_use_case::{transition_lifecycle, TransitionCommand};
    use domain_core::ids::EntityId;
    use domain_core::lifecycle::data_asset::EntityType;
    use persistence_lifecycle::repositories::lifecycle::SqliteLifecycleRepository;
    use persistence_plans::repositories::projects as projects_repo;

    let uuid = match uuid::Uuid::parse_str(project_id) {
        Ok(u) => u,
        Err(e) => {
            tracing::error!(%project_id, error=%e, "prepared lifecycle closure: project id is not a uuid");
            return;
        }
    };

    let current = match projects_repo::get_project(pool, project_id).await {
        Ok(p) => p.lifecycle,
        Err(e) => {
            tracing::error!(%project_id, error=%e, "prepared lifecycle closure: project not found");
            return;
        }
    };

    if current == "prepared" {
        return;
    }

    if current != "ready" {
        tracing::warn!(
            %project_id, from_state = %current,
            "prepared lifecycle closure: source view applied outside 'ready'; leaving lifecycle unchanged"
        );
        return;
    }

    let repo = SqliteLifecycleRepository::new(pool.clone(), bus.clone());
    let cmd = TransitionCommand {
        entity_id: EntityId::from_uuid(uuid),
        entity_type: EntityType::Project,
        from_state: current,
        to_state: "prepared".to_owned(),
        trigger: "sourceview.plan.applied".to_owned(),
        actor: "user".to_owned(),
        request_id: EntityId::new(),
    };

    if let Err(e) = transition_lifecycle(&repo, bus, cmd).await {
        tracing::error!(%project_id, error=%e, "prepared lifecycle closure: transition to prepared failed");
    }
}

// ── Calibration master archive lifecycle closure (#886) ──────────────────────

/// Terminal step of a successful `origin = calibration_master_archive` plan
/// apply: mark the master archived + record the owning plan, so
/// `calibration.masters.list` drops it and `archive.list` picks it up.
///
/// Unlike [`finalize_archive_lifecycle`], masters have no lifecycle state
/// machine (migration 0050 dropped `calibration_session.state`) — this is a
/// plain flag+link set only from this call site (Constitution §II:
/// reviewable-plan discipline gates the write, not a schema constraint).
/// Idempotent: re-recording an already-archived master just overwrites the
/// plan link.
///
/// Best-effort: the file is already archived, so a failure here must NOT
/// fail the apply. Every failure is logged for an external watchdog (§II).
pub(super) async fn finalize_calibration_master_archive(
    pool: &SqlitePool,
    plan_id: &str,
    master_id: &str,
) {
    use persistence_calibration::repositories::q_calibration;

    let archived_at = Timestamp::now_iso();
    match q_calibration::set_master_archived(pool, master_id, plan_id, &archived_at).await {
        Ok(()) => {
            // F0 invalidate-after-commit contract (crates/app/cache/src/lib.rs,
            // mirrors plan_listener.rs's master-confirm write): the write above
            // has committed (sqlx pool auto-commits per statement), so the
            // masters snapshot cache is safe to clear now — never before, to
            // avoid a reader repopulating it with a stale pre-commit value.
            // `calibration.masters.list` reads through this no-TTL cache; without
            // this call an archived master stays visible until an unrelated
            // master-confirm event happens to invalidate it.
            crate::calibration::caches::invalidate_calibration_masters();
        }
        Err(e) => {
            tracing::error!(
                %master_id, %plan_id, error=%e,
                "master archive closure: failed to record archived_at/archived_via_plan_id"
            );
        }
    }
}

/// Terminal step of a successful `origin = calibration_master_restore` plan
/// apply (#886): clear the archived flag so `calibration.masters.list`
/// picks the master back up and `archive.list` drops it.
///
/// Best-effort: the file is already moved back, so a failure here must NOT
/// fail the apply. Every failure is logged for an external watchdog (§II).
pub(super) async fn finalize_calibration_master_restore(pool: &SqlitePool, master_id: &str) {
    use persistence_calibration::repositories::q_calibration;

    match q_calibration::clear_master_archived(pool, master_id).await {
        // Same invalidate-after-commit contract as the archive closure above.
        Ok(()) => crate::calibration::caches::invalidate_calibration_masters(),
        Err(e) => {
            tracing::error!(%master_id, error=%e, "master restore closure: failed to clear archived flag");
        }
    }
}
