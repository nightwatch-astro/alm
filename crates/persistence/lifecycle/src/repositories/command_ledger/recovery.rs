// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Expired-lease recovery logic for the command ledger.
//!
//! Handles two recoverable shapes: (a) a clean untouched execution (no evidence
//! rows exist) → re-claimed; (b) one audit + matching outbox sequence
//! consistent with the evidence marker → reconciled as replayed terminal.

use serde_json::Value;
use sqlx::FromRow;

use super::validate::validate_state_error_consistency;
use super::{
    bounded_safe_string, canonical_json, digest_manifest, AuditOutcome, ClaimOutcome, CommandFence,
    CommandLease, CommandLedgerError, CommandRow, CommandTerminal, Result, TerminalState,
    MAX_OUTBOX_EVENTS, MAX_RESPONSE_BYTES,
};

#[derive(Debug, FromRow)]
pub(super) struct RecoveryOutboxRow {
    pub event_ordinal: i64,
    pub event_type: String,
    pub payload_json: String,
    pub operation_row_id: Option<i64>,
    pub proposal_row_id: Option<i64>,
    pub session_row_id: Option<i64>,
    pub panel_group_row_id: Option<i64>,
    pub mosaic_row_id: Option<i64>,
    pub project_row_id: Option<i64>,
    pub handoff_row_id: Option<i64>,
}

/// Loaded evidence from the database for recovery decisions.
pub(super) struct RecoveryEvidence {
    pub audit_rows: Vec<(String,)>,
    pub outbox_rows: Vec<RecoveryOutboxRow>,
}

/// Load audit and outbox evidence rows for the given command.
pub(super) async fn load_recovery_evidence(
    connection: &mut sqlx::pool::PoolConnection<sqlx::Sqlite>,
    row_id: i64,
) -> Result<RecoveryEvidence> {
    let audit_rows: Vec<(String,)> =
        sqlx::query_as("SELECT outcome FROM audit_event WHERE command_row_id = ? ORDER BY row_id")
            .bind(row_id)
            .fetch_all(&mut **connection)
            .await?;
    let outbox_rows: Vec<RecoveryOutboxRow> = sqlx::query_as(
        "SELECT event_ordinal, event_type, payload_json, operation_row_id, proposal_row_id,
                session_row_id, panel_group_row_id, mosaic_row_id, project_row_id, handoff_row_id
         FROM outbox_event WHERE command_row_id = ? ORDER BY event_ordinal",
    )
    .bind(row_id)
    .fetch_all(&mut **connection)
    .await?;
    Ok(RecoveryEvidence { audit_rows, outbox_rows })
}

/// Validate that recovery evidence has a recognizable shape.
/// Returns an error on any ambiguous partial-commit state.
pub(super) fn validate_evidence_shape(evidence: &RecoveryEvidence) -> Result<()> {
    if evidence.audit_rows.is_empty() && evidence.outbox_rows.is_empty() {
        return Ok(());
    }
    if evidence.audit_rows.len() != 1
        || evidence.outbox_rows.len() > MAX_OUTBOX_EVENTS
        || evidence.outbox_rows.iter().enumerate().any(|(ordinal, event)| {
            event.event_ordinal != i64::try_from(ordinal).unwrap_or(i64::MAX)
        })
    {
        return Err(CommandLedgerError::AmbiguousRecovery);
    }
    Ok(())
}

/// For a row with no evidence (no audit, no outbox): verify it has no
/// leftover recovery markers that would indicate a partial commit.
pub(super) fn validate_clean_execution(row: &CommandRow) -> Result<()> {
    if row.response_json.is_some()
        || row.error_code.is_some()
        || row.recovery_terminal_outcome.is_some()
        || row.recovery_response_json.is_some()
        || row.recovery_error_code.is_some()
        || row.recovery_expected_outbox_count.is_some()
        || row.recovery_expected_outbox_digest.is_some()
    {
        return Err(CommandLedgerError::AmbiguousRecovery);
    }
    Ok(())
}

/// Reconcile a discovered audit + outbox evidence set against the row's
/// recovery markers, returning the terminal result on success.
// CCN 12 tolerated: flat sequential invariant checks that must all pass.
pub(super) fn reconcile_discovered_evidence(
    row: &CommandRow,
    evidence: &RecoveryEvidence,
) -> Result<(TerminalState, Option<String>, Option<String>)> {
    let expected_count = row
        .recovery_expected_outbox_count
        .filter(|count| *count >= 0)
        .and_then(|count| usize::try_from(count).ok())
        .ok_or(CommandLedgerError::AmbiguousRecovery)?;
    if expected_count != evidence.outbox_rows.len() {
        return Err(CommandLedgerError::AmbiguousRecovery);
    }
    let expected_digest = row
        .recovery_expected_outbox_digest
        .as_deref()
        .ok_or(CommandLedgerError::AmbiguousRecovery)?;
    if recovery_outbox_digest(&evidence.outbox_rows)? != expected_digest {
        return Err(CommandLedgerError::AmbiguousRecovery);
    }

    let recorded_outcome = &evidence.audit_rows[0].0;
    let audit_outcome =
        AuditOutcome::parse(recorded_outcome).ok_or(CommandLedgerError::AmbiguousRecovery)?;
    let pending_outcome = row
        .recovery_terminal_outcome
        .as_deref()
        .and_then(AuditOutcome::parse)
        .ok_or(CommandLedgerError::AmbiguousRecovery)?;
    if pending_outcome != audit_outcome {
        return Err(CommandLedgerError::AmbiguousRecovery);
    }

    let state = audit_outcome.command_state();
    let response_json = row.recovery_response_json.clone();
    let error_code = row.recovery_error_code.clone();

    validate_recovered_response(response_json.as_deref())?;
    validate_recovered_error_code(error_code.as_deref())?;
    validate_state_error_consistency(state, error_code.as_deref())
        .map_err(|_| CommandLedgerError::AmbiguousRecovery)?;

    Ok((state, response_json, error_code))
}

fn validate_recovered_response(response_json: Option<&str>) -> Result<()> {
    let Some(response) = response_json else { return Ok(()) };
    let parsed = serde_json::from_str::<Value>(response)
        .map_err(|_| CommandLedgerError::AmbiguousRecovery)?;
    if canonical_json(&parsed).map_err(|_| CommandLedgerError::AmbiguousRecovery)? != response {
        return Err(CommandLedgerError::AmbiguousRecovery);
    }
    if response.len() > MAX_RESPONSE_BYTES {
        return Err(CommandLedgerError::AmbiguousRecovery);
    }
    Ok(())
}

fn validate_recovered_error_code(error_code: Option<&str>) -> Result<()> {
    if let Some(error) = error_code {
        bounded_safe_string(error).map_err(|_| CommandLedgerError::AmbiguousRecovery)?;
    }
    Ok(())
}

/// Commit a reconciled terminal result to the command row.
pub(super) async fn commit_reconciled_terminal(
    connection: &mut sqlx::pool::PoolConnection<sqlx::Sqlite>,
    row: &CommandRow,
    state: TerminalState,
    response_json: Option<&str>,
    error_code: Option<&str>,
    now: &str,
) -> Result<ClaimOutcome> {
    let update = sqlx::query(
        "UPDATE command_execution
         SET state = ?, state_version = state_version + 1, lease_owner = NULL,
             lease_expires_at = NULL, heartbeat_at = NULL, response_json = ?,
             error_code = ?, finished_at = ?, recovery_terminal_outcome = NULL,
             recovery_response_json = NULL, recovery_error_code = NULL,
             recovery_expected_outbox_count = NULL, recovery_expected_outbox_digest = NULL
         WHERE row_id = ? AND state_version = ? AND lease_generation = ?",
    )
    .bind(state.as_str())
    .bind(response_json)
    .bind(error_code)
    .bind(now)
    .bind(row.row_id)
    .bind(row.state_version)
    .bind(row.lease_generation)
    .execute(&mut **connection)
    .await?;
    if update.rows_affected() != 1 {
        return Err(CommandLedgerError::AmbiguousRecovery);
    }
    Ok(ClaimOutcome::Replayed(CommandTerminal {
        command_id: row.public_id.clone(),
        state,
        response_json: response_json.map(str::to_owned),
        error_code: error_code.map(str::to_owned),
        finished_at: now.to_owned(),
    }))
}

/// Re-claim an expired lease for fresh execution.
pub(super) async fn reclaim_expired_lease(
    connection: &mut sqlx::pool::PoolConnection<sqlx::Sqlite>,
    row: &CommandRow,
    worker_id: &str,
    expiry: &str,
    now: &str,
) -> Result<ClaimOutcome> {
    let update = sqlx::query(
        "UPDATE command_execution
         SET state = 'executing', state_version = state_version + 1,
             lease_generation = lease_generation + 1, lease_owner = ?,
             lease_expires_at = ?, heartbeat_at = ?, started_at = COALESCE(started_at, ?)
         WHERE row_id = ? AND state IN ('received','executing')
           AND state_version = ? AND lease_generation = ?
           AND (lease_expires_at IS NULL OR lease_expires_at <= ?)",
    )
    .bind(worker_id)
    .bind(expiry)
    .bind(now)
    .bind(now)
    .bind(row.row_id)
    .bind(row.state_version)
    .bind(row.lease_generation)
    .bind(now)
    .execute(&mut **connection)
    .await?;
    if update.rows_affected() != 1 {
        return Err(CommandLedgerError::InProgress);
    }
    Ok(ClaimOutcome::Claimed(CommandLease {
        fence: CommandFence {
            command_id: row.public_id.clone(),
            lease_owner: worker_id.to_owned(),
            lease_generation: row.lease_generation + 1,
        },
        state_version: row.state_version + 1,
        lease_expires_at: expiry.to_owned(),
        heartbeat_at: now.to_owned(),
    }))
}

/// Compute the recovery outbox digest from persisted outbox rows.
pub(super) fn recovery_outbox_digest(rows: &[RecoveryOutboxRow]) -> Result<String> {
    let mut manifest = Vec::with_capacity(rows.len());
    for row in rows {
        let payload = serde_json::from_str::<Value>(&row.payload_json)
            .map_err(|_| CommandLedgerError::AmbiguousRecovery)?;
        let aggregate = Value::Array(
            [
                row.operation_row_id,
                row.proposal_row_id,
                row.session_row_id,
                row.panel_group_row_id,
                row.mosaic_row_id,
                row.project_row_id,
                row.handoff_row_id,
            ]
            .into_iter()
            .map(|value| value.map_or(Value::Null, Value::from))
            .collect(),
        );
        manifest.push(serde_json::json!({
            "ordinal": row.event_ordinal,
            "aggregate": aggregate,
            "eventType": row.event_type,
            "payload": payload,
        }));
    }
    digest_manifest(&Value::Array(manifest))
}
