// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

use std::io::Write;
use std::process::{Command, Stdio};

use contracts_core::sessions::heterogeneity::calibration::{
    AutomaticEligibility, CalibrationCandidateEvidence, CalibrationEvent, CalibrationHandoffFrame,
    CalibrationKind, CalibrationListOperation, CalibrationQuery, HandoffOperationQueryRequest,
    IndexedReadableState,
};
use contracts_core::sessions::heterogeneity::inbox::{
    InboxEvent, InboxListOperation, InboxQuery, OperationQueryRequest,
};
use contracts_core::sessions::heterogeneity::metadata::{
    MetadataEvent, MetadataEvidenceQueryRequest, MetadataListOperation, MetadataQuery,
};
use contracts_core::sessions::heterogeneity::projects::{
    ProjectEvent, ProjectListOperation, ProjectQuery, ProjectQueryRequest,
    RelatedSessionListRequest,
};
use contracts_core::sessions::heterogeneity::relations::{
    EvidenceTernary, ManualRelationCreateRequest, ManualRelationKind, ManualTargetScope,
    PanelListRequest, ParityEvidence, RelationEvent, RelationEvidence, RelationListOperation,
    RelationQuery, SessionQueryRequest, TargetCompatibility, TraversalStartRequest,
};
use contracts_core::sessions::heterogeneity::settings::{
    CalibrationAgePolicy, DarkThermalThresholds, FixedMatchingRules, FlatAgeThresholds,
    FlatOrientationThresholds, GeometryThresholds, MatchingSettings, MatchingSettingsEvent,
    MatchingSettingsGetRequest, MatchingSettingsQuery, MatchingSettingsUpdatedEvent,
    MosaicThresholds, SettingsSeverity,
};
use contracts_core::sessions::heterogeneity::shared::{
    BoundedList, CanonicalId, CanonicalRelativePath, CanonicalUuid, CommandFence, ContractRange,
    Cursor, EntityRef, ErrorCode, FiniteDecimal, IdempotencyOutcome, KeysetListOperation,
    MutationContext, NamedSafeValue, NonBlankSafeText, Page, PageBasis, PageRequest,
    PortableContractError, ProtectedResourceState, RevisionRef, Rfc3339Timestamp, SafeErrorDetails,
    SafeScalar, SafeText, SupportedFrameKind, MAX_CURSOR_BYTES, MAX_REQUEST_BYTES,
    MAX_RESPONSE_BYTES,
};
use schemars::schema_for;
use serde_json::{json, Value};

const ID: &str = "018f22b2-7f7f-7f7f-8f7f-7f7f7f7f7f7f";
const OTHER_ID: &str = "018f22b2-7f7f-7f7f-8f7f-7f7f7f7f7f80";
const DIGEST: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

fn id(value: &str) -> CanonicalId {
    CanonicalId::try_new(value).expect("fixture id is canonical")
}

fn uuid(value: &str) -> CanonicalUuid {
    CanonicalUuid::try_new(value).expect("fixture UUID is canonical")
}

fn safe(value: &str) -> SafeText {
    SafeText::try_new(value).expect("fixture text is safe")
}

fn non_blank(value: &str) -> NonBlankSafeText {
    NonBlankSafeText::try_new(value).expect("fixture text is non-blank")
}

fn decimal(value: f64) -> FiniteDecimal {
    FiniteDecimal::try_new(value).expect("fixture decimal is finite")
}

fn append_registry<T, const N: usize>(
    operations: [T; N],
    entries: &mut Vec<(&'static str, &'static [&'static str])>,
) where
    T: Copy + KeysetListOperation,
{
    for operation in operations {
        entries.push((operation.query_name(), operation.unique_order()));
    }
}

#[test]
fn snapshot_and_watermark_pages_round_trip_but_ambiguous_pages_are_rejected() {
    let page = Page::try_new(
        BoundedList::try_new(vec![safe("accepted")]).unwrap(),
        PageBasis::Snapshot { snapshot_id: id(ID) },
        Some(Cursor::try_new("next-page").unwrap()),
    )
    .unwrap();

    let encoded = serde_json::to_value(&page).unwrap();
    assert_eq!(encoded["snapshotId"], ID);
    assert!(encoded.get("watermark").is_none());
    assert_eq!(serde_json::from_value::<Page<SafeText>>(encoded).unwrap(), page);

    let ambiguous = json!({
        "items": [],
        "snapshotId": ID,
        "watermark": "42"
    });
    assert!(serde_json::from_value::<Page<SafeText>>(ambiguous).is_err());
    assert!(serde_json::from_value::<Page<SafeText>>(json!({ "items": [] })).is_err());

    let watermark = Page::<SafeText>::try_new(
        BoundedList::default(),
        PageBasis::Watermark { watermark: safe("43") },
        None,
    )
    .unwrap();
    assert_eq!(serde_json::to_value(watermark).unwrap()["watermark"], "43");
}

#[test]
fn portable_bounds_are_enforced_during_construction_and_decode() {
    assert!(BoundedList::<u8, 2>::try_new(vec![1, 2]).is_ok());
    assert!(BoundedList::<u8, 2>::try_new(vec![1, 2, 3]).is_err());
    assert!(serde_json::from_value::<BoundedList<u8, 2>>(json!([1, 2, 3])).is_err());
    assert!(PageRequest::try_new(None, 0).is_err());
    assert!(PageRequest::try_new(None, 501).is_err());
    assert_eq!(serde_json::from_value::<PageRequest>(json!({})).unwrap().limit, 100);
    assert!(serde_json::from_value::<PageRequest>(json!({ "limit": 501 })).is_err());
    assert!(Cursor::try_new("x".repeat(MAX_CURSOR_BYTES + 1)).is_err());
    assert!(SafeText::try_new("contains\ncontrol").is_err());
    assert!(CanonicalRelativePath::try_new("../outside.fit").is_err());
    assert!(CanonicalId::try_new("550e8400-e29b-41d4-a716-446655440000").is_err());
    assert!(CanonicalUuid::try_new("550e8400-e29b-41d4-a716-446655440000").is_ok());
    assert_eq!(MAX_REQUEST_BYTES, 1_048_576);
    assert_eq!(MAX_RESPONSE_BYTES, 4_194_304);
}

#[test]
fn runtime_bounds_and_defaults_are_present_in_json_schema() {
    let bounded = serde_json::to_value(schema_for!(BoundedList<u8, 2>)).unwrap();
    assert_eq!(bounded["maxItems"], 2);

    let page = serde_json::to_value(schema_for!(PageRequest)).unwrap();
    assert_eq!(page["properties"]["limit"]["default"], 100);
    assert_eq!(page["properties"]["limit"]["minimum"], 1);
    assert_eq!(page["properties"]["limit"]["maximum"], 500);
    assert!(!page["required"]
        .as_array()
        .is_some_and(|required| { required.iter().any(|field| field == "limit") }));

    let entity_id = serde_json::to_value(schema_for!(CanonicalId)).unwrap();
    assert!(entity_id["pattern"].as_str().unwrap().contains("-7"));

    let relative_path = serde_json::to_value(schema_for!(CanonicalRelativePath)).unwrap();
    assert_eq!(relative_path["x-maxUtf8Bytes"], 4096);
    assert_eq!(relative_path["x-maxSegmentUtf8Bytes"], 255);
    let path_pattern = relative_path["pattern"].as_str().unwrap();
    for structural_rule in ["(?!/)", "(?![A-Za-z]:)", "(?!.*\\\\)", "\\.\\.?", "{0,63}"] {
        assert!(path_pattern.contains(structural_rule), "missing schema rule {structural_rule}");
    }
}

#[test]
fn relative_path_runtime_matches_every_published_schema_boundary() {
    for accepted in ["frame.fit", "night-1/frame.fit", &"a".repeat(255)] {
        assert!(CanonicalRelativePath::try_new(accepted).is_ok(), "accepted path: {accepted}");
    }
    let too_many_segments = std::iter::repeat_n("a", 65).collect::<Vec<_>>().join("/");
    let too_many_bytes = std::iter::repeat_n("a".repeat(64), 64).collect::<Vec<_>>().join("/");
    for rejected in [
        "",
        "/frame.fit",
        "C:/frame.fit",
        "night\\frame.fit",
        "night//frame.fit",
        "night/./frame.fit",
        "night/../frame.fit",
        "night/frame.fit/",
        &"a".repeat(256),
        &too_many_segments,
        &too_many_bytes,
    ] {
        assert!(CanonicalRelativePath::try_new(rejected).is_err(), "rejected path: {rejected}");
    }
}

#[test]
fn relative_path_schema_and_serde_share_utf8_byte_boundaries() {
    let segment_255 = format!("{}a", "é".repeat(127));
    let segment_256 = "é".repeat(128);
    let total_4096 = std::iter::repeat_n("é".repeat(120), 17).collect::<Vec<_>>().join("/");
    let mut total_4097 = total_4096.clone();
    total_4097.push('a');
    let corpus =
        [(segment_255, true), (segment_256, false), (total_4096, true), (total_4097, false)];
    assert_eq!(corpus[0].0.len(), 255);
    assert_eq!(corpus[1].0.len(), 256);
    assert_eq!(corpus[2].0.len(), 4096);
    assert_eq!(corpus[3].0.len(), 4097);

    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir.parent().and_then(std::path::Path::parent).unwrap();
    let validator = repo_root.join("packages/contracts/tests/utf8-byte-keywords.test.mjs");
    let mut child = Command::new("node")
        .arg(validator)
        .arg("--stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("project AJV validator should start");
    let schema = serde_json::to_value(schema_for!(CanonicalRelativePath)).unwrap();
    let request = json!({
        "schema": schema,
        "cases": corpus.iter().map(|(value, _)| json!({ "value": value })).collect::<Vec<_>>(),
    });
    serde_json::to_writer(child.stdin.as_mut().unwrap(), &request).unwrap();
    child.stdin.take().unwrap().flush().unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "AJV validator failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let schema_results: Vec<bool> = serde_json::from_slice(&output.stdout).unwrap();

    for ((value, expected), schema_valid) in corpus.iter().zip(schema_results) {
        let serde_valid = serde_json::from_value::<CanonicalRelativePath>(json!(value)).is_ok();
        assert_eq!(schema_valid, *expected, "schema byte boundary for {} bytes", value.len());
        assert_eq!(serde_valid, *expected, "Serde byte boundary for {} bytes", value.len());
        assert_eq!(schema_valid, serde_valid, "schema/Serde parity for {} bytes", value.len());
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn all_65_published_list_operations_match_the_documented_total_order_fixture() {
    let mut actual = Vec::new();
    append_registry(InboxListOperation::ALL, &mut actual);
    append_registry(MetadataListOperation::ALL, &mut actual);
    append_registry(RelationListOperation::ALL, &mut actual);
    append_registry(ProjectListOperation::ALL, &mut actual);
    append_registry(CalibrationListOperation::ALL, &mut actual);

    let expected: &[(&str, &[&str])] = &[
        (
            "inbox.acquisition_site_candidate.list",
            &["confidenceRank DESC", "normalizedLabel ASC", "siteId ASC"],
        ),
        (
            "inbox.materialization_plan.proposed_session.list",
            &["ordinal ASC", "proposedSessionKey ASC"],
        ),
        ("inbox.materialization_plan.proposed_frame.list", &["ordinal ASC", "frameId ASC"]),
        ("inbox.materialization_plan.blocked_frame.list", &["ordinal ASC", "frameId ASC"]),
        ("session.materialization.result_session.list", &["ordinal ASC", "sessionId ASC"]),
        ("session.materialization.result_frame.list", &["ordinal ASC", "frameId ASC"]),
        ("session.materialization.blocked_frame.list", &["ordinal ASC", "frameId ASC"]),
        ("metadata.reclassification.replacement_frame.list", &["ordinal ASC", "frameId ASC"]),
        (
            "metadata.reclassification.replacement_session.list",
            &["ordinal ASC", "replacementKey ASC"],
        ),
        ("metadata.reclassification.panel_consequence.list", &["ordinal ASC", "panelGroupId ASC"]),
        (
            "metadata.reclassification.panel_consequence_session.list",
            &["ordinal ASC", "proposedSessionId ASC"],
        ),
        (
            "metadata.reclassification.panel_consequence_retirement.list",
            &["ordinal ASC", "predecessorPanelGroupId ASC"],
        ),
        (
            "metadata.reclassification.panel_consequence_lineage.list",
            &[
                "ordinal ASC",
                "lineage.predecessorPanelGroupId ASC",
                "lineage.successorPanelGroupId ASC",
            ],
        ),
        ("metadata.reclassification.stale_mosaic_edge.list", &["ordinal ASC", "edgeId ASC"]),
        (
            "metadata.reclassification.project_consequence.list",
            &["projectId ASC", "unchangedPinnedSessionId ASC"],
        ),
        (
            "metadata.reclassification.project_consequence_replacement.list",
            &["ordinal ASC", "replacementKey ASC"],
        ),
        (
            "metadata.reclassification.apply_result.replacement_session.list",
            &["ordinal ASC", "replacementSessionId ASC"],
        ),
        (
            "metadata.reclassification.apply_result.panel_revision.list",
            &["ordinal ASC", "revisionRef.revisionId ASC"],
        ),
        (
            "metadata.reclassification.apply_result.invalidated_edge.list",
            &["ordinal ASC", "edgeId ASC"],
        ),
        (
            "metadata.reclassification.apply_result.retired_panel_group.list",
            &["ordinal ASC", "panelGroupId ASC"],
        ),
        (
            "metadata.reclassification.apply_result.panel_lineage.list",
            &[
                "ordinal ASC",
                "lineage.predecessorPanelGroupId ASC",
                "lineage.successorPanelGroupId ASC",
            ],
        ),
        (
            "metadata.reclassification.apply_result.project_proposal.list",
            &["ordinal ASC", "projectReplacementProposalId ASC"],
        ),
        ("session.list", &["createdAt DESC", "sessionId ASC"]),
        ("session.frame.list", &["ordinal ASC", "frameId ASC"]),
        ("session.supersession_successor.list", &["ordinal ASC", "successorSessionId ASC"]),
        ("session.supersession_predecessor.list", &["ordinal ASC", "predecessorSessionId ASC"]),
        ("panel_group.membership.list", &["ordinal ASC", "sessionId ASC"]),
        ("panel_group.history.list", &["revisionNumber DESC", "revisionId ASC"]),
        (
            "panel_group.lineage_predecessor.list",
            &["acceptedAt DESC", "acceptedProposalId ASC", "ordinal ASC", "predecessorGroupId ASC"],
        ),
        (
            "panel_group.lineage_successor.list",
            &["acceptedAt DESC", "acceptedProposalId ASC", "ordinal ASC", "successorGroupId ASC"],
        ),
        ("panel_group.list", &["acceptedAt DESC", "panelGroupId ASC"]),
        ("mosaic.panel.list", &["ordinal ASC", "panelRevisionId ASC", "panelGroupId ASC"]),
        ("mosaic.edge.list", &["ordinal ASC", "edgeId ASC"]),
        ("mosaic.history.list", &["revisionNumber DESC", "revisionId ASC"]),
        (
            "mosaic.lineage_predecessor.list",
            &[
                "acceptedAt DESC",
                "acceptedProposalId ASC",
                "ordinal ASC",
                "predecessorMosaicId ASC",
            ],
        ),
        (
            "mosaic.lineage_successor.list",
            &["acceptedAt DESC", "acceptedProposalId ASC", "ordinal ASC", "successorMosaicId ASC"],
        ),
        ("mosaic.object_evidence.list", &["canonicalObjectId ASC"]),
        ("relation_proposal.list", &["createdAt DESC", "proposalId ASC"]),
        (
            "relation_proposal.source_revision.list",
            &["ordinal ASC", "entityType ASC", "entityId ASC", "revisionId ASC"],
        ),
        ("relation_proposal.subject.list", &["ordinal ASC", "entityType ASC", "entityId ASC"]),
        ("relation_proposal.membership.list", &["ordinal ASC", "entityType ASC", "entityId ASC"]),
        ("relation_proposal.edge.list", &["ordinal ASC", "edgeId ASC"]),
        (
            "relation_proposal.lineage.list",
            &["ordinal ASC", "predecessorGroupId ASC", "successorGroupId ASC"],
        ),
        (
            "relation_proposal.decision_revision.list",
            &["ordinal ASC", "entityType ASC", "entityId ASC", "revisionId ASC"],
        ),
        ("relation_proposal.decision_retired_group.list", &["ordinal ASC", "groupId ASC"]),
        (
            "relation_proposal.decision_session_supersession.list",
            &["ordinal ASC", "predecessorSessionId ASC", "successorSessionId ASC"],
        ),
        (
            "relation_proposal.decision_group_lineage.list",
            &["ordinal ASC", "predecessorGroupId ASC", "successorGroupId ASC"],
        ),
        (
            "relation_traversal_preview.node.list",
            &["ordinal ASC", "nodeRef.entityType ASC", "nodeRef.entityId ASC"],
        ),
        (
            "relation_traversal_preview.edge.list",
            &["ordinal ASC", "edgeRef.entityType ASC", "edgeRef.entityId ASC"],
        ),
        ("project.related_session.list", &["firstAvailableAt DESC", "sessionId ASC"]),
        ("project.view_state.pin.list", &["sessionId ASC"]),
        ("project.view_state.materialized_session.list", &["sessionId ASC"]),
        ("project.view_state.unmaterialized_session.list", &["sessionId ASC"]),
        ("project.manifest.entry.list", &["ordinal ASC", "entryId ASC"]),
        ("project.manifest.correction_overlay.list", &["ordinal ASC", "overlayId ASC"]),
        ("project.correction_overlay.mapping.list", &["ordinal ASC", "predecessorEntryId ASC"]),
        ("project.update_view.pinned_session.list", &["ordinal ASC", "sessionId ASC"]),
        ("project.update_view.added_session.list", &["ordinal ASC", "sessionId ASC"]),
        ("project.update_view.item.list", &["ordinal ASC", "itemId ASC"]),
        ("project.update_view.conflict.list", &["ordinal ASC", "itemId ASC"]),
        ("project.update_view.overlay_mapping.list", &["ordinal ASC", "predecessorEntryId ASC"]),
        (
            "calibration.candidate.list",
            &[
                "compatibilityRank ASC",
                "sufficiencyRank ASC",
                "observingNight DESC",
                "createdAt DESC",
                "sessionId ASC",
            ],
        ),
        ("calibration.handoff.requirement.list", &["requirementId ASC"]),
        ("calibration.handoff.selection.list", &["selectedAt ASC", "selectionId ASC"]),
        (
            "calibration.handoff.frame.list",
            &["selectionId ASC", "sessionMembershipOrdinal ASC", "frameId ASC"],
        ),
    ];
    assert_eq!(expected.len(), 65);
    assert_eq!(actual, expected);
}

#[test]
fn six_surface_wire_operations_are_dotted_flat_and_operation_specific() {
    let samples = [
        serde_json::to_value(InboxQuery::Materialization(OperationQueryRequest {
            operation_id: id(ID),
        }))
        .unwrap(),
        serde_json::to_value(MetadataQuery::Evidence(MetadataEvidenceQueryRequest {
            session_id: id(ID),
            evidence_revision: Some(3),
        }))
        .unwrap(),
        serde_json::to_value(RelationQuery::Session(SessionQueryRequest { session_id: id(ID) }))
            .unwrap(),
        serde_json::to_value(ProjectQuery::ViewState(ProjectQueryRequest { project_id: id(ID) }))
            .unwrap(),
        serde_json::to_value(CalibrationQuery::HandoffOperation(HandoffOperationQueryRequest {
            operation_id: id(ID),
        }))
        .unwrap(),
        serde_json::to_value(MatchingSettingsQuery::Get(MatchingSettingsGetRequest {
            revision: Some(7),
        }))
        .unwrap(),
    ];
    let operations = [
        "session.materialization.query",
        "metadata.evidence.query",
        "session.query",
        "project.view_state.query",
        "calibration.handoff.operation.query",
        "matching_settings.get",
    ];
    for (sample, operation) in samples.iter().zip(operations) {
        assert_eq!(sample["operation"], operation);
        assert!(sample.get("payload").is_none());
    }
    assert_eq!(samples[1]["sessionId"], ID);
    assert_eq!(samples[1]["evidenceRevision"], 3);
    assert!(samples[1].get("entityId").is_none());
    assert_eq!(samples[2]["sessionId"], ID);
}

#[test]
fn six_surface_events_use_published_dotted_discriminants_and_flat_fields() {
    let samples = [
        (
            serde_json::to_value(InboxEvent::MaterializationDiscarded { plan_id: id(ID) }).unwrap(),
            json!({ "event": "inbox.materialization_discarded", "planId": ID }),
        ),
        (
            serde_json::to_value(MetadataEvent::ReclassificationDiscarded { plan_id: id(ID) })
                .unwrap(),
            json!({ "event": "metadata.reclassification_discarded", "planId": ID }),
        ),
        (
            serde_json::to_value(RelationEvent::SessionSuperseded {
                predecessor_session_id: id(ID),
                replacement_session_count: 2,
                applied_reclassification_plan_revision_id: id(OTHER_ID),
            })
            .unwrap(),
            json!({
                "event": "session.superseded",
                "predecessorSessionId": ID,
                "replacementSessionCount": 2,
                "appliedReclassificationPlanRevisionId": OTHER_ID,
            }),
        ),
        (
            serde_json::to_value(ProjectEvent::ViewStale {
                project_id: id(ID),
                unmaterialized_session_count: 3,
            })
            .unwrap(),
            json!({
                "event": "project.view_stale",
                "projectId": ID,
                "unmaterializedSessionCount": 3,
            }),
        ),
        (
            serde_json::to_value(CalibrationEvent::HandoffCreated {
                project_id: id(ID),
                handoff_id: id(OTHER_ID),
                snapshot_id: id(ID),
                automatic_selection_ids: BoundedList::default(),
                unselected_requirement_ids: BoundedList::default(),
                warning_codes: BoundedList::default(),
            })
            .unwrap(),
            json!({
                "event": "calibration.handoff_created",
                "projectId": ID,
                "handoffId": OTHER_ID,
                "snapshotId": ID,
                "automaticSelectionIds": [],
                "unselectedRequirementIds": [],
                "warningCodes": [],
            }),
        ),
        (
            serde_json::to_value(MatchingSettingsEvent::Updated(MatchingSettingsUpdatedEvent {
                previous_revision: 7,
                revision: 8,
                changed_field_paths: BoundedList::default(),
                warning_codes: BoundedList::default(),
            }))
            .unwrap(),
            json!({
                "event": "matching_settings.updated",
                "previousRevision": 7,
                "revision": 8,
                "changedFieldPaths": [],
                "warningCodes": [],
            }),
        ),
    ];

    for (actual, expected) in samples {
        assert_eq!(actual, expected);
        assert!(actual.get("payload").is_none());
    }
}

#[test]
fn unknown_calibration_evidence_is_explicitly_blocked() {
    let candidate: CalibrationCandidateEvidence = serde_json::from_value(json!({
        "evidenceId": ID,
        "sessionId": ID,
        "requirementId": ID,
        "recipeCompatibility": "unknown",
        "recipeEvidenceRef": "recipe-evidence",
        "recipeEvidenceComplete": true,
        "missingRecipeFields": [],
        "temperatureMode": "unknown",
        "age": {
            "basis": "elapsed_days",
            "state": "unknown",
            "freshThroughDays": 30,
            "redAfterDays": 365,
            "settingsRevision": 4
        },
        "thermal": {
            "state": "unknown",
            "missingReadingCount": 1,
            "invalidReadingCount": 0
        },
        "orientation": { "state": "not_applicable" },
        "sourceAvailability": {
            "indexedFrameCount": 4,
            "availableReadableIndexedFrameCount": 4,
            "checkedAt": "2026-07-22T00:00:00Z"
        },
        "sufficient": true,
        "automaticEligibility": "blocked",
        "warningCodes": ["calibration.temperature_unknown"],
        "basisFingerprint": DIGEST
    }))
    .unwrap();

    assert_eq!(candidate.automatic_eligibility, AutomaticEligibility::Blocked);
    assert!(candidate.derives_blocked_state());
}

#[test]
fn risky_but_valid_settings_produce_yellow_warnings() {
    let settings = MatchingSettings {
        revision: 8,
        same_session: GeometryThresholds {
            coverage_min_percent: decimal(92.0),
            center_separation_max_percent: decimal(2.0),
            rotation_max_deg: decimal(1.0),
        },
        sibling: GeometryThresholds {
            coverage_min_percent: decimal(84.0),
            center_separation_max_percent: decimal(5.0),
            rotation_max_deg: decimal(5.0),
        },
        mosaic: MosaicThresholds {
            overlap_min_percent: decimal(2.0),
            overlap_max_percent: decimal(30.0),
            residual_sky_rotation_cap_deg: decimal(10.0),
        },
        dark_thermal: DarkThermalThresholds {
            moderate_deg: decimal(0.5),
            severe_deg: decimal(2.0),
        },
        calibration_age: BoundedList::<CalibrationAgePolicy, 500>::default(),
        flat_orientation: FlatOrientationThresholds {
            normal_through_deg: decimal(2.0),
            red_above_deg: decimal(6.0),
        },
        flat_age: FlatAgeThresholds { red_after_nights: 100 },
        fixed_rules: FixedMatchingRules::default(),
        updated_at: Rfc3339Timestamp::try_new("2026-07-22T00:00:00Z").unwrap(),
        updated_by: id(ID),
    };

    let validation = settings.validate();
    assert!(validation.valid);
    assert!(!validation.issues.is_empty());
    assert!(validation.issues.iter().all(|issue| issue.severity == SettingsSeverity::Yellow));
}

// Every field at its matching-settings.md default, so only the cap under test can
// contribute an issue.
fn settings_with_rotation_cap(cap: FiniteDecimal) -> MatchingSettings {
    MatchingSettings {
        revision: 1,
        same_session: GeometryThresholds {
            coverage_min_percent: decimal(95.0),
            center_separation_max_percent: decimal(2.0),
            rotation_max_deg: decimal(1.0),
        },
        sibling: GeometryThresholds {
            coverage_min_percent: decimal(90.0),
            center_separation_max_percent: decimal(5.0),
            rotation_max_deg: decimal(5.0),
        },
        mosaic: MosaicThresholds {
            overlap_min_percent: decimal(5.0),
            overlap_max_percent: decimal(40.0),
            residual_sky_rotation_cap_deg: cap,
        },
        dark_thermal: DarkThermalThresholds {
            moderate_deg: decimal(0.5),
            severe_deg: decimal(2.0),
        },
        calibration_age: BoundedList::<CalibrationAgePolicy, 500>::default(),
        flat_orientation: FlatOrientationThresholds {
            normal_through_deg: decimal(2.0),
            red_above_deg: decimal(5.0),
        },
        flat_age: FlatAgeThresholds { red_after_nights: 7 },
        fixed_rules: FixedMatchingRules::default(),
        updated_at: Rfc3339Timestamp::try_new("2026-07-22T00:00:00Z").unwrap(),
        updated_by: id(ID),
    }
}

#[test]
fn mosaic_residual_sky_rotation_cap_is_fixed_at_the_spec_value() {
    // matching-settings.md:17 fixes residualSkyRotationCapDeg at 10. It has no row in
    // the bounds table, so any other value is a red fixed-rule change, not a warning.
    let at_spec = settings_with_rotation_cap(decimal(10.0));
    assert!(at_spec
        .validate()
        .issues
        .iter()
        .all(|issue| issue.code.as_str() != "settings.fixed_rule_changed"));

    let off_spec = settings_with_rotation_cap(decimal(90.0));
    let validation = off_spec.validate();
    assert!(!validation.valid);
    assert!(validation.issues.iter().any(|issue| {
        issue.code.as_str() == "settings.fixed_rule_changed"
            && issue.severity == SettingsSeverity::Red
            && issue
                .field_paths
                .iter()
                .any(|path| path.as_str() == "mosaic.residualSkyRotationCapDeg")
    }));
}

#[test]
fn every_configured_hard_bound_rejects_the_adjacent_float_outside_it() {
    // The bounds in matching-settings.md are inclusive, so the nearest representable
    // value beyond each edge must be red. Probing adjacent floats rather than round
    // numbers is what distinguishes an inclusive check from an exclusive one.
    type Case = (&'static str, fn(&mut MatchingSettings, FiniteDecimal), f64, f64);
    let cases: [Case; 12] = [
        (
            "sameSession.coverageMinPercent",
            |s, v| s.same_session.coverage_min_percent = v,
            90.0,
            99.5,
        ),
        (
            "sameSession.centerSeparationMaxPercent",
            |s, v| s.same_session.center_separation_max_percent = v,
            0.5,
            5.0,
        ),
        ("sameSession.rotationMaxDeg", |s, v| s.same_session.rotation_max_deg = v, 0.25, 3.0),
        ("sibling.coverageMinPercent", |s, v| s.sibling.coverage_min_percent = v, 80.0, 95.0),
        (
            "sibling.centerSeparationMaxPercent",
            |s, v| s.sibling.center_separation_max_percent = v,
            2.0,
            15.0,
        ),
        ("sibling.rotationMaxDeg", |s, v| s.sibling.rotation_max_deg = v, 1.0, 15.0),
        ("mosaic.overlapMinPercent", |s, v| s.mosaic.overlap_min_percent = v, 1.0, 20.0),
        ("mosaic.overlapMaxPercent", |s, v| s.mosaic.overlap_max_percent = v, 20.0, 60.0),
        ("darkThermal.moderateDeg", |s, v| s.dark_thermal.moderate_deg = v, 0.1, 2.0),
        ("darkThermal.severeDeg", |s, v| s.dark_thermal.severe_deg = v, 0.5, 5.0),
        (
            "flatOrientation.normalThroughDeg",
            |s, v| s.flat_orientation.normal_through_deg = v,
            0.5,
            5.0,
        ),
        ("flatOrientation.redAboveDeg", |s, v| s.flat_orientation.red_above_deg = v, 0.5, 15.0),
    ];

    for (field_path, mutate, minimum, maximum) in cases {
        let outside =
            [f64::from_bits(minimum.to_bits() - 1), f64::from_bits(maximum.to_bits() + 1)];
        for value in outside {
            let mut settings = settings_with_rotation_cap(decimal(10.0));
            mutate(&mut settings, decimal(value));
            let validation = settings.validate();
            assert!(
                !validation.valid,
                "{field_path} accepted {value} outside {minimum}..={maximum}"
            );
            assert!(
                validation.issues.iter().any(|issue| {
                    issue.code.as_str() == "settings.out_of_bounds"
                        && issue.severity == SettingsSeverity::Red
                        && issue.field_paths.iter().any(|path| path.as_str() == field_path)
                }),
                "{field_path} at {value} produced no out_of_bounds issue for that field"
            );
        }
    }
}

#[test]
fn stale_errors_are_typed_and_detail_shapes_are_allowlisted_on_decode() {
    let stale = PortableContractError::try_new(
        ErrorCode::RelationProposalStale,
        safe("Proposal basis is stale."),
        Some(SafeErrorDetails::StaleRevisions {
            proposal_id: id(ID),
            revisions: BoundedList::default(),
            total_count: 0,
            truncated: false,
        }),
    )
    .unwrap();
    let encoded = serde_json::to_value(&stale).unwrap();
    assert_eq!(encoded["code"], "relation_proposal.stale");
    assert_eq!(serde_json::from_value::<PortableContractError>(encoded).unwrap(), stale);

    let invalid = json!({
        "code": "resource.unavailable",
        "message": "Resource unavailable.",
        "details": {
            "kind": "domain",
            "code": "secret",
            "values": []
        }
    });
    assert!(serde_json::from_value::<PortableContractError>(invalid).is_err());

    let wrong_field = PortableContractError::try_new(
        ErrorCode::SessionNotFound,
        safe("Session was not found."),
        Some(SafeErrorDetails::Domain {
            code: safe("session.not_found"),
            values: BoundedList::try_new(vec![NamedSafeValue {
                name: safe("secretPath"),
                value: SafeScalar::Text(safe("redacted")),
            }])
            .unwrap(),
            decision_snapshot_id: None,
        }),
    );
    assert!(wrong_field.is_err());
}

#[test]
fn domain_errors_require_the_exact_published_field_set() {
    let detail = |code: &str, values: Vec<(&str, SafeScalar)>, decision_snapshot_id| {
        SafeErrorDetails::Domain {
            code: safe(code),
            values: BoundedList::try_new(
                values
                    .into_iter()
                    .map(|(name, value)| NamedSafeValue { name: safe(name), value })
                    .collect(),
            )
            .unwrap(),
            decision_snapshot_id,
        }
    };
    let text = || SafeScalar::Text(safe(ID));

    assert!(PortableContractError::try_new(
        ErrorCode::SessionNotFound,
        safe("Session was not found."),
        Some(detail("session.not_found", vec![("sessionId", text())], None)),
    )
    .is_ok());
    assert!(PortableContractError::try_new(
        ErrorCode::SessionNotFound,
        safe("Session was not found."),
        Some(detail("session.not_found", vec![], None)),
    )
    .is_err());
    assert!(PortableContractError::try_new(
        ErrorCode::InboxPlanDigestMismatch,
        safe("Digest mismatch."),
        Some(detail(
            "inbox.plan_digest_mismatch",
            vec![("planId", text()), ("expectedDigest", text())],
            None,
        )),
    )
    .is_err());
    assert!(PortableContractError::try_new(
        ErrorCode::SessionNotFound,
        safe("Session was not found."),
        Some(detail(
            "session.not_found",
            vec![("sessionId", text()), ("unexpected", text())],
            None,
        )),
    )
    .is_err());
    assert!(PortableContractError::try_new(
        ErrorCode::SessionNotFound,
        safe("Session was not found."),
        Some(
            detail("session.not_found", vec![("sessionId", text()), ("sessionId", text())], None,)
        ),
    )
    .is_err());
    assert!(PortableContractError::try_new(
        ErrorCode::InboxPlanStale,
        safe("Plan is stale."),
        Some(detail(
            "inbox.plan_stale",
            vec![("planId", text()), ("staleRevisionCount", SafeScalar::Unsigned(1)),],
            None,
        )),
    )
    .is_err());
    assert!(PortableContractError::try_new(
        ErrorCode::InboxPlanStale,
        safe("Plan is stale."),
        Some(detail(
            "inbox.plan_stale",
            vec![("planId", text()), ("staleRevisionCount", SafeScalar::Unsigned(1)),],
            Some(id(ID)),
        )),
    )
    .is_ok());
    assert!(PortableContractError::try_new(
        ErrorCode::SessionNotFound,
        safe("Session was not found."),
        Some(detail("session.not_found", vec![("sessionId", text())], Some(id(ID)),)),
    )
    .is_err());
    assert!(PortableContractError::try_new(
        ErrorCode::CalibrationHandoffTooLarge,
        safe("Handoff is too large."),
        Some(detail(
            "calibration.handoff_too_large",
            vec![
                ("sourceByteCount", SafeScalar::Unsigned(10)),
                ("maximumSourceBytes", SafeScalar::Unsigned(8)),
            ],
            None,
        )),
    )
    .is_ok());
}

#[test]
fn protected_missing_and_unauthorized_resources_are_indistinguishable() {
    let missing = ProtectedResourceState::Missing.projected_error().unwrap();
    let unauthorized = ProtectedResourceState::Unauthorized.projected_error().unwrap();
    assert_eq!(missing, unauthorized);
    let encoded = serde_json::to_string(&missing).unwrap();
    assert_eq!(encoded, r#"{"code":"resource.unavailable","message":"Resource unavailable."}"#);
    assert!(!encoded.contains(ID));
}

#[test]
fn idempotency_and_fencing_have_explicit_portable_states() {
    let current = CommandFence { command_id: uuid(ID), lease_generation: 7 };
    let stale = CommandFence { command_id: uuid(ID), lease_generation: 6 };
    let other = CommandFence { command_id: uuid(OTHER_ID), lease_generation: 7 };
    assert!(current.is_current(&current));
    assert!(!stale.is_current(&current));
    assert!(!other.is_current(&current));

    let mismatch = IdempotencyOutcome::<SafeText>::PayloadMismatch;
    assert_eq!(serde_json::to_value(mismatch).unwrap()["state"], "payload_mismatch");
}

#[test]
fn manual_relation_request_discloses_missing_evidence_and_required_collections() {
    let entity = EntityRef { entity_type: safe("session"), entity_id: id(ID) };
    let request = ManualRelationCreateRequest {
        relation_kind: ManualRelationKind::PanelAdd,
        source_revision_refs: BoundedList::try_new(vec![RevisionRef {
            entity_type: safe("panel_group"),
            entity_id: id(ID),
            revision_id: id(OTHER_ID),
            revision_number: 3,
        }])
        .unwrap(),
        subject_refs: BoundedList::try_new(vec![entity.clone()]).unwrap(),
        proposed_membership_refs: Some(BoundedList::try_new(vec![entity]).unwrap()),
        proposed_edges: None,
        proposed_lineage: None,
        target_scope: ManualTargetScope::SameTarget { canonical_target_id: id(ID) },
        evidence: RelationEvidence {
            evidence_id: id(ID),
            target_compatibility: TargetCompatibility::SameTarget,
            footprint_coverage_percent: None,
            center_separation_percent: None,
            residual_sky_rotation_deg: None,
            allowed_residual_rotation_ranges_deg:
                BoundedList::<ContractRange<FiniteDecimal>, 16>::default(),
            parity: ParityEvidence::Unknown,
            acquisition_geometry: EvidenceTernary::Unknown,
            equipment: EvidenceTernary::Unknown,
            missing_evidence_codes: BoundedList::try_new(vec![safe("equipment.unknown")]).unwrap(),
            threshold_snapshot: BoundedList::default(),
        },
        review_reason: non_blank("Reviewed by operator"),
        mutation_context: MutationContext {
            command_id: uuid(OTHER_ID),
            reason: Some(safe("manual relation")),
            approval_digest: None,
        },
    };

    assert!(request.has_required_collections());
    let encoded = serde_json::to_value(&request).unwrap();
    assert_eq!(encoded["relationKind"], "panel_add");
    assert_eq!(encoded["evidence"]["missingEvidenceCodes"][0], "equipment.unknown");
    assert_eq!(
        serde_json::from_value::<ManualRelationCreateRequest>(encoded.clone()).unwrap(),
        request
    );

    let mut wrong_kind = encoded.clone();
    wrong_kind["relationKind"] = json!("mosaic_edge");
    assert!(serde_json::from_value::<ManualRelationCreateRequest>(wrong_kind).is_err());

    let mut duplicate_cross_target = encoded;
    duplicate_cross_target["targetScope"] = json!({
        "kind": "new_reviewed_cross_target",
        "canonicalTargetIds": [ID, ID],
        "purpose": "Reviewed association"
    });
    assert!(serde_json::from_value::<ManualRelationCreateRequest>(duplicate_cross_target).is_err());
}

#[test]
fn optional_defaults_and_relation_bounds_are_enforced_on_decode() {
    let panel: PanelListRequest = serde_json::from_value(json!({ "page": {} })).unwrap();
    assert!(panel.active_only);

    let related: RelatedSessionListRequest = serde_json::from_value(json!({
        "projectId": ID,
        "page": {}
    }))
    .unwrap();
    assert!(!related.include_pinned);

    let traversal: TraversalStartRequest = serde_json::from_value(json!({
        "startRefs": [{ "entityType": "panel_group", "entityId": ID }],
        "graph": "panel_lineage",
        "direction": "both"
    }))
    .unwrap();
    assert_eq!(traversal.limits.max_depth, 64);
    assert_eq!(traversal.limits.max_nodes, 10_000);
    assert_eq!(traversal.limits.max_edges, 50_000);
    assert!(serde_json::from_value::<TraversalStartRequest>(json!({
        "startRefs": [{ "entityType": "panel_group", "entityId": ID }],
        "graph": "panel_lineage",
        "direction": "both",
        "limits": { "maxDepth": 0, "maxNodes": 1, "maxEdges": 1 }
    }))
    .is_err());

    let mut fixed = serde_json::to_value(FixedMatchingRules::default()).unwrap();
    fixed["opticalProfileSameMaxPercent"] = json!(6);
    assert!(serde_json::from_value::<FixedMatchingRules>(fixed).is_err());
}

#[test]
fn dark_flat_is_not_a_supported_frame_or_calibration_kind() {
    assert!(serde_json::from_str::<CalibrationKind>(r#""dark_flat""#).is_err());
    assert!(serde_json::from_str::<SupportedFrameKind>(r#""dark_flat""#).is_err());

    let calibration_schema = serde_json::to_string(&schema_for!(CalibrationKind)).unwrap();
    let frame_schema = serde_json::to_string(&schema_for!(SupportedFrameKind)).unwrap();
    assert!(!calibration_schema.contains("dark_flat"));
    assert!(!frame_schema.contains("dark_flat"));
}

#[test]
fn redacted_calibration_sources_omit_paths_and_fingerprints() {
    let redacted = CalibrationHandoffFrame::Redacted {
        selection_id: id(ID),
        session_id: id(ID),
        session_membership_ordinal: 1,
        frame_id: id(OTHER_ID),
        source_state: IndexedReadableState::IndexedReadable,
    };
    let encoded = serde_json::to_value(redacted).unwrap();
    assert_eq!(encoded["visibility"], "redacted");
    for forbidden in
        ["sourceRootId", "sourceRelativePath", "stableFileIdentity", "strongContentFingerprint"]
    {
        assert!(encoded.get(forbidden).is_none());
    }
}

#[test]
fn portable_error_and_contract_schema_roots_are_serializable() {
    let schema: Value = serde_json::to_value(schema_for!(PortableContractError)).unwrap();
    assert!(schema.get("$schema").is_some());
}
