// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Pure panel, mosaic, lineage, proposal, and object-evidence invariants.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt::Write,
};

use target_match::{CoverageState, FootprintUnion, ObjectShape, SkyFootprint};
use uuid::Uuid;

/// Stable logical panel identity and one immutable accepted revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelGroupRevision {
    pub panel_group_id: String,
    pub revision_id: String,
    pub parent_revision_id: Option<String>,
    pub session_ids: BTreeSet<String>,
    pub representative_session_id: String,
    pub matching_settings_revision: u64,
}

impl PanelGroupRevision {
    /// Create the stable singleton group required for every light session.
    #[must_use]
    pub fn singleton(
        panel_group_id: impl Into<String>,
        revision_id: impl Into<String>,
        session_id: impl Into<String>,
        matching_settings_revision: u64,
    ) -> Self {
        let session_id = session_id.into();
        Self {
            panel_group_id: panel_group_id.into(),
            revision_id: revision_id.into(),
            parent_revision_id: None,
            session_ids: BTreeSet::from([session_id.clone()]),
            representative_session_id: session_id,
            matching_settings_revision,
        }
    }

    /// Validate non-vacuous membership and representative stability.
    ///
    /// # Errors
    ///
    /// Returns an invariant error for empty membership or a representative
    /// outside the immutable membership snapshot.
    pub fn validate(&self) -> Result<(), RelationInvariantError> {
        if self.session_ids.is_empty() {
            return Err(RelationInvariantError::EmptyPanelMembership);
        }
        if !self.session_ids.contains(&self.representative_session_id) {
            return Err(RelationInvariantError::RepresentativeNotMember);
        }
        Ok(())
    }

    /// Validate all current panel heads as one immutable snapshot.
    ///
    /// # Errors
    ///
    /// Returns an invariant error for an invalid revision, duplicate current
    /// membership, or predecessor/replacement coexistence.
    pub fn validate_current(
        revisions: &[Self],
        supersessions: &[(String, String)],
    ) -> Result<(), RelationInvariantError> {
        validate_current_panel_membership(revisions, supersessions)
    }
}

/// Validate current panel heads as one snapshot.
///
/// A session belongs to at most one current group, and no current group may
/// contain both sides of an accepted supersession relation.
///
/// # Errors
///
/// Returns an invariant error for an invalid revision, duplicate current
/// membership, or predecessor/replacement coexistence.
fn validate_current_panel_membership(
    revisions: &[PanelGroupRevision],
    supersessions: &[(String, String)],
) -> Result<(), RelationInvariantError> {
    let mut current_members = BTreeSet::new();
    for revision in revisions {
        revision.validate()?;
        for session_id in &revision.session_ids {
            if !current_members.insert(session_id) {
                return Err(RelationInvariantError::DuplicateCurrentPanelMembership);
            }
        }
        if supersessions.iter().any(|(predecessor, successor)| {
            revision.session_ids.contains(predecessor) && revision.session_ids.contains(successor)
        }) {
            return Err(RelationInvariantError::PredecessorAndReplacementCoexist);
        }
    }
    Ok(())
}

/// One exact accepted adjacency edge between panel revisions.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MosaicEdge {
    id: String,
    left_revision: String,
    right_revision: String,
    evidence: String,
}

impl MosaicEdge {
    /// Construct a validated undirected panel-adjacency edge.
    ///
    /// # Errors
    ///
    /// Returns an invariant error when the edge ID, evidence ID, or either
    /// endpoint is blank or whitespace-only.
    pub fn new(
        edge_id: impl Into<String>,
        left: impl Into<String>,
        right: impl Into<String>,
        evidence_id: impl Into<String>,
    ) -> Result<Self, RelationInvariantError> {
        let id = edge_id.into();
        let left_revision = left.into();
        let right_revision = right.into();
        let evidence = evidence_id.into();
        if id.trim().is_empty() {
            return Err(RelationInvariantError::BlankMosaicEdgeId);
        }
        if evidence.trim().is_empty() {
            return Err(RelationInvariantError::BlankEvidenceId);
        }
        if left_revision.trim().is_empty() || right_revision.trim().is_empty() {
            return Err(RelationInvariantError::BlankPanelRevisionId);
        }
        Ok(Self { id, left_revision, right_revision, evidence })
    }

    #[must_use]
    pub fn edge_id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub fn evidence_id(&self) -> &str {
        &self.evidence
    }

    #[must_use]
    pub fn endpoints(&self) -> (&str, &str) {
        (&self.left_revision, &self.right_revision)
    }

    /// Whether this edge connects two components implied by accepted edges.
    #[must_use]
    pub fn bridges(&self, accepted_edges: &[Self]) -> bool {
        edge_bridges_components(&self.left_revision, &self.right_revision, accepted_edges)
    }

    /// Whether this edge connects two accepted mosaic membership snapshots.
    #[must_use]
    pub fn bridges_accepted(&self, accepted_components: &[BTreeSet<String>]) -> bool {
        edge_bridges_accepted_mosaics(
            &self.left_revision,
            &self.right_revision,
            accepted_components,
        )
    }

    fn canonical_undirected_record(&self) -> (&str, &str, &str, &str) {
        let (left, right) = self.endpoints();
        if left <= right {
            (left, right, self.edge_id(), self.evidence_id())
        } else {
            (right, left, self.edge_id(), self.evidence_id())
        }
    }
}

/// Validate that exact mosaic members are connected by exact accepted edges.
///
/// # Errors
///
/// Returns an invariant error when fewer than two panels are supplied, an edge
/// is self-referential or outside membership, or the accepted graph is not
/// connected.
pub fn validate_mosaic_connectivity(
    panels: &BTreeSet<String>,
    edges: &[MosaicEdge],
) -> Result<(), RelationInvariantError> {
    if panels.len() < 2 {
        return Err(RelationInvariantError::MosaicNeedsTwoPanels);
    }
    let mut adjacency: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for edge in edges {
        let (left, right) = edge.endpoints();
        if left == right || !panels.contains(left) || !panels.contains(right) {
            return Err(RelationInvariantError::InvalidMosaicEdge);
        }
        adjacency.entry(left).or_default().push(right);
        adjacency.entry(right).or_default().push(left);
    }
    let Some(seed) = panels.first().map(String::as_str) else {
        return Err(RelationInvariantError::MosaicNeedsTwoPanels);
    };
    let mut visited = BTreeSet::from([seed]);
    let mut queue = VecDeque::from([seed]);
    while let Some(node) = queue.pop_front() {
        for next in adjacency.get(node).into_iter().flatten().copied() {
            if visited.insert(next) {
                queue.push_back(next);
            }
        }
    }
    if visited.len() != panels.len() {
        return Err(RelationInvariantError::DisconnectedMosaic);
    }
    Ok(())
}

/// Whether a proposed edge connects two already accepted components.
#[must_use]
fn edge_bridges_components(left: &str, right: &str, accepted_edges: &[MosaicEdge]) -> bool {
    if left == right {
        return false;
    }
    !reachable(left, right, accepted_edges)
        && accepted_edges
            .iter()
            .any(|edge| edge.left_revision == left || edge.right_revision == left)
        && accepted_edges
            .iter()
            .any(|edge| edge.left_revision == right || edge.right_revision == right)
}

/// Detect a bridge when accepted component membership is already known.
#[must_use]
fn edge_bridges_accepted_mosaics(
    left: &str,
    right: &str,
    accepted_components: &[BTreeSet<String>],
) -> bool {
    let left_component = accepted_components.iter().position(|component| component.contains(left));
    let right_component =
        accepted_components.iter().position(|component| component.contains(right));
    matches!((left_component, right_component), (Some(left), Some(right)) if left != right)
}

fn reachable(start: &str, destination: &str, edges: &[MosaicEdge]) -> bool {
    let mut visited = BTreeSet::from([start]);
    let mut queue = VecDeque::from([start]);
    while let Some(node) = queue.pop_front() {
        for edge in edges {
            let next = if edge.left_revision == node {
                Some(edge.right_revision.as_str())
            } else if edge.right_revision == node {
                Some(edge.left_revision.as_str())
            } else {
                None
            };
            if let Some(next) = next {
                if next == destination {
                    return true;
                }
                if visited.insert(next) {
                    queue.push_back(next);
                }
            }
        }
    }
    false
}

/// Validate a directed lineage snapshot, including proposed edges, is acyclic.
///
/// # Errors
///
/// Returns [`RelationInvariantError::LineageCycle`] for a self-edge or cycle.
pub fn validate_acyclic_lineage(edges: &[(String, String)]) -> Result<(), RelationInvariantError> {
    let mut successors: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut indegree: BTreeMap<&str, usize> = BTreeMap::new();
    for (predecessor, successor) in edges {
        if predecessor == successor {
            return Err(RelationInvariantError::LineageCycle);
        }
        successors.entry(predecessor).or_default().push(successor);
        indegree.entry(predecessor).or_default();
        *indegree.entry(successor).or_default() += 1;
    }
    let mut queue: VecDeque<&str> =
        indegree.iter().filter_map(|(node, degree)| (*degree == 0).then_some(*node)).collect();
    let mut visited = 0;
    while let Some(node) = queue.pop_front() {
        visited += 1;
        for successor in successors.get(node).into_iter().flatten().copied() {
            if let Some(degree) = indegree.get_mut(successor) {
                *degree -= 1;
                if *degree == 0 {
                    queue.push_back(successor);
                }
            }
        }
    }
    if visited != indegree.len() {
        return Err(RelationInvariantError::LineageCycle);
    }
    Ok(())
}

/// Kinds accepted by explicit manual relation review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationKind {
    PanelAdd,
    PanelReplace,
    PanelSplit,
    PanelMerge,
    MosaicCreate,
    MosaicEdge,
    MosaicSplit,
    MosaicMerge,
}

/// Target scope reviewed with a manual relation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetScope {
    SameTarget { canonical_target_id: String },
    ExistingCrossTarget { association_id: String },
    NewReviewedCrossTarget { canonical_target_ids: BTreeSet<String> },
}

/// Pure shape of a manual relation proposal before persistence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualRelation {
    pub kind: RelationKind,
    pub review_reason: String,
    pub target_scope: TargetScope,
    pub source_revision_ids: BTreeSet<String>,
    pub subject_ids: BTreeSet<String>,
    pub membership_ids: BTreeSet<String>,
    pub edges: Vec<MosaicEdge>,
    pub lineage: Vec<(String, String)>,
    pub missing_evidence_codes: BTreeSet<String>,
}

impl ManualRelation {
    /// Enforce the non-vacuous and kind-specific contract before persistence.
    ///
    /// # Errors
    ///
    /// Returns an invariant error when inputs or outputs are empty, target
    /// scope is invalid, or the kind-specific membership/topology shape fails.
    pub fn validate(&self) -> Result<(), RelationInvariantError> {
        if self.review_reason.trim().is_empty() {
            return Err(RelationInvariantError::MissingReviewReason);
        }
        if self.source_revision_ids.is_empty() || self.subject_ids.is_empty() {
            return Err(RelationInvariantError::EmptyProposalInputs);
        }
        if self.membership_ids.is_empty() && self.edges.is_empty() && self.lineage.is_empty() {
            return Err(RelationInvariantError::RelationFreeProposal);
        }
        if let TargetScope::NewReviewedCrossTarget { canonical_target_ids } = &self.target_scope {
            if canonical_target_ids.len() < 2 {
                return Err(RelationInvariantError::CrossTargetNeedsTwoTargets);
            }
        }
        match self.kind {
            RelationKind::PanelAdd | RelationKind::PanelReplace => {
                if self.source_revision_ids.len() != 1 || self.membership_ids.is_empty() {
                    return Err(RelationInvariantError::KindShapeMismatch);
                }
            }
            RelationKind::PanelSplit | RelationKind::PanelMerge => {
                if self.membership_ids.is_empty() || self.lineage.is_empty() {
                    return Err(RelationInvariantError::KindShapeMismatch);
                }
            }
            RelationKind::MosaicCreate => {
                validate_mosaic_connectivity(&self.membership_ids, &self.edges)?;
            }
            RelationKind::MosaicEdge => {
                if self.subject_ids.len() != 2 || self.edges.len() != 1 {
                    return Err(RelationInvariantError::KindShapeMismatch);
                }
                let edge = &self.edges[0];
                if edge.left_revision == edge.right_revision {
                    return Err(RelationInvariantError::InvalidMosaicEdge);
                }
                let reviewed_endpoints =
                    BTreeSet::from([edge.left_revision.clone(), edge.right_revision.clone()]);
                if reviewed_endpoints != self.subject_ids {
                    return Err(RelationInvariantError::MosaicEdgeOutsideReviewedSubjects);
                }
            }
            RelationKind::MosaicSplit | RelationKind::MosaicMerge => {
                if self.membership_ids.is_empty()
                    || self.edges.is_empty()
                    || self.lineage.is_empty()
                {
                    return Err(RelationInvariantError::KindShapeMismatch);
                }
                validate_mosaic_connectivity(&self.membership_ids, &self.edges)?;
            }
        }
        validate_acyclic_lineage(&self.lineage)
    }
}

/// Lifecycle state of one immutable relation-proposal revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalState {
    Pending,
    Accepted,
    Rejected,
    Superseded,
    Stale,
}

/// Canonical inputs that materially determine an automatic proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposalBasis {
    pub kind: RelationKind,
    pub target_scope: TargetScope,
    pub source_revision_ids: BTreeSet<String>,
    pub subject_ids: BTreeSet<String>,
    pub membership_ids: BTreeSet<String>,
    pub edges: Vec<MosaicEdge>,
    pub lineage: Vec<(String, String)>,
}

impl ProposalBasis {
    /// Produce a deterministic fingerprint independent of collection order.
    #[must_use]
    pub fn fingerprint(&self, evidence_revision: &str, matching_settings_revision: u64) -> String {
        let mut canonical = String::new();
        push_canonical_field(&mut canonical, "proposal_basis_v1");
        push_canonical_field(&mut canonical, self.kind.canonical_name());
        push_target_scope(&mut canonical, &self.target_scope);
        push_canonical_field(&mut canonical, "source_revisions");
        push_sorted_strings(&mut canonical, &self.source_revision_ids);
        push_canonical_field(&mut canonical, "subjects");
        push_sorted_strings(&mut canonical, &self.subject_ids);
        push_canonical_field(&mut canonical, "memberships");
        push_sorted_strings(&mut canonical, &self.membership_ids);

        let edges: BTreeSet<_> =
            self.edges.iter().map(MosaicEdge::canonical_undirected_record).collect();
        push_canonical_field(&mut canonical, "edges");
        push_canonical_field(&mut canonical, &edges.len().to_string());
        for (left, right, edge_id, evidence_id) in edges {
            push_canonical_field(&mut canonical, left);
            push_canonical_field(&mut canonical, right);
            push_canonical_field(&mut canonical, edge_id);
            push_canonical_field(&mut canonical, evidence_id);
        }

        let mut lineage = self.lineage.clone();
        lineage.sort();
        push_canonical_field(&mut canonical, "lineage");
        push_canonical_field(&mut canonical, &lineage.len().to_string());
        for (predecessor, successor) in lineage {
            push_canonical_field(&mut canonical, &predecessor);
            push_canonical_field(&mut canonical, &successor);
        }
        push_canonical_field(&mut canonical, evidence_revision);
        push_canonical_field(&mut canonical, &matching_settings_revision.to_string());
        Uuid::new_v5(&Uuid::NAMESPACE_OID, canonical.as_bytes()).to_string()
    }
}

impl RelationKind {
    fn canonical_name(self) -> &'static str {
        match self {
            Self::PanelAdd => "panel_add",
            Self::PanelReplace => "panel_replace",
            Self::PanelSplit => "panel_split",
            Self::PanelMerge => "panel_merge",
            Self::MosaicCreate => "mosaic_create",
            Self::MosaicEdge => "mosaic_edge",
            Self::MosaicSplit => "mosaic_split",
            Self::MosaicMerge => "mosaic_merge",
        }
    }
}

fn push_target_scope(canonical: &mut String, target_scope: &TargetScope) {
    match target_scope {
        TargetScope::SameTarget { canonical_target_id } => {
            push_canonical_field(canonical, "same_target");
            push_canonical_field(canonical, canonical_target_id);
        }
        TargetScope::ExistingCrossTarget { association_id } => {
            push_canonical_field(canonical, "existing_cross_target");
            push_canonical_field(canonical, association_id);
        }
        TargetScope::NewReviewedCrossTarget { canonical_target_ids } => {
            push_canonical_field(canonical, "new_reviewed_cross_target");
            push_sorted_strings(canonical, canonical_target_ids);
        }
    }
}

fn push_sorted_strings(canonical: &mut String, values: &BTreeSet<String>) {
    let _ = write!(canonical, "{}[", values.len());
    for value in values {
        push_canonical_field(canonical, value);
    }
    canonical.push(']');
}

fn push_canonical_field(canonical: &mut String, value: &str) {
    let _ = write!(canonical, "{}:{value};", value.len());
}

/// Immutable proposal head used for optimistic acceptance and stale marking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationProposal {
    pub revision: u64,
    pub state: ProposalState,
    pub basis_fingerprint: String,
}

impl RelationProposal {
    #[must_use]
    pub fn pending(revision: u64, basis_fingerprint: impl Into<String>) -> Self {
        Self {
            revision,
            state: ProposalState::Pending,
            basis_fingerprint: basis_fingerprint.into(),
        }
    }

    /// Validate the optimistic revision and current material basis.
    ///
    /// # Errors
    ///
    /// Returns a state, revision, or basis-staleness error without changing
    /// the proposal.
    pub fn validate_pending(
        &self,
        expected_revision: u64,
        current_basis_fingerprint: &str,
    ) -> Result<(), RelationInvariantError> {
        if self.state != ProposalState::Pending {
            return Err(RelationInvariantError::ProposalNotPending);
        }
        if self.revision != expected_revision {
            return Err(RelationInvariantError::StaleProposalRevision);
        }
        if self.basis_fingerprint != current_basis_fingerprint {
            return Err(RelationInvariantError::StaleProposalBasis);
        }
        Ok(())
    }

    /// Mark this exact pending revision stale and advance its CAS token.
    ///
    /// # Errors
    ///
    /// Returns a state or revision error and leaves the proposal unchanged.
    pub fn mark_stale(&mut self, expected_revision: u64) -> Result<(), RelationInvariantError> {
        if self.state != ProposalState::Pending {
            return Err(RelationInvariantError::ProposalNotPending);
        }
        if self.revision != expected_revision {
            return Err(RelationInvariantError::StaleProposalRevision);
        }
        let successor_revision = self
            .revision
            .checked_add(1)
            .ok_or(RelationInvariantError::ProposalRevisionExhausted)?;
        self.state = ProposalState::Stale;
        self.revision = successor_revision;
        Ok(())
    }
}

/// Complete fingerprint that suppresses only equivalent automatic proposals.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RejectionFingerprint {
    pub basis_fingerprint: String,
    pub evidence_revision: String,
    pub matching_settings_revision: u64,
}

impl RejectionFingerprint {
    #[must_use]
    pub fn from_basis(
        basis: &ProposalBasis,
        evidence_revision: impl Into<String>,
        matching_settings_revision: u64,
    ) -> Self {
        let evidence_revision = evidence_revision.into();
        Self {
            basis_fingerprint: basis.fingerprint(&evidence_revision, matching_settings_revision),
            evidence_revision,
            matching_settings_revision,
        }
    }
}

/// In-memory domain model of remembered rejection semantics.
#[derive(Debug, Default)]
pub struct RememberedRejections(BTreeSet<RejectionFingerprint>);

impl RememberedRejections {
    pub fn remember(&mut self, fingerprint: RejectionFingerprint) {
        self.0.insert(fingerprint);
    }

    #[must_use]
    pub fn suppresses(&self, fingerprint: &RejectionFingerprint) -> bool {
        self.0.contains(fingerprint)
    }
}

/// Contract-facing object coverage retained for a captured mosaic union.
#[derive(Debug, Clone, PartialEq)]
pub struct MosaicObjectEvidence {
    pub state: MosaicObjectCoverage,
    pub covered_fraction: Option<f64>,
    pub panel_evidence_ids: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MosaicObjectCoverage {
    Full,
    Partial,
}

fn measure_mosaic_object(
    union: &FootprintUnion,
    object: ObjectShape<'_>,
) -> target_match::Result<Option<MosaicObjectEvidence>> {
    let evidence = union.measure_object(object)?;
    if evidence.state == CoverageState::None {
        return Ok(None);
    }
    let panel_evidence_ids = if let Some(point) = evidence.point {
        point
            .panels
            .into_iter()
            .filter(|panel| panel.containment.is_covered())
            .map(|panel| panel.provenance.as_str().to_owned())
            .collect()
    } else {
        evidence.panels.into_iter().map(|panel| panel.provenance.as_str().to_owned()).collect()
    };
    Ok(Some(MosaicObjectEvidence {
        state: match evidence.state {
            CoverageState::Partial => MosaicObjectCoverage::Partial,
            CoverageState::Full => MosaicObjectCoverage::Full,
            CoverageState::None => unreachable!("zero coverage returned early"),
        },
        covered_fraction: evidence.covered_fraction,
        panel_evidence_ids,
    }))
}

/// Captured, hole-aware geometry for one exact mosaic revision.
#[derive(Debug, Clone)]
pub struct CapturedMosaic(FootprintUnion);

impl CapturedMosaic {
    /// Build captured geometry while preserving gaps and disconnected regions.
    ///
    /// # Errors
    ///
    /// Returns the upstream typed geometry error for an empty set, duplicate
    /// provenance, incompatible epochs, projection failure, or invalid geometry.
    pub fn new(footprints: &[SkyFootprint]) -> target_match::Result<Self> {
        FootprintUnion::new(footprints).map(Self)
    }

    #[must_use]
    pub fn component_count(&self) -> usize {
        self.0.component_count()
    }

    /// Measure one object and exclude zero-coverage results, including gap points.
    ///
    /// # Errors
    ///
    /// Returns the upstream typed geometry error when the object cannot be
    /// projected or measured against the union's persisted anchor.
    pub fn measure(
        &self,
        object: ObjectShape<'_>,
    ) -> target_match::Result<Option<MosaicObjectEvidence>> {
        measure_mosaic_object(&self.0, object)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationInvariantError {
    EmptyPanelMembership,
    RepresentativeNotMember,
    MosaicNeedsTwoPanels,
    InvalidMosaicEdge,
    DisconnectedMosaic,
    LineageCycle,
    MissingReviewReason,
    EmptyProposalInputs,
    RelationFreeProposal,
    CrossTargetNeedsTwoTargets,
    KindShapeMismatch,
    DuplicateCurrentPanelMembership,
    PredecessorAndReplacementCoexist,
    MosaicEdgeOutsideReviewedSubjects,
    ProposalNotPending,
    StaleProposalRevision,
    StaleProposalBasis,
    ProposalRevisionExhausted,
    BlankMosaicEdgeId,
    BlankEvidenceId,
    BlankPanelRevisionId,
}

impl std::fmt::Display for RelationInvariantError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for RelationInvariantError {}

#[cfg(test)]
mod tests {
    use super::*;
    // target-match embeds skymath 0.6; use its re-export so the types match.
    use target_match::{skymath as tm_skymath, FootprintProvenance, ImageParity, SkyEllipse};
    use tm_skymath::{Angle, Equatorial};

    fn edge(left: &str, right: &str) -> MosaicEdge {
        MosaicEdge::new(
            format!("edge-{left}-{right}"),
            left,
            right,
            format!("evidence-{left}-{right}"),
        )
        .expect("valid edge")
    }

    fn identified_edge(edge_id: &str, left: &str, right: &str, evidence_id: &str) -> MosaicEdge {
        MosaicEdge::new(edge_id, left, right, evidence_id).expect("valid edge")
    }

    fn coordinate(ra: f64, dec: f64) -> Equatorial {
        Equatorial::j2000(Angle::from_degrees(ra), Angle::from_degrees(dec))
            .expect("valid coordinate")
    }

    fn square(centre_ra: f64, provenance: &str) -> SkyFootprint {
        SkyFootprint::new(
            coordinate(centre_ra, 0.0),
            vec![
                coordinate(centre_ra - 0.5, -0.5),
                coordinate(centre_ra + 0.5, -0.5),
                coordinate(centre_ra + 0.5, 0.5),
                coordinate(centre_ra - 0.5, 0.5),
            ],
            Angle::from_degrees(0.0),
            ImageParity::Direct,
            FootprintProvenance::new(provenance).expect("valid provenance"),
        )
        .expect("valid footprint")
    }

    #[test]
    fn singleton_panel_is_valid_and_stable() {
        let group = PanelGroupRevision::singleton("panel", "r1", "session", 7);
        assert_eq!(group.representative_session_id, "session");
        assert_eq!(group.matching_settings_revision, 7);
        assert_eq!(group.validate(), Ok(()));
    }

    #[test]
    fn current_panel_membership_excludes_duplicates_and_replacements() {
        let first = PanelGroupRevision::singleton("p1", "r1", "old", 1);
        let duplicate = PanelGroupRevision::singleton("p2", "r2", "old", 1);
        assert_eq!(
            PanelGroupRevision::validate_current(&[first.clone(), duplicate], &[]),
            Err(RelationInvariantError::DuplicateCurrentPanelMembership)
        );

        let mut coexist = first;
        coexist.session_ids.insert("new".into());
        assert_eq!(
            PanelGroupRevision::validate_current(&[coexist], &[("old".into(), "new".into())],),
            Err(RelationInvariantError::PredecessorAndReplacementCoexist)
        );
    }

    #[test]
    fn mosaic_requires_connected_exact_members() {
        let panels = BTreeSet::from(["a".into(), "b".into(), "c".into()]);
        assert_eq!(
            validate_mosaic_connectivity(&panels, &[edge("a", "b")]),
            Err(RelationInvariantError::DisconnectedMosaic)
        );
        assert_eq!(
            validate_mosaic_connectivity(&panels, &[edge("a", "b"), edge("b", "c")]),
            Ok(())
        );
    }

    #[test]
    fn mosaic_connectivity_is_edge_order_invariant() {
        let panels = BTreeSet::from(["a".into(), "b".into(), "c".into(), "d".into()]);
        let permutations = [
            vec![edge("a", "b"), edge("b", "c"), edge("c", "d")],
            vec![edge("c", "d"), edge("a", "b"), edge("b", "c")],
            vec![edge("b", "c"), edge("c", "d"), edge("a", "b")],
        ];
        for edges in permutations {
            assert_eq!(validate_mosaic_connectivity(&panels, &edges), Ok(()));
        }
    }

    #[test]
    fn bridge_between_accepted_components_requires_merge_review() {
        let accepted = [edge("a", "b"), edge("c", "d")];
        assert!(edge("b", "c").bridges(&accepted));
        assert!(!edge("a", "b").bridges(&accepted));
        let components =
            [BTreeSet::from(["singleton".into()]), BTreeSet::from(["a".into(), "b".into()])];
        assert!(edge("singleton", "a").bridges_accepted(&components));
    }

    #[test]
    fn bridge_detection_handles_long_components_and_unknown_endpoints() {
        let left: BTreeSet<String> = (0..1_000).map(|index| format!("left-{index}")).collect();
        let right: BTreeSet<String> = (0..1_000).map(|index| format!("right-{index}")).collect();
        let components = [left, right];
        assert!(edge("left-999", "right-999").bridges_accepted(&components));
        assert!(!edge("left-0", "left-999").bridges_accepted(&components));
        assert!(!edge("left-0", "unknown").bridges_accepted(&components));
    }

    #[test]
    fn mosaic_edge_rejects_blank_nested_identifiers() {
        for blank in ["", " ", "\t\n"] {
            assert_eq!(
                MosaicEdge::new(blank, "left", "right", "evidence"),
                Err(RelationInvariantError::BlankMosaicEdgeId)
            );
            assert_eq!(
                MosaicEdge::new("edge", "left", "right", blank),
                Err(RelationInvariantError::BlankEvidenceId)
            );
            assert_eq!(
                MosaicEdge::new("edge", blank, "right", "evidence"),
                Err(RelationInvariantError::BlankPanelRevisionId)
            );
        }

        let nested: Result<Vec<_>, _> =
            [("edge-a", "evidence-a"), ("edge-b", "   "), ("edge-c", "evidence-c")]
                .into_iter()
                .map(|(edge_id, evidence_id)| {
                    MosaicEdge::new(edge_id, "left", "right", evidence_id)
                })
                .collect();
        assert_eq!(nested, Err(RelationInvariantError::BlankEvidenceId));
    }

    #[test]
    fn lineage_cycle_is_rejected() {
        let edges = vec![("a".into(), "b".into()), ("b".into(), "a".into())];
        assert_eq!(validate_acyclic_lineage(&edges), Err(RelationInvariantError::LineageCycle));
    }

    #[test]
    fn remembered_rejection_changes_with_material_evidence() {
        let basis = proposal_basis();
        let old = RejectionFingerprint::from_basis(&basis, "e1", 1);
        let changed = RejectionFingerprint::from_basis(&basis, "e2", 1);
        let mut remembered = RememberedRejections::default();
        remembered.remember(old.clone());
        assert!(remembered.suppresses(&old));
        assert!(!remembered.suppresses(&changed));
    }

    #[test]
    fn manual_relation_cannot_be_vacuous() {
        let relation = ManualRelation {
            kind: RelationKind::PanelAdd,
            review_reason: "geometry unavailable".into(),
            target_scope: TargetScope::SameTarget { canonical_target_id: "target".into() },
            source_revision_ids: BTreeSet::from(["r1".into()]),
            subject_ids: BTreeSet::from(["s1".into()]),
            membership_ids: BTreeSet::new(),
            edges: Vec::new(),
            lineage: Vec::new(),
            missing_evidence_codes: BTreeSet::from(["footprint_missing".into()]),
        };
        assert_eq!(relation.validate(), Err(RelationInvariantError::RelationFreeProposal));
    }

    fn manual_mosaic_edge(subject_ids: BTreeSet<String>, edge: MosaicEdge) -> ManualRelation {
        ManualRelation {
            kind: RelationKind::MosaicEdge,
            review_reason: "reviewed geometry".into(),
            target_scope: TargetScope::SameTarget { canonical_target_id: "target".into() },
            source_revision_ids: BTreeSet::from(["mosaic-r1".into()]),
            subject_ids,
            membership_ids: BTreeSet::new(),
            edges: vec![edge],
            lineage: Vec::new(),
            missing_evidence_codes: BTreeSet::new(),
        }
    }

    #[test]
    fn manual_mosaic_edge_matches_exact_reviewed_subjects() {
        let reviewed = BTreeSet::from(["a".into(), "b".into()]);
        assert_eq!(manual_mosaic_edge(reviewed.clone(), edge("a", "b")).validate(), Ok(()));
        assert_eq!(
            manual_mosaic_edge(reviewed.clone(), edge("a", "a")).validate(),
            Err(RelationInvariantError::InvalidMosaicEdge)
        );
        assert_eq!(
            manual_mosaic_edge(reviewed, edge("a", "outside")).validate(),
            Err(RelationInvariantError::MosaicEdgeOutsideReviewedSubjects)
        );
    }

    fn populated_manual_relation(kind: RelationKind) -> ManualRelation {
        ManualRelation {
            kind,
            review_reason: "reviewed".into(),
            target_scope: TargetScope::SameTarget { canonical_target_id: "target".into() },
            source_revision_ids: BTreeSet::from(["source".into()]),
            subject_ids: BTreeSet::from(["a".into(), "b".into()]),
            membership_ids: BTreeSet::from(["a".into(), "b".into()]),
            edges: vec![edge("a", "b")],
            lineage: vec![("old".into(), "new".into())],
            missing_evidence_codes: BTreeSet::new(),
        }
    }

    #[test]
    fn every_manual_kind_rejects_its_vacuous_shape() {
        let mut cases = Vec::new();
        for kind in [RelationKind::PanelAdd, RelationKind::PanelReplace] {
            let mut relation = populated_manual_relation(kind);
            relation.membership_ids.clear();
            cases.push(relation);
        }
        for kind in [RelationKind::PanelSplit, RelationKind::PanelMerge] {
            let mut relation = populated_manual_relation(kind);
            relation.lineage.clear();
            cases.push(relation);
        }
        let mut mosaic_create = populated_manual_relation(RelationKind::MosaicCreate);
        mosaic_create.membership_ids = BTreeSet::from(["a".into()]);
        cases.push(mosaic_create);

        let mut mosaic_edge = populated_manual_relation(RelationKind::MosaicEdge);
        mosaic_edge.subject_ids = BTreeSet::from(["a".into()]);
        cases.push(mosaic_edge);

        let mut mosaic_split = populated_manual_relation(RelationKind::MosaicSplit);
        mosaic_split.edges.clear();
        cases.push(mosaic_split);

        let mut mosaic_merge = populated_manual_relation(RelationKind::MosaicMerge);
        mosaic_merge.lineage.clear();
        cases.push(mosaic_merge);

        assert_eq!(cases.len(), 8);
        for relation in cases {
            assert!(relation.validate().is_err(), "accepted invalid {:?}", relation.kind);
        }
    }

    fn proposal_basis() -> ProposalBasis {
        ProposalBasis {
            kind: RelationKind::MosaicMerge,
            target_scope: TargetScope::SameTarget { canonical_target_id: "target".into() },
            source_revision_ids: BTreeSet::from(["source-a".into(), "source-b".into()]),
            subject_ids: BTreeSet::from(["a".into(), "b".into(), "c".into()]),
            membership_ids: BTreeSet::from(["a".into(), "b".into(), "c".into()]),
            edges: vec![edge("a", "b"), edge("b", "c")],
            lineage: vec![("old-a".into(), "new".into()), ("old-b".into(), "new".into())],
        }
    }

    #[test]
    fn canonical_proposal_fingerprint_is_reorder_stable() {
        let basis = proposal_basis();
        let mut reordered = basis.clone();
        reordered.edges.reverse();
        reordered.lineage.reverse();
        assert_eq!(basis.fingerprint("evidence-1", 7), reordered.fingerprint("evidence-1", 7));
    }

    #[test]
    fn canonical_proposal_fingerprint_normalizes_undirected_edges_and_duplicates() {
        let mut forward = proposal_basis();
        forward.edges = vec![identified_edge("edge", "a", "b", "evidence")];

        let mut reversed = forward.clone();
        reversed.edges = vec![
            identified_edge("edge", "b", "a", "evidence"),
            identified_edge("edge", "a", "b", "evidence"),
        ];
        assert_eq!(
            forward.fingerprint("evidence-revision", 7),
            reversed.fingerprint("evidence-revision", 7)
        );
    }

    #[test]
    fn canonical_proposal_fingerprint_keeps_lineage_directed() {
        let forward = proposal_basis();
        let mut reversed = forward.clone();
        reversed.lineage = forward
            .lineage
            .iter()
            .map(|(predecessor, successor)| (successor.clone(), predecessor.clone()))
            .collect();
        assert_ne!(
            forward.fingerprint("evidence-revision", 7),
            reversed.fingerprint("evidence-revision", 7)
        );
    }

    #[test]
    fn canonical_proposal_fingerprint_changes_with_relevant_inputs() {
        let basis = proposal_basis();
        let original = basis.fingerprint("evidence-1", 7);
        assert_ne!(original, basis.fingerprint("evidence-2", 7));
        assert_ne!(original, basis.fingerprint("evidence-1", 8));

        let mut changed_membership = basis;
        changed_membership.membership_ids.insert("d".into());
        assert_ne!(original, changed_membership.fingerprint("evidence-1", 7));
    }

    #[test]
    fn stale_proposal_revision_is_rejected_without_mutation() {
        let fingerprint = proposal_basis().fingerprint("evidence-1", 7);
        let mut proposal = RelationProposal::pending(3, fingerprint.clone());
        assert_eq!(
            proposal.validate_pending(2, &fingerprint),
            Err(RelationInvariantError::StaleProposalRevision)
        );
        assert_eq!(
            proposal.validate_pending(3, "changed"),
            Err(RelationInvariantError::StaleProposalBasis)
        );
        assert_eq!(proposal.mark_stale(2), Err(RelationInvariantError::StaleProposalRevision));
        assert_eq!(proposal.state, ProposalState::Pending);
        assert_eq!(proposal.revision, 3);

        assert_eq!(proposal.mark_stale(3), Ok(()));
        assert_eq!(proposal.state, ProposalState::Stale);
        assert_eq!(proposal.revision, 4);
        assert_eq!(
            proposal.validate_pending(4, &fingerprint),
            Err(RelationInvariantError::ProposalNotPending)
        );
    }

    #[test]
    fn proposal_revision_overflow_fails_without_partial_state_change() {
        let mut proposal = RelationProposal::pending(u64::MAX, "basis");
        assert_eq!(
            proposal.mark_stale(u64::MAX),
            Err(RelationInvariantError::ProposalRevisionExhausted)
        );
        assert_eq!(proposal.state, ProposalState::Pending);
        assert_eq!(proposal.revision, u64::MAX);
    }

    #[test]
    fn point_in_uncaptured_gap_is_excluded() {
        let union = CapturedMosaic::new(&[square(8.0, "left"), square(12.0, "right")])
            .expect("valid disconnected union");
        assert_eq!(union.component_count(), 2);
        assert_eq!(
            union.measure(ObjectShape::Point(coordinate(10.0, 0.0))).expect("coverage measurement"),
            None
        );
        let captured = union
            .measure(ObjectShape::Point(coordinate(8.0, 0.0)))
            .expect("coverage measurement")
            .expect("captured point");
        assert_eq!(captured.state, MosaicObjectCoverage::Full);
        assert_eq!(captured.panel_evidence_ids, BTreeSet::from(["left".into()]));

        let spanning = SkyEllipse::new(
            coordinate(10.0, 0.0),
            Angle::from_degrees(3.0),
            Angle::from_degrees(0.25),
            Angle::from_degrees(90.0),
            128,
        )
        .expect("valid extended object");
        let extended = union
            .measure(ObjectShape::Ellipse(&spanning))
            .expect("coverage measurement")
            .expect("panels intersect extended object");
        assert_eq!(extended.state, MosaicObjectCoverage::Partial);
        assert_eq!(extended.panel_evidence_ids, BTreeSet::from(["left".into(), "right".into()]));
    }

    #[test]
    fn forward_only_lineage_is_acyclic_across_sampled_sizes() {
        for node_count in 1_usize..128 {
            let edges: Vec<(String, String)> = (0..node_count.saturating_sub(1))
                .map(|index| (format!("n{index}"), format!("n{}", index + 1)))
                .collect();
            assert_eq!(validate_acyclic_lineage(&edges), Ok(()));
        }
    }
}
