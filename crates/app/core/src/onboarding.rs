// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Onboarding use cases and item registry (spec 056).
//!
//! Successor to the removed spec-010 first-project coach state machine.
//! Three layers share this one backend:
//! - L1 orientation walk: `orientation_complete` sets
//!   `onboarding_flags.orientation_done_at`.
//! - L2 checklists: `get_state`/`set_item_state`/`section_set` over
//!   `ITEM_REGISTRY`.
//! - L3 find-it spotlight: reads `anchor` from the registry; no backend use
//!   case (UI-only).
//!
//! ## Item registry
//!
//! `ITEM_REGISTRY` is the single source of truth for the five FR-006
//! workflow pages (`inbox`, `sessions`, `calibration`, `targets`,
//! `projects`), 2-4 items each. AUTOMATIC items (`completion_topic` +
//! `seed_query` both set) are ticked by the live bus subscriber
//! (`apps/desktop/src-tauri/src/commands/onboarding.rs`, T005) via
//! `tick_from_event` and re-derived by seed/restore below; the rest are
//! manual (FR-017). `completion_topic`/`seed_query` pairs are drawn from the
//! research R4 verified auto-tick inventory only — no new backend events are
//! minted (decision record #2). Labels/tooltips/reasons are Paraglide keys
//! derived from `item_id` (research R9), not stored here.
//!
//! ## Seed/restore derivation (FR-014/PQ-001)
//!
//! `target_state_for` computes "what should this item's state be given
//! real recorded data" for every item — the one derivation routine backing
//! both:
//! - the lazy first-activation seed (`ensure_seeded`, called from
//!   `get_state`): only inserts rows that don't exist yet, via
//!   `persistence_lifecycle::repositories::onboarding::insert_if_missing` — never
//!   re-touches an existing row, so a mere read can never silently
//!   auto-tick an item (that would bypass the bus subscriber, breaking
//!   FR-021's backend-authoritative-via-subscriber-only invariant);
//! - the explicit `restore` command: re-derives every AUTOMATIC item's row
//!   via `upsert_if_unsettled`, which is a no-op for settled
//!   (`manually_checked`/`dismissed`) rows — user progress is never
//!   discarded;
//! - the once-per-startup `reconcile_missed_events` pass (PQ-005): the same
//!   re-derivation narrowed to rows that are `unchecked` from a non-`user`
//!   source, so a missed live event self-heals without ever undoing a
//!   deliberate un-check.
//!
//! ## FR-031 settle path
//!
//! `settle_and_maybe_hide` runs after every write that could be a
//! *settling* transition — a live-event tick (`tick_from_event`) or a
//! manual check-off/dismiss (`set_item_state`) — never seed/restore, which
//! are derivations, not transitions. When it leaves zero `unchecked` rows
//! across the whole registry, `onboarding_flags.section_hidden_at` is set.
//! `restore` always clears that flag regardless of the resulting item
//! states (FR-014) — a still-complete section stays visible until a NEW
//! settling transition or an explicit `section_set { hidden: true }`.

use std::collections::{HashMap, HashSet};

use contracts_core::onboarding::{
    OnboardingFlagsDto, OnboardingItemDto, OnboardingItemSetStateRequest,
    OnboardingItemSetStateResponse, OnboardingItemState, OnboardingManualState,
    OnboardingOrientationCompleteRequest, OnboardingOrientationCompleteResponse, OnboardingPage,
    OnboardingPageProgressDto, OnboardingPrerequisiteDto, OnboardingProgressDto,
    OnboardingRestoreResponse, OnboardingSectionSetRequest, OnboardingSectionSetResponse,
    OnboardingStateDto, OnboardingStateGetResponse, OnboardingStateSource,
};
use contracts_core::{error_code::ErrorCode, ContractError, ErrorSeverity};
use domain_core::ids::Timestamp;
use persistence_lifecycle::repositories::onboarding as repo;
use persistence_lifecycle::repositories::onboarding::{OnboardingFlagsRow, UpsertOutcome};
use sqlx::SqlitePool;

// ── Item registry ─────────────────────────────────────────────────────────────

/// How an AUTOMATIC item's seed/restore target state is derived from real
/// recorded data (research R4's verified auto-tick inventory). Each variant
/// dispatches to a dedicated read-only query in
/// `persistence_lifecycle::repositories::onboarding` — raw SQL stays in that crate
/// per the db-boundary rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeedMilestone {
    InboxConfirmed,
    PlanApplied,
    ProjectCreated,
    ToolLaunchSpawned,
    TargetResolved,
}

impl SeedMilestone {
    async fn met(self, pool: &SqlitePool) -> Result<bool, OnboardingError> {
        match self {
            Self::InboxConfirmed => repo::has_confirmed_inbox_item(pool).await,
            Self::PlanApplied => repo::has_applied_plan(pool).await,
            Self::ProjectCreated => repo::has_project(pool).await,
            Self::ToolLaunchSpawned => repo::has_spawned_tool_launch(pool).await,
            Self::TargetResolved => repo::has_resolved_target(pool).await,
        }
        .map_err(db_err)
    }
}

/// An item's prerequisite: an upstream registry item id plus the page to
/// jump to when it's unmet (FR-010). Satisfaction is computed live from the
/// upstream item's own `seed_query` — never cached (data-model.md
/// "Derived (never stored)").
pub struct PrerequisiteDef {
    pub upstream_item_id: &'static str,
    pub jump_page: OnboardingPage,
}

/// Static item definition. Labels/tooltips/reasons are Paraglide keys
/// derived from `item_id`, not stored here (research R9).
pub struct ItemDef {
    pub item_id: &'static str,
    pub page: OnboardingPage,
    /// Bus topic that auto-ticks this item; `None` = manual item (FR-017).
    pub completion_topic: Option<&'static str>,
    /// Payload predicate gating the tick (e.g. `tool.launch` requires
    /// `outcome == "spawned"`). Only meaningful when `completion_topic` is
    /// set; evaluated by the T005 subscriber, not here.
    pub payload_filter: Option<fn(&serde_json::Value) -> bool>,
    /// How seed/restore derives "already met" for this item. `None` for
    /// manual items — they always seed to `unchecked`.
    pub seed_query: Option<SeedMilestone>,
    pub prerequisite: Option<PrerequisiteDef>,
    /// `data-guide-anchor` value for the L3 spotlight target.
    pub anchor: &'static str,
}

impl ItemDef {
    #[must_use]
    pub fn is_automatic(&self) -> bool {
        self.completion_topic.is_some()
    }
}

/// The item registry (v1) — 11 items across the five FR-006 pages (2-4 each:
/// inbox 2, sessions 2, calibration 2, targets 2, projects 3). Five are
/// AUTOMATIC (tied to research R4's verified topics); the rest are manual.
/// Missing-milestone follow-ups (calibration master registration, site save
/// — research R4) stay manual per the v1 "no new backend events" constraint.
pub const ITEM_REGISTRY: &[ItemDef] = &[
    ItemDef {
        item_id: "inbox.confirm_first",
        page: OnboardingPage::Inbox,
        completion_topic: Some(audit::TOPIC_INVENTORY_CONFIRMED),
        payload_filter: None,
        seed_query: Some(SeedMilestone::InboxConfirmed),
        prerequisite: None,
        anchor: "inbox.confirm-row",
    },
    ItemDef {
        item_id: "inbox.apply_first_plan",
        page: OnboardingPage::Inbox,
        completion_topic: Some("plan.applying.completed"),
        payload_filter: None,
        seed_query: Some(SeedMilestone::PlanApplied),
        prerequisite: Some(PrerequisiteDef {
            upstream_item_id: "inbox.confirm_first",
            jump_page: OnboardingPage::Inbox,
        }),
        anchor: "inbox.apply-plan-cta",
    },
    ItemDef {
        item_id: "sessions.review_first",
        page: OnboardingPage::Sessions,
        completion_topic: None,
        payload_filter: None,
        seed_query: None,
        prerequisite: Some(PrerequisiteDef {
            upstream_item_id: "inbox.confirm_first",
            jump_page: OnboardingPage::Inbox,
        }),
        anchor: "sessions.review-row",
    },
    ItemDef {
        item_id: "sessions.add_note",
        page: OnboardingPage::Sessions,
        completion_topic: None,
        payload_filter: None,
        seed_query: None,
        prerequisite: Some(PrerequisiteDef {
            upstream_item_id: "inbox.confirm_first",
            jump_page: OnboardingPage::Inbox,
        }),
        anchor: "sessions.add-note-cta",
    },
    ItemDef {
        item_id: "calibration.match_master",
        page: OnboardingPage::Calibration,
        completion_topic: None,
        payload_filter: None,
        seed_query: None,
        prerequisite: Some(PrerequisiteDef {
            upstream_item_id: "inbox.confirm_first",
            jump_page: OnboardingPage::Inbox,
        }),
        anchor: "calibration.match-cta",
    },
    ItemDef {
        item_id: "calibration.review_masters",
        page: OnboardingPage::Calibration,
        completion_topic: None,
        payload_filter: None,
        seed_query: None,
        prerequisite: None,
        anchor: "calibration.review-row",
    },
    ItemDef {
        item_id: "targets.resolve_first",
        page: OnboardingPage::Targets,
        completion_topic: Some("target.resolved"),
        payload_filter: None,
        seed_query: Some(SeedMilestone::TargetResolved),
        prerequisite: None,
        anchor: "targets.resolve-cta",
    },
    ItemDef {
        item_id: "targets.add_favourite",
        page: OnboardingPage::Targets,
        completion_topic: None,
        payload_filter: None,
        seed_query: None,
        prerequisite: Some(PrerequisiteDef {
            upstream_item_id: "targets.resolve_first",
            jump_page: OnboardingPage::Targets,
        }),
        anchor: "targets.favourite-cta",
    },
    ItemDef {
        item_id: "projects.create_first",
        page: OnboardingPage::Projects,
        completion_topic: Some("project.created"),
        payload_filter: None,
        seed_query: Some(SeedMilestone::ProjectCreated),
        prerequisite: Some(PrerequisiteDef {
            upstream_item_id: "inbox.confirm_first",
            jump_page: OnboardingPage::Inbox,
        }),
        anchor: "projects.create-cta",
    },
    ItemDef {
        item_id: "projects.launch_tool",
        page: OnboardingPage::Projects,
        completion_topic: Some("tool.launch"),
        payload_filter: Some(|payload| {
            payload.get("outcome").and_then(|v| v.as_str()) == Some("spawned")
        }),
        seed_query: Some(SeedMilestone::ToolLaunchSpawned),
        prerequisite: Some(PrerequisiteDef {
            upstream_item_id: "projects.create_first",
            jump_page: OnboardingPage::Projects,
        }),
        anchor: "project.open-in-tool",
    },
    ItemDef {
        item_id: "projects.review_artifacts",
        page: OnboardingPage::Projects,
        completion_topic: None,
        payload_filter: None,
        seed_query: None,
        prerequisite: Some(PrerequisiteDef {
            upstream_item_id: "projects.launch_tool",
            jump_page: OnboardingPage::Projects,
        }),
        anchor: "projects.artifacts-row",
    },
];

/// Look up a registry item by id.
#[must_use]
pub fn find_item(item_id: &str) -> Option<&'static ItemDef> {
    ITEM_REGISTRY.iter().find(|i| i.item_id == item_id)
}

fn page_order(page: OnboardingPage) -> u8 {
    match page {
        OnboardingPage::Inbox => 0,
        OnboardingPage::Sessions => 1,
        OnboardingPage::Calibration => 2,
        OnboardingPage::Targets => 3,
        OnboardingPage::Projects => 4,
    }
}

// ── Error type ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OnboardingError {
    /// `item.set_state` referenced an `item_id` not in `ITEM_REGISTRY`.
    #[error("unknown onboarding item id: {0}")]
    UnknownItem(String),
    /// An automatic item was manually marked complete. Automatic completion
    /// is reserved for recorded domain milestones.
    #[error("automatic onboarding item cannot be manually completed: {0}")]
    AutomaticItemManualCompletion(String),
    /// `section.set` was called with neither field set.
    #[error("onboarding.section.set request must set hidden or sidebarCollapsed")]
    SectionSetEmptyRequest,
    /// `section.set { hidden: false }` — unhiding is exclusively
    /// `onboarding.restore`.
    #[error("onboarding.section.set hidden may only be true; unhide via onboarding.restore")]
    SectionUnhideNotAllowed,
    /// A persistence layer failure.
    #[error("persistence unavailable: {0}")]
    PersistenceUnavailable(String),
}

impl From<OnboardingError> for ContractError {
    fn from(e: OnboardingError) -> Self {
        match e {
            OnboardingError::UnknownItem(id) => Self::new(
                ErrorCode::OnboardingItemUnknown,
                format!("unknown onboarding item id: {id}"),
                ErrorSeverity::Blocking,
                false,
            ),
            OnboardingError::AutomaticItemManualCompletion(_)
            | OnboardingError::SectionSetEmptyRequest
            | OnboardingError::SectionUnhideNotAllowed => Self::new(
                ErrorCode::OnboardingInvalidState,
                e.to_string(),
                ErrorSeverity::Blocking,
                false,
            ),
            OnboardingError::PersistenceUnavailable(msg) => {
                Self::new(ErrorCode::InternalDatabase, msg, ErrorSeverity::Fatal, true)
            }
        }
    }
}

#[allow(clippy::needless_pass_by_value)]
fn db_err(e: persistence_core::DbError) -> OnboardingError {
    OnboardingError::PersistenceUnavailable(e.to_string())
}

// ── Seed/restore derivation (FR-014/PQ-001) ──────────────────────────────────

/// The shared per-item derivation: `unchecked` for manual items, and for
/// AUTOMATIC items whatever [`SeedMilestone::met`] reports right now.
async fn target_state_for(
    pool: &SqlitePool,
    item: &ItemDef,
) -> Result<&'static str, OnboardingError> {
    match item.seed_query {
        None => Ok("unchecked"),
        Some(milestone) => {
            Ok(if milestone.met(pool).await? { "auto_checked" } else { "unchecked" })
        }
    }
}

/// Lazy first-activation seed: insert a row for every registry item that
/// doesn't have one yet, computed via [`target_state_for`]. Never touches an
/// existing row (see module docs — that's what keeps a mere read from
/// silently auto-ticking an item).
async fn ensure_seeded(pool: &SqlitePool) -> Result<(), OnboardingError> {
    let existing = repo::load_state(pool).await.map_err(db_err)?;
    let existing_ids: HashSet<&str> = existing.iter().map(|r| r.item_id.as_str()).collect();

    for item in ITEM_REGISTRY {
        if existing_ids.contains(item.item_id) {
            continue;
        }
        let state = target_state_for(pool, item).await?;
        repo::insert_if_missing(pool, item.item_id, state, "seed").await.map_err(db_err)?;
    }
    Ok(())
}

/// Explicit restore: re-derive every AUTOMATIC item via
/// [`repo::upsert_if_unsettled`] (a no-op against settled
/// `manually_checked`/`dismissed` rows); defensively insert any missing
/// manual-item row (never overwriting an existing one).
async fn derive_all_automatic_items(pool: &SqlitePool) -> Result<(), OnboardingError> {
    for item in ITEM_REGISTRY {
        if !item.is_automatic() {
            repo::insert_if_missing(pool, item.item_id, "unchecked", "seed")
                .await
                .map_err(db_err)?;
            continue;
        }
        let state = target_state_for(pool, item).await?;
        repo::upsert_if_unsettled(pool, item.item_id, state, "seed").await.map_err(db_err)?;
    }
    Ok(())
}

// ── FR-031 settle path ───────────────────────────────────────────────────────

/// Run after a settling write (live-event tick or manual check-off/dismiss).
/// Hides the section once every registry item is settled; a no-op while any
/// item is still `unchecked`.
async fn settle_and_maybe_hide(pool: &SqlitePool) -> Result<(), OnboardingError> {
    if !repo::all_items_settled(pool).await.map_err(db_err)? {
        return Ok(());
    }
    let mut flags = repo::load_flags(pool).await.map_err(db_err)?;
    flags.section_hidden_at = Some(Timestamp::now_iso());
    repo::upsert_flags(pool, &flags).await.map_err(db_err)?;
    Ok(())
}

// ── DTO builders ──────────────────────────────────────────────────────────────

fn parse_state(s: &str) -> OnboardingItemState {
    match s {
        "auto_checked" => OnboardingItemState::AutoChecked,
        "manually_checked" => OnboardingItemState::ManuallyChecked,
        "dismissed" => OnboardingItemState::Dismissed,
        _ => OnboardingItemState::Unchecked,
    }
}

fn parse_source(s: &str) -> OnboardingStateSource {
    match s {
        "event" => OnboardingStateSource::Event,
        "user" => OnboardingStateSource::User,
        _ => OnboardingStateSource::Seed,
    }
}

fn flags_dto(row: &OnboardingFlagsRow) -> OnboardingFlagsDto {
    OnboardingFlagsDto {
        orientation_done: row.orientation_done_at.is_some(),
        section_hidden: row.section_hidden_at.is_some(),
        sidebar_collapsed: row.sidebar_collapsed,
    }
}

async fn prerequisite_dto(
    pool: &SqlitePool,
    def: &PrerequisiteDef,
) -> Result<OnboardingPrerequisiteDto, OnboardingError> {
    let upstream = find_item(def.upstream_item_id)
        .expect("registry prerequisite must reference a real item_id");
    let met = match upstream.seed_query {
        Some(milestone) => milestone.met(pool).await?,
        None => false,
    };
    Ok(OnboardingPrerequisiteDto {
        upstream_item_id: def.upstream_item_id.to_owned(),
        met,
        reason_key: format!("onboarding.prerequisite.{}", def.upstream_item_id),
        jump_page: def.jump_page,
    })
}

async fn item_dto(
    pool: &SqlitePool,
    item: &ItemDef,
    row: &repo::OnboardingStateRow,
) -> Result<OnboardingItemDto, OnboardingError> {
    let prerequisite = match &item.prerequisite {
        Some(def) => Some(prerequisite_dto(pool, def).await?),
        None => None,
    };
    Ok(OnboardingItemDto {
        item_id: item.item_id.to_owned(),
        page: item.page,
        state: parse_state(&row.state),
        at: row.at.clone(),
        source: parse_source(&row.source),
        prerequisite,
        has_auto_tick: item.is_automatic(),
    })
}

async fn build_state_dto(pool: &SqlitePool) -> Result<OnboardingStateDto, OnboardingError> {
    let rows = repo::load_state(pool).await.map_err(db_err)?;
    let by_id: HashMap<&str, &repo::OnboardingStateRow> =
        rows.iter().map(|r| (r.item_id.as_str(), r)).collect();

    let mut items = Vec::with_capacity(ITEM_REGISTRY.len());
    let mut per_page: HashMap<OnboardingPage, (u32, u32)> = HashMap::new();

    for item in ITEM_REGISTRY {
        // Not seeded yet — callers always seed first (get_state/restore);
        // skip defensively rather than fail the whole read.
        let Some(row) = by_id.get(item.item_id) else { continue };

        let completed = matches!(
            parse_state(&row.state),
            OnboardingItemState::AutoChecked | OnboardingItemState::ManuallyChecked
        );
        let entry = per_page.entry(item.page).or_insert((0, 0));
        entry.1 += 1;
        if completed {
            entry.0 += 1;
        }

        items.push(item_dto(pool, item, row).await?);
    }

    let done: u32 = per_page.values().map(|(done, _)| done).sum();
    let total: u32 = per_page.values().map(|(_, total)| total).sum();
    let mut per_page_dto: Vec<OnboardingPageProgressDto> = per_page
        .into_iter()
        .map(|(page, (done, total))| OnboardingPageProgressDto { page, done, total })
        .collect();
    per_page_dto.sort_by_key(|p| page_order(p.page));

    let flags = repo::load_flags(pool).await.map_err(db_err)?;
    Ok(OnboardingStateDto {
        items,
        flags: flags_dto(&flags),
        progress: OnboardingProgressDto { done, total, per_page: per_page_dto },
    })
}

// ── Use cases ─────────────────────────────────────────────────────────────────

/// `onboarding.state.get` — read the full projection for UI hydration.
/// Lazily seeds any registry item without a row yet (first-ever call, or a
/// registry grown since the last seed/restore).
///
/// # Errors
///
/// - `PersistenceUnavailable`: database failure.
pub async fn get_state(pool: &SqlitePool) -> Result<OnboardingStateGetResponse, OnboardingError> {
    ensure_seeded(pool).await?;
    Ok(OnboardingStateGetResponse { state: build_state_dto(pool).await? })
}

/// `onboarding.item.set_state` — manual check-off or dismiss (FR-017). Auto
/// states are structurally unreachable via [`OnboardingManualState`]. An
/// automatic item rejects manual completion so its checked state always
/// corresponds to a recorded domain milestone. A
/// repeat call against an already-settled item is a no-op (manual states are
/// permanent — PQ-002/FR-017); no per-item undo in v1.
///
/// # Errors
///
/// - `UnknownItem`: `item_id` is not in the registry.
/// - `PersistenceUnavailable`: database failure.
///
/// # Panics
///
/// Never in practice: the just-written row is re-read immediately after a
/// successful upsert, which always leaves a row present.
pub async fn set_item_state(
    pool: &SqlitePool,
    req: &OnboardingItemSetStateRequest,
) -> Result<OnboardingItemSetStateResponse, OnboardingError> {
    let item =
        find_item(&req.item_id).ok_or_else(|| OnboardingError::UnknownItem(req.item_id.clone()))?;
    if item.is_automatic() && req.state == OnboardingManualState::ManuallyChecked {
        return Err(OnboardingError::AutomaticItemManualCompletion(item.item_id.to_owned()));
    }
    match req.state {
        OnboardingManualState::Unchecked => {
            repo::force_unchecked(pool, item.item_id).await.map_err(db_err)?;
        }
        OnboardingManualState::ManuallyChecked | OnboardingManualState::Dismissed => {
            let target = match req.state {
                OnboardingManualState::ManuallyChecked => "manually_checked",
                OnboardingManualState::Dismissed => "dismissed",
                OnboardingManualState::Unchecked => unreachable!("handled above"),
            };
            let outcome = repo::upsert_if_unsettled(pool, item.item_id, target, "user")
                .await
                .map_err(db_err)?;
            if outcome == UpsertOutcome::Written {
                settle_and_maybe_hide(pool).await?;
            }
        }
    }

    let row = repo::load_item(pool, item.item_id)
        .await
        .map_err(db_err)?
        .expect("row exists after upsert");
    Ok(OnboardingItemSetStateResponse { item: item_dto(pool, item, &row).await? })
}

/// `onboarding.orientation.complete` — mark the walk finished or skipped;
/// both set done-forever (FR-004). Idempotent — repeat calls return the
/// original timestamp regardless of `outcome` (there is no separate
/// finished-vs-skipped storage; both are terminal "done").
///
/// # Errors
///
/// - `PersistenceUnavailable`: database failure.
///
/// # Panics
///
/// Never in practice: `orientation_done_at` is always `Some` by this point,
/// either just-set above or already set on entry.
pub async fn orientation_complete(
    pool: &SqlitePool,
    _req: &OnboardingOrientationCompleteRequest,
) -> Result<OnboardingOrientationCompleteResponse, OnboardingError> {
    let mut flags = repo::load_flags(pool).await.map_err(db_err)?;
    if flags.orientation_done_at.is_none() {
        flags.orientation_done_at = Some(Timestamp::now_iso());
        repo::upsert_flags(pool, &flags).await.map_err(db_err)?;
    }
    Ok(OnboardingOrientationCompleteResponse {
        orientation_done_at: flags.orientation_done_at.expect("set above or already set"),
    })
}

/// `onboarding.section.set` — explicit remove (FR-013) and collapse
/// persistence (FR-012). `hidden` accepts only `true`; unhiding is
/// exclusively `restore`. The FR-031 completion auto-hide is written only
/// by `settle_and_maybe_hide`, never through this command.
///
/// # Errors
///
/// - `SectionSetEmptyRequest`: neither field was set.
/// - `SectionUnhideNotAllowed`: `hidden: false` was requested.
/// - `PersistenceUnavailable`: database failure.
pub async fn section_set(
    pool: &SqlitePool,
    req: &OnboardingSectionSetRequest,
) -> Result<OnboardingSectionSetResponse, OnboardingError> {
    if req.hidden.is_none() && req.sidebar_collapsed.is_none() {
        return Err(OnboardingError::SectionSetEmptyRequest);
    }
    if req.hidden == Some(false) {
        return Err(OnboardingError::SectionUnhideNotAllowed);
    }

    let mut flags = repo::load_flags(pool).await.map_err(db_err)?;
    if req.hidden == Some(true) {
        flags.section_hidden_at = Some(Timestamp::now_iso());
    }
    if let Some(collapsed) = req.sidebar_collapsed {
        flags.sidebar_collapsed = collapsed;
    }
    repo::upsert_flags(pool, &flags).await.map_err(db_err)?;

    Ok(OnboardingSectionSetResponse { flags: flags_dto(&flags) })
}

/// `onboarding.restore` — the single Settings → Advanced restore/reset
/// (FR-014). Clears the hidden flag (explicit removal or completion
/// auto-hide), then re-derives AUTOMATIC items only; `manually_checked` and
/// `dismissed` rows keep their state. Idempotent.
///
/// # Errors
///
/// - `PersistenceUnavailable`: database failure.
pub async fn restore(pool: &SqlitePool) -> Result<OnboardingRestoreResponse, OnboardingError> {
    derive_all_automatic_items(pool).await?;

    let mut flags = repo::load_flags(pool).await.map_err(db_err)?;
    flags.section_hidden_at = None;
    repo::upsert_flags(pool, &flags).await.map_err(db_err)?;

    Ok(OnboardingRestoreResponse { state: build_state_dto(pool).await? })
}

/// Tick an item from a live bus event. Not itself a Tauri command — the T005
/// subscriber calls this after its own topic/payload-filter/envelope-source
/// checks pass (FR-016/FR-021: only the subscriber may produce
/// `auto_checked`). Exposed here, not inlined in the subscriber, so the
/// settle path stays centralized and both call sites share one code path.
///
/// # Errors
///
/// - `PersistenceUnavailable`: database failure.
pub async fn tick_from_event(pool: &SqlitePool, item_id: &str) -> Result<(), OnboardingError> {
    let outcome =
        repo::upsert_if_unsettled(pool, item_id, "auto_checked", "event").await.map_err(db_err)?;
    if outcome == UpsertOutcome::Written {
        settle_and_maybe_hide(pool).await?;
    }
    Ok(())
}

/// Startup reconciliation (PQ-005): recover ticks whose live bus event was
/// missed — published before the subscriber subscribed, or lost because the
/// process died between the action and the tick write. Without this, such an
/// item stays `unchecked` forever unless the user happens to run the Settings
/// restore, which nothing prompts them to do.
///
/// Re-derives AUTOMATIC items via `target_state_for`, but only for rows that
/// are currently `unchecked` from a non-user source. A deliberate PQ-002
/// un-check must survive startup reconciliation.
///
/// Items with no row yet are skipped, not seeded: `ensure_seeded` derives
/// those correctly on the next `state.get`, so there is nothing to recover.
///
/// This does NOT violate FR-021's backend-authoritative-via-subscriber-only
/// invariant. The invariant that matters is that a mere READ must never
/// auto-tick (why `ensure_seeded` uses `insert_if_missing`). This is a
/// distinct, explicit, once-per-startup write path deriving from real recorded
/// data — not a read side effect. It runs the FR-031 settle path for the same
/// reason: it stands in for the tick that was lost, so it must settle exactly
/// as that tick would have.
///
/// Returns the number of items actually recovered (for startup logging).
///
/// # Errors
///
/// - `PersistenceUnavailable`: database failure.
pub async fn reconcile_missed_events(pool: &SqlitePool) -> Result<usize, OnboardingError> {
    let rows = repo::load_state(pool).await.map_err(db_err)?;
    let mut recovered = 0usize;

    for row in rows.iter().filter(|r| r.state == "unchecked" && r.source != "user") {
        let Some(item) = find_item(&row.item_id) else { continue };
        if !item.is_automatic() || target_state_for(pool, item).await? != "auto_checked" {
            continue;
        }
        let outcome = repo::upsert_if_unsettled(pool, item.item_id, "auto_checked", "event")
            .await
            .map_err(db_err)?;
        if outcome == UpsertOutcome::Written {
            recovered += 1;
        }
    }

    if recovered > 0 {
        settle_and_maybe_hide(pool).await?;
    }
    Ok(recovered)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use contracts_core::onboarding::{OnboardingOrientationOutcome, OnboardingSectionSetRequest};

    async fn setup_pool() -> SqlitePool {
        let db = persistence_core::Database::in_memory().await.expect("in-memory DB");
        db.migrate().await.expect("migrations");
        db.pool().clone()
    }

    // ── Registry shape ───────────────────────────────────────────────────────

    #[test]
    fn registry_has_two_to_four_items_per_page() {
        let mut counts: HashMap<OnboardingPage, usize> = HashMap::new();
        for item in ITEM_REGISTRY {
            *counts.entry(item.page).or_default() += 1;
        }
        for page in [
            OnboardingPage::Inbox,
            OnboardingPage::Sessions,
            OnboardingPage::Calibration,
            OnboardingPage::Targets,
            OnboardingPage::Projects,
        ] {
            let n = counts.get(&page).copied().unwrap_or(0);
            assert!((2..=4).contains(&n), "{page:?} has {n} items, expected 2-4 (FR-006)");
        }
    }

    /// research R4: only these five topics are verified auto-tick sources;
    /// no invented events.
    #[test]
    fn only_verified_topics_appear_as_completion_topics() {
        let verified = [
            audit::TOPIC_INVENTORY_CONFIRMED,
            "plan.applying.completed",
            "project.created",
            "tool.launch",
            "target.resolved",
        ];
        for item in ITEM_REGISTRY {
            if let Some(topic) = item.completion_topic {
                assert!(verified.contains(&topic), "unverified completion_topic: {topic}");
            }
        }
    }

    #[test]
    fn prerequisites_reference_real_registry_items() {
        for item in ITEM_REGISTRY {
            if let Some(def) = &item.prerequisite {
                assert!(
                    find_item(def.upstream_item_id).is_some(),
                    "{}'s prerequisite references unknown item {}",
                    item.item_id,
                    def.upstream_item_id
                );
            }
        }
    }

    #[test]
    fn tool_launch_payload_filter_requires_spawned_outcome() {
        let item = find_item("projects.launch_tool").unwrap();
        let filter = item.payload_filter.expect("projects.launch_tool must gate on outcome");
        assert!(filter(&serde_json::json!({"outcome": "spawned"})));
        assert!(!filter(&serde_json::json!({"outcome": "spawn_failed"})));
        assert!(!filter(&serde_json::json!({})));
    }

    // ── get_state / seeding ──────────────────────────────────────────────────

    #[tokio::test]
    async fn get_state_seeds_every_registry_item_on_first_call() {
        let pool = setup_pool().await;
        let resp = get_state(&pool).await.unwrap();
        assert_eq!(resp.state.items.len(), ITEM_REGISTRY.len());
        assert!(resp.state.items.iter().all(|i| i.state == OnboardingItemState::Unchecked));
        assert_eq!(resp.state.progress.total, u32::try_from(ITEM_REGISTRY.len()).unwrap());
        assert_eq!(resp.state.progress.done, 0);
    }

    #[tokio::test]
    async fn get_state_pre_ticks_automatic_items_already_met_on_upgrade() {
        let pool = setup_pool().await;
        // Simulate a pre-existing library: a project already exists before
        // onboarding ever ran (PQ-001 — first activation uses the same
        // recorded-state derivation as restore).
        sqlx::query(
            "INSERT INTO projects (id, name, tool, lifecycle, path, channel_drift, is_mosaic, created_at, updated_at) \
             VALUES ('proj-1', 'Proj 1', 'PixInsight', 'setup_incomplete', 'proj-1', 0, 0, \
                     '2026-07-18T00:00:00Z', '2026-07-18T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let resp = get_state(&pool).await.unwrap();
        let create_first =
            resp.state.items.iter().find(|i| i.item_id == "projects.create_first").unwrap();
        assert_eq!(create_first.state, OnboardingItemState::AutoChecked);
        assert_eq!(create_first.source, OnboardingStateSource::Seed);
    }

    #[tokio::test]
    async fn get_state_never_re_derives_after_first_seed() {
        let pool = setup_pool().await;
        get_state(&pool).await.unwrap();

        // A milestone becomes true AFTER the first seed, with no live event
        // and no restore — a plain read must not silently auto-tick it
        // (that would bypass the FR-021 subscriber-only invariant).
        sqlx::query(
            "INSERT INTO projects (id, name, tool, lifecycle, path, channel_drift, is_mosaic, created_at, updated_at) \
             VALUES ('proj-1', 'Proj 1', 'PixInsight', 'setup_incomplete', 'proj-1', 0, 0, \
                     '2026-07-18T00:00:00Z', '2026-07-18T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let resp = get_state(&pool).await.unwrap();
        let create_first =
            resp.state.items.iter().find(|i| i.item_id == "projects.create_first").unwrap();
        assert_eq!(create_first.state, OnboardingItemState::Unchecked);
    }

    #[tokio::test]
    async fn get_state_computes_prerequisite_met_live() {
        let pool = setup_pool().await;
        let resp = get_state(&pool).await.unwrap();
        let create_first =
            resp.state.items.iter().find(|i| i.item_id == "projects.create_first").unwrap();
        let prereq = create_first.prerequisite.as_ref().expect("create_first has a prerequisite");
        assert!(!prereq.met);
        assert_eq!(prereq.jump_page, OnboardingPage::Inbox);

        sqlx::query(
            "INSERT INTO inbox_items (id, root_id, relative_path, file_count, discovered_at, last_scanned_at, state, lane) \
             VALUES ('item-1', 'root-1', 'a.fits', 1, '2026-07-18T00:00:00Z', '2026-07-18T00:00:00Z', 'plan_open', 'fits')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let resp = get_state(&pool).await.unwrap();
        let create_first =
            resp.state.items.iter().find(|i| i.item_id == "projects.create_first").unwrap();
        assert!(
            create_first.prerequisite.as_ref().unwrap().met,
            "prerequisite must be computed live, not cached"
        );
    }

    // ── set_item_state ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn set_item_state_manually_checks_item() {
        let pool = setup_pool().await;
        get_state(&pool).await.unwrap();

        let resp = set_item_state(
            &pool,
            &OnboardingItemSetStateRequest {
                item_id: "sessions.review_first".to_owned(),
                state: OnboardingManualState::ManuallyChecked,
            },
        )
        .await
        .unwrap();

        assert_eq!(resp.item.state, OnboardingItemState::ManuallyChecked);
        assert_eq!(resp.item.source, OnboardingStateSource::User);
    }

    #[tokio::test]
    async fn set_item_state_rejects_manual_completion_for_automatic_item() {
        let pool = setup_pool().await;
        get_state(&pool).await.unwrap();

        let err = set_item_state(
            &pool,
            &OnboardingItemSetStateRequest {
                item_id: "inbox.confirm_first".to_owned(),
                state: OnboardingManualState::ManuallyChecked,
            },
        )
        .await
        .unwrap_err();

        assert_eq!(
            err,
            OnboardingError::AutomaticItemManualCompletion("inbox.confirm_first".to_owned())
        );
        let item = get_state(&pool)
            .await
            .unwrap()
            .state
            .items
            .into_iter()
            .find(|item| item.item_id == "inbox.confirm_first")
            .unwrap();
        assert_eq!(item.state, OnboardingItemState::Unchecked);
    }

    #[tokio::test]
    async fn dismissed_item_is_settled_but_not_completed_in_progress() {
        let pool = setup_pool().await;
        get_state(&pool).await.unwrap();

        set_item_state(
            &pool,
            &OnboardingItemSetStateRequest {
                item_id: "sessions.review_first".to_owned(),
                state: OnboardingManualState::Dismissed,
            },
        )
        .await
        .unwrap();

        let state = get_state(&pool).await.unwrap().state;
        assert_eq!(state.progress.done, 0);
        assert_eq!(
            state
                .progress
                .per_page
                .iter()
                .find(|progress| progress.page == OnboardingPage::Sessions)
                .unwrap()
                .done,
            0
        );
    }

    #[tokio::test]
    async fn set_item_state_unknown_item_errors() {
        let pool = setup_pool().await;
        let err = set_item_state(
            &pool,
            &OnboardingItemSetStateRequest {
                item_id: "nonexistent".to_owned(),
                state: OnboardingManualState::Dismissed,
            },
        )
        .await
        .unwrap_err();
        assert_eq!(err, OnboardingError::UnknownItem("nonexistent".to_owned()));
    }

    #[tokio::test]
    async fn set_item_state_is_permanent_no_undo() {
        let pool = setup_pool().await;
        get_state(&pool).await.unwrap();

        set_item_state(
            &pool,
            &OnboardingItemSetStateRequest {
                item_id: "sessions.review_first".to_owned(),
                state: OnboardingManualState::ManuallyChecked,
            },
        )
        .await
        .unwrap();

        // A second manual call (e.g. trying to dismiss instead) is a no-op —
        // PQ-002: no per-item undo in v1.
        let resp = set_item_state(
            &pool,
            &OnboardingItemSetStateRequest {
                item_id: "sessions.review_first".to_owned(),
                state: OnboardingManualState::Dismissed,
            },
        )
        .await
        .unwrap();
        assert_eq!(resp.item.state, OnboardingItemState::ManuallyChecked);
    }

    // ── tick_from_event ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn tick_from_event_auto_checks_item() {
        let pool = setup_pool().await;
        get_state(&pool).await.unwrap();

        tick_from_event(&pool, "inbox.confirm_first").await.unwrap();

        let resp = get_state(&pool).await.unwrap();
        let item = resp.state.items.iter().find(|i| i.item_id == "inbox.confirm_first").unwrap();
        assert_eq!(item.state, OnboardingItemState::AutoChecked);
        assert_eq!(item.source, OnboardingStateSource::Event);
    }

    #[tokio::test]
    async fn tick_from_event_never_downgrades_manually_dismissed_item() {
        let pool = setup_pool().await;
        get_state(&pool).await.unwrap();
        set_item_state(
            &pool,
            &OnboardingItemSetStateRequest {
                item_id: "calibration.review_masters".to_owned(),
                state: OnboardingManualState::Dismissed,
            },
        )
        .await
        .unwrap();

        tick_from_event(&pool, "calibration.review_masters").await.unwrap();

        let resp = get_state(&pool).await.unwrap();
        let item =
            resp.state.items.iter().find(|i| i.item_id == "calibration.review_masters").unwrap();
        assert_eq!(item.state, OnboardingItemState::Dismissed);
    }

    // ── orientation_complete ─────────────────────────────────────────────────

    #[tokio::test]
    async fn orientation_complete_is_idempotent() {
        let pool = setup_pool().await;
        let r1 = orientation_complete(
            &pool,
            &OnboardingOrientationCompleteRequest {
                outcome: OnboardingOrientationOutcome::Finished,
            },
        )
        .await
        .unwrap();
        let r2 = orientation_complete(
            &pool,
            &OnboardingOrientationCompleteRequest {
                outcome: OnboardingOrientationOutcome::Skipped,
            },
        )
        .await
        .unwrap();
        assert_eq!(r1.orientation_done_at, r2.orientation_done_at);
    }

    // ── section_set ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn section_set_hides_permanently() {
        let pool = setup_pool().await;
        let resp = section_set(
            &pool,
            &OnboardingSectionSetRequest { hidden: Some(true), sidebar_collapsed: None },
        )
        .await
        .unwrap();
        assert!(resp.flags.section_hidden);
    }

    #[tokio::test]
    async fn section_set_rejects_unhide() {
        let pool = setup_pool().await;
        let err = section_set(
            &pool,
            &OnboardingSectionSetRequest { hidden: Some(false), sidebar_collapsed: None },
        )
        .await
        .unwrap_err();
        assert_eq!(err, OnboardingError::SectionUnhideNotAllowed);
    }

    #[tokio::test]
    async fn section_set_rejects_empty_request() {
        let pool = setup_pool().await;
        let err = section_set(
            &pool,
            &OnboardingSectionSetRequest { hidden: None, sidebar_collapsed: None },
        )
        .await
        .unwrap_err();
        assert_eq!(err, OnboardingError::SectionSetEmptyRequest);
    }

    #[tokio::test]
    async fn section_set_persists_collapse() {
        let pool = setup_pool().await;
        let resp = section_set(
            &pool,
            &OnboardingSectionSetRequest { hidden: None, sidebar_collapsed: Some(true) },
        )
        .await
        .unwrap();
        assert!(resp.flags.sidebar_collapsed);
        assert!(!resp.flags.section_hidden);
    }

    // ── FR-031 settle path ───────────────────────────────────────────────────

    #[tokio::test]
    async fn settling_last_item_hides_section() {
        let pool = setup_pool().await;
        get_state(&pool).await.unwrap();

        // Dismiss every item; the last one settling must auto-hide the
        // section (FR-031).
        let ids: Vec<&str> = ITEM_REGISTRY.iter().map(|i| i.item_id).collect();
        for (idx, id) in ids.iter().enumerate() {
            let resp = set_item_state(
                &pool,
                &OnboardingItemSetStateRequest {
                    item_id: (*id).to_owned(),
                    state: OnboardingManualState::Dismissed,
                },
            )
            .await
            .unwrap();
            let is_last = idx == ids.len() - 1;
            assert_eq!(resp.item.state, OnboardingItemState::Dismissed);
            let flags = repo::load_flags(&pool).await.unwrap();
            assert_eq!(
                flags.section_hidden_at.is_some(),
                is_last,
                "section must hide exactly on the settling transition of the last item"
            );
        }
    }

    #[tokio::test]
    async fn restore_clears_auto_hide_but_leaves_items_settled() {
        let pool = setup_pool().await;
        get_state(&pool).await.unwrap();
        for item in ITEM_REGISTRY {
            set_item_state(
                &pool,
                &OnboardingItemSetStateRequest {
                    item_id: item.item_id.to_owned(),
                    state: OnboardingManualState::Dismissed,
                },
            )
            .await
            .unwrap();
        }
        assert!(repo::load_flags(&pool).await.unwrap().section_hidden_at.is_some());

        let resp = restore(&pool).await.unwrap();
        assert!(!resp.state.flags.section_hidden, "restore must unhide the section (FR-014)");
        // All-dismissed state survives restore untouched (manual states are
        // never re-derived).
        assert!(resp.state.items.iter().all(|i| i.state == OnboardingItemState::Dismissed));
    }

    // ── restore (FR-014) ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn restore_re_derives_automatic_items_only() {
        let pool = setup_pool().await;
        get_state(&pool).await.unwrap();

        // Manually check one item, dismiss another — both must survive
        // restore untouched.
        set_item_state(
            &pool,
            &OnboardingItemSetStateRequest {
                item_id: "sessions.review_first".to_owned(),
                state: OnboardingManualState::ManuallyChecked,
            },
        )
        .await
        .unwrap();
        set_item_state(
            &pool,
            &OnboardingItemSetStateRequest {
                item_id: "calibration.review_masters".to_owned(),
                state: OnboardingManualState::Dismissed,
            },
        )
        .await
        .unwrap();

        // A milestone becomes newly true; only restore should surface it
        // (not a plain get_state read).
        sqlx::query(
            "INSERT INTO projects (id, name, tool, lifecycle, path, channel_drift, is_mosaic, created_at, updated_at) \
             VALUES ('proj-1', 'Proj 1', 'PixInsight', 'setup_incomplete', 'proj-1', 0, 0, \
                     '2026-07-18T00:00:00Z', '2026-07-18T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let resp = restore(&pool).await.unwrap();
        let by_id: HashMap<&str, OnboardingItemState> =
            resp.state.items.iter().map(|i| (i.item_id.as_str(), i.state)).collect();

        assert_eq!(by_id["sessions.review_first"], OnboardingItemState::ManuallyChecked);
        assert_eq!(by_id["calibration.review_masters"], OnboardingItemState::Dismissed);
        assert_eq!(by_id["projects.create_first"], OnboardingItemState::AutoChecked);
    }

    #[tokio::test]
    async fn restore_is_idempotent() {
        let pool = setup_pool().await;
        get_state(&pool).await.unwrap();
        let r1 = restore(&pool).await.unwrap();
        let r2 = restore(&pool).await.unwrap();
        let ids1: Vec<_> = r1.state.items.iter().map(|i| (i.item_id.clone(), i.state)).collect();
        let ids2: Vec<_> = r2.state.items.iter().map(|i| (i.item_id.clone(), i.state)).collect();
        assert_eq!(ids1, ids2);
    }
}
