// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Ingest light frames → acquisition sessions grouped by capture identity
//! (spec 035 US4, FR-016).
//!
//! When a reviewable inbox plan reaches `applied`, each light frame it moved or
//! catalogued must be folded into an `acquisition_session` keyed by capture
//! identity (`session_key`: target/OBJECT, filter, binning, gain,
//! observing-night) and linked to its resolved canonical target. This module is
//! the production entry point the plan-apply listener calls.
//!
//! ## Per-frame pipeline (`ingest_light_frame`)
//!
//! 1. **Resolve the destination root → `library_root` (R9).** Applied inbox
//!    items store `to_root_id` as a `registered_sources` id, but
//!    `file_record.root_id` references `library_root`. We mirror the
//!    `registered_sources` row into a `library_root` row with the SAME id before
//!    inserting the file record. If neither table can supply a path, the frame
//!    is skipped (logged) and the rest of the plan still ingests.
//! 2. **Upsert `file_record`** by its UNIQUE `(root_id, relative_path)` — reuses
//!    an existing id, sets `state = 'classified'`.
//! 3. **Associate the FITS `OBJECT`** via [`crate::ingest_resolution::
//!    associate_or_enqueue`]: a cache hit links the canonical target inline; a
//!    miss enqueues a `pending` row for the background drain. Never blocks, never
//!    fabricates a target (FR-009/FR-013).
//! 4. **Derive `session_key`** ([`sessions::SessionKey::new`]). When the observer
//!    location is unset the observing-night boundary is computed in UTC (R11)
//!    and `has_observer_location = 0` is recorded; the error is never propagated
//!    so ingest is never blocked. The key's target component is the resolved
//!    canonical-target id on a cache hit, else the raw `OBJECT` string so
//!    unresolved frames still group coherently.
//! 5. **Upsert `acquisition_session`** by `session_key`: append the file-record
//!    id to the `frame_ids` JSON array (set-deduped). On insert,
//!    `canonical_target_id` = resolved id or NULL, legacy `target_id` left
//!    NULL (R10). Sessions are derived, already-confirmed inventory (spec
//!    041 FR-051) — there is no review-state column to set.
//! 6. **Propagate to linked projects** (spec 041 R-17/FR-052, T075): whenever a
//!    session's `canonical_target_id` transitions from unset to resolved (on
//!    insert, on back-fill-on-append, or via [`backfill_session_targets`]),
//!    every project linked to that session through `project_sources` has its
//!    own `canonical_target_id` set to match — but only if the project does
//!    not already have one (never overwrites an existing value, manually
//!    picked or otherwise). This closes spec-035 project↔target gap #1 for
//!    the live-ingest path: `projects.canonical_target_id` (migration 0033)
//!    was previously only ever set once, manually, at project creation; it
//!    now also gets set from whatever the project's lights first resolve to
//!    when it was unset. A session with no linked project is a no-op.
//!
//! ## Idempotency (R12)
//!
//! Re-ingesting the same applied plan is a no-op: `file_record` upserts by its
//! UNIQUE `(root_id, relative_path)`, `acquisition_session` upserts by a
//! SELECT-by-`session_key` (a non-unique lookup index) then INSERT-or-append,
//! and `frame_ids` set-dedup drops a frame id already present.
//!
//! ## Constitution
//!
//! - §I/§III: metadata/identity only; no image bytes are read beyond the FITS
//!   header, and no files are written or processed.
//! - §V: SQLite is the durable record; grouping + linkage are explicit rows.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use audit::EventBus;
use metadata_core::RawFileMetadata;
use persistence_lifecycle::repositories::first_run;
use persistence_plans::repositories::plans as plans_repo;
use persistence_plans::repositories::projects as repo_projects;
use persistence_targets::repositories::framing as framing_repo;
use persistence_targets::repositories::inventory;
use persistence_targets::repositories::q_targets_ingest as repo;
use sessions::{ObserverContext, SessionKey};
use sqlx::SqlitePool;
use time::format_description::well_known::Iso8601;
use time::{OffsetDateTime, PrimitiveDateTime, UtcOffset};

use contracts_core::ContractError;

use crate::ingest_resolution::{associate_or_enqueue, AssociateOutcome};

use app_core_errors::db_err;

/// Summary of one [`ingest_light_frames`] pass.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IngestSummary {
    /// Light frames examined (after filtering out calibration items).
    pub considered: usize,
    /// Frames folded into a session (created or appended).
    pub ingested: usize,
    /// Frames skipped because their destination root could not be resolved (R9).
    pub skipped: usize,
}

/// One applied plan item to ingest (subset of the `plan_items` row).
#[derive(Clone, Debug)]
struct AppliedItem {
    /// Destination root id (a `registered_sources` id for inbox plans).
    root_id: String,
    /// Destination relative path under that root.
    relative_path: String,
}

/// Ingest every applied light frame from a completed plan (FR-016).
///
/// Only `move`/`catalogue` items whose effective frame type is `light` are
/// processed; calibration frames are out of US4 scope (handled by the spec-040
/// master path). The plan is read by `plan_id`; the destination path of each
/// item (`to_root_id` + `to_relative_path`) locates the file on disk to read its
/// FITS header.
///
/// `root_path_of` maps a destination root id to its absolute filesystem path so
/// the FITS header can be read; it is injected so tests can avoid touching the
/// real `registered_sources` table layout.
///
/// # Errors
///
/// Returns [`ContractError`] (`internal.database`) only on a query failure.
/// Per-frame metadata/IO problems are handled inline (skip + log), never
/// propagated, so one bad file cannot abort the whole ingest.
pub async fn ingest_light_frames(
    pool: &SqlitePool,
    bus: Option<&EventBus>,
    plan_id: &str,
) -> Result<IngestSummary, ContractError> {
    // Applied move/catalogue items with a destination root + path. Item type is
    // filtered to light frames below (by reading the FITS header), so calibration
    // moves are excluded even though they share the `move` action.
    let rows = repo::list_applied_light_plan_items(pool, plan_id).await.map_err(db_err)?;

    let mut summary = IngestSummary::default();

    for row in rows {
        // Catalogue-in-place keeps the file at its source; a move lands it at the
        // destination. Prefer the destination, falling back to the source root
        // when the item carries no `to_root_id` (older/catalogue rows).
        let (root_id, relative_path) = match (row.to_root_id, row.from_root_id) {
            (Some(r), _) if !row.to_relative_path.is_empty() => (r, row.to_relative_path),
            (_, Some(r)) => (r, row.from_relative_path),
            _ => continue,
        };
        let item = AppliedItem { root_id, relative_path };

        match ingest_light_frame(pool, bus, plan_id, &item).await? {
            FrameOutcome::NotLight => {}
            FrameOutcome::Skipped => summary.skipped += 1,
            FrameOutcome::Ingested => {
                summary.considered += 1;
                summary.ingested += 1;
            }
        }
    }

    Ok(summary)
}

enum FrameOutcome {
    /// Not a light frame (calibration / unknown IMAGETYP) — out of US4 scope.
    NotLight,
    /// A light frame whose destination root could not be resolved (R9).
    Skipped,
    /// A light frame folded into a session.
    Ingested,
}

/// Ingest one applied light frame. See the module docs for the pipeline.
async fn ingest_light_frame(
    pool: &SqlitePool,
    bus: Option<&EventBus>,
    plan_id: &str,
    item: &AppliedItem,
) -> Result<FrameOutcome, ContractError> {
    // R9: ensure a `library_root` row for the destination root before the
    // `file_record` FK insert. If the path cannot be resolved at all, skip.
    let Some(root_path) = ensure_library_root(pool, &item.root_id).await? else {
        tracing::warn!(
            root_id = %item.root_id,
            relative_path = %item.relative_path,
            "ingest: destination root not resolvable to a library_root; skipping frame"
        );
        return Ok(FrameOutcome::Skipped);
    };

    // Read the FITS/XISF header at the destination. A missing/unreadable file or
    // a calibration frame is not an error — it just isn't an ingestable light.
    let abs_path = join_path(&root_path, &item.relative_path);
    let meta = read_metadata(&abs_path);
    let Some(meta) = meta else { return Ok(FrameOutcome::NotLight) };
    if !is_light_frame(&meta) {
        return Ok(FrameOutcome::NotLight);
    }

    // Upsert the file record (UNIQUE root_id+relative_path → reuse id) with the
    // REAL on-disk size (spec 048 FR-001; was `size_bytes = 0`). A stat failure
    // (file vanished between apply and this read) falls back to 0/now rather
    // than blocking ingest — reconciliation (spec 048 US2) corrects it later.
    let (size_bytes, mtime) = crate::frame_writer::stat_frame(&abs_path).unwrap_or_else(|| {
        (0, OffsetDateTime::now_utc().format(&Iso8601::DEFAULT).unwrap_or_default())
    });
    let image_id = crate::frame_writer::upsert_frame_record(
        pool,
        &item.root_id,
        &item.relative_path,
        size_bytes,
        &mtime,
        "classified",
    )
    .await?;

    // Associate the FITS OBJECT → canonical target (inline cache hit or pending).
    let object_raw = meta.object.as_deref().unwrap_or("").trim().to_owned();
    let canonical_target_id = if object_raw.is_empty() {
        None
    } else {
        match associate_or_enqueue(pool, bus, &image_id, &object_raw).await? {
            AssociateOutcome::ResolvedInline(target_id) => Some(target_id),
            AssociateOutcome::Enqueued | AssociateOutcome::NoObject => None,
        }
    };

    // Derive the session key. The target component is the resolved canonical id
    // on a cache hit, else the raw OBJECT string so unresolved frames still
    // group (and back-fill later). A blank OBJECT groups under "unknown".
    let key_target = canonical_target_id.clone().unwrap_or_else(|| {
        if object_raw.is_empty() {
            "unknown".to_owned()
        } else {
            object_raw.clone()
        }
    });

    let (key, has_observer_location) = derive_session_key(&key_target, &meta);

    let session_id = upsert_session(
        pool,
        &key,
        has_observer_location,
        canonical_target_id.as_deref(),
        &image_id,
        &item.root_id,
    )
    .await?;

    // F-Framing-5: populate the durable session-level clustering key from
    // this frame's real header data — the "populated at confirm" (loosely,
    // the confirm-driven ingest pipeline) counterpart to the staged geometry
    // the Inbox-confirm attribution pass matches against. Q16 null semantics:
    // fields missing from this frame's header fill in only where the
    // session's existing snapshot is also missing (never regress a known
    // value to NULL because a later frame's header happened to omit it).
    //
    // Logged, never propagated (module docs: "one bad file cannot abort the
    // whole ingest") — a `?` here would abort `ingest_light_frames`'s loop
    // for every remaining frame in the plan, not just this one.
    if let Err(e) = backfill_session_geometry(pool, &session_id, &meta).await {
        tracing::warn!(session_id, "ingest: failed to backfill session geometry: {e:?}");
    }

    // F-Framing-10: bind this session to the attribution pick recorded on
    // the confirming plan, if any — the earliest point a real session id
    // exists to add as a framing member. Logged, never propagated (same
    // reasoning as above).
    if let Err(e) = bind_chosen_framing(pool, plan_id, &session_id).await {
        tracing::warn!(plan_id, session_id, "ingest: failed to bind chosen framing: {e:?}");
    }

    Ok(FrameOutcome::Ingested)
}

/// Fill in the session's durable geometry columns from `meta`, preserving any
/// already-known field (mirrors `upsert_session`'s `root_id` COALESCE
/// precedent) rather than overwriting a good value with a later frame's
/// missing header data.
async fn backfill_session_geometry(
    pool: &SqlitePool,
    session_id: &str,
    meta: &RawFileMetadata,
) -> Result<(), ContractError> {
    let existing = framing_repo::get_session_geometry(pool, session_id).await.map_err(db_err)?;
    let optic_train_key = sessions::optic_train_key(
        meta.telescop.as_deref(),
        meta.instrume.as_deref(),
        meta.focal_length_mm,
    );

    let (pointing_ra_deg, pointing_dec_deg, rotation_deg, optic_train_key) = match &existing {
        Some(row) => (
            row.pointing_ra_deg.or(meta.ra_deg),
            row.pointing_dec_deg.or(meta.dec_deg),
            row.rotation_deg.or(meta.rotator_angle_deg),
            row.optic_train_key.clone().or(optic_train_key),
        ),
        None => (meta.ra_deg, meta.dec_deg, meta.rotator_angle_deg, optic_train_key),
    };

    // Every light frame in a session runs this (`ingest_light_frame`'s
    // per-frame hook), but a session's geometry is only ever WRITTEN ONCE in
    // practice: the COALESCE-with-existing merge above means frame 2+ of an
    // already-backfilled session recomputes the exact same tuple it read.
    // Skipping the no-op write avoids an avoidable `UPDATE` (and its
    // exclusive lock) on every subsequent frame of a session — real
    // contention on the shared, non-WAL-mode pool this background task
    // shares with foreground UI reads (`plan_listener` runs on the same
    // connection pool as every other command).
    let unchanged = existing.as_ref().is_some_and(|row| {
        row.pointing_ra_deg == pointing_ra_deg
            && row.pointing_dec_deg == pointing_dec_deg
            && row.rotation_deg == rotation_deg
            && row.optic_train_key.as_deref() == optic_train_key.as_deref()
    });
    if unchanged {
        return Ok(());
    }

    framing_repo::set_session_geometry(
        pool,
        session_id,
        pointing_ra_deg,
        pointing_dec_deg,
        rotation_deg,
        optic_train_key.as_deref(),
    )
    .await
    .map_err(db_err)
}

/// Add `session_id` to the plan's chosen attribution framing, if one was
/// recorded at confirm time (F-Framing-10). No-op when the plan carries no
/// pick, or when this session was already a member of some framing (a
/// reasonable outcome on repeat ingest — the executor's own
/// `add_session_to_framing` UNIQUE(session_id) constraint would otherwise
/// reject it) — logged, never propagated, matching this module's existing
/// per-frame error posture.
async fn bind_chosen_framing(
    pool: &SqlitePool,
    plan_id: &str,
    session_id: &str,
) -> Result<(), ContractError> {
    let Some(framing_id) =
        plans_repo::get_chosen_framing_id(pool, plan_id).await.map_err(db_err)?
    else {
        return Ok(());
    };
    if framing_repo::get_framing_id_for_session(pool, session_id).await.map_err(db_err)?.is_some() {
        return Ok(());
    }
    if let Err(e) = framing_repo::add_session_to_framing(pool, &framing_id, session_id).await {
        tracing::warn!(
            plan_id,
            session_id,
            framing_id,
            "ingest: failed to bind session to chosen attribution framing: {e:?}"
        );
    }
    Ok(())
}

// ── R9: library_root mirroring ──────────────────────────────────────────────────

/// Ensure a `library_root` row exists for `root_id`, returning its absolute path.
///
/// Resolution order (R9):
/// 1. An existing `library_root` row → use its `current_path`.
/// 2. A `registered_sources` row with the same id → mirror it into a
///    `library_root` row (same id, `current_path = registered_sources.path`),
///    and return that path.
/// 3. Neither → `None` (caller skips the frame).
///
/// `pub` (spec 048 T012) so calibration-frame apply
/// (`app_core_inbox::plan_listener`) can resolve the same root path before
/// writing a `file_record` via `crate::frame_writer`.
///
/// # Errors
///
/// Returns [`ContractError`] (`internal.database`) on a query/insert failure.
pub async fn ensure_library_root(
    pool: &SqlitePool,
    root_id: &str,
) -> Result<Option<String>, ContractError> {
    if let Some(path) = inventory::get_library_root_path(pool, root_id).await.map_err(db_err)? {
        return Ok(Some(path));
    }

    let Some(path) = first_run::get_source_path(pool, root_id).await.map_err(db_err)? else {
        return Ok(None);
    };

    // Mirror into library_root with the SAME id so the file_record FK holds.
    repo::insert_library_root_mirror(
        pool,
        root_id,
        &path,
        &OffsetDateTime::now_utc().format(&Iso8601::DEFAULT).unwrap_or_default(),
    )
    .await
    .map_err(db_err)?;

    Ok(Some(path))
}

// ── file_record upsert: see `crate::frame_writer::upsert_frame_record` (spec 048 T002) ──

// ── session_key derivation ────────────────────────────────────────────────────

/// Derive the `session_key` for a frame, returning `(key, has_observer_location)`.
///
/// `has_observer_location` is always `false` for v1: the observer's geographic
/// location is not yet threaded into the ingest path, so the observing-night
/// boundary uses the UTC fallback (R11). The error path of
/// [`sessions::SessionKey::new`] is never reached because an observer is always
/// supplied (UTC); ingest is never blocked on a missing location.
fn derive_session_key(key_target: &str, meta: &RawFileMetadata) -> (String, bool) {
    let filter = meta.filter.as_deref().unwrap_or("").trim();
    let binning = binning_of(meta);
    let gain = meta.gain.as_deref().unwrap_or("").trim();
    let capture_at = parse_date_obs(meta.date_obs.as_deref());

    // R11: UTC observer fallback — never propagate ObserverLocationMissing.
    let observer = ObserverContext { utc_offset: UtcOffset::UTC };
    let key = SessionKey::new(key_target, filter, &binning, gain, capture_at, Some(&observer))
        .unwrap_or_else(|_| SessionKey(format!("{key_target}|{filter}|{binning}|{gain}|")));
    (key.0, false)
}

/// Combine XBINNING/YBINNING into the canonical `NxM` form (e.g. `1x1`), or `""`.
fn binning_of(meta: &RawFileMetadata) -> String {
    match (meta.x_binning.as_deref(), meta.y_binning.as_deref()) {
        (Some(x), Some(y)) => format!("{}x{}", x.trim(), y.trim()),
        (Some(x), None) => x.trim().to_owned(),
        _ => String::new(),
    }
}

/// Parse a FITS `DATE-OBS` value into a UTC [`OffsetDateTime`].
///
/// FITS `DATE-OBS` is ISO 8601, usually without a timezone designator
/// (`2026-03-15T21:00:00[.sss]`). We try a full offset parse first, then a
/// primitive (no-offset) parse assumed UTC. An absent/garbled value falls back
/// to the current UTC time so the frame still groups (into "today"'s night).
fn parse_date_obs(raw: Option<&str>) -> OffsetDateTime {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return OffsetDateTime::now_utc();
    };
    if let Ok(dt) = OffsetDateTime::parse(raw, &Iso8601::DEFAULT) {
        return dt;
    }
    if let Ok(dt) = PrimitiveDateTime::parse(raw, &Iso8601::DEFAULT) {
        return dt.assume_utc();
    }
    OffsetDateTime::now_utc()
}

// ── acquisition_session upsert ────────────────────────────────────────────────

/// Upsert an `acquisition_session` by `session_key`, appending `image_id` to the
/// `frame_ids` JSON array (set-deduped). On insert: `canonical_target_id` set,
/// legacy `target_id` NULL (R10). No review-state column (spec 041 FR-051).
///
/// Idempotent (R12): the SELECT-by-`session_key` lookup keeps grouping
/// single-row, and a repeat append of the same frame id is dropped (set-dedup).
///
/// `root_id` (R9/T036/FR-012 — spec 006's `acquisition_session.root_id`,
/// migration 0021) is set on every insert and back-filled with
/// `COALESCE(root_id, ?)` on every append so a session's first-known root
/// sticks even across repeat ingests. This closed a real gap (found via
/// #470's Layer-2 journeys, round 6): `root_id` was never written by this
/// function at all — the column existed and `persistence_db::repositories::
/// inventory::update_acquisition_session_root_id` existed to set it (its own
/// doc comment: "Called when the inbox confirm pipeline resolves the root
/// for a session"), but nothing ever called it, so every real ingested
/// session's `root_id` stayed `NULL`. Sqlx-sqlite (this project's version)
/// silently decodes a `NULL` `TEXT` column into a plain (non-`Option`)
/// `String` as `""` rather than erroring, so downstream readers that assume
/// `root_id` is always populated (`app_core_projects::source_view_generate`)
/// got a real-looking-but-empty id and failed with a confusing
/// `no_link_kind`/"source root  could not be resolved" (note the double
/// space) instead of a clear "root never set" error.
/// Returns the session's id (new or existing) so callers can write
/// per-session follow-up state — the durable geometry snapshot (F-Framing-5)
/// and the attribution apply-path's framing binding (F-Framing-10) both need
/// it and no session exists before this call returns.
async fn upsert_session(
    pool: &SqlitePool,
    key: &str,
    has_observer_location: bool,
    canonical_target_id: Option<&str>,
    image_id: &str,
    root_id: &str,
) -> Result<String, ContractError> {
    if let Some(existing) =
        repo::find_acquisition_session_by_key(pool, key).await.map_err(db_err)?
    {
        let mut frames: BTreeSet<String> =
            serde_json::from_str(&existing.frame_ids).unwrap_or_default();
        frames.insert(image_id.to_owned());
        let frames_json = serde_json::to_string(&frames.into_iter().collect::<Vec<_>>())
            .map_err(|e| app_core_errors::db_internal_ctx(e, "serialize frame_ids"))?;

        // Back-fill the link if it resolved this pass and the row had none.
        match (existing.canonical_target_id.is_none(), canonical_target_id) {
            (true, Some(target_id)) => {
                repo::append_acquisition_session_frames_with_target(
                    pool,
                    &existing.id,
                    &frames_json,
                    target_id,
                    root_id,
                )
                .await
                .map_err(db_err)?;
                // T075/FR-052: newly resolved on this session → propagate to any
                // linked project.
                propagate_target_to_projects(pool, &existing.id, target_id).await?;
            }
            _ => {
                repo::append_acquisition_session_frames(pool, &existing.id, &frames_json, root_id)
                    .await
                    .map_err(db_err)?;
            }
        }
        return Ok(existing.id);
    }

    let id = domain_core::ids::new_id();
    let frames_json = serde_json::to_string(&[image_id])
        .map_err(|e| app_core_errors::db_internal_ctx(e, "serialize frame_ids"))?;
    repo::insert_acquisition_session(
        pool,
        &id,
        key,
        canonical_target_id,
        has_observer_location,
        &frames_json,
        root_id,
        &OffsetDateTime::now_utc().format(&Iso8601::DEFAULT).unwrap_or_default(),
    )
    .await
    .map_err(db_err)?;

    // T075/FR-052: the new session already resolved a target → propagate.
    if let Some(target_id) = canonical_target_id {
        propagate_target_to_projects(pool, &id, target_id).await?;
    }
    Ok(id)
}

// ── Target propagation (T075) ─────────────────────────────────────────────────

/// Propagate a session's resolved canonical target to every project linked to
/// it via `project_sources` (spec 041 R-17/FR-052).
///
/// Closes spec-035 project↔target gap #1 for the live-ingest path:
/// `projects.canonical_target_id` (migration 0033) was previously only ever
/// set once, manually, at project creation (`CreateProjectDialog`); this keeps
/// it in sync with whatever the project's lights actually resolve to,
/// whenever a linked session's own `canonical_target_id` becomes known.
///
/// A session with no linked project (`project_sources` has no row for it) is
/// a no-op — never an error.
///
/// # Errors
///
/// Returns [`ContractError`] (`internal.database`) on a query failure.
async fn propagate_target_to_projects(
    pool: &SqlitePool,
    session_id: &str,
    canonical_target_id: &str,
) -> Result<(), ContractError> {
    let project_ids =
        repo_projects::list_project_ids_for_session(pool, session_id).await.map_err(db_err)?;
    for project_id in project_ids {
        repo_projects::set_project_canonical_target_id(pool, &project_id, canonical_target_id)
            .await
            .map_err(db_err)?;
    }
    Ok(())
}

// ── Back-fill (T043) ──────────────────────────────────────────────────────────

/// Back-fill `acquisition_session.canonical_target_id` for sessions whose frames
/// resolved after the initial ingest (spec 035 US4/T043, FR-016).
///
/// For each session with a NULL `canonical_target_id`, look for any frame id in
/// its `frame_ids` array that now has a `resolved` `ingest_resolution` row, and
/// adopt that row's `target_id`. Idempotent: already-linked sessions are
/// skipped, and a session is updated at most once per resolved frame found.
///
/// Returns the number of sessions linked in this pass.
///
/// # Errors
///
/// Returns [`ContractError`] (`internal.database`) on a query failure.
pub async fn backfill_session_targets(pool: &SqlitePool) -> Result<usize, ContractError> {
    // Map of image_id → resolved canonical target id (only resolved rows).
    let resolved = repo::list_resolved_ingest_resolutions(pool).await.map_err(db_err)?;
    if resolved.is_empty() {
        return Ok(0);
    }
    let resolved: std::collections::HashMap<String, String> =
        resolved.into_iter().map(|r| (r.image_id, r.target_id)).collect();

    let unlinked = repo::list_unlinked_acquisition_sessions(pool).await.map_err(db_err)?;

    let mut linked = 0usize;
    for session in unlinked {
        let frames: Vec<String> = serde_json::from_str(&session.frame_ids).unwrap_or_default();
        let Some(target_id) = frames.iter().find_map(|f| resolved.get(f)) else {
            continue;
        };
        repo::set_acquisition_session_canonical_target_if_null(pool, &session.id, target_id)
            .await
            .map_err(db_err)?;
        // T075/FR-052: newly resolved via back-fill → propagate.
        propagate_target_to_projects(pool, &session.id, target_id).await?;
        linked += 1;
    }
    Ok(linked)
}

// ── FITS helpers ──────────────────────────────────────────────────────────────

/// Read FITS/XISF header metadata for a file, or `None` when unreadable /
/// unsupported (treated as "not an ingestable light", never an error).
///
/// Served through [`crate::metadata_cache::cached_extract`] (in-memory caching
/// layer F0), memoized by `(path, mtime, size)` — a burst of reads for the
/// same file during a scan does not re-parse the header once per caller.
fn read_metadata(abs_path: &Path) -> Option<Arc<RawFileMetadata>> {
    crate::metadata_cache::cached_extract(abs_path).ok()
}

/// True when the frame's `IMAGETYP` normalizes to a light frame. Calibration
/// frames (bias/dark/flat) and unknown/absent IMAGETYP are excluded (US4 scope).
fn is_light_frame(meta: &RawFileMetadata) -> bool {
    matches!(
        meta.image_typ.as_deref().map(str::trim).map(str::to_ascii_lowercase).as_deref(),
        Some(
            "light"
                | "light frame"
                | "light frames"
                | "science"
                | "science frame"
                | "science frames"
                | "object"
        )
    )
}

/// Join a root path and a relative path with a single separator.
fn join_path(root: &str, relative: &str) -> std::path::PathBuf {
    Path::new(root).join(relative)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta_imagetyp(t: &str) -> RawFileMetadata {
        RawFileMetadata { image_typ: Some(t.to_owned()), ..RawFileMetadata::default() }
    }

    #[test]
    fn is_light_recognizes_variants() {
        assert!(is_light_frame(&meta_imagetyp("Light Frame")));
        assert!(is_light_frame(&meta_imagetyp("LIGHT")));
        assert!(is_light_frame(&meta_imagetyp("Science")));
        assert!(!is_light_frame(&meta_imagetyp("Dark")));
        assert!(!is_light_frame(&meta_imagetyp("Flat Frame")));
        assert!(!is_light_frame(&meta_imagetyp("Bias")));
        assert!(!is_light_frame(&RawFileMetadata::default()));
    }

    #[test]
    fn binning_combines_axes() {
        let m = RawFileMetadata {
            x_binning: Some("1".to_owned()),
            y_binning: Some("1".to_owned()),
            ..RawFileMetadata::default()
        };
        assert_eq!(binning_of(&m), "1x1");
        let m = RawFileMetadata { x_binning: Some("1".to_owned()), ..RawFileMetadata::default() };
        assert_eq!(binning_of(&m), "1");
    }

    #[test]
    fn date_obs_parses_no_offset_as_utc() {
        let dt = parse_date_obs(Some("2026-03-15T21:00:00"));
        assert_eq!(dt.offset(), UtcOffset::UTC);
        assert_eq!(dt.year(), 2026);
    }

    #[test]
    fn session_key_uses_utc_fallback() {
        let m = RawFileMetadata {
            filter: Some("Ha".to_owned()),
            x_binning: Some("1".to_owned()),
            y_binning: Some("1".to_owned()),
            gain: Some("100".to_owned()),
            date_obs: Some("2026-03-15T21:00:00".to_owned()),
            ..RawFileMetadata::default()
        };
        let (key, has_loc) = derive_session_key("M 31", &m);
        assert!(!has_loc, "UTC fallback marks observer location absent");
        assert_eq!(key, "M 31|Ha|1x1|100|2026-03-15");
    }

    // ── T075/FR-052: target propagation to linked projects ────────────────────

    use persistence_plans::repositories::projects::InsertProject;

    async fn test_db() -> persistence_core::Database {
        persistence_core::test_support::setup_db().await
    }

    /// Insert a minimal `canonical_target` row (same shape used elsewhere in the
    /// workspace, e.g. `project_setup::seed_canonical_target`).
    async fn seed_canonical_target(pool: &SqlitePool, id: &str, designation: &str) {
        sqlx::query(
            "INSERT INTO canonical_target
                (id, simbad_oid, primary_designation, object_type, ra_deg, dec_deg, source, resolved_at)
             VALUES (?, NULL, ?, 'galaxy', 10.68, 41.27, 'resolved', '2026-01-01T00:00:00Z')",
        )
        .bind(id)
        .bind(designation)
        .execute(pool)
        .await
        .unwrap();
    }

    /// Insert a minimal `library_root` row so `acquisition_session.root_id`'s
    /// FK (migration 0021) is satisfiable in tests that pass a `root_id` to
    /// `upsert_session`.
    async fn seed_library_root(pool: &SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO library_root (id, label, current_path, kind, state, created_at)
             VALUES (?, ?, ?, 'local', 'active', '2026-01-01T00:00:00Z')",
        )
        .bind(id)
        .bind(id)
        .bind(format!("/tmp/{id}"))
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_project(pool: &SqlitePool, id: &str) {
        repo_projects::insert_project(
            pool,
            &InsertProject {
                id,
                name: id,
                tool: "PixInsight",
                lifecycle: "setup_incomplete",
                path: &format!("projects/{id}"),
                notes: None,
                canonical_target_id: None,
                is_mosaic: false,
            },
        )
        .await
        .unwrap();
    }

    async fn link_project_to_session(pool: &SqlitePool, project_id: &str, session_id: &str) {
        repo_projects::insert_project_source(
            pool,
            &repo_projects::InsertProjectSource {
                id: &domain_core::ids::new_id(),
                project_id,
                inventory_session_id: session_id,
                name_snapshot: "",
                frames_snapshot: 0,
                filter_snapshot: "",
                exposure_snapshot: "",
                linked_at: "2026-01-01T00:00:00Z",
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn propagate_updates_linked_project_canonical_target() {
        let db = test_db().await;
        seed_canonical_target(db.pool(), "target-1", "M 31").await;
        seed_project(db.pool(), "proj-1").await;
        link_project_to_session(db.pool(), "proj-1", "session-1").await;

        propagate_target_to_projects(db.pool(), "session-1", "target-1").await.unwrap();

        let got =
            repo_projects::get_project_canonical_target_id(db.pool(), "proj-1").await.unwrap();
        assert_eq!(got.as_deref(), Some("target-1"));
    }

    #[tokio::test]
    async fn propagate_never_overwrites_an_existing_project_target() {
        let db = test_db().await;
        seed_canonical_target(db.pool(), "target-old", "M 42").await;
        seed_canonical_target(db.pool(), "target-new", "M 31").await;
        seed_project(db.pool(), "proj-1").await;
        // Project already carries a target — manually picked at project
        // creation (spec-035 gap #1) or from an earlier propagation. Either
        // way, propagation is first-write-wins: it must not clobber it.
        repo_projects::set_project_canonical_target_id(db.pool(), "proj-1", "target-old")
            .await
            .unwrap();
        link_project_to_session(db.pool(), "proj-1", "session-1").await;

        propagate_target_to_projects(db.pool(), "session-1", "target-new").await.unwrap();

        let got =
            repo_projects::get_project_canonical_target_id(db.pool(), "proj-1").await.unwrap();
        assert_eq!(got.as_deref(), Some("target-old"), "existing target must not be overwritten");
    }

    #[tokio::test]
    async fn propagate_with_no_linked_project_is_a_noop() {
        let db = test_db().await;
        seed_canonical_target(db.pool(), "target-1", "M 31").await;

        // No project_sources row for "session-lonely" — must not error or panic.
        propagate_target_to_projects(db.pool(), "session-lonely", "target-1").await.unwrap();
    }

    #[tokio::test]
    async fn upsert_session_insert_branch_with_no_linked_project_does_not_error() {
        let db = test_db().await;
        seed_canonical_target(db.pool(), "target-1", "M 31").await;
        seed_library_root(db.pool(), "root-1").await;

        // A brand-new session (INSERT branch of upsert_session) resolves a
        // target immediately; `project_sources` cannot yet reference this
        // session's id (a project can only link an *existing* session in the
        // real UI flow), so there is no linked project. The INSERT-branch
        // propagation call must be a safe no-op, not a panic/error.
        upsert_session(db.pool(), "sk-1", false, Some("target-1"), "image-1", "root-1")
            .await
            .unwrap();

        let (canonical_target_id,): (Option<String>,) = sqlx::query_as(
            "SELECT canonical_target_id FROM acquisition_session WHERE session_key = 'sk-1'",
        )
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(canonical_target_id.as_deref(), Some("target-1"));
    }

    /// Regression test for #470 round 6: `acquisition_session.root_id`
    /// (migration 0021) was never written by real ingest — `upsert_session`'s
    /// INSERT/UPDATE statements simply omitted the column, and the
    /// `update_acquisition_session_root_id` repo fn that existed specifically
    /// to set it (T036/FR-012) was never called from anywhere. Every
    /// downstream reader that assumes `root_id` is populated
    /// (`app_core_projects::source_view_generate`) got sqlx's silent
    /// NULL-into-`String` coercion to `""` instead, and failed with a
    /// confusing `no_link_kind`/"source root  could not be resolved" (double
    /// space) rather than a clear error.
    ///
    /// Registers the root through the REAL `roots.register` repo path
    /// (`persistence_lifecycle::repositories::first_run::register_source`) — a
    /// `registered_sources` row, NOT a hand-seeded `library_root` row — and
    /// mirrors it via the same `ensure_library_root` (R9) call the real
    /// `ingest_light_frame` pipeline makes, so this test exercises the actual
    /// production root-resolution path rather than a `library_root`-only
    /// fixture shortcut that would mask the same gap the backend unit test
    /// for `source_view_generate` originally did.
    #[tokio::test]
    async fn upsert_session_persists_root_id_from_a_really_registered_source() {
        let db = test_db().await;

        let register_req = domain_core::first_run::RegisterSourceRequest {
            kind: domain_core::first_run::SourceKind::LightFrames,
            path: "/tmp/e2e-lights".to_owned(),
            kind_subtype: None,
            scan_depth: domain_core::first_run::ScanDepth::Recursive,
            organization_state: domain_core::first_run::OrganizationState::Organized,
        };
        let registered = persistence_lifecycle::repositories::first_run::register_source(
            db.pool(),
            &register_req,
        )
        .await
        .unwrap();

        // R9: the same mirroring `ingest_light_frame` performs before ever
        // calling `upsert_session` — real users hit this via the applied
        // plan's `to_root_id`, which is a `registered_sources` id.
        let mirrored_path = ensure_library_root(db.pool(), &registered.source_id).await.unwrap();
        assert_eq!(mirrored_path.as_deref(), Some("/tmp/e2e-lights"));

        upsert_session(db.pool(), "sk-root-real", false, None, "image-1", &registered.source_id)
            .await
            .unwrap();

        let (root_id,): (String,) =
            sqlx::query_as("SELECT root_id FROM acquisition_session WHERE session_key = ?")
                .bind("sk-root-real")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(
            root_id, registered.source_id,
            "acquisition_session.root_id must be set to the real registered source id, not left \
             empty/unset"
        );
    }

    #[tokio::test]
    async fn upsert_session_backfill_branch_propagates_when_project_prelinked() {
        let db = test_db().await;
        seed_canonical_target(db.pool(), "target-1", "M 31").await;
        seed_project(db.pool(), "proj-1").await;
        seed_library_root(db.pool(), "root-1").await;

        // First frame arrives with no resolvable target — session created NULL.
        upsert_session(db.pool(), "sk-2", false, None, "image-1", "root-1").await.unwrap();
        let (session_id,): (String,) =
            sqlx::query_as("SELECT id FROM acquisition_session WHERE session_key = 'sk-2'")
                .fetch_one(db.pool())
                .await
                .unwrap();
        link_project_to_session(db.pool(), "proj-1", &session_id).await;

        // Second frame in the same session resolves — back-fill branch runs and
        // must propagate to the now-linked project.
        upsert_session(db.pool(), "sk-2", false, Some("target-1"), "image-2", "root-1")
            .await
            .unwrap();

        let got =
            repo_projects::get_project_canonical_target_id(db.pool(), "proj-1").await.unwrap();
        assert_eq!(got.as_deref(), Some("target-1"));
    }

    #[tokio::test]
    async fn backfill_session_targets_propagates_to_linked_project() {
        let db = test_db().await;
        seed_canonical_target(db.pool(), "target-1", "M 31").await;
        seed_project(db.pool(), "proj-1").await;

        // `ingest_resolution.image_id` FKs to `file_record(id)`, which itself
        // FKs to `library_root(id)` — seed both minimally so the resolved-row
        // insert below satisfies the constraints.
        sqlx::query(
            "INSERT INTO library_root (id, label, current_path, kind, state, created_at)
             VALUES ('root-1', 'root-1', '/tmp/root-1', 'local', 'active', '2026-01-01T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO file_record
                (id, root_id, relative_path, size_bytes, mtime, state, first_seen_at, last_seen_at)
             VALUES ('image-1', 'root-1', 'lights/light_001.fits', 0, '2026-01-01T00:00:00Z',
                     'classified', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();

        // A session with an unresolved (NULL) target, one frame.
        upsert_session(db.pool(), "sk-3", false, None, "image-1", "root-1").await.unwrap();
        let (session_id,): (String,) =
            sqlx::query_as("SELECT id FROM acquisition_session WHERE session_key = 'sk-3'")
                .fetch_one(db.pool())
                .await
                .unwrap();
        link_project_to_session(db.pool(), "proj-1", &session_id).await;

        // The frame resolves later (background drain) — insert a `resolved`
        // ingest_resolution row for it.
        sqlx::query(
            "INSERT INTO ingest_resolution (id, image_id, state, target_id, object_raw, attempts)
             VALUES (?, 'image-1', 'resolved', 'target-1', 'M 31', 1)",
        )
        .bind(domain_core::ids::new_id())
        .execute(db.pool())
        .await
        .unwrap();

        let linked = backfill_session_targets(db.pool()).await.unwrap();
        assert_eq!(linked, 1);

        let got =
            repo_projects::get_project_canonical_target_id(db.pool(), "proj-1").await.unwrap();
        assert_eq!(got.as_deref(), Some("target-1"));
    }

    // ── backfill_session_geometry (F-Framing-5) ────────────────────────────────

    #[tokio::test]
    async fn backfill_session_geometry_populates_from_first_frame() {
        let db = test_db().await;
        seed_library_root(db.pool(), "root-geo").await;
        let session_id = upsert_session(db.pool(), "sk-geo-1", false, None, "image-1", "root-geo")
            .await
            .unwrap();

        let meta = RawFileMetadata {
            telescop: Some("RASA 8".to_owned()),
            instrume: Some("ASI2600MM".to_owned()),
            focal_length_mm: Some(400.0),
            ra_deg: Some(83.633),
            dec_deg: Some(22.0145),
            rotator_angle_deg: Some(1.5),
            ..RawFileMetadata::default()
        };
        backfill_session_geometry(db.pool(), &session_id, &meta).await.unwrap();

        let geo =
            framing_repo::get_session_geometry(db.pool(), &session_id).await.unwrap().unwrap();
        assert_eq!(geo.pointing_ra_deg, Some(83.633));
        assert_eq!(geo.pointing_dec_deg, Some(22.0145));
        assert_eq!(geo.rotation_deg, Some(1.5));
        assert_eq!(geo.optic_train_key.as_deref(), Some("rasa 8|asi2600mm|400"));
    }

    /// Q16 null semantics: a later frame's missing header data must never
    /// regress an already-known geometry field to NULL (mirrors
    /// `upsert_session`'s `root_id` COALESCE precedent).
    #[tokio::test]
    async fn backfill_session_geometry_never_regresses_a_known_field_to_null() {
        let db = test_db().await;
        seed_library_root(db.pool(), "root-geo2").await;
        let session_id = upsert_session(db.pool(), "sk-geo-2", false, None, "image-1", "root-geo2")
            .await
            .unwrap();

        let complete = RawFileMetadata {
            telescop: Some("RASA 8".to_owned()),
            instrume: Some("ASI2600MM".to_owned()),
            focal_length_mm: Some(400.0),
            ra_deg: Some(83.633),
            dec_deg: Some(22.0145),
            rotator_angle_deg: Some(1.5),
            ..RawFileMetadata::default()
        };
        backfill_session_geometry(db.pool(), &session_id, &complete).await.unwrap();

        // A second frame in the same session with a blank header.
        let empty = RawFileMetadata::default();
        backfill_session_geometry(db.pool(), &session_id, &empty).await.unwrap();

        let geo =
            framing_repo::get_session_geometry(db.pool(), &session_id).await.unwrap().unwrap();
        assert_eq!(geo.pointing_ra_deg, Some(83.633), "known RA must not regress to NULL");
        assert_eq!(geo.optic_train_key.as_deref(), Some("rasa 8|asi2600mm|400"));
    }

    // ── bind_chosen_framing (F-Framing-10) ──────────────────────────────────────

    async fn seed_project_row(pool: &SqlitePool, id: &str) {
        repo_projects::insert_project(
            pool,
            &repo_projects::InsertProject {
                id,
                name: id,
                tool: "PixInsight",
                lifecycle: "ready",
                path: &format!("projects/{id}"),
                notes: None,
                canonical_target_id: None,
                is_mosaic: false,
            },
        )
        .await
        .unwrap();
    }

    async fn seed_framing_row(pool: &SqlitePool, id: &str, project_id: &str) {
        framing_repo::insert_framing(
            pool,
            &framing_repo::InsertFraming {
                id,
                project_id,
                target_id: None,
                optic_train_key: "rasa 8|asi2600mm|400",
                pointing_ra_deg: 83.633,
                pointing_dec_deg: 22.0145,
                rotation_deg: 0.0,
                tolerance_pointing: 0.1,
                tolerance_rotation_deg: 3.0,
                clustering: "suggested",
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn bind_chosen_framing_adds_the_session_when_the_plan_has_a_pick() {
        let db = test_db().await;
        seed_project_row(db.pool(), "proj-bind").await;
        seed_framing_row(db.pool(), "framing-bind", "proj-bind").await;
        plans_repo::insert_plan(
            db.pool(),
            &plans_repo::InsertPlan {
                id: "plan-bind",
                title: "test",
                origin: "inbox",
                origin_path: None,
                plan_type: "split",
                destructive_destination: "archive",
                parent_plan_id: None,
                total_bytes_required: 0,
            },
        )
        .await
        .unwrap();
        plans_repo::set_chosen_framing_id(db.pool(), "plan-bind", "framing-bind").await.unwrap();

        seed_library_root(db.pool(), "root-bind").await;
        let session_id = upsert_session(db.pool(), "sk-bind", false, None, "image-1", "root-bind")
            .await
            .unwrap();

        bind_chosen_framing(db.pool(), "plan-bind", &session_id).await.unwrap();

        assert_eq!(
            framing_repo::get_framing_id_for_session(db.pool(), &session_id).await.unwrap(),
            Some("framing-bind".to_owned())
        );
    }

    #[tokio::test]
    async fn bind_chosen_framing_is_a_noop_when_the_plan_has_no_pick() {
        let db = test_db().await;
        plans_repo::insert_plan(
            db.pool(),
            &plans_repo::InsertPlan {
                id: "plan-nopick",
                title: "test",
                origin: "inbox",
                origin_path: None,
                plan_type: "split",
                destructive_destination: "archive",
                parent_plan_id: None,
                total_bytes_required: 0,
            },
        )
        .await
        .unwrap();
        seed_library_root(db.pool(), "root-nopick").await;
        let session_id =
            upsert_session(db.pool(), "sk-nopick", false, None, "image-1", "root-nopick")
                .await
                .unwrap();

        bind_chosen_framing(db.pool(), "plan-nopick", &session_id).await.unwrap();

        assert_eq!(
            framing_repo::get_framing_id_for_session(db.pool(), &session_id).await.unwrap(),
            None
        );
    }
}
