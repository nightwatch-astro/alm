// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Root path remap preview + apply (`roots.remap`/`.apply`, P6a).

use audit::bus::EventBus;
use audit::event_bus::{RootRemapped, Source, TOPIC_ROOT_REMAPPED};
use audit::{AuditLogEntry, Outcome, Severity};
use contracts_core::roots::{RemapSample, RemapVerification};
use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use domain_core::ids::EntityId;
use domain_core::lifecycle::data_asset::EntityType;
use persistence_lifecycle::repositories::first_run as repo;
use sqlx::SqlitePool;

use crate::audit_ids::deterministic_entity_id;
use crate::caches;
use crate::errors::bus_err;

use super::{
    db_to_contract, error_code_str, get_root_or_not_found, validate_path, write_root_op_refusal,
};

/// Preview a root path remap (`roots.remap`, P6a).
///
/// Validates that `new_path` exists, is a directory, and is readable, then
/// checks EVERY relative path previously recorded for `root_id` (via
/// `file_record` and pending `inbox_items` — see
/// [`repo::relative_paths_for_root`]) and reports whether each resolves under
/// `new_path`. Does NOT mutate anything — call [`apply_root_remap`] after
/// review.
///
/// Exhaustive by design (issue #560): a bounded sample let files outside it
/// go unverified, and checking `file_record` alone let a root that was only
/// ever scanned into the Inbox report a vacuous "all verified" from zero
/// samples, unlocking Apply against a path that held none of its real
/// content. Roots with no recorded rows in either table (calibration/project
/// roots, or raw roots registered directly without ever receiving an inbox
/// scan) report zero samples — `all_verified` then reflects only
/// `new_path`'s own validity, which is correct: there is nothing recorded to
/// silently orphan.
///
/// # Errors
///
/// - `source.not_found` — no root with `root_id`.
/// - `path.not_exists` / `path.not_directory` / `path.permission_denied` —
///   `new_path` fails validation.
/// - `internal.database` — query failure.
pub async fn remap_root(
    pool: &SqlitePool,
    root_id: &str,
    new_path: &str,
) -> Result<RemapVerification, ContractError> {
    let (_, original_path) = get_root_or_not_found(pool, root_id).await?;

    validate_path(new_path).map_err(|e| *e)?;

    let (samples, all_verified) = compute_remap_verification(pool, root_id, new_path).await?;

    Ok(RemapVerification {
        root_id: root_id.to_owned(),
        original_path,
        new_path: new_path.to_owned(),
        samples,
        all_verified,
    })
}

/// Compute remap verification samples for `root_id` against `new_path`:
/// every relative path previously recorded for the root (see
/// [`repo::relative_paths_for_root`]), and whether each resolves under
/// `new_path`. Shared by [`remap_root`] (preview) and [`apply_root_remap`]
/// (independent re-verification — nJ01a review carry-over: `apply_root_remap`
/// used to trust the caller-supplied `verified` flag outright, so a stale
/// preview or a directly-called IPC command could apply an unverified remap).
async fn compute_remap_verification(
    pool: &SqlitePool,
    root_id: &str,
    new_path: &str,
) -> Result<(Vec<RemapSample>, bool), ContractError> {
    let relative_paths =
        repo::relative_paths_for_root(pool, root_id).await.map_err(db_to_contract)?;

    let new_root = std::path::Path::new(new_path);
    let samples: Vec<RemapSample> = relative_paths
        .into_iter()
        .map(|relative_path| {
            let found = new_root.join(&relative_path).exists();
            RemapSample { relative_path, found }
        })
        .collect();
    let all_verified = samples.iter().all(|s| s.found);
    Ok((samples, all_verified))
}

/// Gate a remap apply on verification, refusing (and auditing) when either
/// the caller-supplied `verified` flag is `false`, or a freshly recomputed
/// [`compute_remap_verification`] disagrees with a `verified: true` claim.
/// Extracted from [`apply_root_remap`] to keep it under clippy's line budget
/// (nJ01a review carry-over — see that function's doc comment for why the
/// flag alone isn't trusted).
async fn ensure_remap_verified(
    pool: &SqlitePool,
    bus: &EventBus,
    root_id: &str,
    new_path: &str,
    verified: bool,
) -> Result<(), ContractError> {
    // Issue #707: `verified` must actually gate the mutation, not just ride
    // along as audit metadata — the whole point of the two-step Verify →
    // Apply flow (constitution §II) is that Apply is refused without a prior
    // successful Verify. Checked independently of any UI-side disabled state
    // since this is reachable directly over IPC.
    let recomputed_verified = if verified {
        match compute_remap_verification(pool, root_id, new_path).await {
            Ok((_, recomputed)) => recomputed,
            Err(e) => {
                write_root_op_refusal(
                    bus,
                    root_id,
                    "root.remap.apply",
                    Outcome::Failed,
                    &error_code_str(e.code),
                )
                .await?;
                return Err(e);
            }
        }
    } else {
        false
    };

    if !recomputed_verified {
        write_root_op_refusal(
            bus,
            root_id,
            "root.remap.apply",
            Outcome::Refused,
            "remap.not_verified",
        )
        .await?;
        return Err(ContractError::new(
            ErrorCode::RemapNotVerified,
            "remap must be verified before it can be applied",
            ErrorSeverity::Blocking,
            false,
        ));
    }

    Ok(())
}

/// Apply a previously previewed root remap (`roots.remap.apply`, P6a).
///
/// Updates the root's stored path in `registered_sources` (metadata only —
/// no files are moved) and publishes a best-effort `root.remapped` audit
/// event recording the prior path, new path, and the caller-supplied
/// `verified` flag (expected to be the `all_verified` value from a matching
/// [`remap_root`] preview), per constitution Principle II.
///
/// Re-validates `new_path` so an apply cannot silently succeed against a path
/// that no longer exists between preview and apply (e.g. an unmounted drive).
/// Also re-derives verification itself via `compute_remap_verification`
/// rather than trusting the caller-supplied `verified` flag: both the flag
/// AND the freshly recomputed state must agree the remap is verified, so a
/// stale preview or a direct IPC call can't bypass the gate.
///
/// # Errors
///
/// - `source.not_found` — no root with `root_id`.
/// - `path.not_exists` / `path.not_directory` / `path.permission_denied` —
///   `new_path` fails validation.
/// - `remap.not_verified` — `verified` is `false`, or the current state no
///   longer backs a `verified: true` claim.
/// - `internal.database` — persistence failure.
pub async fn apply_root_remap(
    pool: &SqlitePool,
    bus: &EventBus,
    root_id: &str,
    new_path: &str,
    verified: bool,
) -> Result<(), ContractError> {
    let (_, original_path) = match get_root_or_not_found(pool, root_id).await {
        Ok(v) => v,
        Err(e) => {
            write_root_op_refusal(
                bus,
                root_id,
                "root.remap.apply",
                Outcome::Failed,
                &error_code_str(e.code),
            )
            .await?;
            return Err(e);
        }
    };

    if let Err(e) = validate_path(new_path) {
        write_root_op_refusal(
            bus,
            root_id,
            "root.remap.apply",
            Outcome::Refused,
            &error_code_str(e.code),
        )
        .await?;
        return Err(*e);
    }

    ensure_remap_verified(pool, bus, root_id, new_path, verified).await?;

    if let Err(e) = repo::set_source_path(pool, root_id, new_path).await {
        // FIX (review round 1 #2): the write was attempted (root exists,
        // new_path validated) and failed — audit as `Failed`.
        let err = db_to_contract(e);
        write_root_op_refusal(
            bus,
            root_id,
            "root.remap.apply",
            Outcome::Failed,
            &error_code_str(err.code),
        )
        .await?;
        return Err(err);
    }
    // Invalidate after commit (F0 contract) so the next read reloads the new path.
    caches::invalidate_library_root(root_id);

    // Write durable audit row + live event (T125, FR-130/FR-131).
    let entry = AuditLogEntry::new(
        EntityType::LibraryRoot,
        deterministic_entity_id("root", root_id),
        "root.remap.apply",
        "user",
        Outcome::Applied,
        Severity::Workflow,
        EntityId::new(),
    )
    .with_payload(
        serde_json::json!({"before": original_path, "after": new_path, "verified": verified}),
    );
    bus.write_audit(
        entry,
        TOPIC_ROOT_REMAPPED,
        Source::User,
        RootRemapped {
            root_id: root_id.to_owned(),
            original_path,
            new_path: new_path.to_owned(),
            verified,
        },
    )
    .await
    .map_err(bus_err)?;

    Ok(())
}
