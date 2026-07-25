// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Spec 017 US1 + spec 030 cleanup candidate generator.
//!
//! Two-step flow (D11):
//!   1. [`scan`] — pure, read-only preview. Enumerates a project's observed
//!      processing artifacts, classifies each into a [`DataType`], applies the
//!      persisted `CleanupPolicy`, and returns candidate files plus reclaimable
//!      bytes. NO plan row is created and NO filesystem mutation occurs (FR-002).
//!   2. [`generate`] — materialises a reviewable cleanup plan from the same
//!      candidates by building `CleanupPlanItem`s and delegating to the spec-016
//!      persistence tail [`crate::protection::generate_cleanup_plan`], which
//!      resolves per-item protection and gates approval.
//!
//! ## Read path (documented decision)
//!
//! A project's on-disk files are enumerated from the `processing_artifacts`
//! table (spec 012 artifact observation), the ONLY per-project file store that
//! the real pipeline populates: the filesystem watcher observes output files
//! under a project's folder and records `path`, `kind`, `size_bytes`, and a
//! rule/override classification. We call
//! [`persistence_plans::repositories::artifacts::list_artifacts_for_project`]
//! directly.
//!
//! Raw sub-frame cleanup (e.g. "light subs now covered by a master") is a
//! SEPARATE scan/generate pair — [`scan_raw_frames`]/[`generate_raw_frame_plan`]
//! (spec 048 US3) — rather than a `DataType` extension of the flow above: it
//! enumerates present `file_record` rows (root- or session-scoped) via
//! `crate::frame_inventory::list_frames` rather than `processing_artifacts`,
//! which is a different recorded-inventory source with a different identity
//! shape (`frame_id`/`session_id` vs `file_path`/`data_type`). Both scan/generate
//! pairs share the same [`crate::protection::generate_cleanup_plan`] tail (PR
//! #408 overlap guard, `.astro-plan-archive/<planId>/` destination).
//!
//! ## Classification model
//!
//! Grounded strictly in what inventory records: `processing_artifacts.kind` is
//! constrained to `intermediate | master | final` (spec 012's classification
//! pass; masters flow through here via spec 040 detection). We map that 1:1.
//! Anything unrecognised is [`DataType::Unclassified`] and is EXCLUDED from
//! cleanup candidates (safe default).
//!
//! ## Policy storage (D13)
//!
//! The `CleanupPolicy` is persisted through the existing generic
//! `protection_defaults` (scope, key, value-JSON) store (migration 0035) under
//! `scope = "cleanup"`, `key = "policy"`. The policy serialises cleanly to JSON,
//! so no new table or migration is required.

#![allow(clippy::doc_markdown)] // domain terminology not appropriate for backticks

mod generate;
mod policy;
mod raw_frames;
mod scan;
#[cfg(test)]
mod tests;

pub use generate::generate;
pub use policy::{default_cleanup_policy, get_policy, set_policy};
pub use raw_frames::{generate_raw_frame_plan, scan_raw_frames};
pub use scan::scan;

// ── Data-type classification model ─────────────────────────────────────────

/// Classification of a project file for cleanup purposes.
///
/// Grounded in `processing_artifacts.kind` (`intermediate | master | final`).
/// Unrecognised inputs are [`DataType::Unclassified`] and are excluded from
/// cleanup candidates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataType {
    /// Reproducible processing intermediates (calibrated/registered/drizzle/
    /// debayered frames, cosmetic correction, etc.). The primary safe-to-clean
    /// class.
    Intermediate,
    /// Master calibration frames (spec 040 detection surfaces as `kind=master`).
    Master,
    /// Final science outputs (integrations, finished images).
    Final,
    /// Unknown / not represented in recorded inventory — always excluded.
    Unclassified,
}

impl DataType {
    /// Canonical policy string for this data type.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Intermediate => "intermediate",
            Self::Master => "master",
            Self::Final => "final",
            Self::Unclassified => "unclassified",
        }
    }

    /// Map a `processing_artifacts.kind` value to a [`DataType`]. Unknown kinds
    /// become [`DataType::Unclassified`].
    #[must_use]
    pub fn from_artifact_kind(kind: &str) -> Self {
        match kind {
            "intermediate" => Self::Intermediate,
            "master" => Self::Master,
            "final" => Self::Final,
            _ => Self::Unclassified,
        }
    }

    /// Parse a canonical policy string back into a [`DataType`].
    #[must_use]
    pub fn from_policy_str(s: &str) -> Self {
        match s {
            "intermediate" => Self::Intermediate,
            "master" => Self::Master,
            "final" => Self::Final,
            _ => Self::Unclassified,
        }
    }

    /// Protected-category name used for protection resolution. Master and Final
    /// map to the default protected categories (`masters`, `finals`) so they
    /// gate approval; intermediates map to a non-protected category.
    #[must_use]
    pub fn protection_category(self) -> &'static str {
        match self {
            Self::Intermediate => "intermediate",
            Self::Master => "masters",
            Self::Final => "finals",
            Self::Unclassified => "unclassified",
        }
    }
}

// ── Shared helpers ────────────────────────────────────────────────────────

/// Take the tail of a project-relative path (the file name) for display.
fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}
