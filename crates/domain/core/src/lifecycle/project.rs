// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Project lifecycle state model (spec 002 data-model.md §Project).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use specta::Type;
use strum::{EnumString, IntoStaticStr};

use crate::ids::{EntityId, Timestamp};

/// Lifecycle state for a `Project`.
///
/// 7 variants from spec 002 §Project and research.md §2.1.
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
pub enum ProjectState {
    SetupIncomplete,
    Ready,
    Prepared,
    Processing,
    Completed,
    Archived,
    /// Carries a `block_reason` on the entity row.
    Blocked,
}

impl ProjectState {
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

    #[must_use]
    pub const fn is_before_archive(self) -> bool {
        matches!(
            self,
            Self::SetupIncomplete
                | Self::Ready
                | Self::Prepared
                | Self::Processing
                | Self::Completed
        )
    }

    /// True when all edits are refused (R-Archived).
    #[must_use]
    pub const fn is_read_only(self) -> bool {
        matches!(self, Self::Archived)
    }

    /// True when `tool` field changes are refused (R-Tool-Lock).
    #[must_use]
    pub const fn is_tool_locked(self) -> bool {
        matches!(self, Self::Prepared | Self::Processing | Self::Completed | Self::Blocked)
    }

    /// True when source removal is refused (spec 008 FR-011).
    #[must_use]
    pub const fn is_source_remove_locked(self) -> bool {
        matches!(self, Self::Prepared | Self::Processing | Self::Completed | Self::Archived)
    }

    /// True when completed projects reopen on new data (F-Framing-6).
    #[must_use]
    pub const fn reopens_on_new_data(self) -> bool {
        matches!(self, Self::Completed)
    }
}

/// Canonical allowed `(from, to)` edge list per data-model.md §Project §Lifecycle
/// and research.md §2.1. 19 edges including the spec 009 R-Unarchive (`archived → ready`)
/// and A3 (`blocked → archived`) amendments.
pub const TRANSITIONS: &[(ProjectState, ProjectState)] = &[
    (ProjectState::SetupIncomplete, ProjectState::Ready),
    (ProjectState::SetupIncomplete, ProjectState::Blocked),
    (ProjectState::Ready, ProjectState::Prepared),
    (ProjectState::Ready, ProjectState::Processing),
    (ProjectState::Ready, ProjectState::Blocked),
    (ProjectState::Prepared, ProjectState::Ready),
    (ProjectState::Prepared, ProjectState::Processing),
    (ProjectState::Prepared, ProjectState::Blocked),
    (ProjectState::Processing, ProjectState::Completed),
    (ProjectState::Processing, ProjectState::Blocked),
    (ProjectState::Completed, ProjectState::Archived),
    (ProjectState::Completed, ProjectState::Processing),
    (ProjectState::Archived, ProjectState::Processing),
    (ProjectState::Archived, ProjectState::Ready),
    (ProjectState::Blocked, ProjectState::SetupIncomplete),
    (ProjectState::Blocked, ProjectState::Ready),
    (ProjectState::Blocked, ProjectState::Prepared),
    (ProjectState::Blocked, ProjectState::Processing),
    (ProjectState::Blocked, ProjectState::Archived),
];

#[must_use]
pub fn is_allowed(from: ProjectState, to: ProjectState) -> bool {
    TRANSITIONS.iter().any(|&(f, t)| f == from && t == to)
}

/// Snapshot of `{label, at, actor}` recorded in the UI projection column.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct LastAction {
    pub label: String,
    pub at: String,
    pub actor: String,
}

/// Stub entity struct — full fields wired in persistence layer.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub id: EntityId,
    pub name: String,
    pub target_id: EntityId,
    pub session_ids: Vec<EntityId>,
    pub state: ProjectState,
    pub last_action: Option<LastAction>,
    pub block_reason: Option<String>,
    pub created_at: Timestamp,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All 19 allowed edges, independently transcribed from
    /// specs/002-data-lifecycle-state-model/data-model.md §Lifecycle (the
    /// spec's `PROJECT_TRANSITIONS` table), NOT read from `TRANSITIONS`
    /// itself — so a drift between the table and the spec, or a broken
    /// `is_allowed`, is caught rather than checking `TRANSITIONS` against
    /// its own membership predicate.
    #[test]
    fn all_allowed_edges_are_accepted() {
        use ProjectState::{
            Archived, Blocked, Completed, Prepared, Processing, Ready, SetupIncomplete,
        };
        let spec_edges: &[(ProjectState, ProjectState)] = &[
            (SetupIncomplete, Ready),
            (SetupIncomplete, Blocked),
            (Ready, Prepared),
            (Ready, Processing),
            (Ready, Blocked),
            (Prepared, Ready),
            (Prepared, Processing),
            (Prepared, Blocked),
            (Processing, Completed),
            (Processing, Blocked),
            (Completed, Archived),
            (Completed, Processing),
            (Archived, Processing),
            (Archived, Ready),
            (Blocked, SetupIncomplete),
            (Blocked, Ready),
            (Blocked, Prepared),
            (Blocked, Processing),
            (Blocked, Archived),
        ];
        assert_eq!(spec_edges.len(), 19, "expected exactly 19 allowed edges");
        for &(from, to) in spec_edges {
            assert!(is_allowed(from, to), "expected ({from:?} → {to:?}) to be allowed");
        }
        assert_eq!(
            TRANSITIONS.len(),
            spec_edges.len(),
            "TRANSITIONS has drifted from the spec's edge count"
        );
        for &(from, to) in TRANSITIONS {
            assert!(
                spec_edges.contains(&(from, to)),
                "TRANSITIONS has an edge ({from:?} → {to:?}) not present in the spec table"
            );
        }
    }

    /// Forbidden edges: a representative set. Includes the explicitly
    /// documented forbidden cases: `processing → ready`, `blocked → completed`,
    /// `archived → completed`, and direct skip `setup_incomplete → prepared`.
    #[test]
    fn forbidden_edges_are_rejected() {
        let forbidden: &[(ProjectState, ProjectState)] = &[
            (ProjectState::Processing, ProjectState::Ready),
            (ProjectState::Blocked, ProjectState::Completed),
            (ProjectState::Archived, ProjectState::Completed),
            (ProjectState::Archived, ProjectState::Prepared),
            (ProjectState::SetupIncomplete, ProjectState::Prepared),
            (ProjectState::SetupIncomplete, ProjectState::Processing),
            (ProjectState::SetupIncomplete, ProjectState::Completed),
            (ProjectState::SetupIncomplete, ProjectState::Archived),
            (ProjectState::Ready, ProjectState::Completed),
            (ProjectState::Ready, ProjectState::Archived),
            (ProjectState::Ready, ProjectState::SetupIncomplete),
            (ProjectState::Prepared, ProjectState::Completed),
            (ProjectState::Prepared, ProjectState::Archived),
            (ProjectState::Prepared, ProjectState::SetupIncomplete),
            (ProjectState::Processing, ProjectState::Ready),
            (ProjectState::Processing, ProjectState::Prepared),
            (ProjectState::Processing, ProjectState::SetupIncomplete),
            (ProjectState::Processing, ProjectState::Archived),
            (ProjectState::Completed, ProjectState::Ready),
            (ProjectState::Completed, ProjectState::Prepared),
            (ProjectState::Completed, ProjectState::Blocked),
            (ProjectState::Completed, ProjectState::SetupIncomplete),
        ];
        for &(from, to) in forbidden {
            assert!(!is_allowed(from, to), "expected ({from:?} → {to:?}) to be forbidden");
        }
    }

    /// Verify specific edges from the spec (spot checks).
    #[test]
    fn blocked_archived_is_allowed_escape_hatch() {
        assert!(is_allowed(ProjectState::Blocked, ProjectState::Archived));
    }

    #[test]
    fn archived_ready_is_allowed_r_unarchive() {
        assert!(is_allowed(ProjectState::Archived, ProjectState::Ready));
    }

    #[test]
    fn blocked_completed_is_forbidden_a3() {
        assert!(!is_allowed(ProjectState::Blocked, ProjectState::Completed));
    }

    #[test]
    fn archived_prepared_is_forbidden() {
        assert!(!is_allowed(ProjectState::Archived, ProjectState::Prepared));
    }
}
