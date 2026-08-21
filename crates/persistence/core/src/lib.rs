// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Core persistence primitives: connection pool, migrations, error types.
//!
//! Sub-crates (`persistence_inbox`, `persistence_lifecycle`, etc.) depend on
//! this crate for `Database`, `DbResult`, and the `Repository` marker trait.
//! The `persistence_db` facade re-exports everything from here for dependents
//! that have not yet migrated to direct sub-crate dependencies.
#![allow(clippy::doc_markdown)]

use std::str::FromStr;

use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous,
};

pub mod operation_state;
/// Reference pattern for sea-query + sqlx 0.9 binding. Compiled only under `cfg(test)`.
#[cfg(test)]
mod query_builder_example;
pub mod repositories;
mod schema_cache;

#[cfg(any(test, feature = "test-fixture"))]
pub mod test_support;

pub const CRATE_NAME: &str = "persistence_core";

/// How long a writer waits for the SQLite write lock before `SQLITE_BUSY`.
///
/// Matches sqlx's current default; stated here so the value is ours, not a
/// transitively-inherited one (#1231).
pub const BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

pub type DbResult<T> = Result<T, DbError>;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("entity not found: {0}")]
    NotFound(String),
    #[error("compare-and-swap failed: {0}")]
    CasFailed(String),
    #[error("serialisation error: {0}")]
    Serialise(#[from] serde_json::Error),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("not implemented")]
    NotImplemented,
    /// Catalog origin 'user' is not implemented in v1 (A2 — deferred to v1.x).
    /// Returned by catalog install functions when `origin == CatalogOrigin::User`.
    #[error("origin 'user' is not implemented in v1 (A2)")]
    OriginNotImplemented,
}

impl DbError {
    /// Convenience constructor for invalid-state / optimistic-lock failures.
    #[must_use]
    pub fn invalid_state(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }
}

pub trait Repository {
    fn repository_name(&self) -> &'static str;
}

/// Production SQLite connection pool.
///
/// Callers use `Database::connect` for a file-backed store or
/// `Database::in_memory` for ephemeral test fixtures. Both run migrations
/// automatically via [`Database::migrate`].
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Connect to a file-backed SQLite database.
    ///
    /// Durability tier policy (constitution v1.1.0 §V):
    ///
    /// - **WAL** (`journal_mode = WAL`, #830): concurrent readers during a write,
    ///   no reader-writer blocking, and a crash-safe write path on all platforms.
    /// - **`synchronous = NORMAL`** (tier-2): the OS journal is fsynced at each
    ///   checkpoint, not every commit.  Under WAL, a crash between commits risks
    ///   at most losing the last un-checkpointed transaction — the database is
    ///   never corrupted.  This is materially faster than `FULL` on spinning or
    ///   network-backed storage without sacrificing integrity guarantees the app
    ///   depends on.  No writer raises `synchronous`; this pool default is the
    ///   only setting in the repo.  One tier-1 path syncs separately: the
    ///   filesystem-mutation intent commit is followed by a best-effort
    ///   `PRAGMA wal_checkpoint(TRUNCATE)` (see `checkpoint_intent` in
    ///   `persistence_plans::repositories::plan_apply`), whose failure both
    ///   call sites log and ignore.  The other tier-1 records §V names —
    ///   frame-to-session attribution, lifecycle choices, user overrides — have
    ///   no such escalation; whether they require one is open
    ///   (astro-plan-53b8).
    /// - **`foreign_keys = ON`**: enforced explicitly so the constraint is not
    ///   silently absent if a future sqlx version changes its default.
    /// - **`busy_timeout`** (#1231): pins the wait interval so it is ours, not
    ///   a transitively-inherited default.
    ///
    /// # Errors
    ///
    /// Returns `persistence_core::DbError::Database` if the pool cannot connect to the given URL.
    pub async fn connect(connection_string: &str) -> DbResult<Self> {
        let options = SqliteConnectOptions::from_str(connection_string)?
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(BUSY_TIMEOUT);
        let pool = SqlitePoolOptions::new().max_connections(8).connect_with(options).await?;
        Ok(Self { pool })
    }

    /// Convenience constructor for in-memory SQLite (test and CI use).
    ///
    /// Each call generates a unique named in-memory URI
    /// (`file:{uuid}?mode=memory&cache=shared`). The `cache=shared` flag means
    /// every pool connection that opens the same URI sees the same database, so
    /// `migrate()` and subsequent queries all operate on one shared schema.
    /// The unique name isolates each `Database::in_memory()` instance from
    /// every other — tests that run concurrently do not cross-contaminate.
    ///
    /// Using `sqlite::memory:` (no name) instead would give each pool
    /// connection its own private blank database, so queries after `migrate()`
    /// could land on an unmigrated connection and fail with "no such table"
    /// under parallel test load.
    ///
    /// # Errors
    ///
    /// Returns `persistence_core::DbError::Database` if the in-memory pool cannot be created.
    pub async fn in_memory() -> DbResult<Self> {
        let db_name = uuid::Uuid::new_v4().simple();
        let url = format!("sqlite:file:{db_name}?mode=memory&cache=shared");
        Self::connect(&url).await
    }

    /// Return `true` when at least one migration in the embedded set has not yet
    /// been applied to this database.
    ///
    /// Returns `false` for fresh databases (the `_sqlx_migrations` table does
    /// not exist yet) because there is nothing to back up before a first-time
    /// schema creation.  The backup logic in the desktop shell uses this to skip
    /// the VACUUM INTO cost when startup would be a no-op anyway.
    ///
    /// # Panics
    ///
    /// Panics if the number of compiled-in migrations exceeds `i64::MAX`, which
    /// cannot occur in practice.
    ///
    /// # Errors
    ///
    /// Returns `persistence_core::DbError::Database` on an unexpected SQL error other than the
    /// table-not-found case.
    pub async fn has_pending_migrations(&self) -> DbResult<bool> {
        let mut conn = self.pool.acquire().await?;
        // If the migration tracking table has never been created this is a fresh
        // database — no backup needed.
        let table_exists: bool = sqlx::query_scalar(
            "SELECT COUNT(*) > 0 FROM sqlite_master \
             WHERE type = 'table' AND name = '_sqlx_migrations'",
        )
        .fetch_one(&mut *conn)
        .await?;
        if !table_exists {
            return Ok(false);
        }
        let applied_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
            .fetch_one(&mut *conn)
            .await?;
        let total =
            i64::try_from(schema_cache::MIGRATOR.iter().count()).expect("migration count fits i64");
        Ok(applied_count < total)
    }

    /// Copy this database to `dest` using SQLite's `VACUUM INTO` statement.
    ///
    /// `VACUUM INTO` writes a consistent, compacted snapshot of the live
    /// database to a new file without interrupting readers or writers.
    ///
    /// `VACUUM INTO` does not accept bind parameters — the destination path
    /// must appear as a string literal in the statement.  Single quotes inside
    /// the path are doubled (`'` → `''`) to form a valid SQLite string literal.
    /// The resulting statement is wrapped in [`sqlx::AssertSqlSafe`] to satisfy
    /// the sqlx dynamic-SQL audit requirement; the quoting is the audit.
    ///
    /// # Errors
    ///
    /// Returns `persistence_core::DbError::Database` if the statement fails (e.g. destination
    /// already exists, or disk full).
    pub async fn backup_to(&self, dest: &std::path::Path) -> DbResult<()> {
        let dest_str = dest.display().to_string().replace('\'', "''");
        let stmt = sqlx::AssertSqlSafe(format!("VACUUM INTO '{dest_str}'"));
        sqlx::query(stmt).execute(&self.pool).await?;
        Ok(())
    }

    /// Run all pending migrations from the frozen baseline and future append-only files.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Migration`] if any migration script fails.
    pub async fn migrate(&self) -> DbResult<()> {
        self.migrate_uncached().await
    }

    /// Run the embedded migration set directly.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Migration`] if any migration script fails.
    pub async fn migrate_uncached(&self) -> DbResult<()> {
        let mut conn = self.pool.acquire().await?;
        schema_cache::MIGRATOR.run(&mut *conn).await?;
        Ok(())
    }

    /// The embedded migration set, for tests that inspect migration metadata.
    #[must_use]
    pub fn migrator() -> &'static sqlx::migrate::Migrator {
        &schema_cache::MIGRATOR
    }

    /// Expose the underlying pool for repository constructors.
    #[must_use]
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

/// Describe a migration failure that means "this database file was written by a
/// different revision of the app", or `None` for any other failure.
#[must_use]
pub fn migration_divergence_detail(error: &DbError) -> Option<String> {
    let DbError::Migration(error) = error else { return None };
    let detail = match error {
        sqlx::migrate::MigrateError::VersionMismatch(version) => format!(
            "migration {version} was already applied to this database, but its script differs from the one in this build"
        ),
        sqlx::migrate::MigrateError::VersionMissing(version) => format!(
            "migration {version} was applied to this database but does not exist in this build"
        ),
        sqlx::migrate::MigrateError::VersionNotPresent(version) => format!(
            "migration {version} is recorded in this database but is absent from this build's migration set"
        ),
        _ => return None,
    };
    Some(detail)
}

#[cfg(test)]
mod tests {
    use super::CRATE_NAME;

    #[test]
    fn exposes_crate_name() {
        assert_eq!(CRATE_NAME, "persistence_core");
    }

    #[test]
    fn divergence_detail_names_the_offending_migration() {
        for error in [
            sqlx::migrate::MigrateError::VersionMismatch(71),
            sqlx::migrate::MigrateError::VersionMissing(71),
            sqlx::migrate::MigrateError::VersionNotPresent(71),
        ] {
            let detail = super::migration_divergence_detail(&super::DbError::Migration(error))
                .expect("divergence variant should produce a detail");
            assert!(detail.contains("71"), "detail should name the version: {detail}");
        }
    }

    #[test]
    fn divergence_detail_ignores_unrelated_failures() {
        assert!(super::migration_divergence_detail(&super::DbError::NotImplemented).is_none());
        assert!(super::migration_divergence_detail(&super::DbError::Migration(
            sqlx::migrate::MigrateError::Dirty(71)
        ))
        .is_none());
    }

    #[tokio::test]
    async fn real_sqlx_divergence_is_classified() {
        let db = super::Database::in_memory().await.expect("in-memory connect");
        db.migrate().await.expect("first migrate");

        sqlx::query("UPDATE _sqlx_migrations SET checksum = X'00' WHERE version = 1")
            .execute(db.pool())
            .await
            .expect("tamper with recorded checksum");

        let error = db.migrate().await.expect_err("divergent history must fail");
        let detail = super::migration_divergence_detail(&error)
            .expect("a real sqlx divergence must be classified, not fall through");
        assert!(detail.contains('1'), "detail should name the version: {detail}");
    }

    #[tokio::test]
    async fn database_connect_in_memory_and_migrate() {
        let db = super::Database::in_memory().await.expect("in-memory connect");
        db.migrate().await.expect("migrations");
        let row: (i64,) = sqlx::query_as("SELECT 1").fetch_one(db.pool()).await.unwrap();
        assert_eq!(row.0, 1);
    }

    #[tokio::test]
    async fn foreign_keys_enabled_on_fresh_connection() {
        let db = super::Database::in_memory().await.expect("in-memory connect");
        let fk_on: i64 =
            sqlx::query_scalar("PRAGMA foreign_keys").fetch_one(db.pool()).await.unwrap();
        assert_eq!(fk_on, 1, "PRAGMA foreign_keys must be 1 (ON) on every connection");
    }

    #[tokio::test]
    async fn has_pending_migrations_is_false_for_fresh_db() {
        let db = super::Database::in_memory().await.expect("in-memory connect");
        // Fresh DB: _sqlx_migrations does not exist yet.
        let pending = db.has_pending_migrations().await.expect("has_pending_migrations");
        assert!(!pending, "a fresh database has no pending migrations to back up");
    }

    #[tokio::test]
    async fn has_pending_migrations_is_false_after_migrate() {
        let db = super::Database::in_memory().await.expect("in-memory connect");
        db.migrate().await.expect("migrate");
        let pending = db.has_pending_migrations().await.expect("has_pending_migrations");
        assert!(!pending, "all migrations applied: no pending migrations");
    }

    #[tokio::test]
    async fn has_pending_migrations_is_true_when_behind() {
        let db = super::Database::in_memory().await.expect("in-memory connect");
        db.migrate().await.expect("initial migrate");

        // Simulate a DB that has had one migration rolled back by deleting the
        // last applied row — so applied_count < total.
        sqlx::query("DELETE FROM _sqlx_migrations WHERE version = (SELECT MAX(version) FROM _sqlx_migrations)")
            .execute(db.pool())
            .await
            .expect("remove last applied migration");

        let pending = db.has_pending_migrations().await.expect("has_pending_migrations");
        assert!(pending, "one migration removed: must report pending");
    }
}
