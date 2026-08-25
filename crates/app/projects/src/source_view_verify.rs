// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Spec 049 US4: `sourceview.verify` — read-only pre-processing check that
//! every link in a generated (or spec-026-produced) source view still
//! resolves to a present canonical source.
//!
//! Companion to `crate::source_view_generate` and `crate::prepared_views`:
//! reuses the same `PreparedSourceView`/`PreparedSourceViewItem` entities.
//! FR-014/FR-015: this MUST NOT mutate the filesystem and MUST NOT
//! auto-repair — repair is via explicit `preparedview.regenerate`
//! (spec 026), never here.
//!
//! No project-lifecycle gate: verification is a read-only check, unlike
//! generate/remove/regenerate which mutate plan/view state.

use camino::Utf8Path;
use contracts_core::source_view_verify::{BrokenItem, BrokenItemState, SourceViewVerifyResponse};
use contracts_core::{error_code::ErrorCode, ContractError};
use domain_core::lifecycle::prepared_source::ItemObservedState;
use persistence_plans::repositories::prepared_source_views as views_repo;
use sqlx::SqlitePool;

// Domain-scoped DB error mapper: routes NotFound to view.not_found code.
// Other variants delegate to the canonical generic mapper (bd astro-plan-kyo7.88).
fn db_err(e: persistence_core::DbError) -> ContractError {
    app_core_errors::db_err_with_not_found(ErrorCode::ViewNotFound)(e)
}

/// Resolved canonical-source state for one view item's `inventory_item_id`.
///
/// `pub(crate)`: also reused by `prepared_views::regenerate_prepared_view`
/// (T013) to resolve each item's real absolute source path for the `link`
/// plan action, rather than duplicating this file_record→root-path lookup.
pub(crate) struct SourceResolution {
    /// Absolute path of the canonical source, when the source root and
    /// `file_record` row both resolve.
    pub(crate) abs_path: Option<camino::Utf8PathBuf>,
    /// `true` when the `file_record` row is absent or its state is
    /// `missing`/`rejected` — the source itself is gone from the inventory's
    /// point of view (FR-019-style "moved/removed" signal).
    pub(crate) source_gone: bool,
}

pub(crate) async fn resolve_source(pool: &SqlitePool, inventory_item_id: &str) -> SourceResolution {
    use persistence_targets::repositories::inventory;

    let Ok(Some(record)) = inventory::get_file_record_lookup(pool, inventory_item_id).await else {
        return SourceResolution { abs_path: None, source_gone: true };
    };

    if record.state == "missing" || record.state == "rejected" {
        return SourceResolution { abs_path: None, source_gone: true };
    }

    let root_path = inventory::get_library_root_path(pool, &record.root_id).await.unwrap_or(None);
    let Some(root_path) = root_path else {
        return SourceResolution { abs_path: None, source_gone: true };
    };

    SourceResolution {
        abs_path: Some(camino::Utf8PathBuf::from(root_path).join(&record.relative_path)),
        source_gone: false,
    }
}

/// Bytes probed from the start of each file for the copy-kind content-drift
/// check (#746). Partial, not a full-file hash: constitution ("Large-file
/// hashing MUST be optional or lazy") plus astro frames can be huge; a
/// changed size or a differing partial digest both prove drift, and this
/// mirrors the existing partial-hash convention (`app_core_inbox::signature`).
const HASH_PROBE_BYTES: usize = 65536;

/// Cheap (size, partial-digest) content probe for the drift check. `None`
/// when the file cannot be read (never treated as a divergence — avoids a
/// false positive from a transient permission/race error).
fn partial_content_probe(path: &Utf8Path) -> Option<(u64, [u8; 32])> {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    let probe_len = usize::try_from(size).unwrap_or(usize::MAX).min(HASH_PROBE_BYTES);
    let mut buf = vec![0u8; probe_len];
    let mut reader = file.take(probe_len as u64);
    reader.read_exact(&mut buf).ok()?;
    Some((size, Sha256::digest(&buf).into()))
}

/// Classify one view item against the live filesystem + inventory state.
/// `None` means the item is clean. Shared by `verify_source_view` (read-only
/// report) and `sweep_view_staleness` (T014/T015: same resolution logic,
/// persisted) so the two never diverge on what counts as "broken".
///
/// Per item:
/// 1. The canonical source (`inventory_item_id`) is resolved; a
///    missing/rejected/absent `file_record` row is reported as `moved`
///    (FR-019-style "the source is gone").
/// 2. The destination path (`view_relative_path`, recorded as an absolute
///    path at generation/regeneration time) is inspected with `lstat`
///    (never followed automatically): absent → `missing`.
/// 3. A symlink destination whose target does not resolve (dangling) →
///    `unresolved_link`.
/// 4. The recorded `materialization` disagreeing with the actual on-disk
///    kind (symlink vs. not) → `changed_kind`.
fn classify_item(
    source: &SourceResolution,
    item: &views_repo::PreparedSourceViewItemRow,
) -> Option<BrokenItemState> {
    if source.source_gone {
        return Some(BrokenItemState::Moved);
    }

    let dest = Utf8Path::new(&item.view_relative_path);
    let Ok(lstat) = std::fs::symlink_metadata(dest) else {
        return Some(BrokenItemState::Missing);
    };

    // The shared primitive, not `file_type().is_symlink()`: a Windows junction
    // is a reparse point that `is_symlink()` need not report, and a
    // misclassified one falls into the copy branch below, whose
    // `partial_content_probe` opens the path and so reads through the link.
    let is_symlink = fs_pathsafe::is_link_or_junction_metadata(&lstat);
    let recorded_symlink = item.materialization == "symlink";

    if is_symlink {
        // Dangling check: `metadata` follows the link; NotFound means the
        // target no longer exists (FR-015: report, never auto-repair).
        if std::fs::metadata(dest).is_err() {
            return Some(BrokenItemState::UnresolvedLink);
        }
        // Target resolves — but does it still point at the canonical
        // source (not merely at *some* live file)?
        if let Some(expected) = &source.abs_path {
            let matches_target = std::fs::canonicalize(dest)
                .ok()
                .zip(std::fs::canonicalize(expected).ok())
                .is_some_and(|(actual, expected)| actual == expected);
            if !matches_target {
                return Some(BrokenItemState::UnresolvedLink);
            }
        }
        if !recorded_symlink {
            return Some(BrokenItemState::ChangedKind);
        }
    } else if recorded_symlink {
        // Recorded as a symlink but the destination is a regular file —
        // the on-disk kind diverged (spec 026 FR-008 mixed-kind concept).
        return Some(BrokenItemState::ChangedKind);
    } else if item.materialization == "copy" {
        // Copy-kind destinations are independent bytes on disk (unlike a
        // hardlink, which shares the source's inode and can never drift) —
        // #746: content can silently diverge from the canonical source.
        if let Some(expected) = &source.abs_path {
            let diverged = match (partial_content_probe(dest), partial_content_probe(expected)) {
                (Some(a), Some(b)) => a != b,
                // Can't probe one side (permission/race) — don't false-positive.
                _ => false,
            };
            if diverged {
                return Some(BrokenItemState::HashDiverged);
            }
        }
    }
    // Hardlink destinations that lstat succeeded on are clean: their bytes
    // are identical to the source by construction (shared inode).
    None
}

/// Verify a `PreparedSourceView`'s links without mutating anything.
///
/// See `classify_item` for the per-item resolution rules.
///
/// # Errors
///
/// Returns `view.not_found` or an `internal.*` error on failure.
pub async fn verify_source_view(
    pool: &SqlitePool,
    view_id: &str,
) -> Result<SourceViewVerifyResponse, ContractError> {
    // 1. The view must exist (A4: records are never hard-deleted, so this
    //    only fails for a genuinely unknown id).
    views_repo::get_view(pool, view_id).await.map_err(db_err)?;

    let items = views_repo::list_view_items(pool, view_id).await.map_err(db_err)?;

    let mut broken_items = Vec::new();

    for item in items {
        let source = resolve_source(pool, &item.inventory_item_id).await;
        if let Some(state) = classify_item(&source, &item) {
            broken_items.push(BrokenItem {
                inventory_item_id: item.inventory_item_id,
                view_relative_path: item.view_relative_path,
                state,
            });
        }
    }

    let clean = broken_items.is_empty();
    Ok(SourceViewVerifyResponse { clean, broken_items })
}

/// Stale-detection sweep (spec 026 US3, T014/T015): recompute every item's
/// [`ItemObservedState`] from the live filesystem + inventory (same
/// `classify_item` logic as [`verify_source_view`]) and persist it, then
/// derive and persist the view's own `state` from the aggregate.
///
/// Read-only on the filesystem — it only ever `stat`s paths, never writes,
/// moves, or deletes anything (constitution §II: mutations only via
/// reviewable plans). The DB writes here are an observation cache, exactly
/// like inventory scan bookkeeping, not a filesystem mutation.
///
/// Terminal view states (`removed`, `kind_diverged`) are skipped: their
/// on-disk representation is either gone or blocked pending manual
/// resolution (D-026-H2), so a staleness sweep has nothing meaningful to
/// report for them.
///
/// View state after a sweep:
/// - `current`  — every item resolved clean.
/// - `missing`  — every item's destination itself is absent (the whole view
///   folder is gone).
/// - `stale`    — some, but not all, items are broken.
///
/// # Errors
///
/// Returns `view.not_found` or an `internal.*` error on failure.
pub async fn sweep_view_staleness(pool: &SqlitePool, view_id: &str) -> Result<(), ContractError> {
    let view = views_repo::get_view(pool, view_id).await.map_err(db_err)?;
    if view.state == "removed" || view.state == "kind_diverged" {
        return Ok(());
    }

    let items = views_repo::list_view_items(pool, view_id).await.map_err(db_err)?;
    if items.is_empty() {
        return Ok(());
    }

    let mut any_broken = false;
    let mut all_missing = true;

    for item in &items {
        let source = resolve_source(pool, &item.inventory_item_id).await;
        let broken = classify_item(&source, item);

        let observed = match broken {
            None => ItemObservedState::Present,
            Some(BrokenItemState::Missing) => ItemObservedState::Missing,
            Some(BrokenItemState::ChangedKind) => ItemObservedState::ChangedKind,
            Some(BrokenItemState::HashDiverged) => ItemObservedState::HashDiverged,
            Some(BrokenItemState::Moved | BrokenItemState::UnresolvedLink) => {
                ItemObservedState::Diverged
            }
        };

        if broken.is_some() {
            any_broken = true;
        }
        if !matches!(broken, Some(BrokenItemState::Missing)) {
            all_missing = false;
        }

        if item.last_observed_state != observed.as_str() {
            views_repo::update_item_observed_state(pool, &item.id, observed.as_str())
                .await
                .map_err(db_err)?;
        }
    }

    let new_state = if !any_broken {
        "current"
    } else if all_missing {
        "missing"
    } else {
        "stale"
    };

    if view.state != new_state {
        views_repo::update_view_state(pool, view_id, new_state).await.map_err(db_err)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_core::Database;

    async fn setup() -> Database {
        let db = Database::in_memory().await.expect("in-memory DB");
        db.migrate().await.expect("migrations");
        db
    }

    async fn insert_project(db: &Database, id: &str, path: &str) {
        sqlx::query(
            "INSERT INTO projects (id, name, tool, lifecycle, path, created_at, updated_at)
             VALUES (?, ?, 'PixInsight', 'ready', ?, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        )
        .bind(id)
        .bind(id)
        .bind(path)
        .execute(db.pool())
        .await
        .unwrap();
    }

    async fn insert_root(db: &Database, id: &str, path: &str) {
        sqlx::query(
            "INSERT INTO library_root (id, label, current_path, kind, state, created_at)
             VALUES (?, ?, ?, 'local', 'active', '2026-01-01T00:00:00Z')",
        )
        .bind(id)
        .bind(id)
        .bind(path)
        .execute(db.pool())
        .await
        .unwrap();
    }

    async fn insert_file_record(db: &Database, id: &str, root_id: &str, relative_path: &str) {
        sqlx::query(
            "INSERT INTO file_record (id, root_id, relative_path, size_bytes, mtime, state, first_seen_at, last_seen_at)
             VALUES (?, ?, ?, 100, '2026-01-01T00:00:00Z', 'classified', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        )
        .bind(id)
        .bind(root_id)
        .bind(relative_path)
        .execute(db.pool())
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn view_not_found_surfaces_error() {
        let db = setup().await;
        let err = verify_source_view(db.pool(), "nonexistent").await.unwrap_err();
        assert_eq!(err.code, ErrorCode::ViewNotFound);
    }

    #[tokio::test]
    async fn all_present_view_verifies_clean() {
        let db = setup().await;
        let dir = tempfile::tempdir().unwrap();
        let source_dir = dir.path().join("source");
        let dest_dir = dir.path().join("dest");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&dest_dir).unwrap();
        let source_file = source_dir.join("light1.fits");
        std::fs::write(&source_file, b"x").unwrap();
        let dest_file = dest_dir.join("light1.fits");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&source_file, &dest_file).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&source_file, &dest_file).unwrap();

        insert_project(&db, "p1", dir.path().join("proj").to_str().unwrap()).await;
        insert_root(&db, "root1", source_dir.to_str().unwrap()).await;
        insert_file_record(&db, "frame1", "root1", "light1.fits").await;

        views_repo::insert_view(
            db.pool(),
            &views_repo::InsertPreparedSourceView {
                id: "view1",
                project_id: "p1",
                kind: "symlink",
            },
        )
        .await
        .unwrap();
        views_repo::insert_view_item(
            db.pool(),
            &views_repo::InsertPreparedSourceViewItem {
                id: "item1",
                view_id: "view1",
                inventory_item_id: "frame1",
                view_relative_path: dest_file.to_str().unwrap(),
                materialization: "symlink",
            },
        )
        .await
        .unwrap();

        let resp = verify_source_view(db.pool(), "view1").await.unwrap();
        assert!(resp.clean, "expected clean, got broken_items: {:?}", resp.broken_items);
        assert!(resp.broken_items.is_empty());
    }

    #[tokio::test]
    async fn moved_source_is_reported_without_mutation() {
        let db = setup().await;
        let dir = tempfile::tempdir().unwrap();
        let source_dir = dir.path().join("source");
        let dest_dir = dir.path().join("dest");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&dest_dir).unwrap();
        let source_file = source_dir.join("light1.fits");
        std::fs::write(&source_file, b"x").unwrap();
        let dest_file = dest_dir.join("light1.fits");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&source_file, &dest_file).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&source_file, &dest_file).unwrap();

        insert_project(&db, "p1", dir.path().join("proj").to_str().unwrap()).await;
        insert_root(&db, "root1", source_dir.to_str().unwrap()).await;
        insert_file_record(&db, "frame1", "root1", "light1.fits").await;

        views_repo::insert_view(
            db.pool(),
            &views_repo::InsertPreparedSourceView {
                id: "view2",
                project_id: "p1",
                kind: "symlink",
            },
        )
        .await
        .unwrap();
        views_repo::insert_view_item(
            db.pool(),
            &views_repo::InsertPreparedSourceViewItem {
                id: "item2",
                view_id: "view2",
                inventory_item_id: "frame1",
                view_relative_path: dest_file.to_str().unwrap(),
                materialization: "symlink",
            },
        )
        .await
        .unwrap();

        // Source removed outside the app (dangling symlink) — no mutation
        // performed by verify itself; the fixture-side removal simulates the
        // "moved/removed source" scenario (US4 AS2).
        std::fs::remove_file(&source_file).unwrap();
        let before_dest_exists = dest_file.symlink_metadata().is_ok();

        let resp = verify_source_view(db.pool(), "view2").await.unwrap();
        assert!(!resp.clean);
        assert_eq!(resp.broken_items.len(), 1);
        assert_eq!(resp.broken_items[0].inventory_item_id, "frame1");
        assert_eq!(resp.broken_items[0].state, BrokenItemState::UnresolvedLink);

        // No filesystem mutation: the dangling symlink itself is untouched.
        assert_eq!(dest_file.symlink_metadata().is_ok(), before_dest_exists);
    }

    #[tokio::test]
    async fn removed_file_record_row_is_reported_as_moved() {
        let db = setup().await;
        let dir = tempfile::tempdir().unwrap();
        let dest_dir = dir.path().join("dest");
        std::fs::create_dir_all(&dest_dir).unwrap();
        let dest_file = dest_dir.join("gone.fits");
        std::fs::write(&dest_file, b"x").unwrap();

        insert_project(&db, "p1", dir.path().join("proj").to_str().unwrap()).await;

        views_repo::insert_view(
            db.pool(),
            &views_repo::InsertPreparedSourceView {
                id: "view3",
                project_id: "p1",
                kind: "hardlink",
            },
        )
        .await
        .unwrap();
        views_repo::insert_view_item(
            db.pool(),
            &views_repo::InsertPreparedSourceViewItem {
                id: "item3",
                view_id: "view3",
                inventory_item_id: "no-such-frame",
                view_relative_path: dest_file.to_str().unwrap(),
                materialization: "hardlink",
            },
        )
        .await
        .unwrap();

        let resp = verify_source_view(db.pool(), "view3").await.unwrap();
        assert!(!resp.clean);
        assert_eq!(resp.broken_items[0].state, BrokenItemState::Moved);
    }

    // ── #746: copy-kind content-drift detection ──────────────────────────────

    #[tokio::test]
    async fn copy_kind_identical_content_verifies_clean() {
        let db = setup().await;
        let dir = tempfile::tempdir().unwrap();
        let source_dir = dir.path().join("source");
        let dest_dir = dir.path().join("dest");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&dest_dir).unwrap();
        let source_file = source_dir.join("light1.fits");
        std::fs::write(&source_file, b"identical bytes").unwrap();
        let dest_file = dest_dir.join("light1.fits");
        std::fs::write(&dest_file, b"identical bytes").unwrap();

        insert_project(&db, "p1", dir.path().join("proj").to_str().unwrap()).await;
        insert_root(&db, "root1", source_dir.to_str().unwrap()).await;
        insert_file_record(&db, "frame1", "root1", "light1.fits").await;

        views_repo::insert_view(
            db.pool(),
            &views_repo::InsertPreparedSourceView {
                id: "view-copy-1",
                project_id: "p1",
                kind: "copy",
            },
        )
        .await
        .unwrap();
        views_repo::insert_view_item(
            db.pool(),
            &views_repo::InsertPreparedSourceViewItem {
                id: "item-copy-1",
                view_id: "view-copy-1",
                inventory_item_id: "frame1",
                view_relative_path: dest_file.to_str().unwrap(),
                materialization: "copy",
            },
        )
        .await
        .unwrap();

        let resp = verify_source_view(db.pool(), "view-copy-1").await.unwrap();
        assert!(resp.clean, "expected clean, got broken_items: {:?}", resp.broken_items);
    }

    #[tokio::test]
    async fn copy_kind_diverged_content_is_reported() {
        let db = setup().await;
        let dir = tempfile::tempdir().unwrap();
        let source_dir = dir.path().join("source");
        let dest_dir = dir.path().join("dest");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&dest_dir).unwrap();
        let source_file = source_dir.join("light1.fits");
        std::fs::write(&source_file, b"original bytes").unwrap();
        let dest_file = dest_dir.join("light1.fits");
        // Destination copy edited independently after generation — the drift
        // FR-009 requires detecting (previously always reported clean).
        std::fs::write(&dest_file, b"edited elsewhere").unwrap();

        insert_project(&db, "p1", dir.path().join("proj").to_str().unwrap()).await;
        insert_root(&db, "root1", source_dir.to_str().unwrap()).await;
        insert_file_record(&db, "frame1", "root1", "light1.fits").await;

        views_repo::insert_view(
            db.pool(),
            &views_repo::InsertPreparedSourceView {
                id: "view-copy-2",
                project_id: "p1",
                kind: "copy",
            },
        )
        .await
        .unwrap();
        views_repo::insert_view_item(
            db.pool(),
            &views_repo::InsertPreparedSourceViewItem {
                id: "item-copy-2",
                view_id: "view-copy-2",
                inventory_item_id: "frame1",
                view_relative_path: dest_file.to_str().unwrap(),
                materialization: "copy",
            },
        )
        .await
        .unwrap();

        let resp = verify_source_view(db.pool(), "view-copy-2").await.unwrap();
        assert!(!resp.clean);
        assert_eq!(resp.broken_items[0].state, BrokenItemState::HashDiverged);
    }

    #[tokio::test]
    async fn hardlink_kind_never_probed_for_hash_divergence() {
        // A hardlink shares the source's inode: its bytes are the source's
        // bytes by construction, so no content probe should ever run (and
        // none is needed — verifies the existing clean-hardlink contract is
        // untouched by the new copy-kind probe).
        let db = setup().await;
        let dir = tempfile::tempdir().unwrap();
        let source_dir = dir.path().join("source");
        let dest_dir = dir.path().join("dest");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&dest_dir).unwrap();
        let source_file = source_dir.join("light1.fits");
        std::fs::write(&source_file, b"x").unwrap();
        let dest_file = dest_dir.join("light1.fits");
        std::fs::hard_link(&source_file, &dest_file).unwrap();

        insert_project(&db, "p1", dir.path().join("proj").to_str().unwrap()).await;
        insert_root(&db, "root1", source_dir.to_str().unwrap()).await;
        insert_file_record(&db, "frame1", "root1", "light1.fits").await;

        views_repo::insert_view(
            db.pool(),
            &views_repo::InsertPreparedSourceView {
                id: "view-hl-1",
                project_id: "p1",
                kind: "hardlink",
            },
        )
        .await
        .unwrap();
        views_repo::insert_view_item(
            db.pool(),
            &views_repo::InsertPreparedSourceViewItem {
                id: "item-hl-1",
                view_id: "view-hl-1",
                inventory_item_id: "frame1",
                view_relative_path: dest_file.to_str().unwrap(),
                materialization: "hardlink",
            },
        )
        .await
        .unwrap();

        let resp = verify_source_view(db.pool(), "view-hl-1").await.unwrap();
        assert!(resp.clean, "expected clean, got broken_items: {:?}", resp.broken_items);
    }

    // ── sweep_view_staleness (T014/T015) ─────────────────────────────────────

    #[tokio::test]
    async fn sweep_marks_clean_view_current() {
        let db = setup().await;
        let dir = tempfile::tempdir().unwrap();
        let source_dir = dir.path().join("source");
        let dest_dir = dir.path().join("dest");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&dest_dir).unwrap();
        let source_file = source_dir.join("light1.fits");
        std::fs::write(&source_file, b"x").unwrap();
        let dest_file = dest_dir.join("light1.fits");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&source_file, &dest_file).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&source_file, &dest_file).unwrap();

        insert_project(&db, "p1", dir.path().join("proj").to_str().unwrap()).await;
        insert_root(&db, "root1", source_dir.to_str().unwrap()).await;
        insert_file_record(&db, "frame1", "root1", "light1.fits").await;

        views_repo::insert_view(
            db.pool(),
            &views_repo::InsertPreparedSourceView { id: "sv1", project_id: "p1", kind: "symlink" },
        )
        .await
        .unwrap();
        views_repo::insert_view_item(
            db.pool(),
            &views_repo::InsertPreparedSourceViewItem {
                id: "svi1",
                view_id: "sv1",
                inventory_item_id: "frame1",
                view_relative_path: dest_file.to_str().unwrap(),
                materialization: "symlink",
            },
        )
        .await
        .unwrap();

        sweep_view_staleness(db.pool(), "sv1").await.unwrap();

        let view = views_repo::get_view(db.pool(), "sv1").await.unwrap();
        assert_eq!(view.state, "current");
        let items = views_repo::list_view_items(db.pool(), "sv1").await.unwrap();
        assert_eq!(items[0].last_observed_state, "present");
    }

    #[tokio::test]
    async fn sweep_marks_partially_broken_view_stale() {
        let db = setup().await;
        let dir = tempfile::tempdir().unwrap();
        let source_dir = dir.path().join("source");
        let dest_dir = dir.path().join("dest");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&dest_dir).unwrap();

        let ok_source = source_dir.join("ok.fits");
        std::fs::write(&ok_source, b"x").unwrap();
        let ok_dest = dest_dir.join("ok.fits");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&ok_source, &ok_dest).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&ok_source, &ok_dest).unwrap();

        // Second item's destination link was never created — reports `missing`.
        let missing_dest = dest_dir.join("missing.fits");

        insert_project(&db, "p1", dir.path().join("proj").to_str().unwrap()).await;
        insert_root(&db, "root1", source_dir.to_str().unwrap()).await;
        insert_file_record(&db, "frame-ok", "root1", "ok.fits").await;
        insert_file_record(&db, "frame-missing", "root1", "missing.fits").await;

        views_repo::insert_view(
            db.pool(),
            &views_repo::InsertPreparedSourceView { id: "sv2", project_id: "p1", kind: "symlink" },
        )
        .await
        .unwrap();
        views_repo::insert_view_item(
            db.pool(),
            &views_repo::InsertPreparedSourceViewItem {
                id: "svi-ok",
                view_id: "sv2",
                inventory_item_id: "frame-ok",
                view_relative_path: ok_dest.to_str().unwrap(),
                materialization: "symlink",
            },
        )
        .await
        .unwrap();
        views_repo::insert_view_item(
            db.pool(),
            &views_repo::InsertPreparedSourceViewItem {
                id: "svi-missing",
                view_id: "sv2",
                inventory_item_id: "frame-missing",
                view_relative_path: missing_dest.to_str().unwrap(),
                materialization: "symlink",
            },
        )
        .await
        .unwrap();

        sweep_view_staleness(db.pool(), "sv2").await.unwrap();

        let view = views_repo::get_view(db.pool(), "sv2").await.unwrap();
        assert_eq!(view.state, "stale");
        let items = views_repo::list_view_items(db.pool(), "sv2").await.unwrap();
        let ok_item = items.iter().find(|i| i.id == "svi-ok").unwrap();
        let missing_item = items.iter().find(|i| i.id == "svi-missing").unwrap();
        assert_eq!(ok_item.last_observed_state, "present");
        assert_eq!(missing_item.last_observed_state, "missing");
    }

    #[tokio::test]
    async fn sweep_marks_fully_broken_view_missing() {
        let db = setup().await;
        let dir = tempfile::tempdir().unwrap();
        let source_dir = dir.path().join("source");
        let dest_dir = dir.path().join("dest");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&dest_dir).unwrap();
        // Resolvable source, but its destination link was never (or no
        // longer) created — `missing`, distinct from a `moved`/gone source.
        std::fs::write(source_dir.join("gone.fits"), b"x").unwrap();
        let gone_dest = dest_dir.join("gone.fits");

        insert_project(&db, "p1", dir.path().join("proj").to_str().unwrap()).await;
        insert_root(&db, "root1", source_dir.to_str().unwrap()).await;
        insert_file_record(&db, "frame-gone", "root1", "gone.fits").await;

        views_repo::insert_view(
            db.pool(),
            &views_repo::InsertPreparedSourceView { id: "sv3", project_id: "p1", kind: "symlink" },
        )
        .await
        .unwrap();
        views_repo::insert_view_item(
            db.pool(),
            &views_repo::InsertPreparedSourceViewItem {
                id: "svi-gone",
                view_id: "sv3",
                inventory_item_id: "frame-gone",
                view_relative_path: gone_dest.to_str().unwrap(),
                materialization: "symlink",
            },
        )
        .await
        .unwrap();

        sweep_view_staleness(db.pool(), "sv3").await.unwrap();

        let view = views_repo::get_view(db.pool(), "sv3").await.unwrap();
        assert_eq!(view.state, "missing");
        let items = views_repo::list_view_items(db.pool(), "sv3").await.unwrap();
        assert_eq!(items[0].last_observed_state, "missing");
    }

    #[tokio::test]
    async fn sweep_skips_removed_view() {
        let db = setup().await;
        insert_project(&db, "p1", "proj/p1").await;
        views_repo::insert_view(
            db.pool(),
            &views_repo::InsertPreparedSourceView { id: "sv4", project_id: "p1", kind: "symlink" },
        )
        .await
        .unwrap();
        views_repo::mark_view_removed(db.pool(), "sv4").await.unwrap();

        sweep_view_staleness(db.pool(), "sv4").await.unwrap();

        let view = views_repo::get_view(db.pool(), "sv4").await.unwrap();
        assert_eq!(view.state, "removed");
    }
}
