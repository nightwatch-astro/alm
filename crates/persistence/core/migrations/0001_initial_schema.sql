-- The pre-1.0 SQLite schema and seed definition. This is migration version 1,
-- it stays version 1, and it is edited in place until 1.0; there is no 0002+.
-- sqlite_* objects and _sqlx_migrations are intentionally excluded.
-- onboarding_state and onboarding_flags intentionally start empty.
-- frame_footprint_rtree virtual table is included; shadow tables are
-- created automatically by rtree on first use.

CREATE TABLE acquisition_fingerprint (
    id                    TEXT PRIMARY KEY NOT NULL REFERENCES acquisition_session(id),
    session_type          TEXT NOT NULL DEFAULT 'light',
    gain                  REAL,
    offset_val            REAL,
    exposure_s            REAL,
    temp_c                REAL,
    filter_name           TEXT,
    rotation_deg          REAL,
    binning               TEXT,
    optic_train           TEXT,
    observing_night_date  TEXT,   -- YYYY-MM-DD local observing night
    has_observer_location INTEGER NOT NULL DEFAULT 0,
    has_exposure_start_utc INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE acquisition_session (
    id TEXT PRIMARY KEY NOT NULL,
    session_key TEXT NOT NULL,
    target_id TEXT REFERENCES target(id),
    -- frame_ids stored as JSON array; relational join table deferred to T006.
    frame_ids TEXT NOT NULL DEFAULT '[]',
    observer_location TEXT,
    created_at TEXT NOT NULL
, root_id TEXT REFERENCES library_root(id), canonical_target_id TEXT REFERENCES canonical_target(id), has_observer_location INTEGER NOT NULL DEFAULT 0, pointing_ra_deg  REAL, pointing_dec_deg REAL, rotation_deg     REAL, optic_train_key  TEXT, notes TEXT);

CREATE TABLE acquisition_site (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    label TEXT NOT NULL,
    timezone_name TEXT,
    timezone_state TEXT NOT NULL CHECK (timezone_state IN ('confirmed','unconfirmed','absent')),
    latitude_udeg INTEGER,
    longitude_udeg INTEGER,
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    CHECK ((timezone_state = 'absent') = (timezone_name IS NULL))
) STRICT;

CREATE TABLE acquisition_site_resolution (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    head_revision_row_id INTEGER,
    head_generation INTEGER NOT NULL DEFAULT 0 CHECK (head_generation >= 0),
    created_at TEXT NOT NULL,
    FOREIGN KEY (head_revision_row_id, row_id)
        REFERENCES acquisition_site_resolution_revision(row_id, resolution_row_id)
        DEFERRABLE INITIALLY DEFERRED
) STRICT;

CREATE TABLE acquisition_site_resolution_candidate (
    revision_row_id INTEGER NOT NULL REFERENCES acquisition_site_resolution_revision(row_id),
    site_row_id INTEGER NOT NULL REFERENCES acquisition_site(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    evidence_digest TEXT NOT NULL,
    PRIMARY KEY (revision_row_id, site_row_id),
    UNIQUE (revision_row_id, ordinal)
) STRICT;

CREATE TABLE acquisition_site_resolution_conflict (
    revision_row_id INTEGER NOT NULL REFERENCES acquisition_site_resolution_revision(row_id),
    reason_code TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (revision_row_id, reason_code),
    UNIQUE (revision_row_id, ordinal)
) STRICT;

CREATE TABLE acquisition_site_resolution_revision (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    resolution_row_id INTEGER NOT NULL REFERENCES acquisition_site_resolution(row_id),
    revision_number INTEGER NOT NULL CHECK (revision_number >= 1),
    predecessor_revision_row_id INTEGER,
    state TEXT NOT NULL CHECK (state IN ('resolved','blocked','needs_review')),
    selected_site_row_id INTEGER REFERENCES acquisition_site(row_id),
    timezone_name TEXT,
    canonical_exposure_at_utc TEXT,
    local_exposure_text TEXT,
    observing_night_date TEXT,
    canonical_digest TEXT NOT NULL,
    actor_row_id INTEGER NOT NULL REFERENCES spec062_actor(row_id),
    command_row_id INTEGER NOT NULL REFERENCES command_execution(row_id),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    UNIQUE (resolution_row_id, revision_number),
    UNIQUE (predecessor_revision_row_id),
    UNIQUE (row_id, resolution_row_id),
    FOREIGN KEY (predecessor_revision_row_id, resolution_row_id)
        REFERENCES acquisition_site_resolution_revision(row_id, resolution_row_id)
) STRICT;

CREATE TABLE audit_event (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    command_row_id INTEGER NOT NULL REFERENCES command_execution(row_id),
    operation_row_id INTEGER REFERENCES session_materialization_operation(row_id),
    proposal_row_id INTEGER REFERENCES relation_proposal(row_id),
    decision_snapshot_row_id INTEGER REFERENCES relation_decision_snapshot(row_id),
    actor_row_id INTEGER NOT NULL REFERENCES spec062_actor(row_id),
    action TEXT NOT NULL,
    session_row_id INTEGER REFERENCES session(row_id),
    panel_group_row_id INTEGER REFERENCES panel_group(row_id),
    mosaic_row_id INTEGER REFERENCES mosaic(row_id),
    project_row_id INTEGER REFERENCES spec062_project(row_id),
    handoff_row_id INTEGER REFERENCES calibration_handoff(row_id),
    outcome TEXT NOT NULL CHECK (outcome IN ('applied','rejected','refused','failed')),
    reason_code TEXT NOT NULL,
    payload_json TEXT,
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    occurred_at TEXT NOT NULL,
    CHECK (
        (operation_row_id IS NOT NULL)
        + (proposal_row_id IS NOT NULL)
        + (session_row_id IS NOT NULL)
        + (panel_group_row_id IS NOT NULL)
        + (mosaic_row_id IS NOT NULL)
        + (project_row_id IS NOT NULL)
        + (handoff_row_id IS NOT NULL) = 1
    )
) STRICT;

CREATE TABLE audit_log_entry (
    audit_id TEXT PRIMARY KEY NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    from_state TEXT,
    to_state TEXT,
    trigger TEXT NOT NULL,
    actor TEXT NOT NULL CHECK (actor IN ('user', 'system')),
    outcome TEXT NOT NULL CHECK (outcome IN ('applied', 'refused', 'failed')),
    severity TEXT NOT NULL CHECK (severity IN ('workflow', 'diagnostic')),
    request_id TEXT NOT NULL,
    at TEXT NOT NULL,
    payload TEXT   -- JSON; NULL when absent
, reason_code TEXT);

CREATE TABLE bias_recipe_identity (
    family_row_id INTEGER PRIMARY KEY REFERENCES calibration_family(row_id),
    gain_text TEXT NOT NULL,
    offset_state TEXT NOT NULL CHECK (offset_state IN ('present','absent')),
    offset_value INTEGER,
    binning_state TEXT NOT NULL CHECK (binning_state IN ('present','absent')),
    bin_x INTEGER,
    bin_y INTEGER,
    readout_state TEXT NOT NULL CHECK (readout_state IN ('present','absent')),
    readout_mode TEXT,
    raster_width INTEGER NOT NULL CHECK (raster_width > 0),
    raster_height INTEGER NOT NULL CHECK (raster_height > 0),
    CHECK ((offset_state = 'present') = (offset_value IS NOT NULL)),
    CHECK ((binning_state = 'present') = (bin_x IS NOT NULL AND bin_y IS NOT NULL)),
    CHECK ((readout_state = 'present') = (readout_mode IS NOT NULL))
) STRICT;

CREATE TABLE calibration_assignment (
    id                    TEXT PRIMARY KEY NOT NULL,
    session_id            TEXT NOT NULL,
    calibration_type      TEXT NOT NULL CHECK (calibration_type IN ('dark', 'flat', 'bias')),
    master_id             TEXT NOT NULL,
    confidence            REAL NOT NULL,
    was_override          INTEGER NOT NULL DEFAULT 0,  -- 0=false, 1=true
    mismatched_dimensions TEXT NOT NULL DEFAULT '[]',  -- JSON array of strings
    assigned_at           TEXT NOT NULL,               -- ISO-8601 UTC

    UNIQUE (session_id, calibration_type)
);

CREATE TABLE calibration_family (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL CHECK (kind IN ('dark','bias','flat')),
    camera_row_id INTEGER REFERENCES camera(row_id),
    optical_profile_row_id INTEGER REFERENCES optical_profile(row_id),
    filter_label_row_id INTEGER,
    identity_digest TEXT NOT NULL,
    representative_session_row_id INTEGER NOT NULL UNIQUE,
    camera_regulation_decision_row_id INTEGER REFERENCES camera_regulation_decision(row_id),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    UNIQUE (row_id, kind),
    CHECK ((kind IN ('dark','bias')) = (camera_row_id IS NOT NULL)),
    CHECK ((kind = 'flat') = (optical_profile_row_id IS NOT NULL)),
    CHECK ((kind = 'flat') = (filter_label_row_id IS NOT NULL)),
    CHECK ((kind = 'dark') = (camera_regulation_decision_row_id IS NOT NULL)),
    FOREIGN KEY (filter_label_row_id, optical_profile_row_id)
        REFERENCES filter_label(row_id, optical_profile_row_id),
    FOREIGN KEY (representative_session_row_id, kind)
        REFERENCES session(row_id, kind)
) STRICT;

CREATE TABLE calibration_fingerprint (
    id                    TEXT PRIMARY KEY NOT NULL REFERENCES calibration_session(id),
    calibration_type      TEXT NOT NULL CHECK (calibration_type IN ('dark', 'flat', 'bias')),
    gain                  REAL,
    offset_val            REAL,
    exposure_s            REAL,
    temp_c                REAL,
    filter_name           TEXT,
    rotation_deg          REAL,
    binning               TEXT,
    optic_train           TEXT,
    source_session_id     TEXT,   -- originating capture session (for same_session reason)
    observing_night_date  TEXT    -- YYYY-MM-DD local observing night
);

CREATE TABLE calibration_handoff (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    project_row_id INTEGER NOT NULL REFERENCES spec062_project(row_id),
    external_processor TEXT NOT NULL,
    head_snapshot_row_id INTEGER,
    head_generation INTEGER NOT NULL DEFAULT 0 CHECK (head_generation >= 0),
    created_at TEXT NOT NULL,
    FOREIGN KEY (head_snapshot_row_id, row_id)
        REFERENCES calibration_handoff_snapshot(row_id, handoff_row_id)
        DEFERRABLE INITIALLY DEFERRED
) STRICT;

CREATE TABLE calibration_handoff_candidate_evidence (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    handoff_row_id INTEGER NOT NULL,
    snapshot_row_id INTEGER NOT NULL,
    requirement_row_id INTEGER NOT NULL,
    session_row_id INTEGER NOT NULL REFERENCES spec062_calibration_session(session_row_id),
    recipe_compatible INTEGER NOT NULL CHECK (recipe_compatible IN (0,1)),
    recipe_complete INTEGER NOT NULL CHECK (recipe_complete IN (0,1)),
    age_days INTEGER NOT NULL CHECK (age_days >= 0),
    age_severity TEXT NOT NULL CHECK (age_severity IN ('normal','yellow','red')),
    thermal_state TEXT NOT NULL CHECK (thermal_state IN ('normal','yellow','red','unknown')),
    available_frame_count INTEGER NOT NULL CHECK (available_frame_count >= 0),
    readable_frame_count INTEGER NOT NULL CHECK (readable_frame_count >= 0),
    automatic_eligible INTEGER NOT NULL CHECK (automatic_eligible IN (0,1)),
    evidence_digest TEXT NOT NULL,
    observed_at TEXT NOT NULL,
    UNIQUE (row_id, handoff_row_id, requirement_row_id, session_row_id),
    FOREIGN KEY (snapshot_row_id, handoff_row_id)
        REFERENCES calibration_handoff_snapshot(row_id, handoff_row_id),
    FOREIGN KEY (requirement_row_id, handoff_row_id)
        REFERENCES calibration_handoff_requirement(row_id, handoff_row_id)
) STRICT;

CREATE TABLE calibration_handoff_candidate_warning (
    candidate_evidence_row_id INTEGER NOT NULL REFERENCES calibration_handoff_candidate_evidence(row_id),
    warning_code TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (candidate_evidence_row_id, warning_code),
    UNIQUE (candidate_evidence_row_id, ordinal)
) STRICT;

CREATE TABLE calibration_handoff_frame (
    selection_row_id INTEGER NOT NULL REFERENCES calibration_handoff_selection(row_id),
    frame_row_id INTEGER NOT NULL REFERENCES frame_record(row_id),
    session_membership_ordinal INTEGER NOT NULL CHECK (session_membership_ordinal >= 0),
    file_row_id INTEGER NOT NULL REFERENCES spec062_file_identity(row_id),
    source_root_row_id INTEGER NOT NULL REFERENCES spec062_source_root(row_id),
    canonical_relative_path TEXT NOT NULL,
    stable_file_identity TEXT NOT NULL,
    byte_size INTEGER NOT NULL CHECK (byte_size >= 0),
    sha256_fingerprint TEXT NOT NULL,
    no_follow_verified INTEGER NOT NULL CHECK (no_follow_verified = 1),
    verified_at TEXT NOT NULL,
    PRIMARY KEY (selection_row_id, frame_row_id),
    UNIQUE (selection_row_id, session_membership_ordinal)
) STRICT;

CREATE TABLE calibration_handoff_operation (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    handoff_row_id INTEGER NOT NULL REFERENCES calibration_handoff(row_id),
    command_row_id INTEGER NOT NULL UNIQUE REFERENCES command_execution(row_id),
    state TEXT NOT NULL CHECK (state IN ('ready','verifying','cancelling','cancelled','applied','failed')),
    state_version INTEGER NOT NULL DEFAULT 0 CHECK (state_version >= 0),
    lease_owner TEXT,
    lease_generation INTEGER NOT NULL DEFAULT 0 CHECK (lease_generation >= 0),
    frame_progress INTEGER NOT NULL DEFAULT 0 CHECK (frame_progress >= 0),
    byte_progress INTEGER NOT NULL DEFAULT 0 CHECK (byte_progress >= 0),
    terminal_snapshot_row_id INTEGER REFERENCES calibration_handoff_snapshot(row_id),
    created_at TEXT NOT NULL
) STRICT;

CREATE TABLE calibration_handoff_requirement (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    handoff_row_id INTEGER NOT NULL REFERENCES calibration_handoff(row_id),
    kind TEXT NOT NULL CHECK (kind IN ('dark','bias','flat')),
    camera_row_id INTEGER REFERENCES camera(row_id),
    family_row_id INTEGER REFERENCES calibration_family(row_id),
    recipe_revision INTEGER NOT NULL CHECK (recipe_revision >= 1),
    evidence_digest TEXT NOT NULL,
    required_field_state TEXT NOT NULL CHECK (required_field_state IN ('complete','incomplete','contradictory')),
    UNIQUE (row_id, handoff_row_id)
) STRICT;

CREATE TABLE calibration_handoff_review (
    selection_row_id INTEGER PRIMARY KEY REFERENCES calibration_handoff_selection(row_id),
    actor_row_id INTEGER NOT NULL REFERENCES spec062_actor(row_id),
    reason_code TEXT NOT NULL,
    audit_row_id INTEGER NOT NULL UNIQUE REFERENCES audit_event(row_id)
        DEFERRABLE INITIALLY DEFERRED,
    created_at TEXT NOT NULL
) STRICT;

CREATE TABLE calibration_handoff_review_warning (
    selection_row_id INTEGER NOT NULL REFERENCES calibration_handoff_review(selection_row_id),
    warning_code TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (selection_row_id, warning_code),
    UNIQUE (selection_row_id, ordinal)
) STRICT;

CREATE TABLE calibration_handoff_selection (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    handoff_row_id INTEGER NOT NULL REFERENCES calibration_handoff(row_id),
    requirement_row_id INTEGER NOT NULL,
    session_row_id INTEGER NOT NULL REFERENCES spec062_calibration_session(session_row_id),
    candidate_evidence_row_id INTEGER NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('automatic','reviewed')),
    selected_at TEXT NOT NULL,
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    UNIQUE (row_id, handoff_row_id),
    FOREIGN KEY (requirement_row_id, handoff_row_id)
        REFERENCES calibration_handoff_requirement(row_id, handoff_row_id),
    FOREIGN KEY (candidate_evidence_row_id, handoff_row_id, requirement_row_id, session_row_id)
        REFERENCES calibration_handoff_candidate_evidence(row_id, handoff_row_id, requirement_row_id, session_row_id)
) STRICT;

CREATE TABLE calibration_handoff_snapshot (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    handoff_row_id INTEGER NOT NULL REFERENCES calibration_handoff(row_id),
    predecessor_snapshot_row_id INTEGER,
    evaluation_at TEXT NOT NULL,
    matching_settings_revision_row_id INTEGER NOT NULL REFERENCES matching_settings_revision(row_id),
    basis_digest TEXT NOT NULL,
    requirement_count INTEGER NOT NULL CHECK (requirement_count >= 0),
    selection_count INTEGER NOT NULL CHECK (selection_count >= 0),
    frame_count INTEGER NOT NULL CHECK (frame_count >= 0),
    source_byte_count INTEGER NOT NULL CHECK (source_byte_count BETWEEN 0 AND 17592186044416),
    actor_row_id INTEGER NOT NULL REFERENCES spec062_actor(row_id),
    command_row_id INTEGER NOT NULL REFERENCES command_execution(row_id),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    UNIQUE (handoff_row_id, predecessor_snapshot_row_id),
    UNIQUE (row_id, handoff_row_id),
    FOREIGN KEY (predecessor_snapshot_row_id, handoff_row_id)
        REFERENCES calibration_handoff_snapshot(row_id, handoff_row_id)
) STRICT;

CREATE TABLE calibration_handoff_snapshot_requirement (
    snapshot_row_id INTEGER NOT NULL,
    requirement_row_id INTEGER NOT NULL,
    handoff_row_id INTEGER NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (snapshot_row_id, requirement_row_id),
    UNIQUE (snapshot_row_id, ordinal),
    FOREIGN KEY (snapshot_row_id, handoff_row_id)
        REFERENCES calibration_handoff_snapshot(row_id, handoff_row_id),
    FOREIGN KEY (requirement_row_id, handoff_row_id)
        REFERENCES calibration_handoff_requirement(row_id, handoff_row_id)
) STRICT;

CREATE TABLE calibration_handoff_snapshot_selection (
    snapshot_row_id INTEGER NOT NULL,
    selection_row_id INTEGER NOT NULL,
    handoff_row_id INTEGER NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (snapshot_row_id, selection_row_id),
    UNIQUE (snapshot_row_id, ordinal),
    FOREIGN KEY (snapshot_row_id, handoff_row_id)
        REFERENCES calibration_handoff_snapshot(row_id, handoff_row_id),
    FOREIGN KEY (selection_row_id, handoff_row_id)
        REFERENCES calibration_handoff_selection(row_id, handoff_row_id)
) STRICT;

CREATE TABLE calibration_master (
    id TEXT PRIMARY KEY NOT NULL,
    source_session_id TEXT NOT NULL REFERENCES calibration_session(id),
    artifact_id TEXT NOT NULL,  -- FK → processing_artifact; deferred FK to avoid ordering issue
    kind TEXT NOT NULL CHECK (kind IN ('master_dark', 'master_flat', 'master_bias', 'master_flat_dark')),
    reuse_match_key TEXT NOT NULL,
    expires_at TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE calibration_reuse_decision (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    calibration_session_row_id INTEGER NOT NULL REFERENCES spec062_calibration_session(session_row_id),
    light_session_row_id INTEGER REFERENCES session(row_id),
    handoff_row_id INTEGER REFERENCES calibration_handoff(row_id),
    family_row_id INTEGER NOT NULL REFERENCES calibration_family(row_id),
    age_days INTEGER NOT NULL CHECK (age_days >= 0),
    age_severity TEXT NOT NULL CHECK (age_severity IN ('normal','yellow','red')),
    evidence_severity TEXT NOT NULL CHECK (evidence_severity IN ('normal','yellow','red','unknown')),
    decision_mode TEXT NOT NULL CHECK (decision_mode IN ('automatic','audited_manual')),
    proposal_row_id INTEGER REFERENCES relation_proposal(row_id),
    config_revision_row_id INTEGER NOT NULL REFERENCES spec062_config_revision(row_id),
    actor_row_id INTEGER NOT NULL REFERENCES spec062_actor(row_id),
    audit_row_id INTEGER REFERENCES audit_event(row_id),
    reason_code TEXT NOT NULL,
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    CHECK ((light_session_row_id IS NULL) <> (handoff_row_id IS NULL)),
    CHECK ((decision_mode = 'audited_manual') = (audit_row_id IS NOT NULL)),
    CHECK (age_severity <> 'red' OR decision_mode = 'audited_manual'),
    CHECK (evidence_severity <> 'red' OR decision_mode = 'audited_manual')
) STRICT;

CREATE TABLE calibration_session (
    id TEXT PRIMARY KEY NOT NULL,
    session_key TEXT NOT NULL,
    frame_ids TEXT NOT NULL DEFAULT '[]',
    kind TEXT NOT NULL CHECK (kind IN ('dark', 'flat', 'bias', 'flat_dark')),
    created_at TEXT NOT NULL
, root_id TEXT REFERENCES library_root(id), source_inbox_item_id TEXT, notes TEXT, archived_at TEXT, archived_via_plan_id TEXT);

CREATE TABLE calibration_tolerances (
    singleton_id            TEXT PRIMARY KEY DEFAULT 'default' CHECK (singleton_id = 'default'),
    temperature_tolerance_c REAL NOT NULL DEFAULT 5.0,
    exposure_tolerance_s    REAL NOT NULL DEFAULT 2.0,
    aging_limit_days        INTEGER NOT NULL DEFAULT 365,
    require_same_gain       INTEGER NOT NULL DEFAULT 1,
    require_same_binning    INTEGER NOT NULL DEFAULT 1,
    updated_at              TEXT NOT NULL
, require_same_offset INTEGER NOT NULL DEFAULT 1);

CREATE TABLE camera (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    regulation_head_decision_row_id INTEGER,
    head_generation INTEGER NOT NULL DEFAULT 0 CHECK (head_generation >= 0),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    FOREIGN KEY (regulation_head_decision_row_id, row_id)
        REFERENCES camera_regulation_decision(row_id, camera_row_id)
        DEFERRABLE INITIALLY DEFERRED
) STRICT;

CREATE TABLE camera_regulation_decision (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    camera_row_id INTEGER NOT NULL REFERENCES camera(row_id),
    predecessor_decision_row_id INTEGER,
    mode TEXT NOT NULL CHECK (mode IN ('regulated','unregulated_reviewed')),
    proposal_row_id INTEGER NOT NULL REFERENCES relation_proposal(row_id),
    config_revision_row_id INTEGER NOT NULL REFERENCES spec062_config_revision(row_id),
    actor_row_id INTEGER NOT NULL REFERENCES spec062_actor(row_id),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    UNIQUE (predecessor_decision_row_id),
    UNIQUE (row_id, camera_row_id),
    FOREIGN KEY (predecessor_decision_row_id, camera_row_id)
        REFERENCES camera_regulation_decision(row_id, camera_row_id)
) STRICT;

CREATE TABLE cameras (
    id            TEXT PRIMARY KEY NOT NULL,
    name          TEXT NOT NULL,
    aliases       TEXT NOT NULL DEFAULT '[]',  -- JSON array of alternate names
    auto_detected INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT NOT NULL
, sensor_type TEXT, passband TEXT, pixel_size_um REAL
    CHECK (pixel_size_um IS NULL OR pixel_size_um > 0), sensor_width_px INTEGER
    CHECK (sensor_width_px IS NULL OR sensor_width_px > 0), sensor_height_px INTEGER
    CHECK (sensor_height_px IS NULL OR sensor_height_px > 0));

CREATE TABLE canonical_target (
    id                  TEXT    NOT NULL PRIMARY KEY,     -- UUID v5
    simbad_oid          INTEGER,                          -- SIMBAD physical-object id; UNIQUE when non-null
    primary_designation TEXT    NOT NULL,                 -- canonical display designation
    object_type         TEXT    NOT NULL,                 -- closed ObjectType enum (snake_case)
    ra_deg              REAL    NOT NULL CHECK (ra_deg  >= 0   AND ra_deg  < 360),
    dec_deg             REAL    NOT NULL CHECK (dec_deg >= -90 AND dec_deg <= 90),
    source              TEXT    NOT NULL CHECK (source IN ('seed', 'resolved', 'user-override')),
    resolved_at         TEXT    NOT NULL,                 -- RFC 3339 UTC
    -- spec 036: optional user-owned display label. NULL = show primary_designation.
    -- Presentation only — never matched/normalized; preserved across re-resolution.
    display_alias       TEXT
, constellation TEXT, magnitude     REAL, notes TEXT);

CREATE TABLE capture_field_mapping (
    capture_profile_version_row_id INTEGER NOT NULL REFERENCES capture_profile_version(row_id),
    semantic_field TEXT NOT NULL CHECK (semantic_field IN ('camera','telescope','filter','focal_length','rotator')),
    source_field TEXT NOT NULL,
    value_type TEXT NOT NULL CHECK (value_type IN ('text','integer','decimal','angle')),
    precedence INTEGER NOT NULL CHECK (precedence >= 0),
    unit TEXT,
    physical_rotator_confirmed INTEGER NOT NULL CHECK (physical_rotator_confirmed IN (0,1)),
    PRIMARY KEY (capture_profile_version_row_id, semantic_field, source_field)
) STRICT;

CREATE TABLE capture_profile (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    head_version_row_id INTEGER,
    head_generation INTEGER NOT NULL DEFAULT 0 CHECK (head_generation >= 0),
    created_at TEXT NOT NULL,
    FOREIGN KEY (head_version_row_id, row_id)
        REFERENCES capture_profile_version(row_id, capture_profile_row_id)
        DEFERRABLE INITIALLY DEFERRED
) STRICT;

CREATE TABLE capture_profile_version (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    capture_profile_row_id INTEGER NOT NULL REFERENCES capture_profile(row_id),
    version_number INTEGER NOT NULL CHECK (version_number >= 1),
    predecessor_version_row_id INTEGER,
    parser_version TEXT NOT NULL,
    canonical_digest TEXT NOT NULL,
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    UNIQUE (capture_profile_row_id, version_number),
    UNIQUE (predecessor_version_row_id),
    UNIQUE (row_id, capture_profile_row_id),
    FOREIGN KEY (predecessor_version_row_id, capture_profile_row_id)
        REFERENCES capture_profile_version(row_id, capture_profile_row_id)
) STRICT;

CREATE TABLE catalog_equivalence (
    id TEXT PRIMARY KEY NOT NULL,
    target_id TEXT NOT NULL REFERENCES target(id),
    alias TEXT NOT NULL,
    catalog_id TEXT,
    catalog_display TEXT,
    designation TEXT,
    created_at TEXT NOT NULL,
    UNIQUE (target_id, alias)
);

CREATE TABLE classification_overrides (
    artifact_id  TEXT NOT NULL PRIMARY KEY REFERENCES processing_artifacts(id),
    kind         TEXT NOT NULL CHECK (kind IN ('intermediate','master','final')),
    created_at   TEXT NOT NULL,
    reason       TEXT NULL
);

CREATE TABLE cleanup_policy (
    data_type  TEXT PRIMARY KEY NOT NULL,
    action     TEXT NOT NULL DEFAULT 'keep' CHECK (action IN ('keep', 'archive', 'delete')),
    updated_at TEXT NOT NULL
);

CREATE TABLE command_execution (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    actor_row_id INTEGER NOT NULL REFERENCES spec062_actor(row_id),
    operation TEXT NOT NULL,
    canonical_payload_digest TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('received','executing','applied','refused','failed')),
    state_version INTEGER NOT NULL DEFAULT 0 CHECK (state_version >= 0),
    lease_generation INTEGER NOT NULL DEFAULT 0 CHECK (lease_generation >= 0),
    lease_owner TEXT,
    lease_expires_at TEXT,
    heartbeat_at TEXT,
    response_json TEXT,
    materialization_result_snapshot_row_id INTEGER REFERENCES session_materialization_result_snapshot(row_id),
    relation_decision_snapshot_row_id INTEGER REFERENCES relation_decision_snapshot(row_id),
    project_materialization_snapshot_row_id INTEGER REFERENCES project_materialization_snapshot(row_id),
    calibration_handoff_snapshot_row_id INTEGER REFERENCES calibration_handoff_snapshot(row_id),
    error_code TEXT,
    created_at TEXT NOT NULL,
    started_at TEXT,
    finished_at TEXT, recovery_terminal_outcome TEXT, recovery_response_json TEXT, recovery_error_code TEXT, recovery_expected_outbox_count INTEGER, recovery_expected_outbox_digest TEXT,
    CHECK ((lease_owner IS NULL) = (lease_expires_at IS NULL)),
    CHECK ((lease_owner IS NULL) = (heartbeat_at IS NULL)),
    CHECK ((state IN ('applied','refused','failed')) = (finished_at IS NOT NULL)),
    CHECK ((state IN ('refused','failed')) = (error_code IS NOT NULL)),
    CHECK (
        (materialization_result_snapshot_row_id IS NOT NULL)
        + (relation_decision_snapshot_row_id IS NOT NULL)
        + (project_materialization_snapshot_row_id IS NOT NULL)
        + (calibration_handoff_snapshot_row_id IS NOT NULL) <= 1
    )
) STRICT;

CREATE TABLE correction_overlay (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    project_row_id INTEGER NOT NULL REFERENCES spec062_project(row_id),
    predecessor_overlay_row_id INTEGER,
    applied_plan_revision_row_id INTEGER NOT NULL REFERENCES reclassification_plan_revision(row_id),
    mapping_count INTEGER NOT NULL CHECK (mapping_count >= 0),
    actor_row_id INTEGER NOT NULL REFERENCES spec062_actor(row_id),
    command_row_id INTEGER NOT NULL REFERENCES command_execution(row_id),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    UNIQUE (row_id, project_row_id),
    UNIQUE (predecessor_overlay_row_id),
    FOREIGN KEY (predecessor_overlay_row_id, project_row_id)
        REFERENCES correction_overlay(row_id, project_row_id)
) STRICT;

CREATE TABLE correction_overlay_mapping (
    overlay_row_id INTEGER NOT NULL REFERENCES correction_overlay(row_id),
    predecessor_entry_row_id INTEGER NOT NULL REFERENCES materialized_entry(row_id),
    replacement_entry_row_id INTEGER REFERENCES materialized_entry(row_id),
    exclusion_reason_code TEXT,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (overlay_row_id, predecessor_entry_row_id),
    UNIQUE (overlay_row_id, ordinal),
    CHECK ((replacement_entry_row_id IS NULL) <> (exclusion_reason_code IS NULL))
) STRICT;

CREATE TABLE cross_target_association (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    purpose TEXT NOT NULL,
    accepted_proposal_row_id INTEGER NOT NULL UNIQUE REFERENCES relation_proposal(row_id),
    actor_row_id INTEGER NOT NULL REFERENCES spec062_actor(row_id),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL
) STRICT;

CREATE TABLE cross_target_association_target (
    association_row_id INTEGER NOT NULL REFERENCES cross_target_association(row_id),
    canonical_target_row_id INTEGER NOT NULL REFERENCES spec062_target(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (association_row_id, canonical_target_row_id),
    UNIQUE (association_row_id, ordinal)
) STRICT;

CREATE TABLE dark_recipe_identity (
    family_row_id INTEGER PRIMARY KEY REFERENCES calibration_family(row_id),
    temperature_mode TEXT NOT NULL CHECK (temperature_mode IN ('regulated','unregulated_reviewed')),
    cooling_setpoint_millic INTEGER,
    representative_exposure_us INTEGER NOT NULL CHECK (representative_exposure_us >= 0),
    gain_text TEXT NOT NULL,
    offset_state TEXT NOT NULL CHECK (offset_state IN ('present','absent')),
    offset_value INTEGER,
    binning_state TEXT NOT NULL CHECK (binning_state IN ('present','absent')),
    bin_x INTEGER,
    bin_y INTEGER,
    readout_state TEXT NOT NULL CHECK (readout_state IN ('present','absent')),
    readout_mode TEXT,
    raster_width INTEGER NOT NULL CHECK (raster_width > 0),
    raster_height INTEGER NOT NULL CHECK (raster_height > 0),
    CHECK ((offset_state = 'present') = (offset_value IS NOT NULL)),
    CHECK ((binning_state = 'present') = (bin_x IS NOT NULL AND bin_y IS NOT NULL)),
    CHECK ((readout_state = 'present') = (readout_mode IS NOT NULL)),
    CHECK ((temperature_mode = 'regulated') = (cooling_setpoint_millic IS NOT NULL))
) STRICT;

CREATE TABLE dark_thermal_evidence (
    session_row_id INTEGER PRIMARY KEY REFERENCES spec062_calibration_session(session_row_id),
    valid_count INTEGER NOT NULL CHECK (valid_count >= 0),
    missing_count INTEGER NOT NULL CHECK (missing_count >= 0),
    invalid_count INTEGER NOT NULL CHECK (invalid_count >= 0),
    minimum_abs_deviation_millic INTEGER,
    median_abs_deviation_millic INTEGER,
    maximum_abs_deviation_millic INTEGER,
    p95_abs_deviation_millic INTEGER,
    valid_ratio_ppm INTEGER NOT NULL CHECK (valid_ratio_ppm BETWEEN 0 AND 1000000),
    severity TEXT NOT NULL CHECK (severity IN ('normal','yellow','red','unknown')),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    CHECK ((valid_count = 0) = (minimum_abs_deviation_millic IS NULL)),
    CHECK ((valid_count = 0) = (median_abs_deviation_millic IS NULL)),
    CHECK ((valid_count = 0) = (maximum_abs_deviation_millic IS NULL)),
    CHECK ((valid_count = 0) = (p95_abs_deviation_millic IS NULL))
) STRICT;

CREATE TABLE equipment_alias_evidence (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    evidence_identity_row_id INTEGER NOT NULL REFERENCES equipment_alias_evidence_identity(row_id),
    revision_number INTEGER NOT NULL CHECK (revision_number >= 1),
    predecessor_evidence_row_id INTEGER,
    equipment_kind TEXT NOT NULL CHECK (equipment_kind IN ('camera','optical_profile')),
    camera_row_id INTEGER REFERENCES camera(row_id),
    optical_profile_row_id INTEGER REFERENCES optical_profile(row_id),
    capture_profile_version_row_id INTEGER NOT NULL REFERENCES capture_profile_version(row_id),
    semantic_field TEXT NOT NULL CHECK (semantic_field IN ('camera','telescope','filter','focal_length','rotator')),
    source_field TEXT NOT NULL,
    normalized_value TEXT NOT NULL,
    first_seen_frame_row_id INTEGER NOT NULL REFERENCES frame_record(row_id),
    review_state TEXT NOT NULL CHECK (review_state IN ('automatic','accepted','rejected')),
    actor_row_id INTEGER NOT NULL REFERENCES spec062_actor(row_id),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    UNIQUE (evidence_identity_row_id, revision_number),
    UNIQUE (predecessor_evidence_row_id),
    UNIQUE (row_id, evidence_identity_row_id),
    CHECK ((equipment_kind = 'camera') = (camera_row_id IS NOT NULL)),
    CHECK ((equipment_kind = 'optical_profile') = (optical_profile_row_id IS NOT NULL)),
    FOREIGN KEY (predecessor_evidence_row_id, evidence_identity_row_id)
        REFERENCES equipment_alias_evidence(row_id, evidence_identity_row_id)
) STRICT;

CREATE TABLE equipment_alias_evidence_head (
    evidence_identity_row_id INTEGER PRIMARY KEY REFERENCES equipment_alias_evidence_identity(row_id),
    head_evidence_row_id INTEGER NOT NULL UNIQUE,
    head_generation INTEGER NOT NULL DEFAULT 0 CHECK (head_generation >= 0),
    FOREIGN KEY (head_evidence_row_id, evidence_identity_row_id)
        REFERENCES equipment_alias_evidence(row_id, evidence_identity_row_id)
) STRICT;

CREATE TABLE equipment_alias_evidence_identity (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    head_revision_row_id INTEGER,
    head_generation INTEGER NOT NULL DEFAULT 0 CHECK (head_generation >= 0),
    created_at TEXT NOT NULL,
    FOREIGN KEY (head_revision_row_id, row_id)
        REFERENCES equipment_alias_evidence(row_id, evidence_identity_row_id)
        DEFERRABLE INITIALLY DEFERRED
) STRICT;

CREATE TABLE events (
    event_id INTEGER PRIMARY KEY AUTOINCREMENT,
    topic TEXT NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('user', 'restore', 'system')),
    emitted_at TEXT NOT NULL,
    payload TEXT NOT NULL  -- JSON
);

CREATE TABLE file_record (
    id TEXT PRIMARY KEY NOT NULL,
    root_id TEXT NOT NULL REFERENCES library_root(id),
    relative_path TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    mtime TEXT NOT NULL,
    content_hash TEXT,
    state TEXT NOT NULL CHECK (state IN ('observed', 'changed', 'classified', 'missing', 'rejected', 'protected')),
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    UNIQUE (root_id, relative_path)
);

CREATE TABLE filesystem_plan (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('organize', 'prepare_source', 'cleanup', 'archive', 'regenerate_artifact')),
    -- items: JSON array of per-item mutation records
    items TEXT NOT NULL DEFAULT '[]',
    state TEXT NOT NULL CHECK (state IN ('draft', 'ready_for_review', 'approved', 'applying', 'paused', 'applied', 'partially_applied', 'failed', 'cancelled', 'discarded')),
    parent_plan_id TEXT REFERENCES filesystem_plan(id),
    created_by TEXT NOT NULL CHECK (created_by IN ('user', 'system')),
    created_at TEXT NOT NULL,
    applied_at TEXT
);

CREATE TABLE filter_label (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    optical_profile_row_id INTEGER NOT NULL REFERENCES optical_profile(row_id),
    state TEXT NOT NULL CHECK (state IN ('captured','absent')),
    normalized_label TEXT,
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    UNIQUE (optical_profile_row_id, state, normalized_label),
    UNIQUE (row_id, optical_profile_row_id),
    CHECK ((state = 'captured') = (normalized_label IS NOT NULL))
) STRICT;

CREATE TABLE filters (
    id            TEXT PRIMARY KEY NOT NULL,
    name          TEXT NOT NULL UNIQUE,
    category      TEXT NOT NULL CHECK (category IN ('narrowband', 'broadband', 'dual_band', 'other', 'custom')),
    auto_detected INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT NOT NULL
);

CREATE TABLE "first_run_state" (
    singleton_id TEXT PRIMARY KEY DEFAULT 'first_run' CHECK (singleton_id = 'first_run'),
    completed_at TEXT,
    last_step    TEXT NOT NULL DEFAULT 'source_folders' CHECK (last_step IN ('source_folders', 'processing_tools', 'catalogs', 'confirm', 'complete')),
    updated_at   TEXT NOT NULL
);

CREATE TABLE flat_family_identity (
    family_row_id INTEGER PRIMARY KEY REFERENCES calibration_family(row_id),
    gain_text TEXT NOT NULL,
    offset_state TEXT NOT NULL CHECK (offset_state IN ('present','absent')),
    offset_value INTEGER,
    binning_state TEXT NOT NULL CHECK (binning_state IN ('present','absent')),
    bin_x INTEGER,
    bin_y INTEGER,
    readout_state TEXT NOT NULL CHECK (readout_state IN ('present','absent')),
    readout_mode TEXT,
    raster_width INTEGER NOT NULL CHECK (raster_width > 0),
    raster_height INTEGER NOT NULL CHECK (raster_height > 0),
    physical_rotator_state TEXT NOT NULL CHECK (physical_rotator_state IN ('verified','absent','unverified')),
    physical_rotator_udeg INTEGER,
    CHECK ((physical_rotator_state = 'verified') = (physical_rotator_udeg IS NOT NULL)),
    CHECK ((offset_state = 'present') = (offset_value IS NOT NULL)),
    CHECK ((binning_state = 'present') = (bin_x IS NOT NULL AND bin_y IS NOT NULL)),
    CHECK ((readout_state = 'present') = (readout_mode IS NOT NULL))
) STRICT;

CREATE TABLE frame_metadata_evidence (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    frame_row_id INTEGER NOT NULL REFERENCES frame_record(row_id),
    revision_number INTEGER NOT NULL CHECK (revision_number >= 1),
    predecessor_evidence_row_id INTEGER,
    detected_kind TEXT NOT NULL CHECK (detected_kind IN ('light','dark','bias','flat')),
    classification_source TEXT NOT NULL DEFAULT 'rule'
        CHECK (classification_source IN ('rule','manual_override','fallback')),
    classification_confidence REAL NOT NULL DEFAULT 1.0
        CHECK (classification_confidence BETWEEN 0.0 AND 1.0),
    canonical_exposure_at_utc TEXT,
    canonical_time_source TEXT,
    local_exposure_text TEXT,
    local_time_parse_state TEXT,
    exposure_us INTEGER CHECK (exposure_us IS NULL OR exposure_us >= 0),
    gain_text TEXT,
    offset_state TEXT NOT NULL CHECK (offset_state IN ('present','absent','invalid','contradictory')),
    offset_value INTEGER,
    binning_state TEXT NOT NULL CHECK (binning_state IN ('present','absent','invalid','contradictory')),
    bin_x INTEGER,
    bin_y INTEGER,
    readout_state TEXT NOT NULL CHECK (readout_state IN ('present','absent','invalid','contradictory')),
    readout_mode TEXT,
    raster_width INTEGER CHECK (raster_width IS NULL OR raster_width > 0),
    raster_height INTEGER CHECK (raster_height IS NULL OR raster_height > 0),
    crop_state TEXT NOT NULL DEFAULT 'absent'
        CHECK (crop_state IN ('reported_full','reported_crop','reported_subframe','absent','invalid','contradictory')),
    crop_payload TEXT,
    parity TEXT CHECK (parity IS NULL OR parity IN ('normal','mirrored')),
    cooling_setpoint_state TEXT NOT NULL DEFAULT 'absent'
        CHECK (cooling_setpoint_state IN ('present','absent','invalid','contradictory')),
    cooling_setpoint_millic INTEGER,
    sensor_temperature_state TEXT NOT NULL DEFAULT 'absent'
        CHECK (sensor_temperature_state IN ('present','absent','invalid','contradictory')),
    sensor_temperature_millic INTEGER,
    camera_reported TEXT,
    telescope_reported TEXT,
    focal_length_reported_um INTEGER CHECK (focal_length_reported_um IS NULL OR focal_length_reported_um > 0),
    focal_length_calculated_um INTEGER CHECK (focal_length_calculated_um IS NULL OR focal_length_calculated_um > 0),
    filter_state TEXT NOT NULL DEFAULT 'absent'
        CHECK (filter_state IN ('present','absent','invalid','contradictory')),
    filter_reported TEXT,
    physical_rotator_state TEXT NOT NULL DEFAULT 'absent'
        CHECK (physical_rotator_state IN ('verified','absent','unverified','invalid','contradictory')),
    physical_rotator_udeg INTEGER,
    physical_rotator_field_id TEXT,
    sky_orientation_state TEXT NOT NULL DEFAULT 'absent'
        CHECK (sky_orientation_state IN ('present','absent','invalid','contradictory')),
    sky_orientation_udeg INTEGER,
    footprint_wkb BLOB,
    footprint_digest TEXT,
    centre_ra_udeg INTEGER CHECK (centre_ra_udeg IS NULL OR centre_ra_udeg BETWEEN 0 AND 360000000),
    centre_dec_udeg INTEGER CHECK (centre_dec_udeg IS NULL OR centre_dec_udeg BETWEEN -90000000 AND 90000000),
    bbox_min_x_ppb INTEGER CHECK (bbox_min_x_ppb BETWEEN -1000000000 AND 1000000000),
    bbox_max_x_ppb INTEGER CHECK (bbox_max_x_ppb BETWEEN -1000000000 AND 1000000000),
    bbox_min_y_ppb INTEGER CHECK (bbox_min_y_ppb BETWEEN -1000000000 AND 1000000000),
    bbox_max_y_ppb INTEGER CHECK (bbox_max_y_ppb BETWEEN -1000000000 AND 1000000000),
    bbox_min_z_ppb INTEGER CHECK (bbox_min_z_ppb BETWEEN -1000000000 AND 1000000000),
    bbox_max_z_ppb INTEGER CHECK (bbox_max_z_ppb BETWEEN -1000000000 AND 1000000000),
    geometry_solver_version TEXT,
    capture_profile_version_row_id INTEGER REFERENCES capture_profile_version(row_id),
    source_payload_json TEXT,
    actor_row_id INTEGER NOT NULL REFERENCES spec062_actor(row_id),
    command_row_id INTEGER NOT NULL REFERENCES command_execution(row_id),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    recorded_at TEXT NOT NULL,
    UNIQUE (frame_row_id, revision_number),
    UNIQUE (predecessor_evidence_row_id),
    UNIQUE (row_id, frame_row_id),
    FOREIGN KEY (predecessor_evidence_row_id, frame_row_id)
        REFERENCES frame_metadata_evidence(row_id, frame_row_id),
    CHECK ((offset_state = 'present') = (offset_value IS NOT NULL)),
    CHECK ((cooling_setpoint_state = 'present') = (cooling_setpoint_millic IS NOT NULL)),
    CHECK ((sensor_temperature_state = 'present') = (sensor_temperature_millic IS NOT NULL)),
    CHECK ((filter_state = 'present') = (filter_reported IS NOT NULL)),
    CHECK ((physical_rotator_state = 'verified') = (physical_rotator_udeg IS NOT NULL)),
    CHECK ((physical_rotator_state = 'verified') = (physical_rotator_field_id IS NOT NULL)),
    CHECK ((sky_orientation_state = 'present') = (sky_orientation_udeg IS NOT NULL)),
    CHECK ((binning_state = 'present') = (bin_x IS NOT NULL AND bin_y IS NOT NULL)),
    CHECK (bin_x IS NULL OR bin_x > 0),
    CHECK (bin_y IS NULL OR bin_y > 0),
    CHECK ((readout_state = 'present') = (readout_mode IS NOT NULL)),
    CHECK ((crop_state IN ('reported_crop','reported_subframe')) = (crop_payload IS NOT NULL)),
    CHECK ((footprint_wkb IS NULL) = (footprint_digest IS NULL)),
    CHECK ((footprint_wkb IS NULL) = (bbox_min_x_ppb IS NULL)),
    CHECK ((footprint_wkb IS NULL) = (bbox_max_x_ppb IS NULL)),
    CHECK ((footprint_wkb IS NULL) = (bbox_min_y_ppb IS NULL)),
    CHECK ((footprint_wkb IS NULL) = (bbox_max_y_ppb IS NULL)),
    CHECK ((footprint_wkb IS NULL) = (bbox_min_z_ppb IS NULL)),
    CHECK ((footprint_wkb IS NULL) = (bbox_max_z_ppb IS NULL)),
    CHECK (bbox_min_x_ppb IS NULL OR bbox_min_x_ppb <= bbox_max_x_ppb),
    CHECK (bbox_min_y_ppb IS NULL OR bbox_min_y_ppb <= bbox_max_y_ppb),
    CHECK (bbox_min_z_ppb IS NULL OR bbox_min_z_ppb <= bbox_max_z_ppb)
) STRICT;

CREATE TABLE frame_metadata_evidence_head (
    frame_row_id INTEGER PRIMARY KEY REFERENCES frame_record(row_id),
    head_evidence_row_id INTEGER NOT NULL UNIQUE,
    head_generation INTEGER NOT NULL DEFAULT 0 CHECK (head_generation >= 0),
    FOREIGN KEY (head_evidence_row_id, frame_row_id)
        REFERENCES frame_metadata_evidence(row_id, frame_row_id)
) STRICT;

CREATE TABLE frame_record (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    file_row_id INTEGER NOT NULL UNIQUE REFERENCES spec062_file_identity(row_id),
    content_fingerprint TEXT,
    byte_size INTEGER NOT NULL CHECK (byte_size >= 0),
    captured_metadata_digest TEXT NOT NULL,
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL
) STRICT;

CREATE TABLE framing (
    id                      TEXT NOT NULL PRIMARY KEY,
    project_id              TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    -- Nullable: FR-018 forbids OBJECT/panel-name attribution; a framing may
    -- exist before target resolution completes.
    target_id               TEXT REFERENCES canonical_target(id),
    -- Optic-train identity (Q12/Q17 grouping key).
    optic_train_key         TEXT NOT NULL,
    -- Representative FOV-relative pointing (circular-mean of members, R11a).
    pointing_ra_deg         REAL NOT NULL,
    pointing_dec_deg        REAL NOT NULL,
    rotation_deg            REAL NOT NULL,
    -- Snapshot of the tunable tolerance the clustering pass used (FR-014).
    -- `tolerance_pointing` is unit-agnostic (FOV-relative fraction, or the
    -- absolute-degree no-FOV fallback per research R11a); never an exact key.
    tolerance_pointing      REAL NOT NULL,
    tolerance_rotation_deg  REAL NOT NULL,
    -- 'suggested' | 'user_adjusted' (FR-015). Re-derivation MUST NEVER modify
    -- a 'user_adjusted' framing.
    clustering              TEXT NOT NULL DEFAULT 'suggested'
                                 CHECK (clustering IN ('suggested', 'user_adjusted')),
    created_at              TEXT NOT NULL,
    updated_at              TEXT NOT NULL
);

CREATE TABLE framing_session (
    framing_id  TEXT NOT NULL REFERENCES framing(id) ON DELETE CASCADE,
    -- UNIQUE: a light session belongs to at most one framing.
    session_id  TEXT NOT NULL UNIQUE REFERENCES acquisition_session(id) ON DELETE CASCADE,
    added_at    TEXT NOT NULL,
    PRIMARY KEY (framing_id, session_id)
);

CREATE TABLE group_action_session_snapshot (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    source_panel_revision_row_id INTEGER REFERENCES panel_group_revision(row_id),
    source_mosaic_revision_row_id INTEGER REFERENCES mosaic_revision(row_id),
    source_digest TEXT NOT NULL,
    session_count INTEGER NOT NULL CHECK (session_count >= 0),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    CHECK ((source_panel_revision_row_id IS NULL) <> (source_mosaic_revision_row_id IS NULL))
) STRICT;

CREATE TABLE group_action_snapshot_session (
    snapshot_row_id INTEGER NOT NULL REFERENCES group_action_session_snapshot(row_id),
    session_row_id INTEGER NOT NULL REFERENCES session(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (snapshot_row_id, session_row_id),
    UNIQUE (snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE inbox_classification_breakdown (
    id                  TEXT        NOT NULL PRIMARY KEY,
    inbox_item_id       TEXT        NOT NULL
                            REFERENCES inbox_items(id) ON DELETE CASCADE,
    kind                TEXT        NOT NULL
                            CHECK (kind IN ('light','dark','bias','flat','dark_flat')),
    count               INTEGER     NOT NULL DEFAULT 0,
    destination_preview TEXT,                        -- preview path from active pattern (spec 015)
    sample_files        TEXT        NOT NULL DEFAULT '[]'  -- JSON array of up to 10 filenames
);

CREATE TABLE "inbox_classification_evidence" (
    id                  TEXT        NOT NULL PRIMARY KEY,
    inbox_item_id       TEXT        NOT NULL
                            REFERENCES inbox_items(id) ON DELETE CASCADE,
    relative_file_path  TEXT        NOT NULL,
    frame_type          TEXT
                            CHECK (frame_type IN ('light','dark','bias','flat','dark_flat')),
    evidence_source     TEXT        NOT NULL DEFAULT 'none'
                            CHECK (evidence_source IN (
                                'imagetyp_header',
                                'xisf_property',
                                'manual_override',
                                'none'
                            )),
    raw_value           TEXT,
    unclassified        INTEGER     NOT NULL DEFAULT 0  CHECK (unclassified IN (0,1)),
    -- manual_override retained: the "correct classification" action still uses it
    -- until T068 (field-agnostic reclassify) lands. After T068, the frameType
    -- override in inbox_file_overrides supersedes this column.
    manual_override     TEXT
                            CHECK (manual_override IN ('light','dark','bias','flat','dark_flat')),
    -- from migration 0042
    is_master           INTEGER     NOT NULL DEFAULT 0  CHECK (is_master IN (0,1)),
    master_detector     TEXT,
    -- override_stale kept (per-file staleness flag used by UI)
    override_stale      INTEGER     NOT NULL DEFAULT 0
);

CREATE TABLE "inbox_classifications" (
    inbox_item_id           TEXT        NOT NULL PRIMARY KEY
                                REFERENCES inbox_items(id) ON DELETE CASCADE,
    result                  TEXT        NOT NULL
                                CHECK (result IN ('classified', 'unclassified')),
    frame_type              TEXT
                                CHECK (frame_type IN ('light','dark','bias','flat','dark_flat')),
    computed_at             TEXT        NOT NULL,
    content_signature       TEXT        NOT NULL,
    unclassified_file_count INTEGER     NOT NULL DEFAULT 0
);

CREATE TABLE inbox_file_metadata (
    id                   TEXT PRIMARY KEY NOT NULL,
    inbox_item_id        TEXT NOT NULL REFERENCES inbox_items(id) ON DELETE CASCADE,
    relative_file_path   TEXT NOT NULL,
    -- extracted image-header fields (all nullable — not all file types carry all fields)
    filter               TEXT,
    exposure_s           REAL,
    gain                 TEXT,
    binning_x            INTEGER,
    binning_y            INTEGER,
    temperature_c        REAL,
    object               TEXT,
    date_obs             TEXT,
    instrume             TEXT,
    telescop             TEXT,
    naxis1               INTEGER,
    naxis2               INTEGER,
    stack_count          INTEGER,
    -- cheap per-file identity for override staleness (R-4; no full-content hash)
    file_size_bytes      INTEGER,
    file_mtime           TEXT, offset          INTEGER, set_temp_c      REAL, ccd_temp_c      REAL, ra_deg          REAL, dec_deg         REAL, rotator_angle_deg REAL, rotator_name    TEXT, sky_rotation_deg REAL, readout_mode    TEXT, focal_length_mm REAL, pixel_size_um   REAL, observer_lat    REAL, observer_long   REAL, observer_elev   REAL, date_loc        TEXT, date_end        TEXT, mjd_avg         REAL, mjd_obs         REAL, wcs_ra_deg       REAL, wcs_dec_deg      REAL, wcs_rotation_deg REAL,
    UNIQUE (inbox_item_id, relative_file_path)
);

CREATE TABLE inbox_file_overrides (
    id                  TEXT        NOT NULL PRIMARY KEY,
    source_group_id     TEXT        NOT NULL
                            REFERENCES inbox_source_groups(id) ON DELETE CASCADE,
    relative_file_path  TEXT        NOT NULL,           -- file within the source group
    property_key        TEXT        NOT NULL,           -- from R-13 property registry
    value               TEXT        NOT NULL,           -- typed per registry; stored as text/JSON
    file_size_bytes     INTEGER,                        -- staleness identity (R-4)
    file_mtime          TEXT,                           -- staleness identity (R-4)
    override_stale      INTEGER     NOT NULL DEFAULT 0, -- 1 when file size/mtime changed
    set_at              TEXT        NOT NULL,
    UNIQUE (source_group_id, relative_file_path, property_key)
);

CREATE TABLE inbox_ingestion_operation (
    operation_row_id INTEGER PRIMARY KEY REFERENCES session_materialization_operation(row_id),
    inbox_plan_result_snapshot_row_id INTEGER NOT NULL UNIQUE
        REFERENCES inbox_materialization_plan_result_snapshot(row_id),
    approved_plan_digest TEXT NOT NULL,
    approved_by_actor_row_id INTEGER NOT NULL REFERENCES spec062_actor(row_id),
    approved_at TEXT NOT NULL
) STRICT;

CREATE TABLE "inbox_items" (
    id                  TEXT        NOT NULL PRIMARY KEY,
    root_id             TEXT        NOT NULL,
    relative_path       TEXT        NOT NULL,
    source_group_id     TEXT        REFERENCES inbox_source_groups(id) ON DELETE SET NULL,
    group_key           TEXT        NOT NULL DEFAULT '',  -- empty sentinel for legacy rows
    group_label         TEXT,
    frame_type          TEXT
                            CHECK (frame_type IN ('light','dark','bias','flat','dark_flat')),
    file_count          INTEGER     NOT NULL DEFAULT 0,
    discovered_at       TEXT        NOT NULL,
    last_scanned_at     TEXT        NOT NULL,
    content_signature   TEXT,
    state               TEXT        NOT NULL DEFAULT 'pending_classification'
                            CHECK (state IN (
                                'pending_classification',
                                'classified',
                                'plan_open',
                                'resolved'
                            )),
    lane                TEXT        NOT NULL DEFAULT 'fits'
                            CHECK (lane IN ('fits', 'video')),
    -- columns from migration 0043
    format              TEXT,
    is_master_item      INTEGER     NOT NULL DEFAULT 0
                            CHECK (is_master_item IN (0, 1)),
    master_frame_type   TEXT,
    master_filter       TEXT,
    master_exposure_s   REAL, needs_review INTEGER NOT NULL DEFAULT 0
        CHECK (needs_review IN (0, 1)),
    UNIQUE (root_id, relative_path, group_key)
);

CREATE TABLE inbox_materialization_plan_result_snapshot (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    plan_row_id INTEGER NOT NULL REFERENCES spec062_inbox_materialization_plan(row_id),
    plan_revision INTEGER NOT NULL CHECK (plan_revision >= 1),
    config_revision_row_id INTEGER NOT NULL REFERENCES spec062_config_revision(row_id),
    input_evidence_revision INTEGER NOT NULL CHECK (input_evidence_revision >= 1),
    proposed_session_count INTEGER NOT NULL CHECK (proposed_session_count >= 0),
    frame_count INTEGER NOT NULL CHECK (frame_count >= 0),
    blocked_frame_count INTEGER NOT NULL CHECK (blocked_frame_count >= 0),
    canonical_digest TEXT NOT NULL,
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    UNIQUE (plan_row_id, plan_revision)
) STRICT;

CREATE TABLE inbox_plan_links (
    inbox_item_id   TEXT        NOT NULL PRIMARY KEY
                        REFERENCES inbox_items(id) ON DELETE CASCADE,
    plan_id         TEXT        NOT NULL REFERENCES plans(id) ON DELETE CASCADE,
    linked_at       TEXT        NOT NULL
);

CREATE TABLE inbox_plan_result_blocked_frame (
    snapshot_row_id INTEGER NOT NULL REFERENCES inbox_materialization_plan_result_snapshot(row_id),
    frame_row_id INTEGER NOT NULL REFERENCES frame_record(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    reason_code TEXT NOT NULL,
    PRIMARY KEY (snapshot_row_id, frame_row_id),
    UNIQUE (snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE inbox_plan_result_proposed_session (
    row_id INTEGER PRIMARY KEY,
    snapshot_row_id INTEGER NOT NULL REFERENCES inbox_materialization_plan_result_snapshot(row_id),
    proposed_session_key TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('light','dark','bias','flat')),
    site_resolution_revision_row_id INTEGER NOT NULL REFERENCES acquisition_site_resolution_revision(row_id),
    identity_digest TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    frame_count INTEGER NOT NULL CHECK (frame_count > 0),
    UNIQUE (snapshot_row_id, proposed_session_key),
    UNIQUE (snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE inbox_plan_result_proposed_session_frame (
    proposed_session_row_id INTEGER NOT NULL REFERENCES inbox_plan_result_proposed_session(row_id),
    frame_row_id INTEGER NOT NULL REFERENCES frame_record(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (proposed_session_row_id, frame_row_id),
    UNIQUE (proposed_session_row_id, ordinal)
) STRICT;

CREATE TABLE inbox_source_groups (
    id                  TEXT        NOT NULL PRIMARY KEY,
    root_id             TEXT        NOT NULL,           -- FK to library root (Constitution §I)
    relative_path       TEXT        NOT NULL,           -- leaf folder relative to root
    discovered_at       TEXT        NOT NULL,
    last_scanned_at     TEXT        NOT NULL,
    content_signature   TEXT,                           -- folder-level signature (partial 65 KB read); lazy
    format              TEXT,                           -- dominant format: fits/xisf/video/mixed/NULL
    lane                TEXT,                           -- move-vs-catalogue (from source organization_state)
    child_count         INTEGER     NOT NULL DEFAULT 0, file_count INTEGER NOT NULL DEFAULT 0, -- single-type sub-items from this group
    UNIQUE (root_id, relative_path)
);

CREATE TABLE ingest_resolution (
    id          TEXT    NOT NULL PRIMARY KEY,             -- UUID
    image_id    TEXT    NOT NULL REFERENCES file_record(id) ON DELETE CASCADE,
    object_raw  TEXT    NOT NULL,                         -- verbatim FITS OBJECT value
    state       TEXT    NOT NULL CHECK (state IN ('pending', 'resolved', 'unresolved')),
    target_id   TEXT    REFERENCES canonical_target(id),  -- set when resolved
    attempts    INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE ingestion_settings (
    singleton_id                    TEXT PRIMARY KEY DEFAULT 'default' CHECK (singleton_id = 'default'),
    watcher_enabled                 INTEGER NOT NULL DEFAULT 1,
    scan_on_startup                 INTEGER NOT NULL DEFAULT 1,
    follow_symlinks                 INTEGER NOT NULL DEFAULT 0,
    follow_junctions                INTEGER NOT NULL DEFAULT 0,
    eager_hashing                   INTEGER NOT NULL DEFAULT 0,
    metadata_extraction             INTEGER NOT NULL DEFAULT 1,
    exposure_grouping_tolerance_s   REAL NOT NULL DEFAULT 2.0,
    temperature_grouping_tolerance_c REAL NOT NULL DEFAULT 5.0,
    default_filter                  TEXT,
    updated_at                      TEXT NOT NULL
);

CREATE TABLE library_root (
    id TEXT PRIMARY KEY NOT NULL,
    label TEXT NOT NULL,
    current_path TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('local', 'external', 'network')),
    state TEXT NOT NULL CHECK (state IN ('active', 'missing', 'disabled', 'reconnect_required')),
    last_seen_at TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE light_session_identity (
    session_row_id INTEGER PRIMARY KEY REFERENCES session(row_id),
    optical_profile_row_id INTEGER NOT NULL REFERENCES optical_profile(row_id),
    filter_label_row_id INTEGER NOT NULL,
    exposure_us INTEGER NOT NULL CHECK (exposure_us >= 0),
    gain_text TEXT NOT NULL,
    offset_state TEXT NOT NULL CHECK (offset_state IN ('present','absent')),
    offset_value INTEGER,
    binning_state TEXT NOT NULL CHECK (binning_state IN ('present','absent')),
    bin_x INTEGER,
    bin_y INTEGER,
    readout_state TEXT NOT NULL CHECK (readout_state IN ('present','absent')),
    readout_mode TEXT,
    raster_width INTEGER NOT NULL CHECK (raster_width > 0),
    raster_height INTEGER NOT NULL CHECK (raster_height > 0),
    crop_state TEXT NOT NULL CHECK (crop_state IN ('reported_full','reported_crop','reported_subframe','absent')),
    crop_payload TEXT,
    parity TEXT NOT NULL CHECK (parity IN ('normal','mirrored')),
    footprint_digest TEXT NOT NULL,
    representative_orientation_udeg INTEGER NOT NULL,
    CHECK ((offset_state = 'present') = (offset_value IS NOT NULL)),
    CHECK ((binning_state = 'present') = (bin_x IS NOT NULL AND bin_y IS NOT NULL)),
    CHECK (bin_x IS NULL OR bin_x > 0),
    CHECK (bin_y IS NULL OR bin_y > 0),
    CHECK ((readout_state = 'present') = (readout_mode IS NOT NULL)),
    CHECK ((crop_state IN ('reported_crop','reported_subframe')) = (crop_payload IS NOT NULL)),
    FOREIGN KEY (filter_label_row_id, optical_profile_row_id)
        REFERENCES filter_label(row_id, optical_profile_row_id)
) STRICT;

CREATE TABLE manifests (
    id          TEXT    NOT NULL PRIMARY KEY,   -- UUID (C4)
    project_id  TEXT    NOT NULL REFERENCES projects(id),
    reason      TEXT    NOT NULL CHECK (reason IN (
                    'created','source_change','lifecycle_transition',
                    'cleanup_applied','workflow_run'
                )),
    timestamp   TEXT    NOT NULL,               -- RFC-3339 UTC
    path        TEXT    NOT NULL,               -- project-relative, e.g. notes/manifest-…md
    version     INTEGER NOT NULL DEFAULT 1,     -- front-matter schema version
    body_json   TEXT    NOT NULL DEFAULT '{}'   -- JSON serialisation of ManifestBody
);

CREATE TABLE matching_settings_camera_policy (
    settings_revision_row_id INTEGER NOT NULL REFERENCES matching_settings_revision(row_id),
    camera_row_id INTEGER NOT NULL REFERENCES camera(row_id),
    kind TEXT NOT NULL CHECK (kind IN ('dark','bias')),
    fresh_age_days INTEGER NOT NULL CHECK (fresh_age_days BETWEEN 1 AND 1795),
    red_age_days INTEGER NOT NULL CHECK (red_age_days BETWEEN 31 AND 1825),
    PRIMARY KEY (settings_revision_row_id, camera_row_id, kind),
    CHECK (red_age_days >= fresh_age_days + 30)
) STRICT;

CREATE TABLE matching_settings_head (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    head_revision_row_id INTEGER NOT NULL UNIQUE REFERENCES matching_settings_revision(row_id),
    head_generation INTEGER NOT NULL DEFAULT 0 CHECK (head_generation >= 0)
) STRICT;

CREATE TABLE matching_settings_revision (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    revision_number INTEGER NOT NULL UNIQUE CHECK (revision_number >= 1),
    predecessor_revision_row_id INTEGER UNIQUE REFERENCES matching_settings_revision(row_id),
    same_session_coverage_min_ppm INTEGER NOT NULL CHECK (same_session_coverage_min_ppm BETWEEN 900000 AND 995000),
    same_session_centre_max_ppm INTEGER NOT NULL CHECK (same_session_centre_max_ppm BETWEEN 5000 AND 50000),
    same_session_rotation_max_udeg INTEGER NOT NULL CHECK (same_session_rotation_max_udeg BETWEEN 250000 AND 3000000),
    sibling_coverage_min_ppm INTEGER NOT NULL CHECK (sibling_coverage_min_ppm BETWEEN 800000 AND 950000),
    sibling_centre_max_ppm INTEGER NOT NULL CHECK (sibling_centre_max_ppm BETWEEN 20000 AND 150000),
    sibling_rotation_max_udeg INTEGER NOT NULL CHECK (sibling_rotation_max_udeg BETWEEN 1000000 AND 15000000),
    mosaic_overlap_min_ppm INTEGER NOT NULL CHECK (mosaic_overlap_min_ppm BETWEEN 10000 AND 200000),
    mosaic_overlap_max_ppm INTEGER NOT NULL CHECK (mosaic_overlap_max_ppm BETWEEN 200000 AND 600000),
    dark_thermal_moderate_millic INTEGER NOT NULL CHECK (dark_thermal_moderate_millic BETWEEN 100 AND 2000),
    dark_thermal_severe_millic INTEGER NOT NULL CHECK (dark_thermal_severe_millic BETWEEN 500 AND 5000),
    flat_orientation_normal_udeg INTEGER NOT NULL CHECK (flat_orientation_normal_udeg BETWEEN 500000 AND 5000000),
    flat_orientation_red_udeg INTEGER NOT NULL CHECK (flat_orientation_red_udeg BETWEEN 500001 AND 15000000),
    flat_red_age_days INTEGER NOT NULL CHECK (flat_red_age_days BETWEEN 7 AND 365),
    canonical_digest TEXT NOT NULL UNIQUE,
    actor_row_id INTEGER NOT NULL REFERENCES spec062_actor(row_id),
    command_row_id INTEGER NOT NULL REFERENCES command_execution(row_id),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    CHECK (sibling_coverage_min_ppm <= same_session_coverage_min_ppm),
    CHECK (sibling_centre_max_ppm >= same_session_centre_max_ppm),
    CHECK (sibling_rotation_max_udeg >= same_session_rotation_max_udeg),
    CHECK (mosaic_overlap_min_ppm < mosaic_overlap_max_ppm),
    CHECK (mosaic_overlap_max_ppm <= sibling_coverage_min_ppm - 100000),
    CHECK (dark_thermal_severe_millic >= dark_thermal_moderate_millic + 500),
    CHECK (flat_orientation_red_udeg > flat_orientation_normal_udeg)
) STRICT;

CREATE TABLE materialization_install_intent (
    plan_item_row_id INTEGER PRIMARY KEY,
    plan_row_id INTEGER NOT NULL REFERENCES materialization_update_plan(row_id),
    collision_key TEXT NOT NULL,
    canonical_destination TEXT NOT NULL,
    approved_fingerprint TEXT NOT NULL,
    ownership_token TEXT NOT NULL,
    command_row_id INTEGER NOT NULL REFERENCES command_execution(row_id),
    lease_owner TEXT NOT NULL,
    lease_generation INTEGER NOT NULL CHECK (lease_generation >= 0),
    state TEXT NOT NULL CHECK (state IN ('prepared','installed','journaled')),
    updated_at TEXT NOT NULL,
    UNIQUE (plan_item_row_id, plan_row_id),
    UNIQUE (plan_item_row_id, plan_row_id, command_row_id, lease_owner, lease_generation),
    UNIQUE (plan_row_id, collision_key),
    FOREIGN KEY (plan_item_row_id, plan_row_id)
        REFERENCES materialization_plan_entry(row_id, plan_row_id),
    FOREIGN KEY (command_row_id, lease_owner, lease_generation)
        REFERENCES command_execution(row_id, lease_owner, lease_generation)
        DEFERRABLE INITIALLY DEFERRED
) STRICT;

CREATE TABLE materialization_item_journal (
    plan_item_row_id INTEGER PRIMARY KEY,
    plan_row_id INTEGER NOT NULL REFERENCES materialization_update_plan(row_id),
    operation_command_row_id INTEGER NOT NULL REFERENCES command_execution(row_id),
    resulting_entry_row_id INTEGER NOT NULL UNIQUE REFERENCES materialized_entry(row_id),
    destination_root_row_id INTEGER NOT NULL REFERENCES spec062_destination_root(row_id),
    relative_path TEXT NOT NULL,
    content_fingerprint TEXT NOT NULL,
    lease_owner TEXT NOT NULL,
    lease_generation INTEGER NOT NULL CHECK (lease_generation >= 0),
    completed_at TEXT NOT NULL,
    UNIQUE (plan_row_id, destination_root_row_id, relative_path),
    FOREIGN KEY (plan_item_row_id, plan_row_id)
        REFERENCES materialization_plan_entry(row_id, plan_row_id),
    FOREIGN KEY (plan_item_row_id, plan_row_id, operation_command_row_id, lease_owner, lease_generation)
        REFERENCES materialization_install_intent(
            plan_item_row_id, plan_row_id, command_row_id, lease_owner, lease_generation
        )
) STRICT;

CREATE TABLE materialization_plan_entry (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    plan_row_id INTEGER NOT NULL REFERENCES materialization_update_plan(row_id),
    session_row_id INTEGER NOT NULL REFERENCES session(row_id),
    frame_row_id INTEGER NOT NULL REFERENCES frame_record(row_id),
    destination_root_row_id INTEGER NOT NULL REFERENCES spec062_destination_root(row_id),
    relative_path TEXT NOT NULL,
    approved_fingerprint TEXT NOT NULL,
    collision_state TEXT NOT NULL CHECK (collision_state IN ('clear','collision')),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    UNIQUE (plan_row_id, ordinal),
    UNIQUE (row_id, plan_row_id),
    UNIQUE (plan_row_id, destination_root_row_id, relative_path)
) STRICT;

CREATE TABLE materialization_plan_overlay_mapping (
    plan_row_id INTEGER NOT NULL REFERENCES materialization_update_plan(row_id),
    predecessor_entry_row_id INTEGER NOT NULL REFERENCES materialized_entry(row_id),
    replacement_plan_entry_row_id INTEGER REFERENCES materialization_plan_entry(row_id),
    exclusion_reason_code TEXT,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (plan_row_id, predecessor_entry_row_id),
    UNIQUE (plan_row_id, ordinal),
    CHECK ((replacement_plan_entry_row_id IS NULL) <> (exclusion_reason_code IS NULL))
) STRICT;

CREATE TABLE materialization_update_plan (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    project_row_id INTEGER NOT NULL REFERENCES spec062_project(row_id),
    base_snapshot_row_id INTEGER,
    target_membership_revision_row_id INTEGER NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('draft','approved','applying','stopped','applied','failed','discarded','stale')),
    content_digest TEXT NOT NULL,
    session_count INTEGER NOT NULL CHECK (session_count BETWEEN 0 AND 500),
    item_count INTEGER NOT NULL CHECK (item_count BETWEEN 0 AND 100000),
    source_frame_count INTEGER NOT NULL CHECK (source_frame_count BETWEEN 0 AND 100000),
    source_byte_count INTEGER NOT NULL CHECK (source_byte_count BETWEEN 0 AND 17592186044416),
    remaining_session_count INTEGER NOT NULL CHECK (remaining_session_count >= 0),
    next_session_row_id INTEGER REFERENCES session(row_id),
    actor_row_id INTEGER NOT NULL REFERENCES spec062_actor(row_id),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    FOREIGN KEY (base_snapshot_row_id, project_row_id)
        REFERENCES project_materialization_snapshot(row_id, project_row_id),
    FOREIGN KEY (target_membership_revision_row_id, project_row_id)
        REFERENCES project_membership_revision(row_id, project_row_id)
) STRICT;

CREATE TABLE materialization_update_plan_session (
    plan_row_id INTEGER NOT NULL REFERENCES materialization_update_plan(row_id),
    session_row_id INTEGER NOT NULL REFERENCES session(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (plan_row_id, session_row_id),
    UNIQUE (plan_row_id, ordinal)
) STRICT;

CREATE TABLE materialized_entry (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    project_row_id INTEGER NOT NULL REFERENCES spec062_project(row_id),
    first_snapshot_row_id INTEGER NOT NULL,
    source_session_row_id INTEGER NOT NULL REFERENCES session(row_id),
    source_frame_row_id INTEGER NOT NULL REFERENCES frame_record(row_id),
    destination_root_row_id INTEGER NOT NULL REFERENCES spec062_destination_root(row_id),
    relative_path TEXT NOT NULL,
    content_fingerprint TEXT,
    created_by_plan_row_id INTEGER NOT NULL REFERENCES materialization_update_plan(row_id),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    UNIQUE (project_row_id, destination_root_row_id, relative_path),
    FOREIGN KEY (first_snapshot_row_id, project_row_id)
        REFERENCES project_materialization_snapshot(row_id, project_row_id)
) STRICT;

CREATE TABLE mosaic (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    canonical_target_row_id INTEGER REFERENCES spec062_target(row_id),
    cross_target_association_row_id INTEGER REFERENCES cross_target_association(row_id),
    status TEXT NOT NULL CHECK (status IN ('active','retired')),
    head_revision_row_id INTEGER,
    head_generation INTEGER NOT NULL DEFAULT 0 CHECK (head_generation >= 0),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    retired_at TEXT,
    CHECK ((canonical_target_row_id IS NULL) <> (cross_target_association_row_id IS NULL)),
    CHECK ((status = 'retired') = (retired_at IS NOT NULL)),
    FOREIGN KEY (head_revision_row_id, row_id)
        REFERENCES mosaic_revision(row_id, mosaic_row_id)
        DEFERRABLE INITIALLY DEFERRED
) STRICT;

CREATE TABLE mosaic_edge_evidence (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    left_panel_revision_row_id INTEGER NOT NULL REFERENCES panel_group_revision(row_id),
    right_panel_revision_row_id INTEGER NOT NULL REFERENCES panel_group_revision(row_id),
    overlap_ppm INTEGER NOT NULL CHECK (overlap_ppm BETWEEN 0 AND 1000000),
    centre_separation_udeg INTEGER NOT NULL CHECK (centre_separation_udeg >= 0),
    residual_orientation_udeg INTEGER NOT NULL,
    parity_match INTEGER NOT NULL CHECK (parity_match IN (0,1)),
    evidence_digest TEXT NOT NULL,
    config_revision_row_id INTEGER NOT NULL REFERENCES spec062_config_revision(row_id),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    UNIQUE (left_panel_revision_row_id, right_panel_revision_row_id, evidence_digest),
    CHECK (left_panel_revision_row_id < right_panel_revision_row_id)
) STRICT;

CREATE TABLE mosaic_edge_invalidation (
    edge_evidence_row_id INTEGER NOT NULL REFERENCES mosaic_edge_evidence(row_id),
    applied_plan_revision_row_id INTEGER NOT NULL REFERENCES reclassification_plan_revision(row_id),
    reason_code TEXT NOT NULL,
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    PRIMARY KEY (edge_evidence_row_id, applied_plan_revision_row_id)
) STRICT;

CREATE TABLE mosaic_head_history (
    mosaic_row_id INTEGER NOT NULL REFERENCES mosaic(row_id),
    generation INTEGER NOT NULL CHECK (generation >= 0),
    head_revision_row_id INTEGER NOT NULL,
    accepted_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    retired_sequence INTEGER REFERENCES repository_change(sequence),
    PRIMARY KEY (mosaic_row_id, generation),
    FOREIGN KEY (head_revision_row_id, mosaic_row_id)
        REFERENCES mosaic_revision(row_id, mosaic_row_id),
    CHECK (retired_sequence IS NULL OR retired_sequence > accepted_sequence)
) STRICT;

CREATE TABLE mosaic_lineage (
    predecessor_mosaic_row_id INTEGER NOT NULL REFERENCES mosaic(row_id),
    successor_mosaic_row_id INTEGER NOT NULL REFERENCES mosaic(row_id),
    kind TEXT NOT NULL CHECK (kind IN ('split','merge','identity_change')),
    proposal_row_id INTEGER NOT NULL REFERENCES relation_proposal(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    PRIMARY KEY (predecessor_mosaic_row_id, successor_mosaic_row_id),
    CHECK (predecessor_mosaic_row_id <> successor_mosaic_row_id)
) STRICT;

CREATE TABLE mosaic_object_evidence (
    mosaic_revision_row_id INTEGER NOT NULL REFERENCES mosaic_revision(row_id),
    canonical_object_row_id INTEGER NOT NULL REFERENCES spec062_canonical_object(row_id),
    extent_kind TEXT NOT NULL CHECK (extent_kind IN ('point','extended')),
    intersection_state TEXT NOT NULL CHECK (intersection_state IN ('partial','full')),
    covered_fraction_ppm INTEGER CHECK (covered_fraction_ppm BETWEEN 0 AND 1000000),
    union_geometry_digest TEXT NOT NULL,
    catalogue_version TEXT NOT NULL,
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    PRIMARY KEY (mosaic_revision_row_id, canonical_object_row_id),
    CHECK ((intersection_state = 'partial') = (covered_fraction_ppm IS NOT NULL))
) STRICT;

CREATE TABLE mosaic_object_panel_evidence (
    mosaic_revision_row_id INTEGER NOT NULL,
    canonical_object_row_id INTEGER NOT NULL,
    panel_revision_row_id INTEGER NOT NULL REFERENCES panel_group_revision(row_id),
    session_row_id INTEGER NOT NULL REFERENCES session(row_id),
    containment_state TEXT NOT NULL CHECK (containment_state IN ('partial','full')),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (mosaic_revision_row_id, canonical_object_row_id, panel_revision_row_id, session_row_id),
    UNIQUE (mosaic_revision_row_id, canonical_object_row_id, ordinal),
    FOREIGN KEY (mosaic_revision_row_id, canonical_object_row_id)
        REFERENCES mosaic_object_evidence(mosaic_revision_row_id, canonical_object_row_id)
) STRICT;

CREATE TABLE mosaic_revision (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    mosaic_row_id INTEGER NOT NULL REFERENCES mosaic(row_id),
    revision_number INTEGER NOT NULL CHECK (revision_number >= 1),
    parent_revision_row_id INTEGER,
    proposal_row_id INTEGER NOT NULL REFERENCES relation_proposal(row_id),
    config_revision_row_id INTEGER NOT NULL REFERENCES spec062_config_revision(row_id),
    actor_row_id INTEGER NOT NULL REFERENCES spec062_actor(row_id),
    reason_code TEXT NOT NULL,
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    UNIQUE (mosaic_row_id, revision_number),
    UNIQUE (parent_revision_row_id),
    UNIQUE (row_id, mosaic_row_id),
    FOREIGN KEY (parent_revision_row_id, mosaic_row_id)
        REFERENCES mosaic_revision(row_id, mosaic_row_id)
) STRICT;

CREATE TABLE mosaic_revision_edge (
    mosaic_revision_row_id INTEGER NOT NULL REFERENCES mosaic_revision(row_id),
    edge_evidence_row_id INTEGER NOT NULL REFERENCES mosaic_edge_evidence(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (mosaic_revision_row_id, edge_evidence_row_id),
    UNIQUE (mosaic_revision_row_id, ordinal)
) STRICT;

CREATE TABLE mosaic_revision_panel (
    mosaic_revision_row_id INTEGER NOT NULL REFERENCES mosaic_revision(row_id),
    panel_revision_row_id INTEGER NOT NULL REFERENCES panel_group_revision(row_id),
    panel_group_row_id INTEGER NOT NULL REFERENCES panel_group(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (mosaic_revision_row_id, panel_revision_row_id),
    UNIQUE (mosaic_revision_row_id, panel_group_row_id),
    UNIQUE (mosaic_revision_row_id, ordinal),
    FOREIGN KEY (panel_revision_row_id, panel_group_row_id)
        REFERENCES panel_group_revision(row_id, panel_group_row_id)
) STRICT;

CREATE TABLE onboarding_flags (
    singleton_id       INTEGER NOT NULL PRIMARY KEY CHECK (singleton_id = 1),
    orientation_done_at TEXT,
    section_hidden_at   TEXT,
    sidebar_collapsed   INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE onboarding_state (
    item_id TEXT NOT NULL PRIMARY KEY,
    state   TEXT NOT NULL CHECK (state IN ('unchecked', 'auto_checked', 'manually_checked', 'dismissed')),
    at      TEXT NOT NULL,
    source  TEXT NOT NULL CHECK (source IN ('seed', 'event', 'user'))
);

CREATE TABLE operation_states (
    id TEXT PRIMARY KEY NOT NULL,
    operation_type TEXT NOT NULL,
    status TEXT NOT NULL,
    progress_current INTEGER,
    progress_total INTEGER,
    current_message TEXT,
    started_at TEXT,
    finished_at TEXT,
    resume_token TEXT,
    error_code TEXT,
    error_message TEXT,
    updated_at TEXT NOT NULL
);

CREATE TABLE optical_profile (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    representative_camera_row_id INTEGER REFERENCES camera(row_id),
    representative_focal_length_um INTEGER NOT NULL CHECK (representative_focal_length_um > 0),
    representative_raster_width INTEGER NOT NULL CHECK (representative_raster_width > 0),
    representative_raster_height INTEGER NOT NULL CHECK (representative_raster_height > 0),
    representative_pixel_size_nm INTEGER CHECK (representative_pixel_size_nm IS NULL OR representative_pixel_size_nm > 0),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL
) STRICT;

CREATE TABLE optical_trains (
    id              TEXT PRIMARY KEY NOT NULL,
    name            TEXT NOT NULL,
    telescope_id    TEXT REFERENCES telescopes(id),
    camera_id       TEXT REFERENCES cameras(id),
    focal_length_mm INTEGER NOT NULL,
    created_at      TEXT NOT NULL
);

CREATE TABLE outbox_event (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    command_row_id INTEGER NOT NULL REFERENCES command_execution(row_id),
    event_ordinal INTEGER NOT NULL CHECK (event_ordinal >= 0),
    operation_row_id INTEGER REFERENCES session_materialization_operation(row_id),
    proposal_row_id INTEGER REFERENCES relation_proposal(row_id),
    session_row_id INTEGER REFERENCES session(row_id),
    panel_group_row_id INTEGER REFERENCES panel_group(row_id),
    mosaic_row_id INTEGER REFERENCES mosaic(row_id),
    project_row_id INTEGER REFERENCES spec062_project(row_id),
    handoff_row_id INTEGER REFERENCES calibration_handoff(row_id),
    event_type TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    occurred_at TEXT NOT NULL,
    published_at TEXT,
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    last_error TEXT,
    UNIQUE (command_row_id, event_ordinal),
    CHECK (
        (operation_row_id IS NOT NULL)
        + (proposal_row_id IS NOT NULL)
        + (session_row_id IS NOT NULL)
        + (panel_group_row_id IS NOT NULL)
        + (mosaic_row_id IS NOT NULL)
        + (project_row_id IS NOT NULL)
        + (handoff_row_id IS NOT NULL) = 1
    )
) STRICT;

CREATE TABLE panel_group (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    canonical_target_row_id INTEGER REFERENCES spec062_target(row_id),
    cross_target_association_row_id INTEGER REFERENCES cross_target_association(row_id),
    status TEXT NOT NULL CHECK (status IN ('active','retired')),
    head_revision_row_id INTEGER,
    head_generation INTEGER NOT NULL DEFAULT 0 CHECK (head_generation >= 0),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    retired_at TEXT,
    CHECK ((canonical_target_row_id IS NULL) <> (cross_target_association_row_id IS NULL)),
    CHECK ((status = 'retired') = (retired_at IS NOT NULL)),
    FOREIGN KEY (head_revision_row_id, row_id)
        REFERENCES panel_group_revision(row_id, panel_group_row_id)
        DEFERRABLE INITIALLY DEFERRED
) STRICT;

CREATE TABLE panel_group_head_history (
    panel_group_row_id INTEGER NOT NULL REFERENCES panel_group(row_id),
    generation INTEGER NOT NULL CHECK (generation >= 0),
    head_revision_row_id INTEGER NOT NULL,
    accepted_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    retired_sequence INTEGER REFERENCES repository_change(sequence),
    PRIMARY KEY (panel_group_row_id, generation),
    FOREIGN KEY (head_revision_row_id, panel_group_row_id)
        REFERENCES panel_group_revision(row_id, panel_group_row_id),
    CHECK (retired_sequence IS NULL OR retired_sequence > accepted_sequence)
) STRICT;

CREATE TABLE panel_group_lineage (
    predecessor_group_row_id INTEGER NOT NULL REFERENCES panel_group(row_id),
    successor_group_row_id INTEGER NOT NULL REFERENCES panel_group(row_id),
    kind TEXT NOT NULL CHECK (kind IN ('split','merge','identity_change')),
    proposal_row_id INTEGER NOT NULL REFERENCES relation_proposal(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    PRIMARY KEY (predecessor_group_row_id, successor_group_row_id),
    CHECK (predecessor_group_row_id <> successor_group_row_id)
) STRICT;

CREATE TABLE panel_group_revision (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    panel_group_row_id INTEGER NOT NULL REFERENCES panel_group(row_id),
    revision_number INTEGER NOT NULL CHECK (revision_number >= 1),
    parent_revision_row_id INTEGER,
    representative_session_row_id INTEGER NOT NULL,
    representative_session_kind TEXT NOT NULL DEFAULT 'light' CHECK (representative_session_kind = 'light'),
    proposal_row_id INTEGER REFERENCES relation_proposal(row_id),
    config_revision_row_id INTEGER NOT NULL REFERENCES spec062_config_revision(row_id),
    actor_row_id INTEGER NOT NULL REFERENCES spec062_actor(row_id),
    reason_code TEXT NOT NULL,
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    UNIQUE (panel_group_row_id, revision_number),
    UNIQUE (parent_revision_row_id),
    UNIQUE (row_id, panel_group_row_id),
    FOREIGN KEY (parent_revision_row_id, panel_group_row_id)
        REFERENCES panel_group_revision(row_id, panel_group_row_id),
    FOREIGN KEY (representative_session_row_id, representative_session_kind)
        REFERENCES session(row_id, kind)
) STRICT;

CREATE TABLE panel_revision_session (
    panel_revision_row_id INTEGER NOT NULL REFERENCES panel_group_revision(row_id),
    session_row_id INTEGER NOT NULL,
    session_kind TEXT NOT NULL DEFAULT 'light' CHECK (session_kind = 'light'),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (panel_revision_row_id, session_row_id),
    UNIQUE (panel_revision_row_id, ordinal),
    FOREIGN KEY (session_row_id, session_kind) REFERENCES session(row_id, kind)
) STRICT;

CREATE TABLE plan_apply_events (
    id           TEXT PRIMARY KEY NOT NULL,
    run_id       TEXT NOT NULL REFERENCES plan_apply_runs(id),
    plan_id      TEXT NOT NULL,
    item_id      TEXT,   -- NULL for plan-level events
    prior_state  TEXT NOT NULL,
    new_state    TEXT NOT NULL,
    at           TEXT NOT NULL,
    -- Failure detail (set when new_state = 'failed' or 'stale')
    failure_code          TEXT,
    failure_message       TEXT,
    failure_recoverable   INTEGER,  -- 0/1 boolean
    -- Rollback detail (set when a rollback was attempted)
    rollback_attempted    INTEGER,  -- 0/1 boolean
    rollback_outcome      TEXT CHECK (rollback_outcome IN ('succeeded','failed','not_applicable') OR rollback_outcome IS NULL),
    rollback_message      TEXT
);

CREATE TABLE plan_apply_runs (
    id               TEXT PRIMARY KEY NOT NULL,
    plan_id          TEXT NOT NULL REFERENCES plans(id),
    approval_token   TEXT NOT NULL,
    started_at       TEXT NOT NULL,
    ended_at         TEXT,
    terminal_state   TEXT CHECK (terminal_state IN ('applied','partially_applied','failed','cancelled','paused')),
    items_total      INTEGER NOT NULL DEFAULT 0,
    items_applied    INTEGER NOT NULL DEFAULT 0,
    items_failed     INTEGER NOT NULL DEFAULT 0,
    items_skipped    INTEGER NOT NULL DEFAULT 0,
    items_cancelled  INTEGER NOT NULL DEFAULT 0,
    items_pending    INTEGER NOT NULL DEFAULT 0,
    pause_reason     TEXT    -- last pause reason: 'volume.unavailable' | 'disk.full' | 'item.stale'
);

CREATE TABLE "plan_items" (
    id                          TEXT PRIMARY KEY NOT NULL,
    plan_id                     TEXT NOT NULL REFERENCES plans(id) ON DELETE CASCADE,
    item_index                  INTEGER NOT NULL,
    name                        TEXT NOT NULL,
    action                      TEXT NOT NULL CHECK (action IN (
                                    'move', 'archive', 'delete', 'link', 'write',
                                    'mkdir', 'write_manifest', 'catalogue'
                                )),
    from_root_id                TEXT,
    from_relative_path          TEXT NOT NULL DEFAULT '',
    to_root_id                  TEXT,
    to_relative_path            TEXT NOT NULL DEFAULT '',
    reason                      TEXT NOT NULL DEFAULT '',
    protection                  TEXT NOT NULL DEFAULT 'normal'
                                    CHECK (protection IN ('normal', 'protected')),
    linked_entity               TEXT,
    item_state                  TEXT NOT NULL DEFAULT 'pending'
                                    CHECK (item_state IN (
                                        'pending', 'applying', 'succeeded',
                                        'failed', 'skipped', 'cancelled'
                                    )),
    failure_reason              TEXT,
    provenance                  TEXT,
    approved_mtime              TEXT,
    approved_size_bytes         INTEGER,
    archive_path                TEXT,
    created_at                  TEXT NOT NULL,
    -- added by migration 0015
    item_stale                  INTEGER NOT NULL DEFAULT 0,
    -- added by migration 0039
    source_id                   TEXT,
    category                    TEXT,
    requires_destructive_confirm INTEGER NOT NULL DEFAULT 0,
    resolved_pattern            TEXT,
    -- added by migration 0041
    destructive_confirmed       INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE "plans" (
    id                       TEXT    NOT NULL PRIMARY KEY,
    number                   INTEGER NOT NULL,
    title                    TEXT    NOT NULL,
    origin                   TEXT    NOT NULL CHECK (origin IN ('project','inbox','cleanup','archive','restore','source_view','manifest','prepared_view_removal','prepared_view_regeneration','prepared_view_generation','calibration_master_archive','calibration_master_restore')),
    origin_path              TEXT,
    state                    TEXT    NOT NULL CHECK (state IN ('draft','ready_for_review','approved','applying','paused','applied','partially_applied','failed','cancelled','discarded')),
    plan_type                TEXT    NOT NULL CHECK (plan_type IN ('split','restructure','cleanup','archive','restore','source_map','project_create','source_view_removal','source_view_regeneration','source_view_generation','calibration_master_archive','calibration_master_restore')),
    destructive_destination  TEXT    NOT NULL DEFAULT 'archive'
                               CHECK (destructive_destination IN ('archive','trash')),
    parent_plan_id           TEXT    REFERENCES "plans"(id),
    items_total              INTEGER NOT NULL DEFAULT 0,
    items_applied            INTEGER NOT NULL DEFAULT 0,
    items_failed             INTEGER NOT NULL DEFAULT 0,
    items_skipped            INTEGER NOT NULL DEFAULT 0,
    items_cancelled          INTEGER NOT NULL DEFAULT 0,
    items_pending            INTEGER NOT NULL DEFAULT 0,
    total_bytes_required     INTEGER NOT NULL DEFAULT 0,
    approval_token           TEXT,
    approved_at              TEXT,
    discarded_at             TEXT,
    created_at               TEXT    NOT NULL,
    chosen_framing_id        TEXT    REFERENCES framing(id),
    destination_root         TEXT
);

CREATE TABLE prepared_source_view (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES project(id),
    state TEXT NOT NULL CHECK (state IN ('not_created', 'planned', 'ready', 'stale', 'retired')),
    created_at TEXT NOT NULL
);

CREATE TABLE prepared_source_view_items (
    id                   TEXT    NOT NULL PRIMARY KEY,  -- UUID
    view_id              TEXT    NOT NULL REFERENCES prepared_source_views(id),
    inventory_item_id    TEXT    NOT NULL,              -- FK to inventory items (no FK constraint — inventory may be missing)
    view_relative_path   TEXT    NOT NULL,              -- path under project workspace
    materialization      TEXT    NOT NULL CHECK (materialization IN (
                             'symlink', 'junction', 'copy', 'hardlink'
                         )),
    last_observed_state  TEXT    NOT NULL DEFAULT 'present' CHECK (last_observed_state IN (
                             'present', 'missing', 'changed_kind', 'diverged', 'hash_diverged'
                         ))
);

CREATE TABLE prepared_source_views (
    id           TEXT    NOT NULL PRIMARY KEY,          -- UUID
    project_id   TEXT    NOT NULL REFERENCES projects(id),
    kind         TEXT    NOT NULL CHECK (kind IN (
                     'symlink', 'junction', 'copy', 'hardlink'
                 )),
    state        TEXT    NOT NULL DEFAULT 'current' CHECK (state IN (
                     'current', 'stale', 'missing', 'removed',
                     'failed', 'kind_diverged'
                 )),
    created_at   TEXT    NOT NULL,                      -- RFC-3339 UTC
    removed_at   TEXT                                   -- set when ViewRemovalPlan applied
);

CREATE TABLE processing_artifact (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT REFERENCES project(id),
    file_record_id TEXT NOT NULL REFERENCES file_record(id),
    kind TEXT NOT NULL CHECK (kind IN ('master', 'integration', 'drizzle', 'manifest', 'other')),
    tool TEXT,
    staleness TEXT NOT NULL CHECK (staleness IN ('current', 'stale', 'regenerating')),
    created_at TEXT NOT NULL
);

CREATE TABLE processing_artifacts (
    id                         TEXT    NOT NULL PRIMARY KEY,  -- UUID (C4)
    project_id                 TEXT    NOT NULL,
    tool_launch_id             TEXT    NULL      REFERENCES tool_launches(id),
    path                       TEXT    NOT NULL,              -- project-relative
    kind                       TEXT    NOT NULL CHECK (kind IN ('intermediate','master','final')),
    tool                       TEXT    NOT NULL,              -- workflow-profile tool id
    detected_at                TEXT    NOT NULL,              -- RFC-3339; app-clock (R-AppClock)
    last_seen_at               TEXT    NOT NULL,              -- updated on every reconciliation pass
    state                      TEXT    NOT NULL DEFAULT 'present'
                                       CHECK (state IN ('present','missing','user_resolved_missing')),
    classification_confidence  REAL    NOT NULL CHECK (classification_confidence >= 0.0 AND classification_confidence <= 1.0),
    classification_source      TEXT    NOT NULL CHECK (classification_source IN ('rule','manual_override','fallback')),
    size_bytes                 INTEGER NOT NULL,
    file_mtime                 TEXT    NOT NULL,              -- stored; NOT used for attribution
    content_hash               TEXT    NULL,                  -- hex SHA-256; updated in-place on rerun (A8)
    UNIQUE (project_id, path)
);

CREATE TABLE project (
    id          TEXT PRIMARY KEY NOT NULL,
    name        TEXT NOT NULL,
    target_id   TEXT NOT NULL REFERENCES target(id),
    session_ids TEXT NOT NULL DEFAULT '[]',
    created_at  TEXT NOT NULL
);

CREATE TABLE project_channels (
    project_id          TEXT        NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    label               TEXT        NOT NULL,
    -- "inferred" | "manual"
    source              TEXT        NOT NULL DEFAULT 'inferred',
    added_at            TEXT        NOT NULL,

    PRIMARY KEY (project_id, label)
);

CREATE TABLE project_manifest (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    project_row_id INTEGER NOT NULL REFERENCES spec062_project(row_id),
    version_number INTEGER NOT NULL CHECK (version_number >= 1),
    predecessor_manifest_row_id INTEGER,
    materialization_snapshot_row_id INTEGER NOT NULL REFERENCES project_materialization_snapshot(row_id),
    command_row_id INTEGER NOT NULL REFERENCES command_execution(row_id),
    actor_row_id INTEGER NOT NULL REFERENCES spec062_actor(row_id),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    UNIQUE (project_row_id, version_number),
    UNIQUE (predecessor_manifest_row_id),
    UNIQUE (row_id, project_row_id),
    FOREIGN KEY (predecessor_manifest_row_id, project_row_id)
        REFERENCES project_manifest(row_id, project_row_id)
) STRICT;

CREATE TABLE project_manifest_entry (
    manifest_row_id INTEGER NOT NULL REFERENCES project_manifest(row_id),
    entry_row_id INTEGER NOT NULL REFERENCES materialized_entry(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (manifest_row_id, entry_row_id),
    UNIQUE (manifest_row_id, ordinal)
) STRICT;

CREATE TABLE project_manifest_overlay (
    manifest_row_id INTEGER NOT NULL REFERENCES project_manifest(row_id),
    overlay_row_id INTEGER NOT NULL REFERENCES correction_overlay(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (manifest_row_id, overlay_row_id),
    UNIQUE (manifest_row_id, ordinal)
) STRICT;

CREATE TABLE project_materialization_head_history (
    project_row_id INTEGER NOT NULL REFERENCES spec062_project(row_id),
    generation INTEGER NOT NULL CHECK (generation >= 0),
    head_snapshot_row_id INTEGER NOT NULL,
    accepted_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    retired_sequence INTEGER REFERENCES repository_change(sequence),
    PRIMARY KEY (project_row_id, generation),
    FOREIGN KEY (head_snapshot_row_id, project_row_id)
        REFERENCES project_materialization_snapshot(row_id, project_row_id),
    CHECK (retired_sequence IS NULL OR retired_sequence > accepted_sequence)
) STRICT;

CREATE TABLE project_materialization_snapshot (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    project_row_id INTEGER NOT NULL REFERENCES spec062_project(row_id),
    membership_revision_row_id INTEGER NOT NULL,
    predecessor_snapshot_row_id INTEGER,
    applied_plan_row_id INTEGER NOT NULL UNIQUE REFERENCES materialization_update_plan(row_id),
    entry_count INTEGER NOT NULL CHECK (entry_count >= 0),
    session_count INTEGER NOT NULL CHECK (session_count >= 0),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    UNIQUE (row_id, project_row_id),
    UNIQUE (predecessor_snapshot_row_id),
    FOREIGN KEY (membership_revision_row_id, project_row_id)
        REFERENCES project_membership_revision(row_id, project_row_id),
    FOREIGN KEY (predecessor_snapshot_row_id, project_row_id)
        REFERENCES project_materialization_snapshot(row_id, project_row_id)
) STRICT;

CREATE TABLE project_materialization_snapshot_entry (
    snapshot_row_id INTEGER NOT NULL REFERENCES project_materialization_snapshot(row_id),
    entry_row_id INTEGER NOT NULL REFERENCES materialized_entry(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (snapshot_row_id, entry_row_id),
    UNIQUE (snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE project_materialization_snapshot_session (
    snapshot_row_id INTEGER NOT NULL REFERENCES project_materialization_snapshot(row_id),
    session_row_id INTEGER NOT NULL REFERENCES session(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (snapshot_row_id, session_row_id),
    UNIQUE (snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE project_membership_head_history (
    project_row_id INTEGER NOT NULL REFERENCES spec062_project(row_id),
    generation INTEGER NOT NULL CHECK (generation >= 0),
    head_revision_row_id INTEGER NOT NULL,
    accepted_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    retired_sequence INTEGER REFERENCES repository_change(sequence),
    PRIMARY KEY (project_row_id, generation),
    FOREIGN KEY (head_revision_row_id, project_row_id)
        REFERENCES project_membership_revision(row_id, project_row_id),
    CHECK (retired_sequence IS NULL OR retired_sequence > accepted_sequence)
) STRICT;

CREATE TABLE project_membership_revision (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    project_row_id INTEGER NOT NULL REFERENCES spec062_project(row_id),
    revision_number INTEGER NOT NULL CHECK (revision_number >= 1),
    parent_revision_row_id INTEGER,
    proposal_row_id INTEGER REFERENCES relation_proposal(row_id),
    actor_row_id INTEGER NOT NULL REFERENCES spec062_actor(row_id),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    UNIQUE (project_row_id, revision_number),
    UNIQUE (parent_revision_row_id),
    UNIQUE (row_id, project_row_id),
    FOREIGN KEY (parent_revision_row_id, project_row_id)
        REFERENCES project_membership_revision(row_id, project_row_id)
) STRICT;

CREATE TABLE project_membership_revision_session (
    revision_row_id INTEGER NOT NULL REFERENCES project_membership_revision(row_id),
    session_row_id INTEGER NOT NULL REFERENCES session(row_id),
    pin_revision INTEGER NOT NULL CHECK (pin_revision >= 1),
    source TEXT NOT NULL CHECK (source IN ('explicit_add','explicit_replacement','project_creation')),
    replaces_session_row_id INTEGER REFERENCES session(row_id),
    applied_reclassification_plan_revision_row_id INTEGER REFERENCES reclassification_plan_revision(row_id),
    pinned_by_actor_row_id INTEGER NOT NULL REFERENCES spec062_actor(row_id),
    pinned_at TEXT NOT NULL,
    PRIMARY KEY (revision_row_id, session_row_id),
    CHECK ((source = 'explicit_replacement') = (replaces_session_row_id IS NOT NULL)),
    CHECK ((source = 'explicit_replacement') = (applied_reclassification_plan_revision_row_id IS NOT NULL))
) STRICT;

CREATE TABLE project_notes (
    id          TEXT    NOT NULL PRIMARY KEY,   -- UUID (C4)
    project_id  TEXT    NOT NULL UNIQUE REFERENCES projects(id),
    updated_at  TEXT    NOT NULL,               -- RFC-3339 UTC
    content     TEXT    NOT NULL DEFAULT ''     -- ≤16 384 UTF-8 bytes enforced in app layer
);

CREATE TABLE project_sources (
    -- stable row identifier (UUID string)
    id                  TEXT        NOT NULL PRIMARY KEY,
    project_id          TEXT        NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    -- hard reference to the Inventory/AcquisitionSession row
    inventory_session_id TEXT       NOT NULL,
    -- snapshot fields copied from the Inventory row at link time
    name_snapshot       TEXT        NOT NULL DEFAULT '',
    frames_snapshot     INTEGER     NOT NULL DEFAULT 0,
    filter_snapshot     TEXT        NOT NULL DEFAULT '',
    exposure_snapshot   TEXT        NOT NULL DEFAULT '',
    linked_at           TEXT        NOT NULL,

    UNIQUE(project_id, inventory_session_id)
);

CREATE TABLE projects (
    id                  TEXT        NOT NULL PRIMARY KEY,
    name                TEXT        NOT NULL,
    -- "PixInsight" | "Siril" | "Planetary Suite"
    tool                TEXT        NOT NULL,
    -- lifecycle states per spec 009 / domain_core::lifecycle::project::ProjectState
    lifecycle           TEXT        NOT NULL DEFAULT 'setup_incomplete',
    -- library-root-relative path (Constitution I: roots modelled separately)
    path                TEXT        NOT NULL,
    notes               TEXT,
    -- channel drift flag: set to 1 when sources are added after last channel review
    channel_drift       INTEGER     NOT NULL DEFAULT 0,
    created_at          TEXT        NOT NULL,
    updated_at          TEXT        NOT NULL, canonical_target_id TEXT REFERENCES canonical_target(id), blocked_reason_kind TEXT, blocked_reason_note TEXT, archived_via_plan_id TEXT, is_mosaic INTEGER NOT NULL DEFAULT 0,

    UNIQUE(name),
    UNIQUE(path)
);

CREATE TABLE proposal_measurement (
    proposal_row_id INTEGER NOT NULL REFERENCES relation_proposal(row_id),
    measurement_key TEXT NOT NULL,
    integer_value INTEGER NOT NULL,
    unit TEXT NOT NULL,
    comparison TEXT NOT NULL CHECK (comparison IN ('lt','lte','eq','gte','gt','inside')),
    threshold_min INTEGER,
    threshold_max INTEGER,
    outcome TEXT NOT NULL CHECK (outcome IN ('pass','warn','fail')),
    source_evidence_digest TEXT NOT NULL,
    PRIMARY KEY (proposal_row_id, measurement_key)
) STRICT;

CREATE TABLE proposal_mosaic_edge (
    proposal_row_id INTEGER NOT NULL REFERENCES relation_proposal(row_id),
    edge_evidence_row_id INTEGER NOT NULL REFERENCES mosaic_edge_evidence(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (proposal_row_id, edge_evidence_row_id),
    UNIQUE (proposal_row_id, ordinal)
) STRICT;

CREATE TABLE proposal_mosaic_lineage (
    proposal_row_id INTEGER NOT NULL REFERENCES relation_proposal(row_id),
    predecessor_mosaic_row_id INTEGER NOT NULL REFERENCES mosaic(row_id),
    successor_mosaic_row_id INTEGER NOT NULL REFERENCES mosaic(row_id),
    kind TEXT NOT NULL CHECK (kind IN ('split','merge','identity_change')),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (proposal_row_id, predecessor_mosaic_row_id, successor_mosaic_row_id),
    UNIQUE (proposal_row_id, ordinal),
    CHECK (predecessor_mosaic_row_id <> successor_mosaic_row_id)
) STRICT;

CREATE TABLE proposal_mosaic_membership (
    proposal_row_id INTEGER NOT NULL REFERENCES relation_proposal(row_id),
    destination_mosaic_row_id INTEGER REFERENCES mosaic(row_id),
    panel_revision_row_id INTEGER NOT NULL REFERENCES panel_group_revision(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (proposal_row_id, panel_revision_row_id),
    UNIQUE (proposal_row_id, ordinal)
) STRICT;

CREATE TABLE proposal_mosaic_revision_input (
    proposal_row_id INTEGER NOT NULL REFERENCES relation_proposal(row_id),
    mosaic_revision_row_id INTEGER NOT NULL REFERENCES mosaic_revision(row_id),
    role TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (proposal_row_id, mosaic_revision_row_id, role),
    UNIQUE (proposal_row_id, ordinal)
) STRICT;

CREATE TABLE proposal_panel_lineage (
    proposal_row_id INTEGER NOT NULL REFERENCES relation_proposal(row_id),
    predecessor_group_row_id INTEGER NOT NULL REFERENCES panel_group(row_id),
    successor_group_row_id INTEGER NOT NULL REFERENCES panel_group(row_id),
    kind TEXT NOT NULL CHECK (kind IN ('split','merge','identity_change')),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (proposal_row_id, predecessor_group_row_id, successor_group_row_id),
    UNIQUE (proposal_row_id, ordinal),
    CHECK (predecessor_group_row_id <> successor_group_row_id)
) STRICT;

CREATE TABLE proposal_panel_membership (
    proposal_row_id INTEGER NOT NULL REFERENCES relation_proposal(row_id),
    destination_group_row_id INTEGER REFERENCES panel_group(row_id),
    session_row_id INTEGER NOT NULL REFERENCES session(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (proposal_row_id, session_row_id),
    UNIQUE (proposal_row_id, ordinal)
) STRICT;

CREATE TABLE proposal_panel_revision_input (
    proposal_row_id INTEGER NOT NULL REFERENCES relation_proposal(row_id),
    panel_revision_row_id INTEGER NOT NULL REFERENCES panel_group_revision(row_id),
    role TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (proposal_row_id, panel_revision_row_id, role),
    UNIQUE (proposal_row_id, ordinal)
) STRICT;

CREATE TABLE proposal_project_revision_input (
    proposal_row_id INTEGER NOT NULL REFERENCES relation_proposal(row_id),
    project_membership_revision_row_id INTEGER NOT NULL REFERENCES project_membership_revision(row_id),
    role TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (proposal_row_id, project_membership_revision_row_id, role),
    UNIQUE (proposal_row_id, ordinal)
) STRICT;

CREATE TABLE proposal_session_input (
    proposal_row_id INTEGER NOT NULL REFERENCES relation_proposal(row_id),
    session_row_id INTEGER NOT NULL REFERENCES session(row_id),
    role TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (proposal_row_id, session_row_id, role),
    UNIQUE (proposal_row_id, ordinal)
) STRICT;

CREATE TABLE proposal_target_scope (
    proposal_row_id INTEGER NOT NULL REFERENCES relation_proposal(row_id),
    canonical_target_row_id INTEGER NOT NULL REFERENCES spec062_target(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (proposal_row_id, canonical_target_row_id),
    UNIQUE (proposal_row_id, ordinal)
) STRICT;

CREATE TABLE protection_defaults (
    scope      TEXT NOT NULL,
    key        TEXT NOT NULL,
    value      TEXT NOT NULL,  -- JSON-encoded value
    updated_at TEXT NOT NULL,
    PRIMARY KEY (scope, key)
);

CREATE TABLE provenance_history_archive (
    id TEXT PRIMARY KEY NOT NULL,
    asset_type TEXT NOT NULL,
    asset_id TEXT NOT NULL,
    field_path TEXT NOT NULL,
    origin TEXT NOT NULL CHECK (origin IN ('observed', 'inferred', 'reviewed', 'generated', 'planned', 'applied')),
    value TEXT NOT NULL,  -- JSON
    captured_at TEXT NOT NULL,
    source_id TEXT,
    replaced_by TEXT,
    archived_at TEXT NOT NULL
);

CREATE TABLE reclassification_apply_created_session (
    snapshot_row_id INTEGER NOT NULL REFERENCES reclassification_apply_result_snapshot(row_id),
    session_row_id INTEGER NOT NULL REFERENCES session(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (snapshot_row_id, session_row_id),
    UNIQUE (snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE reclassification_apply_invalidated_edge (
    snapshot_row_id INTEGER NOT NULL REFERENCES reclassification_apply_result_snapshot(row_id),
    edge_evidence_row_id INTEGER NOT NULL REFERENCES mosaic_edge_evidence(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (snapshot_row_id, edge_evidence_row_id),
    UNIQUE (snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE reclassification_apply_panel_lineage (
    snapshot_row_id INTEGER NOT NULL REFERENCES reclassification_apply_result_snapshot(row_id),
    predecessor_group_row_id INTEGER NOT NULL REFERENCES panel_group(row_id),
    successor_group_row_id INTEGER NOT NULL REFERENCES panel_group(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (snapshot_row_id, predecessor_group_row_id, successor_group_row_id),
    UNIQUE (snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE reclassification_apply_panel_revision (
    snapshot_row_id INTEGER NOT NULL REFERENCES reclassification_apply_result_snapshot(row_id),
    panel_revision_row_id INTEGER NOT NULL REFERENCES panel_group_revision(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (snapshot_row_id, panel_revision_row_id),
    UNIQUE (snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE reclassification_apply_project_proposal (
    snapshot_row_id INTEGER NOT NULL REFERENCES reclassification_apply_result_snapshot(row_id),
    proposal_row_id INTEGER NOT NULL REFERENCES relation_proposal(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (snapshot_row_id, proposal_row_id),
    UNIQUE (snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE reclassification_apply_result_snapshot (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    plan_result_snapshot_row_id INTEGER NOT NULL UNIQUE REFERENCES reclassification_plan_result_snapshot(row_id),
    operation_row_id INTEGER NOT NULL UNIQUE REFERENCES session_materialization_operation(row_id),
    created_session_count INTEGER NOT NULL CHECK (created_session_count >= 0),
    accepted_panel_count INTEGER NOT NULL CHECK (accepted_panel_count >= 0),
    retirement_count INTEGER NOT NULL CHECK (retirement_count >= 0),
    lineage_count INTEGER NOT NULL CHECK (lineage_count >= 0),
    invalidated_edge_count INTEGER NOT NULL CHECK (invalidated_edge_count >= 0),
    project_proposal_count INTEGER NOT NULL CHECK (project_proposal_count >= 0),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL
) STRICT;

CREATE TABLE reclassification_apply_retired_panel_group (
    snapshot_row_id INTEGER NOT NULL REFERENCES reclassification_apply_result_snapshot(row_id),
    panel_group_row_id INTEGER NOT NULL REFERENCES panel_group(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (snapshot_row_id, panel_group_row_id),
    UNIQUE (snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE reclassification_plan (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    head_revision_row_id INTEGER,
    head_generation INTEGER NOT NULL DEFAULT 0 CHECK (head_generation >= 0),
    created_at TEXT NOT NULL,
    FOREIGN KEY (head_revision_row_id, row_id)
        REFERENCES reclassification_plan_revision(row_id, plan_row_id)
        DEFERRABLE INITIALLY DEFERRED
) STRICT;

CREATE TABLE reclassification_plan_edge_consequence (
    plan_revision_row_id INTEGER NOT NULL REFERENCES reclassification_plan_revision(row_id),
    edge_evidence_row_id INTEGER NOT NULL REFERENCES mosaic_edge_evidence(row_id),
    reason_code TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (plan_revision_row_id, edge_evidence_row_id),
    UNIQUE (plan_revision_row_id, ordinal)
) STRICT;

CREATE TABLE reclassification_plan_input (
    plan_revision_row_id INTEGER NOT NULL REFERENCES reclassification_plan_revision(row_id),
    frame_row_id INTEGER NOT NULL REFERENCES frame_record(row_id),
    evidence_row_id INTEGER NOT NULL,
    source_panel_revision_row_id INTEGER REFERENCES panel_group_revision(row_id),
    matching_settings_revision_row_id INTEGER NOT NULL REFERENCES matching_settings_revision(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (plan_revision_row_id, frame_row_id),
    UNIQUE (plan_revision_row_id, ordinal),
    FOREIGN KEY (evidence_row_id, frame_row_id)
        REFERENCES frame_metadata_evidence(row_id, frame_row_id)
) STRICT;

CREATE TABLE reclassification_plan_output (
    row_id INTEGER PRIMARY KEY,
    plan_revision_row_id INTEGER NOT NULL REFERENCES reclassification_plan_revision(row_id),
    replacement_key TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('light','dark','bias','flat')),
    identity_digest TEXT NOT NULL,
    equipment_resolution_row_id INTEGER NOT NULL REFERENCES session_equipment_resolution(row_id),
    frame_count INTEGER NOT NULL CHECK (frame_count > 0),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    UNIQUE (plan_revision_row_id, replacement_key),
    UNIQUE (plan_revision_row_id, ordinal)
) STRICT;

CREATE TABLE reclassification_plan_output_frame (
    output_row_id INTEGER NOT NULL REFERENCES reclassification_plan_output(row_id),
    frame_row_id INTEGER NOT NULL REFERENCES frame_record(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (output_row_id, frame_row_id),
    UNIQUE (output_row_id, ordinal)
) STRICT;

CREATE TABLE reclassification_plan_panel_consequence (
    plan_revision_row_id INTEGER NOT NULL REFERENCES reclassification_plan_revision(row_id),
    source_panel_revision_row_id INTEGER NOT NULL REFERENCES panel_group_revision(row_id),
    action TEXT NOT NULL CHECK (action IN ('retain','replace','retire','split','merge')),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (plan_revision_row_id, source_panel_revision_row_id),
    UNIQUE (plan_revision_row_id, ordinal)
) STRICT;

CREATE TABLE reclassification_plan_project_consequence (
    plan_revision_row_id INTEGER NOT NULL REFERENCES reclassification_plan_revision(row_id),
    project_membership_revision_row_id INTEGER NOT NULL REFERENCES project_membership_revision(row_id),
    source_session_row_id INTEGER NOT NULL REFERENCES session(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (plan_revision_row_id, project_membership_revision_row_id, source_session_row_id),
    UNIQUE (plan_revision_row_id, ordinal)
) STRICT;

CREATE TABLE reclassification_plan_result_frame (
    snapshot_row_id INTEGER NOT NULL REFERENCES reclassification_plan_result_snapshot(row_id),
    output_row_id INTEGER NOT NULL REFERENCES reclassification_plan_output(row_id),
    frame_row_id INTEGER NOT NULL REFERENCES frame_record(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (snapshot_row_id, output_row_id, frame_row_id),
    UNIQUE (snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE reclassification_plan_result_lineage (
    snapshot_row_id INTEGER NOT NULL REFERENCES reclassification_plan_result_snapshot(row_id),
    predecessor_panel_group_row_id INTEGER NOT NULL REFERENCES panel_group(row_id),
    successor_panel_group_public_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind = 'identity_change'),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (snapshot_row_id, predecessor_panel_group_row_id, successor_panel_group_public_id),
    UNIQUE (snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE reclassification_plan_result_panel_consequence (
    snapshot_row_id INTEGER NOT NULL REFERENCES reclassification_plan_result_snapshot(row_id),
    source_panel_revision_row_id INTEGER NOT NULL REFERENCES panel_group_revision(row_id),
    destination_panel_group_public_id TEXT,
    destination_panel_revision_public_id TEXT,
    action TEXT NOT NULL CHECK (action IN ('retain','replace','retire','split','merge')),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (snapshot_row_id, source_panel_revision_row_id),
    UNIQUE (snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE reclassification_plan_result_project_consequence (
    snapshot_row_id INTEGER NOT NULL REFERENCES reclassification_plan_result_snapshot(row_id),
    project_membership_revision_row_id INTEGER NOT NULL REFERENCES project_membership_revision(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (snapshot_row_id, project_membership_revision_row_id),
    UNIQUE (snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE reclassification_plan_result_project_replacement (
    snapshot_row_id INTEGER NOT NULL REFERENCES reclassification_plan_result_snapshot(row_id),
    project_membership_revision_row_id INTEGER NOT NULL REFERENCES project_membership_revision(row_id),
    source_session_row_id INTEGER NOT NULL REFERENCES session(row_id),
    replacement_output_row_id INTEGER NOT NULL REFERENCES reclassification_plan_output(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (snapshot_row_id, project_membership_revision_row_id, source_session_row_id, replacement_output_row_id),
    UNIQUE (snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE reclassification_plan_result_retirement (
    snapshot_row_id INTEGER NOT NULL REFERENCES reclassification_plan_result_snapshot(row_id),
    predecessor_panel_group_row_id INTEGER NOT NULL REFERENCES panel_group(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (snapshot_row_id, predecessor_panel_group_row_id),
    UNIQUE (snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE reclassification_plan_result_session (
    snapshot_row_id INTEGER NOT NULL REFERENCES reclassification_plan_result_snapshot(row_id),
    output_row_id INTEGER NOT NULL REFERENCES reclassification_plan_output(row_id),
    destination_session_public_id TEXT NOT NULL,
    destination_panel_group_public_id TEXT,
    destination_panel_revision_public_id TEXT,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (snapshot_row_id, output_row_id),
    UNIQUE (snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE reclassification_plan_result_snapshot (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    plan_revision_row_id INTEGER NOT NULL UNIQUE REFERENCES reclassification_plan_revision(row_id),
    basis_digest TEXT NOT NULL,
    replacement_session_count INTEGER NOT NULL CHECK (replacement_session_count >= 0),
    frame_count INTEGER NOT NULL CHECK (frame_count >= 0),
    panel_consequence_count INTEGER NOT NULL CHECK (panel_consequence_count >= 0),
    retirement_count INTEGER NOT NULL CHECK (retirement_count >= 0),
    lineage_count INTEGER NOT NULL CHECK (lineage_count >= 0),
    stale_edge_count INTEGER NOT NULL CHECK (stale_edge_count >= 0),
    project_consequence_count INTEGER NOT NULL CHECK (project_consequence_count >= 0),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL
) STRICT;

CREATE TABLE reclassification_plan_result_stale_edge (
    snapshot_row_id INTEGER NOT NULL REFERENCES reclassification_plan_result_snapshot(row_id),
    edge_evidence_row_id INTEGER NOT NULL REFERENCES mosaic_edge_evidence(row_id),
    reason_code TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (snapshot_row_id, edge_evidence_row_id),
    UNIQUE (snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE reclassification_plan_revision (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    plan_row_id INTEGER NOT NULL REFERENCES reclassification_plan(row_id),
    revision_number INTEGER NOT NULL CHECK (revision_number >= 1),
    predecessor_revision_row_id INTEGER,
    state TEXT NOT NULL CHECK (state IN ('open','applied','discarded','stale','refused')),
    source_session_row_id INTEGER NOT NULL REFERENCES session(row_id),
    metadata_resolution_row_id INTEGER NOT NULL REFERENCES session_metadata_resolution(row_id),
    equipment_resolution_row_id INTEGER NOT NULL REFERENCES session_equipment_resolution(row_id),
    basis_digest TEXT NOT NULL,
    actor_row_id INTEGER NOT NULL REFERENCES spec062_actor(row_id),
    command_row_id INTEGER NOT NULL REFERENCES command_execution(row_id),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    decided_at TEXT,
    UNIQUE (plan_row_id, revision_number),
    UNIQUE (predecessor_revision_row_id),
    UNIQUE (row_id, plan_row_id),
    FOREIGN KEY (predecessor_revision_row_id, plan_row_id)
        REFERENCES reclassification_plan_revision(row_id, plan_row_id)
) STRICT;

CREATE TABLE "registered_sources" (
    id           TEXT PRIMARY KEY,
    kind         TEXT NOT NULL CHECK (kind IN ('light_frames', 'calibration', 'project', 'inbox')),
    path         TEXT NOT NULL,
    kind_subtype TEXT,
    scan_depth   TEXT NOT NULL DEFAULT 'recursive' CHECK (scan_depth IN ('recursive', 'single')),
    created_at   TEXT NOT NULL,
    created_via  TEXT NOT NULL CHECK (created_via IN ('first_run', 'settings_add', 'settings_restart')),
    last_seen_at TEXT, organization_state TEXT NOT NULL DEFAULT 'unorganized'
        CHECK (organization_state IN ('organized', 'unorganized')), active INTEGER NOT NULL DEFAULT 1,
    UNIQUE(kind, path)
);

CREATE TABLE relation_decision_mosaic_lineage (
    decision_snapshot_row_id INTEGER NOT NULL REFERENCES relation_decision_snapshot(row_id),
    predecessor_mosaic_row_id INTEGER NOT NULL REFERENCES mosaic(row_id),
    successor_mosaic_row_id INTEGER NOT NULL REFERENCES mosaic(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (decision_snapshot_row_id, predecessor_mosaic_row_id, successor_mosaic_row_id),
    UNIQUE (decision_snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE relation_decision_mosaic_revision (
    decision_snapshot_row_id INTEGER NOT NULL REFERENCES relation_decision_snapshot(row_id),
    mosaic_revision_row_id INTEGER NOT NULL REFERENCES mosaic_revision(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (decision_snapshot_row_id, mosaic_revision_row_id),
    UNIQUE (decision_snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE relation_decision_panel_lineage (
    decision_snapshot_row_id INTEGER NOT NULL REFERENCES relation_decision_snapshot(row_id),
    predecessor_group_row_id INTEGER NOT NULL REFERENCES panel_group(row_id),
    successor_group_row_id INTEGER NOT NULL REFERENCES panel_group(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (decision_snapshot_row_id, predecessor_group_row_id, successor_group_row_id),
    UNIQUE (decision_snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE relation_decision_panel_revision (
    decision_snapshot_row_id INTEGER NOT NULL REFERENCES relation_decision_snapshot(row_id),
    panel_revision_row_id INTEGER NOT NULL REFERENCES panel_group_revision(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (decision_snapshot_row_id, panel_revision_row_id),
    UNIQUE (decision_snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE relation_decision_retired_mosaic (
    decision_snapshot_row_id INTEGER NOT NULL REFERENCES relation_decision_snapshot(row_id),
    mosaic_row_id INTEGER NOT NULL REFERENCES mosaic(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (decision_snapshot_row_id, mosaic_row_id),
    UNIQUE (decision_snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE relation_decision_retired_panel_group (
    decision_snapshot_row_id INTEGER NOT NULL REFERENCES relation_decision_snapshot(row_id),
    panel_group_row_id INTEGER NOT NULL REFERENCES panel_group(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (decision_snapshot_row_id, panel_group_row_id),
    UNIQUE (decision_snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE relation_decision_session_supersession (
    decision_snapshot_row_id INTEGER NOT NULL REFERENCES relation_decision_snapshot(row_id),
    predecessor_session_row_id INTEGER NOT NULL REFERENCES session(row_id),
    replacement_session_row_id INTEGER NOT NULL REFERENCES session(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (decision_snapshot_row_id, predecessor_session_row_id, replacement_session_row_id),
    UNIQUE (decision_snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE relation_decision_snapshot (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    proposal_row_id INTEGER NOT NULL REFERENCES relation_proposal(row_id),
    proposal_revision INTEGER NOT NULL CHECK (proposal_revision >= 1),
    decision_kind TEXT NOT NULL CHECK (decision_kind IN ('accepted','rejected','corrected')),
    accepted_revision_count INTEGER NOT NULL CHECK (accepted_revision_count >= 0),
    retired_group_count INTEGER NOT NULL CHECK (retired_group_count >= 0),
    lineage_count INTEGER NOT NULL CHECK (lineage_count >= 0),
    actor_row_id INTEGER NOT NULL REFERENCES spec062_actor(row_id),
    reason_code TEXT NOT NULL,
    audit_row_id INTEGER NOT NULL UNIQUE REFERENCES audit_event(row_id)
        DEFERRABLE INITIALLY DEFERRED,
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    UNIQUE (proposal_row_id, proposal_revision)
) STRICT;

CREATE TABLE relation_proposal (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    proposal_revision INTEGER NOT NULL CHECK (proposal_revision >= 1),
    kind TEXT NOT NULL CHECK (kind IN ('panel_add','panel_replace','panel_split','panel_merge','mosaic_create','mosaic_edge','mosaic_split','mosaic_merge','manual_relation')),
    basis_digest TEXT NOT NULL,
    evidence_digest TEXT NOT NULL,
    config_revision_row_id INTEGER NOT NULL REFERENCES spec062_config_revision(row_id),
    state TEXT NOT NULL CHECK (state IN ('pending','accepted','rejected','superseded','stale')),
    actor_row_id INTEGER REFERENCES spec062_actor(row_id),
    reason_code TEXT,
    superseded_by_proposal_row_id INTEGER REFERENCES relation_proposal(row_id),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    decided_sequence INTEGER REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    decided_at TEXT,
    UNIQUE (kind, basis_digest, evidence_digest, config_revision_row_id),
    CHECK ((state = 'pending') = (decided_at IS NULL)),
    CHECK ((state = 'superseded') = (superseded_by_proposal_row_id IS NOT NULL))
) STRICT;

CREATE TABLE relation_proposal_visibility_history (
    proposal_row_id INTEGER NOT NULL REFERENCES relation_proposal(row_id),
    proposal_revision INTEGER NOT NULL CHECK (proposal_revision >= 1),
    state TEXT NOT NULL CHECK (state IN ('pending','accepted','rejected','superseded','stale')),
    visible_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    hidden_sequence INTEGER REFERENCES repository_change(sequence),
    PRIMARY KEY (proposal_row_id, proposal_revision),
    CHECK (hidden_sequence IS NULL OR hidden_sequence > visible_sequence)
) STRICT;

CREATE TABLE relation_rejection (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    proposal_kind TEXT NOT NULL CHECK (proposal_kind IN ('panel_add','panel_replace','panel_split','panel_merge','mosaic_create','mosaic_edge','mosaic_split','mosaic_merge','manual_relation')),
    basis_digest TEXT NOT NULL,
    evidence_digest TEXT NOT NULL,
    config_revision_row_id INTEGER NOT NULL REFERENCES spec062_config_revision(row_id),
    actor_row_id INTEGER NOT NULL REFERENCES spec062_actor(row_id),
    reason_code TEXT NOT NULL,
    note TEXT,
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    UNIQUE (proposal_kind, basis_digest, evidence_digest, config_revision_row_id)
) STRICT;

CREATE TABLE repository_change (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    command_row_id INTEGER,
    created_at TEXT NOT NULL
) STRICT;

CREATE TABLE resolver_settings (
    id                   INTEGER NOT NULL PRIMARY KEY CHECK (id = 1),
    online_enabled       INTEGER NOT NULL DEFAULT 1,      -- bool; online SIMBAD resolution (FR-015)
    simbad_endpoint      TEXT    NOT NULL DEFAULT 'https://simbad.cds.unistra.fr/simbad/sim-tap/sync',
    debounce_ms          INTEGER NOT NULL DEFAULT 300,    -- interactive query debounce
    request_timeout_secs INTEGER NOT NULL DEFAULT 10      -- per-request timeout; degrade to seed+cache
);

CREATE TABLE session (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    materialization_operation_row_id INTEGER NOT NULL REFERENCES session_materialization_operation(row_id),
    kind TEXT NOT NULL CHECK (kind IN ('light','dark','bias','flat')),
    ordinal_in_operation INTEGER NOT NULL CHECK (ordinal_in_operation >= 0),
    identity_digest TEXT NOT NULL,
    observing_night_date TEXT NOT NULL,
    site_row_id INTEGER REFERENCES acquisition_site(row_id),
    timezone_name_snapshot TEXT,
    night_derivation TEXT NOT NULL CHECK (night_derivation IN ('acquisition_timezone','reviewed_local_fallback')),
    canonical_target_row_id INTEGER REFERENCES spec062_target(row_id),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    UNIQUE (materialization_operation_row_id, ordinal_in_operation),
    UNIQUE (materialization_operation_row_id, identity_digest),
    UNIQUE (row_id, materialization_operation_row_id),
    UNIQUE (row_id, kind),
    CHECK ((kind = 'light') = (canonical_target_row_id IS NOT NULL)),
    CHECK ((night_derivation = 'acquisition_timezone') = (site_row_id IS NOT NULL AND timezone_name_snapshot IS NOT NULL))
) STRICT;

CREATE TABLE session_equipment_resolution (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    session_row_id INTEGER NOT NULL REFERENCES session(row_id),
    revision_number INTEGER NOT NULL CHECK (revision_number >= 1),
    predecessor_resolution_row_id INTEGER,
    camera_row_id INTEGER REFERENCES camera(row_id),
    optical_profile_row_id INTEGER REFERENCES optical_profile(row_id),
    camera_alias_evidence_row_id INTEGER REFERENCES equipment_alias_evidence(row_id),
    optical_alias_evidence_row_id INTEGER REFERENCES equipment_alias_evidence(row_id),
    focal_length_reported_um INTEGER,
    focal_length_calculated_um INTEGER,
    comparison_severity TEXT NOT NULL CHECK (comparison_severity IN ('normal','yellow','red','unknown')),
    assignment_mode TEXT NOT NULL CHECK (assignment_mode IN ('automatic','reviewed')),
    accepted_proposal_row_id INTEGER REFERENCES relation_proposal(row_id),
    config_revision_row_id INTEGER NOT NULL REFERENCES spec062_config_revision(row_id),
    actor_row_id INTEGER NOT NULL REFERENCES spec062_actor(row_id),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    UNIQUE (session_row_id, revision_number),
    UNIQUE (predecessor_resolution_row_id),
    UNIQUE (row_id, session_row_id),
    FOREIGN KEY (predecessor_resolution_row_id, session_row_id)
        REFERENCES session_equipment_resolution(row_id, session_row_id)
) STRICT;

CREATE TABLE session_equipment_resolution_head (
    session_row_id INTEGER PRIMARY KEY REFERENCES session(row_id),
    head_resolution_row_id INTEGER NOT NULL UNIQUE,
    head_generation INTEGER NOT NULL DEFAULT 0 CHECK (head_generation >= 0),
    FOREIGN KEY (head_resolution_row_id, session_row_id)
        REFERENCES session_equipment_resolution(row_id, session_row_id)
) STRICT;

CREATE TABLE session_frame (
    session_row_id INTEGER NOT NULL,
    frame_row_id INTEGER NOT NULL REFERENCES frame_record(row_id),
    materialization_operation_row_id INTEGER NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    is_representative INTEGER NOT NULL CHECK (is_representative IN (0,1)),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    PRIMARY KEY (session_row_id, frame_row_id),
    UNIQUE (session_row_id, ordinal),
    UNIQUE (materialization_operation_row_id, frame_row_id),
    FOREIGN KEY (session_row_id, materialization_operation_row_id)
        REFERENCES session(row_id, materialization_operation_row_id)
) STRICT;

CREATE TABLE session_materialization_operation (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL CHECK (kind IN ('inbox_ingestion','metadata_reclassification')),
    command_row_id INTEGER NOT NULL UNIQUE REFERENCES command_execution(row_id),
    config_revision_row_id INTEGER NOT NULL REFERENCES spec062_config_revision(row_id),
    state TEXT NOT NULL CHECK (state IN ('ready','applying','cancelling','cancelled','applied','failed')),
    state_version INTEGER NOT NULL DEFAULT 0 CHECK (state_version >= 0),
    result_snapshot_row_id INTEGER,
    session_count INTEGER CHECK (session_count IS NULL OR session_count >= 0),
    membership_count INTEGER CHECK (membership_count IS NULL OR membership_count >= 0),
    singleton_group_count INTEGER CHECK (singleton_group_count IS NULL OR singleton_group_count >= 0),
    blocked_frame_count INTEGER CHECK (blocked_frame_count IS NULL OR blocked_frame_count >= 0),
    started_at TEXT,
    finished_at TEXT,
    failure_code TEXT,
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    CHECK ((state IN ('cancelled','applied','failed')) = (finished_at IS NOT NULL)),
    CHECK ((state = 'failed') = (failure_code IS NOT NULL)),
    CHECK ((state = 'applied') = (result_snapshot_row_id IS NOT NULL)),
    FOREIGN KEY (result_snapshot_row_id, row_id)
        REFERENCES session_materialization_result_snapshot(row_id, operation_row_id)
        DEFERRABLE INITIALLY DEFERRED
) STRICT;

CREATE TABLE session_materialization_result_blocked_frame (
    snapshot_row_id INTEGER NOT NULL REFERENCES session_materialization_result_snapshot(row_id),
    frame_row_id INTEGER NOT NULL REFERENCES frame_record(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    reason_code TEXT NOT NULL,
    PRIMARY KEY (snapshot_row_id, frame_row_id),
    UNIQUE (snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE session_materialization_result_frame (
    snapshot_row_id INTEGER NOT NULL REFERENCES session_materialization_result_snapshot(row_id),
    session_row_id INTEGER NOT NULL,
    frame_row_id INTEGER NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (snapshot_row_id, session_row_id, frame_row_id),
    UNIQUE (snapshot_row_id, ordinal),
    FOREIGN KEY (session_row_id, frame_row_id) REFERENCES session_frame(session_row_id, frame_row_id)
) STRICT;

CREATE TABLE session_materialization_result_panel_group (
    snapshot_row_id INTEGER NOT NULL REFERENCES session_materialization_result_snapshot(row_id),
    session_row_id INTEGER NOT NULL REFERENCES session(row_id),
    panel_group_row_id INTEGER NOT NULL REFERENCES panel_group(row_id),
    initial_panel_revision_row_id INTEGER NOT NULL REFERENCES panel_group_revision(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (snapshot_row_id, session_row_id),
    UNIQUE (snapshot_row_id, panel_group_row_id),
    UNIQUE (snapshot_row_id, initial_panel_revision_row_id),
    UNIQUE (snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE session_materialization_result_session (
    snapshot_row_id INTEGER NOT NULL REFERENCES session_materialization_result_snapshot(row_id),
    session_row_id INTEGER NOT NULL REFERENCES session(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (snapshot_row_id, session_row_id),
    UNIQUE (snapshot_row_id, ordinal)
) STRICT;

CREATE TABLE session_materialization_result_snapshot (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    operation_row_id INTEGER NOT NULL UNIQUE REFERENCES session_materialization_operation(row_id),
    session_count INTEGER NOT NULL CHECK (session_count >= 0),
    membership_count INTEGER NOT NULL CHECK (membership_count >= 0),
    singleton_group_count INTEGER NOT NULL CHECK (singleton_group_count >= 0),
    blocked_frame_count INTEGER NOT NULL CHECK (blocked_frame_count >= 0),
    canonical_digest TEXT NOT NULL,
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    UNIQUE (row_id, operation_row_id)
) STRICT;

CREATE TABLE session_metadata_resolution (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    session_row_id INTEGER NOT NULL REFERENCES session(row_id),
    revision_number INTEGER NOT NULL CHECK (revision_number >= 1),
    predecessor_resolution_row_id INTEGER,
    state TEXT NOT NULL CHECK (state IN ('accepted','superseded','rejected')),
    actor_row_id INTEGER NOT NULL REFERENCES spec062_actor(row_id),
    command_row_id INTEGER NOT NULL REFERENCES command_execution(row_id),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    UNIQUE (session_row_id, revision_number),
    UNIQUE (predecessor_resolution_row_id),
    UNIQUE (row_id, session_row_id),
    FOREIGN KEY (predecessor_resolution_row_id, session_row_id)
        REFERENCES session_metadata_resolution(row_id, session_row_id)
) STRICT;

CREATE TABLE session_metadata_resolution_frame (
    resolution_row_id INTEGER NOT NULL REFERENCES session_metadata_resolution(row_id),
    frame_row_id INTEGER NOT NULL REFERENCES frame_record(row_id),
    evidence_row_id INTEGER NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (resolution_row_id, frame_row_id),
    UNIQUE (resolution_row_id, ordinal),
    FOREIGN KEY (evidence_row_id, frame_row_id)
        REFERENCES frame_metadata_evidence(row_id, frame_row_id)
) STRICT;

CREATE TABLE session_metadata_resolution_head (
    session_row_id INTEGER PRIMARY KEY REFERENCES session(row_id),
    head_resolution_row_id INTEGER NOT NULL UNIQUE,
    head_generation INTEGER NOT NULL DEFAULT 0 CHECK (head_generation >= 0),
    FOREIGN KEY (head_resolution_row_id, session_row_id)
        REFERENCES session_metadata_resolution(row_id, session_row_id)
) STRICT;

CREATE TABLE session_supersession (
    predecessor_session_row_id INTEGER NOT NULL,
    replacement_session_row_id INTEGER NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('light','dark','bias','flat')),
    applied_plan_revision_row_id INTEGER NOT NULL REFERENCES reclassification_plan_revision(row_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    PRIMARY KEY (predecessor_session_row_id, replacement_session_row_id),
    CHECK (predecessor_session_row_id <> replacement_session_row_id),
    FOREIGN KEY (predecessor_session_row_id, kind) REFERENCES session(row_id, kind),
    FOREIGN KEY (replacement_session_row_id, kind) REFERENCES session(row_id, kind)
) STRICT;

CREATE TABLE session_visibility_history (
    session_row_id INTEGER NOT NULL REFERENCES session(row_id),
    visible_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    hidden_sequence INTEGER REFERENCES repository_change(sequence),
    reason_code TEXT NOT NULL,
    PRIMARY KEY (session_row_id, visible_sequence),
    CHECK (hidden_sequence IS NULL OR hidden_sequence > visible_sequence)
) STRICT;

CREATE TABLE settings (
    key        TEXT PRIMARY KEY NOT NULL,
    value      TEXT NOT NULL, -- JSON-encoded value
    updated_at TEXT NOT NULL
);

CREATE TABLE source_availability_rollup (
    session_row_id INTEGER PRIMARY KEY REFERENCES session(row_id),
    indexed_frame_count INTEGER NOT NULL CHECK (indexed_frame_count >= 0),
    available_frame_count INTEGER NOT NULL CHECK (available_frame_count >= 0),
    readable_frame_count INTEGER NOT NULL CHECK (readable_frame_count >= 0),
    source_byte_count INTEGER NOT NULL CHECK (source_byte_count >= 0),
    observed_at TEXT NOT NULL,
    CHECK (readable_frame_count <= available_frame_count),
    CHECK (available_frame_count <= indexed_frame_count)
) STRICT;

CREATE TABLE source_overrides (
    source_id  TEXT NOT NULL,
    key        TEXT NOT NULL,
    value      TEXT NOT NULL, -- JSON-encoded value
    updated_at TEXT NOT NULL,
    PRIMARY KEY (source_id, key)
);

CREATE TABLE "source_protection_state" (
    source_id             TEXT PRIMARY KEY NOT NULL,
    level                 TEXT NOT NULL CHECK (level IN ('protected', 'unprotected')),
    block_permanent_delete INTEGER,
    categories            TEXT,
    updated_at            TEXT NOT NULL,
    updated_by            TEXT NOT NULL DEFAULT 'system'
);

CREATE TABLE source_view_config (
    singleton_id TEXT PRIMARY KEY DEFAULT 'default' CHECK (singleton_id = 'default'),
    strategy     TEXT NOT NULL DEFAULT 'junctions' CHECK (strategy IN ('junctions', 'symlinks', 'hardlinks', 'copy')),
    updated_at   TEXT NOT NULL
);

CREATE TABLE spec062_actor (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL
) STRICT;

CREATE TABLE spec062_calibration_session (
    session_row_id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('dark','bias','flat')),
    family_row_id INTEGER,
    assignment_state TEXT NOT NULL CHECK (assignment_state IN ('assigned','blocked_unknown_temperature','needs_review')),
    assignment_proposal_row_id INTEGER REFERENCES relation_proposal(row_id),
    age_anchor_at_utc TEXT NOT NULL,
    cooling_setpoint_state TEXT NOT NULL CHECK (cooling_setpoint_state IN ('present','absent','invalid','contradictory')),
    cooling_setpoint_millic INTEGER,
    representative_sensor_temperature_state TEXT NOT NULL CHECK (representative_sensor_temperature_state IN ('present','absent','invalid','contradictory')),
    representative_sensor_temperature_millic INTEGER,
    created_sequence INTEGER NOT NULL REFERENCES repository_change(sequence),
    created_at TEXT NOT NULL,
    CHECK ((assignment_state = 'assigned') = (family_row_id IS NOT NULL)),
    CHECK ((cooling_setpoint_state = 'present') = (cooling_setpoint_millic IS NOT NULL)),
    CHECK ((representative_sensor_temperature_state = 'present') = (representative_sensor_temperature_millic IS NOT NULL)),
    FOREIGN KEY (session_row_id, kind) REFERENCES session(row_id, kind),
    FOREIGN KEY (family_row_id, kind) REFERENCES calibration_family(row_id, kind)
) STRICT;

CREATE TABLE spec062_canonical_object (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL
) STRICT;

CREATE TABLE spec062_config_revision (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    revision_number INTEGER NOT NULL UNIQUE CHECK (revision_number >= 1),
    canonical_digest TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL
) STRICT;

CREATE TABLE spec062_destination_root (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    project_row_id INTEGER NOT NULL REFERENCES spec062_project(row_id),
    created_at TEXT NOT NULL,
    UNIQUE (row_id, project_row_id)
) STRICT;

CREATE TABLE spec062_file_identity (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    legacy_file_public_id TEXT UNIQUE,
    created_at TEXT NOT NULL
) STRICT;

CREATE TABLE spec062_inbox_materialization_plan (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL
) STRICT;

CREATE TABLE spec062_project (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    membership_head_revision_row_id INTEGER,
    membership_head_generation INTEGER NOT NULL DEFAULT 0 CHECK (membership_head_generation >= 0),
    materialization_head_snapshot_row_id INTEGER,
    materialization_head_generation INTEGER NOT NULL DEFAULT 0 CHECK (materialization_head_generation >= 0),
    current_manifest_row_id INTEGER,
    current_manifest_generation INTEGER NOT NULL DEFAULT 0 CHECK (current_manifest_generation >= 0),
    created_at TEXT NOT NULL,
    FOREIGN KEY (membership_head_revision_row_id, row_id)
        REFERENCES project_membership_revision(row_id, project_row_id)
        DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (materialization_head_snapshot_row_id, row_id)
        REFERENCES project_materialization_snapshot(row_id, project_row_id)
        DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (current_manifest_row_id, row_id)
        REFERENCES project_manifest(row_id, project_row_id)
        DEFERRABLE INITIALLY DEFERRED
) STRICT;

CREATE TABLE spec062_source_root (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL
) STRICT;

CREATE TABLE spec062_target (
    row_id INTEGER PRIMARY KEY,
    public_id TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL
) STRICT;

CREATE TABLE target (
    id TEXT PRIMARY KEY NOT NULL,
    primary_designation TEXT NOT NULL UNIQUE,
    aliases TEXT NOT NULL DEFAULT '[]',   -- JSON array
    catalog_refs TEXT NOT NULL DEFAULT '[]', -- JSON array
    created_at TEXT NOT NULL
);

CREATE TABLE target_alias (
    id          TEXT NOT NULL PRIMARY KEY,                -- UUID
    target_id   TEXT NOT NULL REFERENCES canonical_target(id) ON DELETE CASCADE,
    alias       TEXT NOT NULL,                            -- display designation / NAME common name
    normalized  TEXT NOT NULL,                            -- normalized form for matching (spec 013)
    -- spec 036: 'user' marks a user-added alias (only these are user-removable).
    kind        TEXT NOT NULL CHECK (kind IN ('designation', 'common_name', 'user')),
    UNIQUE (target_id, normalized)
);

CREATE TABLE target_favourite (
    target_id     TEXT NOT NULL PRIMARY KEY REFERENCES canonical_target(id) ON DELETE CASCADE,
    favourited_at TEXT NOT NULL
);

CREATE TABLE telescopes (
    id              TEXT PRIMARY KEY NOT NULL,
    name            TEXT NOT NULL,
    aliases         TEXT NOT NULL DEFAULT '[]',  -- JSON array of alternate names
    focal_length_mm INTEGER,
    auto_detected   INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL
);

CREATE TABLE tool_launches (
    id           TEXT    NOT NULL PRIMARY KEY,  -- UUID
    project_id   TEXT    NOT NULL,
    tool_id      TEXT    NOT NULL,              -- matches ToolProfile.id
    launched_at  TEXT    NOT NULL,              -- RFC-3339
    pid          INTEGER,                       -- OS PID; NULL when not surfaced before detach
    working_dir  TEXT,                          -- resolved cwd passed to the child
    args_hash    TEXT,                          -- BLAKE3 hex; NULL when failure before render
    outcome      TEXT    NOT NULL DEFAULT 'spawned',
    completed_at TEXT,                          -- reserved for spec 012; always NULL in v1
    audit_id     TEXT    NOT NULL               -- back-reference to events/audit table
);

CREATE VIRTUAL TABLE frame_footprint_rtree USING rtree_i32(
    evidence_row_id,
    min_x_ppb, max_x_ppb,
    min_y_ppb, max_y_ppb,
    min_z_ppb, max_z_ppb
);

CREATE INDEX idx_acq_fp_gain_binning
    ON acquisition_fingerprint (gain, binning);

CREATE INDEX idx_acq_session_canonical_target
    ON acquisition_session(canonical_target_id)
    WHERE canonical_target_id IS NOT NULL;

CREATE INDEX idx_acq_session_optic_train_key
    ON acquisition_session(optic_train_key)
    WHERE optic_train_key IS NOT NULL;

CREATE INDEX idx_acq_session_root ON acquisition_session(root_id);

CREATE INDEX idx_acq_session_session_key
    ON acquisition_session(session_key);

CREATE INDEX idx_acq_session_target ON acquisition_session(target_id);

CREATE INDEX idx_artifacts_project
    ON processing_artifacts (project_id, detected_at DESC);

CREATE INDEX idx_artifacts_state
    ON processing_artifacts (state);

CREATE INDEX idx_audit_at ON audit_log_entry(at);

CREATE INDEX idx_audit_entity ON audit_log_entry(entity_type, entity_id);

CREATE INDEX idx_audit_event_mosaic ON audit_event(mosaic_row_id, occurred_at DESC, row_id);

CREATE INDEX idx_audit_event_panel ON audit_event(panel_group_row_id, occurred_at DESC, row_id);

CREATE INDEX idx_audit_event_project ON audit_event(project_row_id, occurred_at DESC, row_id);

CREATE INDEX idx_audit_event_proposal
    ON audit_event(proposal_row_id, occurred_at);

CREATE INDEX idx_audit_event_session ON audit_event(session_row_id, occurred_at DESC, row_id);

CREATE INDEX idx_audit_outcome ON audit_log_entry(outcome, reason_code);

CREATE INDEX idx_audit_severity ON audit_log_entry(severity);

CREATE INDEX idx_cal_assignment_session
    ON calibration_assignment (session_id);

CREATE INDEX idx_cal_fingerprint_type
    ON calibration_fingerprint (calibration_type);

CREATE INDEX idx_cal_fp_type_filter
    ON calibration_fingerprint (calibration_type, filter_name)
    WHERE filter_name IS NOT NULL;

CREATE INDEX idx_cal_fp_type_gain_binning
    ON calibration_fingerprint (calibration_type, gain, binning);

CREATE INDEX idx_cal_session_root ON calibration_session(root_id);

CREATE UNIQUE INDEX idx_calibration_family_dark_bias
    ON calibration_family(camera_row_id, kind, identity_digest)
    WHERE kind IN ('dark','bias');

CREATE UNIQUE INDEX idx_calibration_family_flat
    ON calibration_family(optical_profile_row_id, filter_label_row_id, identity_digest)
    WHERE kind = 'flat';

CREATE INDEX idx_calibration_session_recency
    ON spec062_calibration_session(family_row_id, age_anchor_at_utc DESC, session_row_id);

CREATE UNIQUE INDEX idx_canonical_target_simbad_oid
    ON canonical_target(simbad_oid) WHERE simbad_oid IS NOT NULL;

CREATE INDEX idx_cat_equiv_alias ON catalog_equivalence(alias);

CREATE INDEX idx_command_execution_recovery
    ON command_execution(lease_expires_at, row_id)
    WHERE state IN ('received','executing');

CREATE UNIQUE INDEX idx_equipment_alias_accepted
    ON equipment_alias_evidence(capture_profile_version_row_id, semantic_field, normalized_value)
    WHERE review_state = 'accepted';

CREATE INDEX idx_events_topic ON events(topic, event_id);

CREATE INDEX idx_file_record_root ON file_record(root_id);

CREATE INDEX idx_file_record_state ON file_record(state);

CREATE UNIQUE INDEX idx_filter_label_one_absent
    ON filter_label(optical_profile_row_id) WHERE state = 'absent';

CREATE INDEX idx_framing_optic_train_key ON framing(optic_train_key);

CREATE INDEX idx_framing_project ON framing(project_id);

CREATE INDEX idx_framing_target  ON framing(target_id)
    WHERE target_id IS NOT NULL;

CREATE INDEX idx_ingest_resolution_pending
    ON ingest_resolution(state) WHERE state = 'pending';

CREATE INDEX idx_manifests_project_ts
    ON manifests (project_id, timestamp DESC);

CREATE INDEX idx_mosaic_lineage_successor
    ON mosaic_lineage(successor_mosaic_row_id, predecessor_mosaic_row_id);

CREATE INDEX idx_optical_train_camera ON optical_trains(camera_id);

CREATE INDEX idx_optical_train_telescope ON optical_trains(telescope_id);

CREATE INDEX idx_outbox_event_unpublished
    ON outbox_event(occurred_at, row_id) WHERE published_at IS NULL;

CREATE INDEX idx_panel_head_watermark
    ON panel_group_head_history(accepted_sequence DESC, panel_group_row_id);

CREATE INDEX idx_panel_lineage_successor
    ON panel_group_lineage(successor_group_row_id, predecessor_group_row_id);

CREATE INDEX idx_panel_revision_session_lookup
    ON panel_revision_session(session_row_id, panel_revision_row_id);

CREATE INDEX idx_plan_state ON filesystem_plan(state);

CREATE INDEX idx_prepared_source_views_project
    ON prepared_source_views (project_id);

CREATE INDEX idx_project_membership_by_session
    ON project_membership_revision_session(session_row_id, revision_row_id);

CREATE INDEX idx_project_sources_project_id ON project_sources(project_id);

CREATE INDEX idx_project_target ON project(target_id);

CREATE INDEX idx_projects_canonical_target_id
    ON projects(canonical_target_id);

CREATE INDEX idx_projects_lifecycle ON projects(lifecycle);

CREATE INDEX idx_projects_tool      ON projects(tool);

CREATE INDEX idx_prov_archive_asset ON provenance_history_archive(asset_type, asset_id, field_path);

CREATE INDEX idx_psvi_view_id
    ON prepared_source_view_items (view_id);

CREATE INDEX idx_session_frame_by_frame ON session_frame(frame_row_id, session_row_id);

CREATE UNIQUE INDEX idx_session_frame_one_representative
    ON session_frame(session_row_id) WHERE is_representative = 1;

CREATE INDEX idx_session_kind_cursor ON session(kind, created_at DESC, public_id);

CREATE INDEX idx_session_supersession_replacement
    ON session_supersession(replacement_session_row_id, predecessor_session_row_id);

CREATE INDEX idx_session_target_cursor ON session(canonical_target_row_id, created_at DESC, public_id);

CREATE INDEX idx_target_alias_normalized
    ON target_alias(normalized);

CREATE UNIQUE INDEX inbox_breakdown_item_kind
    ON inbox_classification_breakdown (inbox_item_id, kind);

CREATE INDEX inbox_evidence_item
    ON inbox_classification_evidence (inbox_item_id);

CREATE UNIQUE INDEX inbox_evidence_item_path
    ON inbox_classification_evidence (inbox_item_id, relative_file_path);

CREATE INDEX inbox_file_metadata_item
    ON inbox_file_metadata (inbox_item_id);

CREATE INDEX inbox_file_overrides_group
    ON inbox_file_overrides (source_group_id);

CREATE INDEX inbox_file_overrides_group_file
    ON inbox_file_overrides (source_group_id, relative_file_path);

CREATE INDEX inbox_items_root_path
    ON inbox_items (root_id, relative_path);

CREATE INDEX inbox_items_source_group
    ON inbox_items (source_group_id)
    WHERE source_group_id IS NOT NULL;

CREATE INDEX inbox_plan_links_plan
    ON inbox_plan_links (plan_id);

CREATE INDEX inbox_source_groups_root
    ON inbox_source_groups (root_id);

CREATE INDEX plan_apply_events_plan ON plan_apply_events (plan_id, at ASC);

CREATE INDEX plan_apply_events_run  ON plan_apply_events (run_id, at ASC);

CREATE INDEX plan_apply_runs_plan ON plan_apply_runs (plan_id);

CREATE INDEX plan_items_plan   ON plan_items (plan_id, item_index ASC);

CREATE INDEX plan_items_source ON plan_items (source_id)
    WHERE source_id IS NOT NULL;

CREATE INDEX plans_parent        ON plans (parent_plan_id);

CREATE INDEX plans_state_created ON plans (state, created_at DESC);

CREATE INDEX tool_launches_project_launched
    ON tool_launches (project_id, launched_at DESC);

CREATE UNIQUE INDEX uq_command_execution_live_fence
    ON command_execution(row_id, lease_owner, lease_generation);

CREATE VIEW calibration_master_view AS
SELECT
    cs.id                                               AS id,
    cs.kind                                             AS kind,
    COALESCE(cf.calibration_type, cs.kind)              AS calibration_type,
    cs.created_at                                       AS created_at,
    CAST(NULL AS INTEGER)                               AS size_bytes,
    cf.gain                                             AS fp_gain,
    cf.offset_val                                       AS fp_offset_val,
    cf.exposure_s                                       AS fp_exposure_s,
    cf.temp_c                                           AS fp_temp_c,
    cf.filter_name                                      AS fp_filter_name,
    cf.rotation_deg                                     AS fp_rotation_deg,
    cf.binning                                          AS fp_binning,
    cf.optic_train                                      AS fp_optic_train,
    cf.source_session_id                                AS source_session_id,
    cf.observing_night_date                             AS observing_night_date,
    cs.root_id                                          AS root_id,
    fr.relative_path                                    AS frame_relative_path,
    cs.archived_at                                      AS archived_at,
    cs.archived_via_plan_id                             AS archived_via_plan_id
FROM calibration_session cs
LEFT JOIN calibration_fingerprint cf ON cf.id = cs.id
LEFT JOIN file_record fr ON fr.id = json_extract(cs.frame_ids, '$[0]')
WHERE cs.kind IN ('dark', 'flat', 'bias');

CREATE VIEW ledger_view AS
SELECT
    'file_record'      AS entity_type,
    id                 AS entity_id,
    state              AS state,
    NULL               AS title,
    relative_path      AS path,
    NULL               AS project_id,
    last_seen_at       AS updated_at
FROM file_record
UNION ALL
SELECT
    'project',
    id,
    lifecycle,
    name,
    NULL,
    id,
    updated_at
FROM projects
UNION ALL
SELECT
    'filesystem_plan',
    id,
    state,
    NULL,
    NULL,
    NULL,
    COALESCE(applied_at, created_at)
FROM filesystem_plan
UNION ALL
SELECT
    'processing_artifact',
    id,
    staleness,
    NULL,
    NULL,
    project_id,
    created_at
FROM processing_artifact
UNION ALL
SELECT
    'prepared_source',
    id,
    state,
    NULL,
    NULL,
    project_id,
    created_at
FROM prepared_source_view
UNION ALL
SELECT
    'data_source',
    id,
    state,
    label,
    current_path,
    NULL,
    COALESCE(last_seen_at, created_at)
FROM library_root;

CREATE VIEW spec062_invariant_violation AS
SELECT 'session_membership_cardinality' AS invariant, s.row_id AS owner_row_id
FROM session AS s
LEFT JOIN session_frame AS sf ON sf.session_row_id = s.row_id
GROUP BY s.row_id
HAVING COUNT(sf.frame_row_id) = 0 OR SUM(sf.is_representative) <> 1
UNION ALL
SELECT 'operation_session_count', rs.row_id
FROM session_materialization_result_snapshot AS rs
LEFT JOIN session_materialization_result_session AS child ON child.snapshot_row_id = rs.row_id
GROUP BY rs.row_id
HAVING COUNT(child.session_row_id) <> rs.session_count
UNION ALL
SELECT 'operation_membership_count', rs.row_id
FROM session_materialization_result_snapshot AS rs
LEFT JOIN session_materialization_result_frame AS child ON child.snapshot_row_id = rs.row_id
GROUP BY rs.row_id
HAVING COUNT(child.frame_row_id) <> rs.membership_count
UNION ALL
SELECT 'operation_blocked_count', rs.row_id
FROM session_materialization_result_snapshot AS rs
LEFT JOIN session_materialization_result_blocked_frame AS child ON child.snapshot_row_id = rs.row_id
GROUP BY rs.row_id
HAVING COUNT(child.frame_row_id) <> rs.blocked_frame_count
UNION ALL
SELECT 'operation_singleton_group_count', rs.row_id
FROM session_materialization_result_snapshot AS rs
LEFT JOIN session_materialization_result_panel_group AS child ON child.snapshot_row_id = rs.row_id
GROUP BY rs.row_id
HAVING COUNT(child.panel_group_row_id) <> rs.singleton_group_count
UNION ALL
SELECT 'decision_revision_count', ds.row_id
FROM relation_decision_snapshot AS ds
WHERE (
    (SELECT COUNT(*) FROM relation_decision_panel_revision WHERE decision_snapshot_row_id = ds.row_id)
    + (SELECT COUNT(*) FROM relation_decision_mosaic_revision WHERE decision_snapshot_row_id = ds.row_id)
) <> ds.accepted_revision_count
UNION ALL
SELECT 'decision_retired_group_count', ds.row_id
FROM relation_decision_snapshot AS ds
WHERE (
    (SELECT COUNT(*) FROM relation_decision_retired_panel_group WHERE decision_snapshot_row_id = ds.row_id)
    + (SELECT COUNT(*) FROM relation_decision_retired_mosaic WHERE decision_snapshot_row_id = ds.row_id)
) <> ds.retired_group_count
UNION ALL
SELECT 'decision_lineage_count', ds.row_id
FROM relation_decision_snapshot AS ds
WHERE (
    (SELECT COUNT(*) FROM relation_decision_panel_lineage WHERE decision_snapshot_row_id = ds.row_id)
    + (SELECT COUNT(*) FROM relation_decision_mosaic_lineage WHERE decision_snapshot_row_id = ds.row_id)
    + (SELECT COUNT(*) FROM relation_decision_session_supersession WHERE decision_snapshot_row_id = ds.row_id)
) <> ds.lineage_count
UNION ALL
SELECT 'project_snapshot_session_count', ps.row_id
FROM project_materialization_snapshot AS ps
LEFT JOIN project_materialization_snapshot_session AS child ON child.snapshot_row_id = ps.row_id
GROUP BY ps.row_id
HAVING COUNT(child.session_row_id) <> ps.session_count
UNION ALL
SELECT 'project_snapshot_entry_count', ps.row_id
FROM project_materialization_snapshot AS ps
LEFT JOIN project_materialization_snapshot_entry AS child ON child.snapshot_row_id = ps.row_id
GROUP BY ps.row_id
HAVING COUNT(child.entry_row_id) <> ps.entry_count
UNION ALL
SELECT 'update_plan_session_count', p.row_id
FROM materialization_update_plan AS p
LEFT JOIN materialization_update_plan_session AS child ON child.plan_row_id = p.row_id
GROUP BY p.row_id
HAVING COUNT(child.session_row_id) <> p.session_count
UNION ALL
SELECT 'update_plan_item_count', p.row_id
FROM materialization_update_plan AS p
LEFT JOIN materialization_plan_entry AS child ON child.plan_row_id = p.row_id
GROUP BY p.row_id
HAVING COUNT(child.row_id) <> p.item_count
UNION ALL
SELECT 'inbox_proposed_session_count', p.row_id
FROM inbox_materialization_plan_result_snapshot AS p
LEFT JOIN inbox_plan_result_proposed_session AS child ON child.snapshot_row_id = p.row_id
GROUP BY p.row_id
HAVING COUNT(child.row_id) <> p.proposed_session_count
UNION ALL
SELECT 'inbox_frame_count', p.row_id
FROM inbox_materialization_plan_result_snapshot AS p
WHERE (
    (SELECT COUNT(*)
     FROM inbox_plan_result_proposed_session_frame AS child
     JOIN inbox_plan_result_proposed_session AS proposed
       ON proposed.row_id = child.proposed_session_row_id
     WHERE proposed.snapshot_row_id = p.row_id)
    + (SELECT COUNT(*)
       FROM inbox_plan_result_blocked_frame AS blocked
       WHERE blocked.snapshot_row_id = p.row_id)
) <> p.frame_count
UNION ALL
SELECT 'inbox_proposed_session_frame_count', p.row_id
FROM inbox_plan_result_proposed_session AS p
LEFT JOIN inbox_plan_result_proposed_session_frame AS child
    ON child.proposed_session_row_id = p.row_id
GROUP BY p.row_id
HAVING COUNT(child.frame_row_id) <> p.frame_count
UNION ALL
SELECT 'inbox_blocked_frame_count', p.row_id
FROM inbox_materialization_plan_result_snapshot AS p
LEFT JOIN inbox_plan_result_blocked_frame AS child ON child.snapshot_row_id = p.row_id
GROUP BY p.row_id
HAVING COUNT(child.frame_row_id) <> p.blocked_frame_count
UNION ALL
SELECT 'group_action_session_count', p.row_id
FROM group_action_session_snapshot AS p
LEFT JOIN group_action_snapshot_session AS child ON child.snapshot_row_id = p.row_id
GROUP BY p.row_id
HAVING COUNT(child.session_row_id) <> p.session_count
UNION ALL
SELECT 'handoff_requirement_count', p.row_id
FROM calibration_handoff_snapshot AS p
LEFT JOIN calibration_handoff_snapshot_requirement AS child ON child.snapshot_row_id = p.row_id
GROUP BY p.row_id
HAVING COUNT(child.requirement_row_id) <> p.requirement_count
UNION ALL
SELECT 'handoff_selection_count', p.row_id
FROM calibration_handoff_snapshot AS p
LEFT JOIN calibration_handoff_snapshot_selection AS child ON child.snapshot_row_id = p.row_id
GROUP BY p.row_id
HAVING COUNT(child.selection_row_id) <> p.selection_count
UNION ALL
SELECT 'handoff_frame_count', p.row_id
FROM calibration_handoff_snapshot AS p
LEFT JOIN calibration_handoff_snapshot_selection AS ss ON ss.snapshot_row_id = p.row_id
LEFT JOIN calibration_handoff_frame AS child ON child.selection_row_id = ss.selection_row_id
GROUP BY p.row_id
HAVING COUNT(child.frame_row_id) <> p.frame_count
UNION ALL
SELECT 'reclassification_output_frame_count', p.row_id
FROM reclassification_plan_output AS p
LEFT JOIN reclassification_plan_output_frame AS child ON child.output_row_id = p.row_id
GROUP BY p.row_id
HAVING COUNT(child.frame_row_id) <> p.frame_count
UNION ALL
SELECT 'reclassification_replacement_count', p.row_id
FROM reclassification_plan_result_snapshot AS p
LEFT JOIN reclassification_plan_result_session AS child ON child.snapshot_row_id = p.row_id
GROUP BY p.row_id
HAVING COUNT(child.output_row_id) <> p.replacement_session_count
UNION ALL
SELECT 'reclassification_frame_count', p.row_id
FROM reclassification_plan_result_snapshot AS p
LEFT JOIN reclassification_plan_result_frame AS child ON child.snapshot_row_id = p.row_id
GROUP BY p.row_id
HAVING COUNT(child.frame_row_id) <> p.frame_count
UNION ALL
SELECT 'reclassification_panel_consequence_count', p.row_id
FROM reclassification_plan_result_snapshot AS p
LEFT JOIN reclassification_plan_result_panel_consequence AS child ON child.snapshot_row_id = p.row_id
GROUP BY p.row_id
HAVING COUNT(child.source_panel_revision_row_id) <> p.panel_consequence_count
UNION ALL
SELECT 'reclassification_retirement_count', p.row_id
FROM reclassification_plan_result_snapshot AS p
LEFT JOIN reclassification_plan_result_retirement AS child ON child.snapshot_row_id = p.row_id
GROUP BY p.row_id
HAVING COUNT(child.predecessor_panel_group_row_id) <> p.retirement_count
UNION ALL
SELECT 'reclassification_lineage_count', p.row_id
FROM reclassification_plan_result_snapshot AS p
LEFT JOIN reclassification_plan_result_lineage AS child ON child.snapshot_row_id = p.row_id
GROUP BY p.row_id
HAVING COUNT(child.predecessor_panel_group_row_id) <> p.lineage_count
UNION ALL
SELECT 'reclassification_stale_edge_count', p.row_id
FROM reclassification_plan_result_snapshot AS p
LEFT JOIN reclassification_plan_result_stale_edge AS child ON child.snapshot_row_id = p.row_id
GROUP BY p.row_id
HAVING COUNT(child.edge_evidence_row_id) <> p.stale_edge_count
UNION ALL
SELECT 'reclassification_project_consequence_count', p.row_id
FROM reclassification_plan_result_snapshot AS p
LEFT JOIN reclassification_plan_result_project_consequence AS child ON child.snapshot_row_id = p.row_id
GROUP BY p.row_id
HAVING COUNT(child.project_membership_revision_row_id) <> p.project_consequence_count
UNION ALL
SELECT 'reclassification_apply_created_session_count', p.row_id
FROM reclassification_apply_result_snapshot AS p
LEFT JOIN reclassification_apply_created_session AS child ON child.snapshot_row_id = p.row_id
GROUP BY p.row_id
HAVING COUNT(child.session_row_id) <> p.created_session_count
UNION ALL
SELECT 'reclassification_apply_accepted_panel_count', p.row_id
FROM reclassification_apply_result_snapshot AS p
LEFT JOIN reclassification_apply_panel_revision AS child ON child.snapshot_row_id = p.row_id
GROUP BY p.row_id
HAVING COUNT(child.panel_revision_row_id) <> p.accepted_panel_count
UNION ALL
SELECT 'reclassification_apply_retirement_count', p.row_id
FROM reclassification_apply_result_snapshot AS p
LEFT JOIN reclassification_apply_retired_panel_group AS child ON child.snapshot_row_id = p.row_id
GROUP BY p.row_id
HAVING COUNT(child.panel_group_row_id) <> p.retirement_count
UNION ALL
SELECT 'reclassification_apply_lineage_count', p.row_id
FROM reclassification_apply_result_snapshot AS p
LEFT JOIN reclassification_apply_panel_lineage AS child ON child.snapshot_row_id = p.row_id
GROUP BY p.row_id
HAVING COUNT(child.predecessor_group_row_id) <> p.lineage_count
UNION ALL
SELECT 'reclassification_apply_invalidated_edge_count', p.row_id
FROM reclassification_apply_result_snapshot AS p
LEFT JOIN reclassification_apply_invalidated_edge AS child ON child.snapshot_row_id = p.row_id
GROUP BY p.row_id
HAVING COUNT(child.edge_evidence_row_id) <> p.invalidated_edge_count
UNION ALL
SELECT 'reclassification_apply_project_proposal_count', p.row_id
FROM reclassification_apply_result_snapshot AS p
LEFT JOIN reclassification_apply_project_proposal AS child ON child.snapshot_row_id = p.row_id
GROUP BY p.row_id
HAVING COUNT(child.proposal_row_id) <> p.project_proposal_count
UNION ALL
SELECT 'correction_overlay_mapping_count', p.row_id
FROM correction_overlay AS p
LEFT JOIN correction_overlay_mapping AS child ON child.overlay_row_id = p.row_id
GROUP BY p.row_id
HAVING COUNT(child.predecessor_entry_row_id) <> p.mapping_count;

CREATE TRIGGER audit_event_immutable_delete BEFORE DELETE ON audit_event BEGIN
    SELECT RAISE(ABORT, 'audit event is append-only');
END;

CREATE TRIGGER audit_event_immutable_update BEFORE UPDATE ON audit_event BEGIN
    SELECT RAISE(ABORT, 'audit event is append-only');
END;

CREATE TRIGGER chk_u_mosaic_head_history BEFORE UPDATE OF retired_sequence ON mosaic_head_history
WHEN OLD.retired_sequence IS NOT NULL OR NEW.retired_sequence IS NULL OR NEW.retired_sequence <= NEW.accepted_sequence
BEGIN SELECT RAISE(ABORT, 'head retirement must advance the watermark exactly once'); END;

CREATE TRIGGER chk_u_panel_group_head_history BEFORE UPDATE OF retired_sequence ON panel_group_head_history
WHEN OLD.retired_sequence IS NOT NULL OR NEW.retired_sequence IS NULL OR NEW.retired_sequence <= NEW.accepted_sequence
BEGIN SELECT RAISE(ABORT, 'head retirement must advance the watermark exactly once'); END;

CREATE TRIGGER chk_u_project_materialization_head_history BEFORE UPDATE OF retired_sequence ON project_materialization_head_history
WHEN OLD.retired_sequence IS NOT NULL OR NEW.retired_sequence IS NULL OR NEW.retired_sequence <= NEW.accepted_sequence
BEGIN SELECT RAISE(ABORT, 'head retirement must advance the watermark exactly once'); END;

CREATE TRIGGER chk_u_project_membership_head_history BEFORE UPDATE OF retired_sequence ON project_membership_head_history
WHEN OLD.retired_sequence IS NOT NULL OR NEW.retired_sequence IS NULL OR NEW.retired_sequence <= NEW.accepted_sequence
BEGIN SELECT RAISE(ABORT, 'head retirement must advance the watermark exactly once'); END;

CREATE TRIGGER chk_u_relation_proposal_visibility_history BEFORE UPDATE OF hidden_sequence ON relation_proposal_visibility_history
WHEN OLD.hidden_sequence IS NOT NULL OR NEW.hidden_sequence IS NULL OR NEW.hidden_sequence <= NEW.visible_sequence
BEGIN SELECT RAISE(ABORT, 'visibility closure must advance the watermark exactly once'); END;

CREATE TRIGGER chk_u_session_visibility_history BEFORE UPDATE OF hidden_sequence ON session_visibility_history
WHEN OLD.hidden_sequence IS NOT NULL OR NEW.hidden_sequence IS NULL OR NEW.hidden_sequence <= NEW.visible_sequence
BEGIN SELECT RAISE(ABORT, 'visibility closure must advance the watermark exactly once'); END;

CREATE TRIGGER frame_metadata_evidence_immutable_delete BEFORE DELETE ON frame_metadata_evidence BEGIN
    SELECT RAISE(ABORT, 'frame metadata evidence is append-only');
END;

CREATE TRIGGER frame_metadata_evidence_immutable_update BEFORE UPDATE ON frame_metadata_evidence BEGIN
    SELECT RAISE(ABORT, 'frame metadata evidence is append-only');
END;

CREATE TRIGGER frame_metadata_evidence_rtree_insert
AFTER INSERT ON frame_metadata_evidence
WHEN NEW.footprint_wkb IS NOT NULL
BEGIN
    INSERT INTO frame_footprint_rtree VALUES (
        NEW.row_id,
        NEW.bbox_min_x_ppb, NEW.bbox_max_x_ppb,
        NEW.bbox_min_y_ppb, NEW.bbox_max_y_ppb,
        NEW.bbox_min_z_ppb, NEW.bbox_max_z_ppb
    );
END;

CREATE TRIGGER guard_d_acquisition_site BEFORE DELETE ON acquisition_site BEGIN SELECT RAISE(ABORT, 'acquisition site is append-only'); END;

CREATE TRIGGER guard_d_acquisition_site_resolution_candidate BEFORE DELETE ON acquisition_site_resolution_candidate BEGIN SELECT RAISE(ABORT, 'site candidate is append-only'); END;

CREATE TRIGGER guard_d_acquisition_site_resolution_conflict BEFORE DELETE ON acquisition_site_resolution_conflict BEGIN SELECT RAISE(ABORT, 'site conflict is append-only'); END;

CREATE TRIGGER guard_d_bias_recipe_identity BEFORE DELETE ON bias_recipe_identity BEGIN SELECT RAISE(ABORT, 'bias recipe identity is append-only'); END;

CREATE TRIGGER guard_d_calibration_family BEFORE DELETE ON calibration_family BEGIN SELECT RAISE(ABORT, 'calibration family is append-only'); END;

CREATE TRIGGER guard_d_capture_field_mapping BEFORE DELETE ON capture_field_mapping BEGIN SELECT RAISE(ABORT, 'capture field mapping is append-only'); END;

CREATE TRIGGER guard_d_capture_profile BEFORE DELETE ON capture_profile BEGIN SELECT RAISE(ABORT, 'capture profile identity is append-only'); END;

CREATE TRIGGER guard_d_capture_profile_version BEFORE DELETE ON capture_profile_version BEGIN SELECT RAISE(ABORT, 'capture profile version is append-only'); END;

CREATE TRIGGER guard_d_correction_overlay_mapping BEFORE DELETE ON correction_overlay_mapping BEGIN SELECT RAISE(ABORT, 'correction overlay mapping is append-only'); END;

CREATE TRIGGER guard_d_cross_target_association BEFORE DELETE ON cross_target_association BEGIN SELECT RAISE(ABORT, 'cross-target association is append-only'); END;

CREATE TRIGGER guard_d_cross_target_association_target BEFORE DELETE ON cross_target_association_target BEGIN SELECT RAISE(ABORT, 'cross-target membership is append-only'); END;

CREATE TRIGGER guard_d_dark_recipe_identity BEFORE DELETE ON dark_recipe_identity BEGIN SELECT RAISE(ABORT, 'dark recipe identity is append-only'); END;

CREATE TRIGGER guard_d_dark_thermal_evidence BEFORE DELETE ON dark_thermal_evidence BEGIN SELECT RAISE(ABORT, 'thermal evidence is append-only'); END;

CREATE TRIGGER guard_d_equipment_alias_evidence_identity BEFORE DELETE ON equipment_alias_evidence_identity BEGIN SELECT RAISE(ABORT, 'equipment evidence identity is append-only'); END;

CREATE TRIGGER guard_d_filter_label BEFORE DELETE ON filter_label BEGIN SELECT RAISE(ABORT, 'filter label is append-only'); END;

CREATE TRIGGER guard_d_flat_family_identity BEFORE DELETE ON flat_family_identity BEGIN SELECT RAISE(ABORT, 'flat family identity is append-only'); END;

CREATE TRIGGER guard_d_frame_record BEFORE DELETE ON frame_record BEGIN SELECT RAISE(ABORT, 'frame record is append-only'); END;

CREATE TRIGGER guard_d_inbox_ingestion_operation BEFORE DELETE ON inbox_ingestion_operation BEGIN SELECT RAISE(ABORT, 'inbox operation binding is append-only'); END;

CREATE TRIGGER guard_d_light_session_identity BEFORE DELETE ON light_session_identity BEGIN SELECT RAISE(ABORT, 'light identity is append-only'); END;

CREATE TRIGGER guard_d_matching_settings_camera_policy BEFORE DELETE ON matching_settings_camera_policy BEGIN SELECT RAISE(ABORT, 'settings policy is append-only'); END;

CREATE TRIGGER guard_d_materialization_plan_entry BEFORE DELETE ON materialization_plan_entry BEGIN SELECT RAISE(ABORT, 'materialization plan entry is append-only'); END;

CREATE TRIGGER guard_d_materialization_plan_overlay_mapping BEFORE DELETE ON materialization_plan_overlay_mapping BEGIN SELECT RAISE(ABORT, 'materialization overlay mapping is append-only'); END;

CREATE TRIGGER guard_d_materialization_update_plan_session BEFORE DELETE ON materialization_update_plan_session BEGIN SELECT RAISE(ABORT, 'update plan session is append-only'); END;

CREATE TRIGGER guard_d_optical_profile BEFORE DELETE ON optical_profile BEGIN SELECT RAISE(ABORT, 'optical profile is append-only'); END;

CREATE TRIGGER guard_d_project_manifest_entry BEFORE DELETE ON project_manifest_entry BEGIN SELECT RAISE(ABORT, 'manifest entry is append-only'); END;

CREATE TRIGGER guard_d_project_manifest_overlay BEFORE DELETE ON project_manifest_overlay BEGIN SELECT RAISE(ABORT, 'manifest overlay is append-only'); END;

CREATE TRIGGER guard_d_project_membership_revision BEFORE DELETE ON project_membership_revision BEGIN SELECT RAISE(ABORT, 'project membership revision is append-only'); END;

CREATE TRIGGER guard_d_proposal_measurement BEFORE DELETE ON proposal_measurement BEGIN SELECT RAISE(ABORT, 'proposal measurement is append-only'); END;

CREATE TRIGGER guard_d_proposal_mosaic_edge BEFORE DELETE ON proposal_mosaic_edge BEGIN SELECT RAISE(ABORT, 'proposal edge is append-only'); END;

CREATE TRIGGER guard_d_proposal_mosaic_lineage BEFORE DELETE ON proposal_mosaic_lineage BEGIN SELECT RAISE(ABORT, 'proposal lineage is append-only'); END;

CREATE TRIGGER guard_d_proposal_mosaic_membership BEFORE DELETE ON proposal_mosaic_membership BEGIN SELECT RAISE(ABORT, 'proposal membership is append-only'); END;

CREATE TRIGGER guard_d_proposal_mosaic_revision_input BEFORE DELETE ON proposal_mosaic_revision_input BEGIN SELECT RAISE(ABORT, 'proposal input is append-only'); END;

CREATE TRIGGER guard_d_proposal_panel_lineage BEFORE DELETE ON proposal_panel_lineage BEGIN SELECT RAISE(ABORT, 'proposal lineage is append-only'); END;

CREATE TRIGGER guard_d_proposal_panel_membership BEFORE DELETE ON proposal_panel_membership BEGIN SELECT RAISE(ABORT, 'proposal membership is append-only'); END;

CREATE TRIGGER guard_d_proposal_panel_revision_input BEFORE DELETE ON proposal_panel_revision_input BEGIN SELECT RAISE(ABORT, 'proposal input is append-only'); END;

CREATE TRIGGER guard_d_proposal_project_revision_input BEFORE DELETE ON proposal_project_revision_input BEGIN SELECT RAISE(ABORT, 'proposal input is append-only'); END;

CREATE TRIGGER guard_d_proposal_session_input BEFORE DELETE ON proposal_session_input BEGIN SELECT RAISE(ABORT, 'proposal input is append-only'); END;

CREATE TRIGGER guard_d_proposal_target_scope BEFORE DELETE ON proposal_target_scope BEGIN SELECT RAISE(ABORT, 'proposal scope is append-only'); END;

CREATE TRIGGER guard_d_reclassification_apply_created_session BEFORE DELETE ON reclassification_apply_created_session BEGIN SELECT RAISE(ABORT, 'reclassification apply child is append-only'); END;

CREATE TRIGGER guard_d_reclassification_apply_invalidated_edge BEFORE DELETE ON reclassification_apply_invalidated_edge BEGIN SELECT RAISE(ABORT, 'reclassification apply child is append-only'); END;

CREATE TRIGGER guard_d_reclassification_apply_panel_lineage BEFORE DELETE ON reclassification_apply_panel_lineage BEGIN SELECT RAISE(ABORT, 'reclassification apply child is append-only'); END;

CREATE TRIGGER guard_d_reclassification_apply_panel_revision BEFORE DELETE ON reclassification_apply_panel_revision BEGIN SELECT RAISE(ABORT, 'reclassification apply child is append-only'); END;

CREATE TRIGGER guard_d_reclassification_apply_project_proposal BEFORE DELETE ON reclassification_apply_project_proposal BEGIN SELECT RAISE(ABORT, 'reclassification apply child is append-only'); END;

CREATE TRIGGER guard_d_reclassification_apply_retired_panel_group BEFORE DELETE ON reclassification_apply_retired_panel_group BEGIN SELECT RAISE(ABORT, 'reclassification apply child is append-only'); END;

CREATE TRIGGER guard_d_reclassification_plan_edge_consequence BEFORE DELETE ON reclassification_plan_edge_consequence BEGIN SELECT RAISE(ABORT, 'reclassification edge consequence is append-only'); END;

CREATE TRIGGER guard_d_reclassification_plan_input BEFORE DELETE ON reclassification_plan_input BEGIN SELECT RAISE(ABORT, 'reclassification input is append-only'); END;

CREATE TRIGGER guard_d_reclassification_plan_output BEFORE DELETE ON reclassification_plan_output BEGIN SELECT RAISE(ABORT, 'reclassification output is append-only'); END;

CREATE TRIGGER guard_d_reclassification_plan_output_frame BEFORE DELETE ON reclassification_plan_output_frame BEGIN SELECT RAISE(ABORT, 'reclassification output frame is append-only'); END;

CREATE TRIGGER guard_d_reclassification_plan_panel_consequence BEFORE DELETE ON reclassification_plan_panel_consequence BEGIN SELECT RAISE(ABORT, 'reclassification panel consequence is append-only'); END;

CREATE TRIGGER guard_d_reclassification_plan_project_consequence BEFORE DELETE ON reclassification_plan_project_consequence BEGIN SELECT RAISE(ABORT, 'reclassification project consequence is append-only'); END;

CREATE TRIGGER guard_d_reclassification_plan_result_lineage BEFORE DELETE ON reclassification_plan_result_lineage BEGIN SELECT RAISE(ABORT, 'reclassification result child is append-only'); END;

CREATE TRIGGER guard_d_reclassification_plan_result_panel_consequence BEFORE DELETE ON reclassification_plan_result_panel_consequence BEGIN SELECT RAISE(ABORT, 'reclassification result child is append-only'); END;

CREATE TRIGGER guard_d_reclassification_plan_result_project_consequence BEFORE DELETE ON reclassification_plan_result_project_consequence BEGIN SELECT RAISE(ABORT, 'reclassification result child is append-only'); END;

CREATE TRIGGER guard_d_reclassification_plan_result_project_replacement BEFORE DELETE ON reclassification_plan_result_project_replacement BEGIN SELECT RAISE(ABORT, 'reclassification result child is append-only'); END;

CREATE TRIGGER guard_d_reclassification_plan_result_retirement BEFORE DELETE ON reclassification_plan_result_retirement BEGIN SELECT RAISE(ABORT, 'reclassification result child is append-only'); END;

CREATE TRIGGER guard_d_reclassification_plan_result_stale_edge BEFORE DELETE ON reclassification_plan_result_stale_edge BEGIN SELECT RAISE(ABORT, 'reclassification result child is append-only'); END;

CREATE TRIGGER guard_d_reclassification_plan_revision BEFORE DELETE ON reclassification_plan_revision BEGIN SELECT RAISE(ABORT, 'reclassification revision is append-only'); END;

CREATE TRIGGER guard_d_relation_proposal BEFORE DELETE ON relation_proposal BEGIN SELECT RAISE(ABORT, 'relation proposal is append-only'); END;

CREATE TRIGGER guard_d_repository_change BEFORE DELETE ON repository_change BEGIN SELECT RAISE(ABORT, 'repository change is append-only'); END;

CREATE TRIGGER guard_d_spec062_actor BEFORE DELETE ON spec062_actor BEGIN SELECT RAISE(ABORT, 'actor identity is append-only'); END;

CREATE TRIGGER guard_d_spec062_calibration_session BEFORE DELETE ON spec062_calibration_session BEGIN SELECT RAISE(ABORT, 'calibration session is append-only'); END;

CREATE TRIGGER guard_d_spec062_config_revision BEFORE DELETE ON spec062_config_revision BEGIN SELECT RAISE(ABORT, 'configuration revision is append-only'); END;

CREATE TRIGGER guard_d_spec062_file_identity BEFORE DELETE ON spec062_file_identity BEGIN SELECT RAISE(ABORT, 'file identity is append-only'); END;

CREATE TRIGGER guard_d_spec062_inbox_materialization_plan BEFORE DELETE ON spec062_inbox_materialization_plan BEGIN SELECT RAISE(ABORT, 'inbox plan is append-only'); END;

CREATE TRIGGER guard_d_spec062_target BEFORE DELETE ON spec062_target BEGIN SELECT RAISE(ABORT, 'target identity is append-only'); END;

CREATE TRIGGER guard_u_acquisition_site BEFORE UPDATE ON acquisition_site BEGIN SELECT RAISE(ABORT, 'acquisition site is append-only'); END;

CREATE TRIGGER guard_u_acquisition_site_resolution_candidate BEFORE UPDATE ON acquisition_site_resolution_candidate BEGIN SELECT RAISE(ABORT, 'site candidate is append-only'); END;

CREATE TRIGGER guard_u_acquisition_site_resolution_conflict BEFORE UPDATE ON acquisition_site_resolution_conflict BEGIN SELECT RAISE(ABORT, 'site conflict is append-only'); END;

CREATE TRIGGER guard_u_bias_recipe_identity BEFORE UPDATE ON bias_recipe_identity BEGIN SELECT RAISE(ABORT, 'bias recipe identity is append-only'); END;

CREATE TRIGGER guard_u_calibration_family BEFORE UPDATE ON calibration_family BEGIN SELECT RAISE(ABORT, 'calibration family is append-only'); END;

CREATE TRIGGER guard_u_capture_field_mapping BEFORE UPDATE ON capture_field_mapping BEGIN SELECT RAISE(ABORT, 'capture field mapping is append-only'); END;

CREATE TRIGGER guard_u_capture_profile BEFORE UPDATE OF public_id, display_name, created_at ON capture_profile BEGIN SELECT RAISE(ABORT, 'capture profile identity is append-only'); END;

CREATE TRIGGER guard_u_capture_profile_version BEFORE UPDATE ON capture_profile_version BEGIN SELECT RAISE(ABORT, 'capture profile version is append-only'); END;

CREATE TRIGGER guard_u_correction_overlay_mapping BEFORE UPDATE ON correction_overlay_mapping BEGIN SELECT RAISE(ABORT, 'correction overlay mapping is append-only'); END;

CREATE TRIGGER guard_u_cross_target_association BEFORE UPDATE ON cross_target_association BEGIN SELECT RAISE(ABORT, 'cross-target association is append-only'); END;

CREATE TRIGGER guard_u_cross_target_association_target BEFORE UPDATE ON cross_target_association_target BEGIN SELECT RAISE(ABORT, 'cross-target membership is append-only'); END;

CREATE TRIGGER guard_u_dark_recipe_identity BEFORE UPDATE ON dark_recipe_identity BEGIN SELECT RAISE(ABORT, 'dark recipe identity is append-only'); END;

CREATE TRIGGER guard_u_dark_thermal_evidence BEFORE UPDATE ON dark_thermal_evidence BEGIN SELECT RAISE(ABORT, 'thermal evidence is append-only'); END;

CREATE TRIGGER guard_u_equipment_alias_evidence_identity BEFORE UPDATE OF public_id, created_at ON equipment_alias_evidence_identity BEGIN SELECT RAISE(ABORT, 'equipment evidence identity is append-only'); END;

CREATE TRIGGER guard_u_filter_label BEFORE UPDATE ON filter_label BEGIN SELECT RAISE(ABORT, 'filter label is append-only'); END;

CREATE TRIGGER guard_u_flat_family_identity BEFORE UPDATE ON flat_family_identity BEGIN SELECT RAISE(ABORT, 'flat family identity is append-only'); END;

CREATE TRIGGER guard_u_frame_record BEFORE UPDATE ON frame_record BEGIN SELECT RAISE(ABORT, 'frame record is append-only'); END;

CREATE TRIGGER guard_u_inbox_ingestion_operation BEFORE UPDATE ON inbox_ingestion_operation BEGIN SELECT RAISE(ABORT, 'inbox operation binding is append-only'); END;

CREATE TRIGGER guard_u_light_session_identity BEFORE UPDATE ON light_session_identity BEGIN SELECT RAISE(ABORT, 'light identity is append-only'); END;

CREATE TRIGGER guard_u_matching_settings_camera_policy BEFORE UPDATE ON matching_settings_camera_policy BEGIN SELECT RAISE(ABORT, 'settings policy is append-only'); END;

CREATE TRIGGER guard_u_materialization_plan_entry BEFORE UPDATE ON materialization_plan_entry BEGIN SELECT RAISE(ABORT, 'materialization plan entry is append-only'); END;

CREATE TRIGGER guard_u_materialization_plan_overlay_mapping BEFORE UPDATE ON materialization_plan_overlay_mapping BEGIN SELECT RAISE(ABORT, 'materialization overlay mapping is append-only'); END;

CREATE TRIGGER guard_u_materialization_update_plan_session BEFORE UPDATE ON materialization_update_plan_session BEGIN SELECT RAISE(ABORT, 'update plan session is append-only'); END;

CREATE TRIGGER guard_u_optical_profile BEFORE UPDATE ON optical_profile BEGIN SELECT RAISE(ABORT, 'optical profile is append-only'); END;

CREATE TRIGGER guard_u_project_manifest_entry BEFORE UPDATE ON project_manifest_entry BEGIN SELECT RAISE(ABORT, 'manifest entry is append-only'); END;

CREATE TRIGGER guard_u_project_manifest_overlay BEFORE UPDATE ON project_manifest_overlay BEGIN SELECT RAISE(ABORT, 'manifest overlay is append-only'); END;

CREATE TRIGGER guard_u_project_membership_revision BEFORE UPDATE ON project_membership_revision BEGIN SELECT RAISE(ABORT, 'project membership revision is append-only'); END;

CREATE TRIGGER guard_u_proposal_measurement BEFORE UPDATE ON proposal_measurement BEGIN SELECT RAISE(ABORT, 'proposal measurement is append-only'); END;

CREATE TRIGGER guard_u_proposal_mosaic_edge BEFORE UPDATE ON proposal_mosaic_edge BEGIN SELECT RAISE(ABORT, 'proposal edge is append-only'); END;

CREATE TRIGGER guard_u_proposal_mosaic_lineage BEFORE UPDATE ON proposal_mosaic_lineage BEGIN SELECT RAISE(ABORT, 'proposal lineage is append-only'); END;

CREATE TRIGGER guard_u_proposal_mosaic_membership BEFORE UPDATE ON proposal_mosaic_membership BEGIN SELECT RAISE(ABORT, 'proposal membership is append-only'); END;

CREATE TRIGGER guard_u_proposal_mosaic_revision_input BEFORE UPDATE ON proposal_mosaic_revision_input BEGIN SELECT RAISE(ABORT, 'proposal input is append-only'); END;

CREATE TRIGGER guard_u_proposal_panel_lineage BEFORE UPDATE ON proposal_panel_lineage BEGIN SELECT RAISE(ABORT, 'proposal lineage is append-only'); END;

CREATE TRIGGER guard_u_proposal_panel_membership BEFORE UPDATE ON proposal_panel_membership BEGIN SELECT RAISE(ABORT, 'proposal membership is append-only'); END;

CREATE TRIGGER guard_u_proposal_panel_revision_input BEFORE UPDATE ON proposal_panel_revision_input BEGIN SELECT RAISE(ABORT, 'proposal input is append-only'); END;

CREATE TRIGGER guard_u_proposal_project_revision_input BEFORE UPDATE ON proposal_project_revision_input BEGIN SELECT RAISE(ABORT, 'proposal input is append-only'); END;

CREATE TRIGGER guard_u_proposal_session_input BEFORE UPDATE ON proposal_session_input BEGIN SELECT RAISE(ABORT, 'proposal input is append-only'); END;

CREATE TRIGGER guard_u_proposal_target_scope BEFORE UPDATE ON proposal_target_scope BEGIN SELECT RAISE(ABORT, 'proposal scope is append-only'); END;

CREATE TRIGGER guard_u_reclassification_apply_created_session BEFORE UPDATE ON reclassification_apply_created_session BEGIN SELECT RAISE(ABORT, 'reclassification apply child is append-only'); END;

CREATE TRIGGER guard_u_reclassification_apply_invalidated_edge BEFORE UPDATE ON reclassification_apply_invalidated_edge BEGIN SELECT RAISE(ABORT, 'reclassification apply child is append-only'); END;

CREATE TRIGGER guard_u_reclassification_apply_panel_lineage BEFORE UPDATE ON reclassification_apply_panel_lineage BEGIN SELECT RAISE(ABORT, 'reclassification apply child is append-only'); END;

CREATE TRIGGER guard_u_reclassification_apply_panel_revision BEFORE UPDATE ON reclassification_apply_panel_revision BEGIN SELECT RAISE(ABORT, 'reclassification apply child is append-only'); END;

CREATE TRIGGER guard_u_reclassification_apply_project_proposal BEFORE UPDATE ON reclassification_apply_project_proposal BEGIN SELECT RAISE(ABORT, 'reclassification apply child is append-only'); END;

CREATE TRIGGER guard_u_reclassification_apply_retired_panel_group BEFORE UPDATE ON reclassification_apply_retired_panel_group BEGIN SELECT RAISE(ABORT, 'reclassification apply child is append-only'); END;

CREATE TRIGGER guard_u_reclassification_plan_edge_consequence BEFORE UPDATE ON reclassification_plan_edge_consequence BEGIN SELECT RAISE(ABORT, 'reclassification edge consequence is append-only'); END;

CREATE TRIGGER guard_u_reclassification_plan_input BEFORE UPDATE ON reclassification_plan_input BEGIN SELECT RAISE(ABORT, 'reclassification input is append-only'); END;

CREATE TRIGGER guard_u_reclassification_plan_output BEFORE UPDATE ON reclassification_plan_output BEGIN SELECT RAISE(ABORT, 'reclassification output is append-only'); END;

CREATE TRIGGER guard_u_reclassification_plan_output_frame BEFORE UPDATE ON reclassification_plan_output_frame BEGIN SELECT RAISE(ABORT, 'reclassification output frame is append-only'); END;

CREATE TRIGGER guard_u_reclassification_plan_panel_consequence BEFORE UPDATE ON reclassification_plan_panel_consequence BEGIN SELECT RAISE(ABORT, 'reclassification panel consequence is append-only'); END;

CREATE TRIGGER guard_u_reclassification_plan_project_consequence BEFORE UPDATE ON reclassification_plan_project_consequence BEGIN SELECT RAISE(ABORT, 'reclassification project consequence is append-only'); END;

CREATE TRIGGER guard_u_reclassification_plan_result_lineage BEFORE UPDATE ON reclassification_plan_result_lineage BEGIN SELECT RAISE(ABORT, 'reclassification result child is append-only'); END;

CREATE TRIGGER guard_u_reclassification_plan_result_panel_consequence BEFORE UPDATE ON reclassification_plan_result_panel_consequence BEGIN SELECT RAISE(ABORT, 'reclassification result child is append-only'); END;

CREATE TRIGGER guard_u_reclassification_plan_result_project_consequence BEFORE UPDATE ON reclassification_plan_result_project_consequence BEGIN SELECT RAISE(ABORT, 'reclassification result child is append-only'); END;

CREATE TRIGGER guard_u_reclassification_plan_result_project_replacement BEFORE UPDATE ON reclassification_plan_result_project_replacement BEGIN SELECT RAISE(ABORT, 'reclassification result child is append-only'); END;

CREATE TRIGGER guard_u_reclassification_plan_result_retirement BEFORE UPDATE ON reclassification_plan_result_retirement BEGIN SELECT RAISE(ABORT, 'reclassification result child is append-only'); END;

CREATE TRIGGER guard_u_reclassification_plan_result_stale_edge BEFORE UPDATE ON reclassification_plan_result_stale_edge BEGIN SELECT RAISE(ABORT, 'reclassification result child is append-only'); END;

CREATE TRIGGER guard_u_reclassification_plan_revision BEFORE UPDATE ON reclassification_plan_revision BEGIN SELECT RAISE(ABORT, 'reclassification revision is append-only'); END;

CREATE TRIGGER guard_u_relation_proposal BEFORE UPDATE ON relation_proposal
WHEN OLD.state <> 'pending'
  OR NEW.state = 'pending'
  OR NEW.row_id <> OLD.row_id
  OR NEW.public_id <> OLD.public_id
  OR NEW.proposal_revision <> OLD.proposal_revision + 1
  OR NEW.kind <> OLD.kind
  OR NEW.basis_digest <> OLD.basis_digest
  OR NEW.evidence_digest <> OLD.evidence_digest
  OR NEW.config_revision_row_id <> OLD.config_revision_row_id
  OR NEW.created_sequence <> OLD.created_sequence
  OR NEW.created_at <> OLD.created_at
  OR NEW.actor_row_id IS NULL
  OR NEW.reason_code IS NULL
  OR NEW.decided_sequence IS NULL
  OR NEW.decided_at IS NULL
BEGIN SELECT RAISE(ABORT, 'proposal permits only one pending-to-decision transition'); END;

CREATE TRIGGER guard_u_repository_change BEFORE UPDATE ON repository_change BEGIN SELECT RAISE(ABORT, 'repository change is append-only'); END;

CREATE TRIGGER guard_u_spec062_actor BEFORE UPDATE ON spec062_actor BEGIN SELECT RAISE(ABORT, 'actor identity is append-only'); END;

CREATE TRIGGER guard_u_spec062_calibration_session BEFORE UPDATE ON spec062_calibration_session BEGIN SELECT RAISE(ABORT, 'calibration session is append-only'); END;

CREATE TRIGGER guard_u_spec062_config_revision BEFORE UPDATE ON spec062_config_revision BEGIN SELECT RAISE(ABORT, 'configuration revision is append-only'); END;

CREATE TRIGGER guard_u_spec062_file_identity BEFORE UPDATE ON spec062_file_identity BEGIN SELECT RAISE(ABORT, 'file identity is append-only'); END;

CREATE TRIGGER guard_u_spec062_inbox_materialization_plan BEFORE UPDATE ON spec062_inbox_materialization_plan BEGIN SELECT RAISE(ABORT, 'inbox plan is append-only'); END;

CREATE TRIGGER guard_u_spec062_target BEFORE UPDATE ON spec062_target BEGIN SELECT RAISE(ABORT, 'target identity is append-only'); END;

CREATE TRIGGER imm_d_acquisition_site_resolution_revision BEFORE DELETE ON acquisition_site_resolution_revision BEGIN SELECT RAISE(ABORT, 'accepted history is append-only'); END;

CREATE TRIGGER imm_d_calibration_handoff_candidate_evidence BEFORE DELETE ON calibration_handoff_candidate_evidence BEGIN SELECT RAISE(ABORT, 'handoff evidence is append-only'); END;

CREATE TRIGGER imm_d_calibration_handoff_frame BEFORE DELETE ON calibration_handoff_frame BEGIN SELECT RAISE(ABORT, 'handoff frame is append-only'); END;

CREATE TRIGGER imm_d_calibration_handoff_selection BEFORE DELETE ON calibration_handoff_selection BEGIN SELECT RAISE(ABORT, 'handoff selection is append-only'); END;

CREATE TRIGGER imm_d_calibration_handoff_snapshot BEFORE DELETE ON calibration_handoff_snapshot BEGIN SELECT RAISE(ABORT, 'accepted snapshot is append-only'); END;

CREATE TRIGGER imm_d_calibration_handoff_snapshot_requirement BEFORE DELETE ON calibration_handoff_snapshot_requirement BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_d_calibration_handoff_snapshot_selection BEFORE DELETE ON calibration_handoff_snapshot_selection BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_d_calibration_reuse_decision BEFORE DELETE ON calibration_reuse_decision BEGIN SELECT RAISE(ABORT, 'accepted decision is append-only'); END;

CREATE TRIGGER imm_d_camera_regulation_decision BEFORE DELETE ON camera_regulation_decision BEGIN SELECT RAISE(ABORT, 'accepted decision is append-only'); END;

CREATE TRIGGER imm_d_correction_overlay BEFORE DELETE ON correction_overlay BEGIN SELECT RAISE(ABORT, 'overlay is append-only'); END;

CREATE TRIGGER imm_d_equipment_alias_evidence BEFORE DELETE ON equipment_alias_evidence BEGIN SELECT RAISE(ABORT, 'accepted evidence is append-only'); END;

CREATE TRIGGER imm_d_group_action_session_snapshot BEFORE DELETE ON group_action_session_snapshot BEGIN SELECT RAISE(ABORT, 'accepted snapshot is append-only'); END;

CREATE TRIGGER imm_d_group_action_snapshot_session BEFORE DELETE ON group_action_snapshot_session BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_d_inbox_materialization_plan_result_snapshot BEFORE DELETE ON inbox_materialization_plan_result_snapshot BEGIN SELECT RAISE(ABORT, 'accepted snapshot is append-only'); END;

CREATE TRIGGER imm_d_inbox_plan_result_blocked_frame BEFORE DELETE ON inbox_plan_result_blocked_frame BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_d_inbox_plan_result_proposed_session BEFORE DELETE ON inbox_plan_result_proposed_session BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_d_inbox_plan_result_proposed_session_frame BEFORE DELETE ON inbox_plan_result_proposed_session_frame BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_d_matching_settings_revision BEFORE DELETE ON matching_settings_revision BEGIN SELECT RAISE(ABORT, 'settings revision is append-only'); END;

CREATE TRIGGER imm_d_materialization_item_journal BEFORE DELETE ON materialization_item_journal BEGIN SELECT RAISE(ABORT, 'item journal is append-only'); END;

CREATE TRIGGER imm_d_mosaic_edge_evidence BEFORE DELETE ON mosaic_edge_evidence BEGIN SELECT RAISE(ABORT, 'edge evidence is append-only'); END;

CREATE TRIGGER imm_d_mosaic_edge_invalidation BEFORE DELETE ON mosaic_edge_invalidation BEGIN SELECT RAISE(ABORT, 'invalidation is append-only'); END;

CREATE TRIGGER imm_d_mosaic_head_history BEFORE DELETE ON mosaic_head_history BEGIN SELECT RAISE(ABORT, 'head history is append-only'); END;

CREATE TRIGGER imm_d_mosaic_lineage BEFORE DELETE ON mosaic_lineage BEGIN SELECT RAISE(ABORT, 'lineage is append-only'); END;

CREATE TRIGGER imm_d_panel_group_head_history BEFORE DELETE ON panel_group_head_history BEGIN SELECT RAISE(ABORT, 'head history is append-only'); END;

CREATE TRIGGER imm_d_panel_group_lineage BEFORE DELETE ON panel_group_lineage BEGIN SELECT RAISE(ABORT, 'lineage is append-only'); END;

CREATE TRIGGER imm_d_project_manifest BEFORE DELETE ON project_manifest BEGIN SELECT RAISE(ABORT, 'manifest is append-only'); END;

CREATE TRIGGER imm_d_project_materialization_head_history BEFORE DELETE ON project_materialization_head_history BEGIN SELECT RAISE(ABORT, 'head history is append-only'); END;

CREATE TRIGGER imm_d_project_materialization_snapshot BEFORE DELETE ON project_materialization_snapshot BEGIN SELECT RAISE(ABORT, 'accepted snapshot is append-only'); END;

CREATE TRIGGER imm_d_project_materialization_snapshot_entry BEFORE DELETE ON project_materialization_snapshot_entry BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_d_project_materialization_snapshot_session BEFORE DELETE ON project_materialization_snapshot_session BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_d_project_membership_head_history BEFORE DELETE ON project_membership_head_history BEGIN SELECT RAISE(ABORT, 'head history is append-only'); END;

CREATE TRIGGER imm_d_reclassification_apply_created_session BEFORE DELETE ON reclassification_apply_created_session BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_d_reclassification_apply_result_snapshot BEFORE DELETE ON reclassification_apply_result_snapshot BEGIN SELECT RAISE(ABORT, 'accepted snapshot is append-only'); END;

CREATE TRIGGER imm_d_reclassification_plan_result_frame BEFORE DELETE ON reclassification_plan_result_frame BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_d_reclassification_plan_result_session BEFORE DELETE ON reclassification_plan_result_session BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_d_reclassification_plan_result_snapshot BEFORE DELETE ON reclassification_plan_result_snapshot BEGIN SELECT RAISE(ABORT, 'accepted snapshot is append-only'); END;

CREATE TRIGGER imm_d_reclassification_plan_revision BEFORE DELETE ON reclassification_plan_revision BEGIN SELECT RAISE(ABORT, 'accepted revision is append-only'); END;

CREATE TRIGGER imm_d_relation_decision_mosaic_lineage BEFORE DELETE ON relation_decision_mosaic_lineage BEGIN SELECT RAISE(ABORT, 'decision child is append-only'); END;

CREATE TRIGGER imm_d_relation_decision_mosaic_revision BEFORE DELETE ON relation_decision_mosaic_revision BEGIN SELECT RAISE(ABORT, 'decision child is append-only'); END;

CREATE TRIGGER imm_d_relation_decision_panel_lineage BEFORE DELETE ON relation_decision_panel_lineage BEGIN SELECT RAISE(ABORT, 'decision child is append-only'); END;

CREATE TRIGGER imm_d_relation_decision_panel_revision BEFORE DELETE ON relation_decision_panel_revision BEGIN SELECT RAISE(ABORT, 'decision child is append-only'); END;

CREATE TRIGGER imm_d_relation_decision_retired_mosaic BEFORE DELETE ON relation_decision_retired_mosaic BEGIN SELECT RAISE(ABORT, 'decision child is append-only'); END;

CREATE TRIGGER imm_d_relation_decision_retired_panel_group BEFORE DELETE ON relation_decision_retired_panel_group BEGIN SELECT RAISE(ABORT, 'decision child is append-only'); END;

CREATE TRIGGER imm_d_relation_decision_session_supersession BEFORE DELETE ON relation_decision_session_supersession BEGIN SELECT RAISE(ABORT, 'decision child is append-only'); END;

CREATE TRIGGER imm_d_relation_decision_snapshot BEFORE DELETE ON relation_decision_snapshot BEGIN SELECT RAISE(ABORT, 'accepted decision is append-only'); END;

CREATE TRIGGER imm_d_relation_proposal_visibility_history BEFORE DELETE ON relation_proposal_visibility_history BEGIN SELECT RAISE(ABORT, 'visibility history is append-only'); END;

CREATE TRIGGER imm_d_relation_rejection BEFORE DELETE ON relation_rejection BEGIN SELECT RAISE(ABORT, 'rejection is append-only'); END;

CREATE TRIGGER imm_d_session_equipment_resolution BEFORE DELETE ON session_equipment_resolution BEGIN SELECT RAISE(ABORT, 'accepted history is append-only'); END;

CREATE TRIGGER imm_d_session_materialization_result_blocked_frame BEFORE DELETE ON session_materialization_result_blocked_frame BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_d_session_materialization_result_frame BEFORE DELETE ON session_materialization_result_frame BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_d_session_materialization_result_session BEFORE DELETE ON session_materialization_result_session BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_d_session_materialization_result_snapshot BEFORE DELETE ON session_materialization_result_snapshot BEGIN SELECT RAISE(ABORT, 'accepted snapshot is append-only'); END;

CREATE TRIGGER imm_d_session_metadata_resolution BEFORE DELETE ON session_metadata_resolution BEGIN SELECT RAISE(ABORT, 'accepted history is append-only'); END;

CREATE TRIGGER imm_d_session_supersession BEFORE DELETE ON session_supersession BEGIN SELECT RAISE(ABORT, 'supersession is append-only'); END;

CREATE TRIGGER imm_d_session_visibility_history BEFORE DELETE ON session_visibility_history BEGIN SELECT RAISE(ABORT, 'visibility history is append-only'); END;

CREATE TRIGGER imm_u_acquisition_site_resolution_revision BEFORE UPDATE ON acquisition_site_resolution_revision BEGIN SELECT RAISE(ABORT, 'accepted history is append-only'); END;

CREATE TRIGGER imm_u_calibration_handoff_candidate_evidence BEFORE UPDATE ON calibration_handoff_candidate_evidence BEGIN SELECT RAISE(ABORT, 'handoff evidence is append-only'); END;

CREATE TRIGGER imm_u_calibration_handoff_frame BEFORE UPDATE ON calibration_handoff_frame BEGIN SELECT RAISE(ABORT, 'handoff frame is append-only'); END;

CREATE TRIGGER imm_u_calibration_handoff_selection BEFORE UPDATE ON calibration_handoff_selection BEGIN SELECT RAISE(ABORT, 'handoff selection is append-only'); END;

CREATE TRIGGER imm_u_calibration_handoff_snapshot BEFORE UPDATE ON calibration_handoff_snapshot BEGIN SELECT RAISE(ABORT, 'accepted snapshot is append-only'); END;

CREATE TRIGGER imm_u_calibration_handoff_snapshot_requirement BEFORE UPDATE ON calibration_handoff_snapshot_requirement BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_u_calibration_handoff_snapshot_selection BEFORE UPDATE ON calibration_handoff_snapshot_selection BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_u_calibration_reuse_decision BEFORE UPDATE ON calibration_reuse_decision BEGIN SELECT RAISE(ABORT, 'accepted decision is append-only'); END;

CREATE TRIGGER imm_u_camera_regulation_decision BEFORE UPDATE ON camera_regulation_decision BEGIN SELECT RAISE(ABORT, 'accepted decision is append-only'); END;

CREATE TRIGGER imm_u_correction_overlay BEFORE UPDATE ON correction_overlay BEGIN SELECT RAISE(ABORT, 'overlay is append-only'); END;

CREATE TRIGGER imm_u_equipment_alias_evidence BEFORE UPDATE ON equipment_alias_evidence BEGIN SELECT RAISE(ABORT, 'accepted evidence is append-only'); END;

CREATE TRIGGER imm_u_group_action_session_snapshot BEFORE UPDATE ON group_action_session_snapshot BEGIN SELECT RAISE(ABORT, 'accepted snapshot is append-only'); END;

CREATE TRIGGER imm_u_group_action_snapshot_session BEFORE UPDATE ON group_action_snapshot_session BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_u_inbox_materialization_plan_result_snapshot BEFORE UPDATE ON inbox_materialization_plan_result_snapshot BEGIN SELECT RAISE(ABORT, 'accepted snapshot is append-only'); END;

CREATE TRIGGER imm_u_inbox_plan_result_blocked_frame BEFORE UPDATE ON inbox_plan_result_blocked_frame BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_u_inbox_plan_result_proposed_session BEFORE UPDATE ON inbox_plan_result_proposed_session BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_u_inbox_plan_result_proposed_session_frame BEFORE UPDATE ON inbox_plan_result_proposed_session_frame BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_u_matching_settings_revision BEFORE UPDATE ON matching_settings_revision BEGIN SELECT RAISE(ABORT, 'settings revision is append-only'); END;

CREATE TRIGGER imm_u_materialization_item_journal BEFORE UPDATE ON materialization_item_journal BEGIN SELECT RAISE(ABORT, 'item journal is append-only'); END;

CREATE TRIGGER imm_u_mosaic_edge_evidence BEFORE UPDATE ON mosaic_edge_evidence BEGIN SELECT RAISE(ABORT, 'edge evidence is append-only'); END;

CREATE TRIGGER imm_u_mosaic_edge_invalidation BEFORE UPDATE ON mosaic_edge_invalidation BEGIN SELECT RAISE(ABORT, 'invalidation is append-only'); END;

CREATE TRIGGER imm_u_mosaic_head_history BEFORE UPDATE ON mosaic_head_history
WHEN NEW.mosaic_row_id <> OLD.mosaic_row_id
  OR NEW.generation <> OLD.generation
  OR NEW.head_revision_row_id <> OLD.head_revision_row_id
  OR NEW.accepted_sequence <> OLD.accepted_sequence
  OR (OLD.retired_sequence IS NOT NULL AND NEW.retired_sequence IS NOT OLD.retired_sequence)
  OR (OLD.retired_sequence IS NULL AND NEW.retired_sequence IS NULL)
BEGIN SELECT RAISE(ABORT, 'head history permits only one retirement closure'); END;

CREATE TRIGGER imm_u_mosaic_lineage BEFORE UPDATE ON mosaic_lineage BEGIN SELECT RAISE(ABORT, 'lineage is append-only'); END;

CREATE TRIGGER imm_u_panel_group_head_history BEFORE UPDATE ON panel_group_head_history
WHEN NEW.panel_group_row_id <> OLD.panel_group_row_id
  OR NEW.generation <> OLD.generation
  OR NEW.head_revision_row_id <> OLD.head_revision_row_id
  OR NEW.accepted_sequence <> OLD.accepted_sequence
  OR (OLD.retired_sequence IS NOT NULL AND NEW.retired_sequence IS NOT OLD.retired_sequence)
  OR (OLD.retired_sequence IS NULL AND NEW.retired_sequence IS NULL)
BEGIN SELECT RAISE(ABORT, 'head history permits only one retirement closure'); END;

CREATE TRIGGER imm_u_panel_group_lineage BEFORE UPDATE ON panel_group_lineage BEGIN SELECT RAISE(ABORT, 'lineage is append-only'); END;

CREATE TRIGGER imm_u_project_manifest BEFORE UPDATE ON project_manifest BEGIN SELECT RAISE(ABORT, 'manifest is append-only'); END;

CREATE TRIGGER imm_u_project_materialization_head_history BEFORE UPDATE ON project_materialization_head_history
WHEN NEW.project_row_id <> OLD.project_row_id
  OR NEW.generation <> OLD.generation
  OR NEW.head_snapshot_row_id <> OLD.head_snapshot_row_id
  OR NEW.accepted_sequence <> OLD.accepted_sequence
  OR (OLD.retired_sequence IS NOT NULL AND NEW.retired_sequence IS NOT OLD.retired_sequence)
  OR (OLD.retired_sequence IS NULL AND NEW.retired_sequence IS NULL)
BEGIN SELECT RAISE(ABORT, 'head history permits only one retirement closure'); END;

CREATE TRIGGER imm_u_project_materialization_snapshot BEFORE UPDATE ON project_materialization_snapshot BEGIN SELECT RAISE(ABORT, 'accepted snapshot is append-only'); END;

CREATE TRIGGER imm_u_project_materialization_snapshot_entry BEFORE UPDATE ON project_materialization_snapshot_entry BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_u_project_materialization_snapshot_session BEFORE UPDATE ON project_materialization_snapshot_session BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_u_project_membership_head_history BEFORE UPDATE ON project_membership_head_history
WHEN NEW.project_row_id <> OLD.project_row_id
  OR NEW.generation <> OLD.generation
  OR NEW.head_revision_row_id <> OLD.head_revision_row_id
  OR NEW.accepted_sequence <> OLD.accepted_sequence
  OR (OLD.retired_sequence IS NOT NULL AND NEW.retired_sequence IS NOT OLD.retired_sequence)
  OR (OLD.retired_sequence IS NULL AND NEW.retired_sequence IS NULL)
BEGIN SELECT RAISE(ABORT, 'head history permits only one retirement closure'); END;

CREATE TRIGGER imm_u_reclassification_apply_created_session BEFORE UPDATE ON reclassification_apply_created_session BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_u_reclassification_apply_result_snapshot BEFORE UPDATE ON reclassification_apply_result_snapshot BEGIN SELECT RAISE(ABORT, 'accepted snapshot is append-only'); END;

CREATE TRIGGER imm_u_reclassification_plan_result_frame BEFORE UPDATE ON reclassification_plan_result_frame BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_u_reclassification_plan_result_session BEFORE UPDATE ON reclassification_plan_result_session BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_u_reclassification_plan_result_snapshot BEFORE UPDATE ON reclassification_plan_result_snapshot BEGIN SELECT RAISE(ABORT, 'accepted snapshot is append-only'); END;

CREATE TRIGGER imm_u_reclassification_plan_revision BEFORE UPDATE ON reclassification_plan_revision BEGIN SELECT RAISE(ABORT, 'accepted revision is append-only'); END;

CREATE TRIGGER imm_u_relation_decision_mosaic_lineage BEFORE UPDATE ON relation_decision_mosaic_lineage BEGIN SELECT RAISE(ABORT, 'decision child is append-only'); END;

CREATE TRIGGER imm_u_relation_decision_mosaic_revision BEFORE UPDATE ON relation_decision_mosaic_revision BEGIN SELECT RAISE(ABORT, 'decision child is append-only'); END;

CREATE TRIGGER imm_u_relation_decision_panel_lineage BEFORE UPDATE ON relation_decision_panel_lineage BEGIN SELECT RAISE(ABORT, 'decision child is append-only'); END;

CREATE TRIGGER imm_u_relation_decision_panel_revision BEFORE UPDATE ON relation_decision_panel_revision BEGIN SELECT RAISE(ABORT, 'decision child is append-only'); END;

CREATE TRIGGER imm_u_relation_decision_retired_mosaic BEFORE UPDATE ON relation_decision_retired_mosaic BEGIN SELECT RAISE(ABORT, 'decision child is append-only'); END;

CREATE TRIGGER imm_u_relation_decision_retired_panel_group BEFORE UPDATE ON relation_decision_retired_panel_group BEGIN SELECT RAISE(ABORT, 'decision child is append-only'); END;

CREATE TRIGGER imm_u_relation_decision_session_supersession BEFORE UPDATE ON relation_decision_session_supersession BEGIN SELECT RAISE(ABORT, 'decision child is append-only'); END;

CREATE TRIGGER imm_u_relation_decision_snapshot BEFORE UPDATE ON relation_decision_snapshot BEGIN SELECT RAISE(ABORT, 'accepted decision is append-only'); END;

CREATE TRIGGER imm_u_relation_proposal_visibility_history BEFORE UPDATE ON relation_proposal_visibility_history
WHEN NEW.proposal_row_id <> OLD.proposal_row_id
  OR NEW.proposal_revision <> OLD.proposal_revision
  OR NEW.state <> OLD.state
  OR NEW.visible_sequence <> OLD.visible_sequence
  OR (OLD.hidden_sequence IS NOT NULL AND NEW.hidden_sequence IS NULL)
  OR (OLD.hidden_sequence IS NOT NULL AND NEW.hidden_sequence IS NOT OLD.hidden_sequence)
  OR (OLD.hidden_sequence IS NULL AND NEW.hidden_sequence IS NULL)
BEGIN SELECT RAISE(ABORT, 'visibility history permits only one closure'); END;

CREATE TRIGGER imm_u_relation_rejection BEFORE UPDATE ON relation_rejection BEGIN SELECT RAISE(ABORT, 'rejection is append-only'); END;

CREATE TRIGGER imm_u_session_equipment_resolution BEFORE UPDATE ON session_equipment_resolution BEGIN SELECT RAISE(ABORT, 'accepted history is append-only'); END;

CREATE TRIGGER imm_u_session_materialization_result_blocked_frame BEFORE UPDATE ON session_materialization_result_blocked_frame BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_u_session_materialization_result_frame BEFORE UPDATE ON session_materialization_result_frame BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_u_session_materialization_result_session BEFORE UPDATE ON session_materialization_result_session BEGIN SELECT RAISE(ABORT, 'snapshot child is append-only'); END;

CREATE TRIGGER imm_u_session_materialization_result_snapshot BEFORE UPDATE ON session_materialization_result_snapshot BEGIN SELECT RAISE(ABORT, 'accepted snapshot is append-only'); END;

CREATE TRIGGER imm_u_session_metadata_resolution BEFORE UPDATE ON session_metadata_resolution BEGIN SELECT RAISE(ABORT, 'accepted history is append-only'); END;

CREATE TRIGGER imm_u_session_supersession BEFORE UPDATE ON session_supersession BEGIN SELECT RAISE(ABORT, 'supersession is append-only'); END;

CREATE TRIGGER imm_u_session_visibility_history BEFORE UPDATE ON session_visibility_history
WHEN NEW.session_row_id <> OLD.session_row_id
  OR NEW.visible_sequence <> OLD.visible_sequence
  OR NEW.reason_code <> OLD.reason_code
  OR (OLD.hidden_sequence IS NOT NULL AND NEW.hidden_sequence IS NULL)
  OR (OLD.hidden_sequence IS NOT NULL AND NEW.hidden_sequence IS NOT OLD.hidden_sequence)
  OR (OLD.hidden_sequence IS NULL AND NEW.hidden_sequence IS NULL)
BEGIN SELECT RAISE(ABORT, 'visibility history permits only one closure'); END;

CREATE TRIGGER materialized_entry_immutable_delete BEFORE DELETE ON materialized_entry BEGIN
    SELECT RAISE(ABORT, 'materialized entry is append-only');
END;

CREATE TRIGGER materialized_entry_immutable_update BEFORE UPDATE ON materialized_entry BEGIN
    SELECT RAISE(ABORT, 'materialized entry is append-only');
END;

CREATE TRIGGER mosaic_revision_immutable_delete BEFORE DELETE ON mosaic_revision BEGIN
    SELECT RAISE(ABORT, 'mosaic revision is append-only');
END;

CREATE TRIGGER mosaic_revision_immutable_update BEFORE UPDATE ON mosaic_revision BEGIN
    SELECT RAISE(ABORT, 'mosaic revision is append-only');
END;

CREATE TRIGGER outbox_event_domain_immutable
BEFORE UPDATE OF command_row_id, event_ordinal, operation_row_id, proposal_row_id,
                 session_row_id, panel_group_row_id, mosaic_row_id, project_row_id,
                 handoff_row_id, event_type, payload_json, created_sequence, occurred_at
ON outbox_event BEGIN
    SELECT RAISE(ABORT, 'outbox domain fields are append-only');
END;

CREATE TRIGGER outbox_event_immutable_delete BEFORE DELETE ON outbox_event BEGIN
    SELECT RAISE(ABORT, 'outbox event is append-only');
END;

CREATE TRIGGER panel_membership_immutable_delete BEFORE DELETE ON panel_revision_session BEGIN
    SELECT RAISE(ABORT, 'panel membership is append-only');
END;

CREATE TRIGGER panel_membership_immutable_update BEFORE UPDATE ON panel_revision_session BEGIN
    SELECT RAISE(ABORT, 'panel membership is append-only');
END;

CREATE TRIGGER panel_revision_immutable_delete BEFORE DELETE ON panel_group_revision BEGIN
    SELECT RAISE(ABORT, 'panel revision is append-only');
END;

CREATE TRIGGER panel_revision_immutable_update BEFORE UPDATE ON panel_group_revision BEGIN
    SELECT RAISE(ABORT, 'panel revision is append-only');
END;

CREATE TRIGGER project_membership_immutable_delete BEFORE DELETE ON project_membership_revision BEGIN
    SELECT RAISE(ABORT, 'project membership revision is append-only');
END;

CREATE TRIGGER project_membership_immutable_update BEFORE UPDATE ON project_membership_revision BEGIN
    SELECT RAISE(ABORT, 'project membership revision is append-only');
END;

CREATE TRIGGER project_membership_session_immutable_delete BEFORE DELETE ON project_membership_revision_session BEGIN
    SELECT RAISE(ABORT, 'project session membership is append-only');
END;

CREATE TRIGGER project_membership_session_immutable_update BEFORE UPDATE ON project_membership_revision_session BEGIN
    SELECT RAISE(ABORT, 'project session membership is append-only');
END;

CREATE TRIGGER session_frame_immutable_delete BEFORE DELETE ON session_frame BEGIN
    SELECT RAISE(ABORT, 'session membership is append-only');
END;

CREATE TRIGGER session_frame_immutable_update BEFORE UPDATE ON session_frame BEGIN
    SELECT RAISE(ABORT, 'session membership is append-only');
END;

CREATE TRIGGER session_immutable_delete BEFORE DELETE ON session BEGIN
    SELECT RAISE(ABORT, 'session is append-only');
END;

CREATE TRIGGER session_immutable_update BEFORE UPDATE ON session BEGIN
    SELECT RAISE(ABORT, 'session is append-only');
END;

INSERT INTO "calibration_tolerances" VALUES('default', 5.0, 2.0, 365, 1, 1, '2026-05-26T00:00:00Z', 1);
INSERT INTO "cleanup_policy" VALUES('calibrated_lights', 'keep', '2026-05-26T00:00:00Z');
INSERT INTO "cleanup_policy" VALUES('registered_lights', 'keep', '2026-05-26T00:00:00Z');
INSERT INTO "cleanup_policy" VALUES('drizzle_data', 'keep', '2026-05-26T00:00:00Z');
INSERT INTO "cleanup_policy" VALUES('cosmetic_correction', 'keep', '2026-05-26T00:00:00Z');
INSERT INTO "cleanup_policy" VALUES('debayered_frames', 'keep', '2026-05-26T00:00:00Z');
INSERT INTO "cleanup_policy" VALUES('master_bias', 'keep', '2026-05-26T00:00:00Z');
INSERT INTO "cleanup_policy" VALUES('master_dark', 'keep', '2026-05-26T00:00:00Z');
INSERT INTO "cleanup_policy" VALUES('master_flat', 'keep', '2026-05-26T00:00:00Z');
INSERT INTO "cleanup_policy" VALUES('master_light', 'keep', '2026-05-26T00:00:00Z');
INSERT INTO "cleanup_policy" VALUES('processing_logs', 'keep', '2026-05-26T00:00:00Z');
INSERT INTO "cleanup_policy" VALUES('sequence_files', 'keep', '2026-05-26T00:00:00Z');
INSERT INTO "cleanup_policy" VALUES('light_subs_with_master', 'keep', '2026-05-26T00:00:00Z');
INSERT INTO "cleanup_policy" VALUES('dark_subs_with_master', 'keep', '2026-05-26T00:00:00Z');
INSERT INTO "cleanup_policy" VALUES('flat_subs_with_master', 'keep', '2026-05-26T00:00:00Z');
INSERT INTO "cleanup_policy" VALUES('bias_subs_with_master', 'keep', '2026-05-26T00:00:00Z');
INSERT INTO "filters" VALUES('a0000000-0000-4000-8000-000000000001', 'Ha', 'narrowband', 0, '2026-05-26T00:00:00Z');
INSERT INTO "filters" VALUES('a0000000-0000-4000-8000-000000000002', 'SII', 'narrowband', 0, '2026-05-26T00:00:00Z');
INSERT INTO "filters" VALUES('a0000000-0000-4000-8000-000000000003', 'OIII', 'narrowband', 0, '2026-05-26T00:00:00Z');
INSERT INTO "filters" VALUES('a0000000-0000-4000-8000-000000000004', 'NII', 'narrowband', 0, '2026-05-26T00:00:00Z');
INSERT INTO "filters" VALUES('a0000000-0000-4000-8000-000000000005', 'L', 'broadband', 0, '2026-05-26T00:00:00Z');
INSERT INTO "filters" VALUES('a0000000-0000-4000-8000-000000000006', 'R', 'broadband', 0, '2026-05-26T00:00:00Z');
INSERT INTO "filters" VALUES('a0000000-0000-4000-8000-000000000007', 'G', 'broadband', 0, '2026-05-26T00:00:00Z');
INSERT INTO "filters" VALUES('a0000000-0000-4000-8000-000000000008', 'B', 'broadband', 0, '2026-05-26T00:00:00Z');
INSERT INTO "filters" VALUES('a0000000-0000-4000-8000-000000000009', 'HO', 'dual_band', 0, '2026-05-26T00:00:00Z');
INSERT INTO "filters" VALUES('a0000000-0000-4000-8000-00000000000a', 'SO', 'dual_band', 0, '2026-05-26T00:00:00Z');
INSERT INTO "filters" VALUES('a0000000-0000-4000-8000-00000000000b', 'UV/IR Cut', 'other', 0, '2026-05-26T00:00:00Z');
INSERT INTO "ingestion_settings" VALUES('default', 1, 1, 0, 0, 0, 1, 2.0, 5.0, NULL, '2026-05-26T00:00:00Z');
INSERT INTO "protection_defaults" VALUES('global', 'defaultProtection', '"protected"', '1970-01-01T00:00:00Z');
INSERT INTO "protection_defaults" VALUES('global', 'blockPermanentDelete', 'true', '1970-01-01T00:00:00Z');
INSERT INTO "protection_defaults" VALUES('global', 'protectedCategories', '["lights","masters","finals"]', '1970-01-01T00:00:00Z');
INSERT INTO "resolver_settings" VALUES(1, 1, 'https://simbad.cds.unistra.fr/simbad/sim-tap/sync', 300, 10);
INSERT INTO "source_view_config" VALUES('default', 'junctions', '2026-05-26T00:00:00Z');
