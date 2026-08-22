use super::{
    active_runs, caches, first_run_repo, inventory_repo, plans_repo, ActiveRun, ActiveRunGuard,
    CasSnapshot, ContractError, ErrorCode, ErrorSeverity, ExecutorItem, ExecutorItemAction,
    HashMap, PlanPathSet, SqlitePool, Utf8Path, Utf8PathBuf,
};

// ── Overlap check (FR-017, R-Concur-1) ────────────────────────────────────────

/// Serializes the FR-017 overlap check with the registry insert so two
/// concurrent `apply_plan` calls cannot both pass the check and then both
/// register overlapping runs. Sync-only critical section: the lock is never
/// held across an `.await`.
pub(super) static OVERLAP_GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Resolve one claimed relative path the same way the executor resolves item
/// paths (`resolve_item_path`): join against the root when known, then
/// lexically normalize. Unrooted paths normalize as-is — they never falsely
/// prefix-match rooted absolute paths.
pub(super) fn resolve_claimed_path(relative: &str, root: Option<&Utf8PathBuf>) -> Utf8PathBuf {
    use fs_executor::ops::path_gate::lexical_normalize;
    match root {
        Some(r) => lexical_normalize(&r.join(relative)),
        None => lexical_normalize(Utf8Path::new(relative)),
    }
}

/// Compute the plan's claimed (source ∪ destination ∪ archive) path set for
/// the FR-017 overlap check (research R7).
///
/// The destination prefers the destination root when it resolves and falls
/// back to the source root — over-claiming rather than under-claiming, which
/// is the safe direction for a concurrency guard. Absolute archive paths
/// (pre-computed at plan generation) are claimed verbatim.
pub(super) fn compute_plan_path_set(
    item_rows: &[plans_repo::PlanItemRow],
    root_map: &HashMap<String, Utf8PathBuf>,
) -> PlanPathSet {
    use fs_executor::ops::path_gate::lexical_normalize;

    let mut set = PlanPathSet::new();
    for row in item_rows {
        let from_root = row.from_root_id.as_deref().and_then(|rid| root_map.get(rid));
        let to_root = row.to_root_id.as_deref().and_then(|rid| root_map.get(rid)).or(from_root);

        if !row.from_relative_path.is_empty() {
            set.insert(resolve_claimed_path(&row.from_relative_path, from_root));
        }
        if !row.to_relative_path.is_empty() {
            set.insert(resolve_claimed_path(&row.to_relative_path, to_root));
        }
        if let Some(archive) = row.archive_path.as_deref().filter(|a| !a.is_empty()) {
            let p = Utf8Path::new(archive);
            if p.is_absolute() {
                set.insert(lexical_normalize(p));
            } else {
                set.insert(resolve_claimed_path(archive, to_root));
            }
        }
    }
    set
}

/// Check the FR-017 concurrency invariants and, when they hold, register the
/// run in [`ACTIVE_RUNS`] — atomically with respect to other apply calls
/// (guarded by [`OVERLAP_GATE`]).
///
/// Returns the RAII removal guard on success. On failure nothing is
/// registered:
/// - `plan.invalid_state` — this plan already has an active run (same-plan
///   double-apply backstop, T021; the state CAS blocks the common path).
/// - `plan.conflict.overlap` — the plan's path set overlaps an active run's
///   path set at subtree-prefix granularity (FR-017, R-Concur-1).
#[allow(clippy::result_large_err)]
pub(super) fn check_overlap_and_register(
    plan_id: &str,
    run: ActiveRun,
) -> Result<ActiveRunGuard, ContractError> {
    let registry = active_runs();
    let _gate = OVERLAP_GATE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);

    if registry.contains_key(plan_id) {
        return Err(ContractError::new(
            ErrorCode::PlanInvalidState,
            format!("plan {plan_id} already has an active apply run"),
            ErrorSeverity::Blocking,
            false,
        ));
    }

    for entry in registry.iter() {
        if let Some((mine, theirs)) = run.path_set.first_overlap(&entry.value().path_set) {
            return Err(ContractError::new(
                ErrorCode::PlanConflictOverlap,
                format!(
                    "plan {plan_id} path '{mine}' overlaps path '{theirs}' claimed by \
                     active plan {}; wait for that apply to finish",
                    entry.key()
                ),
                ErrorSeverity::Blocking,
                false,
            ));
        }
    }

    registry.insert(plan_id.to_owned(), run);
    Ok(ActiveRunGuard { registry, plan_id: plan_id.to_owned() })
}

// ── Approval token verification (A1) ─────────────────────────────────────────

/// Verify the approval token is present and non-empty.
///
/// v1: the token is the value produced by `approve_plan`
/// (`"tok-<planId>-<uuid>"`). The spec calls for HMAC verification; this
/// is documented as a future upgrade. For v1 we check that the stored
/// `approval_token` on the plan row matches what the caller supplies.
#[allow(clippy::result_large_err)]
pub(super) fn verify_approval_token(
    stored_token: Option<&str>,
    supplied_token: &str,
) -> Result<(), ContractError> {
    match stored_token {
        None => Err(ContractError::new(
            ErrorCode::PlanApprovalStale,
            "no approval token on record; plan must be approved before apply".to_owned(),
            ErrorSeverity::Blocking,
            false,
        )),
        Some(stored) if stored != supplied_token => Err(ContractError::new(
            ErrorCode::PlanApprovalStale,
            "approval token mismatch; plan may have been re-approved or tampered".to_owned(),
            ErrorSeverity::Blocking,
            false,
        )),
        Some(_) => Ok(()),
    }
}

// ── Item → ExecutorItem mapping ───────────────────────────────────────────────

/// Parse the recorded link-materialization kind out of a plan item's
/// free-form `provenance` JSON (`[{"label":"materialization","value":"..."}]`,
/// spec 049 generation/regeneration plan builders). Falls back to `Symlink`
/// (the constitution-preferred default) when absent or unparseable, rather
/// than guessing a destructive kind.
pub(super) fn materialization_from_provenance(
    row: &plans_repo::PlanItemRow,
) -> domain_core::source_view::Materialization {
    row.provenance
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Vec<serde_json::Value>>(raw).ok())
        .and_then(|entries| {
            entries.into_iter().find_map(|entry| {
                if entry.get("label").and_then(serde_json::Value::as_str) == Some("materialization")
                {
                    entry
                        .get("value")
                        .and_then(serde_json::Value::as_str)
                        .and_then(domain_core::source_view::Materialization::from_str_opt)
                } else {
                    None
                }
            })
        })
        .unwrap_or(domain_core::source_view::Materialization::Symlink)
}

/// Convert a `PlanItemRow` into an `ExecutorItem`, resolving the source and
/// destination roots from the provided root-id → absolute-path map (T023a).
///
/// When `root_map` contains the `from_root_id` for this item, `library_root`
/// is set to the absolute path so the path-escape/symlink/staleness gate in
/// the executor fires on real items. When the root cannot be resolved (no
/// `from_root_id` or id absent from the map), `library_root` is `None` and
/// the gate is skipped (legacy/test mode).
///
/// `destination_root` is resolved independently from `to_root_id` so a
/// cross-root move joins `to_relative_path` against the *picked* destination
/// root instead of silently reusing the source root (#765). Precedence:
/// `to_root_id` → `plan_destination_root` (`plans.destination_root`) →
/// `library_root`.
///
/// `plan_destructive_destination` is the *plan-level* `plans.destructive_destination`
/// choice ("archive" | "trash"), not a per-item column: both `cleanup_generator`
/// and `archive_generator` always store `action = "archive"` for a
/// destructive-but-reversible item (the item-level `"trash"` action string is
/// otherwise dead — no generator ever writes it). Without consulting this, a
/// user's review-time "System trash" choice had no effect at apply time —
/// every such item silently archived into `.astro-plan-archive` instead.
pub(super) fn item_row_to_executor_item(
    row: &plans_repo::PlanItemRow,
    root_map: &HashMap<String, Utf8PathBuf>,
    plan_destructive_destination: &str,
    plan_destination_root: Option<&Utf8Path>,
) -> ExecutorItem {
    // DB path columns are stored as `String` (unchanged DB representation,
    // Local-First custody §I). Rust strings are already UTF-8, so building a
    // `Utf8PathBuf` from them is infallible and lossless.
    let action = match row.action.as_str() {
        "move" => ExecutorItemAction::Move,
        // The user's review-time "System trash" choice (plan-level) routes an
        // `action = "archive"` item through OS trash instead of the app
        // archive folder — see this function's doc comment.
        "archive" if plan_destructive_destination == "trash" => {
            ExecutorItemAction::Trash { fallback_archive_destination: None }
        }
        "archive" => {
            // archive_path stores the pre-computed relative archive path.
            let archive_dest = row
                .archive_path
                .as_deref()
                .map_or_else(|| Utf8PathBuf::from(&row.to_relative_path), Utf8PathBuf::from);
            ExecutorItemAction::Archive { archive_destination: archive_dest }
        }
        // T022: map "trash" action to the Trash variant.
        "trash" => ExecutorItemAction::Trash { fallback_archive_destination: None },
        "delete" => ExecutorItemAction::Delete,
        // spec 041: catalogue = record-in-place, no filesystem mutation.
        "catalogue" => ExecutorItemAction::Catalogue,
        // spec 008 scaffolding: create the destination directory for real
        // (previously fell through to NoOp, so applied mkdir plans never
        // created anything on disk).
        "mkdir" => ExecutorItemAction::Mkdir,
        // spec 049: create a real link (or, with explicit copy opt-in, a real
        // copy). Previously fell through to NoOp, so applied source-view
        // generation/regeneration plans never created anything on disk. The
        // recorded materialization kind rides the free-form `provenance` JSON
        // array (`[{"label":"materialization","value":"symlink"}]`, spec 014
        // convention); an unparseable/missing value conservatively falls back
        // to `symlink` (the constitution-preferred default) rather than
        // guessing a destructive kind.
        "link" => ExecutorItemAction::Link { kind: materialization_from_provenance(row) },
        // astro-plan-l3y0: previously fell through to NoOp, so an applied
        // project-create plan never wrote the app-owned project marker file
        // to disk despite the plan reporting the item as applied. The
        // project id rides `linked_entity` (set by `project_setup::create`
        // for every item in the plan, including this one).
        "write_manifest" => ExecutorItemAction::WriteManifest {
            project_id: row.linked_entity.clone().unwrap_or_default(),
        },
        _ => ExecutorItemAction::NoOp,
    };

    // T023a: Resolve library_root from the DB root map.
    // When from_root_id is set and the root exists in the map, the path gate
    // (T018: escape/symlink/staleness) will fire on this item.
    let library_root: Option<Utf8PathBuf> =
        row.from_root_id.as_deref().and_then(|rid| root_map.get(rid)).cloned();

    // #765: destination_root resolves independently from to_root_id, falling
    // back to library_root — NOT reusing from_root_id's resolution outright —
    // so a cross-root move/link/mkdir joins to_relative_path against the
    // destination root the user actually picked.
    // astro-plan-d8cyr: `plans.destination_root` outranks the `library_root`
    // fallback. A source-view item stores an absolute destination with no
    // `to_root_id`, so falling straight back to the SOURCE root left the
    // executor gating the destination against a root it never belonged to.
    let destination_root: Option<Utf8PathBuf> = row
        .to_root_id
        .as_deref()
        .and_then(|rid| root_map.get(rid))
        .cloned()
        .or_else(|| plan_destination_root.map(Utf8Path::to_path_buf))
        .or_else(|| library_root.clone());

    // Paths are stored as relative to the library root.
    let source_path = if row.from_relative_path.is_empty() {
        None
    } else {
        Some(Utf8PathBuf::from(&row.from_relative_path))
    };

    let destination_path = if row.to_relative_path.is_empty() {
        None
    } else {
        Some(Utf8PathBuf::from(&row.to_relative_path))
    };

    let is_protected = row.protection == "protected";

    // T020: `requires_destructive_confirm` is derived from action type,
    // independent of `is_protected`. Replaces the old `confirm_required = is_protected` inversion.
    let requires_destructive_confirm = matches!(row.action.as_str(), "delete" | "trash")
        || row.requires_destructive_confirm.unwrap_or(0) != 0;

    // T023a: `destructive_confirmed` is now a real DB column (migration 0033).
    let destructive_confirmed = row.destructive_confirmed != 0;

    ExecutorItem {
        id: row.id.clone(),
        plan_id: row.plan_id.clone(),
        action,
        source_path,
        destination_path,
        library_root,
        destination_root,
        cas_snapshot: CasSnapshot {
            approved_mtime: row.approved_mtime.clone(),
            approved_size_bytes: row.approved_size_bytes,
        },
        is_protected,
        requires_destructive_confirm,
        destructive_confirmed,
        current_state: row.item_state.clone(),
    }
}

/// Resolve a `root_id` to its absolute path: legacy `library_root` table
/// first, then `registered_sources` (gen-3 source model).
///
/// Read-through `caches::library_root` (F0) wraps only the
/// `registered_sources` fallback, not the legacy `library_root` table
/// lookup: `invalidate_library_root` is only called from `first_run.rs`'s
/// writers of `registered_sources` (register / remap / delete), so caching
/// the legacy-table branch too would go stale on writes this module never
/// sees.
// `pub(crate)`: reused by `crate::plans::send_archive_to_trash` /
// `permanently_delete_archive` (spec 017 US6) to resolve the same
// root_id → absolute-path mapping the apply executor uses (T023a), so an
// archive item's `archive_path` (stored root-relative when `from_root_id`
// is set) can be turned into a real filesystem path.
pub async fn resolve_root_path(pool: &SqlitePool, root_id: &str) -> Option<String> {
    if let Ok(Some(path)) = inventory_repo::get_library_root_path(pool, root_id).await {
        return Some(path);
    }

    let key = root_id.to_owned();
    if let Some(cached) = caches::library_root().get(&key) {
        return Some(cached);
    }

    let loaded = first_run_repo::get_source_path(pool, root_id).await.ok().flatten();
    if let Some(path) = &loaded {
        // GFD-3: Re-check cache before insert — another concurrent caller may
        // have populated it while we were loading. The get_or_insert_with API
        // is sync-only so we guard manually.
        if caches::library_root().get(&key).is_none() {
            caches::library_root().insert(key, path.clone());
        }
    }
    loaded
}
