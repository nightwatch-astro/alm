// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Session, group, mosaic, and relation-proposal contracts.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use specta::Type;

use super::shared::{
    BoundedList, CanonicalId, ContractRange, Digest, EntityRef, FiniteDecimal, KeysetListOperation,
    LocalDate, MaterializationKind, MutationContext, NonBlankSafeText, PageRequest,
    PortableContractError, RevisionRef, Rfc3339Timestamp, SafeText, SupportedFrameKind,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(
    tag = "state",
    content = "value",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ValueState<T> {
    Known(T),
    Absent,
    Unknown,
    Contradictory { evidence_refs: BoundedList<SafeText, 100> },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum ObservingNightDerivation {
    AcquisitionTimezone {
        timezone: SafeText,
        local_boundary_time: SafeText,
    },
    ReviewedLocalFallback {
        local_boundary_time: SafeText,
        review_evidence_id: CanonicalId,
        reviewed_at: Rfc3339Timestamp,
        reviewed_by: CanonicalId,
        reason: SafeText,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActivePanelMembership {
    pub panel_group_id: CanonicalId,
    pub panel_revision_id: CanonicalId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub session_id: CanonicalId,
    pub materialization_operation_id: CanonicalId,
    pub materialization_kind: MaterializationKind,
    pub frame_kind: SupportedFrameKind,
    pub observing_night: LocalDate,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acquisition_timezone: Option<SafeText>,
    pub night_derivation: ObservingNightDerivation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_target_id: Option<CanonicalId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera_id: Option<CanonicalId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optical_profile_id: Option<CanonicalId>,
    pub frame_count: u64,
    pub created_at: Rfc3339Timestamp,
    pub superseded_by_session_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_panel_membership: Option<ActivePanelMembership>,
    pub warning_codes: BoundedList<SafeText, 100>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImmutableSessionIdentity {
    pub filter: ValueState<SafeText>,
    pub exposure_ms: ValueState<u64>,
    pub gain: ValueState<FiniteDecimal>,
    pub offset: ValueState<FiniteDecimal>,
    pub binning_x: ValueState<u32>,
    pub binning_y: ValueState<u32>,
    pub readout_mode: ValueState<SafeText>,
    pub raster_width: u32,
    pub raster_height: u32,
    pub crop_evidence: ValueState<SafeText>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry_evidence_id: Option<CanonicalId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionProvenance {
    pub source_group_id: CanonicalId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acquisition_site_id: Option<CanonicalId>,
    pub approved_at: Rfc3339Timestamp,
    pub approved_by: CanonicalId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImmutableSessionDetail {
    pub summary: SessionSummary,
    pub identity: ImmutableSessionIdentity,
    pub provenance: SessionProvenance,
    pub predecessor_session_count: u64,
    pub metadata_resolution_revision: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PanelGroupRevision {
    pub panel_group_id: CanonicalId,
    pub revision_id: CanonicalId,
    pub revision_number: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_revision_id: Option<CanonicalId>,
    pub accepted_head: bool,
    pub retired: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_target_id: Option<CanonicalId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cross_target_association_id: Option<CanonicalId>,
    pub session_count: u32,
    pub representative_session_id: CanonicalId,
    /// Absent on a revision written without an evidence envelope, which
    /// `crates/app/inbox/src/session_materialization/apply.rs` does for every
    /// ingestion-created singleton panel group.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub representative_evidence_id: Option<CanonicalId>,
    pub matching_settings_revision: u64,
    pub accepted_at: Rfc3339Timestamp,
    pub accepted_by: CanonicalId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_reason: Option<SafeText>,
    pub predecessor_group_count: u64,
    pub successor_group_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MosaicRevision {
    pub mosaic_id: CanonicalId,
    pub revision_id: CanonicalId,
    pub revision_number: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_revision_id: Option<CanonicalId>,
    pub accepted_head: bool,
    pub retired: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intended_target_id: Option<CanonicalId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cross_target_association_id: Option<CanonicalId>,
    pub panel_count: u32,
    pub edge_count: u32,
    /// Absent on a revision written without an evidence envelope.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub captured_union_evidence_id: Option<CanonicalId>,
    pub matching_settings_revision: u64,
    pub accepted_at: Rfc3339Timestamp,
    pub accepted_by: CanonicalId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_reason: Option<SafeText>,
    pub predecessor_mosaic_count: u64,
    pub successor_mosaic_count: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MosaicEdge {
    pub edge_id: CanonicalId,
    pub left_panel_revision_id: CanonicalId,
    pub right_panel_revision_id: CanonicalId,
    pub overlap_percent: FiniteDecimal,
    pub residual_sky_rotation_deg: FiniteDecimal,
    pub allowed_residual_rotation_ranges_deg: BoundedList<ContractRange<FiniteDecimal>, 16>,
    pub parity_match: bool,
    pub acquisition_geometry_compatible: bool,
    pub evidence_id: CanonicalId,
    pub stale: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invalidation_reason_code: Option<SafeText>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied_reclassification_plan_revision_id: Option<CanonicalId>,
}

impl MosaicEdge {
    #[must_use]
    pub fn stale_fields_consistent(&self) -> bool {
        self.stale
            == (self.invalidation_reason_code.is_some()
                && self.applied_reclassification_plan_revision_id.is_some())
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RelationProposalKind {
    PanelAdd,
    PanelReplace,
    PanelSplit,
    PanelMerge,
    MosaicCreate,
    MosaicEdge,
    MosaicSplit,
    MosaicMerge,
    ManualRelation,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ManualRelationKind {
    PanelAdd,
    PanelReplace,
    PanelSplit,
    PanelMerge,
    MosaicCreate,
    MosaicEdge,
    MosaicSplit,
    MosaicMerge,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RelationProposalState {
    Pending,
    Accepted,
    Rejected,
    Superseded,
    Stale,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TargetCompatibility {
    SameTarget,
    ReviewedCrossTarget,
    Incompatible,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceTernary {
    Compatible,
    Incompatible,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ParityEvidence {
    Match,
    Mismatch,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ThresholdComparison {
    Lt,
    Lte,
    Eq,
    Gte,
    Gt,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ThresholdOutcome {
    Pass,
    Fail,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ThresholdMeasurement {
    pub key: SafeText,
    pub measured_value: FiniteDecimal,
    pub unit: SafeText,
    pub comparison: ThresholdComparison,
    pub threshold_value: FiniteDecimal,
    pub outcome: ThresholdOutcome,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RelationEvidence {
    pub evidence_id: CanonicalId,
    pub target_compatibility: TargetCompatibility,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footprint_coverage_percent: Option<FiniteDecimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub center_separation_percent: Option<FiniteDecimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub residual_sky_rotation_deg: Option<FiniteDecimal>,
    pub allowed_residual_rotation_ranges_deg: BoundedList<ContractRange<FiniteDecimal>, 16>,
    pub parity: ParityEvidence,
    pub acquisition_geometry: EvidenceTernary,
    pub equipment: EvidenceTernary,
    pub missing_evidence_codes: BoundedList<SafeText, 100>,
    pub threshold_snapshot: BoundedList<ThresholdMeasurement, 100>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CoverageState {
    Full,
    Partial,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MosaicObjectEvidenceItem {
    pub canonical_object_id: CanonicalId,
    pub panel_containment_refs: BoundedList<EntityRef, 100>,
    pub session_containment_refs: BoundedList<EntityRef, 100>,
    pub coverage_state: CoverageState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProposalDecisionKind {
    Accepted,
    Rejected,
    Corrected,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProposalDecision {
    pub decision: ProposalDecisionKind,
    pub decided_at: Rfc3339Timestamp,
    pub reason: SafeText,
    pub audit_id: CanonicalId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum ManualTargetScope {
    SameTarget {
        canonical_target_id: CanonicalId,
    },
    ExistingCrossTarget {
        cross_target_association_id: CanonicalId,
    },
    NewReviewedCrossTarget {
        canonical_target_ids: BoundedList<CanonicalId, 500>,
        purpose: NonBlankSafeText,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ManualRelationReview {
    pub relation_kind: ManualRelationKind,
    pub review_reason: NonBlankSafeText,
    pub target_scope: ManualTargetScope,
    pub missing_evidence_codes: BoundedList<SafeText, 100>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RelationProposal {
    pub proposal_id: CanonicalId,
    pub proposal_revision: u64,
    pub kind: RelationProposalKind,
    pub state: RelationProposalState,
    pub source_revision_count: u64,
    pub subject_count: u64,
    pub proposed_membership_count: u64,
    pub proposed_edge_count: u64,
    pub proposed_lineage_count: u64,
    pub evidence: RelationEvidence,
    pub matching_settings_revision: u64,
    pub basis_fingerprint: Digest,
    pub created_at: Rfc3339Timestamp,
    pub created_by: CanonicalId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manual_relation: Option<ManualRelationReview>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<ProposalDecision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_by_proposal_id: Option<CanonicalId>,
}

impl RelationProposal {
    #[must_use]
    pub fn manual_relation_consistent(&self) -> bool {
        (self.kind == RelationProposalKind::ManualRelation) == self.manual_relation.is_some()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RelationDecisionCounts {
    pub accepted_revision_count: u64,
    pub retired_group_count: u64,
    pub session_supersession_count: u64,
    pub panel_lineage_count: u64,
    pub mosaic_lineage_count: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TraversalState {
    Queued,
    Running,
    Completed,
    Cancelled,
    CeilingExceeded,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TraversalPreviewProgress {
    pub operation_id: CanonicalId,
    pub read_watermark: u64,
    pub state: TraversalState,
    pub visited_node_count: u64,
    pub visited_edge_count: u64,
    pub frontier_count: u64,
    pub deepest_level: u32,
    pub updated_at: Rfc3339Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_error: Option<PortableContractError>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RelationListOperation {
    Session,
    SessionFrame,
    SessionSupersessionSuccessor,
    SessionSupersessionPredecessor,
    PanelMembership,
    PanelHistory,
    PanelLineagePredecessor,
    PanelLineageSuccessor,
    Panel,
    MosaicPanel,
    MosaicEdge,
    MosaicHistory,
    MosaicLineagePredecessor,
    MosaicLineageSuccessor,
    MosaicObjectEvidence,
    Proposal,
    ProposalSourceRevision,
    ProposalSubject,
    ProposalMembership,
    ProposalEdge,
    ProposalLineage,
    DecisionRevision,
    DecisionRetiredGroup,
    DecisionSessionSupersession,
    DecisionGroupLineage,
    TraversalNode,
    TraversalEdge,
}

impl RelationListOperation {
    pub const ALL: [Self; 27] = [
        Self::Session,
        Self::SessionFrame,
        Self::SessionSupersessionSuccessor,
        Self::SessionSupersessionPredecessor,
        Self::PanelMembership,
        Self::PanelHistory,
        Self::PanelLineagePredecessor,
        Self::PanelLineageSuccessor,
        Self::Panel,
        Self::MosaicPanel,
        Self::MosaicEdge,
        Self::MosaicHistory,
        Self::MosaicLineagePredecessor,
        Self::MosaicLineageSuccessor,
        Self::MosaicObjectEvidence,
        Self::Proposal,
        Self::ProposalSourceRevision,
        Self::ProposalSubject,
        Self::ProposalMembership,
        Self::ProposalEdge,
        Self::ProposalLineage,
        Self::DecisionRevision,
        Self::DecisionRetiredGroup,
        Self::DecisionSessionSupersession,
        Self::DecisionGroupLineage,
        Self::TraversalNode,
        Self::TraversalEdge,
    ];
}

impl KeysetListOperation for RelationListOperation {
    fn query_name(&self) -> &'static str {
        match self {
            Self::Session => "session.list",
            Self::SessionFrame => "session.frame.list",
            Self::SessionSupersessionSuccessor => "session.supersession_successor.list",
            Self::SessionSupersessionPredecessor => "session.supersession_predecessor.list",
            Self::PanelMembership => "panel_group.membership.list",
            Self::PanelHistory => "panel_group.history.list",
            Self::PanelLineagePredecessor => "panel_group.lineage_predecessor.list",
            Self::PanelLineageSuccessor => "panel_group.lineage_successor.list",
            Self::Panel => "panel_group.list",
            Self::MosaicPanel => "mosaic.panel.list",
            Self::MosaicEdge => "mosaic.edge.list",
            Self::MosaicHistory => "mosaic.history.list",
            Self::MosaicLineagePredecessor => "mosaic.lineage_predecessor.list",
            Self::MosaicLineageSuccessor => "mosaic.lineage_successor.list",
            Self::MosaicObjectEvidence => "mosaic.object_evidence.list",
            Self::Proposal => "relation_proposal.list",
            Self::ProposalSourceRevision => "relation_proposal.source_revision.list",
            Self::ProposalSubject => "relation_proposal.subject.list",
            Self::ProposalMembership => "relation_proposal.membership.list",
            Self::ProposalEdge => "relation_proposal.edge.list",
            Self::ProposalLineage => "relation_proposal.lineage.list",
            Self::DecisionRevision => "relation_proposal.decision_revision.list",
            Self::DecisionRetiredGroup => "relation_proposal.decision_retired_group.list",
            Self::DecisionSessionSupersession => {
                "relation_proposal.decision_session_supersession.list"
            }
            Self::DecisionGroupLineage => "relation_proposal.decision_group_lineage.list",
            Self::TraversalNode => "relation_traversal_preview.node.list",
            Self::TraversalEdge => "relation_traversal_preview.edge.list",
        }
    }

    fn unique_order(&self) -> &'static [&'static str] {
        match self {
            Self::Session => &["createdAt DESC", "sessionId ASC"],
            Self::SessionFrame => &["ordinal ASC", "frameId ASC"],
            Self::SessionSupersessionSuccessor => &["ordinal ASC", "successorSessionId ASC"],
            Self::SessionSupersessionPredecessor => &["ordinal ASC", "predecessorSessionId ASC"],
            Self::PanelMembership => &["ordinal ASC", "sessionId ASC"],
            Self::PanelHistory | Self::MosaicHistory => &["revisionNumber DESC", "revisionId ASC"],
            Self::PanelLineagePredecessor => &[
                "acceptedAt DESC",
                "acceptedProposalId ASC",
                "ordinal ASC",
                "predecessorGroupId ASC",
            ],
            Self::PanelLineageSuccessor => &[
                "acceptedAt DESC",
                "acceptedProposalId ASC",
                "ordinal ASC",
                "successorGroupId ASC",
            ],
            Self::Panel => &["acceptedAt DESC", "panelGroupId ASC"],
            Self::MosaicPanel => &["ordinal ASC", "panelRevisionId ASC", "panelGroupId ASC"],
            Self::MosaicEdge | Self::ProposalEdge => &["ordinal ASC", "edgeId ASC"],
            Self::MosaicLineagePredecessor => &[
                "acceptedAt DESC",
                "acceptedProposalId ASC",
                "ordinal ASC",
                "predecessorMosaicId ASC",
            ],
            Self::MosaicLineageSuccessor => &[
                "acceptedAt DESC",
                "acceptedProposalId ASC",
                "ordinal ASC",
                "successorMosaicId ASC",
            ],
            Self::MosaicObjectEvidence => &["canonicalObjectId ASC"],
            Self::Proposal => &["createdAt DESC", "proposalId ASC"],
            Self::ProposalSourceRevision | Self::DecisionRevision => {
                &["ordinal ASC", "entityType ASC", "entityId ASC", "revisionId ASC"]
            }
            Self::ProposalSubject | Self::ProposalMembership => {
                &["ordinal ASC", "entityType ASC", "entityId ASC"]
            }
            Self::ProposalLineage | Self::DecisionGroupLineage => {
                &["ordinal ASC", "predecessorGroupId ASC", "successorGroupId ASC"]
            }
            Self::DecisionRetiredGroup => &["ordinal ASC", "groupId ASC"],
            Self::DecisionSessionSupersession => {
                &["ordinal ASC", "predecessorSessionId ASC", "successorSessionId ASC"]
            }
            Self::TraversalNode => {
                &["ordinal ASC", "nodeRef.entityType ASC", "nodeRef.entityId ASC"]
            }
            Self::TraversalEdge => {
                &["ordinal ASC", "edgeRef.entityType ASC", "edgeRef.entityId ASC"]
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionQueryRequest {
    pub session_id: CanonicalId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionPageRequest {
    pub session_id: CanonicalId,
    pub page: PageRequest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PanelGroupQueryRequest {
    pub panel_group_id: CanonicalId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision_id: Option<CanonicalId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PanelGroupPageRequest {
    pub panel_group_id: CanonicalId,
    pub page: PageRequest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PanelGroupRevisionPageRequest {
    pub panel_group_id: CanonicalId,
    pub revision_id: CanonicalId,
    pub page: PageRequest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MosaicQueryRequest {
    pub mosaic_id: CanonicalId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision_id: Option<CanonicalId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MosaicPageRequest {
    pub mosaic_id: CanonicalId,
    pub page: PageRequest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MosaicRevisionPageRequest {
    pub mosaic_id: CanonicalId,
    pub revision_id: CanonicalId,
    pub page: PageRequest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProposalQueryRequest {
    pub proposal_id: CanonicalId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProposalPageRequest {
    pub proposal_id: CanonicalId,
    pub page: PageRequest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionListRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<CanonicalId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_kind: Option<SupportedFrameKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observing_night_from: Option<LocalDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observing_night_to: Option<LocalDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera_id: Option<CanonicalId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optical_profile_id: Option<CanonicalId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded: Option<SupersededFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub panel_group_id: Option<CanonicalId>,
    pub page: PageRequest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SupersededFilter {
    Exclude,
    Include,
    Only,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PanelListRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<CanonicalId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<CanonicalId>,
    #[serde(default = "default_true")]
    #[schemars(default = "default_true")]
    pub active_only: bool,
    pub page: PageRequest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProposalListRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<RelationProposalState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<RelationProposalKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<CanonicalId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_ref: Option<EntityRef>,
    pub page: PageRequest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GroupType {
    Panel,
    Mosaic,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DecisionPageRequest {
    pub proposal_id: CanonicalId,
    pub decision_snapshot_id: CanonicalId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_type: Option<GroupType>,
    pub page: PageRequest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TraversalGraph {
    PanelLineage,
    MosaicLineage,
    AcceptedMosaicConnectivity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TraversalDirection {
    Predecessors,
    Successors,
    Both,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TraversalLimits {
    #[schemars(range(min = 1, max = 4096))]
    pub max_depth: u32,
    #[schemars(range(min = 1, max = 100_000))]
    pub max_nodes: u64,
    #[schemars(range(min = 1, max = 2_000_000))]
    pub max_edges: u64,
}

const fn default_true() -> bool {
    true
}

const fn default_max_depth() -> u32 {
    64
}

const fn default_max_nodes() -> u64 {
    10_000
}

const fn default_max_edges() -> u64 {
    50_000
}

impl Default for TraversalLimits {
    fn default() -> Self {
        Self { max_depth: 64, max_nodes: 10_000, max_edges: 50_000 }
    }
}

impl TraversalLimits {
    #[must_use]
    pub const fn within_contract_bounds(&self) -> bool {
        self.max_depth >= 1
            && self.max_depth <= 4_096
            && self.max_nodes >= 1
            && self.max_nodes <= 100_000
            && self.max_edges >= 1
            && self.max_edges <= 2_000_000
    }
}

impl<'de> Deserialize<'de> for TraversalLimits {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[allow(clippy::struct_field_names)]
        #[serde(rename_all = "camelCase")]
        struct Wire {
            #[serde(default = "default_max_depth")]
            max_depth: u32,
            #[serde(default = "default_max_nodes")]
            max_nodes: u64,
            #[serde(default = "default_max_edges")]
            max_edges: u64,
        }

        let wire = Wire::deserialize(deserializer)?;
        let limits = Self {
            max_depth: wire.max_depth,
            max_nodes: wire.max_nodes,
            max_edges: wire.max_edges,
        };
        if !limits.within_contract_bounds() {
            return Err(serde::de::Error::custom("traversal limits are outside contract bounds"));
        }
        Ok(limits)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TraversalStartRequest {
    pub start_refs: BoundedList<EntityRef, 500>,
    pub graph: TraversalGraph,
    pub direction: TraversalDirection,
    #[serde(default)]
    #[schemars(default)]
    pub limits: TraversalLimits,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TraversalOperationRequest {
    pub operation_id: CanonicalId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TraversalResultPageRequest {
    pub operation_id: CanonicalId,
    pub page: PageRequest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(tag = "operation")]
pub enum RelationQuery {
    #[serde(rename = "session.query")]
    Session(SessionQueryRequest),
    #[serde(rename = "session.list")]
    SessionList(SessionListRequest),
    #[serde(rename = "session.frame.list")]
    SessionFrameList(SessionPageRequest),
    #[serde(rename = "session.supersession_successor.list")]
    SessionSupersessionSuccessorList(SessionPageRequest),
    #[serde(rename = "session.supersession_predecessor.list")]
    SessionSupersessionPredecessorList(SessionPageRequest),
    #[serde(rename = "panel_group.query")]
    PanelGroup(PanelGroupQueryRequest),
    #[serde(rename = "panel_group.membership.list")]
    PanelMembershipList(PanelGroupRevisionPageRequest),
    #[serde(rename = "panel_group.history.list")]
    PanelHistoryList(PanelGroupPageRequest),
    #[serde(rename = "panel_group.lineage_predecessor.list")]
    PanelLineagePredecessorList(PanelGroupPageRequest),
    #[serde(rename = "panel_group.lineage_successor.list")]
    PanelLineageSuccessorList(PanelGroupPageRequest),
    #[serde(rename = "panel_group.list")]
    PanelList(PanelListRequest),
    #[serde(rename = "mosaic.query")]
    Mosaic(MosaicQueryRequest),
    #[serde(rename = "mosaic.panel.list")]
    MosaicPanelList(MosaicRevisionPageRequest),
    #[serde(rename = "mosaic.edge.list")]
    MosaicEdgeList(MosaicRevisionPageRequest),
    #[serde(rename = "mosaic.history.list")]
    MosaicHistoryList(MosaicPageRequest),
    #[serde(rename = "mosaic.lineage_predecessor.list")]
    MosaicLineagePredecessorList(MosaicPageRequest),
    #[serde(rename = "mosaic.lineage_successor.list")]
    MosaicLineageSuccessorList(MosaicPageRequest),
    #[serde(rename = "mosaic.object_evidence.list")]
    MosaicObjectEvidenceList(MosaicRevisionPageRequest),
    #[serde(rename = "relation_proposal.list")]
    ProposalList(ProposalListRequest),
    #[serde(rename = "relation_proposal.query")]
    Proposal(ProposalQueryRequest),
    #[serde(rename = "relation_proposal.source_revision.list")]
    ProposalSourceRevisionList(ProposalPageRequest),
    #[serde(rename = "relation_proposal.subject.list")]
    ProposalSubjectList(ProposalPageRequest),
    #[serde(rename = "relation_proposal.membership.list")]
    ProposalMembershipList(ProposalPageRequest),
    #[serde(rename = "relation_proposal.edge.list")]
    ProposalEdgeList(ProposalPageRequest),
    #[serde(rename = "relation_proposal.lineage.list")]
    ProposalLineageList(ProposalPageRequest),
    #[serde(rename = "relation_proposal.decision_revision.list")]
    DecisionRevisionList(DecisionPageRequest),
    #[serde(rename = "relation_proposal.decision_retired_group.list")]
    DecisionRetiredGroupList(DecisionPageRequest),
    #[serde(rename = "relation_proposal.decision_session_supersession.list")]
    DecisionSessionSupersessionList(DecisionPageRequest),
    #[serde(rename = "relation_proposal.decision_group_lineage.list")]
    DecisionGroupLineageList(DecisionPageRequest),
    #[serde(rename = "relation_traversal_preview.progress.query")]
    TraversalProgress(TraversalOperationRequest),
    #[serde(rename = "relation_traversal_preview.result.query")]
    TraversalResult(TraversalOperationRequest),
    #[serde(rename = "relation_traversal_preview.node.list")]
    TraversalNodeList(TraversalResultPageRequest),
    #[serde(rename = "relation_traversal_preview.edge.list")]
    TraversalEdgeList(TraversalResultPageRequest),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProposedLineage {
    pub predecessor_group_id: CanonicalId,
    pub successor_group_id: CanonicalId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ManualRelationCreateRequest {
    pub relation_kind: ManualRelationKind,
    pub source_revision_refs: BoundedList<RevisionRef, 500>,
    pub subject_refs: BoundedList<EntityRef, 500>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposed_membership_refs: Option<BoundedList<EntityRef, 500>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposed_edges: Option<BoundedList<MosaicEdge, 500>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposed_lineage: Option<BoundedList<ProposedLineage, 500>>,
    pub target_scope: ManualTargetScope,
    pub evidence: RelationEvidence,
    pub review_reason: NonBlankSafeText,
    pub mutation_context: MutationContext,
}

impl ManualRelationCreateRequest {
    #[must_use]
    pub fn has_required_collections(&self) -> bool {
        !self.source_revision_refs.is_empty()
            && !self.subject_refs.is_empty()
            && [
                self.proposed_membership_refs.as_ref().map_or(0, BoundedList::len),
                self.proposed_edges.as_ref().map_or(0, BoundedList::len),
                self.proposed_lineage.as_ref().map_or(0, BoundedList::len),
            ]
            .into_iter()
            .sum::<usize>()
                > 0
    }

    /// # Errors
    ///
    /// Returns an error when the request cannot describe the selected relation kind.
    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.has_required_collections() {
            return Err("manual relation requires source, subject, and output collections");
        }
        if let ManualTargetScope::NewReviewedCrossTarget { canonical_target_ids, .. } =
            &self.target_scope
        {
            let distinct = canonical_target_ids.iter().collect::<std::collections::HashSet<_>>();
            if distinct.len() < 2 {
                return Err("reviewed cross-target scope requires two distinct target IDs");
            }
        }

        let membership_count = self.proposed_membership_refs.as_ref().map_or(0, BoundedList::len);
        let edge_count = self.proposed_edges.as_ref().map_or(0, BoundedList::len);
        let lineage_count = self.proposed_lineage.as_ref().map_or(0, BoundedList::len);
        let valid = match self.relation_kind {
            ManualRelationKind::PanelAdd | ManualRelationKind::PanelReplace => {
                self.source_revision_refs.len() == 1 && membership_count > 0
            }
            ManualRelationKind::PanelSplit | ManualRelationKind::PanelMerge => {
                membership_count > 0 && lineage_count > 0
            }
            ManualRelationKind::MosaicCreate => {
                self.source_revision_refs.len() >= 2 && edge_count > 0
            }
            ManualRelationKind::MosaicEdge => {
                self.source_revision_refs.len() == 2 && edge_count == 1
            }
            ManualRelationKind::MosaicSplit | ManualRelationKind::MosaicMerge => {
                membership_count > 0 && edge_count > 0 && lineage_count > 0
            }
        };
        if !valid {
            return Err("manual relation collections do not match relation kind");
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManualRelationCreateWire {
    relation_kind: ManualRelationKind,
    source_revision_refs: BoundedList<RevisionRef, 500>,
    subject_refs: BoundedList<EntityRef, 500>,
    proposed_membership_refs: Option<BoundedList<EntityRef, 500>>,
    proposed_edges: Option<BoundedList<MosaicEdge, 500>>,
    proposed_lineage: Option<BoundedList<ProposedLineage, 500>>,
    target_scope: ManualTargetScope,
    evidence: RelationEvidence,
    review_reason: NonBlankSafeText,
    mutation_context: MutationContext,
}

impl<'de> Deserialize<'de> for ManualRelationCreateRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = ManualRelationCreateWire::deserialize(deserializer)?;
        let request = Self {
            relation_kind: wire.relation_kind,
            source_revision_refs: wire.source_revision_refs,
            subject_refs: wire.subject_refs,
            proposed_membership_refs: wire.proposed_membership_refs,
            proposed_edges: wire.proposed_edges,
            proposed_lineage: wire.proposed_lineage,
            target_scope: wire.target_scope,
            evidence: wire.evidence,
            review_reason: wire.review_reason,
            mutation_context: wire.mutation_context,
        };
        request.validate().map_err(serde::de::Error::custom)?;
        Ok(request)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ManualRelationCreateResponse {
    pub proposal: RelationProposal,
    pub audit_id: CanonicalId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProposalAcceptRequest {
    pub proposal_id: CanonicalId,
    pub expected_proposal_revision: u64,
    pub expected_source_revision_set_digest: Digest,
    pub mutation_context: MutationContext,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProposalAcceptResponse {
    pub proposal: RelationProposal,
    pub decision_snapshot_id: CanonicalId,
    pub result_counts: RelationDecisionCounts,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cross_target_association_id: Option<CanonicalId>,
    pub audit_id: CanonicalId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProposalRejectRequest {
    pub proposal_id: CanonicalId,
    pub expected_proposal_revision: u64,
    pub rejection_reason: NonBlankSafeText,
    pub mutation_context: MutationContext,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProposalCorrection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub membership_refs: Option<BoundedList<EntityRef, 500>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_overrides: Option<BoundedList<MosaicEdge, 500>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intended_target_id: Option<CanonicalId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_scope: Option<ManualTargetScope>,
    pub note: NonBlankSafeText,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProposalCorrectRequest {
    pub proposal_id: CanonicalId,
    pub expected_proposal_revision: u64,
    pub correction: ProposalCorrection,
    pub mutation_context: MutationContext,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TraversalCancelRequest {
    pub operation_id: CanonicalId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(tag = "operation")]
pub enum RelationCommand {
    #[serde(rename = "relation_proposal.manual.create")]
    ManualCreate(ManualRelationCreateRequest),
    #[serde(rename = "relation_proposal.accept")]
    ProposalAccept(ProposalAcceptRequest),
    #[serde(rename = "relation_proposal.reject")]
    ProposalReject(ProposalRejectRequest),
    #[serde(rename = "relation_proposal.correct")]
    ProposalCorrect(ProposalCorrectRequest),
    #[serde(rename = "relation_traversal_preview.start")]
    TraversalStart(TraversalStartRequest),
    #[serde(rename = "relation_traversal_preview.cancel")]
    TraversalCancel(TraversalCancelRequest),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(tag = "event", rename_all_fields = "camelCase")]
pub enum RelationEvent {
    #[serde(rename = "session.materialized")]
    SessionMaterialized {
        session_id: CanonicalId,
        materialization_operation_id: CanonicalId,
        materialization_kind: MaterializationKind,
        frame_kind: SupportedFrameKind,
        frame_count: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        panel_group_id: Option<CanonicalId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        panel_revision_id: Option<CanonicalId>,
    },
    #[serde(rename = "session.superseded")]
    SessionSuperseded {
        predecessor_session_id: CanonicalId,
        replacement_session_count: u64,
        applied_reclassification_plan_revision_id: CanonicalId,
    },
    #[serde(rename = "relation_proposal.created")]
    ProposalCreated {
        proposal_id: CanonicalId,
        kind: RelationProposalKind,
        subject_count: u64,
        basis_fingerprint: Digest,
        #[serde(skip_serializing_if = "Option::is_none")]
        manual_relation_kind: Option<ManualRelationKind>,
        #[serde(skip_serializing_if = "Option::is_none")]
        missing_evidence_code_count: Option<u64>,
    },
    #[serde(rename = "relation_proposal.accepted")]
    ProposalAccepted {
        proposal_id: CanonicalId,
        decision_snapshot_id: CanonicalId,
        result_counts: RelationDecisionCounts,
        #[serde(skip_serializing_if = "Option::is_none")]
        cross_target_association_id: Option<CanonicalId>,
    },
    #[serde(rename = "cross_target_association.created")]
    CrossTargetAssociationCreated {
        cross_target_association_id: CanonicalId,
        accepted_proposal_id: CanonicalId,
        canonical_target_count: u64,
    },
    #[serde(rename = "relation_proposal.rejected")]
    ProposalRejected {
        proposal_id: CanonicalId,
        suppression_fingerprint: Digest,
        rejection_reason: SafeText,
    },
    #[serde(rename = "relation_proposal.corrected")]
    ProposalCorrected {
        proposal_id: CanonicalId,
        corrected_proposal_id: CanonicalId,
        correction_note: SafeText,
    },
    #[serde(rename = "group.head_changed")]
    GroupHeadChanged {
        group_type: GroupType,
        group_id: CanonicalId,
        previous_revision_id: CanonicalId,
        accepted_revision_id: CanonicalId,
    },
}
