// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository methods for onboarding state (spec 056, T003).
//!
//! Operates on `onboarding_state` (per-item rows) and `onboarding_flags`
//! (singleton) from migration 0069. The item registry (page/topic/
//! prerequisite/seed_query) lives in `app_core::onboarding` — this module
//! only persists and reads item_id-keyed rows and the section flags.
//!
//! ## The single guarded-upsert primitive
//!
//! [`upsert_if_unsettled`] is the ONLY write path for item state and serves
//! seed, restore, live-event ticks, and manual check-off/dismiss alike: it
//! inserts a missing row, or overwrites an existing row only while its
//! current state is `unchecked`/`auto_checked` (data-model.md "State
//! transitions" — settled states are terminal). This is what makes seed and
//! restore share one derivation routine (FR-014/PQ-001: the caller computes
//! a target state per item and calls this same primitive either way) and
//! what makes live-event ticks and manual actions idempotent/non-downgrading
//! without a read-then-write race.

use domain_core::ids::Timestamp;
use sqlx::SqlitePool;

use persistence_core::DbResult;

// ── Row types ─────────────────────────────────────────────────────────────────

/// Raw persisted row from `onboarding_state`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OnboardingStateRow {
    pub item_id: String,
    /// `unchecked` | `auto_checked` | `manually_checked` | `dismissed`.
    pub state: String,
    pub at: String,
    /// `seed` | `event` | `user`.
    pub source: String,
}

/// Raw persisted row from `onboarding_flags` (defaults when no row exists
/// yet — the singleton is created lazily on first write).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OnboardingFlagsRow {
    pub orientation_done_at: Option<String>,
    pub section_hidden_at: Option<String>,
    pub sidebar_collapsed: bool,
}

/// Outcome of a guarded upsert — whether it actually changed the row (versus
/// being a no-op against an already-settled item).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpsertOutcome {
    Written,
    SkippedSettled,
}

// ── onboarding_state ─────────────────────────────────────────────────────────

/// Load every persisted item row.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn load_state(pool: &SqlitePool) -> DbResult<Vec<OnboardingStateRow>> {
    let rows: Vec<(String, String, String, String)> =
        sqlx::query_as("SELECT item_id, state, at, source FROM onboarding_state")
            .fetch_all(pool)
            .await?;

    Ok(rows
        .into_iter()
        .map(|(item_id, state, at, source)| OnboardingStateRow { item_id, state, at, source })
        .collect())
}

/// Load a single item row, or `None` if it has not been seeded yet.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn load_item(pool: &SqlitePool, item_id: &str) -> DbResult<Option<OnboardingStateRow>> {
    let row: Option<(String, String, String, String)> =
        sqlx::query_as("SELECT item_id, state, at, source FROM onboarding_state WHERE item_id = ?")
            .bind(item_id)
            .fetch_optional(pool)
            .await?;

    Ok(row.map(|(item_id, state, at, source)| OnboardingStateRow { item_id, state, at, source }))
}

/// Insert a missing item row, or overwrite an existing one only while its
/// current state is `unchecked`/`auto_checked`. Settled rows
/// (`manually_checked`/`dismissed`) are left untouched (data-model.md —
/// settled states are terminal).
///
/// This single primitive is used for: explicit restore re-derivation
/// (source=`seed`), live-event ticks (source=`event`), and manual
/// check-off/dismiss (source=`user`) — it is deliberately NOT used for
/// lazy first-activation seeding ([`insert_if_missing`]), which must never
/// re-touch an existing `unchecked`/`auto_checked` row outside of an
/// explicit restore (that would silently auto-tick an item on a read path,
/// bypassing the bus subscriber — FR-021 backend-authoritative-via-
/// subscriber-only). Callers distinguish "did this write actually change
/// anything" via the returned [`UpsertOutcome`] (needed for the FR-031
/// settle path, which only fires on a real settling transition, not a
/// repeat/idempotent call).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn upsert_if_unsettled(
    pool: &SqlitePool,
    item_id: &str,
    state: &str,
    source: &str,
) -> DbResult<UpsertOutcome> {
    let now = Timestamp::now_iso();
    let result = sqlx::query(
        "INSERT INTO onboarding_state (item_id, state, at, source) VALUES (?, ?, ?, ?) \
         ON CONFLICT(item_id) DO UPDATE SET \
             state  = excluded.state, \
             at     = excluded.at, \
             source = excluded.source \
         WHERE onboarding_state.state IN ('unchecked', 'auto_checked')",
    )
    .bind(item_id)
    .bind(state)
    .bind(&now)
    .bind(source)
    .execute(pool)
    .await?;

    Ok(if result.rows_affected() > 0 {
        UpsertOutcome::Written
    } else {
        UpsertOutcome::SkippedSettled
    })
}

/// Force an item back to `unchecked`, whatever its current state.
///
/// The deliberate escape hatch from [`upsert_if_unsettled`]'s terminality
/// rule, and the ONLY write that may clear a settled row. Terminality exists
/// so that seed re-derivation, live ticks and repeat calls can never *silently*
/// downgrade a user's decision; an explicit un-check is the opposite — the user
/// asking for exactly that, once, by hand.
///
/// Applies to automatic rows too. Un-checking one is not permanent and is not
/// meant to be: the item re-ticks when the underlying action happens again
/// (a fresh bus event), and an explicit Settings restore re-derives it from
/// real database state, because re-deriving is precisely what restore is for.
/// The checklist therefore never ends up permanently contradicting the library.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn force_unchecked(pool: &SqlitePool, item_id: &str) -> DbResult<UpsertOutcome> {
    let now = Timestamp::now_iso();
    let result = sqlx::query(
        "INSERT INTO onboarding_state (item_id, state, at, source) VALUES (?, 'unchecked', ?, 'user') \
         ON CONFLICT(item_id) DO UPDATE SET \
             state  = 'unchecked', \
             at     = excluded.at, \
             source = 'user' \
         WHERE onboarding_state.state <> 'unchecked'",
    )
    .bind(item_id)
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(if result.rows_affected() > 0 {
        UpsertOutcome::Written
    } else {
        // Already unchecked — a genuine no-op, not a refusal.
        UpsertOutcome::SkippedSettled
    })
}

/// Insert a row only if `item_id` has none yet; a no-op otherwise. Used
/// exclusively by lazy first-activation seeding (`onboarding.state.get`'s
/// first-ever call): every already-present row — regardless of its state —
/// is left completely untouched, so this can run on every read without ever
/// silently re-deriving or auto-ticking an existing item.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn insert_if_missing(
    pool: &SqlitePool,
    item_id: &str,
    state: &str,
    source: &str,
) -> DbResult<()> {
    let now = Timestamp::now_iso();
    sqlx::query(
        "INSERT INTO onboarding_state (item_id, state, at, source) VALUES (?, ?, ?, ?) \
         ON CONFLICT(item_id) DO NOTHING",
    )
    .bind(item_id)
    .bind(state)
    .bind(&now)
    .bind(source)
    .execute(pool)
    .await?;
    Ok(())
}

/// True when no item row is `unchecked` (every registered item is
/// `auto_checked`/`manually_checked`/`dismissed`, or unregistered rows don't
/// exist yet). Used by the FR-031 settle path: the caller has already
/// confirmed all registry items have a row before relying on this.
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn all_items_settled(pool: &SqlitePool) -> DbResult<bool> {
    let unchecked_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM onboarding_state WHERE state = 'unchecked'")
            .fetch_one(pool)
            .await?;
    Ok(unchecked_count == 0)
}

// ── Milestone queries (seed/restore derivation, FR-014) ──────────────────────
//
// Read-only checks against real domain tables. These back the item
// registry's `seed_query` in `app_core::onboarding` (research R4's verified
// auto-tick inventory) — kept here, not in app-core, per the db-boundary
// rule (all raw SQL lives in this crate).

/// At least one inbox item is currently confirmed (`inventory.confirmed`
/// milestone; `inbox.confirm_first`).
///
/// Checks `inbox_items.state` rather than `inbox_plan_links` (whose rows are
/// deleted once a plan resolves — apply/discard/fail/cancel — so it cannot
/// answer "was anything ever confirmed"). `plan_open`/`resolved` are the
/// post-confirm states; a discarded plan reverts its item to `classified`
/// (`plan_listener.rs::handle_plan_discarded`), which correctly un-derives
/// the milestone if it was the only confirm — restore reflects *current*
/// recorded state, not historical audit trail (FR-014).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn has_confirmed_inbox_item(pool: &SqlitePool) -> DbResult<bool> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM inbox_items WHERE state IN ('plan_open', 'resolved'))",
    )
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// At least one plan has finished applying, fully or partially
/// (`plan.applying.completed` milestone; `inbox.apply_first_plan`).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn has_applied_plan(pool: &SqlitePool) -> DbResult<bool> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM plans WHERE state IN ('applied', 'partially_applied'))",
    )
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// At least one project exists (`project.created` milestone;
/// `projects.create_first`).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn has_project(pool: &SqlitePool) -> DbResult<bool> {
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM projects)").fetch_one(pool).await?;
    Ok(exists)
}

/// At least one processing tool has been successfully spawned
/// (`tool.launch` milestone, `outcome == "spawned"`; `projects.launch_tool`).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn has_spawned_tool_launch(pool: &SqlitePool) -> DbResult<bool> {
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tool_launches WHERE outcome = 'spawned')")
            .fetch_one(pool)
            .await?;
    Ok(exists)
}

/// At least one FITS `OBJECT` value has resolved to a canonical target
/// (`target.resolved` milestone; `targets.resolve_first`).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn has_resolved_target(pool: &SqlitePool) -> DbResult<bool> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM ingest_resolution WHERE state = 'resolved')",
    )
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

// ── onboarding_flags ──────────────────────────────────────────────────────────

/// Load the section flags, defaulting to an all-unset row when the singleton
/// has not been written yet (no row = orientation not done, section not
/// hidden, sidebar expanded).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn load_flags(pool: &SqlitePool) -> DbResult<OnboardingFlagsRow> {
    let row: Option<(Option<String>, Option<String>, i64)> = sqlx::query_as(
        "SELECT orientation_done_at, section_hidden_at, sidebar_collapsed \
         FROM onboarding_flags WHERE singleton_id = 1",
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map_or_else(
        OnboardingFlagsRow::default,
        |(orientation_done_at, section_hidden_at, sidebar_collapsed)| OnboardingFlagsRow {
            orientation_done_at,
            section_hidden_at,
            sidebar_collapsed: sidebar_collapsed != 0,
        },
    ))
}

/// Overwrite the full flags row (used after every flag transition — the
/// singleton upsert pattern).
///
/// # Errors
///
/// Returns `persistence_core::DbError::Database` on query failure.
pub async fn upsert_flags(pool: &SqlitePool, row: &OnboardingFlagsRow) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO onboarding_flags (singleton_id, orientation_done_at, section_hidden_at, sidebar_collapsed) \
         VALUES (1, ?, ?, ?) \
         ON CONFLICT(singleton_id) DO UPDATE SET \
             orientation_done_at = excluded.orientation_done_at, \
             section_hidden_at   = excluded.section_hidden_at, \
             sidebar_collapsed   = excluded.sidebar_collapsed",
    )
    .bind(&row.orientation_done_at)
    .bind(&row.section_hidden_at)
    .bind(i64::from(row.sidebar_collapsed))
    .execute(pool)
    .await?;

    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_core::Database;

    async fn setup() -> SqlitePool {
        let db = Database::in_memory().await.expect("in-memory DB");
        db.migrate().await.expect("migrations");
        db.pool().clone()
    }

    // ── onboarding_state ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn load_state_empty_when_no_rows() {
        let pool = setup().await;
        assert!(load_state(&pool).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn upsert_inserts_missing_row() {
        let pool = setup().await;
        let outcome =
            upsert_if_unsettled(&pool, "inbox.confirm_first", "unchecked", "seed").await.unwrap();
        assert_eq!(outcome, UpsertOutcome::Written);

        let row = load_item(&pool, "inbox.confirm_first").await.unwrap().unwrap();
        assert_eq!(row.state, "unchecked");
        assert_eq!(row.source, "seed");
    }

    #[tokio::test]
    async fn insert_if_missing_creates_absent_row() {
        let pool = setup().await;
        insert_if_missing(&pool, "targets.resolve_first", "auto_checked", "seed").await.unwrap();
        let row = load_item(&pool, "targets.resolve_first").await.unwrap().unwrap();
        assert_eq!(row.state, "auto_checked");
    }

    #[tokio::test]
    async fn insert_if_missing_never_touches_existing_row() {
        let pool = setup().await;
        upsert_if_unsettled(&pool, "targets.resolve_first", "unchecked", "seed").await.unwrap();
        // A stale re-derivation attempt (e.g. a repeat lazy-seed call) must
        // not flip this to auto_checked even though the milestone is met.
        insert_if_missing(&pool, "targets.resolve_first", "auto_checked", "seed").await.unwrap();
        assert_eq!(
            load_item(&pool, "targets.resolve_first").await.unwrap().unwrap().state,
            "unchecked"
        );
    }

    #[tokio::test]
    async fn upsert_overwrites_unchecked_row() {
        let pool = setup().await;
        upsert_if_unsettled(&pool, "inbox.confirm_first", "unchecked", "seed").await.unwrap();
        let outcome = upsert_if_unsettled(&pool, "inbox.confirm_first", "auto_checked", "event")
            .await
            .unwrap();
        assert_eq!(outcome, UpsertOutcome::Written);

        let row = load_item(&pool, "inbox.confirm_first").await.unwrap().unwrap();
        assert_eq!(row.state, "auto_checked");
        assert_eq!(row.source, "event");
    }

    #[tokio::test]
    async fn upsert_overwrites_auto_checked_row() {
        let pool = setup().await;
        upsert_if_unsettled(&pool, "targets.resolve_first", "auto_checked", "event").await.unwrap();
        // Restore re-derives auto_checked items too (e.g. re-affirming with a
        // fresh timestamp/source=seed).
        let outcome = upsert_if_unsettled(&pool, "targets.resolve_first", "auto_checked", "seed")
            .await
            .unwrap();
        assert_eq!(outcome, UpsertOutcome::Written);
        assert_eq!(
            load_item(&pool, "targets.resolve_first").await.unwrap().unwrap().source,
            "seed"
        );
    }

    #[tokio::test]
    async fn upsert_never_downgrades_manually_checked_row() {
        let pool = setup().await;
        upsert_if_unsettled(&pool, "sessions.review_first", "manually_checked", "user")
            .await
            .unwrap();

        // A live event trying to tick it, and a restore trying to re-derive
        // it to unchecked, both must be no-ops.
        let event_outcome =
            upsert_if_unsettled(&pool, "sessions.review_first", "auto_checked", "event")
                .await
                .unwrap();
        let restore_outcome =
            upsert_if_unsettled(&pool, "sessions.review_first", "unchecked", "seed").await.unwrap();

        assert_eq!(event_outcome, UpsertOutcome::SkippedSettled);
        assert_eq!(restore_outcome, UpsertOutcome::SkippedSettled);

        let row = load_item(&pool, "sessions.review_first").await.unwrap().unwrap();
        assert_eq!(row.state, "manually_checked");
        assert_eq!(row.source, "user");
    }

    #[tokio::test]
    async fn upsert_never_downgrades_dismissed_row() {
        let pool = setup().await;
        upsert_if_unsettled(&pool, "calibration.review_masters", "dismissed", "user")
            .await
            .unwrap();
        let outcome =
            upsert_if_unsettled(&pool, "calibration.review_masters", "auto_checked", "event")
                .await
                .unwrap();
        assert_eq!(outcome, UpsertOutcome::SkippedSettled);
        assert_eq!(
            load_item(&pool, "calibration.review_masters").await.unwrap().unwrap().state,
            "dismissed"
        );
    }

    #[tokio::test]
    async fn all_items_settled_true_when_no_unchecked_rows() {
        let pool = setup().await;
        assert!(all_items_settled(&pool).await.unwrap(), "empty table has no unchecked rows");

        upsert_if_unsettled(&pool, "inbox.confirm_first", "auto_checked", "event").await.unwrap();
        assert!(all_items_settled(&pool).await.unwrap());

        upsert_if_unsettled(&pool, "sessions.review_first", "unchecked", "seed").await.unwrap();
        assert!(!all_items_settled(&pool).await.unwrap());
    }

    /// The escape hatch clears rows `upsert_if_unsettled` refuses to touch.
    /// Both settled states, because the point is undoing a user's own click.
    #[tokio::test]
    async fn force_unchecked_clears_settled_rows() {
        let pool = setup().await;

        for settled in ["manually_checked", "dismissed"] {
            upsert_if_unsettled(&pool, "sessions.review_first", settled, "user").await.unwrap();
            assert_eq!(
                load_item(&pool, "sessions.review_first").await.unwrap().unwrap().state,
                settled
            );

            let outcome = force_unchecked(&pool, "sessions.review_first").await.unwrap();
            assert_eq!(outcome, UpsertOutcome::Written, "{settled} must be clearable");

            let row = load_item(&pool, "sessions.review_first").await.unwrap().unwrap();
            assert_eq!(row.state, "unchecked");
            assert_eq!(row.source, "user", "an un-check is a user action, not a re-derivation");
        }
    }

    /// Automatic rows too — un-checking one is not a lie about the library,
    /// because the next real event or an explicit restore re-derives it.
    #[tokio::test]
    async fn force_unchecked_clears_auto_checked_rows() {
        let pool = setup().await;
        upsert_if_unsettled(&pool, "inbox.confirm_first", "auto_checked", "event").await.unwrap();

        assert_eq!(
            force_unchecked(&pool, "inbox.confirm_first").await.unwrap(),
            UpsertOutcome::Written
        );
        assert_eq!(
            load_item(&pool, "inbox.confirm_first").await.unwrap().unwrap().state,
            "unchecked"
        );

        // …and the row is writable again afterwards, so a later real event
        // re-ticks it exactly as it would have the first time.
        upsert_if_unsettled(&pool, "inbox.confirm_first", "auto_checked", "event").await.unwrap();
        assert_eq!(
            load_item(&pool, "inbox.confirm_first").await.unwrap().unwrap().state,
            "auto_checked"
        );
    }

    /// Un-checking an already-unchecked row changes nothing, so repeat calls
    /// cannot fabricate a state transition for the settle path to react to.
    #[tokio::test]
    async fn force_unchecked_is_a_no_op_when_already_unchecked() {
        let pool = setup().await;
        upsert_if_unsettled(&pool, "sessions.review_first", "unchecked", "seed").await.unwrap();

        assert_eq!(
            force_unchecked(&pool, "sessions.review_first").await.unwrap(),
            UpsertOutcome::SkippedSettled
        );
    }

    // ── Milestone queries ────────────────────────────────────────────────────

    #[tokio::test]
    async fn has_confirmed_inbox_item_false_until_confirmed() {
        let pool = setup().await;
        assert!(!has_confirmed_inbox_item(&pool).await.unwrap());

        sqlx::query(
            "INSERT INTO inbox_items \
                (id, root_id, relative_path, file_count, discovered_at, last_scanned_at, \
                 state, lane) \
             VALUES ('item-1', 'root-1', 'a.fits', 1, '2026-07-18T00:00:00Z', \
                     '2026-07-18T00:00:00Z', 'plan_open', 'fits')",
        )
        .execute(&pool)
        .await
        .unwrap();

        assert!(has_confirmed_inbox_item(&pool).await.unwrap());
    }

    #[tokio::test]
    async fn has_confirmed_inbox_item_false_after_discard_reverts_only_item() {
        let pool = setup().await;
        sqlx::query(
            "INSERT INTO inbox_items \
                (id, root_id, relative_path, file_count, discovered_at, last_scanned_at, \
                 state, lane) \
             VALUES ('item-1', 'root-1', 'a.fits', 1, '2026-07-18T00:00:00Z', \
                     '2026-07-18T00:00:00Z', 'classified', 'fits')",
        )
        .execute(&pool)
        .await
        .unwrap();

        assert!(
            !has_confirmed_inbox_item(&pool).await.unwrap(),
            "classified-only is not confirmed"
        );
    }

    #[tokio::test]
    async fn has_applied_plan_true_only_for_applied_states() {
        let pool = setup().await;
        assert!(!has_applied_plan(&pool).await.unwrap());

        sqlx::query(
            "INSERT INTO plans \
                (id, number, title, origin, state, plan_type, destructive_destination, \
                 items_total, items_applied, items_failed, items_skipped, items_cancelled, \
                 items_pending, total_bytes_required, created_at) \
             VALUES ('plan-1', 1, 'Test', 'cleanup', 'applied', 'cleanup', 'archive', \
                     0, 0, 0, 0, 0, 0, 0, '2026-07-18T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();

        assert!(has_applied_plan(&pool).await.unwrap());
    }

    #[tokio::test]
    async fn has_project_true_once_any_project_exists() {
        let pool = setup().await;
        assert!(!has_project(&pool).await.unwrap());

        sqlx::query(
            "INSERT INTO projects (id, name, tool, lifecycle, path, channel_drift, is_mosaic, created_at, updated_at) \
             VALUES ('proj-1', 'Proj 1', 'PixInsight', 'setup_incomplete', 'proj-1', 0, 0, \
                     '2026-07-18T00:00:00Z', '2026-07-18T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();

        assert!(has_project(&pool).await.unwrap());
    }

    #[tokio::test]
    async fn has_spawned_tool_launch_ignores_non_spawned_outcomes() {
        let pool = setup().await;
        sqlx::query(
            "INSERT INTO tool_launches (id, project_id, tool_id, launched_at, outcome, audit_id) \
             VALUES ('launch-1', 'proj-1', 'pixinsight', '2026-07-18T00:00:00Z', 'spawn_failed', 'audit-1')",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(!has_spawned_tool_launch(&pool).await.unwrap());

        sqlx::query(
            "INSERT INTO tool_launches (id, project_id, tool_id, launched_at, outcome, audit_id) \
             VALUES ('launch-2', 'proj-1', 'pixinsight', '2026-07-18T00:00:01Z', 'spawned', 'audit-2')",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(has_spawned_tool_launch(&pool).await.unwrap());
    }

    #[tokio::test]
    async fn has_resolved_target_true_only_when_state_resolved() {
        let pool = setup().await;
        sqlx::query(
            "INSERT INTO library_root (id, label, current_path, kind, state, created_at) \
             VALUES ('root-1', 'Root', '/lib', 'local', 'active', '2026-07-18T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO file_record (id, root_id, relative_path, size_bytes, mtime, state, first_seen_at, last_seen_at) \
             VALUES ('file-1', 'root-1', 'a.fits', 100, '2026-07-18T00:00:00Z', 'observed', \
                     '2026-07-18T00:00:00Z', '2026-07-18T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO ingest_resolution (id, image_id, object_raw, state, target_id, attempts) \
             VALUES ('ir-1', 'file-1', 'M 31', 'pending', NULL, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(!has_resolved_target(&pool).await.unwrap());

        sqlx::query("UPDATE ingest_resolution SET state = 'resolved' WHERE id = 'ir-1'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(has_resolved_target(&pool).await.unwrap());
    }

    // ── onboarding_flags ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn load_flags_defaults_when_no_row_exists() {
        let pool = setup().await;
        let flags = load_flags(&pool).await.unwrap();
        assert!(flags.orientation_done_at.is_none());
        assert!(flags.section_hidden_at.is_none());
        assert!(!flags.sidebar_collapsed);
    }

    #[tokio::test]
    async fn upsert_flags_roundtrip() {
        let pool = setup().await;
        let row = OnboardingFlagsRow {
            orientation_done_at: Some("2026-07-18T00:00:00Z".to_owned()),
            section_hidden_at: None,
            sidebar_collapsed: true,
        };
        upsert_flags(&pool, &row).await.unwrap();

        let loaded = load_flags(&pool).await.unwrap();
        assert_eq!(loaded, row);
    }

    #[tokio::test]
    async fn upsert_flags_overwrites_on_conflict() {
        let pool = setup().await;
        upsert_flags(
            &pool,
            &OnboardingFlagsRow {
                orientation_done_at: Some("2026-07-18T00:00:00Z".to_owned()),
                section_hidden_at: Some("2026-07-19T00:00:00Z".to_owned()),
                sidebar_collapsed: true,
            },
        )
        .await
        .unwrap();

        // Restore clears section_hidden_at while leaving the rest alone.
        upsert_flags(
            &pool,
            &OnboardingFlagsRow {
                orientation_done_at: Some("2026-07-18T00:00:00Z".to_owned()),
                section_hidden_at: None,
                sidebar_collapsed: true,
            },
        )
        .await
        .unwrap();

        let loaded = load_flags(&pool).await.unwrap();
        assert!(loaded.section_hidden_at.is_none());
        assert!(loaded.sidebar_collapsed);
    }
}
