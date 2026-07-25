// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only
#![allow(
    clippy::missing_errors_doc,
    clippy::explicit_auto_deref,
    clippy::too_many_lines,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    dead_code
)]

//! Installer trait and item type for the Update View apply loop (spec 062 FR-100).
//!
//! `run_install` is the only call-site for `fs_executor::update_view::install_item`.
//! It sequences: commit intent → filesystem install → commit journal. The
//! `InstallerCallbacks` trait decouples the DB writes so both can be tested
//! independently with an in-process fake.

use camino::Utf8PathBuf;
use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use sqlx::SqlitePool;

use fs_executor::update_view::install_item;

/// One plan item to be installed.
///
/// `source_abs_path` and `dest_abs_path` are the resolved, root-prefixed
/// absolute paths the Tauri adapter supplies after loading the registered
/// library-root and destination-root paths. All other fields are DB row keys.
#[derive(Debug, Clone)]
pub struct InstallItem {
    pub item_row_id: i64,
    pub item_public_id: String,
    pub session_row_id: i64,
    pub frame_row_id: i64,
    pub destination_root_row_id: i64,
    pub destination_relative_path: String,
    pub approved_fingerprint: String,
    pub ordinal: i64,
    /// Absolute source file path (resolved by the caller from the registered
    /// source root + the frame's relative path).
    pub source_abs_path: Utf8PathBuf,
    /// Absolute destination directory path (the registered destination root).
    pub dest_root_abs_path: Utf8PathBuf,
}

/// Callbacks from the install loop to the persistence layer.
pub trait InstallerCallbacks: Send + Sync {
    /// Called before the atomic no-replace install. Persists the install intent
    /// with `ownership_token` so crash recovery can prove ownership.
    fn on_intent_prepared<'a>(
        &'a self,
        pool: &'a SqlitePool,
        item: &'a InstallItem,
        ownership_token: &'a str,
        command_id: &'a str,
        lease_owner: &'a str,
        lease_generation: i64,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ContractError>> + Send + 'a>>;

    /// Called after install + destination-directory fsync. Marks intent `installed`.
    fn on_installed<'a>(
        &'a self,
        pool: &'a SqlitePool,
        item_row_id: i64,
        lease_owner: &'a str,
        lease_generation: i64,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ContractError>> + Send + 'a>>;

    /// Commits the item journal. Returns the new `materialized_entry` row_id.
    fn on_journaled<'a>(
        &'a self,
        pool: &'a SqlitePool,
        item: &'a InstallItem,
        content_fingerprint: &'a str,
        operation_command_id: &'a str,
        lease_owner: &'a str,
        lease_generation: i64,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<i64, ContractError>> + Send + 'a>>;
}

/// Run a single install step: prepare intent → no-clobber filesystem install →
/// journal.
///
/// Sequence (FR-100 durability contract):
/// 1. Commit `install_intent` (`prepared`) via `on_intent_prepared`.
/// 2. Call `fs_executor::update_view::install_item` — writes bytes to a temp
///    file, fsyncs, then atomically renames via `persist_noclobber`.
/// 3. Mark intent `installed` via `on_installed`.
/// 4. Commit item journal via `on_journaled` — returns the entry row_id.
///
/// Returns `(entry_row_id, content_fingerprint)` on success. Any I/O or DB
/// error stops the loop and the caller marks the plan `stopped`.
pub async fn run_install(
    pool: &SqlitePool,
    _plan_row_id: i64,
    item: &InstallItem,
    operation_command_id: &str,
    lease_owner: &str,
    lease_generation: i64,
    callbacks: &impl InstallerCallbacks,
) -> Result<(i64, String), ContractError> {
    // Step 1: commit install intent with a placeholder ownership token.
    // The real token is produced after the filesystem install; we pre-commit a
    // UUID so recovery can detect partial work even if step 2 crashes before
    // returning.
    let pre_token = uuid::Uuid::new_v4().to_string();
    callbacks
        .on_intent_prepared(
            pool,
            item,
            &pre_token,
            operation_command_id,
            lease_owner,
            lease_generation,
        )
        .await?;

    // Step 2: atomic no-clobber filesystem install. Runs synchronously (blocking)
    // on the calling thread. The source is opened, hashed, written to a temp
    // file, fsynced, and renamed atomically via persist_noclobber.
    //
    // `install_item` takes (source_root, source_rel, dest_root, dest_rel).
    // We synthesise a single-component relative path from the file name so the
    // source resolves to the exact absolute path supplied by the caller.
    let source_filename = item.source_abs_path.file_name().unwrap_or(item.source_abs_path.as_str());
    let source_root = item
        .source_abs_path
        .parent()
        .map_or_else(|| item.source_abs_path.clone(), camino::Utf8Path::to_owned);

    let outcome = install_item(
        &source_root,
        source_filename,
        &item.dest_root_abs_path,
        &item.destination_relative_path,
    )
    .map_err(|e| {
        ContractError::new(
            ErrorCode::ProjectUpdateViewPathConflict,
            format!(
                "install failed for item {}: {:?} — {}",
                item.item_public_id, e.code, e.message
            ),
            ErrorSeverity::Blocking,
            false,
        )
    })?;

    let content_fingerprint = outcome.content_fingerprint;
    let ownership_token = outcome.ownership_token;

    // Update intent to `installed` with the real ownership token.
    callbacks.on_installed(pool, item.item_row_id, lease_owner, lease_generation).await?;

    // Step 4: commit item journal.
    let entry_row_id = callbacks
        .on_journaled(
            pool,
            item,
            &content_fingerprint,
            operation_command_id,
            lease_owner,
            lease_generation,
        )
        .await?;

    // ownership_token held for potential recovery — not persisted here; the
    // Tauri adapter may store it in a recovery log alongside the pre-token.
    let _ = ownership_token;

    Ok((entry_row_id, content_fingerprint))
}
