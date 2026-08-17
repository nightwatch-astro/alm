// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! FilesystemPlan lifecycle state model (spec 002 data-model.md §FilesystemPlan).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use specta::Type;
use strum::{EnumString, IntoStaticStr};

use crate::ids::{EntityId, Timestamp};

/// Lifecycle state for a `FilesystemPlan`.
///
/// 10 variants per spec 002 data-model.md §FilesystemPlan (inc. `paused` R-Pause-1
/// and `discarded` spec 017 retry-chain terminal).
#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserialize,
    JsonSchema,
    Type,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum PlanState {
    Draft,
    ReadyForReview,
    Approved,
    Applying,
    /// Mid-apply suspension on `volume.unavailable`, `disk.full`, or `item.stale` (R-Pause-1).
    Paused,
    Applied,
    PartiallyApplied,
    Failed,
    Cancelled,
    /// Soft-delete terminal — paired with spec 017 retry-chain semantics.
    Discarded,
}

impl PlanState {
    /// Canonical snake_case string for this state (strum-backed).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        self.into()
    }

    /// Parse a snake_case string into a state, returning `None` on unknown input.
    #[must_use]
    pub fn parse_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }

    /// Terminal states: retry produces a NEW plan with `parent_plan_id` set.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Applied
                | Self::PartiallyApplied
                | Self::Failed
                | Self::Cancelled
                | Self::Discarded
        )
    }

    /// All variants in declaration order — single source of truth for SQL CHECK
    /// generation and schema-sync tests.
    pub const ALL: &'static [Self] = &[
        Self::Draft,
        Self::ReadyForReview,
        Self::Approved,
        Self::Applying,
        Self::Paused,
        Self::Applied,
        Self::PartiallyApplied,
        Self::Failed,
        Self::Cancelled,
        Self::Discarded,
    ];
}

/// Plan origin — who created this plan.
#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserialize,
    JsonSchema,
    Type,
)]
#[serde(rename_all = "snake_case")]
pub enum PlanCreatedBy {
    User,
    System,
}

/// Plan kind — category of filesystem mutations.
#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserialize,
    JsonSchema,
    Type,
)]
#[serde(rename_all = "snake_case")]
pub enum PlanKind {
    Organize,
    PrepareSource,
    Cleanup,
    Archive,
    RegenerateArtifact,
}

/// Canonical allowed `(from, to)` edge list per research.md §2.2 and spec 017.
/// Retry plans are NEW plans with `parent_plan_id` set — no terminal → non-terminal
/// edges exist except via fresh plan creation.
pub const TRANSITIONS: &[(PlanState, PlanState)] = &[
    (PlanState::Draft, PlanState::ReadyForReview),
    (PlanState::Draft, PlanState::Discarded),
    (PlanState::ReadyForReview, PlanState::Approved),
    (PlanState::ReadyForReview, PlanState::Draft),
    (PlanState::ReadyForReview, PlanState::Discarded),
    (PlanState::Approved, PlanState::Applying),
    (PlanState::Approved, PlanState::Draft),
    (PlanState::Applying, PlanState::Applied),
    (PlanState::Applying, PlanState::PartiallyApplied),
    (PlanState::Applying, PlanState::Failed),
    (PlanState::Applying, PlanState::Cancelled),
    (PlanState::Applying, PlanState::Paused),
    (PlanState::Paused, PlanState::Applying),
    (PlanState::Paused, PlanState::Cancelled),
];

#[must_use]
pub fn is_allowed(from: PlanState, to: PlanState) -> bool {
    TRANSITIONS.iter().any(|&(f, t)| f == from && t == to)
}

/// Stub entity struct — full item-level records wired in persistence layer.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FilesystemPlan {
    pub id: EntityId,
    pub kind: PlanKind,
    pub state: PlanState,
    /// Set when this plan is a retry of a failed/cancelled plan.
    pub parent_plan_id: Option<EntityId>,
    pub created_by: PlanCreatedBy,
    pub created_at: Timestamp,
    pub applied_at: Option<Timestamp>,
}
