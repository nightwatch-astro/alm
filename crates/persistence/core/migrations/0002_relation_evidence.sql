-- Copyright (C) 2024-2026 Sjors Robroek
-- SPDX-License-Identifier: AGPL-3.0-only

-- Spec 062 US2: the RelationEvidence envelope every RelationProposal,
-- ManualRelationCreateRequest, PanelGroupRevision, and MosaicRevision carries.
-- See specs/062-session-heterogeneity/data-model.md, `relation_evidence`.
--
-- The three NOT NULL foreign keys the design puts on relation_proposal,
-- panel_group_revision, and mosaic_revision are NOT in this migration. SQLite
-- cannot ADD COLUMN a NOT NULL column with a REFERENCES clause (such a column
-- must default to NULL), so they need a table rebuild, and those three tables
-- have 14 foreign-key dependents between them including two compound keys.
-- Tracked separately.

CREATE TABLE relation_evidence (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    subject_kind TEXT NOT NULL CHECK (subject_kind IN ('proposal','panel_group_representative','mosaic_captured_union')),
    subject_digest TEXT NOT NULL,
    target_compatibility TEXT NOT NULL CHECK (target_compatibility IN ('same_target','reviewed_cross_target','incompatible')),
    parity TEXT NOT NULL CHECK (parity IN ('match','mismatch','unknown')),
    acquisition_geometry TEXT NOT NULL CHECK (acquisition_geometry IN ('compatible','incompatible','unknown')),
    equipment TEXT NOT NULL CHECK (equipment IN ('compatible','incompatible','unknown')),
    -- Nullable by design: the optional geometry values. A committed light
    -- session's singleton panel revision has none, and enumerates the reason in
    -- relation_evidence_missing_code instead.
    footprint_coverage_ppm INTEGER CHECK (footprint_coverage_ppm BETWEEN 0 AND 1000000),
    centre_separation_ppm INTEGER CHECK (centre_separation_ppm >= 0),
    residual_sky_rotation_udeg INTEGER,
    config_revision_row_id INTEGER NOT NULL REFERENCES spec062_config_revision(row_id),
    input_digest TEXT NOT NULL,
    -- Density targets. The child ordinal CHECKs bound each list at its contract
    -- size; these say how many rows the envelope actually claims, so the
    -- completeness predicate can reject a list with holes rather than counting
    -- rows and calling it done.
    expected_measurement_count INTEGER NOT NULL CHECK (expected_measurement_count BETWEEN 0 AND 100),
    expected_missing_code_count INTEGER NOT NULL CHECK (expected_missing_code_count BETWEEN 0 AND 100),
    expected_rotation_range_count INTEGER NOT NULL CHECK (expected_rotation_range_count BETWEEN 0 AND 16),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL
) STRICT;

CREATE INDEX relation_evidence_subject_digest_idx
    ON relation_evidence (subject_kind, subject_digest);

-- Contract bound 100. Every column is NOT NULL: SQLite evaluates a CHECK over a
-- null to null and admits the row, and UNIQUE admits repeated nulls, so a null
-- ordinal would pass both the bound and the per-evidence key while leaving the
-- list unordered.
CREATE TABLE relation_evidence_missing_code (
    evidence_row_id INTEGER NOT NULL REFERENCES relation_evidence(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 0 AND 99),
    code TEXT NOT NULL,
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    PRIMARY KEY (evidence_row_id, ordinal),
    UNIQUE (evidence_row_id, code)
) STRICT;

-- Contract bound 16. lower_udeg and upper_udeg describe an interval closed at
-- both ends, which is why the projection can return minInclusive and
-- maxInclusive as true without storing either flag.
CREATE TABLE relation_evidence_allowed_rotation (
    evidence_row_id INTEGER NOT NULL REFERENCES relation_evidence(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 0 AND 15),
    lower_udeg INTEGER NOT NULL,
    upper_udeg INTEGER NOT NULL,
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    PRIMARY KEY (evidence_row_id, ordinal),
    CHECK (lower_udeg <= upper_udeg)
) STRICT;

-- Contract bound 100. measured_value_micro and threshold_value_micro are signed
-- integer microunits of the row's own unit: REAL would make two envelopes with
-- the same measurement disagree in input_digest. comparison and outcome are the
-- contract ThresholdMeasurement vocabulary, narrower than proposal_measurement's
-- stored set, so no verdict is rewritten during projection.
CREATE TABLE relation_evidence_measurement (
    evidence_row_id INTEGER NOT NULL REFERENCES relation_evidence(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 0 AND 99),
    measurement_key TEXT NOT NULL,
    measured_value_micro INTEGER NOT NULL,
    unit TEXT NOT NULL,
    comparison TEXT NOT NULL CHECK (comparison IN ('lt','lte','eq','gte','gt')),
    threshold_value_micro INTEGER NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('pass','fail')),
    source_evidence_digest TEXT NOT NULL,
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    PRIMARY KEY (evidence_row_id, ordinal),
    UNIQUE (evidence_row_id, measurement_key)
) STRICT;
