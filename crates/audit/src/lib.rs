// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Audit event, operation history, and in-process event bus.
#![allow(clippy::doc_markdown)] // spec/domain terminology not appropriate for backticks

pub mod bus;
pub mod event;
pub mod event_bus;
pub mod pruner;
pub mod stale_propagator;

pub use bus::EventBus;
pub use event::{AuditLogEntry, Outcome, Severity, SeverityFilter};
pub use event_bus::{
    ArchivePermanentlyDeleted, ArchiveSentToTrash, ArtifactClassified, EventEnvelope,
    FirstRunCompleted, InventoryConfirmed, LifecycleTransitionApplied, PlanApplyingCompleted,
    PlanApplyingPaused, PlanApplyingResumed, PlanApplyingStarted, PlanApproved, PlanDiscarded,
    PlanItemProgress, PlanRetryCreated, ProtectionDefaultChanged, ProtectionPlanAcknowledged,
    ProtectionSourceSet, SettingsChanged, SettingsMigration, SettingsRepair, SettingsSnapshot,
    Source, SourceCountByKind, TargetResolveBatchCompleted, TargetResolved,
    TOPIC_ARCHIVE_PERMANENTLY_DELETED, TOPIC_ARCHIVE_SENT_TO_TRASH, TOPIC_ARTIFACT_CLASSIFIED,
    TOPIC_FIRST_RUN_COMPLETED, TOPIC_INVENTORY_CONFIRMED, TOPIC_LIFECYCLE_TRANSITION_APPLIED,
    TOPIC_PLAN_APPLYING_COMPLETED, TOPIC_PLAN_APPLYING_PAUSED, TOPIC_PLAN_APPLYING_RESUMED,
    TOPIC_PLAN_APPLYING_STARTED, TOPIC_PLAN_APPROVED, TOPIC_PLAN_DISCARDED,
    TOPIC_PLAN_ITEM_PROGRESS, TOPIC_PLAN_RETRY_CREATED, TOPIC_PROTECTION_DEFAULT_CHANGED,
    TOPIC_PROTECTION_PLAN_ACKNOWLEDGED, TOPIC_PROTECTION_SOURCE_SET, TOPIC_SETTINGS_CHANGED,
    TOPIC_SETTINGS_MIGRATION, TOPIC_SETTINGS_REPAIR, TOPIC_SETTINGS_SNAPSHOT,
    TOPIC_TARGET_RESOLVED, TOPIC_TARGET_RESOLVE_BATCH_COMPLETED,
};
pub use stale_propagator::{PropagatorFn, StalePropagator};

pub const CRATE_NAME: &str = "audit";
