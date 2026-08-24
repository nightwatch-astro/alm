// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! LifecycleRepository — async trait for spec 002 lifecycle operations.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::Arc;

use audit_types::{
    EventPublisher, LifecycleTransitionApplied, Source, TOPIC_LIFECYCLE_TRANSITION_APPLIED,
};
use domain_core::ids::{AuditId, EntityId, Timestamp};
use domain_core::lifecycle::data_asset::EntityType;
use domain_core::lifecycle::provenance::{ProvenanceTag, ProvenancedValue};
use serde_json::Value;
use uuid::Uuid;

use crate::repositories::provenance::load_provenance;
use persistence_core::{DbError, DbResult};

/// Ledger row — omits provenance fields per FR-006.
///
/// Columns map 1-to-1 to `ledger_view` (migration 0004).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerRow {
    pub entity_id: EntityId,
    pub entity_type: EntityType,
    pub current_state: String,
    pub title: Option<String>,
    pub path: Option<String>,
    pub project_id: Option<EntityId>,
    pub updated_at: Option<String>,
}

/// Filter for ledger list queries.
///
/// All fields are AND-combined. `entity_types` and `states` are IN clauses
/// (empty vec = no filter, matching the absence semantics of `None`).
#[derive(Clone, Debug, Default)]
pub struct LedgerFilter {
    /// One or more entity_type tags to include.
    pub entity_types: Vec<EntityType>,
    /// One or more lifecycle states to include.
    pub states: Vec<String>,
    /// Restrict to assets owned by this project (`project_id = ?`).
    pub project_id: Option<EntityId>,
    /// RFC 3339 lower bound on `updated_at` (inclusive).
    pub updated_after: Option<String>,
    /// RFC 3339 upper bound on `updated_at` (inclusive).
    pub updated_before: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// Full asset detail including hydrated provenance per field path.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetDetail {
    pub entity_id: EntityId,
    pub entity_type: EntityType,
    pub current_state: String,
    /// Map keyed by field_path → typed `ProvenancedValue<Value>`. Inline
    /// `history` is bounded at 10 entries per field (spec 002 amendment
    /// B-provenance-retention); `history_truncated` is set per field when
    /// older entries exist in the archive.
    pub provenance: HashMap<String, ProvenancedValue<Value>>,
    /// True when at least one field's archive holds more than the inline
    /// retention window.
    pub history_truncated: bool,
}

/// Request to apply a lifecycle transition.
#[derive(Clone, Debug)]
pub struct TransitionRequest {
    pub entity_id: EntityId,
    pub entity_type: EntityType,
    pub from_state: String,
    pub to_state: String,
    pub trigger: String,
    pub actor: String,
    pub request_id: EntityId,
}

/// Record of a successfully applied transition.
#[derive(Clone, Debug)]
pub struct TransitionRecord {
    pub audit_id: AuditId,
    pub entity_id: EntityId,
    pub entity_type: EntityType,
    pub from_state: String,
    pub to_state: String,
    pub applied_at: Timestamp,
}

/// Noop sentinel: returned when from_state == to_state.
pub const NOOP_SENTINEL: &str = "__noop__";

pub trait LifecycleRepository {
    fn load_asset_detail(
        &self,
        entity_id: EntityId,
        entity_type: EntityType,
    ) -> impl std::future::Future<Output = DbResult<AssetDetail>> + Send;

    fn list_assets_ledger(
        &self,
        filter: LedgerFilter,
    ) -> impl std::future::Future<Output = DbResult<Vec<LedgerRow>>> + Send;

    fn record_transition(
        &self,
        transition: TransitionRequest,
    ) -> impl std::future::Future<Output = DbResult<TransitionRecord>> + Send;

    /// Persist a `refused` audit row for a transition the use case refused
    /// before mutation. The row records `actor`, `from_state`,
    /// `to_state = NULL` (invariant data-model.md:376), and the refusal
    /// code + message in `payload`. Required by data-model.md §242 and §378
    /// — refused transitions MUST be durable, not just observable via the
    /// response envelope.
    ///
    /// `refusal_params` (D23 upgrade, campaign task #45): optional structured
    /// display parameters (a flat JSON object of string values) stored as
    /// `payload.refusal.params`. When present they identify the exact message
    /// template behind `refusal_code`, so the frontend can localize the
    /// refusal detail at display time instead of showing the write-time
    /// English `refusal_message`. Rows without params keep the English
    /// message as their only rendering (fallback path).
    fn record_refused_transition(
        &self,
        transition: TransitionRequest,
        refusal_code: &'static str,
        refusal_message: &str,
        refusal_params: Option<serde_json::Value>,
    ) -> impl std::future::Future<Output = DbResult<AuditId>> + Send;

    /// Read the winning `ProvenanceTag` per `field_path` for an entity.
    ///
    /// Returns a map keyed by `field_path`. Fields with no provenance rows
    /// are simply absent from the map (callers MUST treat absence as
    /// "no reviewed origin"). Used by the action-bound review gate
    /// (FR-009/FR-010) — see
    /// `domain_core::lifecycle::action_review_requirement`.
    fn field_origins(
        &self,
        entity_id: EntityId,
        entity_type: EntityType,
    ) -> impl std::future::Future<Output = DbResult<HashMap<String, ProvenanceTag>>> + Send;
}

// ── Table name helpers ────────────────────────────────────────────────────────

fn table_for(entity_type: EntityType) -> &'static str {
    match entity_type {
        EntityType::FileRecord => "file_record",
        // FR-019 / T052: canonical project lifecycle lives in `projects.lifecycle`
        // (spec-008 table). The legacy spec-002 `project.state` was removed in
        // migration 0036.
        EntityType::Project => "projects",
        // Plan is an alias for FilesystemPlan in the contract layer
        EntityType::FilesystemPlan | EntityType::Plan => "filesystem_plan",
        EntityType::PreparedSource => "prepared_source_view",
        // Projection staleness is stored in processing_artifact
        EntityType::ProcessingArtifact | EntityType::Projection => "processing_artifact",
        // DataSource and LibraryRoot both map to library_root table
        EntityType::DataSource | EntityType::LibraryRoot => "library_root",
        // Spec 030 T120: Settings/Protection/Equipment are audit-only tags
        // (no lifecycle state table) written via `EventBus::write_audit`,
        // never through `record_transition`'s CAS state-column path. Framing
        // (spec 008 Q27) and Calibration (#1120) join the same audit-only
        // precedent.
        EntityType::Settings
        | EntityType::Protection
        | EntityType::Equipment
        | EntityType::Framing
        | EntityType::Calibration => {
            unreachable!(
                "{entity_type:?} has no lifecycle state table; it never flows through record_transition"
            )
        }
    }
}

fn state_column_for(entity_type: EntityType) -> &'static str {
    match entity_type {
        EntityType::ProcessingArtifact | EntityType::Projection => "staleness",
        // FR-019 / T052: projects table uses `lifecycle` not `state`.
        EntityType::Project => "lifecycle",
        _ => "state",
    }
}

/// FR-003 (#713): resolve the project whose dependents (research.md §6 —
/// `processing_artifact`/`prepared_source_view` rows sharing this
/// `project_id`) the StalePropagator should recompute. Trivial for a Project
/// transition (its own id); a lookup for entity types that themselves carry a
/// `project_id` FK; `None` otherwise (no propagation redesign — session-level
/// dependents are out of scope here).
async fn resolve_transition_project_id(
    pool: &sqlx::SqlitePool,
    entity_type: EntityType,
    id_str: &str,
) -> Option<String> {
    match entity_type {
        EntityType::Project => Some(id_str.to_owned()),
        EntityType::ProcessingArtifact | EntityType::Projection => {
            sqlx::query_scalar::<_, Option<String>>(
                "SELECT project_id FROM processing_artifact WHERE id = ?",
            )
            .bind(id_str)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .flatten()
        }
        EntityType::PreparedSource => sqlx::query_scalar::<_, Option<String>>(
            "SELECT project_id FROM prepared_source_view WHERE id = ?",
        )
        .bind(id_str)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .flatten(),
        _ => None,
    }
}

/// FR-003 (#713): marks a project's dependent `processing_artifact`/
/// `prepared_source_view` rows stale (research.md §6 — `ProjectManifest
/// depends_on Project`). DB-boundary home for
/// `audit::stale_propagator::resolve_project_dependents_hook`, which has no
/// raw-SQL access of its own (check-db-boundary.sh).
///
/// # Errors
/// Returns `DbError` if either `UPDATE` fails.
pub async fn mark_project_dependents_stale(
    pool: &sqlx::SqlitePool,
    project_id: &str,
) -> DbResult<()> {
    sqlx::query(
        "UPDATE processing_artifact SET staleness = 'stale' \
         WHERE project_id = ? AND staleness != 'stale'",
    )
    .bind(project_id)
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE prepared_source_view SET state = 'stale' \
         WHERE project_id = ? AND state NOT IN ('stale', 'retired')",
    )
    .bind(project_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Raw `ledger_view` row, decoded by column name via `#[derive(FromRow)]`
/// instead of a hand-rolled positional-tuple `query_as` + manual per-field
/// map (Tier-3 dedup/abstraction audit). `entity_id`/`entity_type` are kept
/// as plain strings here — parsing them into typed `EntityId`/`EntityType`
/// is a separate domain concern handled in `list_assets_ledger`.
#[derive(sqlx::FromRow)]
struct RawLedgerRow {
    entity_type: String,
    entity_id: String,
    state: String,
    title: Option<String>,
    path: Option<String>,
    project_id: Option<String>,
    updated_at: Option<String>,
}

// ── SQLite-backed implementation ──────────────────────────────────────────────

/// SQLite-backed implementation of [`LifecycleRepository`].
pub struct SqliteLifecycleRepository {
    pool: sqlx::SqlitePool,
    bus: Arc<dyn EventPublisher>,
}

impl SqliteLifecycleRepository {
    /// `bus` is generic over [`EventPublisher`] (not the concrete `audit::EventBus`)
    /// so this crate never depends on `audit` — `audit` depends on
    /// `persistence_db` for its `events`-table SQL, so the reverse edge would
    /// cycle. Callers pass their `EventBus` value unchanged; it implements
    /// `EventPublisher`.
    #[must_use]
    pub fn new(pool: sqlx::SqlitePool, bus: impl EventPublisher + 'static) -> Self {
        Self { pool, bus: Arc::new(bus) }
    }

    #[must_use]
    pub fn pool(&self) -> &sqlx::SqlitePool {
        &self.pool
    }
}

impl LifecycleRepository for SqliteLifecycleRepository {
    async fn load_asset_detail(
        &self,
        entity_id: EntityId,
        entity_type: EntityType,
    ) -> DbResult<AssetDetail> {
        let table = table_for(entity_type);
        let state_col = state_column_for(entity_type);
        let id_str = entity_id.as_uuid().to_string();

        // Runtime-checked query: DATABASE_URL is not set at compile time for this crate.
        // AssertSqlSafe: table and state_col come from `table_for`/`state_column_for` which
        // return only static &str values — never user input.
        let row: Option<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {state_col} FROM {table} WHERE id = ?"
        )))
        .bind(&id_str)
        .fetch_optional(&self.pool)
        .await?;

        let current_state = row
            .map(|(s,)| s)
            .ok_or_else(|| DbError::NotFound(format!("{entity_type:?} {entity_id}")))?;

        // Hydrate provenance from provenance_history_archive (spec 002 Phase 4).
        let (provenance, history_truncated) =
            load_provenance(&self.pool, entity_id, entity_type.as_str()).await?;

        Ok(AssetDetail { entity_id, entity_type, current_state, provenance, history_truncated })
    }

    async fn list_assets_ledger(&self, filter: LedgerFilter) -> DbResult<Vec<LedgerRow>> {
        // Single-query implementation backed by the `ledger_view` materialised
        // view (migration 0004). All filter clauses use `?` placeholders;
        // user-supplied strings are never interpolated into the SQL text.

        let mut sql = String::from(
            "SELECT entity_type, entity_id, state, title, path, project_id, updated_at \
             FROM ledger_view",
        );
        let mut where_clauses: Vec<String> = Vec::new();
        // Track bind values in a typed enum-ish way: each tuple is (binder fn).
        // We bind in two passes (build then execute) since sqlx's QueryAs is
        // not Send across `await` between binds in a loop without it.
        let mut string_binds: Vec<String> = Vec::new();

        if !filter.entity_types.is_empty() {
            let placeholders: Vec<&str> = filter.entity_types.iter().map(|_| "?").collect();
            where_clauses.push(format!("entity_type IN ({})", placeholders.join(",")));
            for et in &filter.entity_types {
                string_binds.push(et.as_str().to_owned());
            }
        }

        if !filter.states.is_empty() {
            let placeholders: Vec<&str> = filter.states.iter().map(|_| "?").collect();
            where_clauses.push(format!("state IN ({})", placeholders.join(",")));
            for s in &filter.states {
                string_binds.push(s.clone());
            }
        }

        if let Some(pid) = filter.project_id {
            where_clauses.push("project_id = ?".to_owned());
            string_binds.push(pid.as_uuid().to_string());
        }

        if let Some(ref ts) = filter.updated_after {
            where_clauses.push("updated_at >= ?".to_owned());
            string_binds.push(ts.clone());
        }

        if let Some(ref ts) = filter.updated_before {
            where_clauses.push("updated_at <= ?".to_owned());
            string_binds.push(ts.clone());
        }

        if !where_clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&where_clauses.join(" AND "));
        }

        // Stable order so limit/offset is meaningful.
        sql.push_str(" ORDER BY updated_at DESC, entity_id ASC");

        if let Some(limit) = filter.limit {
            let _ = write!(sql, " LIMIT {limit}");
            if let Some(offset) = filter.offset {
                let _ = write!(sql, " OFFSET {offset}");
            }
        } else if let Some(offset) = filter.offset {
            // SQLite requires LIMIT when OFFSET is present.
            let _ = write!(sql, " LIMIT -1 OFFSET {offset}");
        }

        // AssertSqlSafe: every dynamic portion above is either a static
        // identifier, a fixed `?` placeholder, or an integer literal derived
        // from typed `u32` filter fields. User-supplied strings flow through
        // `bind` calls below. Named-column `FromRow` decoding (vs. a
        // positional tuple) keeps this immune to the SELECT list being
        // reordered (T1-d Tier-3).
        let mut q = sqlx::query_as::<_, RawLedgerRow>(sqlx::AssertSqlSafe(sql));
        for v in &string_binds {
            q = q.bind(v);
        }

        let raw = q.fetch_all(&self.pool).await?;

        let mut rows = Vec::with_capacity(raw.len());
        for r in raw {
            let entity_id = EntityId::from_uuid(parse_stored_uuid(&r.entity_id)?);
            let entity_type = parse_entity_type(&r.entity_type);
            let project_id = match r.project_id {
                Some(s) => Some(EntityId::from_uuid(parse_stored_uuid(&s)?)),
                None => None,
            };
            rows.push(LedgerRow {
                entity_id,
                entity_type,
                current_state: r.state,
                title: r.title,
                path: r.path,
                project_id,
                updated_at: r.updated_at,
            });
        }

        Ok(rows)
    }

    async fn record_transition(&self, transition: TransitionRequest) -> DbResult<TransitionRecord> {
        // Noop: from_state == to_state — return early without writing audit row.
        if transition.from_state == transition.to_state {
            return Ok(TransitionRecord {
                audit_id: AuditId::new(),
                entity_id: transition.entity_id,
                entity_type: transition.entity_type,
                from_state: transition.from_state.clone(),
                to_state: transition.to_state.clone(),
                applied_at: Timestamp::now_utc(),
            });
        }

        let table = table_for(transition.entity_type);
        let state_col = state_column_for(transition.entity_type);
        let id_str = transition.entity_id.as_uuid().to_string();
        let audit_id = AuditId::new();
        let applied_at = Timestamp::now_utc();

        let applied_at_str = applied_at.to_iso();

        // Atomic CAS: update only if current state matches expected from_state.
        let mut tx = self.pool.begin().await?;

        // AssertSqlSafe: table and state_col are static strings from internal helper functions.
        let rows_affected = sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE {table} SET {state_col} = ? WHERE id = ? AND {state_col} = ?",
        )))
        .bind(&transition.to_state)
        .bind(&id_str)
        .bind(&transition.from_state)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        if rows_affected == 0 {
            tx.rollback().await?;
            return Err(DbError::invalid_state(format!(
                "{:?} {} not in state '{}' (CAS failed)",
                transition.entity_type, transition.entity_id, transition.from_state
            )));
        }

        // FR-020 / T053: when a Project entity transitions OUT of "blocked",
        // clear the blocked_reason_kind/note columns in the same transaction
        // so stale reason data is never visible after an unblock.
        if transition.entity_type == EntityType::Project && transition.from_state == "blocked" {
            sqlx::query(
                "UPDATE projects SET blocked_reason_kind = NULL, blocked_reason_note = NULL \
                 WHERE id = ?",
            )
            .bind(&id_str)
            .execute(&mut *tx)
            .await?;
        }

        // Insert audit log entry.
        let audit_id_str = audit_id.as_uuid().to_string();
        let request_id_str = transition.request_id.as_uuid().to_string();
        sqlx::query(
            "INSERT INTO audit_log_entry \
             (audit_id, entity_type, entity_id, from_state, to_state, trigger, actor, \
              outcome, severity, request_id, at, payload) \
             VALUES (?, ?, ?, ?, ?, ?, ?, 'applied', 'workflow', ?, ?, NULL)",
        )
        .bind(&audit_id_str)
        .bind(transition.entity_type.as_str())
        .bind(&id_str)
        .bind(&transition.from_state)
        .bind(&transition.to_state)
        .bind(&transition.trigger)
        .bind(&transition.actor)
        .bind(&request_id_str)
        .bind(&applied_at_str)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        let project_id =
            resolve_transition_project_id(&self.pool, transition.entity_type, &id_str).await;

        // Publish event after successful commit (fire-and-forget — publish
        // failure must never roll back or fail an already-committed transition).
        let event_payload = serde_json::to_value(LifecycleTransitionApplied {
            entity_type: transition.entity_type,
            entity_id: transition.entity_id.to_string(),
            from_state: transition.from_state.clone(),
            to_state: transition.to_state.clone(),
            actor: transition.actor.clone(),
            at: applied_at,
            project_id,
        })
        .unwrap_or(Value::Null);
        self.bus.publish(TOPIC_LIFECYCLE_TRANSITION_APPLIED, Source::System, event_payload).await;

        Ok(TransitionRecord {
            audit_id,
            entity_id: transition.entity_id,
            entity_type: transition.entity_type,
            from_state: transition.from_state,
            to_state: transition.to_state,
            applied_at,
        })
    }

    async fn record_refused_transition(
        &self,
        transition: TransitionRequest,
        refusal_code: &'static str,
        refusal_message: &str,
        refusal_params: Option<serde_json::Value>,
    ) -> DbResult<AuditId> {
        // Per data-model.md §AuditLogEntry invariants:
        //   - `outcome == refused` MUST have `to_state == null`
        //   - the row is NOT joined to a state mutation (no CAS, no UPDATE)
        // The original actor is preserved (the use case has already done the
        // actor=system policy check; for that specific refusal the actor IS
        // system, which matches §378 of the spec).
        let audit_id = AuditId::new();
        let audit_id_str = audit_id.as_uuid().to_string();
        let entity_id_str = transition.entity_id.as_uuid().to_string();
        let request_id_str = transition.request_id.as_uuid().to_string();
        // RFC3339 format cannot fail for a valid OffsetDateTime; panic on
        // the impossible case rather than silently corrupting audit-table
        // time ordering with a 1970 epoch sentinel.
        let at_str = Timestamp::now_iso();

        // Payload carries the refusal code + message so consumers reading the
        // audit table can reconstruct the refusal envelope without joining
        // against the response log. JSON form matches the contract's
        // dotted-form error codes. `params` (when provided) additionally
        // carries the structured display parameters for catalog-based
        // localization of the detail text (D23 upgrade) — the stored English
        // `message` remains the durable fallback.
        let mut refusal = serde_json::json!({
            "code": refusal_code,
            "message": refusal_message,
        });
        if let (Some(obj), Some(params)) = (refusal.as_object_mut(), refusal_params) {
            obj.insert("params".to_owned(), params);
        }
        let payload = serde_json::json!({
            "refusal": refusal,
            "attempted_to_state": transition.to_state,
        })
        .to_string();

        sqlx::query(
            "INSERT INTO audit_log_entry \
             (audit_id, entity_type, entity_id, from_state, to_state, trigger, actor, \
              outcome, severity, request_id, at, payload) \
             VALUES (?, ?, ?, ?, NULL, ?, ?, 'refused', 'workflow', ?, ?, ?)",
        )
        .bind(&audit_id_str)
        .bind(transition.entity_type.as_str())
        .bind(&entity_id_str)
        .bind(&transition.from_state)
        .bind(&transition.trigger)
        .bind(&transition.actor)
        .bind(&request_id_str)
        .bind(&at_str)
        .bind(&payload)
        .execute(&self.pool)
        .await?;

        Ok(audit_id)
    }

    async fn field_origins(
        &self,
        entity_id: EntityId,
        entity_type: EntityType,
    ) -> DbResult<HashMap<String, ProvenanceTag>> {
        // Provenance rows are stored under the SQL table's canonical asset
        // tag (see `table_for`). Reuse `load_provenance` so origin
        // resolution stays in one place (priority + superseded_by rules).
        let asset_type = provenance_asset_type(entity_type);
        let (per_field, _truncated) = load_provenance(&self.pool, entity_id, asset_type).await?;
        Ok(per_field.into_iter().map(|(path, pv)| (path, pv.origin)).collect())
    }
}

/// Map an `EntityType` to the canonical `asset_type` tag used for provenance
/// archive rows. Mirrors `table_for` for families that share a storage table.
fn provenance_asset_type(entity_type: EntityType) -> &'static str {
    match entity_type {
        EntityType::Plan => "filesystem_plan",
        EntityType::LibraryRoot => "data_source",
        other => other.as_str(),
    }
}

/// Parses a UUID read back from a stored id column.
///
/// A malformed UUID here is row *corruption*, not a missing row — mapping it
/// to `DbError::NotFound` (as this used to) mislabels a decode failure as
/// "no such entity". `sqlx::Error::Decode` is the closest existing fit for a
/// bad-value-in-storage error without adding a new `DbError` variant; the
/// ideal long-term shape is a dedicated `DbError::Decode` variant (T1-d).
fn parse_stored_uuid(s: &str) -> DbResult<Uuid> {
    Uuid::parse_str(s)
        .map_err(|e| DbError::Database(sqlx::Error::Decode(format!("bad uuid {s}: {e}").into())))
}

fn parse_entity_type(s: &str) -> EntityType {
    match s {
        "file_record" => EntityType::FileRecord,
        "filesystem_plan" | "plan" => EntityType::FilesystemPlan,
        "prepared_source" => EntityType::PreparedSource,
        "processing_artifact" => EntityType::ProcessingArtifact,
        "data_source" | "library_root" => EntityType::DataSource,
        "projection" => EntityType::Projection,
        // "project" and any unknown string → Project
        _ => EntityType::Project,
    }
}

// ── In-memory stub ────────────────────────────────────────────────────────────

/// In-memory stub — compiles without sqlx; used by unit tests.
#[derive(Default)]
pub struct InMemoryLifecycleRepository;

// `async` here is the trait's contract, not this stub's need: the sqlx
// implementation awaits, a stub that returns a constant has nothing to await.
// Returning `std::future::ready` instead, as the lint suggests, would run each
// body at call time rather than at poll time.
//
// `allow` rather than `expect`, and paired with `unknown_lints`, because clippy
// added `unused_async_trait_impl` in 1.98: an expectation would go unfulfilled on
// older toolchains, and naming the lint at all is itself unknown to them.
#[allow(unknown_lints, clippy::unused_async_trait_impl)]
impl LifecycleRepository for InMemoryLifecycleRepository {
    async fn load_asset_detail(
        &self,
        entity_id: EntityId,
        entity_type: EntityType,
    ) -> DbResult<AssetDetail> {
        Ok(AssetDetail {
            entity_id,
            entity_type,
            current_state: "unknown".to_owned(),
            provenance: HashMap::new(),
            history_truncated: false,
        })
    }

    async fn list_assets_ledger(&self, _filter: LedgerFilter) -> DbResult<Vec<LedgerRow>> {
        Ok(Vec::new())
    }

    async fn record_transition(&self, transition: TransitionRequest) -> DbResult<TransitionRecord> {
        if transition.from_state == transition.to_state {
            return Err(DbError::NotFound(format!(
                "noop: {} is already in state {}",
                transition.entity_id, transition.from_state
            )));
        }
        Ok(TransitionRecord {
            audit_id: AuditId::new(),
            entity_id: transition.entity_id,
            entity_type: transition.entity_type,
            from_state: transition.from_state,
            to_state: transition.to_state,
            applied_at: Timestamp::now_utc(),
        })
    }

    async fn record_refused_transition(
        &self,
        _transition: TransitionRequest,
        _refusal_code: &'static str,
        _refusal_message: &str,
        _refusal_params: Option<serde_json::Value>,
    ) -> DbResult<AuditId> {
        // In-memory stub: no durable storage. Tests that need to assert the
        // refused audit row exists should use `SqliteLifecycleRepository`.
        Ok(AuditId::new())
    }

    async fn field_origins(
        &self,
        _entity_id: EntityId,
        _entity_type: EntityType,
    ) -> DbResult<HashMap<String, ProvenanceTag>> {
        // The in-memory stub does not store provenance; tests that need a
        // populated map should use `SqliteLifecycleRepository` or wrap this
        // stub with a custom override.
        Ok(HashMap::new())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use domain_core::ids::EntityId;
    use domain_core::lifecycle::data_asset::EntityType;

    use super::{
        InMemoryLifecycleRepository, LedgerFilter, LifecycleRepository, TransitionRequest,
    };

    #[tokio::test]
    async fn stub_load_returns_unknown_state() {
        let repo = InMemoryLifecycleRepository;
        let id = EntityId::new();
        let detail = repo.load_asset_detail(id, EntityType::Project).await.unwrap();
        assert_eq!(detail.current_state, "unknown");
    }

    #[tokio::test]
    async fn stub_record_transition_succeeds() {
        let repo = InMemoryLifecycleRepository;
        let entity_id = EntityId::new();
        let record = repo
            .record_transition(TransitionRequest {
                entity_id,
                entity_type: EntityType::Project,
                from_state: "ready".to_owned(),
                to_state: "prepared".to_owned(),
                trigger: "Prepared".to_owned(),
                actor: "user".to_owned(),
                request_id: EntityId::new(),
            })
            .await
            .unwrap();
        assert_eq!(record.from_state, "ready");
        assert_eq!(record.to_state, "prepared");
    }

    #[tokio::test]
    async fn stub_list_assets_returns_empty() {
        let repo = InMemoryLifecycleRepository;
        let rows = repo.list_assets_ledger(LedgerFilter::default()).await.unwrap();
        assert!(rows.is_empty());
    }

    /// T1-d: a malformed stored UUID is row corruption, not a missing row.
    /// Previously this path mapped to `DbError::NotFound`, mislabeling a
    /// decode failure as "no such entity"; it must now surface as a
    /// non-`NotFound` variant instead.
    #[tokio::test]
    async fn list_assets_ledger_reports_corrupt_uuid_as_non_notfound() {
        let db = persistence_core::Database::in_memory().await.expect("in-memory connect");
        db.migrate().await.expect("migrations");
        let bus = audit::bus::EventBus::new(db.pool().clone(), 16);
        let repo = super::SqliteLifecycleRepository::new(db.pool().clone(), bus);

        sqlx::query(
            "INSERT INTO library_root \
             (id, label, current_path, kind, state, last_seen_at, created_at) \
             VALUES ('not-a-uuid', 'lr', '/tmp/lr', 'local', 'active', \
                     '2026-05-01T00:00:00Z', '2026-05-01T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        let err = repo.list_assets_ledger(LedgerFilter::default()).await.unwrap_err();
        assert!(
            !matches!(err, persistence_core::DbError::NotFound(_)),
            "corrupt uuid must not be reported as NotFound: {err:?}"
        );
    }
}
