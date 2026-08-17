// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Async ingest target-resolution queue (spec 035, US4 — group ingested images
//! by resolved target).
//!
//! During ingest, each image that carries a FITS `OBJECT` header value must be
//! associated with a canonical target. Per FR-013 this MUST NOT block ingest:
//!
//! - A cache/seed hit ([`targeting_resolver::cache::get_by_normalized`])
//!   associates the image inline (`state = resolved`).
//! - A miss enqueues a `pending` row ([`enqueue`]); a background drain
//!   ([`resolve_pending`]) later runs the cache-first → SIMBAD resolve flow and
//!   associates the image, or leaves it `unresolved` (retryable, `attempts += 1`).
//!
//! Matching is exact-normalized only (FR-008); a non-matching / ambiguous
//! `OBJECT` value stays `unresolved` rather than being guessed, and coordinates
//! are never fabricated (FR-009).
//!
//! ## Image↔target association model
//!
//! The association is the `ingest_resolution` row itself: `image_id`
//! (FK → `file_record`) ↔ `target_id` (FK → `canonical_target`), set when the
//! row reaches `state = resolved`. This is the spec-035 link; it intentionally
//! does NOT touch the legacy spec-013 `acquisition_session.target_id` /
//! `projects.target_id` columns (a different table + id space).
//!
//! ## Constitution
//!
//! - §I/§III: metadata/identity only; no image files are written or processed.
//! - §V: SQLite is the durable record; the queue state is explicit
//!   (pending/resolved/unresolved) and never silently mis-assigns (§II).

use audit::event_bus::{TargetResolveBatchCompleted, TargetResolved};
use audit::{EventBus, Source};
use domain_core::ids::Timestamp;
use persistence_targets::repositories::q_targets_ingest as repo;
use sqlx::SqlitePool;

use app_core_errors::db_err;
use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use targeting::normalize::normalize;
use targeting_resolver::cache::{self, CachedTarget};
use targeting_resolver::{ResolveError, Resolver};

// ── Error ─────────────────────────────────────────────────────────────────────

/// Map a `targeting_resolver::cache::CacheError` to a `ContractError`.
///
/// Delegates `Persistence(DbError)` through the canonical `db_err` so
/// `NotFound` stays `Blocking`; maps every other variant as Fatal/retryable.
#[allow(clippy::needless_pass_by_value)]
fn cache_err(e: targeting_resolver::cache::CacheError) -> ContractError {
    use targeting_resolver::cache::CacheError;
    match e {
        CacheError::Persistence(db) => db_err(db),
        other => ContractError::new(
            ErrorCode::InternalDatabase,
            format!("{other}"),
            ErrorSeverity::Fatal,
            true,
        ),
    }
}

// ── Row / outcome types ─────────────────────────────────────────────────────

/// Lifecycle state of an ingest-resolution row (mirrors the `state` CHECK).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IngestState {
    Pending,
    Resolved,
    Unresolved,
}

impl IngestState {
    #[must_use]
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Resolved => "resolved",
            Self::Unresolved => "unresolved",
        }
    }
}

/// Result of associating one image's `OBJECT` value during ingest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssociateOutcome {
    /// Cache/seed hit: associated inline to this canonical target id.
    ResolvedInline(String),
    /// No `OBJECT` value (or blank): nothing to resolve.
    NoObject,
    /// Cache miss: enqueued a `pending` row for the background drain.
    Enqueued,
}

// ── enqueue / inline association (T025 + T026 entry point) ──────────────────────

/// Insert a `pending` ingest-resolution row for `(image_id, object_raw)`.
///
/// Idempotent per `(image_id, object_raw)`: a duplicate enqueue does not create
/// a second pending row. Returns the row id.
///
/// # Errors
///
/// Returns [`ContractError`] (`internal.database`) on query failure.
pub async fn enqueue(
    pool: &SqlitePool,
    image_id: &str,
    object_raw: &str,
) -> Result<String, ContractError> {
    // Reuse an existing non-terminal row for the same (image, object) if present.
    if let Some(id) =
        repo::find_ingest_resolution_id(pool, image_id, object_raw).await.map_err(db_err)?
    {
        return Ok(id);
    }

    let id = domain_core::ids::new_id();
    repo::insert_ingest_resolution(pool, &id, image_id, object_raw, "pending", None)
        .await
        .map_err(db_err)?;
    Ok(id)
}

/// Ingest entry point (T026): associate an image's `OBJECT` value with a
/// canonical target without blocking ingest (FR-013).
///
/// - Blank/empty `object_raw` → [`AssociateOutcome::NoObject`].
/// - Cache/seed hit (exact normalized alias) → write a `resolved`
///   ingest-resolution row linking `image_id` → the cached target, emit
///   `target.resolved`, and return [`AssociateOutcome::ResolvedInline`].
/// - Miss → enqueue a `pending` row ([`AssociateOutcome::Enqueued`]) for the
///   background drain ([`resolve_pending`]).
///
/// `bus` is optional so callers without an event bus (e.g. a pure batch import)
/// can still associate; when present, an inline resolve emits `target.resolved`.
///
/// # Errors
///
/// Returns [`ContractError`] on a local database failure.
pub async fn associate_or_enqueue(
    pool: &SqlitePool,
    bus: Option<&EventBus>,
    image_id: &str,
    object_raw: &str,
) -> Result<AssociateOutcome, ContractError> {
    let trimmed = object_raw.trim();
    if trimmed.is_empty() {
        return Ok(AssociateOutcome::NoObject);
    }

    let norm = normalize(trimmed);
    if let Some(target) = cache::get_by_normalized(pool, &norm).await.map_err(cache_err)? {
        // Cache/seed hit → associate inline.
        write_resolved_row(pool, image_id, trimmed, &target.id.to_string()).await?;
        if let Some(bus) = bus {
            emit_resolved(bus, &target, Some(trimmed)).await;
        }
        return Ok(AssociateOutcome::ResolvedInline(target.id.to_string()));
    }

    enqueue(pool, image_id, trimmed).await?;
    Ok(AssociateOutcome::Enqueued)
}

/// Upsert a `resolved` ingest-resolution row linking `image_id` → `target_id`.
async fn write_resolved_row(
    pool: &SqlitePool,
    image_id: &str,
    object_raw: &str,
    target_id: &str,
) -> Result<(), ContractError> {
    let existing =
        repo::find_ingest_resolution_id(pool, image_id, object_raw).await.map_err(db_err)?;

    if let Some(id) = existing {
        repo::mark_ingest_resolution_resolved(pool, &id, target_id).await.map_err(db_err)?;
    } else {
        repo::insert_ingest_resolution(
            pool,
            &domain_core::ids::new_id(),
            image_id,
            object_raw,
            "resolved",
            Some(target_id),
        )
        .await
        .map_err(db_err)?;
    }
    Ok(())
}

// ── Background drain (T025) ─────────────────────────────────────────────────────

/// Summary of one [`resolve_pending`] drain pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DrainSummary {
    pub considered: usize,
    pub resolved: usize,
    pub unresolved: usize,
    /// Rows left `pending` due to a transient/offline condition (no `attempts`
    /// increment) — retried on the next drain pass (FIX-4).
    pub pending: usize,
}

/// Drain the pending ingest-resolution queue, resolving each row via the
/// cache-first → injected [`Resolver`] flow (T020 semantics).
///
/// For each `pending` row:
/// - cache/seed hit OR a successful resolver call → upsert into the cache,
///   set `state = resolved` + `target_id` (associate the image), emit
///   `target.resolved`;
/// - unknown / offline / ambiguous → `state = unresolved`, `attempts += 1`
///   (retryable; never fabricated, FR-009).
///
/// `online_enabled` gates the network call (FR-015); when `false`, only
/// cache/seed hits resolve and the rest stay `unresolved`. Emits
/// `target.resolve_batch.completed` when the pass finishes (when `bus` is set).
///
/// `limit` caps how many rows are processed in one pass (back-pressure).
///
/// # Errors
///
/// Returns [`ContractError`] on a local database failure. Resolver failures are
/// recorded as `unresolved`, never propagated as an error.
pub async fn resolve_pending<R: Resolver + ?Sized>(
    pool: &SqlitePool,
    resolver: &R,
    bus: Option<&EventBus>,
    online_enabled: bool,
    limit: usize,
) -> Result<DrainSummary, ContractError> {
    let limit_i = i64::try_from(limit.max(1)).unwrap_or(i64::MAX);
    let pending = repo::list_pending_ingest_resolutions(pool, limit_i).await.map_err(db_err)?;

    let considered = pending.len();
    let mut num_resolved = 0usize;
    let mut num_unresolved = 0usize;
    let mut num_pending = 0usize;

    for row in pending {
        let norm = normalize(row.object_raw.trim());

        // 1) Cache-first (FR-006): a cached/seeded object is never re-queried.
        if let Some(target) = cache::get_by_normalized(pool, &norm).await.map_err(cache_err)? {
            mark_resolved(pool, &row.id, &target.id.to_string()).await?;
            if let Some(bus) = bus {
                emit_resolved(bus, &target, Some(&row.object_raw)).await;
            }
            num_resolved += 1;
            continue;
        }

        // 2) Miss → online resolve when enabled. FIX-4: when online is disabled
        // this is a transient/offline condition (config or outage), NOT a real
        // content miss — leave the row `pending` and do NOT burn the retry budget.
        if !online_enabled {
            num_pending += 1;
            continue;
        }

        match resolver.resolve(row.object_raw.trim()).await {
            Ok(identity) => {
                let (id, outcome) =
                    cache::upsert_resolved(pool, &identity).await.map_err(cache_err)?;
                // Invalidate after the write commits (never before); a
                // `SkippedUserOverride` outcome means no row was actually
                // written (sticky user-override lock kept precedence), so the
                // catalog snapshot is not stale (mirrors `target_resolve::resolve`).
                if outcome != cache::UpsertOutcome::SkippedUserOverride {
                    crate::caches::invalidate_catalog();
                }
                let target_id = id.to_string();
                mark_resolved(pool, &row.id, &target_id).await?;
                if let Some(bus) = bus {
                    if let Some(target) = cache::get_by_id(pool, id).await.map_err(cache_err)? {
                        emit_resolved(bus, &target, Some(&row.object_raw)).await;
                    }
                }
                num_resolved += 1;
            }
            // FIX-4: transient/offline failures (SIMBAD outage, timeout, disabled)
            // leave the row `pending` with attempts UNCHANGED, so a single outage
            // during a large ingest doesn't exhaust the retry budget on every row.
            Err(ResolveError::Network(_) | ResolveError::Timeout(_) | ResolveError::Disabled) => {
                num_pending += 1;
            }
            // Genuine content misses (unknown / ambiguous / malformed response)
            // → unresolved + attempts++ (retryable later); never fabricate (FR-009).
            Err(
                ResolveError::NotFound(_) | ResolveError::Ambiguous { .. } | ResolveError::Parse(_),
            ) => {
                mark_unresolved(pool, &row.id, row.attempts).await?;
                num_unresolved += 1;
            }
        }
    }

    // issue #668: the periodic drain (~30s cadence) runs continuously whether
    // or not there's anything in the queue; publishing a completion event for
    // an empty pass floods the activity log with a no-op heartbeat (~470/500
    // rows in the reported sweep) that buries user-meaningful events. Only
    // publish when the pass actually considered something.
    if let Some(bus) = bus {
        if considered > 0 {
            let _ = bus
                .publish(
                    audit::event_bus::TOPIC_TARGET_RESOLVE_BATCH_COMPLETED,
                    Source::System,
                    TargetResolveBatchCompleted {
                        considered,
                        resolved: num_resolved,
                        unresolved: num_unresolved,
                        pending: num_pending,
                        at: Timestamp::now_iso(),
                    },
                )
                .await;
        }
    }

    Ok(DrainSummary {
        considered,
        resolved: num_resolved,
        unresolved: num_unresolved,
        pending: num_pending,
    })
}

/// Build a [`targeting_resolver::simbad::SimbadResolver`] from the persisted
/// `resolver_settings` row and run one full drain pass: [`resolve_pending`]
/// then [`crate::ingest_sessions::backfill_session_targets`].
///
/// Shared by the spec-035 US4/T043 periodic backstop
/// (`desktop_shell::bootstrap::background::spawn_ingest_resolution_drain`) and
/// the spec-035 plan-applied path (`app_core_inbox::plan_listener::
/// ingest_light_frames_if_applicable`, issue #1256): the latter calls this
/// immediately after a plan's light frames are ingested so newly-enqueued
/// `pending` rows (and any session left unlinked from an earlier pass) resolve
/// promptly instead of waiting for the next ~30s periodic tick. Never returns
/// an error — a failure to build the resolver, drain, or back-fill is logged
/// and the caller (periodic tick or next plan-applied event) retries.
pub async fn drain_and_backfill_once(
    pool: &SqlitePool,
    bus: &EventBus,
    resolve_cache: &targeting_resolver::simbad::ResolveCache,
) {
    use targeting_resolver::simbad::{SimbadConfig, SimbadResolver, DEFAULT_TAP_ENDPOINT};

    let settings = persistence_targets::repositories::q_desktop::get_resolver_settings(pool)
        .await
        .unwrap_or(None);
    let (online_enabled, endpoint, timeout_secs) = settings.map_or_else(
        || (true, DEFAULT_TAP_ENDPOINT.to_owned(), 10),
        |r| (r.online_enabled != 0, r.simbad_endpoint, r.request_timeout_secs),
    );

    // `SimbadResolver::new` never builds a reqwest/TLS client when
    // `online_enabled` is false (mirrors target.resolve FIX-3); cache hits
    // still resolve regardless.
    let config =
        SimbadConfig::from_settings(endpoint, u64::try_from(timeout_secs.max(1)).unwrap_or(10));
    let resolver = match SimbadResolver::new(&config, resolve_cache, online_enabled) {
        Ok(resolver) => resolver,
        Err(e) => {
            tracing::warn!("failed to build SimbadResolver for ingest drain: {e:?}");
            return;
        }
    };

    if let Err(e) = resolve_pending(pool, &resolver, Some(bus), online_enabled, 50).await {
        tracing::warn!("ingest_resolution drain failed: {e:?}");
        return;
    }

    if let Err(e) = crate::ingest_sessions::backfill_session_targets(pool).await {
        tracing::warn!("acquisition_session target back-fill failed: {e:?}");
    }
}

async fn mark_resolved(
    pool: &SqlitePool,
    row_id: &str,
    target_id: &str,
) -> Result<(), ContractError> {
    repo::mark_ingest_resolution_resolved(pool, row_id, target_id).await.map_err(db_err)
}

async fn mark_unresolved(
    pool: &SqlitePool,
    row_id: &str,
    attempts: i64,
) -> Result<(), ContractError> {
    repo::mark_ingest_resolution_unresolved(pool, row_id, attempts).await.map_err(db_err)
}

async fn emit_resolved(bus: &EventBus, target: &CachedTarget, query: Option<&str>) {
    let _ = bus
        .publish(
            audit::event_bus::TOPIC_TARGET_RESOLVED,
            Source::System,
            TargetResolved {
                target_id: target.id.to_string(),
                simbad_oid: target.simbad_oid,
                primary_designation: target.primary_designation.clone(),
                source: target.source.as_wire().to_owned(),
                query: query.map(ToOwned::to_owned),
                at: Timestamp::now_iso(),
            },
        )
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_core::Database;
    use targeting_resolver::cache::upsert_resolved;
    use targeting_resolver::{
        AliasKind, FakeResolver, ObjectType, ResolvedAlias, ResolvedIdentity, TargetSource as Src,
    };

    async fn setup() -> Database {
        let db = Database::in_memory().await.expect("in-memory DB");
        db.migrate().await.expect("migrations");
        db
    }

    /// Insert a `library_root` + `file_record` so the ingest_resolution FK holds.
    async fn make_image(db: &Database, rel: &str) -> String {
        let root_id = domain_core::ids::new_id();
        sqlx::query(
            "INSERT INTO library_root (id, label, current_path, kind, state, created_at)
             VALUES (?, 'test', '/tmp/test', 'local', 'active', '2026-01-01T00:00:00Z')",
        )
        .bind(&root_id)
        .execute(db.pool())
        .await
        .expect("library_root insert");
        let image_id = domain_core::ids::new_id();
        sqlx::query(
            "INSERT INTO file_record
                (id, root_id, relative_path, size_bytes, mtime, state, first_seen_at, last_seen_at)
             VALUES (?, ?, ?, 1, '2026-01-01T00:00:00Z', 'observed', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        )
        .bind(&image_id)
        .bind(&root_id)
        .bind(rel)
        .execute(db.pool())
        .await
        .expect("file_record insert");
        image_id
    }

    fn m31() -> ResolvedIdentity {
        ResolvedIdentity {
            simbad_oid: Some(1_575_544),
            primary_designation: "M 31".to_owned(),
            common_name: Some("Andromeda Galaxy".to_owned()),
            object_type: ObjectType::Galaxy,
            ra_deg: 10.684_708,
            dec_deg: 41.268_75,
            v_mag: None,
            aliases: vec![
                ResolvedAlias::new("M 31", AliasKind::Designation),
                ResolvedAlias::new("NGC 224", AliasKind::Designation),
                ResolvedAlias::new("Andromeda Galaxy", AliasKind::CommonName),
            ],
            source: Src::Resolved,
        }
    }

    async fn target_id_of(db: &Database, image_id: &str) -> Option<String> {
        let row: Option<(Option<String>, String)> =
            sqlx::query_as("SELECT target_id, state FROM ingest_resolution WHERE image_id = ?")
                .bind(image_id)
                .fetch_optional(db.pool())
                .await
                .unwrap();
        row.and_then(|(tid, _)| tid)
    }

    #[tokio::test]
    async fn cache_hit_associates_inline() {
        let db = setup().await;
        upsert_resolved(db.pool(), &m31()).await.unwrap();
        let img = make_image(&db, "a.fits").await;

        let outcome = associate_or_enqueue(db.pool(), None, &img, "NGC 224").await.unwrap();
        match outcome {
            AssociateOutcome::ResolvedInline(tid) => {
                assert_eq!(target_id_of(&db, &img).await.as_deref(), Some(tid.as_str()));
            }
            other => panic!("expected inline resolve, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn alias_variants_group_under_one_target() {
        let db = setup().await;
        upsert_resolved(db.pool(), &m31()).await.unwrap();
        let a = make_image(&db, "m31.fits").await;
        let b = make_image(&db, "ngc224.fits").await;
        let c = make_image(&db, "androm.fits").await;

        associate_or_enqueue(db.pool(), None, &a, "M 31").await.unwrap();
        associate_or_enqueue(db.pool(), None, &b, "NGC 224").await.unwrap();
        associate_or_enqueue(db.pool(), None, &c, "Andromeda Galaxy").await.unwrap();

        let ta = target_id_of(&db, &a).await;
        let tb = target_id_of(&db, &b).await;
        let tc = target_id_of(&db, &c).await;
        assert!(ta.is_some());
        assert_eq!(ta, tb, "M 31 and NGC 224 must group under one target");
        assert_eq!(tb, tc, "common name must group under the same target");
    }

    #[tokio::test]
    async fn miss_enqueues_pending() {
        let db = setup().await;
        let img = make_image(&db, "unknown.fits").await;
        let outcome =
            associate_or_enqueue(db.pool(), None, &img, "Some Unknown OBJECT").await.unwrap();
        assert_eq!(outcome, AssociateOutcome::Enqueued);

        let (state,): (String,) =
            sqlx::query_as("SELECT state FROM ingest_resolution WHERE image_id = ?")
                .bind(&img)
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(state, "pending");
    }

    #[tokio::test]
    async fn blank_object_is_noop() {
        let db = setup().await;
        let img = make_image(&db, "blank.fits").await;
        let outcome = associate_or_enqueue(db.pool(), None, &img, "   ").await.unwrap();
        assert_eq!(outcome, AssociateOutcome::NoObject);
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM ingest_resolution")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn drain_resolves_pending_via_resolver_and_caches() {
        let db = setup().await;
        let img = make_image(&db, "m31.fits").await;
        // Miss → pending.
        associate_or_enqueue(db.pool(), None, &img, "M 31").await.unwrap();

        let resolver = FakeResolver::new().with_response("M 31", m31());
        let summary = resolve_pending(db.pool(), &resolver, None, true, 50).await.unwrap();
        assert_eq!(summary.considered, 1);
        assert_eq!(summary.resolved, 1);
        assert_eq!(summary.unresolved, 0);

        // Associated, and cached for next time.
        assert!(target_id_of(&db, &img).await.is_some());
        assert!(cache::get_by_simbad_oid(db.pool(), 1_575_544).await.unwrap().is_some());
    }

    /// issue #668: an empty drain pass (nothing pending — the common case for
    /// the periodic ~30s heartbeat) must NOT publish `target.resolve_batch
    /// .completed` at all, so it can't flood the activity log with no-op
    /// events.
    #[tokio::test]
    async fn drain_with_nothing_pending_does_not_publish_batch_completed() {
        let db = setup().await;
        let bus = audit::EventBus::with_pool(db.pool().clone());
        let resolver = FakeResolver::new();

        let summary = resolve_pending(db.pool(), &resolver, Some(&bus), true, 50).await.unwrap();
        assert_eq!(summary.considered, 0);

        let events = persistence_lifecycle::repositories::events::list_since_by_topic(
            db.pool(),
            0,
            audit::event_bus::TOPIC_TARGET_RESOLVE_BATCH_COMPLETED,
        )
        .await
        .unwrap();
        assert!(events.is_empty(), "empty drain pass must not publish a completion event");
    }

    /// A pass that actually considers rows — even if none resolve — is real
    /// activity and must still publish the completion event.
    #[tokio::test]
    async fn drain_with_pending_rows_publishes_batch_completed() {
        let db = setup().await;
        let bus = audit::EventBus::with_pool(db.pool().clone());
        let img = make_image(&db, "m31.fits").await;
        associate_or_enqueue(db.pool(), None, &img, "M 31").await.unwrap();

        let resolver = FakeResolver::new().with_response("M 31", m31());
        let summary = resolve_pending(db.pool(), &resolver, Some(&bus), true, 50).await.unwrap();
        assert_eq!(summary.considered, 1);

        let events = persistence_lifecycle::repositories::events::list_since_by_topic(
            db.pool(),
            0,
            audit::event_bus::TOPIC_TARGET_RESOLVE_BATCH_COMPLETED,
        )
        .await
        .unwrap();
        assert_eq!(events.len(), 1, "non-empty pass must publish exactly one completion event");
    }

    #[tokio::test]
    async fn drain_unknown_stays_unresolved_retryable() {
        let db = setup().await;
        let img = make_image(&db, "garbled.fits").await;
        associate_or_enqueue(db.pool(), None, &img, "Garbled OBJECT").await.unwrap();

        let resolver = FakeResolver::new(); // default NotFound
        let summary = resolve_pending(db.pool(), &resolver, None, true, 50).await.unwrap();
        assert_eq!(summary.unresolved, 1);

        let (state, attempts, tid): (String, i64, Option<String>) = sqlx::query_as(
            "SELECT state, attempts, target_id FROM ingest_resolution WHERE image_id = ?",
        )
        .bind(&img)
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(state, "unresolved");
        assert_eq!(attempts, 1, "retryable: attempts incremented");
        assert!(tid.is_none(), "never fabricated a target");
    }

    #[tokio::test]
    async fn drain_offline_disabled_keeps_pending_no_attempt_burn() {
        let db = setup().await;
        let img = make_image(&db, "m31.fits").await;
        associate_or_enqueue(db.pool(), None, &img, "M 31").await.unwrap();

        // Resolver would succeed, but online disabled → must not be called, and
        // the row stays `pending` with attempts unchanged (FIX-4).
        let resolver = FakeResolver::new().with_response("M 31", m31());
        let summary = resolve_pending(db.pool(), &resolver, None, false, 50).await.unwrap();
        assert_eq!(summary.pending, 1);
        assert_eq!(summary.unresolved, 0);

        let (state, attempts): (String, i64) =
            sqlx::query_as("SELECT state, attempts FROM ingest_resolution WHERE image_id = ?")
                .bind(&img)
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(state, "pending", "offline leaves row pending, not unresolved");
        assert_eq!(attempts, 0, "transient/offline must NOT burn the retry budget");
        assert!(target_id_of(&db, &img).await.is_none());
    }

    #[tokio::test]
    async fn drain_transient_network_error_keeps_pending() {
        let db = setup().await;
        let img = make_image(&db, "m31.fits").await;
        enqueue(db.pool(), &img, "M 31").await.unwrap();

        // SIMBAD outage: a Network error must leave the row pending, attempts 0.
        let resolver = FakeResolver::new().with_default_error(ResolveError::Network("down".into()));
        let summary = resolve_pending(db.pool(), &resolver, None, true, 50).await.unwrap();
        assert_eq!(summary.pending, 1);
        assert_eq!(summary.unresolved, 0);

        let (state, attempts): (String, i64) =
            sqlx::query_as("SELECT state, attempts FROM ingest_resolution WHERE image_id = ?")
                .bind(&img)
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(state, "pending");
        assert_eq!(attempts, 0, "an outage must not exhaust the retry budget per row");
    }

    #[tokio::test]
    async fn drain_cache_hit_resolves_without_resolver() {
        let db = setup().await;
        upsert_resolved(db.pool(), &m31()).await.unwrap();
        let img = make_image(&db, "ngc224.fits").await;
        // Force a pending row directly (bypass inline path) to exercise drain cache-hit.
        enqueue(db.pool(), &img, "NGC 224").await.unwrap();

        // Resolver is "offline"; cache hit must still resolve the pending row.
        let resolver = FakeResolver::new().with_default_error(ResolveError::Network("x".into()));
        let summary = resolve_pending(db.pool(), &resolver, None, true, 50).await.unwrap();
        assert_eq!(summary.resolved, 1);
        assert!(target_id_of(&db, &img).await.is_some());
    }

    #[tokio::test]
    async fn enqueue_is_idempotent() {
        let db = setup().await;
        let img = make_image(&db, "u.fits").await;
        let id1 = enqueue(db.pool(), &img, "X").await.unwrap();
        let id2 = enqueue(db.pool(), &img, "X").await.unwrap();
        assert_eq!(id1, id2);
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM ingest_resolution")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(count, 1);
    }
}
