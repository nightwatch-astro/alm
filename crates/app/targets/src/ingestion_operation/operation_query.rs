// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only queries for `session.materialization.query` and
//! `session.materialization.result_session.list` contract endpoints.

use sqlx::SqlitePool;

use persistence_core::DbResult;
use persistence_sessions::repositories::actors::lookup_spec062_target_row_id;
use persistence_sessions::repositories::change_sequence::current_sequence;
use persistence_sessions::repositories::materialization::{
    get_operation_by_public_id, get_result_snapshot_by_operation_public_id,
    MaterializationOperationRow, MaterializationResultSnapshotRow,
};
use persistence_sessions::repositories::sessions::{
    list_sessions_at_watermark, SessionListFilter, SessionRow,
};

/// Fetch the `session_materialization_operation` for a contract query.
///
/// # Errors
///
/// Returns [`persistence_core::DbError::NotFound`] when the operation does not
/// exist.
pub async fn query_operation(
    pool: &SqlitePool,
    operation_public_id: &str,
) -> DbResult<MaterializationOperationRow> {
    get_operation_by_public_id(pool, operation_public_id).await
}

/// Fetch the result snapshot for a terminal `applied` operation.
///
/// # Errors
///
/// Returns [`persistence_core::DbError::NotFound`] when the operation has no
/// result snapshot (i.e. it is not yet `applied`).
pub async fn query_result_snapshot(
    pool: &SqlitePool,
    operation_public_id: &str,
) -> DbResult<MaterializationResultSnapshotRow> {
    get_result_snapshot_by_operation_public_id(pool, operation_public_id).await
}

/// Parameters for `session.list` watermarked queries.
pub struct SessionListParams<'a> {
    pub canonical_target_public_id: Option<&'a str>,
    pub kind_filter: Option<&'a str>,
    pub page_size: i64,
    pub cursor_created_at: Option<&'a str>,
    pub cursor_public_id: Option<&'a str>,
}

/// Return a watermarked first page of sessions, resolving the watermark from
/// the current `repository_change` sequence.
///
/// # Errors
///
/// Returns [`persistence_core::DbError`] on SQL errors.
pub async fn list_sessions_first_page(
    pool: &SqlitePool,
    params: &SessionListParams<'_>,
) -> DbResult<(i64, Vec<SessionRow>)> {
    let target_row_id = resolve_target_row_id(pool, params.canonical_target_public_id).await?;
    let watermark = current_sequence(pool).await?;
    let filter = SessionListFilter {
        canonical_target_row_id: target_row_id,
        kind: params.kind_filter,
        ..Default::default()
    };
    let rows = list_sessions_at_watermark(
        pool,
        watermark,
        &filter,
        params.cursor_created_at,
        params.cursor_public_id,
        params.page_size,
    )
    .await?;
    Ok((watermark, rows))
}

/// Return a subsequent page of sessions at a previously captured watermark.
///
/// # Errors
///
/// Returns [`persistence_core::DbError`] on SQL errors.
pub async fn list_sessions_next_page(
    pool: &SqlitePool,
    watermark: i64,
    params: &SessionListParams<'_>,
) -> DbResult<Vec<SessionRow>> {
    let target_row_id = resolve_target_row_id(pool, params.canonical_target_public_id).await?;
    let filter = SessionListFilter {
        canonical_target_row_id: target_row_id,
        kind: params.kind_filter,
        ..Default::default()
    };
    list_sessions_at_watermark(
        pool,
        watermark,
        &filter,
        params.cursor_created_at,
        params.cursor_public_id,
        params.page_size,
    )
    .await
}

/// Resolve an optional `spec062_target` public UUID to its `row_id`.
async fn resolve_target_row_id(
    pool: &SqlitePool,
    public_id: Option<&str>,
) -> DbResult<Option<i64>> {
    let Some(t) = public_id else {
        return Ok(None);
    };
    let mut conn = pool.acquire().await?;
    lookup_spec062_target_row_id(&mut conn, t).await
}
