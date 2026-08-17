// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Rust DTOs mirroring `specs/002-data-lifecycle-state-model/contracts/lifecycle.transition.json`.
//!
//! Contract version: 2.0.0.
//! Field names are camelCase to match the JSON envelope sweep.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

pub const CONTRACT_VERSION: &str = "2.0.0";

// ── State enums — single-sourced from domain_core ────────────────────────────
//
// `ProjectState` and `PlanState` now live in `domain_core::lifecycle` and are
// re-exported here so every import of `contracts_core::lifecycle::PlanState`
// continues to resolve. The generated JSON schema now includes domain variant
// doc-comments (schemars reflects them as `description` fields); this is the
// intended schema-delta noted in kyo7.85.
pub use domain_core::lifecycle::plan::PlanState;
pub use domain_core::lifecycle::project::ProjectState;

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
pub enum DataSourceState {
    Active,
    Missing,
    Disabled,
    ReconnectRequired,
}

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
pub enum FileRecordState {
    Observed,
    Missing,
    Changed,
    Classified,
    Rejected,
    Protected,
}

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
pub enum PreparedSourceState {
    NotCreated,
    Planned,
    Ready,
    Stale,
    Retired,
}

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
pub enum ProjectionState {
    Current,
    Stale,
    Regenerating,
}

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
pub enum TransitionActor {
    User,
    System,
}

// ── Per-family request DTOs ───────────────────────────────────────────────────

// NOTE (issue #423): these structs deliberately carry NO `entity_type` field.
// The discriminant lives solely in `TransitionRequest`'s `#[serde(tag =
// "entityType")]` attribute. A previous duplicated `entity_type: String` field
// made the whole enum impossible to deserialize (serde consumes the tag key
// during variant dispatch, so the inner struct's required field could never be
// filled) and made serialization emit `entityType` twice.
macro_rules! transition_request_base {
    ($name:ident, $state_ty:ty) => {
        #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type)]
        #[serde(rename_all = "camelCase")]
        pub struct $name {
            pub contract_version: String,
            pub request_id: Uuid,
            pub entity_id: Uuid,
            pub current_state: $state_ty,
            pub next_state: $state_ty,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub action_label: Option<String>,
            pub actor: TransitionActor,
        }

        impl $name {
            #[must_use]
            pub fn new(
                request_id: Uuid,
                entity_id: Uuid,
                current_state: $state_ty,
                next_state: $state_ty,
                actor: TransitionActor,
            ) -> Self {
                Self {
                    contract_version: CONTRACT_VERSION.to_owned(),
                    request_id,
                    entity_id,
                    current_state,
                    next_state,
                    action_label: None,
                    actor,
                }
            }
        }
    };
}

transition_request_base!(ProjectTransitionRequest, ProjectState);
transition_request_base!(PlanTransitionRequest, PlanState);
transition_request_base!(DataSourceTransitionRequest, DataSourceState);
transition_request_base!(PreparedSourceTransitionRequest, PreparedSourceState);
transition_request_base!(ProjectionTransitionRequest, ProjectionState);
transition_request_base!(FileRecordTransitionRequest, FileRecordState);

// ── Discriminated request enum ────────────────────────────────────────────────

/// Discriminated request — one variant per entity family.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type)]
#[serde(tag = "entityType", rename_all = "snake_case")]
pub enum TransitionRequest {
    Project(ProjectTransitionRequest),
    Plan(PlanTransitionRequest),
    DataSource(DataSourceTransitionRequest),
    PreparedSource(PreparedSourceTransitionRequest),
    Projection(ProjectionTransitionRequest),
    FileRecord(FileRecordTransitionRequest),
}

// ── Error envelope ────────────────────────────────────────────────────────────

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
pub enum TransitionErrorCode {
    // Use dotted-form codes to match
    // `specs/002-data-lifecycle-state-model/contracts/lifecycle.transition.json`
    // §$defs.ErrorCode — the contract is the source of truth (Constitution §V).
    #[serde(rename = "transition.refused")]
    TransitionRefused,
    #[serde(rename = "entity.not_found")]
    EntityNotFound,
    #[serde(rename = "actor.not_authorised")]
    ActorNotAuthorised,
    #[serde(rename = "plan.required")]
    PlanRequired,
    #[serde(rename = "plan.not_approved")]
    PlanNotApproved,
    #[serde(rename = "provenance.unreviewed")]
    ProvenanceUnreviewed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct TransitionError {
    pub code: TransitionErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<crate::JsonAny>,
}

// ── Response ──────────────────────────────────────────────────────────────────

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
pub enum TransitionStatus {
    Success,
    Noop,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type)]
#[serde(rename_all = "camelCase")]
pub struct TransitionResponse {
    pub status: TransitionStatus,
    pub contract_version: String,
    pub request_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prior_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audit_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<TransitionError>,
}

impl TransitionResponse {
    #[must_use]
    pub fn success(
        request_id: Uuid,
        applied_at: String,
        prior: String,
        new: String,
        audit_id: Uuid,
    ) -> Self {
        Self {
            status: TransitionStatus::Success,
            contract_version: CONTRACT_VERSION.to_owned(),
            request_id,
            applied_at: Some(applied_at),
            prior_state: Some(prior),
            new_state: Some(new),
            audit_id: Some(audit_id),
            plan_id: None,
            error: None,
        }
    }

    #[must_use]
    pub fn noop(request_id: Uuid) -> Self {
        Self {
            status: TransitionStatus::Noop,
            contract_version: CONTRACT_VERSION.to_owned(),
            request_id,
            applied_at: None,
            prior_state: None,
            new_state: None,
            audit_id: None,
            plan_id: None,
            error: None,
        }
    }

    #[must_use]
    pub fn error(request_id: Uuid, error: TransitionError) -> Self {
        Self {
            status: TransitionStatus::Error,
            contract_version: CONTRACT_VERSION.to_owned(),
            request_id,
            applied_at: None,
            prior_state: None,
            new_state: None,
            audit_id: None,
            plan_id: None,
            error: Some(error),
        }
    }
}
