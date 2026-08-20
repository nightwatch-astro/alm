# Session, Group, Mosaic, and Proposal Contracts

This surface queries immutable session and group history. It also records
reviewed relation decisions without editing accepted revisions.

## DTOs

### `SessionSummary`

```text
SessionSummary {
  sessionId: string,
  materializationOperationId: string,
  materializationKind: "inbox_ingestion" | "metadata_reclassification",
  frameKind: "light" | "dark" | "bias" | "flat",
  observingNight: date,
  acquisitionTimezone?: string,
  nightDerivation: ObservingNightDerivation,
  canonicalTargetId?: string,
  cameraId?: string,
  opticalProfileId?: string,
  frameCount: uint64,
  createdAt: timestamp,
  supersededBySessionCount: uint64,
  activePanelMembership?: {
    panelGroupId: string,
    panelRevisionId: string
  },
  warningCodes: BoundedList<string, 100>
}
```

`supersededBySessionCount` is zero for a current session. Supersession
successors remain available through a cursor-paginated query.

`acquisitionTimezone` is present for `acquisition_timezone` derivation and
absent for `reviewed_local_fallback`. The fallback records the local timestamp
evidence, actor, time, and reason for applying the local civil noon boundary
without inventing a timezone.

### `SessionDetail`

```text
SessionDetail {
  summary: SessionSummary,
  identity: {
    filter: ValueState<string>,
    exposureMs: ValueState<uint64>,
    gain: ValueState<decimal>,
    offset: ValueState<decimal>,
    binningX: ValueState<uint32>,
    binningY: ValueState<uint32>,
    readoutMode: ValueState<string>,
    rasterWidth: uint32,
    rasterHeight: uint32,
    cropEvidence: ValueState<string>,
    geometryEvidenceId?: string
  },
  provenance: {
    sourceGroupId: string,
    acquisitionSiteId?: string,
    approvedAt: timestamp,
    approvedBy: string
  },
  predecessorSessionCount: uint64,
  metadataResolutionRevision: uint64
}

ValueState<T> =
  { state: "known", value: T } |
  { state: "absent" } |
  { state: "unknown" } |
  { state: "contradictory", evidenceRefs: BoundedList<string, 100> }

ObservingNightDerivation =
  { kind: "acquisition_timezone",
    timezone: string,
    localBoundaryTime: "12:00:00" } |
  { kind: "reviewed_local_fallback",
    localBoundaryTime: "12:00:00",
    reviewEvidenceId: string,
    reviewedAt: timestamp,
    reviewedBy: string,
    reason: SafeText }
```

### `PanelGroupRevision`

```text
PanelGroupRevision {
  panelGroupId: string,
  revisionId: string,
  revisionNumber: uint64,
  parentRevisionId?: string,
  acceptedHead: boolean,
  retired: boolean,
  canonicalTargetId?: string,
  crossTargetAssociationId?: string,
  sessionCount: uint32,
  representativeSessionId: string,
  representativeEvidenceId: string,
  matchingSettingsRevision: uint64,
  acceptedAt: timestamp,
  acceptedBy: string,
  decisionReason?: string,
  predecessorGroupCount: uint64,
  successorGroupCount: uint64
}
```

`manualRelation` is present if and only if `kind` is `manual_relation`. The
stored value contains the same relation kind, target scope, review reason, and
missing-evidence codes accepted by the creation command.

The paginated membership is an exact immutable snapshot. A current revision
may not contain both a predecessor session and its replacement.

### `MosaicRevision`

```text
MosaicRevision {
  mosaicId: string,
  revisionId: string,
  revisionNumber: uint64,
  parentRevisionId?: string,
  acceptedHead: boolean,
  retired: boolean,
  intendedTargetId?: string,
  crossTargetAssociationId?: string,
  panelCount: uint32,
  edgeCount: uint32,
  capturedUnionEvidenceId: string,
  matchingSettingsRevision: uint64,
  acceptedAt: timestamp,
  acceptedBy: string,
  decisionReason?: string,
  predecessorMosaicCount: uint64,
  successorMosaicCount: uint64
}

MosaicEdge {
  edgeId: string,
  leftPanelRevisionId: string,
  rightPanelRevisionId: string,
  overlapPercent: decimal,
  residualSkyRotationDeg: decimal,
  allowedResidualRotationRangesDeg: BoundedList<Range<decimal>, 16>,
  parityMatch: boolean,
  acquisitionGeometryCompatible: boolean,
  evidenceId: string,
  stale: boolean,
  invalidationReasonCode?: string,
  appliedReclassificationPlanRevisionId?: string
}
```

`invalidationReasonCode` and `appliedReclassificationPlanRevisionId` are
present exactly when `stale` is true. They identify the immutable invalidation
record. Historical edge evidence remains queryable.

### `RelationProposal`

```text
RelationProposal {
  proposalId: string,
  proposalRevision: uint64,
  kind: "panel_add" | "panel_replace" | "panel_split" |
        "panel_merge" | "mosaic_create" | "mosaic_edge" |
        "mosaic_split" | "mosaic_merge" | "manual_relation",
  state: "pending" | "accepted" | "rejected" | "superseded" | "stale",
  sourceRevisionCount: uint64,
  subjectCount: uint64,
  proposedMembershipCount: uint64,
  proposedEdgeCount: uint64,
  proposedLineageCount: uint64,
  evidence: RelationEvidence,
  matchingSettingsRevision: uint64,
  basisFingerprint: string,
  createdAt: timestamp,
  createdBy: string,
  manualRelation?: ManualRelationReview,
  decision?: ProposalDecision,
  supersededByProposalId?: string
}

RelationEvidence {
  evidenceId: string,
  targetCompatibility: "same_target" | "reviewed_cross_target" | "incompatible",
  footprintCoveragePercent?: decimal,
  centerSeparationPercent?: decimal,
  residualSkyRotationDeg?: decimal,
  allowedResidualRotationRangesDeg: BoundedList<Range<decimal>, 16>,
  parity: "match" | "mismatch" | "unknown",
  acquisitionGeometry: "compatible" | "incompatible" | "unknown",
  equipment: "compatible" | "incompatible" | "unknown",
  missingEvidenceCodes: BoundedList<string, 100>,
  thresholdSnapshot: BoundedList<ThresholdMeasurement, 100>
}

ThresholdMeasurement {
  key: string,
  measuredValue: decimal,
  unit: string,
  comparison: "lt" | "lte" | "eq" | "gte" | "gt",
  thresholdValue: decimal,
  outcome: "pass" | "fail"
}

MosaicObjectEvidenceItem {
  canonicalObjectId: string,
  panelContainmentRefs: BoundedList<EntityRef, 100>,
  sessionContainmentRefs: BoundedList<EntityRef, 100>,
  coverageState: "full" | "partial"
}

ProposalDecision {
  decision: "accepted" | "rejected" | "corrected",
  decidedAt: timestamp,
  reason: SafeText,
  auditId: string
}

ManualRelationReview {
  relationKind: "panel_add" | "panel_replace" | "panel_split" |
                "panel_merge" | "mosaic_create" | "mosaic_edge" |
                "mosaic_split" | "mosaic_merge",
  reviewReason: SafeText,
  targetScope:
    { kind: "same_target", canonicalTargetId: string } |
    { kind: "existing_cross_target", crossTargetAssociationId: string } |
    { kind: "new_reviewed_cross_target",
      canonicalTargetIds: BoundedList<string, 500>,
      purpose: SafeText },
  missingEvidenceCodes: BoundedList<string, 100>
}

RelationDecisionCounts {
  acceptedRevisionCount: uint64,
  retiredGroupCount: uint64,
  sessionSupersessionCount: uint64,
  panelLineageCount: uint64,
  mosaicLineageCount: uint64
}

TraversalPreviewProgress {
  operationId: string,
  readWatermark: uint64,
  state: "queued" | "running" | "completed" | "cancelled" |
         "ceiling_exceeded" | "failed",
  visitedNodeCount: uint64,
  visitedEdgeCount: uint64,
  frontierCount: uint64,
  deepestLevel: uint32,
  updatedAt: timestamp,
  terminalError?: ContractError
}
```

`ProposalDecision` exposes the derived actor through the enclosing proposal's
audit reference. Caller input never supplies an actor identity.

An automatic proposal requires complete measured evidence and the threshold
snapshot that produced it. A manual relation lists every missing evidence code.
The `new_reviewed_cross_target` scope is proposal evidence, not an association
identity. Acceptance creates its association and first accepted relation in the
same transaction.

## Queries

### List ordering and cursor snapshots

Every list operation captures a committed `readWatermark` on its first page.
The cursor binds that watermark, the normalized filters, the declared sort,
and the last complete sort key. Later pages reconstruct accepted state at the
same watermark. They exclude later commits and retain rows that were visible
before a later retirement, supersession, or proposal decision. A missing,
expired, or mismatched snapshot returns `pagination.snapshot_unavailable` or
`pagination.cursor_invalid`; the server never resumes against a newer state.

The final identity columns below make each order total. The cursor stores the
complete tuple.

| Operation | Unique total order |
|---|---|
| `session.list` | `createdAt DESC, sessionId ASC` |
| `session.frame.list` | membership `ordinal ASC, frameId ASC` |
| `session.supersession_successor.list` | `ordinal ASC, successorSessionId ASC` |
| `session.supersession_predecessor.list` | `ordinal ASC, predecessorSessionId ASC` |
| `panel_group.membership.list` | `ordinal ASC, sessionId ASC` |
| `panel_group.history.list` | `revisionNumber DESC, revisionId ASC` |
| `panel_group.lineage_predecessor.list` | `acceptedAt DESC, acceptedProposalId ASC, ordinal ASC, predecessorGroupId ASC` |
| `panel_group.lineage_successor.list` | `acceptedAt DESC, acceptedProposalId ASC, ordinal ASC, successorGroupId ASC` |
| `panel_group.list` | `acceptedAt DESC, panelGroupId ASC` |
| `mosaic.panel.list` | `ordinal ASC, panelRevisionId ASC, panelGroupId ASC` |
| `mosaic.edge.list` | `ordinal ASC, edgeId ASC` |
| `mosaic.history.list` | `revisionNumber DESC, revisionId ASC` |
| `mosaic.lineage_predecessor.list` | `acceptedAt DESC, acceptedProposalId ASC, ordinal ASC, predecessorMosaicId ASC` |
| `mosaic.lineage_successor.list` | `acceptedAt DESC, acceptedProposalId ASC, ordinal ASC, successorMosaicId ASC` |
| `mosaic.object_evidence.list` | `canonicalObjectId ASC` |
| `relation_proposal.list` | `createdAt DESC, proposalId ASC` |
| `relation_proposal.source_revision.list` | `ordinal ASC, entityType ASC, entityId ASC, revisionId ASC` |
| `relation_proposal.subject.list` | `ordinal ASC, entityType ASC, entityId ASC` |
| `relation_proposal.membership.list` | `ordinal ASC, entityType ASC, entityId ASC` |
| `relation_proposal.edge.list` | `ordinal ASC, edgeId ASC` |
| `relation_proposal.lineage.list` | `ordinal ASC, predecessorGroupId ASC, successorGroupId ASC` |
| `relation_proposal.decision_revision.list` | `ordinal ASC, entityType ASC, entityId ASC, revisionId ASC` |
| `relation_proposal.decision_retired_group.list` | `ordinal ASC, groupId ASC` |
| `relation_proposal.decision_session_supersession.list` | `ordinal ASC, predecessorSessionId ASC, successorSessionId ASC` |
| `relation_proposal.decision_group_lineage.list` | `ordinal ASC, predecessorGroupId ASC, successorGroupId ASC` |
| `relation_traversal_preview.node.list` | `ordinal ASC, nodeRef.entityType ASC, nodeRef.entityId ASC` |
| `relation_traversal_preview.edge.list` | `ordinal ASC, edgeRef.entityType ASC, edgeRef.entityId ASC` |

### `session.query`

- Type: read-only.
- Request: `{ sessionId }`.
- Response: `SessionDetail`.
- Errors: `session.not_found`.

### `session.list`

- Type: read-only.
- Request: `{ targetId?, frameKind?, observingNightFrom?, observingNightTo?, cameraId?, opticalProfileId?, superseded?: "exclude" | "include" | "only", panelGroupId?, page }`.
- Response: `Page<SessionSummary>`.
- Sort: `createdAt` descending, then `sessionId` ascending.

### `session.frame.list`

- Type: read-only.
- Request: `{ sessionId, page }`.
- Response: `Page<{ frameId: string }>`.
- Sort: immutable session membership order, then `frameId` ascending.
- Errors: `session.not_found`.

### `session.supersession_successor.list`

- Type: read-only.
- Request: `{ predecessorSessionId, page }`.
- Response: `Page<{ successorSessionId: string, ordinal: uint64, appliedReclassificationPlanRevisionId: string }>`.
- Sort: `ordinal` ascending, then `successorSessionId` ascending.
- Errors: `session.not_found`.

### `session.supersession_predecessor.list`

- Type: read-only.
- Request: `{ successorSessionId, page }`.
- Response: `Page<{ predecessorSessionId: string, ordinal: uint64, appliedReclassificationPlanRevisionId: string }>`.
- Sort: `ordinal` ascending, then `predecessorSessionId` ascending.
- Errors: `session.not_found`.

### `panel_group.query`

- Type: read-only.
- Request: `{ panelGroupId, revisionId?: string }`.
- Response: `{ acceptedHead: PanelGroupRevision, requestedRevision?: PanelGroupRevision }`.
- Errors: `panel_group.not_found`, `panel_group.revision_not_found`.

### `panel_group.membership.list`

- Type: read-only.
- Request: `{ panelGroupId, revisionId, page }`.
- Response: `Page<{ sessionId: string, ordinal: uint32 }>`.
- Sort: `ordinal` ascending, then `sessionId` ascending.

### `panel_group.history.list`

- Type: read-only.
- Request: `{ panelGroupId, page }`.
- Response: `Page<PanelGroupRevision>`.
- Sort: `revisionNumber` descending.

### `panel_group.lineage_predecessor.list`

- Type: read-only.
- Request: `{ panelGroupId, page }`.
- Response: `Page<{ predecessorGroupId: string, ordinal: uint64, acceptedProposalId: string }>`.
- Sort: `acceptedAt` descending, then `acceptedProposalId` ascending, then
  `ordinal` ascending, then `predecessorGroupId` ascending.

### `panel_group.lineage_successor.list`

- Type: read-only.
- Request: `{ panelGroupId, page }`.
- Response: `Page<{ successorGroupId: string, ordinal: uint64, acceptedProposalId: string }>`.
- Sort: `acceptedAt` descending, then `acceptedProposalId` ascending, then
  `ordinal` ascending, then `successorGroupId` ascending.

### `panel_group.list`

- Type: read-only.
- Request: `{ targetId?, sessionId?, activeOnly?: boolean = true, page }`.
- Response: `Page<PanelGroupRevision>` containing accepted heads.

### `mosaic.query`

- Type: read-only.
- Request: `{ mosaicId, revisionId?: string }`.
- Response: `{ acceptedHead: MosaicRevision, requestedRevision?: MosaicRevision }`.
- Errors: `mosaic.not_found`, `mosaic.revision_not_found`.

### `mosaic.panel.list`

- Type: read-only.
- Request: `{ mosaicId, revisionId, page }`.
- Response: `Page<{ panelGroupId: string, panelRevisionId: string, ordinal: uint32 }>`.

### `mosaic.edge.list`

- Type: read-only.
- Request: `{ mosaicId, revisionId, page }`.
- Response: `Page<{ edge: MosaicEdge, ordinal: uint32 }>`.

### `mosaic.history.list`

- Type: read-only.
- Request: `{ mosaicId, page }`.
- Response: `Page<MosaicRevision>`.
- Sort: `revisionNumber` descending.

### `mosaic.lineage_predecessor.list`

- Type: read-only.
- Request: `{ mosaicId, page }`.
- Response: `Page<{ predecessorMosaicId: string, ordinal: uint64, acceptedProposalId: string }>`.
- Sort: `acceptedAt` descending, then `acceptedProposalId` ascending, then
  `ordinal` ascending, then `predecessorMosaicId` ascending.

### `mosaic.lineage_successor.list`

- Type: read-only.
- Request: `{ mosaicId, page }`.
- Response: `Page<{ successorMosaicId: string, ordinal: uint64, acceptedProposalId: string }>`.
- Sort: `acceptedAt` descending, then `acceptedProposalId` ascending, then
  `ordinal` ascending, then `successorMosaicId` ascending.

### `mosaic.object_evidence.list`

- Type: read-only.
- Request: `{ mosaicId, revisionId, page }`.
- Response: `Page<MosaicObjectEvidenceItem>`.
- Notes: each item contains one canonical object ID, bounded per-panel and
  per-session containment summaries of at most 100 references each, and
  `coverageState: "full" | "partial"`.
- Notes: results exclude point-like objects outside the captured union and
  extended objects with zero intersection.

### `relation_evidence.query`

- Type: read-only.
- Request: `{ evidenceId }`.
- Response: `RelationEvidence`.
- Errors: `relation_evidence.not_found`.
- `PanelGroupRevision.representativeEvidenceId` and
  `MosaicRevision.capturedUnionEvidenceId` are the only handles a revision gives to
  the reasoning behind it, and neither resolves through `relation_proposal.query`:
  an ingestion-created singleton panel group has no proposal at all, and an
  accepted revision references an exclusively held envelope of its own rather than
  the proposal's. Without this operation a client can show a revision but not why
  it holds.

### `relation_proposal.list`

- Type: read-only.
- Request: `{ state?, kind?, targetId?, subjectRef?, page }`.
- Response: `Page<RelationProposal>`.

### `relation_proposal.query`

- Type: read-only.
- Request: `{ proposalId }`.
- Response: `RelationProposal`.
- Errors: `relation_proposal.not_found`.

### `relation_proposal.source_revision.list`

- Type: read-only.
- Request: `{ proposalId, page }`.
- Response: `Page<{ revisionRef: RevisionRef, ordinal: uint64 }>`.
- Sort: `ordinal` ascending.

### `relation_proposal.subject.list`

- Type: read-only.
- Request: `{ proposalId, page }`.
- Response: `Page<{ subjectRef: EntityRef, ordinal: uint64 }>`.
- Sort: `ordinal` ascending.

### `relation_proposal.membership.list`

- Type: read-only.
- Request: `{ proposalId, page }`.
- Response: `Page<{ membershipRef: EntityRef, ordinal: uint64 }>`.
- Sort: `ordinal` ascending.

### `relation_proposal.edge.list`

- Type: read-only.
- Request: `{ proposalId, page }`.
- Response: `Page<{ edge: MosaicEdge, ordinal: uint64 }>`.
- Sort: `ordinal` ascending.

### `relation_proposal.lineage.list`

- Type: read-only.
- Request: `{ proposalId, page }`.
- Response: `Page<{ predecessorGroupId: string, successorGroupId: string, ordinal: uint64 }>`.
- Sort: `ordinal` ascending.

### `relation_proposal.decision_revision.list`

- Type: read-only.
- Request: `{ proposalId, decisionSnapshotId, page }`.
- Response: `Page<{ revisionRef: RevisionRef, ordinal: uint64 }>`.
- Sort: `ordinal` ascending.

### `relation_proposal.decision_retired_group.list`

- Type: read-only.
- Request: `{ proposalId, decisionSnapshotId, page }`.
- Response: `Page<{ groupId: string, ordinal: uint64 }>`.
- Sort: `ordinal` ascending.

### `relation_proposal.decision_session_supersession.list`

- Type: read-only.
- Request: `{ proposalId, decisionSnapshotId, page }`.
- Response: `Page<{ predecessorSessionId: string, successorSessionId: string, ordinal: uint64 }>`.
- Sort: `ordinal` ascending.

### `relation_proposal.decision_group_lineage.list`

- Type: read-only.
- Request: `{ proposalId, decisionSnapshotId, groupType: "panel" | "mosaic", page }`.
- Response: `Page<{ predecessorGroupId: string, successorGroupId: string, ordinal: uint64 }>`.
- Sort: `ordinal` ascending.

Proposal collection queries traverse the immutable proposal snapshot.
Their cursors support proposals whose collections exceed command batch limits.

### `relation_traversal_preview.start`

- Type: asynchronous read-only query.
- Request: `{ startRefs: BoundedList<EntityRef, 500>, graph: "panel_lineage" | "mosaic_lineage" | "accepted_mosaic_connectivity", direction: "predecessors" | "successors" | "both", limits?: { maxDepth: uint32 = 64, maxNodes: uint64 = 10000, maxEdges: uint64 = 50000 } }`.
- Response: `TraversalPreviewProgress`.
- Guard: `maxDepth` is 1-4,096, `maxNodes` is 1-100,000, and `maxEdges` is 1-2,000,000.
- Effect: captures a `readWatermark` and starts an ephemeral traversal against that immutable snapshot.
- Effect: writes no domain row, SQLite row, audit record, or outbox event.

### `relation_traversal_preview.progress.query`

- Type: read-only.
- Request: `{ operationId }`.
- Response: `TraversalPreviewProgress`.
- Cadence: a running traversal checks cancellation after at most 256 expanded edges and at least every 100 milliseconds.
- Cadence: it publishes an in-memory progress snapshot at least every 500 milliseconds and no more than 10 times per second. A terminal transition publishes immediately.

### `relation_traversal_preview.cancel`

- Type: ephemeral execution control; no database mutation.
- Request: `{ operationId }`.
- Response: `TraversalPreviewProgress`.
- Effect: records cancellation only in the in-memory operation and returns the latest progress.
- Deadline: the operation reaches `cancelled` within one second after receipt unless it reached another terminal state first.
- Effect: writes no domain row, SQLite row, audit record, or outbox event.

### `relation_traversal_preview.result.query`

- Type: read-only.
- Request: `{ operationId }`.
- Response: `{ operationId, readWatermark, state: "completed", nodeCount: uint64, edgeCount: uint64, deepestLevel: uint32 }`.
- Guard: the operation must be `completed`.

### `relation_traversal_preview.node.list`

- Type: read-only.
- Request: `{ operationId, page }`.
- Response: `Page<{ nodeRef: EntityRef, depth: uint32, ordinal: uint64 }>`.
- Guard: the operation must be `completed`.

### `relation_traversal_preview.edge.list`

- Type: read-only.
- Request: `{ operationId, page }`.
- Response: `Page<{ edgeRef: EntityRef, fromRef: EntityRef, toRef: EntityRef, ordinal: uint64 }>`.
- Guard: the operation must be `completed`.

Traversal result cursors bind the operation's `readWatermark`. Cancelling,
expiring, or losing the ephemeral operation makes all result cursors
unavailable rather than replaying the traversal against another snapshot.

## Commands

### `relation_proposal.manual.create`

- Type: atomic database mutation.
- Request: `{ relationKind, sourceRevisionRefs: BoundedList<RevisionRef, 500>, subjectRefs: BoundedList<EntityRef, 500>, proposedMembershipRefs?: BoundedList<EntityRef, 500>, proposedEdges?: BoundedList<MosaicEdge, 500>, proposedLineage?: BoundedList<{ predecessorGroupId: string, successorGroupId: string }, 500>, targetScope: ManualRelationReview.targetScope, evidence: RelationEvidence, expectedMatchingSettingsRevision: uint64, reviewReason: SafeText, mutationContext }`.
- Response: `{ proposal: RelationProposal, auditId }`.
- Guard: `reviewReason` must contain non-whitespace text.
- Guard: `expectedMatchingSettingsRevision` must equal the current matching-settings revision, and is the same `uint64` a caller reads from `MatchingSettings.revision`. The submitted `evidence` was measured under that revision, and both the proposal and the envelope are stamped with it, so a revision change between measurement and submission rejects with a stale-revision error instead of storing thresholds the caller never saw.
- Guard: `evidence.missingEvidenceCodes` must enumerate every required geometry or orientation measurement unavailable to the proposal.
- Guard: a `new_reviewed_cross_target` scope must contain at least two distinct canonical target IDs.
- Guard: every source revision, subject, proposed member, edge endpoint, and lineage endpoint must exist.
- Guard: source revisions and subjects are non-empty. At least one of proposed
  membership, edges, or lineage is non-empty, so no proposal can create a
  relation-free association.
- Guard: every subject and output belongs to the declared same-target,
  existing-association, or proposed cross-target scope.
- Guard: `panel_add` and `panel_replace` name one source panel revision and at
  least one proposed session membership; `panel_split` and `panel_merge` name
  the affected source revisions, non-empty destination memberships, and their
  lineage; `mosaic_create` names at least two panel revisions and a connected
  edge set; `mosaic_edge` names exactly two endpoint panel revisions and one
  edge; `mosaic_split` and `mosaic_merge` name the affected mosaic revisions,
  non-empty destination memberships and edges, and their lineage.
- Effect: creates a pending `manual_relation` proposal with the requested `relationKind`, review rationale, target scope, supplied evidence, and explicit missing-evidence disclosure.
- Effect: does not create or reserve a cross-target association.
- Effect: bypasses remembered automatic-proposal suppression and writes an audit entry.
- Idempotency: the shared `commandId` rule applies.

### `relation_proposal.accept`

- Type: atomic database mutation.
- Request: `{ proposalId, expectedProposalRevision, expectedSourceRevisionSetDigest: Digest, mutationContext }`.
- Response: `{ proposal: RelationProposal, decisionSnapshotId, resultCounts: RelationDecisionCounts, crossTargetAssociationId?: string, auditId }`.
- Guard: the proposal state must be `pending`.
- Guard: every source revision must still be the accepted head.
- Guard: every referenced session must exist and retain the evidence revision used by the proposal.
- Guard: panel membership uniqueness and predecessor/replacement exclusion must hold.
- Guard: lineage must remain acyclic.
- Guard: a bridge between accepted mosaics must use a `mosaic_merge` proposal.
- Guard: automatic cross-target relations require an existing accepted `crossTargetAssociationId`.
- Guard: a manual `new_reviewed_cross_target` scope must still name at least two existing distinct canonical targets.
- Effect: acceptance creates all revisions, edges, lineage links, and retirements in one transaction.
- Effect: acceptance of `new_reviewed_cross_target` creates the reviewed association, its target rows, and the first accepted relation in that same transaction. The response returns the created association ID.
- Effect: any failure creates neither the association nor a relation revision.
- Idempotency: the shared `commandId` rule applies.

### `relation_proposal.reject`

- Type: database mutation.
- Request: `{ proposalId, expectedProposalRevision, rejectionReason: SafeText, mutationContext }`.
- Response: `{ proposal: RelationProposal, suppressionFingerprint: string, auditId }`.
- Guard: `rejectionReason` must contain non-whitespace text.
- Effect: equivalent automatic proposals remain suppressed while the basis fingerprint, evidence revision, and matching-settings revision remain unchanged.
- Effect: explicit manual proposal creation remains available.

### `relation_proposal.correct`

- Type: database mutation that creates a replacement proposal.
- Request: `{ proposalId, expectedProposalRevision, correction: { membershipRefs?: BoundedList<EntityRef, 500>, edgeOverrides?: BoundedList<MosaicEdge, 500>, intendedTargetId?, targetScope?: ManualRelationReview.targetScope, note: SafeText }, mutationContext }`.
- Response: `{ supersededProposal: RelationProposal, correctedProposal: RelationProposal, auditId }`.
- Guard: the source proposal must be `pending`.
- Guard: corrected membership and edge references must exist.
- Guard: cross-target membership requires an existing association scope or a reviewed new-association scope.
- Effect: the original proposal becomes `superseded` and names the replacement
  in `supersededByProposalId`.
- Effect: the corrected proposal remains `pending` until a separate accept or reject command.
- Effect: measured evidence is retained; user changes are stored as review overrides.

## Events

| Event | Payload |
|---|---|
| `session.materialized` | `{ sessionId, materializationOperationId, materializationKind, frameKind, frameCount: uint64, panelGroupId?, panelRevisionId? }`; panel fields are both present for light and both absent for calibration |
| `session.superseded` | `{ predecessorSessionId, replacementSessionCount: uint64, appliedReclassificationPlanRevisionId }` |
| `relation_proposal.created` | `{ proposalId, kind, subjectCount, basisFingerprint, manualRelationKind?, missingEvidenceCodeCount?: uint64 }` |
| `relation_proposal.accepted` | `{ proposalId, decisionSnapshotId, resultCounts: RelationDecisionCounts, crossTargetAssociationId? }` |
| `cross_target_association.created` | `{ crossTargetAssociationId, acceptedProposalId, canonicalTargetCount: uint64 }` |
| `relation_proposal.rejected` | `{ proposalId, suppressionFingerprint, rejectionReason }` |
| `relation_proposal.corrected` | `{ proposalId, correctedProposalId, correctionNote }` |
| `group.head_changed` | `{ groupType, groupId, previousRevisionId, acceptedRevisionId }` |

## Error codes

| Code | Condition | Required details |
|---|---|---|
| `session.not_found` | A session ID is unknown. | `sessionId` |
| `panel_group.not_found` | A panel group ID is unknown. | `panelGroupId` |
| `panel_group.revision_not_found` | A panel revision ID is unknown for the group. | `panelGroupId`, `revisionId` |
| `mosaic.not_found` | A mosaic ID is unknown. | `mosaicId` |
| `mosaic.revision_not_found` | A mosaic revision ID is unknown for the mosaic. | `mosaicId`, `revisionId` |
| `relation_proposal.not_found` | A proposal ID is unknown. | `proposalId` |
| `relation_proposal.not_pending` | A decision already exists. | `proposalId`, `state` |
| `relation_proposal.stale` | Evidence, a source head, or a subject revision changed. | `proposalId`, `staleRefCount`, `staleRefs: BoundedList<RevisionRef, 100>`, `truncated` |
| `relation_proposal.invalid_membership` | Membership violates target, uniqueness, or supersession rules. | `proposalId`, `violations: BoundedList<Violation, 100>` |
| `relation_proposal.lineage_cycle` | Proposed lineage would contain a cycle. | `proposalId`, `groupCount`, `groupIds: BoundedList<string, 100>`, `truncated` |
| `relation_proposal.merge_required` | A bridge would join accepted mosaic components. | `proposalId`, `mosaicCount`, `mosaicIds: BoundedList<string, 100>`, `truncated` |
| `relation_proposal.cross_target_review_required` | A durable cross-target relation lacks an accepted association. | `proposalId`, `targetCount`, `targetIds: BoundedList<string, 100>`, `truncated` |
| `relation_proposal.evidence_missing` | Automatic acceptance lacks required geometry or orientation evidence. | `proposalId`, `missingEvidenceCodes: BoundedList<string, 100>` |
| `relation_proposal.manual_evidence_disclosure_incomplete` | A manual proposal omits a required missing-evidence code. | `missingEvidenceCodes: BoundedList<string, 100>` |
| `pagination.snapshot_unavailable` | A cursor's immutable read watermark is unavailable. | None |
| `traversal.operation_not_found` | A preview operation is unknown, expired, or lost with its process. | `operationId` |
| `traversal.result_not_ready` | Result rows were requested before completion. | `operationId`, `state` |
| `traversal.node_ceiling_exceeded` | Traversal reached its node ceiling. | `operationId`, `maxNodes`, `visitedNodeCount` |
| `traversal.edge_ceiling_exceeded` | Traversal reached its edge ceiling. | `operationId`, `maxEdges`, `visitedEdgeCount` |
| `traversal.depth_ceiling_exceeded` | Traversal found an expansion beyond its depth ceiling. | `operationId`, `maxDepth`, `deepestLevel` |
| `traversal.cancellation_deadline_exceeded` | A worker failed to stop within one second of cancellation receipt. | `operationId`, `cancelRequestedAt` |

## Audit expectations

- Accept records the decision snapshot ID and result counts. The snapshot's
  paginated reference rows preserve every source, supersession, lineage, and
  retired-group result without inline truncation.
- Manual creation records the relation kind, target scope, review reason,
  supplied evidence, and missing-evidence codes.
- First acceptance of a reviewed cross-target scope records the association ID
  in the decision snapshot. Its association, target rows, and first relation
  share the acceptance transaction and outbox publish.
- Reject records the rejection reason and suppression fingerprint.
- Correct records the original proposal, replacement proposal, and each override.
- Atomic acceptance failure records one refused audit entry and no success event.
- Query operations do not write audit entries.

## Contract verification

- An end-to-end test creates a manual relation with missing footprint and
  orientation evidence, then verifies those codes in the pending proposal.
- The same test accepts a `new_reviewed_cross_target` scope and observes one
  transaction containing the association, its targets, and the first relation.
  A forced stale-head failure leaves all three absent.
- A pagination test creates more than 500 supersession and lineage results.
  It follows every cursor without loss or duplication while concurrent commits
  remain outside the captured watermark.
- A traversal test covers each typed ceiling and confirms cancellation reaches
  a terminal state within one second without database, audit, or outbox writes.
