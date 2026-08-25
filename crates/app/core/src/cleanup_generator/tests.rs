// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// See the matching allow in `protection::tests`: `setup()`'s test-lock
// guard is deliberately held across every `.await` in each test body, and
// that's safe under `#[tokio::test]`'s current-thread runtime default.
#![allow(clippy::await_holding_lock)]

use super::*;
use crate::protection;
use audit::bus::EventBus;
use contracts_core::cleanup::{
    CleanupAction, CleanupPolicy, CleanupPolicyEntry, RawFrameCleanupGenerateRequest,
    RawFrameCleanupScanRequest,
};
use contracts_core::protection::{
    PlanProtectionCheckRequest, ProtectionLevel, SourceProtectionSetRequest,
};
use persistence_core::Database;
use persistence_plans::repositories::artifacts::{insert_artifact_if_absent, InsertArtifact};
use persistence_plans::repositories::plans as plans_repo;
use persistence_plans::repositories::projects::{insert_project, InsertProject};
use sqlx::SqlitePool;

async fn setup() -> (Database, EventBus, std::sync::MutexGuard<'static, ()>) {
    // `scan_with_policy` unconditionally calls `protection::load_global_protection`,
    // which read-throughs the process-global `protection_defaults` cache
    // (a single unkeyed slot shared by every in-memory DB in this test
    // binary — see `protection::PROTECTION_DEFAULTS_TEST_LOCK`). Serialize
    // against `protection.rs`'s tests (e.g. `t041_...`, which mutates the
    // default to `"unprotected"`) so a value-sensitive assertion here
    // (e.g. `generate_protected_final_gates_approval` expecting the
    // default `"protected"`) can't race it.
    let lock = crate::protection::PROTECTION_DEFAULTS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // FIX: a prior test's `set_global_protection_default` call (e.g.
    // `category_protection_elevates_real_frame_type_not_fixed_pseudo_category`
    // below) can leave the process-global cache holding a non-"protected"
    // snapshot from ITS OWN (now-dropped) in-memory DB. The lock alone only
    // serializes execution order — it does not reset the cache, so this
    // fresh DB's real `protection_defaults` row (seeded "protected" by
    // migration 0035) was previously shadowed by the stale cached value,
    // intermittently zeroing `protected_item_count` here. Mirrors
    // `protection.rs`'s own `setup()`, which already does this.
    app_core_cache::invalidate_protection_defaults();
    let db = Database::in_memory().await.expect("in-memory DB");
    db.migrate().await.expect("migrations");
    let bus = EventBus::with_pool(db.pool().clone());
    (db, bus, lock)
}

async fn seed_project(db: &Database, id: &str) {
    insert_project(
        db.pool(),
        &InsertProject {
            id,
            name: "M31 LRGB",
            tool: "PixInsight",
            lifecycle: "processing",
            path: "projects/M31_LRGB",
            notes: None,
            canonical_target_id: None,
            is_mosaic: false,
        },
    )
    .await
    .unwrap();
}

#[allow(clippy::too_many_arguments)]
async fn seed_artifact(
    db: &Database,
    id: &str,
    project_id: &str,
    path: &str,
    kind: &str,
    size: i64,
) {
    insert_artifact_if_absent(
        db.pool(),
        InsertArtifact {
            id,
            project_id,
            tool_launch_id: None,
            path,
            kind,
            tool: "PixInsight",
            detected_at: "2026-07-01T00:00:00Z",
            state: "present",
            classification_confidence: 0.9,
            classification_source: "rule",
            size_bytes: size,
            file_mtime: "2026-07-01T00:00:00Z",
            content_hash: None,
        },
    )
    .await
    .unwrap();
}

// ── Classification unit tests ─────────────────────────────────────────

#[test]
fn classify_maps_every_known_kind() {
    assert_eq!(DataType::from_artifact_kind("intermediate"), DataType::Intermediate);
    assert_eq!(DataType::from_artifact_kind("master"), DataType::Master);
    assert_eq!(DataType::from_artifact_kind("final"), DataType::Final);
}

#[test]
fn classify_unknown_kind_is_unclassified() {
    assert_eq!(DataType::from_artifact_kind("something_else"), DataType::Unclassified);
    assert_eq!(DataType::from_artifact_kind(""), DataType::Unclassified);
}

#[test]
fn protection_category_maps_protected_types() {
    assert_eq!(DataType::Master.protection_category(), "masters");
    assert_eq!(DataType::Final.protection_category(), "finals");
    assert_eq!(DataType::Intermediate.protection_category(), "intermediate");
}

// ── Policy persistence round-trip (D13) ───────────────────────────────

#[tokio::test]
async fn policy_defaults_when_unset() {
    let (db, _bus, _lock) = setup().await;
    let policy = get_policy(db.pool()).await.unwrap();
    assert_eq!(policy.entries.len(), 3);
    assert!(!policy.auto_on_completion);
    assert!(policy.entries.iter().all(|e| e.action == CleanupAction::Keep));
}

#[tokio::test]
async fn policy_round_trip_persists() {
    let (db, _bus, _lock) = setup().await;
    let updated = CleanupPolicy {
        entries: vec![
            CleanupPolicyEntry {
                data_type: "intermediate".to_owned(),
                action: CleanupAction::Delete,
            },
            CleanupPolicyEntry { data_type: "master".to_owned(), action: CleanupAction::Keep },
            CleanupPolicyEntry { data_type: "final".to_owned(), action: CleanupAction::Keep },
        ],
        auto_on_completion: true,
    };
    set_policy(db.pool(), &updated).await.unwrap();

    let reloaded = get_policy(db.pool()).await.unwrap();
    assert!(reloaded.auto_on_completion);
    let intermediate = reloaded
        .entries
        .iter()
        .find(|e| e.data_type == "intermediate")
        .expect("intermediate entry");
    assert_eq!(intermediate.action, CleanupAction::Delete);
}

// ── Scan preview ──────────────────────────────────────────────────────

#[tokio::test]
async fn scan_empty_project_has_no_candidates() {
    let (db, _bus, _lock) = setup().await;
    seed_project(&db, "p-empty").await;
    let result = scan(db.pool(), "p-empty").await.unwrap();
    assert!(result.candidates.is_empty());
    assert_eq!(result.total_reclaimable_bytes, 0);
}

#[tokio::test]
async fn scan_default_policy_proposes_nothing() {
    // Default policy is all-Keep: even with artifacts present, no candidates.
    let (db, _bus, _lock) = setup().await;
    seed_project(&db, "p1").await;
    seed_artifact(&db, "a1", "p1", "calibrated/light_001.xisf", "intermediate", 1000).await;
    let result = scan(db.pool(), "p1").await.unwrap();
    assert!(result.candidates.is_empty(), "all-Keep policy must propose nothing");
}

/// Seed one archivable intermediate pair (1000 + 2000) and one archivable
/// final (4000) on project `p1`, so a caller can pick which of the three the
/// protection settings should hold back. Distinct sizes: 0, 3000 and 7000 are
/// mutually distinguishable totals, so a double subtraction or an off-by-one
/// cannot land on the right answer by accident.
async fn seed_archivable_mix(db: &Database) {
    seed_project(db, "p1").await;
    seed_artifact(db, "a1", "p1", "calibrated/light_001.xisf", "intermediate", 1000).await;
    seed_artifact(db, "a2", "p1", "calibrated/light_002.xisf", "intermediate", 2000).await;
    seed_artifact(db, "a3", "p1", "final/M31.xisf", "final", 4000).await;

    set_policy(
        db.pool(),
        &CleanupPolicy {
            entries: vec![
                CleanupPolicyEntry {
                    data_type: "intermediate".to_owned(),
                    action: CleanupAction::Archive,
                },
                CleanupPolicyEntry {
                    data_type: "final".to_owned(),
                    action: CleanupAction::Archive,
                },
            ],
            auto_on_completion: false,
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn scan_actioned_type_becomes_candidate() {
    let (db, _bus, _lock) = setup().await;
    seed_project(&db, "p1").await;
    seed_artifact(&db, "a1", "p1", "calibrated/light_001.xisf", "intermediate", 1000).await;
    seed_artifact(&db, "a2", "p1", "calibrated/light_002.xisf", "intermediate", 2000).await;
    seed_artifact(&db, "a3", "p1", "final/M31.xisf", "final", 5000).await;

    // Opt intermediates in for archiving; keep finals.
    set_policy(
        db.pool(),
        &CleanupPolicy {
            entries: vec![
                CleanupPolicyEntry {
                    data_type: "intermediate".to_owned(),
                    action: CleanupAction::Archive,
                },
                CleanupPolicyEntry { data_type: "final".to_owned(), action: CleanupAction::Keep },
            ],
            auto_on_completion: false,
        },
    )
    .await
    .unwrap();

    let result = scan(db.pool(), "p1").await.unwrap();
    assert_eq!(result.candidates.len(), 2, "only the two intermediates are candidates");
    assert!(result.candidates.iter().all(|c| c.data_type == "intermediate"));
}

// ── Reclaimable total nets out protected bytes (astro-plan-i5qxc) ──────
//
// `total_reclaimable_bytes` is what applying the plan would free under the
// CURRENT protection settings. Three cases, because a fix that always
// subtracts everything satisfies the all-protected case alone.

#[tokio::test]
async fn scan_total_is_zero_when_every_candidate_is_protected() {
    let (db, _bus, _lock) = setup().await;
    seed_archivable_mix(&db).await;

    // Shipped defaults: defaultProtection="protected", so every candidate
    // resolves protected regardless of category membership.
    let result = scan(db.pool(), "p1").await.unwrap();

    assert_eq!(result.candidates.len(), 3, "all three stay listed so protection is visible");
    assert_eq!(
        result.total_reclaimable_bytes, 0,
        "apply refuses every protected item, so nothing is reclaimable"
    );
}

#[tokio::test]
async fn scan_total_is_full_size_when_no_candidate_is_protected() {
    let (db, bus, _lock) = setup().await;
    seed_archivable_mix(&db).await;

    protection::set_global_protection_default(
        db.pool(),
        &bus,
        "global",
        "defaultProtection",
        serde_json::json!("unprotected"),
    )
    .await
    .unwrap();
    protection::set_global_protection_default(
        db.pool(),
        &bus,
        "global",
        "protectedCategories",
        serde_json::json!([]),
    )
    .await
    .unwrap();

    let result = scan(db.pool(), "p1").await.unwrap();

    assert_eq!(result.candidates.len(), 3);
    assert_eq!(result.total_reclaimable_bytes, 7000, "1000 + 2000 + 4000, nothing held back");
}

#[tokio::test]
async fn scan_total_counts_only_the_unprotected_subset() {
    let (db, bus, _lock) = setup().await;
    seed_archivable_mix(&db).await;

    // Only the `final` artifact's category is protected; both intermediates
    // stay unprotected.
    protection::set_global_protection_default(
        db.pool(),
        &bus,
        "global",
        "defaultProtection",
        serde_json::json!("unprotected"),
    )
    .await
    .unwrap();
    protection::set_global_protection_default(
        db.pool(),
        &bus,
        "global",
        "protectedCategories",
        serde_json::json!(["finals"]),
    )
    .await
    .unwrap();

    let result = scan(db.pool(), "p1").await.unwrap();

    assert_eq!(result.candidates.len(), 3, "the protected final is still a listed candidate");
    assert_eq!(
        result.total_reclaimable_bytes, 3000,
        "1000 + 2000 from the intermediates; the 4000 final is held back"
    );
}

#[tokio::test]
async fn scan_excludes_unclassified() {
    let (db, _bus, _lock) = setup().await;
    seed_project(&db, "p1").await;
    // A present artifact whose kind is not intermediate/master/final cannot
    // exist under the CHECK constraint; simulate the exclusion path by
    // asserting a would-be-actioned unknown data_type in the policy has no
    // effect. Here we just confirm masters aren't cleaned under an
    // intermediate-only policy.
    seed_artifact(&db, "a1", "p1", "masters/master_dark.xisf", "master", 4000).await;
    set_policy(
        db.pool(),
        &CleanupPolicy {
            entries: vec![CleanupPolicyEntry {
                data_type: "intermediate".to_owned(),
                action: CleanupAction::Delete,
            }],
            auto_on_completion: false,
        },
    )
    .await
    .unwrap();
    let result = scan(db.pool(), "p1").await.unwrap();
    assert!(result.candidates.is_empty(), "master not covered by policy → Keep default");
}

// ── Generate vs scan separation ───────────────────────────────────────

#[tokio::test]
async fn scan_creates_no_plan() {
    let (db, _bus, _lock) = setup().await;
    seed_project(&db, "p1").await;
    seed_artifact(&db, "a1", "p1", "calibrated/light_001.xisf", "intermediate", 1000).await;
    set_policy(
        db.pool(),
        &CleanupPolicy {
            entries: vec![CleanupPolicyEntry {
                data_type: "intermediate".to_owned(),
                action: CleanupAction::Archive,
            }],
            auto_on_completion: false,
        },
    )
    .await
    .unwrap();

    scan(db.pool(), "p1").await.unwrap();

    // No plan rows exist after a pure scan.
    let plans = plans_repo::list_plans(db.pool(), &[], &[], None, 100).await.unwrap();
    assert!(plans.is_empty(), "scan must not create a plan (D11 step 1)");
}

#[tokio::test]
async fn generate_creates_plan_with_items() {
    let (db, _bus, _lock) = setup().await;
    seed_project(&db, "p1").await;
    seed_artifact(&db, "a1", "p1", "calibrated/light_001.xisf", "intermediate", 1000).await;
    seed_artifact(&db, "a2", "p1", "calibrated/light_002.xisf", "intermediate", 2000).await;
    set_policy(
        db.pool(),
        &CleanupPolicy {
            entries: vec![CleanupPolicyEntry {
                data_type: "intermediate".to_owned(),
                action: CleanupAction::Archive,
            }],
            auto_on_completion: false,
        },
    )
    .await
    .unwrap();

    let resp = generate(db.pool(), "p1", Some("My cleanup"), Some("archive")).await.unwrap();
    assert_eq!(resp.item_count, 2);
    // Safe default: the global default protection level is "protected", so
    // with no per-source override every item resolves protected and gates
    // approval (constitution II).
    assert_eq!(resp.protected_item_count, 2);

    let items = plans_repo::list_plan_items(db.pool(), &resp.plan_id).await.unwrap();
    assert_eq!(items.len(), 2);

    // FR-012 / D17: the plan carries a real destination byte requirement —
    // the sum of the two archive-action item sizes (1000 + 2000).
    let plan = plans_repo::get_plan(db.pool(), &resp.plan_id, false).await.unwrap();
    assert_eq!(plan.total_bytes_required, 3000);
}

#[tokio::test]
async fn generate_delete_items_require_no_destination_bytes() {
    // Delete-action items are removed, not archived, so they contribute
    // zero to the plan's destination byte requirement (D17).
    let (db, _bus, _lock) = setup().await;
    seed_project(&db, "p1").await;
    seed_artifact(&db, "a1", "p1", "calibrated/light_001.xisf", "intermediate", 1000).await;
    set_policy(
        db.pool(),
        &CleanupPolicy {
            entries: vec![CleanupPolicyEntry {
                data_type: "intermediate".to_owned(),
                action: CleanupAction::Delete,
            }],
            auto_on_completion: false,
        },
    )
    .await
    .unwrap();

    let resp = generate(db.pool(), "p1", None, None).await.unwrap();
    assert_eq!(resp.item_count, 1);

    let plan = plans_repo::get_plan(db.pool(), &resp.plan_id, false).await.unwrap();
    assert_eq!(plan.total_bytes_required, 0, "delete items need no destination space");
}

/// Issue #806 regression: a plan generated with `destructive_destination:
/// "trash"` must not fabricate an `.astro-plan-archive/...` item path —
/// that convention only applies to the "archive folder" destination.
#[tokio::test]
async fn generate_with_trash_destination_does_not_fabricate_archive_path() {
    let (db, _bus, _lock) = setup().await;
    seed_project(&db, "p1").await;
    seed_artifact(&db, "a1", "p1", "calibrated/light_001.xisf", "intermediate", 1000).await;
    set_policy(
        db.pool(),
        &CleanupPolicy {
            entries: vec![CleanupPolicyEntry {
                data_type: "intermediate".to_owned(),
                action: CleanupAction::Archive,
            }],
            auto_on_completion: false,
        },
    )
    .await
    .unwrap();

    let resp = generate(db.pool(), "p1", None, Some("trash")).await.unwrap();
    let items = plans_repo::list_plan_items(db.pool(), &resp.plan_id).await.unwrap();
    assert_eq!(items.len(), 1);
    assert!(
        !items[0].to_relative_path.contains(".astro-plan-archive"),
        "trash destination must not show an app-managed archive subfolder path, got {:?}",
        items[0].to_relative_path
    );
}

// ── Protected-category exclusion end-to-end ───────────────────────────

#[tokio::test]
async fn generate_protected_final_gates_approval() {
    let (db, _bus, _lock) = setup().await;
    seed_project(&db, "p1").await;
    // A final output — default protected categories include "finals".
    seed_artifact(&db, "a1", "p1", "final/M31.xisf", "final", 9000).await;
    // Opt finals in for archiving so it becomes a candidate.
    set_policy(
        db.pool(),
        &CleanupPolicy {
            entries: vec![CleanupPolicyEntry {
                data_type: "final".to_owned(),
                action: CleanupAction::Archive,
            }],
            auto_on_completion: false,
        },
    )
    .await
    .unwrap();

    let resp = generate(db.pool(), "p1", None, None).await.unwrap();
    assert_eq!(resp.item_count, 1);
    assert_eq!(resp.protected_item_count, 1, "final maps to protected category 'finals'");

    // The protection gate fires on the generated plan.
    let check = protection::plan_protection_check(
        db.pool(),
        &PlanProtectionCheckRequest { plan_id: resp.plan_id.clone() },
    )
    .await
    .unwrap();
    assert!(check.has_protected_items);
    assert_eq!(check.protected_items.len(), 1);
    assert_eq!(check.protected_items[0].source_id.as_deref(), Some("p1"));
}

#[tokio::test]
async fn generate_respects_per_source_protection_override() {
    // The global default level is "protected", so without an override an
    // intermediate item would gate approval. A per-source "unprotected"
    // override must flow through and downgrade it — proving the override
    // path is live, not inert.
    let (db, bus, _lock) = setup().await;
    seed_project(&db, "p1").await;
    seed_artifact(&db, "a1", "p1", "calibrated/light_001.xisf", "intermediate", 1000).await;
    set_policy(
        db.pool(),
        &CleanupPolicy {
            entries: vec![CleanupPolicyEntry {
                data_type: "intermediate".to_owned(),
                action: CleanupAction::Archive,
            }],
            auto_on_completion: false,
        },
    )
    .await
    .unwrap();

    protection::set_source_protection(
        db.pool(),
        &bus,
        &SourceProtectionSetRequest {
            source_id: "p1".to_owned(),
            level: ProtectionLevel::Unprotected,
            block_permanent_delete: Some(false),
            categories: None,
        },
    )
    .await
    .unwrap();

    let resp = generate(db.pool(), "p1", None, None).await.unwrap();
    assert_eq!(
        resp.protected_item_count, 0,
        "per-source unprotected override downgrades the item from the protected default"
    );
}

// ── Raw sub-frame candidates (spec 048 US3 T027-T031) ──────────────────

use contracts_core::cleanup::RawFrameCleanupScope;

async fn insert_root(pool: &SqlitePool, id: &str, path: &str) {
    sqlx::query(
        "INSERT INTO library_root (id, label, current_path, kind, state, created_at)
         VALUES (?, ?, ?, 'local', 'active', datetime('now'))",
    )
    .bind(id)
    .bind(id)
    .bind(path)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_acquisition_session(pool: &SqlitePool, id: &str, frame_ids: &[&str]) {
    let frame_ids_json = serde_json::to_string(frame_ids).unwrap();
    sqlx::query(
        "INSERT INTO acquisition_session (id, session_key, frame_ids, created_at)
         VALUES (?, '{}', ?, datetime('now'))",
    )
    .bind(id)
    .bind(frame_ids_json)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn scan_raw_frames_by_session_returns_present_candidates_with_reclaimable_bytes() {
    use app_core_targets::frame_writer::upsert_frame_record;

    let (db, bus, _lock) = setup().await;
    // Light frames resolve protected under the shipped defaults, which would
    // net the total to zero; this case is about summing real sizes and
    // excluding `missing`, so relax protection out of the way.
    protection::set_global_protection_default(
        db.pool(),
        &bus,
        "global",
        "defaultProtection",
        serde_json::json!("unprotected"),
    )
    .await
    .unwrap();
    protection::set_global_protection_default(
        db.pool(),
        &bus,
        "global",
        "protectedCategories",
        serde_json::json!([]),
    )
    .await
    .unwrap();
    insert_root(db.pool(), "root-1", "/tmp/lib").await;
    let f1 = upsert_frame_record(db.pool(), "root-1", "l1.fits", 1000, "t0", "classified")
        .await
        .unwrap();
    let f2 = upsert_frame_record(db.pool(), "root-1", "l2.fits", 2000, "t0", "classified")
        .await
        .unwrap();
    let missing =
        upsert_frame_record(db.pool(), "root-1", "l3.fits", 500, "t0", "missing").await.unwrap();
    insert_acquisition_session(db.pool(), "sess-1", &[&f1, &f2, &missing]).await;

    let req = RawFrameCleanupScanRequest {
        scope: RawFrameCleanupScope { session_id: Some("sess-1".to_owned()), root_id: None },
        kinds: None,
    };
    let resp = scan_raw_frames(db.pool(), &req).await.unwrap();

    assert_eq!(resp.candidates.len(), 2, "missing frame excluded (FR-022)");
    assert_eq!(resp.total_reclaimable_bytes, 3000);
    assert!(resp.candidates.iter().all(|c| c.session_id.as_deref() == Some("sess-1")));
    assert!(resp.candidates.iter().all(|c| (c.confidence - 1.0).abs() < f64::EPSILON));
}

/// A frame whose RESOLVED protection level is protected is still listed (so
/// the user can see what is held back) but contributes no bytes — distinct
/// from `scan_raw_frames_excludes_protected_state`, which is about the
/// inventory presence state and drops the candidate entirely.
#[tokio::test]
async fn scan_raw_frames_total_excludes_resolved_protected_bytes() {
    use app_core_targets::frame_writer::upsert_frame_record;

    let (db, _bus, _lock) = setup().await;
    insert_root(db.pool(), "root-1", "/tmp/lib").await;
    let f1 = upsert_frame_record(db.pool(), "root-1", "l1.fits", 1000, "t0", "classified")
        .await
        .unwrap();
    let f2 = upsert_frame_record(db.pool(), "root-1", "l2.fits", 2000, "t0", "classified")
        .await
        .unwrap();
    insert_acquisition_session(db.pool(), "sess-1", &[&f1, &f2]).await;

    // Shipped defaults protect the "lights" category.
    let resp = scan_raw_frames(
        db.pool(),
        &RawFrameCleanupScanRequest {
            scope: RawFrameCleanupScope { session_id: Some("sess-1".to_owned()), root_id: None },
            kinds: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(resp.candidates.len(), 2);
    assert!(
        resp.candidates.iter().all(|c| c.protection == "protected"),
        "fixture precondition: both frames must resolve protected"
    );
    assert_eq!(resp.total_reclaimable_bytes, 0, "protected frames are refused at apply");
}

#[tokio::test]
async fn scan_raw_frames_excludes_protected_state() {
    use app_core_targets::frame_writer::upsert_frame_record;

    let (db, _bus, _lock) = setup().await;
    insert_root(db.pool(), "root-1", "/tmp/lib").await;
    let protected =
        upsert_frame_record(db.pool(), "root-1", "p1.fits", 1000, "t0", "protected").await.unwrap();
    insert_acquisition_session(db.pool(), "sess-1", &[&protected]).await;

    let req = RawFrameCleanupScanRequest {
        scope: RawFrameCleanupScope { session_id: Some("sess-1".to_owned()), root_id: None },
        kinds: None,
    };
    let resp = scan_raw_frames(db.pool(), &req).await.unwrap();

    assert!(resp.candidates.is_empty(), "protected frames must be excluded (FR-021)");
    assert_eq!(resp.total_reclaimable_bytes, 0);
}

/// Issue #731 regression: when the global default level is NOT
/// `protected` (so nothing is protected by default), a raw frame's real
/// type must still be elevated to `protected` when its category name is a
/// member of `protected_categories` — the fixed `"raw_frames"`
/// pseudo-category could never match `["lights","masters","finals"]`.
#[tokio::test]
async fn category_protection_elevates_real_frame_type_not_fixed_pseudo_category() {
    use app_core_targets::frame_writer::upsert_frame_record;

    let (db, bus, _lock) = setup().await;
    insert_root(db.pool(), "root-1", "/tmp/lib").await;
    let light = upsert_frame_record(db.pool(), "root-1", "l1.fits", 1000, "t0", "classified")
        .await
        .unwrap();
    insert_acquisition_session(db.pool(), "sess-1", &[&light]).await;

    // Global default level is "unprotected" (not protected); only category
    // membership should elevate a light frame.
    protection::set_global_protection_default(
        db.pool(),
        &bus,
        "global",
        "defaultProtection",
        serde_json::json!("unprotected"),
    )
    .await
    .unwrap();

    let resp = scan_raw_frames(
        db.pool(),
        &RawFrameCleanupScanRequest {
            scope: RawFrameCleanupScope { session_id: Some("sess-1".to_owned()), root_id: None },
            kinds: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(resp.candidates.len(), 1);
    assert_eq!(
        resp.candidates[0].protection, "protected",
        "a Light frame's real category ('lights') must be elevated by the default \
         protected_categories list, not silently bypassed by a fixed pseudo-category"
    );
}

#[tokio::test]
async fn scan_raw_frames_filters_by_kind() {
    use app_core_targets::frame_writer::upsert_frame_record;

    let (db, _bus, _lock) = setup().await;
    insert_root(db.pool(), "root-1", "/tmp/lib").await;
    let light = upsert_frame_record(db.pool(), "root-1", "l1.fits", 1000, "t0", "classified")
        .await
        .unwrap();
    insert_acquisition_session(db.pool(), "sess-1", &[&light]).await;

    let req = RawFrameCleanupScanRequest {
        scope: RawFrameCleanupScope { session_id: Some("sess-1".to_owned()), root_id: None },
        kinds: Some(vec![contracts_core::inventory_frame::RawFrameType::Dark]),
    };
    let resp = scan_raw_frames(db.pool(), &req).await.unwrap();

    assert!(resp.candidates.is_empty(), "session is Light-kind; Dark filter excludes it");
}

/// Issue #563 regression: a per-root "Unprotected" override (the only
/// override surface the UI ships — the Data Sources card) previously
/// never reached session-attributed frames, because enforcement resolved
/// protection under the session id, found no row, and inherited the
/// global `protected` default — making the override cosmetic exactly
/// where it gates cleanup.
#[tokio::test]
async fn root_override_applies_to_session_attributed_raw_frames() {
    use app_core_targets::frame_writer::upsert_frame_record;

    let (db, bus, _lock) = setup().await;
    insert_root(db.pool(), "root-1", "/tmp/lib").await;
    let f1 = upsert_frame_record(db.pool(), "root-1", "l1.fits", 1000, "t0", "classified")
        .await
        .unwrap();
    insert_acquisition_session(db.pool(), "sess-1", &[&f1]).await;

    protection::set_source_protection(
        db.pool(),
        &bus,
        &SourceProtectionSetRequest {
            source_id: "root-1".to_owned(),
            level: ProtectionLevel::Unprotected,
            block_permanent_delete: Some(false),
            categories: None,
        },
    )
    .await
    .unwrap();

    let scan = scan_raw_frames(
        db.pool(),
        &RawFrameCleanupScanRequest {
            scope: RawFrameCleanupScope { session_id: Some("sess-1".to_owned()), root_id: None },
            kinds: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(scan.candidates.len(), 1);
    assert_eq!(
        scan.candidates[0].protection, "unprotected",
        "per-root override must reach session-attributed frames"
    );

    let plan = generate_raw_frame_plan(
        db.pool(),
        &RawFrameCleanupGenerateRequest {
            selected_frame_ids: vec![f1],
            title: None,
            destructive_destination: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        plan.protected_item_count, 0,
        "generated plan must honour the root override, not the global default"
    );
}

/// Pins the #563 precedence chain: a per-session override row (future
/// surface) still wins over the frame's root override when both exist.
#[tokio::test]
async fn session_override_takes_precedence_over_root_for_raw_frames() {
    use app_core_targets::frame_writer::upsert_frame_record;

    let (db, bus, _lock) = setup().await;
    insert_root(db.pool(), "root-1", "/tmp/lib").await;
    let f1 = upsert_frame_record(db.pool(), "root-1", "l1.fits", 1000, "t0", "classified")
        .await
        .unwrap();
    insert_acquisition_session(db.pool(), "sess-1", &[&f1]).await;

    for (source_id, level) in
        [("root-1", ProtectionLevel::Unprotected), ("sess-1", ProtectionLevel::Protected)]
    {
        protection::set_source_protection(
            db.pool(),
            &bus,
            &SourceProtectionSetRequest {
                source_id: source_id.to_owned(),
                level,
                block_permanent_delete: None,
                categories: None,
            },
        )
        .await
        .unwrap();
    }

    let scan = scan_raw_frames(
        db.pool(),
        &RawFrameCleanupScanRequest {
            scope: RawFrameCleanupScope { session_id: Some("sess-1".to_owned()), root_id: None },
            kinds: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        scan.candidates[0].protection, "protected",
        "an existing session override row outranks the root override"
    );
}

#[tokio::test]
async fn generate_raw_frame_plan_creates_reviewable_plan_with_no_filesystem_mutation() {
    use app_core_targets::frame_writer::upsert_frame_record;

    let (db, _bus, _lock) = setup().await;
    insert_root(db.pool(), "root-1", "/tmp/lib").await;
    let f1 = upsert_frame_record(db.pool(), "root-1", "l1.fits", 1000, "t0", "classified")
        .await
        .unwrap();
    let f2 = upsert_frame_record(db.pool(), "root-1", "l2.fits", 2000, "t0", "classified")
        .await
        .unwrap();
    insert_acquisition_session(db.pool(), "sess-1", &[&f1, &f2]).await;

    let resp = generate_raw_frame_plan(
        db.pool(),
        &RawFrameCleanupGenerateRequest {
            selected_frame_ids: vec![f1.clone(), f2.clone()],
            title: Some("Reclaim M31 lights".to_owned()),
            destructive_destination: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(resp.item_count, 2);

    let items = plans_repo::list_plan_items(db.pool(), &resp.plan_id).await.unwrap();
    assert_eq!(items.len(), 2);
    // The candidate files under /tmp/lib were never created or touched —
    // generation is read-only (FR-019); no assertion can "prove a
    // negative" better than: this test never calls any fs write/remove
    // API, and the plan row alone was written (no PlanApply here).
}

#[tokio::test]
async fn generate_raw_frame_plan_skips_missing_selected_frames() {
    use app_core_targets::frame_writer::upsert_frame_record;

    let (db, _bus, _lock) = setup().await;
    insert_root(db.pool(), "root-1", "/tmp/lib").await;
    let present = upsert_frame_record(db.pool(), "root-1", "l1.fits", 1000, "t0", "classified")
        .await
        .unwrap();
    let missing =
        upsert_frame_record(db.pool(), "root-1", "l2.fits", 2000, "t0", "missing").await.unwrap();

    let resp = generate_raw_frame_plan(
        db.pool(),
        &RawFrameCleanupGenerateRequest {
            selected_frame_ids: vec![present, missing],
            title: None,
            destructive_destination: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(resp.item_count, 1, "the missing frame must not become a plan item (FR-022)");
}
