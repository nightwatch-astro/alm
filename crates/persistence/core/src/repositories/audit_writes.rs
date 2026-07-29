// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Audit-log write primitives shared across persistence sub-crates.
//!
//! Placed in persistence_core so targets, plans, and lifecycle can all call
//! these without creating cross-sub-crate edges.

use audit_types::AuditLogEntry;
use sqlx::{SqliteConnection, SqlitePool};

use crate::DbResult;

/// Insert an `audit_log_entry` row from an `AuditLogEntry`.
///
/// The durable-write path for `EventBus::write_audit` (constitution §II).
/// Callers MUST propagate an `Err` as a command failure — unlike bus emit
/// which is best-effort.
///
/// # Errors
/// Returns `persistence_core::DbError` if the insert fails.
pub async fn insert_audit_entry(pool: &SqlitePool, entry: &AuditLogEntry) -> DbResult<()> {
    let at_str = entry
        .at
        .as_offset_date_time()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned());
    let payload_str = entry.payload.as_ref().map(std::string::ToString::to_string);

    sqlx::query(
        "INSERT INTO audit_log_entry \
         (audit_id, entity_type, entity_id, from_state, to_state, trigger, actor, \
          outcome, severity, request_id, at, payload, reason_code) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(entry.audit_id.as_uuid().to_string())
    .bind(entry.entity_type.as_str())
    .bind(entry.entity_id.to_string())
    .bind(&entry.from_state)
    .bind(&entry.to_state)
    .bind(&entry.trigger)
    .bind(&entry.actor)
    .bind(entry.outcome.as_str())
    .bind(entry.severity.as_str())
    .bind(entry.request_id.to_string())
    .bind(&at_str)
    .bind(&payload_str)
    .bind(&entry.reason_code)
    .execute(pool)
    .await?;

    Ok(())
}

/// Insert an `audit_log_entry` row on an existing connection (for use inside
/// a transaction).
///
/// # Errors
/// Returns `persistence_core::DbError` if the insert fails.
pub async fn insert_audit_entry_conn(
    conn: &mut SqliteConnection,
    entry: &AuditLogEntry,
) -> DbResult<()> {
    let at_str = entry
        .at
        .as_offset_date_time()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned());
    let payload_str = entry.payload.as_ref().map(std::string::ToString::to_string);

    sqlx::query(
        "INSERT INTO audit_log_entry \
         (audit_id, entity_type, entity_id, from_state, to_state, trigger, actor, \
          outcome, severity, request_id, at, payload, reason_code) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(entry.audit_id.as_uuid().to_string())
    .bind(entry.entity_type.as_str())
    .bind(entry.entity_id.to_string())
    .bind(&entry.from_state)
    .bind(&entry.to_state)
    .bind(&entry.trigger)
    .bind(&entry.actor)
    .bind(entry.outcome.as_str())
    .bind(entry.severity.as_str())
    .bind(entry.request_id.to_string())
    .bind(&at_str)
    .bind(&payload_str)
    .bind(&entry.reason_code)
    .execute(conn)
    .await?;

    Ok(())
}

/// Insert a project auto-transition audit row, acquiring a connection from
/// the pool.
///
/// # Errors
/// Returns `persistence_core::DbError` on query failure.
pub async fn insert_project_auto_transition(
    pool: &SqlitePool,
    project_id: &str,
    from_state: &str,
    to_state: &str,
    trigger: &str,
) -> DbResult<()> {
    let mut conn = pool.acquire().await?;
    insert_project_auto_transition_conn(&mut conn, project_id, from_state, to_state, trigger).await
}

/// Insert a project auto-transition audit row on an existing connection
/// (for use inside a transaction).
///
/// # Errors
/// Returns `persistence_core::DbError` on query failure.
pub async fn insert_project_auto_transition_conn(
    conn: &mut SqliteConnection,
    project_id: &str,
    from_state: &str,
    to_state: &str,
    trigger: &str,
) -> DbResult<()> {
    use time::format_description::well_known::Rfc3339;
    use uuid::Uuid;

    let audit_id = Uuid::new_v4().to_string();
    let request_id = Uuid::new_v4().to_string();
    let at = time::OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned());

    sqlx::query(
        "INSERT INTO audit_log_entry \
         (audit_id, entity_type, entity_id, from_state, to_state, trigger, actor, \
          outcome, severity, request_id, at, payload) \
         VALUES (?, 'project', ?, ?, ?, ?, 'system', 'applied', 'workflow', ?, ?, NULL)",
    )
    .bind(&audit_id)
    .bind(project_id)
    .bind(from_state)
    .bind(to_state)
    .bind(trigger)
    .bind(&request_id)
    .bind(&at)
    .execute(conn)
    .await?;

    Ok(())
}

/// Insert a target resolution audit row.
///
/// # Errors
/// Returns `persistence_core::DbError` on query failure.
#[allow(clippy::too_many_arguments)]
pub async fn insert_resolution_audit(
    pool: &SqlitePool,
    audit_id: &str,
    target_id: &str,
    trigger: &str,
    actor: &str,
    request_id: &str,
    at: &str,
    payload: &str,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO audit_log_entry \
         (audit_id, entity_type, entity_id, from_state, to_state, trigger, actor, \
          outcome, severity, request_id, at, payload) \
         VALUES (?, 'canonical_target', ?, NULL, NULL, ?, ?, 'applied', 'workflow', ?, ?, ?)",
    )
    .bind(audit_id)
    .bind(target_id)
    .bind(trigger)
    .bind(actor)
    .bind(request_id)
    .bind(at)
    .bind(payload)
    .execute(pool)
    .await?;
    Ok(())
}
