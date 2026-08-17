// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Terminal-input validation helpers for the command ledger.

use super::{
    audit_outcome_matches_state, bounded_safe_string, canonical_json, default_audit_outcome,
    safe_payload_json, CommandLedgerError, Result, TerminalInput, TerminalState, MAX_OUTBOX_EVENTS,
    MAX_RESPONSE_BYTES,
};

/// Validate all invariants required before persisting a terminal result.
pub(super) fn validate_terminal(input: &TerminalInput) -> Result<()> {
    validate_outbox_bounds(input)?;
    validate_state_error_consistency(input.state, input.error_code.as_deref())?;
    validate_audit_fields(input)?;
    validate_audit_outcome_consistency(input)?;
    validate_outbox_events(&input.outbox)?;
    validate_response_bounds(input)?;
    validate_audit_payload(input)?;
    Ok(())
}

fn validate_outbox_bounds(input: &TerminalInput) -> Result<()> {
    if input.outbox.len() > MAX_OUTBOX_EVENTS {
        return Err(CommandLedgerError::InvalidInput(format!(
            "outbox sequence exceeds {MAX_OUTBOX_EVENTS} events"
        )));
    }
    Ok(())
}

/// Applied commands must not carry an error; refused/failed must.
pub(super) fn validate_state_error_consistency(
    state: TerminalState,
    error_code: Option<&str>,
) -> Result<()> {
    if matches!(state, TerminalState::Applied) && error_code.is_some() {
        return Err(CommandLedgerError::InvalidInput(
            "applied command cannot carry an error code".to_owned(),
        ));
    }
    if !matches!(state, TerminalState::Applied) && error_code.is_none() {
        return Err(CommandLedgerError::InvalidInput(
            "refused and failed commands require an error code".to_owned(),
        ));
    }
    Ok(())
}

fn validate_audit_fields(input: &TerminalInput) -> Result<()> {
    if input.audit.action.is_empty() || input.audit.reason_code.is_empty() {
        return Err(CommandLedgerError::InvalidInput(
            "audit action and reason are required".to_owned(),
        ));
    }
    input.audit.aggregate.validate()
}

fn validate_audit_outcome_consistency(input: &TerminalInput) -> Result<()> {
    let audit_outcome = input.audit.outcome.unwrap_or_else(|| default_audit_outcome(input.state));
    let expected_outcome = default_audit_outcome(input.state);
    if !audit_outcome_matches_state(audit_outcome, input.state)
        || (matches!(input.state, TerminalState::Applied | TerminalState::Failed)
            && audit_outcome != expected_outcome)
    {
        return Err(CommandLedgerError::InvalidInput(
            "audit outcome does not match terminal command state".to_owned(),
        ));
    }
    Ok(())
}

fn validate_outbox_events(outbox: &[super::OutboxInput]) -> Result<()> {
    for event in outbox {
        if event.event_type.is_empty() {
            return Err(CommandLedgerError::InvalidInput(
                "outbox event type is required".to_owned(),
            ));
        }
        bounded_safe_string(&event.event_type)?;
        event.aggregate.validate()?;
    }
    Ok(())
}

fn validate_response_bounds(input: &TerminalInput) -> Result<()> {
    if let Some(response) = &input.response {
        let serialized = canonical_json(response)?;
        if serialized.len() > MAX_RESPONSE_BYTES {
            return Err(CommandLedgerError::InvalidInput(
                "command response is too large".to_owned(),
            ));
        }
    }
    Ok(())
}

fn validate_audit_payload(input: &TerminalInput) -> Result<()> {
    if let Some(payload) = &input.audit.payload {
        let _ = safe_payload_json(payload)?;
    }
    Ok(())
}
