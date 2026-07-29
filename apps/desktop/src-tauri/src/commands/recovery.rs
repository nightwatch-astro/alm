// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Unclean-shutdown recovery command (astro-plan-kyo7.48).
//!
//! `recovery.status` drives the boot-time recovery prompt: it reports whether
//! the previous process exited cleanly and which plans were left mid-apply by a
//! crash. The plan list is queried live (order-independent with the async boot
//! sweep); the clean/unclean verdict comes from the managed boot flag.

use tauri::State;

use crate::clean_shutdown::UncleanShutdown;
use crate::commands::lifecycle::AppState;
use contracts_core::plan_apply::RecoveryStatus;
use contracts_core::ContractError;

/// `recovery.status` — input for the unclean-shutdown recovery prompt.
///
/// # Errors
///
/// Returns [`ContractError`] on a database failure while listing interrupted
/// plans.
#[tauri::command]
#[specta::specta]
pub async fn recovery_status(
    state: State<'_, AppState>,
    unclean: State<'_, UncleanShutdown>,
) -> Result<RecoveryStatus, ContractError> {
    let interrupted_plan_ids =
        app_core::plan_apply::list_crash_interrupted_plans(state.repo.pool())
            .await
            .map_err(|e| ContractError::internal(e.to_string()))?;
    Ok(RecoveryStatus { unclean_shutdown: unclean.0, interrupted_plan_ids })
}
