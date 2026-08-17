// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Path → project attribution (spec 012, WP-012-A).

use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use persistence_plans::repositories::artifacts::{self as repo};
use persistence_plans::repositories::projects::list_projects;
use persistence_targets::repositories::inventory::list_all_roots;
use workflow_artifacts::{resolve_project_for_path, ProjectPathRef};

/// Canonicalize `p`, falling back to `p` unchanged when canonicalization
/// fails (path does not exist yet, permission error, etc). Mirrors the
/// existing convention in `app_core::tool_launch`'s cwd-containment check so
/// the two resolution paths agree on what "under this project" means.
fn canonicalize_or_self(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

/// Resolve the id of the project that owns `artifact_path`, by longest-prefix
/// match against every registered project's root path.
///
/// Used by [`reattribute_root_keyed_artifacts`] to determine the real
/// `project_id` for rows the retired global watcher (pre-#400) keyed by the
/// library-root id verbatim (the WP-012-A bug). The live per-project watcher
/// (`apps/desktop/src-tauri/src/watcher.rs`) no longer needs event-time
/// resolution — it knows its project at attach time — so this resolver's
/// remaining runtime consumer is the startup fix-up.
///
/// Returns `Ok(None)` when no registered project's root contains the path —
/// callers MUST NOT fabricate an id in that case; spec 012's data model has
/// no "unattributed project" placeholder (`processing_artifacts.project_id`
/// is `NOT NULL`, migration 0025) and its user stories are scoped to "a
/// project's output folder", so a path outside every known project was never
/// something this feature promised to track.
///
/// # Errors
/// Returns `Err(String)` on DB failure.
pub async fn resolve_project_id_for_path(
    pool: &SqlitePool,
    artifact_path: &str,
) -> Result<Option<String>, String> {
    let projects =
        list_projects(pool).await.map_err(|e| format!("DB list projects failed: {e}"))?;
    if projects.is_empty() {
        return Ok(None);
    }

    let canon_artifact = canonicalize_or_self(Path::new(artifact_path));
    let artifact_str = canon_artifact.to_string_lossy().into_owned();

    let refs: Vec<ProjectPathRef> = projects
        .iter()
        .map(|p| ProjectPathRef {
            id: p.id.clone(),
            path: canonicalize_or_self(Path::new(&p.path)).to_string_lossy().into_owned(),
        })
        .collect();

    Ok(resolve_project_for_path(&artifact_str, &refs))
}

/// One-time, idempotent fix-up for `processing_artifacts` rows that the
/// pre-fix watcher keyed by a *library root* id instead of the owning
/// project's id (WP-012-A). A genuine project id is a UUID from
/// [`domain_core::ids::new_id`] and will never coincide with a
/// `library_root.id`, so any row whose `project_id` matches a real root id
/// is unambiguously mis-attributed.
///
/// For each such row, re-resolves the owning project from the artifact's
/// stored path (same algorithm as [`resolve_project_id_for_path`]) and
/// updates `project_id` in place. Rows whose path no longer matches any
/// registered project are left untouched (never deleted) and logged via
/// `tracing::warn` so they stay investigable.
///
/// Idempotent: once a row is corrected its `project_id` is a real project id
/// (not a root id), so a second pass skips it. Safe to run on every app
/// startup.
///
/// Returns `(re_attributed_count, still_unmatched_count)`.
///
/// # Errors
/// Returns `Err(String)` on DB failure.
#[expect(
    clippy::cognitive_complexity,
    reason = "per-artifact root reattribution loop with match and miss counters"
)]
pub async fn reattribute_root_keyed_artifacts(pool: &SqlitePool) -> Result<(usize, usize), String> {
    let roots = list_all_roots(pool).await.map_err(|e| format!("DB list roots failed: {e}"))?;
    if roots.is_empty() {
        return Ok((0, 0));
    }
    let root_ids: std::collections::HashSet<&str> = roots.iter().map(|r| r.id.as_str()).collect();

    let identities = repo::list_all_artifact_identities(pool)
        .await
        .map_err(|e| format!("DB list artifact identities failed: {e}"))?;

    let mut fixed = 0usize;
    let mut unmatched = 0usize;
    for identity in identities {
        if !root_ids.contains(identity.project_id.as_str()) {
            // Already keyed to a real project (or already fixed by a prior pass).
            continue;
        }

        let Some(real_project_id) = resolve_project_id_for_path(pool, &identity.path).await? else {
            unmatched += 1;
            tracing::warn!(
                "artifact re-attribution: artifact {} at '{}' still has a library-root id \
                 ({}) as project_id and no registered project claims its path; left as-is \
                 (not deleted)",
                identity.id,
                identity.path,
                identity.project_id
            );
            continue;
        };

        if let Err(e) = repo::set_project_id(pool, &identity.id, &real_project_id).await {
            tracing::warn!(
                "artifact re-attribution: failed to update artifact {} to project {}: {e}",
                identity.id,
                real_project_id
            );
            continue;
        }
        fixed += 1;
    }
    Ok((fixed, unmatched))
}
