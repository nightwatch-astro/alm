// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Project health use case — spec 009 US4 + P7 + P8.
//!
//! Responsibilities:
//! - `check_project_ready_invariant`: fires `setup_incomplete → ready`
//!   auto-transition (P8, R-Ready-Trigger) when `≥1 source` is confirmed.
//! - `emit_block_transition`: emits `* → blocked` when a detectable condition
//!   is found (US4, P7 debounce).
//!
//! ## Storage note
//! The spec 008 `projects` table uses a `lifecycle` column (not `state`).
//! The spec 002 `LifecycleRepository::record_transition` targets the older
//! `project` table which has a `state` column. These are DIFFERENT tables.
//! This module writes to the `projects` table via `repo::update_project_lifecycle`
//! and publishes events on the bus, keeping the lifecycle consistent without
//! hitting the legacy lifecycle repository.
//!
//! ## Actor=system authorization (A4)
//! The `setup_incomplete → ready` edge is explicitly listed as a permitted
//! system-actor invariant transition in spec 009 data-model.md §A4.
//!
//! ## Debounce (P7)
//! The detector MUST suppress re-emission of a block signal for the same
//! `(entity_id, condition_kind)` pair within a 60-second window. The lifecycle
//! use case itself has no debounce; all suppression lives here.
//!
//! ## Deferred detection reasons
//! - `calibration_unmatched`: requires spec 007 calibration matching tables.
//! - `prepared_source_stale`: requires spec 012 / prepared_source_view table.
//! - Both are deferred and documented in tasks.md.

use std::time::Duration;

use app_core_cache::DebounceCache;
use audit::bus::EventBus;
use audit::event_bus::{LifecycleTransitionApplied, Source, TOPIC_LIFECYCLE_TRANSITION_APPLIED};
use persistence_plans::repositories::projects as repo;
use sqlx::SqlitePool;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Default debounce window per (entity_id, condition) pair (P7, D5).
/// Configurable via this in-process constant (no user-facing setting).
#[allow(clippy::duration_suboptimal_units)]
pub const DEBOUNCE_WINDOW: Duration = Duration::from_secs(60);

// ── Blocking reason (mirrors data-model.md §BlockedReason) ───────────────────

/// Structured blocking reason for a system-detected block.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum BlockCondition {
    SourceMissing { inventory_id: String },
    ToolUnconfigured { tool: String },
    User { note: String },
}

impl BlockCondition {
    #[must_use]
    pub fn kind_str(&self) -> &'static str {
        match self {
            Self::SourceMissing { .. } => "source_missing",
            Self::ToolUnconfigured { .. } => "tool_unconfigured",
            Self::User { .. } => "user",
        }
    }

    /// Human-readable message for audit records and the event bus.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::SourceMissing { inventory_id } => format!("Source missing: {inventory_id}"),
            Self::ToolUnconfigured { tool } => format!("Tool path not configured: {tool}"),
            Self::User { note } => note.clone(),
        }
    }
}

// ── Debounce table ────────────────────────────────────────────────────────────

/// Key for the block-transition debounce cache: entity + condition kind.
///
/// Presence of a (non-expired) entry in a [`DebounceCache<DebounceKey>`] means
/// the signal was emitted within the debounce window and must be suppressed.
/// The cache's `time_to_live` is the debounce window (see [`DEBOUNCE_WINDOW`]),
/// so entries auto-expire without manual bookkeeping.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct DebounceKey {
    entity_id: String,
    condition_kind: &'static str,
}

impl DebounceKey {
    fn new(entity_id: &str, condition_kind: &'static str) -> Self {
        Self { entity_id: entity_id.to_owned(), condition_kind }
    }
}

// ── Error ─────────────────────────────────────────────────────────────────────

/// Error from the health use cases.
#[derive(Debug, thiserror::Error)]
pub enum HealthError {
    #[error("persistence error: {0}")]
    Persistence(#[from] persistence_core::DbError),
    #[error("project not found: {0}")]
    NotFound(String),
}

// ── P8: setup_incomplete → ready invariant check ──────────────────────────────

/// Check the `setup_incomplete → ready` invariant (P8, R-Ready-Trigger).
///
/// Condition: `lifecycle == "setup_incomplete"` AND `≥1 source mapped`.
///
/// Writes to the `projects` table via `repo::update_project_lifecycle`, writes
/// an audit row (FR-021), then publishes `project.lifecycle.ready` on the event
/// bus (P8-3).
///
/// Returns `Some("ready")` when the transition fires, `None` when the condition
/// is not met or the project is already past `setup_incomplete`.
///
/// # Errors
/// Returns `HealthError` on database failure.
pub async fn check_project_ready_invariant(
    pool: &SqlitePool,
    bus: &EventBus,
    project_id: &str,
) -> Result<Option<String>, HealthError> {
    let row = repo::get_project(pool, project_id)
        .await
        .map_err(|e| HealthError::NotFound(format!("project {project_id}: {e}")))?;

    if domain_core::lifecycle::ProjectState::parse_str(&row.lifecycle)
        != Some(domain_core::lifecycle::ProjectState::SetupIncomplete)
    {
        return Ok(None);
    }

    let sources =
        repo::list_project_sources(pool, project_id).await.map_err(HealthError::Persistence)?;
    if sources.is_empty() {
        return Ok(None);
    }

    repo::update_project_lifecycle(pool, project_id, "ready")
        .await
        .map_err(HealthError::Persistence)?;

    let now = domain_core::ids::Timestamp::now_utc();

    // FR-021: write an audit row for the automatic ready transition.
    write_auto_transition_audit(
        pool,
        project_id,
        "setup_incomplete",
        "ready",
        "auto: setup_incomplete→ready invariant",
    )
    .await
    .map_err(HealthError::Persistence)?;

    // Publish project.lifecycle.ready event (P8-3).
    let _ = bus
        .publish(
            "project.lifecycle.ready",
            Source::System,
            LifecycleTransitionApplied {
                entity_type: domain_core::lifecycle::data_asset::EntityType::Project,
                entity_id: project_id.to_owned(),
                from_state: "setup_incomplete".to_owned(),
                to_state: "ready".to_owned(),
                actor: "system".to_owned(),
                at: now,
                project_id: Some(project_id.to_owned()),
            },
        )
        .await;

    Ok(Some("ready".to_owned()))
}

// ── US4: auto-blocked detection ───────────────────────────────────────────────

/// Lightweight record returned when a block transition was applied.
#[derive(Clone, Debug)]
pub struct BlockTransitionRecord {
    pub project_id: String,
    pub from_state: String,
    pub condition: BlockCondition,
}

enum BlockTransitionOutcome {
    Applied(BlockTransitionRecord),
    Noop,
    RetryableCasLoss,
}

/// Whether a source-missing invariant pass completed or must be retried after
/// losing a lifecycle compare-and-swap to another blockable state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceMissingCheckOutcome {
    Complete,
    RetryRequired,
}

/// Check the `source_missing` block invariant (US4, FR-020) across every
/// project whose linked acquisition sessions reference a `missing` frame
/// record, and apply the `* → blocked` transition to each.
///
/// This is the detection half that `emit_block_transition` (the emission half)
/// has no caller for on its own — a project whose files disappeared stays
/// `ready` unless something runs this. `frame_inventory::run_reconcile` is the
/// production trigger: it is the only writer of `file_record.state='missing'`.
///
/// `debounce` is passed in rather than read from a global here because the
/// process-global table lives in `app_core::caches`, which depends on this
/// crate.
///
/// # Errors
/// Returns `HealthError` on database failure.
pub async fn check_project_source_missing_invariant(
    pool: &SqlitePool,
    bus: &EventBus,
    debounce: &DebounceCache<DebounceKey>,
) -> Result<SourceMissingCheckOutcome, HealthError> {
    let pairs =
        repo::find_blockable_missing_sources(pool).await.map_err(HealthError::Persistence)?;

    let mut retry_required = false;
    for (project_id, inventory_id) in pairs {
        let condition = BlockCondition::SourceMissing { inventory_id };
        if matches!(
            emit_block_transition_outcome(pool, bus, debounce, &project_id, &condition).await?,
            BlockTransitionOutcome::RetryableCasLoss
        ) {
            retry_required = true;
        }
    }
    Ok(if retry_required {
        SourceMissingCheckOutcome::RetryRequired
    } else {
        SourceMissingCheckOutcome::Complete
    })
}

/// Emit a system-driven `* → blocked` transition for the given project.
///
/// - Performs debounce (P7): if the same `(project_id, condition_kind)` was
///   emitted within `DEBOUNCE_WINDOW`, the transition is suppressed.
/// - Writes the new `blocked` lifecycle + typed blocked reason (FR-020) to
///   the `projects` table.
/// - Writes an audit row for the auto-transition (FR-021).
/// - Publishes `TOPIC_LIFECYCLE_TRANSITION_APPLIED` and
///   `project.lifecycle.blocked` on the event bus.
///
/// Returns `Some(record)` when applied, `None` when debounced or already blocked.
///
/// # Errors
/// Returns `HealthError` on database failure.
pub async fn emit_block_transition(
    pool: &SqlitePool,
    bus: &EventBus,
    debounce: &DebounceCache<DebounceKey>,
    project_id: &str,
    condition: &BlockCondition,
) -> Result<Option<BlockTransitionRecord>, HealthError> {
    Ok(match emit_block_transition_outcome(pool, bus, debounce, project_id, condition).await? {
        BlockTransitionOutcome::Applied(record) => Some(record),
        BlockTransitionOutcome::Noop | BlockTransitionOutcome::RetryableCasLoss => None,
    })
}

async fn emit_block_transition_outcome(
    pool: &SqlitePool,
    bus: &EventBus,
    debounce: &DebounceCache<DebounceKey>,
    project_id: &str,
    condition: &BlockCondition,
) -> Result<BlockTransitionOutcome, HealthError> {
    let condition_kind = condition.kind_str();

    let debounce_key = DebounceKey::new(project_id, condition_kind);
    if debounce.is_suppressed(&debounce_key) {
        return Ok(BlockTransitionOutcome::Noop);
    }

    let row = repo::get_project(pool, project_id)
        .await
        .map_err(|e| HealthError::NotFound(format!("project {project_id}: {e}")))?;

    let from_state = row.lifecycle.clone();
    let message = condition.message();
    let trigger = format!("auto block: {condition_kind} — {message}");

    let outcome = repo::apply_project_auto_block(
        pool,
        project_id,
        &from_state,
        condition_kind,
        &message,
        &trigger,
    )
    .await
    .map_err(HealthError::Persistence)?;
    match outcome {
        repo::ProjectAutoBlockOutcome::Applied => {}
        repo::ProjectAutoBlockOutcome::CasLost { still_blockable: true, .. } => {
            return Ok(BlockTransitionOutcome::RetryableCasLoss);
        }
        repo::ProjectAutoBlockOutcome::Rejected
        | repo::ProjectAutoBlockOutcome::CasLost { still_blockable: false, .. } => {
            return Ok(BlockTransitionOutcome::Noop);
        }
    }
    debounce.record(&debounce_key);

    let now = domain_core::ids::Timestamp::now_utc();

    let _ = bus
        .publish(
            TOPIC_LIFECYCLE_TRANSITION_APPLIED,
            Source::System,
            LifecycleTransitionApplied {
                entity_type: domain_core::lifecycle::data_asset::EntityType::Project,
                entity_id: project_id.to_owned(),
                from_state: from_state.clone(),
                to_state: "blocked".to_owned(),
                actor: "system".to_owned(),
                at: now,
                project_id: Some(project_id.to_owned()),
            },
        )
        .await;

    let _ = bus
        .publish(
            "project.lifecycle.blocked",
            Source::System,
            serde_json::json!({
                "projectId": project_id,
                "fromState": from_state,
                "conditionKind": condition_kind,
                "message": message,
            }),
        )
        .await;

    Ok(BlockTransitionOutcome::Applied(BlockTransitionRecord {
        project_id: project_id.to_owned(),
        from_state,
        condition: condition.clone(),
    }))
}

/// Emit a system-driven `archived → ready` unarchive transition for a project
/// (R-Unarchive, FR-021). This is a plan-required edge; this helper is called
/// after the plan is applied to write the audit row and emit `project.unarchived`.
///
/// # Errors
/// Returns `HealthError` on database failure.
pub async fn emit_unarchive_transition(
    pool: &SqlitePool,
    bus: &EventBus,
    project_id: &str,
) -> Result<(), HealthError> {
    let row = repo::get_project(pool, project_id)
        .await
        .map_err(|e| HealthError::NotFound(format!("project {project_id}: {e}")))?;

    if !domain_core::project::validate::is_read_only(&row.lifecycle) {
        return Ok(());
    }

    repo::update_project_lifecycle_unblock(pool, project_id, "ready")
        .await
        .map_err(HealthError::Persistence)?;

    // FR-021: audit row for the unarchive auto-transition.
    write_auto_transition_audit(pool, project_id, "archived", "ready", "auto: project.unarchived")
        .await
        .map_err(HealthError::Persistence)?;

    let now = domain_core::ids::Timestamp::now_utc();

    let _ = bus
        .publish(
            TOPIC_LIFECYCLE_TRANSITION_APPLIED,
            Source::System,
            LifecycleTransitionApplied {
                entity_type: domain_core::lifecycle::data_asset::EntityType::Project,
                entity_id: project_id.to_owned(),
                from_state: "archived".to_owned(),
                to_state: "ready".to_owned(),
                actor: "system".to_owned(),
                at: now,
                project_id: Some(project_id.to_owned()),
            },
        )
        .await;

    // FR-021: emit the `project.unarchived` named event.
    let _ = bus
        .publish(
            "project.unarchived",
            Source::System,
            serde_json::json!({
                "projectId": project_id,
                "at": now.as_offset_date_time()
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned()),
            }),
        )
        .await;

    Ok(())
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Write a durable audit row for a system-driven (actor=system) lifecycle
/// transition. Used by `check_project_ready_invariant`, `emit_block_transition`,
/// and `emit_unarchive_transition` to satisfy FR-021 (Constitution §V).
///
/// Delegates the raw insert to `persistence_db` (db-boundary rule: no `sqlx` in
/// the app layer).
async fn write_auto_transition_audit(
    pool: &SqlitePool,
    project_id: &str,
    from_state: &str,
    to_state: &str,
    trigger: &str,
) -> persistence_core::DbResult<()> {
    persistence_lifecycle::repositories::audit::insert_project_auto_transition(
        pool, project_id, from_state, to_state, trigger,
    )
    .await
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use audit::bus::EventBus;
    use contracts_core::projects_v2::{ProjectCreateRequest, ProjectSourceAddRequest};
    use persistence_core::Database;
    use sqlx::SqlitePool;

    use super::*;
    use crate::project_setup;

    async fn setup() -> (SqlitePool, EventBus) {
        let db = Database::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        let bus = EventBus::with_pool(db.pool().clone());
        (db.pool().clone(), bus)
    }

    fn new_id() -> String {
        domain_core::ids::new_id()
    }

    /// These tests create projects with no `canonical_target_id`, so
    /// `create()`'s promotion never touches the cache.
    fn empty_cache() -> simbad_resolver::RedbCache {
        simbad_resolver::Store::in_memory().unwrap().cache()
    }

    fn make_create_req(name: &str) -> ProjectCreateRequest {
        use contracts_core::projects_v2::ProjectTool;
        ProjectCreateRequest {
            request_id: new_id(),
            name: name.to_owned(),
            tool: ProjectTool::PixInsight,
            // Platform-absolute: `project_setup::create` anchors/validates
            // the path (Constitution I); these tests exercise health, not
            // anchoring, so no project root needs to be registered.
            path: crate::test_support::abs(&format!("/library/projects/{name}")),
            initial_sources: vec![],
            notes: None,
            canonical_target_id: None,
            is_mosaic: false,
        }
    }

    // P8-4: no sources → invariant not met → no transition
    #[tokio::test]
    async fn ready_invariant_no_sources_no_op() {
        let (pool, bus) = setup().await;
        let created =
            project_setup::create(&pool, &bus, &empty_cache(), &make_create_req("M1 No Sources"))
                .await
                .unwrap();
        assert_eq!(created.lifecycle, "setup_incomplete");

        let result = check_project_ready_invariant(&pool, &bus, &created.project_id).await.unwrap();
        assert_eq!(result, None, "no sources → invariant not met → no transition");
    }

    // P8-4: with confirmed source → auto-transition fires
    #[tokio::test]
    async fn ready_invariant_with_source_transitions() {
        let (pool, bus) = setup().await;
        let created =
            project_setup::create(&pool, &bus, &empty_cache(), &make_create_req("NGC 7000 Auto"))
                .await
                .unwrap();
        assert_eq!(created.lifecycle, "setup_incomplete");

        // Add a source (project_setup.add_source also calls maybe_auto_ready;
        // reset lifecycle afterward to isolate the invariant check).
        project_setup::add_source(
            &pool,
            &bus,
            &ProjectSourceAddRequest {
                request_id: new_id(),
                project_id: created.project_id.clone(),
                inventory_session_id: "inv-auto-001".to_owned(),
            },
        )
        .await
        .unwrap();
        persistence_plans::repositories::projects::update_project_lifecycle(
            &pool,
            &created.project_id,
            "setup_incomplete",
        )
        .await
        .unwrap();

        let result = check_project_ready_invariant(&pool, &bus, &created.project_id).await.unwrap();
        assert_eq!(result, Some("ready".to_owned()), "source present → transition fires");
    }

    // P8-4: invariant is a no-op when project is already ready
    #[tokio::test]
    async fn ready_invariant_already_ready_no_op() {
        let (pool, bus) = setup().await;
        let created =
            project_setup::create(&pool, &bus, &empty_cache(), &make_create_req("Already Ready"))
                .await
                .unwrap();

        persistence_plans::repositories::projects::update_project_lifecycle(
            &pool,
            &created.project_id,
            "ready",
        )
        .await
        .unwrap();

        let result = check_project_ready_invariant(&pool, &bus, &created.project_id).await.unwrap();
        assert_eq!(result, None, "already ready → no-op");
    }

    // P7-2: debounce suppresses duplicate block signals within window
    #[tokio::test]
    async fn debounce_suppresses_duplicate_block() {
        let (pool, bus) = setup().await;
        let debounce = DebounceCache::new(DEBOUNCE_WINDOW);
        let created =
            project_setup::create(&pool, &bus, &empty_cache(), &make_create_req("M31 Block Test"))
                .await
                .unwrap();
        let condition = BlockCondition::SourceMissing { inventory_id: "inv-missing".to_owned() };

        let first = emit_block_transition(&pool, &bus, &debounce, &created.project_id, &condition)
            .await
            .unwrap();
        assert!(first.is_some(), "first block signal should be applied");

        // Reset lifecycle so a real write would succeed if debounce weren't active.
        persistence_plans::repositories::projects::update_project_lifecycle(
            &pool,
            &created.project_id,
            "setup_incomplete",
        )
        .await
        .unwrap();

        let second = emit_block_transition(&pool, &bus, &debounce, &created.project_id, &condition)
            .await
            .unwrap();
        assert!(second.is_none(), "second signal within window → debounce suppressed");
    }

    // P7-2: after window expires, a second signal is emitted
    #[tokio::test]
    async fn debounce_allows_after_window_expires() {
        let (pool, bus) = setup().await;
        let debounce = DebounceCache::new(DEBOUNCE_WINDOW);
        let created =
            project_setup::create(&pool, &bus, &empty_cache(), &make_create_req("M82 Expiry"))
                .await
                .unwrap();
        let condition = BlockCondition::SourceMissing { inventory_id: "inv-gone".to_owned() };

        emit_block_transition(&pool, &bus, &debounce, &created.project_id, &condition)
            .await
            .unwrap();

        // Expire the debounce entry manually (avoids a 60s sleep).
        debounce.invalidate(&DebounceKey::new(&created.project_id, "source_missing"));

        // Reset lifecycle so the transition can succeed again.
        persistence_plans::repositories::projects::update_project_lifecycle(
            &pool,
            &created.project_id,
            "setup_incomplete",
        )
        .await
        .unwrap();

        let second = emit_block_transition(&pool, &bus, &debounce, &created.project_id, &condition)
            .await
            .unwrap();
        assert!(second.is_some(), "after window expiry → second signal applied");
    }

    // P7-3: integration — two rapid source_missing signals produce only one blocked state
    #[tokio::test]
    async fn rapid_source_missing_produces_one_transition() {
        let (pool, bus) = setup().await;
        let debounce = DebounceCache::new(DEBOUNCE_WINDOW);
        let created =
            project_setup::create(&pool, &bus, &empty_cache(), &make_create_req("IC 1805 Rapid"))
                .await
                .unwrap();
        let condition = BlockCondition::SourceMissing { inventory_id: "inv-rapid".to_owned() };

        // First signal — applied.
        let first = emit_block_transition(&pool, &bus, &debounce, &created.project_id, &condition)
            .await
            .unwrap();
        assert!(first.is_some());

        let row =
            persistence_plans::repositories::projects::get_project(&pool, &created.project_id)
                .await
                .unwrap();
        assert_eq!(row.lifecycle, "blocked", "project should be blocked after first signal");

        // Second signal immediately — suppressed by debounce.
        let second = emit_block_transition(&pool, &bus, &debounce, &created.project_id, &condition)
            .await
            .unwrap();
        assert!(second.is_none(), "second rapid signal must be debounced");

        // Verify: lifecycle unchanged (still blocked, no second write occurred).
        let row2 =
            persistence_plans::repositories::projects::get_project(&pool, &created.project_id)
                .await
                .unwrap();
        assert_eq!(row2.lifecycle, "blocked", "lifecycle unchanged — no double-write");
    }
}
