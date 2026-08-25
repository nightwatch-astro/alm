// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `audit.list` / `audit.export` — Tauri commands exposed to the webview.
//!
//! These were spec-029 stubs (hardcoded fixture, filters/pagination ignored).
//! They now read the durable `audit_log_entry` table (migration
//! `0002_lifecycle.sql`) via `persistence_lifecycle::repositories::audit`, which is
//! written by lifecycle transitions (`repositories::lifecycle`), target
//! resolution (`targets::target_resolve`), and system-driven project
//! transitions (`projects::project_health`).
//!
//! `AuditFilterDto` / `AuditPaginationDto` are the first typed shapes for
//! `audit.list`'s `filters?`/`pagination?` args (previously untyped `JsonAny`
//! — spec 029's `contracts/commands.md` left the shape unspecified). They are
//! defined locally, mirroring `commands::lifecycle::LedgerFilterDto`, so the
//! IPC boundary does not leak the persistence-internal `AuditLogFilter` type.

use app_core::errors::db_err;
use contracts_core::audit::{
    AuditActor, AuditEntry, AuditExportResponse, AuditListResponse, AuditOutcome, EntityNameRef,
    EntityNamesResponse,
};
use contracts_core::ContractError;
use persistence_lifecycle::repositories::audit::{
    count_audit_entries, list_audit_entries, AuditLogFilter, AuditLogRow,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use crate::commands::lifecycle::AppState;

/// Filter args for `audit.list` / `audit.export`.
///
/// `entity_type` + `entity_id` are the key fields a future per-entity history
/// view (e.g. an archive-detail audit trail) would reuse — kept as plain
/// equality filters rather than something bespoke to the settings screen.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AuditFilterDto {
    #[serde(default)]
    pub entity_type: Option<String>,
    #[serde(default)]
    pub entity_id: Option<String>,
    #[serde(default)]
    pub outcome: Option<AuditOutcome>,
    #[serde(default)]
    pub severity: Option<audit::Severity>,
    /// Case-insensitive substring match against event/entity/actor text.
    #[serde(default)]
    pub search: Option<String>,
    /// RFC 3339 lower bound on the entry timestamp (inclusive).
    #[serde(default)]
    pub from: Option<String>,
    /// RFC 3339 upper bound on the entry timestamp (exclusive).
    #[serde(default)]
    pub to: Option<String>,
}

/// Pagination args for `audit.list`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AuditPaginationDto {
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub offset: Option<u32>,
}

fn outcome_to_db_str(outcome: AuditOutcome) -> &'static str {
    match outcome {
        AuditOutcome::Applied => "applied",
        AuditOutcome::Ok => "ok",
        AuditOutcome::Refused => "refused",
        AuditOutcome::Failed => "failed",
        AuditOutcome::Paused => "paused",
    }
}

fn severity_to_db_str(severity: audit::Severity) -> &'static str {
    match severity {
        audit::Severity::Workflow => "workflow",
        audit::Severity::Diagnostic => "diagnostic",
    }
}

/// Parse the `outcome` column (constrained by the `audit_log_entry` CHECK
/// clause to `applied` | `refused` | `failed`) into the wider contract enum.
/// Falls back to `Applied` for any unrecognised value rather than failing the
/// whole list — a single malformed row must not break the audit view.
fn parse_outcome(s: &str) -> AuditOutcome {
    match s {
        "refused" => AuditOutcome::Refused,
        "failed" => AuditOutcome::Failed,
        "ok" => AuditOutcome::Ok,
        "paused" => AuditOutcome::Paused,
        _ => AuditOutcome::Applied,
    }
}

fn parse_actor(s: &str) -> AuditActor {
    match s {
        "system" => AuditActor::System,
        _ => AuditActor::User,
    }
}

/// Detail rendering data derived from a row's `trigger` + JSON `payload`.
///
/// `audit_log_entry` has no `detail` column — only a nullable JSON `payload`
/// (present for refusals and target resolution rows; `NULL` for successful
/// transitions).
///
/// **i18n (decision D23, upgraded 2026-07-04 — campaign task #45):** `text`
/// is the backend-composed English detail (refusal message from `payload`,
/// or the `trigger` label) and stays byte-stable in storage and export.
/// `code` + `params` are the STABLE identifiers the Audit Log frontend maps
/// to a Paraglide catalog message at DISPLAY time:
/// - refusal rows: `code = payload.refusal.code`,
///   `params = payload.refusal.params` (written by
///   `transition_use_case::record_refused` only when the pair unambiguously
///   identifies a message template);
/// - target-resolution rows: `code = trigger`
///   (`target.resolved` / `target.user_override`), `params = { query }`.
///
/// Rows without a code (or without the params their template needs) keep
/// rendering the stored English `text` — old rows are unchanged.
struct DerivedDetail {
    text: String,
    code: Option<String>,
    params: Option<std::collections::HashMap<String, String>>,
}

/// Convert a `payload.refusal.params`-style JSON object into the flat
/// string map the `AuditEntry` contract carries. Non-string values are
/// rendered via `to_string()` (defensive — the writer only stores strings).
fn params_map(value: &serde_json::Value) -> Option<std::collections::HashMap<String, String>> {
    let obj = value.as_object()?;
    Some(
        obj.iter()
            .map(|(k, v)| {
                let s = v.as_str().map_or_else(|| v.to_string(), str::to_owned);
                (k.clone(), s)
            })
            .collect(),
    )
}

fn derive_detail(trigger: &str, payload: Option<&str>) -> DerivedDetail {
    if let Some(raw) = payload {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) {
            if let Some(refusal) = value.get("refusal") {
                let text = refusal
                    .get("message")
                    .and_then(serde_json::Value::as_str)
                    .map_or_else(|| trigger.to_owned(), str::to_owned);
                let code =
                    refusal.get("code").and_then(serde_json::Value::as_str).map(str::to_owned);
                let params = refusal.get("params").and_then(params_map);
                return DerivedDetail { text, code, params };
            }
            if let Some(query) = value.get("query").and_then(serde_json::Value::as_str) {
                return DerivedDetail {
                    text: format!("{trigger} ({query})"),
                    code: Some(trigger.to_owned()),
                    params: Some(std::collections::HashMap::from([(
                        "query".to_owned(),
                        query.to_owned(),
                    )])),
                };
            }
        }
    }
    DerivedDetail { text: trigger.to_owned(), code: None, params: None }
}

fn row_to_entry(row: AuditLogRow) -> AuditEntry {
    let derived = derive_detail(&row.trigger, row.payload.as_deref());
    AuditEntry {
        id: row.audit_id,
        timestamp: row.at,
        event_type: row.trigger,
        entity_type: row.entity_type,
        entity_id: row.entity_id,
        from_state: row.from_state,
        to_state: row.to_state,
        actor: parse_actor(&row.actor),
        outcome: parse_outcome(&row.outcome),
        detail: derived.text,
        detail_code: derived.code,
        detail_params: derived.params,
    }
}

#[cfg(test)]
mod detail_tests {
    use super::derive_detail;

    #[test]
    fn refusal_payload_yields_code_params_and_english_fallback() {
        let payload = r#"{"refusal":{"code":"plan.required","message":"edge (project, ready -> prepared) requires an approved FilesystemPlan","params":{"entityType":"project","fromState":"ready","toState":"prepared"}},"attempted_to_state":"prepared"}"#;
        let d = derive_detail("project: ready -> prepared", Some(payload));
        assert_eq!(d.text, "edge (project, ready -> prepared) requires an approved FilesystemPlan");
        assert_eq!(d.code.as_deref(), Some("plan.required"));
        let params = d.params.expect("params present");
        assert_eq!(params.get("entityType").map(String::as_str), Some("project"));
        assert_eq!(params.get("fromState").map(String::as_str), Some("ready"));
        assert_eq!(params.get("toState").map(String::as_str), Some("prepared"));
    }

    #[test]
    fn legacy_refusal_payload_without_params_keeps_code_and_message() {
        // Rows written before the D23 upgrade: code + message, no params.
        let payload = r#"{"refusal":{"code":"transition.refused","message":"some legacy reason"}}"#;
        let d = derive_detail("trigger", Some(payload));
        assert_eq!(d.text, "some legacy reason");
        assert_eq!(d.code.as_deref(), Some("transition.refused"));
        assert!(d.params.is_none());
    }

    #[test]
    fn query_payload_yields_trigger_code_and_query_param() {
        let d = derive_detail("target.resolved", Some(r#"{"query":"M 31"}"#));
        assert_eq!(d.text, "target.resolved (M 31)");
        assert_eq!(d.code.as_deref(), Some("target.resolved"));
        assert_eq!(d.params.expect("params").get("query").map(String::as_str), Some("M 31"));
    }

    #[test]
    fn null_payload_falls_back_to_trigger_with_no_code() {
        let d = derive_detail("project: ready -> processing", None);
        assert_eq!(d.text, "project: ready -> processing");
        assert!(d.code.is_none());
        assert!(d.params.is_none());
    }
}

fn build_filter(
    filters: Option<AuditFilterDto>,
    pagination: Option<AuditPaginationDto>,
) -> AuditLogFilter {
    let f = filters.unwrap_or_default();
    let p = pagination.unwrap_or_default();
    AuditLogFilter {
        entity_type: f.entity_type,
        entity_id: f.entity_id,
        outcome: f.outcome.map(outcome_to_db_str).map(str::to_owned),
        severity: f.severity.map(severity_to_db_str).map(str::to_owned),
        // T120 (spec 030): reason_code filtering is not yet exposed on the
        // `AuditFilterDto` contract — no UI consumer needs it in this task.
        reason_code: None,
        from: f.from,
        to: f.to,
        search: f.search,
        limit: p.limit,
        offset: p.offset,
    }
}

/// `audit.list` — returns paginated audit entries read from `audit_log_entry`.
///
/// Applies a server-side default limit clamp (1..=500, default 100) to prevent
/// unbounded result sets over IPC.
///
/// # Errors
/// Returns `Err(ContractError)` on database failure.
#[tauri::command]
#[specta::specta]
pub async fn audit_list(
    state: State<'_, AppState>,
    filters: Option<AuditFilterDto>,
    pagination: Option<AuditPaginationDto>,
) -> Result<AuditListResponse, ContractError> {
    let pool = state.repo.pool();
    let mut filter = build_filter(filters, pagination);
    // Default limit clamp — same pattern as plans/read.rs:47.
    filter.limit = Some(filter.limit.unwrap_or(100).clamp(1, 500));

    let rows = list_audit_entries(pool, &filter).await.map_err(db_err)?;
    let total = count_audit_entries(pool, &filter).await.map_err(db_err)?;

    let entries = rows.into_iter().map(row_to_entry).collect();
    Ok(AuditListResponse { entries, total })
}

/// `audit.export` — export filtered audit entries as newline-delimited JSON to
/// a file (mirrors `log.export`). Streams backend-side; only the path and count
/// cross IPC.
///
/// The destination passes `fs_pathsafe::export_dest`, so an existing file is
/// refused rather than replaced and the write cannot leave the canonicalized
/// parent directory.
///
/// # Errors
/// Returns `Err(ContractError)` on a refused destination, database failure, or
/// filesystem failure.
#[tauri::command]
#[specta::specta]
pub async fn audit_export(
    state: State<'_, AppState>,
    file_path: String,
    filters: Option<AuditFilterDto>,
) -> Result<AuditExportResponse, ContractError> {
    use fs_pathsafe::export_dest::{validate_export_destination, ExportWriteError};
    use std::io::Write;
    use std::path::Path;

    let pool = state.repo.pool();
    let filter = build_filter(filters, None);

    let dest = Path::new(&file_path);
    let validated = validate_export_destination(dest).map_err(|rejected| {
        ContractError::internal(format!("{}: {}", rejected.message(), dest.display()))
    })?;

    let rows = list_audit_entries(pool, &filter).await.map_err(db_err)?;
    let entries: Vec<AuditEntry> = rows.into_iter().map(row_to_entry).collect();
    let count = entries.len();

    let bytes = validated
        .write_atomically(|file| {
            let mut writer = std::io::BufWriter::new(file);
            for entry in &entries {
                serde_json::to_writer(&mut writer, entry).map_err(std::io::Error::other)?;
                writer.write_all(b"\n")?;
            }
            writer.flush()
        })
        .map_err(|e| match e {
            ExportWriteError::Rejected(rejected) => ContractError::internal(format!(
                "{}: {}",
                rejected.message(),
                validated.path().display()
            )),
            ExportWriteError::Io(e) => ContractError::internal(format!("write error: {e}")),
        })?;

    Ok(AuditExportResponse {
        file_path: validated.path().to_string_lossy().into_owned(),
        count,
        bytes,
    })
}

// ── entity.names ─────────────────────────────────────────────────────────────

/// `entity.names` — batch display-name lookup for `(entityType, entityId)` refs.
///
/// Resolves project, plan, and target refs in three IN-clause DB queries instead
/// of one IPC round-trip per unseen ref.  Session names are not included —
/// callers read those from the inventory-sources query that is already in the
/// session store.
///
/// Absent ids (not in the DB) are simply omitted from the response map; the
/// frontend hook treats absence as "unresolved".
///
/// # Errors
/// Returns `Err(ContractError)` with code `internal.database` on query failure.
#[tauri::command]
#[specta::specta]
pub async fn entity_names(
    state: State<'_, AppState>,
    refs: Vec<EntityNameRef>,
) -> Result<EntityNamesResponse, ContractError> {
    app_core::entity_names::entity_names(state.repo.pool(), refs).await
}
