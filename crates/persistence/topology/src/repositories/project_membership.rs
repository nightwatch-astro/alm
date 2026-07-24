// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only
#![allow(clippy::missing_errors_doc, clippy::too_many_lines, clippy::type_complexity)]

//! Project membership persistence: `spec062_project`, `project_membership_revision`,
//! `project_membership_revision_session`, and the related-session derivation query.
//!
//! All writes acquire a `BEGIN IMMEDIATE` connection supplied by the caller and
//! use CAS on `membership_head_generation`. The repository never issues its own
//! transaction boundaries.

use sqlx::SqliteConnection;

use persistence_core::{DbError, DbResult};

// ── Row types ─────────────────────────────────────────────────────────────────

/// Minimal `spec062_project` projection.
#[derive(Debug, Clone)]
pub struct Spec062ProjectRow {
    pub row_id: i64,
    pub public_id: String,
    pub membership_head_revision_row_id: Option<i64>,
    pub membership_head_generation: i64,
    pub created_at: String,
}

/// `project_membership_revision` row.
#[derive(Debug, Clone)]
pub struct MembershipRevisionRow {
    pub row_id: i64,
    pub public_id: String,
    pub project_row_id: i64,
    pub revision_number: i64,
    pub parent_revision_row_id: Option<i64>,
    pub proposal_row_id: Option<i64>,
    pub actor_row_id: i64,
    pub created_sequence: i64,
    pub created_at: String,
}

/// One pinned session row from `project_membership_revision_session`.
#[derive(Debug, Clone)]
pub struct PinRow {
    pub revision_row_id: i64,
    pub session_row_id: i64,
    pub session_public_id: String,
    pub pin_revision: i64,
    pub source: String,
    pub replaces_session_row_id: Option<i64>,
    pub replaces_session_public_id: Option<String>,
    pub applied_reclassification_plan_revision_row_id: Option<i64>,
    pub applied_reclassification_plan_revision_public_id: Option<String>,
    pub pinned_by_actor_row_id: i64,
    pub pinned_at: String,
}

/// Derived related-session row (read-only projection).
#[derive(Debug, Clone)]
pub struct RelatedSessionRow {
    pub session_row_id: i64,
    pub session_public_id: String,
    /// `"panel_sibling"` or `"session_replacement"`
    pub relation_kind: String,
    /// Panel group public_id for sibling relations; None for replacement.
    pub panel_group_public_id: Option<String>,
    /// Panel head revision public_id used as `evidenceId` for sibling.
    pub evidence_revision_public_id: Option<String>,
    /// Applied reclassification plan revision public_id for replacements.
    pub reclassification_revision_public_id: Option<String>,
    pub first_available_at: String,
    /// True when this session is already pinned in the current membership head.
    pub already_pinned: bool,
}

// ── Input types ───────────────────────────────────────────────────────────────

/// Parameters for inserting one `project_membership_revision` row.
pub struct InsertMembershipRevision<'a> {
    pub public_id: &'a str,
    pub project_row_id: i64,
    pub revision_number: i64,
    pub parent_revision_row_id: Option<i64>,
    /// Nullable — the project.session_pin.add command does not use a proposal.
    pub proposal_row_id: Option<i64>,
    pub actor_row_id: i64,
    pub created_sequence: i64,
    pub created_at: &'a str,
}

/// One session pin to insert in `project_membership_revision_session`.
pub struct InsertPin<'a> {
    pub revision_row_id: i64,
    pub session_row_id: i64,
    pub pin_revision: i64,
    /// `"explicit_add"`, `"explicit_replacement"`, or `"project_creation"`.
    pub source: &'a str,
    pub replaces_session_row_id: Option<i64>,
    pub applied_reclassification_plan_revision_row_id: Option<i64>,
    pub pinned_by_actor_row_id: i64,
    pub pinned_at: &'a str,
}

// ── Queries ───────────────────────────────────────────────────────────────────

/// Fetch a `spec062_project` row by public UUID.
///
/// Returns `DbError::NotFound` when absent.
pub async fn get_spec062_project(
    conn: &mut SqliteConnection,
    public_id: &str,
) -> DbResult<Spec062ProjectRow> {
    let row: Option<(i64, String, Option<i64>, i64, String)> = sqlx::query_as(
        "SELECT row_id, public_id, membership_head_revision_row_id,
                membership_head_generation, created_at
         FROM spec062_project WHERE public_id = ?",
    )
    .bind(public_id)
    .fetch_optional(&mut *conn)
    .await?;

    row.map(|(row_id, public_id, head_rev, head_gen, created_at)| Spec062ProjectRow {
        row_id,
        public_id,
        membership_head_revision_row_id: head_rev,
        membership_head_generation: head_gen,
        created_at,
    })
    .ok_or_else(|| DbError::NotFound(format!("spec062_project {public_id}")))
}

/// Fetch the current membership revision and all its pins.
///
/// Returns `Ok(None)` when the project has no membership head yet (no sessions
/// have ever been pinned).
pub async fn get_current_membership_pins(
    conn: &mut SqliteConnection,
    project_row_id: i64,
    head_revision_row_id: Option<i64>,
) -> DbResult<Option<(MembershipRevisionRow, Vec<PinRow>)>> {
    let Some(rev_row_id) = head_revision_row_id else {
        return Ok(None);
    };

    let rev_row: Option<(i64, String, i64, i64, Option<i64>, Option<i64>, i64, i64, String)> =
        sqlx::query_as(
            "SELECT row_id, public_id, project_row_id, revision_number,
                    parent_revision_row_id, proposal_row_id, actor_row_id,
                    created_sequence, created_at
             FROM project_membership_revision
             WHERE row_id = ? AND project_row_id = ?",
        )
        .bind(rev_row_id)
        .bind(project_row_id)
        .fetch_optional(&mut *conn)
        .await?;

    let Some((row_id, public_id, proj_row_id, rev_num, parent, proposal, actor, seq, created)) =
        rev_row
    else {
        return Ok(None);
    };

    let rev = MembershipRevisionRow {
        row_id,
        public_id,
        project_row_id: proj_row_id,
        revision_number: rev_num,
        parent_revision_row_id: parent,
        proposal_row_id: proposal,
        actor_row_id: actor,
        created_sequence: seq,
        created_at: created,
    };

    let pins = fetch_pins_for_revision(conn, rev_row_id).await?;
    Ok(Some((rev, pins)))
}

/// Fetch all pinned session rows for a given membership revision.
pub async fn fetch_pins_for_revision(
    conn: &mut SqliteConnection,
    revision_row_id: i64,
) -> DbResult<Vec<PinRow>> {
    let rows: Vec<(
        i64,
        i64,
        String,
        i64,
        String,
        Option<i64>,
        Option<String>,
        Option<i64>,
        Option<String>,
        i64,
        String,
    )> = sqlx::query_as(
        "SELECT
             prs.revision_row_id,
             prs.session_row_id,
             s.public_id        AS session_public_id,
             prs.pin_revision,
             prs.source,
             prs.replaces_session_row_id,
             rs.public_id       AS replaces_session_public_id,
             prs.applied_reclassification_plan_revision_row_id,
             rpr.public_id      AS applied_reclassification_plan_revision_public_id,
             prs.pinned_by_actor_row_id,
             prs.pinned_at
         FROM project_membership_revision_session prs
         INNER JOIN session s ON s.row_id = prs.session_row_id
         LEFT JOIN session rs ON rs.row_id = prs.replaces_session_row_id
         LEFT JOIN reclassification_plan_revision rpr
             ON rpr.row_id = prs.applied_reclassification_plan_revision_row_id
         WHERE prs.revision_row_id = ?",
    )
    .bind(revision_row_id)
    .fetch_all(&mut *conn)
    .await?;

    Ok(rows
        .into_iter()
        .map(
            |(
                rev_row_id,
                session_row_id,
                session_public_id,
                pin_revision,
                source,
                replaces_row,
                replaces_pub,
                reclass_rev_row,
                reclass_rev_pub,
                actor_row,
                pinned_at,
            )| {
                PinRow {
                    revision_row_id: rev_row_id,
                    session_row_id,
                    session_public_id,
                    pin_revision,
                    source,
                    replaces_session_row_id: replaces_row,
                    replaces_session_public_id: replaces_pub,
                    applied_reclassification_plan_revision_row_id: reclass_rev_row,
                    applied_reclassification_plan_revision_public_id: reclass_rev_pub,
                    pinned_by_actor_row_id: actor_row,
                    pinned_at,
                }
            },
        )
        .collect())
}

/// Look up a session `row_id` and creation timestamp by public UUID.
///
/// Returns `DbError::NotFound` when the session does not exist.
pub async fn lookup_session_row_id(
    conn: &mut SqliteConnection,
    public_id: &str,
) -> DbResult<(i64, String)> {
    let row: Option<(i64, String)> =
        sqlx::query_as("SELECT row_id, created_at FROM session WHERE public_id = ?")
            .bind(public_id)
            .fetch_optional(&mut *conn)
            .await?;
    row.ok_or_else(|| DbError::NotFound(format!("session {public_id}")))
}

/// Look up a reclassification plan revision by public UUID and verify it is in
/// `applied` state.
///
/// Returns `(row_id, replacement_session_row_ids_and_public_ids)`.
///
/// Replacement sessions are derived from `session_supersession` rows whose
/// `applied_plan_revision_row_id` matches this revision.
///
/// Returns `DbError::NotFound` when the revision is absent or not `applied`.
pub async fn lookup_applied_reclassification_revision(
    conn: &mut SqliteConnection,
    public_id: &str,
) -> DbResult<(i64, Vec<(i64, String)>)> {
    let rev_row: Option<(i64, String)> = sqlx::query_as(
        "SELECT row_id, state FROM reclassification_plan_revision WHERE public_id = ?",
    )
    .bind(public_id)
    .fetch_optional(&mut *conn)
    .await?;

    let Some((row_id, state)) = rev_row else {
        return Err(DbError::NotFound(format!("reclassification_plan_revision {public_id}")));
    };

    if state != "applied" {
        return Err(DbError::NotFound(format!(
            "reclassification_plan_revision {public_id} state={state}, expected applied"
        )));
    }

    // `session_supersession.applied_plan_revision_row_id` is the FK column name.
    let replacements: Vec<(i64, String)> = sqlx::query_as(
        "SELECT ss.replacement_session_row_id, s.public_id
         FROM session_supersession ss
         INNER JOIN session s ON s.row_id = ss.replacement_session_row_id
         WHERE ss.applied_plan_revision_row_id = ?
         ORDER BY s.public_id ASC",
    )
    .bind(row_id)
    .fetch_all(&mut *conn)
    .await?;

    Ok((row_id, replacements))
}

/// Look up the `spec062_actor` row_id for a public UUID, inserting if absent.
///
/// This mirrors `persistence_sessions::repositories::actors::ensure_spec062_actor`.
pub async fn ensure_spec062_actor(
    conn: &mut SqliteConnection,
    public_id: &str,
    created_at: &str,
) -> DbResult<i64> {
    sqlx::query(
        "INSERT INTO spec062_actor(public_id, created_at)
         VALUES (?, ?)
         ON CONFLICT(public_id) DO NOTHING",
    )
    .bind(public_id)
    .bind(created_at)
    .execute(&mut *conn)
    .await?;
    let row: (i64,) = sqlx::query_as("SELECT row_id FROM spec062_actor WHERE public_id = ?")
        .bind(public_id)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.0)
}

// ── Writes ────────────────────────────────────────────────────────────────────

/// Insert one `project_membership_revision` row and return its `row_id`.
///
/// The caller is responsible for holding `BEGIN IMMEDIATE` and for inserting
/// all pin rows and advancing the head CAS before commit.
pub async fn insert_membership_revision(
    conn: &mut SqliteConnection,
    params: &InsertMembershipRevision<'_>,
) -> DbResult<i64> {
    sqlx::query(
        "INSERT INTO project_membership_revision
             (public_id, project_row_id, revision_number, parent_revision_row_id,
              proposal_row_id, actor_row_id, created_sequence, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(params.public_id)
    .bind(params.project_row_id)
    .bind(params.revision_number)
    .bind(params.parent_revision_row_id)
    .bind(params.proposal_row_id)
    .bind(params.actor_row_id)
    .bind(params.created_sequence)
    .bind(params.created_at)
    .execute(&mut *conn)
    .await?;

    let (row_id,): (i64,) =
        sqlx::query_as("SELECT row_id FROM project_membership_revision WHERE public_id = ?")
            .bind(params.public_id)
            .fetch_one(&mut *conn)
            .await?;
    Ok(row_id)
}

/// Insert one `project_membership_revision_session` pin row.
pub async fn insert_pin(conn: &mut SqliteConnection, params: &InsertPin<'_>) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO project_membership_revision_session
             (revision_row_id, session_row_id, pin_revision, source,
              replaces_session_row_id, applied_reclassification_plan_revision_row_id,
              pinned_by_actor_row_id, pinned_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(params.revision_row_id)
    .bind(params.session_row_id)
    .bind(params.pin_revision)
    .bind(params.source)
    .bind(params.replaces_session_row_id)
    .bind(params.applied_reclassification_plan_revision_row_id)
    .bind(params.pinned_by_actor_row_id)
    .bind(params.pinned_at)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Advance the `spec062_project` membership head with CAS.
///
/// Returns `DbError::CasFailed` when the generation does not match — the caller
/// must roll back the transaction.
pub async fn advance_membership_head(
    conn: &mut SqliteConnection,
    project_row_id: i64,
    new_revision_row_id: i64,
    expected_generation: i64,
    created_sequence: i64,
) -> DbResult<()> {
    // Insert history row for the outgoing generation (if there was a previous
    // head) before advancing. The head_history table records the sequence at
    // which this generation became active.
    sqlx::query(
        "INSERT INTO project_membership_head_history
             (project_row_id, generation, head_revision_row_id, accepted_sequence)
         VALUES (?, ?, ?, ?)",
    )
    .bind(project_row_id)
    .bind(expected_generation + 1)
    .bind(new_revision_row_id)
    .bind(created_sequence)
    .execute(&mut *conn)
    .await?;

    let result = sqlx::query(
        "UPDATE spec062_project
         SET membership_head_revision_row_id = ?,
             membership_head_generation = membership_head_generation + 1
         WHERE row_id = ?
           AND membership_head_generation = ?",
    )
    .bind(new_revision_row_id)
    .bind(project_row_id)
    .bind(expected_generation)
    .execute(&mut *conn)
    .await?;

    if result.rows_affected() != 1 {
        return Err(DbError::CasFailed(format!(
            "spec062_project row_id={project_row_id} membership head CAS failed \
             (expected generation {expected_generation})"
        )));
    }
    Ok(())
}

/// Derive the next `pin_revision` for a session being added to a project.
///
/// `pin_revision` is the per-session, per-project monotone counter: how many
/// times this exact session has ever appeared in any revision of this project.
/// Returns 1 on the first add.
pub async fn next_pin_revision(
    conn: &mut SqliteConnection,
    project_row_id: i64,
    session_row_id: i64,
) -> DbResult<i64> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COALESCE(MAX(prs.pin_revision), 0) + 1
         FROM project_membership_revision_session prs
         INNER JOIN project_membership_revision pmr
             ON pmr.row_id = prs.revision_row_id
         WHERE pmr.project_row_id = ?
           AND prs.session_row_id = ?",
    )
    .bind(project_row_id)
    .bind(session_row_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(row.0)
}

// ── Related-session query ─────────────────────────────────────────────────────

/// Read the lifecycle string for a project from the legacy `projects` table.
///
/// Returns `"setup_incomplete"` when no legacy row exists (pure spec-062 projects
/// that have not been linked to a legacy row).
pub async fn fetch_legacy_lifecycle(
    conn: &mut SqliteConnection,
    project_public_id: &str,
) -> DbResult<String> {
    let row: Option<(String,)> = sqlx::query_as("SELECT lifecycle FROM projects WHERE id = ?")
        .bind(project_public_id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map_or_else(|| "setup_incomplete".to_owned(), |(lc,)| lc))
}

/// Return the current revision number of the given membership revision row.
pub async fn fetch_revision_number(
    conn: &mut SqliteConnection,
    revision_row_id: i64,
) -> DbResult<i64> {
    let (num,): (i64,) =
        sqlx::query_as("SELECT revision_number FROM project_membership_revision WHERE row_id = ?")
            .bind(revision_row_id)
            .fetch_one(&mut *conn)
            .await?;
    Ok(num)
}

/// True when the project has at least one materialization snapshot and the
/// newest snapshot does not include `session_row_id`.
///
/// Returns `false` when there is no snapshot at all.
pub async fn is_view_stale_after_add(
    conn: &mut SqliteConnection,
    project_row_id: i64,
    session_row_id: i64,
) -> DbResult<bool> {
    let has_snapshot: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM project_materialization_snapshot WHERE project_row_id = ?",
    )
    .bind(project_row_id)
    .fetch_one(&mut *conn)
    .await?;

    if has_snapshot.0 == 0 {
        return Ok(false);
    }

    let in_snapshot: (i64,) = sqlx::query_as(
        "SELECT COUNT(*)
         FROM project_materialization_snapshot pms
         INNER JOIN project_materialization_snapshot_session pmss
             ON pmss.snapshot_row_id = pms.row_id
         WHERE pms.project_row_id = ?
           AND pmss.session_row_id = ?
           AND pms.row_id = (
               SELECT MAX(row_id) FROM project_materialization_snapshot
               WHERE project_row_id = ?
           )",
    )
    .bind(project_row_id)
    .bind(session_row_id)
    .bind(project_row_id)
    .fetch_one(&mut *conn)
    .await?;

    Ok(in_snapshot.0 == 0)
}

/// Count sessions in a membership revision that are absent from the most recent
/// materialization snapshot. Returns `(unmaterialized_count, stale)`.
pub async fn unmaterialized_session_count(
    conn: &mut SqliteConnection,
    project_row_id: i64,
    head_revision_row_id: Option<i64>,
) -> DbResult<(i64, bool)> {
    let Some(rev_id) = head_revision_row_id else {
        return Ok((0, false));
    };

    let latest_snap: Option<(i64,)> = sqlx::query_as(
        "SELECT MAX(row_id) FROM project_materialization_snapshot WHERE project_row_id = ?",
    )
    .bind(project_row_id)
    .fetch_optional(&mut *conn)
    .await?;

    // When there is no snapshot, no view exists to be stale against.
    // Unmaterialized count is still meaningful for callers (they can see
    // how many sessions haven't been materialized yet) but stale=false.
    let snap_opt = latest_snap.and_then(|(id,)| if id == 0 { None } else { Some(id) });
    let Some(snap_row_id) = snap_opt else {
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM project_membership_revision_session WHERE revision_row_id = ?",
        )
        .bind(rev_id)
        .fetch_one(&mut *conn)
        .await?;
        // No materialization snapshot → not stale (no view to be stale against).
        return Ok((count, false));
    };

    let (not_in_snap,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*)
         FROM project_membership_revision_session prs
         WHERE prs.revision_row_id = ?
           AND NOT EXISTS (
               SELECT 1 FROM project_materialization_snapshot_session pmss
               WHERE pmss.snapshot_row_id = ?
                 AND pmss.session_row_id = prs.session_row_id
           )",
    )
    .bind(rev_id)
    .bind(snap_row_id)
    .fetch_one(&mut *conn)
    .await?;

    Ok((not_in_snap, not_in_snap > 0))
}

/// Count sessions currently pinned in a revision.
pub async fn count_pinned_sessions(
    conn: &mut SqliteConnection,
    revision_row_id: i64,
) -> DbResult<i64> {
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM project_membership_revision_session WHERE revision_row_id = ?",
    )
    .bind(revision_row_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(count)
}

/// List pinned session public IDs for a revision, keyset-paginated by `session.public_id ASC`.
///
/// Returns rows as `(session_row_id, session_public_id, pin_revision, pinned_at, source,
///                   replaces_session_public_id)`.
pub async fn list_pins_paged(
    conn: &mut SqliteConnection,
    revision_row_id: i64,
    cursor_session_public_id: Option<&str>,
    page_size: i64,
) -> DbResult<Vec<(i64, String, i64, String, String, Option<String>, String)>> {
    let rows = sqlx::query_as(
        "SELECT prs.session_row_id, s.public_id AS session_id,
                prs.pin_revision, prs.pinned_at, prs.source,
                rs.public_id AS replaces_session_id,
                a.public_id  AS pinned_by
         FROM project_membership_revision_session prs
         INNER JOIN session s ON s.row_id = prs.session_row_id
         LEFT JOIN session rs ON rs.row_id = prs.replaces_session_row_id
         INNER JOIN spec062_actor a ON a.row_id = prs.pinned_by_actor_row_id
         WHERE prs.revision_row_id = ?
           AND (?2 IS NULL OR s.public_id > ?2)
         ORDER BY s.public_id ASC
         LIMIT ?3",
    )
    .bind(revision_row_id)
    .bind(cursor_session_public_id)
    .bind(page_size)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

/// List related sessions available to a project.
///
/// A session is related when it satisfies one or more of:
/// - It is a panel sibling of a pinned session (in the same active panel head,
///   not already a same-session member, and shares the same canonical target).
/// - It is an authorized replacement for a pinned session (exists in
///   `session_supersession` with the pinned session as predecessor).
///
/// `page_size` controls the maximum rows returned; `cursor_first_available_at`
/// and `cursor_session_public_id` provide keyset pagination in
/// `(firstAvailableAt DESC, sessionId ASC)` order.
///
/// The `include_pinned` flag controls whether already-pinned sessions are
/// returned.
pub async fn list_related_sessions(
    conn: &mut SqliteConnection,
    _project_row_id: i64,
    head_revision_row_id: Option<i64>,
    include_pinned: bool,
    cursor_first_available_at: Option<&str>,
    cursor_session_public_id: Option<&str>,
    page_size: i64,
) -> DbResult<Vec<RelatedSessionRow>> {
    // When there is no membership head the project has no pins, so there are
    // no siblings or replacements to derive.
    let Some(head_rev_id) = head_revision_row_id else {
        return Ok(Vec::new());
    };

    // Panel-sibling relations: sessions in the same active panel head as a
    // pinned session, excluding the pinned session itself.
    //
    // Replacement relations: sessions authorized by an applied reclassification
    // plan whose predecessor is a pinned session.
    let rows: Vec<(
        i64,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        i64,
    )> = sqlx::query_as(
        "WITH pinned AS (
                 SELECT prs.session_row_id
                 FROM project_membership_revision_session prs
                 WHERE prs.revision_row_id = ?1
             ),
             -- Panel siblings: sessions in the same active panel group head
             -- as a pinned session, excluding the pinned session itself.
             siblings AS (
                 SELECT
                     related_s.row_id           AS session_row_id,
                     related_s.public_id        AS session_public_id,
                     'panel_sibling'            AS relation_kind,
                     g.public_id                AS panel_group_public_id,
                     head_rev.public_id         AS evidence_revision_public_id,
                     NULL                       AS reclassification_revision_public_id,
                     related_s.created_at       AS first_available_at
                 FROM pinned p
                 -- Find the active panel group for each pinned session.
                 JOIN panel_revision_session anchor_prs
                     ON anchor_prs.session_row_id = p.session_row_id
                 JOIN panel_group_revision anchor_pgr
                     ON anchor_pgr.row_id = anchor_prs.panel_revision_row_id
                 JOIN panel_group g
                     ON g.row_id = anchor_pgr.panel_group_row_id
                    AND g.head_revision_row_id = anchor_pgr.row_id
                    AND g.status = 'active'
                 -- The head revision is what the anchor session is in.
                 JOIN panel_group_revision head_rev
                     ON head_rev.row_id = g.head_revision_row_id
                 -- Other sessions in that same head revision.
                 JOIN panel_revision_session sibling_prs
                     ON sibling_prs.panel_revision_row_id = head_rev.row_id
                    AND sibling_prs.session_row_id <> p.session_row_id
                 JOIN session related_s ON related_s.row_id = sibling_prs.session_row_id
                 -- Exclude superseded sessions.
                 WHERE NOT EXISTS (
                     SELECT 1 FROM session_supersession
                     WHERE predecessor_session_row_id = related_s.row_id
                 )
             ),
             -- Replacement relations: sessions authorized by an applied
             -- reclassification plan where a pinned session is the predecessor.
             replacements AS (
                 SELECT
                     repl_s.row_id              AS session_row_id,
                     repl_s.public_id           AS session_public_id,
                     'session_replacement'      AS relation_kind,
                     NULL                       AS panel_group_public_id,
                     NULL                       AS evidence_revision_public_id,
                     rpr.public_id              AS reclassification_revision_public_id,
                     repl_s.created_at          AS first_available_at
                 FROM pinned p
                 JOIN session_supersession ss
                     ON ss.predecessor_session_row_id = p.session_row_id
                 JOIN session repl_s ON repl_s.row_id = ss.replacement_session_row_id
                 JOIN reclassification_plan_revision rpr
                     ON rpr.row_id = ss.applied_plan_revision_row_id
             ),
             combined AS (
                 SELECT * FROM siblings
                 UNION
                 SELECT * FROM replacements
             )
             SELECT
                 c.session_row_id,
                 c.session_public_id,
                 c.relation_kind,
                 c.panel_group_public_id,
                 c.evidence_revision_public_id,
                 c.reclassification_revision_public_id,
                 c.first_available_at,
                 CASE WHEN p2.session_row_id IS NOT NULL THEN 1 ELSE 0 END AS already_pinned
             FROM combined c
             LEFT JOIN pinned p2 ON p2.session_row_id = c.session_row_id
             WHERE (?2 = 1 OR p2.session_row_id IS NULL)
               AND (?3 IS NULL OR c.first_available_at < ?3
                    OR (c.first_available_at = ?3 AND c.session_public_id > ?4))
             ORDER BY c.first_available_at DESC, c.session_public_id ASC
             LIMIT ?5",
    )
    .bind(head_rev_id)
    .bind(i64::from(include_pinned))
    .bind(cursor_first_available_at)
    .bind(cursor_session_public_id)
    .bind(page_size)
    .fetch_all(&mut *conn)
    .await?;

    Ok(rows
        .into_iter()
        .map(
            |(
                session_row_id,
                session_public_id,
                relation_kind,
                panel_group_public_id,
                evidence_revision_public_id,
                reclassification_revision_public_id,
                first_available_at,
                already_pinned_flag,
            )| {
                RelatedSessionRow {
                    session_row_id,
                    session_public_id,
                    relation_kind,
                    panel_group_public_id,
                    evidence_revision_public_id,
                    reclassification_revision_public_id,
                    first_available_at,
                    already_pinned: already_pinned_flag != 0,
                }
            },
        )
        .collect())
}
