// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Extracted SQL phases for `CommandLedger::finish_at`.

use uuid::Uuid;

use super::{
    append_repository_change, AuditOutcome, CommandLedgerError, CommandRow, OutboxInput, Result,
    TerminalInput, TerminalState,
};

/// Guard: a live execution may only create its terminal evidence once.
pub(super) async fn guard_no_prior_evidence(
    connection: &mut sqlx::pool::PoolConnection<sqlx::Sqlite>,
    row_id: i64,
) -> Result<()> {
    let audit_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM audit_event WHERE command_row_id = ?")
            .bind(row_id)
            .fetch_one(&mut **connection)
            .await?;
    let outbox_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM outbox_event WHERE command_row_id = ?")
            .bind(row_id)
            .fetch_one(&mut **connection)
            .await?;
    if audit_count.0 != 0 || outbox_count.0 != 0 {
        return Err(CommandLedgerError::AmbiguousRecovery);
    }
    Ok(())
}

/// Write the recovery evidence marker (idempotency: if the process crashes
/// after this but before the terminal commit, recovery can reconcile).
#[allow(clippy::too_many_arguments)]
pub(super) async fn write_evidence_marker(
    connection: &mut sqlx::pool::PoolConnection<sqlx::Sqlite>,
    row: &CommandRow,
    audit_outcome: AuditOutcome,
    response_json: Option<&str>,
    error_code: Option<&str>,
    outbox_len: usize,
    outbox_digest: &str,
    lease_owner: &str,
    lease_generation: i64,
    now: &str,
) -> Result<()> {
    let evidence = sqlx::query(
        "UPDATE command_execution
         SET recovery_terminal_outcome = ?, recovery_response_json = ?,
             recovery_error_code = ?, recovery_expected_outbox_count = ?,
             recovery_expected_outbox_digest = ?
         WHERE row_id = ? AND state = 'executing' AND lease_owner = ?
           AND lease_generation = ? AND lease_expires_at > ?",
    )
    .bind(audit_outcome.as_str())
    .bind(response_json)
    .bind(error_code)
    .bind(i64::try_from(outbox_len).map_err(|_| {
        CommandLedgerError::InvalidInput("outbox sequence exceeds SQLite bounds".to_owned())
    })?)
    .bind(outbox_digest)
    .bind(row.row_id)
    .bind(lease_owner)
    .bind(lease_generation)
    .bind(now)
    .execute(&mut **connection)
    .await?;
    if evidence.rows_affected() != 1 {
        return Err(CommandLedgerError::StaleFence);
    }
    Ok(())
}

/// Write the single audit row and all outbox events.
pub(super) async fn write_audit_and_outbox(
    connection: &mut sqlx::pool::PoolConnection<sqlx::Sqlite>,
    row: &CommandRow,
    input: &TerminalInput,
    audit_outcome: AuditOutcome,
    audit_payload: Option<String>,
    outbox_payloads: Vec<String>,
    now: &str,
) -> Result<i64> {
    let change_sequence = append_repository_change(connection, row.row_id, now).await?;
    let audit_public_id = Uuid::new_v4().to_string();
    let values = input.audit.aggregate.values();
    sqlx::query(
        "INSERT INTO audit_event
         (public_id, command_row_id, operation_row_id, proposal_row_id, session_row_id,
          panel_group_row_id, mosaic_row_id, project_row_id, handoff_row_id, actor_row_id,
          action, outcome, reason_code, payload_json, created_sequence, occurred_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&audit_public_id)
    .bind(row.row_id)
    .bind(values[0])
    .bind(values[1])
    .bind(values[2])
    .bind(values[3])
    .bind(values[4])
    .bind(values[5])
    .bind(values[6])
    .bind(row.actor_row_id)
    .bind(&input.audit.action)
    .bind(audit_outcome.as_str())
    .bind(&input.audit.reason_code)
    .bind(audit_payload)
    .bind(change_sequence)
    .bind(now)
    .execute(&mut **connection)
    .await?;

    write_outbox_events(
        connection,
        row.row_id,
        &input.outbox,
        outbox_payloads,
        change_sequence,
        now,
    )
    .await?;

    Ok(change_sequence)
}

async fn write_outbox_events(
    connection: &mut sqlx::pool::PoolConnection<sqlx::Sqlite>,
    command_row_id: i64,
    outbox: &[OutboxInput],
    payloads: Vec<String>,
    change_sequence: i64,
    now: &str,
) -> Result<()> {
    for (ordinal, (event, payload)) in outbox.iter().zip(payloads).enumerate() {
        let values = event.aggregate.values();
        sqlx::query(
            "INSERT INTO outbox_event
             (public_id, command_row_id, event_ordinal, operation_row_id, proposal_row_id,
              session_row_id, panel_group_row_id, mosaic_row_id, project_row_id, handoff_row_id,
              event_type, payload_json, created_sequence, occurred_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(command_row_id)
        .bind(
            i64::try_from(ordinal).map_err(|_| {
                CommandLedgerError::InvalidInput("event ordinal overflow".to_owned())
            })?,
        )
        .bind(values[0])
        .bind(values[1])
        .bind(values[2])
        .bind(values[3])
        .bind(values[4])
        .bind(values[5])
        .bind(values[6])
        .bind(&event.event_type)
        .bind(payload)
        .bind(change_sequence)
        .bind(now)
        .execute(&mut **connection)
        .await?;
    }
    Ok(())
}

/// Verify exactly one audit and the expected outbox count were written.
pub(super) async fn verify_written_counts(
    connection: &mut sqlx::pool::PoolConnection<sqlx::Sqlite>,
    row_id: i64,
    expected_outbox: usize,
) -> Result<()> {
    let written_audit_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM audit_event WHERE command_row_id = ?")
            .bind(row_id)
            .fetch_one(&mut **connection)
            .await?;
    let written_outbox_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM outbox_event WHERE command_row_id = ?")
            .bind(row_id)
            .fetch_one(&mut **connection)
            .await?;
    if written_audit_count.0 != 1
        || written_outbox_count.0 != i64::try_from(expected_outbox).unwrap_or(i64::MAX)
    {
        return Err(CommandLedgerError::AmbiguousRecovery);
    }
    Ok(())
}

/// Commit the terminal state transition and clear recovery markers.
#[allow(clippy::too_many_arguments)]
pub(super) async fn commit_terminal_state(
    connection: &mut sqlx::pool::PoolConnection<sqlx::Sqlite>,
    row_id: i64,
    state: TerminalState,
    response_json: Option<&str>,
    error_code: Option<&str>,
    lease_owner: &str,
    lease_generation: i64,
    now: &str,
) -> Result<()> {
    let finished = TerminalState::as_str(state);
    let update = sqlx::query(
        "UPDATE command_execution
         SET state = ?, state_version = state_version + 1, lease_owner = NULL,
             lease_expires_at = NULL, heartbeat_at = NULL, response_json = ?,
             error_code = ?, finished_at = ?, recovery_terminal_outcome = NULL,
             recovery_response_json = NULL, recovery_error_code = NULL,
             recovery_expected_outbox_count = NULL, recovery_expected_outbox_digest = NULL
         WHERE row_id = ? AND state = 'executing' AND lease_owner = ?
           AND lease_generation = ? AND lease_expires_at > ?",
    )
    .bind(finished)
    .bind(response_json)
    .bind(error_code)
    .bind(now)
    .bind(row_id)
    .bind(lease_owner)
    .bind(lease_generation)
    .bind(now)
    .execute(&mut **connection)
    .await?;
    if update.rows_affected() != 1 {
        return Err(CommandLedgerError::StaleFence);
    }
    Ok(())
}
