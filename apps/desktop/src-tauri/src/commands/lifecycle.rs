// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Spec 002 lifecycle commands exposed to the Tauri webview.
//!
//! Every command is annotated with both `#[tauri::command]` (so Tauri
//! discovers it) and `#[specta::specta]` (so tauri-specta emits a typed TS
//! binding for it). Inputs/outputs are the language-neutral contract DTOs in
//! `contracts_core::{lifecycle,provenance}` plus a small ledger filter DTO
//! defined locally so we don't leak persistence-internal types through the
//! IPC boundary.

use std::sync::Arc;

use app_core::ledger_use_case::list_assets_ledger;
use app_core::provenance_use_case::read_provenance;
use app_core::transition_use_case::{apply_transition, preview_transition};
use audit::bus::EventBus;
use contracts_core::lifecycle::{TransitionRequest, TransitionResponse};
use contracts_core::provenance::{ProvenanceReadRequest, ProvenanceReadResponse};
use domain_core::ids::EntityId;
use domain_core::lifecycle::data_asset::EntityType;
use persistence_lifecycle::repositories::lifecycle::{
    LedgerFilter, LedgerRow, SqliteLifecycleRepository,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;
use uuid::Uuid;

/// Shared application state managed by Tauri.
pub struct AppState {
    pub repo: Arc<SqliteLifecycleRepository>,
    pub bus: EventBus,
    /// Per-instance in-memory caches (replaces process-global statics for
    /// testability). Shared via `Arc` with background tasks that lack a
    /// `Tauri::State` handle.
    pub caches: Arc<app_core::AppCaches>,
    /// Spec 052 P1 (D2): the shared SIMBAD resolve cache — one global redb
    /// file, opened once at app startup. Readers clone the cheap `Arc`-backed
    /// handle out from under a short-lived read lock; `target.cache.clear`
    /// (`commands::resolve_cache::clear_and_rewarm`) takes the write lock
    /// only to swap in a freshly reopened, still-empty store — the re-warm
    /// itself runs afterward as a background task that never takes this
    /// lock (issue #695).
    pub resolve_cache: tokio::sync::RwLock<targeting_resolver::simbad::ResolveCache>,
    /// Filesystem path backing `resolve_cache`, needed by
    /// `target.cache.clear` to delete + reopen the redb file.
    pub resolve_cache_path: std::path::PathBuf,
    /// True while a bundled-seed/durable-row re-warm of `resolve_cache` is
    /// running in the background (the startup warm in `lib.rs`, or the one
    /// `commands::resolve_cache::clear_and_rewarm` schedules). Set true and
    /// cleared by a [`CacheWarmingGuard`] held for the warm task's whole
    /// scope, never a bare sequential store, so a panic mid-warm still clears
    /// it; `target.search` surfaces the value so a caller whose query landed
    /// mid-warm can tell a still-settling empty result apart from a genuine
    /// miss (issue #818). Two overlapping warms (startup racing an
    /// almost-immediate cache-clear) can make the flag go false slightly
    /// early, when the first of the two finishes — an accepted, rare
    /// edge case, not a full per-warm reference count.
    pub cache_warming: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl AppState {
    #[must_use]
    pub fn new(
        repo: Arc<SqliteLifecycleRepository>,
        bus: EventBus,
        caches: Arc<app_core::AppCaches>,
        resolve_cache: targeting_resolver::simbad::ResolveCache,
        resolve_cache_path: std::path::PathBuf,
        cache_warming: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        Self {
            repo,
            bus,
            caches,
            resolve_cache: tokio::sync::RwLock::new(resolve_cache),
            resolve_cache_path,
            cache_warming,
        }
    }
}

/// RAII guard that flips [`AppState::cache_warming`] back to `false` on
/// **any** scope exit — normal return *or* unwind from a panic inside the
/// warm task (issue #818 review). A bare sequential
/// `flag.store(false, ...)` after both warm phases is skipped if either
/// panics, since the unwind jumps past it; the flag then sticks `true` for
/// the rest of the process, and every later `target.search` pays the full
/// retry budget (`TargetSearch.tsx`) for a warm that will never finish.
///
/// [`CacheWarmingGuard::start`] sets the flag `true` and returns the guard in
/// one call (mirroring `plan_apply::check_overlap_and_register` + its
/// `ActiveRunGuard`) so the flag is visible to a concurrent `target.search`
/// the instant the caller schedules the warm — the guard itself is then
/// moved into the spawned task, so its `Drop` runs exactly once, whichever
/// way that task ends. Shared by both warm sites (`lib.rs`'s startup warm and
/// `resolve_cache.rs`'s `clear_and_rewarm`) instead of duplicating the
/// set/clear pair at each.
pub struct CacheWarmingGuard(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl CacheWarmingGuard {
    /// Set `flag` true and return the guard that will clear it on drop.
    #[must_use]
    pub fn start(flag: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Self {
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
        Self(flag)
    }
}

impl Drop for CacheWarmingGuard {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(test)]
mod cache_warming_guard_tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use super::CacheWarmingGuard;

    #[test]
    fn start_sets_true_and_normal_drop_clears_it() {
        let flag = Arc::new(AtomicBool::new(false));
        {
            let _guard = CacheWarmingGuard::start(flag.clone());
            assert!(flag.load(Ordering::Relaxed), "flag must be true while the guard is held");
        } // guard drops here
        assert!(!flag.load(Ordering::Relaxed), "guard Drop must clear the flag on normal exit");
    }

    /// Regression for the #818 review finding: a panic inside the warm task
    /// (after the guard is constructed) must still clear the flag, mirroring
    /// `plan_apply::active_run_guard_removes_entry_when_scope_panics`.
    #[test]
    fn panic_after_start_still_clears_the_flag() {
        let flag = Arc::new(AtomicBool::new(false));
        let flag_for_scope = flag.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            // Guard is owned by this scope, mirroring
            // `tokio::spawn(async move { let _guard = ...; warm().await })`.
            let _guard = CacheWarmingGuard::start(flag_for_scope);
            panic!("warm task panicked mid-warm");
        }));

        assert!(result.is_err(), "the scope must have panicked");
        assert!(
            !flag.load(Ordering::Relaxed),
            "guard Drop must clear the flag even when the scope unwinds from a panic"
        );
    }
}

/// JSON-friendly ledger filter mirrored to TypeScript via specta.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LedgerFilterDto {
    #[serde(default)]
    pub entity_types: Vec<String>,
    #[serde(default)]
    pub states: Vec<String>,
    #[serde(default)]
    pub project_id: Option<Uuid>,
    #[serde(default)]
    pub updated_after: Option<String>,
    #[serde(default)]
    pub updated_before: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub offset: Option<u32>,
}

impl LedgerFilterDto {
    fn into_filter(self) -> LedgerFilter {
        LedgerFilter {
            entity_types: self.entity_types.iter().filter_map(|s| parse_entity_type(s)).collect(),
            states: self.states,
            project_id: self.project_id.map(EntityId::from_uuid),
            updated_after: self.updated_after,
            updated_before: self.updated_before,
            limit: self.limit,
            offset: self.offset,
        }
    }
}

fn parse_entity_type(s: &str) -> Option<EntityType> {
    Some(match s {
        "library_root" => EntityType::LibraryRoot,
        "file_record" => EntityType::FileRecord,
        "data_source" => EntityType::DataSource,
        "project" => EntityType::Project,
        "prepared_source" => EntityType::PreparedSource,
        "processing_artifact" => EntityType::ProcessingArtifact,
        "projection" => EntityType::Projection,
        "plan" => EntityType::Plan,
        "filesystem_plan" => EntityType::FilesystemPlan,
        _ => return None,
    })
}

/// `provenance.read` Tauri command — returns the contract response shape.
///
/// # Errors
/// Never returns `Err`; persistence failures are folded into
/// `ProvenanceReadResponse::error(...)`. The `Result` shape exists so the
/// frontend's invoke wrapper still gets a typed envelope.
#[tauri::command]
#[specta::specta]
pub async fn provenance_read(
    state: State<'_, AppState>,
    request: ProvenanceReadRequest,
) -> Result<ProvenanceReadResponse, String> {
    Ok(read_provenance(state.repo.pool(), request).await)
}

/// `lifecycle.transition.apply` Tauri command.
///
/// #665: on a successful `Project` entity transition, fires the
/// `LifecycleTransition` manifest trigger — this and the source add/remove
/// trigger were the last of the 4 unwired manifest emitters (project create
/// and source add/remove are wired in `app_core_projects`; `workflow_run` was
/// the only one that ever existed).
///
/// # Errors
/// Never returns `Err`; refusal / persistence errors fold into
/// `TransitionResponse::error(...)` per the contract.
#[tauri::command]
#[specta::specta]
pub async fn lifecycle_transition_apply(
    state: State<'_, AppState>,
    request: TransitionRequest,
) -> Result<TransitionResponse, String> {
    let project_id = match &request {
        TransitionRequest::Project(req) => Some(req.entity_id.to_string()),
        _ => None,
    };

    let response = apply_transition(state.repo.as_ref(), &state.bus, request).await;

    if let Some(project_id) = project_id {
        if response.status == contracts_core::lifecycle::TransitionStatus::Success {
            app_core::project_manifests::write_lifecycle_manifest(
                state.repo.pool(),
                &state.bus,
                &project_id,
                contracts_core::manifests::ManifestReason::LifecycleTransition,
            )
            .await;
        }
    }

    Ok(response)
}

/// `lifecycle.transition.preview` — read-only dry-run for UI button enabling.
///
/// # Errors
/// Never returns `Err`; refusal codes fold into `TransitionResponse::error(...)`.
#[tauri::command]
#[specta::specta]
pub async fn lifecycle_transition_preview(
    request: TransitionRequest,
) -> Result<TransitionResponse, String> {
    Ok(preview_transition(request))
}

/// camelCase wire shape mirroring [`LedgerRow`] for the typed Tauri surface.
///
/// `LedgerRow` itself doesn't derive `specta::Type` (the persistence layer
/// stays language-internal). This DTO is the IPC projection.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LedgerRowDto {
    pub entity_id: Uuid,
    pub entity_type: String,
    pub current_state: String,
    pub title: Option<String>,
    pub path: Option<String>,
    pub project_id: Option<Uuid>,
    pub updated_at: Option<String>,
}

impl From<LedgerRow> for LedgerRowDto {
    fn from(row: LedgerRow) -> Self {
        Self {
            entity_id: row.entity_id.as_uuid(),
            entity_type: row.entity_type.as_str().to_owned(),
            current_state: row.current_state,
            title: row.title,
            path: row.path,
            project_id: row.project_id.map(EntityId::as_uuid),
            updated_at: row.updated_at,
        }
    }
}

/// `lifecycle.ledger.list` Tauri command.
///
/// Applies a server-side default limit clamp (1..=500, default 100) to prevent
/// unbounded result sets over IPC.
///
/// # Errors
/// Returns a stringified persistence error when the repository query fails
/// (e.g. transient DB unavailability). Successful empty results are `Ok(vec![])`.
#[tauri::command]
#[specta::specta]
pub async fn lifecycle_ledger_list(
    state: State<'_, AppState>,
    filter: LedgerFilterDto,
) -> Result<Vec<LedgerRowDto>, String> {
    let mut f = filter.into_filter();
    // Default limit clamp — same pattern as plans/read.rs:47.
    f.limit = Some(f.limit.unwrap_or(100).clamp(1, 500));
    list_assets_ledger(state.repo.as_ref(), f)
        .await
        .map(|rows| rows.into_iter().map(LedgerRowDto::from).collect())
        .map_err(|err| err.to_string())
}
