// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// Guard held across assertions on borrowed data throughout this file.
#![allow(clippy::significant_drop_tightening)]

use super::*;
use audit::bus::EventBus;
use contracts_core::error_code::ErrorCode;
use contracts_core::targets::{
    AliasKind as ContractAliasKind, TargetAliasAddRequest, TargetAliasRemoveRequest,
    TargetDisplayAliasClearRequest, TargetDisplayAliasSetRequest, TargetGetRequest,
    TargetNoteGetRequest, TargetNoteUpdateRequest, TargetProjectsListRequest,
    TargetSessionsListRequest,
};
use persistence_core::Database;
use targeting_resolver::cache::upsert_resolved;
use targeting_resolver::ObjectType;
use targeting_resolver::{AliasKind as CacheKind, ResolvedAlias, ResolvedIdentity, TargetSource};
use uuid::Uuid;

use super::note::MAX_NOTE_BYTES;

async fn setup() -> cache_test_lock::LockedDb {
    cache_test_lock::locked_db().await
}

fn make_bus(db: &Database) -> EventBus {
    EventBus::with_pool(db.pool().clone())
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
            ResolvedAlias::new("M 31", CacheKind::Designation),
            ResolvedAlias::new("NGC 224", CacheKind::Designation),
            ResolvedAlias::new("Andromeda Galaxy", CacheKind::CommonName),
        ],
        source: TargetSource::Resolved,
    }
}

async fn seed_m31(db: &Database) -> Uuid {
    let (id, _) = upsert_resolved(db.pool(), &m31()).await.unwrap();
    id
}

// ── target.get ────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_returns_detail_with_aliases() {
    let db = setup().await;
    let id = seed_m31(&db).await;
    let req = TargetGetRequest { target_id: id.to_string() };
    let detail = get(db.pool(), &req).await.unwrap();
    assert_eq!(detail.primary_designation, "M 31");
    assert_eq!(detail.object_type, "galaxy");
    assert_eq!(detail.source, "resolved");
    assert!(detail.simbad_oid.is_some());
    assert!(detail.display_alias.is_none());
    assert_eq!(detail.effective_label, "M 31");
    assert_eq!(detail.aliases.len(), 3);
}

#[tokio::test]
async fn get_not_found_returns_error() {
    let db = setup().await;
    let req = TargetGetRequest { target_id: Uuid::new_v4().to_string() };
    let err = get(db.pool(), &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::TargetNotFound);
}

#[tokio::test]
async fn get_invalid_id_returns_error() {
    let db = setup().await;
    let req = TargetGetRequest { target_id: "not-a-uuid".to_owned() };
    let err = get(db.pool(), &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::TargetInvalidId);
}

// ── target.list ───────────────────────────────────────────────────────────

#[tokio::test]
async fn list_returns_all_targets() {
    let db = setup().await;
    seed_m31(&db).await;
    let items = list(db.pool(), None).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].primary_designation, "M 31");
    assert_eq!(items[0].object_type, "galaxy");
    assert_eq!(items[0].effective_label, "M 31");
}

#[tokio::test]
async fn list_empty_when_no_targets() {
    let db = setup().await;
    let items = list(db.pool(), None).await.unwrap();
    assert!(items.is_empty());
}

/// `target.list` must carry `ra_deg` and `dec_deg` sourced from
/// `canonical_target` — these are always non-null per the schema constraint.
#[tokio::test]
async fn list_item_carries_ra_dec() {
    let db = setup().await;
    seed_m31(&db).await;
    let items = list(db.pool(), None).await.unwrap();
    assert_eq!(items.len(), 1);
    // M31 fixture values from m31() above (ra=10.684708, dec=41.26875).
    assert!((items[0].ra_deg - 10.684_708).abs() < 1e-6, "ra_deg mismatch: {}", items[0].ra_deg);
    assert!((items[0].dec_deg - 41.268_75).abs() < 1e-6, "dec_deg mismatch: {}", items[0].dec_deg);
}

/// Spec 052 P1 (D8): `upsert_resolved` now enriches `constellation` from
/// `(ra_deg, dec_deg)` on every write, so it is populated even though the
/// `m31()` fixture below doesn't set it explicitly. `magnitude` stays
/// `None` because the fixture carries no `v_mag` (never fabricated).
#[tokio::test]
async fn list_item_constellation_and_magnitude_none_when_not_stored() {
    let db = setup().await;
    seed_m31(&db).await;
    let items = list(db.pool(), None).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].constellation.as_deref(),
        Some("And"),
        "constellation is now derived from coordinates at every upsert (spec 052 P1 D8)"
    );
    assert!(
        items[0].magnitude.is_none(),
        "magnitude must be None when the source has no v_mag, got {:?}",
        items[0].magnitude
    );
}

/// `target.list` must carry all alias display forms so clients can perform
/// alias search (e.g. "Andromeda" → M31) without a separate round-trip.
#[tokio::test]
async fn list_item_carries_aliases() {
    let db = setup().await;
    seed_m31(&db).await;
    let items = list(db.pool(), None).await.unwrap();
    assert_eq!(items.len(), 1);
    // M31 fixture aliases: "M 31", "NGC 224", "Andromeda Galaxy".
    assert_eq!(
        items[0].aliases.len(),
        3,
        "expected 3 aliases in list item, got {:?}",
        items[0].aliases
    );
    assert!(
        items[0].aliases.contains(&"Andromeda Galaxy".to_owned()),
        "alias search pivot 'Andromeda Galaxy' missing from list item"
    );
    assert!(
        items[0].aliases.contains(&"NGC 224".to_owned()),
        "alias 'NGC 224' missing from list item"
    );
}

/// `aliases` must be empty (not absent/null) for targets with no alias rows.
#[tokio::test]
async fn list_item_aliases_empty_when_no_aliases_stored() {
    let db = setup().await;
    // Insert a bare canonical_target with no aliases.
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO canonical_target
         (id, simbad_oid, primary_designation, object_type, ra_deg, dec_deg, source, resolved_at)
         VALUES (?, NULL, 'Bare Target', 'galaxy', 0.0, 0.0, 'seed', '2026-01-01T00:00:00Z')",
    )
    .bind(&id)
    .execute(db.pool())
    .await
    .expect("direct insert failed");

    let items = list(db.pool(), None).await.unwrap();
    assert_eq!(items.len(), 1);
    assert!(items[0].aliases.is_empty(), "aliases must be empty vec, got {:?}", items[0].aliases);
}

/// When `constellation` and `magnitude` are written directly to the DB they
/// are returned by `target.list`.
#[tokio::test]
async fn list_item_returns_stored_constellation_and_magnitude() {
    let db = setup().await;
    let id = seed_m31(&db).await;

    // Write constellation + magnitude directly (simulates a future resolver
    // or seed that populates these fields).
    sqlx::query("UPDATE canonical_target SET constellation = ?, magnitude = ? WHERE id = ?")
        .bind("And")
        .bind(3.44_f64)
        .bind(id.to_string())
        .execute(db.pool())
        .await
        .expect("direct constellation/magnitude update failed");

    let items = list(db.pool(), None).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].constellation.as_deref(), Some("And"), "constellation mismatch");
    assert!(
        items[0].magnitude.is_some_and(|m| (m - 3.44).abs() < 1e-6),
        "magnitude mismatch: {:?}",
        items[0].magnitude
    );
}

// ── target.alias.add ──────────────────────────────────────────────────────

#[tokio::test]
async fn alias_add_inserts_user_alias() {
    let db = setup().await;
    let id = seed_m31(&db).await;
    let req = TargetAliasAddRequest { target_id: id.to_string(), alias: "Andromeda".to_owned() };
    let result = alias_add(db.pool(), &req).await.unwrap();
    assert_eq!(result.alias.alias, "Andromeda");
    assert_eq!(result.alias.kind, ContractAliasKind::User);
    assert!(!result.alias.id.is_empty());
}

#[tokio::test]
async fn alias_add_idempotent_for_duplicate() {
    let db = setup().await;
    let id = seed_m31(&db).await;
    let req = TargetAliasAddRequest { target_id: id.to_string(), alias: "Andromeda".to_owned() };
    let r1 = alias_add(db.pool(), &req).await.unwrap();
    let r2 = alias_add(db.pool(), &req).await.unwrap();
    assert_eq!(r1.alias.id, r2.alias.id);
}

#[tokio::test]
async fn alias_add_blank_returns_error() {
    let db = setup().await;
    let id = seed_m31(&db).await;
    let req = TargetAliasAddRequest { target_id: id.to_string(), alias: "   ".to_owned() };
    let err = alias_add(db.pool(), &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::AliasBlank);
}

#[tokio::test]
async fn alias_add_target_not_found() {
    let db = setup().await;
    let req =
        TargetAliasAddRequest { target_id: Uuid::new_v4().to_string(), alias: "Foo".to_owned() };
    let err = alias_add(db.pool(), &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::TargetNotFound);
}

/// FR-008 (#751): adding an alias to target B that's already an alias of
/// target A must return `alias.duplicate`, never silently succeed —
/// `UNIQUE(target_id, normalized)` alone does not stop cross-target reuse.
#[tokio::test]
async fn alias_add_cross_target_duplicate_returns_error() {
    let db = setup().await;
    let m31_id = seed_m31(&db).await;
    let m42 = ResolvedIdentity {
        simbad_oid: Some(1_575_545),
        primary_designation: "M 42".to_owned(),
        common_name: Some("Orion Nebula".to_owned()),
        object_type: ObjectType::Other,
        ra_deg: 83.822_08,
        dec_deg: -5.391_11,
        v_mag: None,
        aliases: vec![ResolvedAlias::new("M 42", CacheKind::Designation)],
        source: TargetSource::Resolved,
    };
    let (m42_id, _) = upsert_resolved(db.pool(), &m42).await.unwrap();

    // "NGC 224" is already an M31 designation alias (seeded via m31()).
    let req = TargetAliasAddRequest { target_id: m42_id.to_string(), alias: "NGC 224".to_owned() };
    let err = alias_add(db.pool(), &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::AliasDuplicate);

    // Same-target re-add of an existing alias stays idempotent (FR-008
    // first clause) — only the cross-target case above errors.
    let same_target_req =
        TargetAliasAddRequest { target_id: m31_id.to_string(), alias: "NGC 224".to_owned() };
    assert!(alias_add(db.pool(), &same_target_req).await.is_ok());
}

// ── target.alias.remove ───────────────────────────────────────────────────

#[tokio::test]
async fn alias_remove_deletes_user_alias() {
    let db = setup().await;
    let id = seed_m31(&db).await;
    let add_req = TargetAliasAddRequest { target_id: id.to_string(), alias: "Andy".to_owned() };
    let added = alias_add(db.pool(), &add_req).await.unwrap();

    let rem_req = TargetAliasRemoveRequest { target_id: id.to_string(), alias_id: added.alias.id };
    let result = alias_remove(db.pool(), &rem_req).await.unwrap();
    assert!(result.removed);
}

#[tokio::test]
async fn alias_remove_simbad_alias_is_not_removable() {
    let db = setup().await;
    let id = seed_m31(&db).await;
    // Get a designation alias id.
    let aliases = load_alias_dtos(db.pool(), &id.to_string()).await.unwrap();
    let designation = aliases.iter().find(|a| a.kind == ContractAliasKind::Designation).unwrap();

    let rem_req =
        TargetAliasRemoveRequest { target_id: id.to_string(), alias_id: designation.id.clone() };
    let err = alias_remove(db.pool(), &rem_req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::AliasNotRemovable);
}

#[tokio::test]
async fn alias_remove_not_found_returns_error() {
    let db = setup().await;
    let id = seed_m31(&db).await;
    let rem_req = TargetAliasRemoveRequest {
        target_id: id.to_string(),
        alias_id: Uuid::new_v4().to_string(),
    };
    let err = alias_remove(db.pool(), &rem_req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::AliasNotFound);
}

// ── target.display_alias.set / clear ─────────────────────────────────────

#[tokio::test]
async fn display_alias_set_updates_effective_label() {
    let db = setup().await;
    let id = seed_m31(&db).await;
    let req = TargetDisplayAliasSetRequest {
        target_id: id.to_string(),
        display_alias: "Andromeda".to_owned(),
    };
    let detail = display_alias_set(db.pool(), &req).await.unwrap();
    assert_eq!(detail.display_alias.as_deref(), Some("Andromeda"));
    assert_eq!(detail.effective_label, "Andromeda");
}

#[tokio::test]
async fn display_alias_clear_restores_primary_designation() {
    let db = setup().await;
    let id = seed_m31(&db).await;
    // Set first.
    let set_req = TargetDisplayAliasSetRequest {
        target_id: id.to_string(),
        display_alias: "Andromeda".to_owned(),
    };
    display_alias_set(db.pool(), &set_req).await.unwrap();

    // Then clear.
    let clear_req = TargetDisplayAliasClearRequest { target_id: id.to_string() };
    let detail = display_alias_clear(db.pool(), &clear_req).await.unwrap();
    assert!(detail.display_alias.is_none());
    assert_eq!(detail.effective_label, "M 31");
}

#[tokio::test]
async fn display_alias_set_not_found_returns_error() {
    let db = setup().await;
    let req = TargetDisplayAliasSetRequest {
        target_id: Uuid::new_v4().to_string(),
        display_alias: "X".to_owned(),
    };
    let err = display_alias_set(db.pool(), &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::TargetNotFound);
}

#[tokio::test]
async fn upsert_does_not_overwrite_display_alias() {
    let db = setup().await;
    let id = seed_m31(&db).await;

    // Set a display alias.
    let set_req = TargetDisplayAliasSetRequest {
        target_id: id.to_string(),
        display_alias: "My Andromeda".to_owned(),
    };
    display_alias_set(db.pool(), &set_req).await.unwrap();

    // Re-resolve (simulate SIMBAD refresh) — must NOT clear display_alias.
    upsert_resolved(db.pool(), &m31()).await.unwrap();

    let get_req = TargetGetRequest { target_id: id.to_string() };
    let detail = get(db.pool(), &get_req).await.unwrap();
    assert_eq!(
        detail.display_alias.as_deref(),
        Some("My Andromeda"),
        "FR-012: display_alias must survive re-resolution"
    );
}

// ── target.sessions.list (spec 023 US2) ──────────────────────────────────

async fn insert_session_linked_to(db: &Database, session_id: &str, target_id: Uuid) {
    sqlx::query(
        r#"INSERT INTO acquisition_session
           (id, session_key, frame_ids, created_at, canonical_target_id)
           VALUES (?, '{"target":"M 31","filter":"Ha","binning":"1","gain":"0","date":"2026-01-01"}',
                   '[1,2,3]', '2026-01-01T00:00:00Z', ?)"#,
    )
    .bind(session_id)
    .bind(target_id.to_string())
    .execute(db.pool())
    .await
    .expect("insert session failed");
}

async fn insert_project_linked_to(db: &Database, project_id: &str, target_id: Uuid) {
    // Path must be unique per project (UNIQUE constraint on projects.path).
    sqlx::query(
        "INSERT INTO projects
         (id, name, tool, lifecycle, path, canonical_target_id, channel_drift, created_at, updated_at)
         VALUES (?, 'Test Project', 'PixInsight', 'ready', ?, ?, 0,
                 '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .bind(project_id)
    .bind(format!("projects/{project_id}"))
    .bind(target_id.to_string())
    .execute(db.pool())
    .await
    .expect("insert project failed");
}

#[tokio::test]
async fn sessions_list_returns_linked_sessions() {
    let db = setup().await;
    let id = seed_m31(&db).await;
    insert_session_linked_to(&db, "s-001", id).await;
    let req = TargetSessionsListRequest { target_id: id.to_string() };
    let items = sessions_list(db.pool(), &req).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, "s-001");
    assert_eq!(items[0].frame_count, 3);
}

/// FR-003/US2-AC1 (#739): the filter segment of a real (pipe-delimited,
/// `sessions::SessionKey::new`-shaped) `session_key` must surface on the DTO.
#[tokio::test]
async fn sessions_list_extracts_filter_from_session_key() {
    let db = setup().await;
    let id = seed_m31(&db).await;
    sqlx::query(
        r"INSERT INTO acquisition_session
           (id, session_key, frame_ids, created_at, canonical_target_id)
           VALUES ('s-002', 'M 31|Ha|1x1|100|2026-01-01', '[1]',
                   '2026-01-01T00:00:00Z', ?)",
    )
    .bind(id.to_string())
    .execute(db.pool())
    .await
    .expect("insert session failed");

    let req = TargetSessionsListRequest { target_id: id.to_string() };
    let items = sessions_list(db.pool(), &req).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].filter, "Ha");
}

/// A malformed/legacy `session_key` (no pipes) yields `""`, never a panic.
#[tokio::test]
async fn sessions_list_filter_empty_for_malformed_session_key() {
    let db = setup().await;
    let id = seed_m31(&db).await;
    insert_session_linked_to(&db, "s-003", id).await;
    let req = TargetSessionsListRequest { target_id: id.to_string() };
    let items = sessions_list(db.pool(), &req).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].filter, "");
}

#[tokio::test]
async fn sessions_list_empty_for_target_with_no_sessions() {
    let db = setup().await;
    let id = seed_m31(&db).await;
    let req = TargetSessionsListRequest { target_id: id.to_string() };
    let items = sessions_list(db.pool(), &req).await.unwrap();
    assert!(items.is_empty());
}

#[tokio::test]
async fn sessions_list_not_found_for_unknown_target() {
    let db = setup().await;
    let req = TargetSessionsListRequest { target_id: Uuid::new_v4().to_string() };
    let err = sessions_list(db.pool(), &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::TargetNotFound);
}

#[tokio::test]
async fn sessions_list_invalid_id_returns_error() {
    let db = setup().await;
    let req = TargetSessionsListRequest { target_id: "not-a-uuid".to_owned() };
    let err = sessions_list(db.pool(), &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::TargetInvalidId);
}

// ── target.projects.list (spec 023 US3) ──────────────────────────────────

#[tokio::test]
async fn projects_list_returns_linked_projects() {
    let db = setup().await;
    let id = seed_m31(&db).await;
    insert_project_linked_to(&db, "p-001", id).await;
    let req = TargetProjectsListRequest { target_id: id.to_string() };
    let items = projects_list(db.pool(), &req).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, "p-001");
    assert_eq!(items[0].lifecycle, "ready");
}

#[tokio::test]
async fn projects_list_empty_for_target_with_no_projects() {
    let db = setup().await;
    let id = seed_m31(&db).await;
    let req = TargetProjectsListRequest { target_id: id.to_string() };
    let items = projects_list(db.pool(), &req).await.unwrap();
    assert!(items.is_empty());
}

#[tokio::test]
async fn projects_list_not_found_for_unknown_target() {
    let db = setup().await;
    let req = TargetProjectsListRequest { target_id: Uuid::new_v4().to_string() };
    let err = projects_list(db.pool(), &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::TargetNotFound);
}

// ── target.note.get / target.note.update (spec 023 US4) ──────────────────

#[tokio::test]
async fn note_get_returns_none_when_no_note_set() {
    let db = setup().await;
    let id = seed_m31(&db).await;
    let req = TargetNoteGetRequest { target_id: id.to_string() };
    let result = note_get(db.pool(), &req).await.unwrap();
    assert!(result.notes.is_none());
}

#[tokio::test]
async fn note_update_and_get_roundtrip() {
    let db = setup().await;
    let id = seed_m31(&db).await;
    let bus = make_bus(&db);

    let upd_req =
        TargetNoteUpdateRequest { target_id: id.to_string(), notes: "Great seeing.".to_owned() };
    let upd_result = note_update(db.pool(), &bus, &upd_req).await.unwrap();
    assert_eq!(upd_result.notes.as_deref(), Some("Great seeing."));

    let get_req = TargetNoteGetRequest { target_id: id.to_string() };
    let get_result = note_get(db.pool(), &get_req).await.unwrap();
    assert_eq!(get_result.notes.as_deref(), Some("Great seeing."));
}

#[tokio::test]
async fn note_update_whitespace_clears_note() {
    let db = setup().await;
    let id = seed_m31(&db).await;
    let bus = make_bus(&db);

    // Set a note first.
    note_update(
        db.pool(),
        &bus,
        &TargetNoteUpdateRequest { target_id: id.to_string(), notes: "Initial.".to_owned() },
    )
    .await
    .unwrap();

    // Whitespace-only update should clear.
    let clear_result = note_update(
        db.pool(),
        &bus,
        &TargetNoteUpdateRequest { target_id: id.to_string(), notes: "   ".to_owned() },
    )
    .await
    .unwrap();
    assert!(clear_result.notes.is_none(), "whitespace should clear notes");

    let get_result =
        note_get(db.pool(), &TargetNoteGetRequest { target_id: id.to_string() }).await.unwrap();
    assert!(get_result.notes.is_none());
}

#[tokio::test]
async fn note_get_not_found_returns_error() {
    let db = setup().await;
    let req = TargetNoteGetRequest { target_id: Uuid::new_v4().to_string() };
    let err = note_get(db.pool(), &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::TargetNotFound);
}

#[tokio::test]
async fn note_update_not_found_returns_error() {
    let db = setup().await;
    let bus = make_bus(&db);
    let req =
        TargetNoteUpdateRequest { target_id: Uuid::new_v4().to_string(), notes: "x".to_owned() };
    let err = note_update(db.pool(), &bus, &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::TargetNotFound);
}

// ── SC-003 / US4-AS3: note survives alias mutations ───────────────────────

/// A stored note must be unchanged after a user alias is added and then
/// removed on the same target (SC-003 / US4-AS3).
#[tokio::test]
async fn note_survives_alias_add_and_remove() {
    let db = setup().await;
    let id = seed_m31(&db).await;
    let bus = make_bus(&db);

    // Step 1: store a note.
    let upd = TargetNoteUpdateRequest {
        target_id: id.to_string(),
        notes: "Best viewed in autumn.".to_owned(),
    };
    note_update(db.pool(), &bus, &upd).await.unwrap();

    // Step 2: add a user alias.
    let add_req =
        TargetAliasAddRequest { target_id: id.to_string(), alias: "Andromeda".to_owned() };
    let added = alias_add(db.pool(), &add_req).await.unwrap();

    // Step 3: remove that alias.
    let rem_req = TargetAliasRemoveRequest { target_id: id.to_string(), alias_id: added.alias.id };
    alias_remove(db.pool(), &rem_req).await.unwrap();

    // Step 4: note must still be intact.
    let get_req = TargetNoteGetRequest { target_id: id.to_string() };
    let result = note_get(db.pool(), &get_req).await.unwrap();
    assert_eq!(
        result.notes.as_deref(),
        Some("Best viewed in autumn."),
        "SC-003: note must survive alias add + remove"
    );
}

// ── FR-004: 16 KB note size cap ───────────────────────────────────────────

/// A note exceeding 16 384 UTF-8 bytes (after trimming) must be rejected
/// with error code `note.content_too_large` (FR-004).
#[tokio::test]
async fn note_update_over_16kb_rejected() {
    let db = setup().await;
    let id = seed_m31(&db).await;
    let bus = make_bus(&db);

    // 16 385 ASCII bytes (one byte over the 16 384-byte cap).
    let oversized = "x".repeat(MAX_NOTE_BYTES + 1);
    let req = TargetNoteUpdateRequest { target_id: id.to_string(), notes: oversized };
    let err = note_update(db.pool(), &bus, &req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::NoteContentTooLarge, "FR-004: notes >16 KB must be rejected");
}

/// A note exactly at the 16 384-byte limit must be accepted.
#[tokio::test]
async fn note_update_exactly_16kb_accepted() {
    let db = setup().await;
    let id = seed_m31(&db).await;
    let bus = make_bus(&db);

    let at_limit = "x".repeat(MAX_NOTE_BYTES);
    let req = TargetNoteUpdateRequest { target_id: id.to_string(), notes: at_limit.clone() };
    let result = note_update(db.pool(), &bus, &req).await.unwrap();
    assert_eq!(result.notes.as_deref(), Some(at_limit.as_str()));
}
