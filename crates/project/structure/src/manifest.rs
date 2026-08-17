// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Manifest writer for spec 024.
//!
//! Writes versioned project manifest snapshots to the project's `notes/`
//! folder. Each manifest is a markdown file with YAML front-matter and is
//! indexed in the database by the caller.
//!
//! Constitution II: manifest files are never overwritten. Each call produces a
//! new timestamped filename. Retry on failure produces a new filename with a
//! later timestamp.
//! Constitution III: manifests document inputs/outputs only; no image data is
//! read or modified.
//! Constitution V: the DB row is the durable record; the file on disk is the
//! reproducible projection.

use std::path::{Path, PathBuf};

/// Current front-matter schema version written by this module.
pub const MANIFEST_VERSION: i64 = 1;

/// Reason a manifest was generated (mirrors the DB enum).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManifestReason {
    Created,
    SourceChange,
    LifecycleTransition,
    CleanupApplied,
    WorkflowRun,
}

impl ManifestReason {
    /// Canonical `snake_case` string stored in the database and in file names.
    #[must_use]
    pub const fn as_db_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::SourceChange => "source_change",
            Self::LifecycleTransition => "lifecycle_transition",
            Self::CleanupApplied => "cleanup_applied",
            Self::WorkflowRun => "workflow_run",
        }
    }

    /// Short slug used in the filename (hyphen-separated).
    #[must_use]
    pub const fn filename_slug(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::SourceChange => "source-change",
            Self::LifecycleTransition => "lifecycle-transition",
            Self::CleanupApplied => "cleanup-applied",
            Self::WorkflowRun => "workflow-run",
        }
    }

    /// Parse from a DB/contract string.
    ///
    /// # Errors
    /// Returns `Err` for unknown values.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "created" => Ok(Self::Created),
            "source_change" => Ok(Self::SourceChange),
            "lifecycle_transition" => Ok(Self::LifecycleTransition),
            "cleanup_applied" => Ok(Self::CleanupApplied),
            "workflow_run" => Ok(Self::WorkflowRun),
            other => Err(format!("unknown manifest reason: {other}")),
        }
    }
}

/// Structured body embedded into the manifest (also stored as JSON in DB).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestBody {
    /// Project lifecycle state at snapshot time.
    pub lifecycle_state: String,
    /// Snapshot of linked source ids (role → list of inventory ids).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_map: Option<serde_json::Value>,
    /// Optional calibration choice snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calibration: Option<serde_json::Value>,
    /// Workflow profile id, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_profile: Option<String>,
    /// Generated source-view refs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub generated_views: Vec<GeneratedViewRef>,
    /// Full text snapshot of notes at write time (A8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// A reference to a generated source view.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct GeneratedViewRef {
    pub id: String,
    pub path: String,
}

/// Result of a manifest write operation.
#[derive(Clone, Debug)]
pub struct ManifestWriteResult {
    /// The project-relative path that was written.
    pub relative_path: String,
    /// The absolute path written (for audit).
    pub absolute_path: PathBuf,
    /// Timestamp used in the filename and stored in the DB row.
    pub timestamp: String,
}

/// Build the project-relative path for a manifest file.
///
/// Format: `notes/manifest-YYYY-MM-DD-HHMMSS-<reason>.md`
///
/// `timestamp` must be an RFC-3339 UTC string; if parsing fails, the literal
/// string is embedded without formatting.
#[must_use]
pub fn manifest_relative_path(reason: ManifestReason, timestamp: &str) -> String {
    // Parse timestamp to format without colons (safe for filenames on all OSes).
    let slug = if let Ok(dt) =
        time::OffsetDateTime::parse(timestamp, &time::format_description::well_known::Rfc3339)
    {
        format!(
            "{:04}-{:02}-{:02}-{:02}{:02}{:02}",
            dt.year(),
            dt.month() as u8,
            dt.day(),
            dt.hour(),
            dt.minute(),
            dt.second(),
        )
    } else {
        timestamp.replace([':', 'T', 'Z'], "-")
    };
    format!("notes/manifest-{slug}-{}.md", reason.filename_slug())
}

/// Render the markdown body for a manifest file.
///
/// Format:
/// ```text
/// ---
/// version: 1
/// reason: created
/// project_id: <id>
/// timestamp: <ts>
/// lifecycle_state: <state>
/// ---
///
/// # Project Manifest — <reason> — <timestamp>
///
/// **Lifecycle**: <state>
///
/// <notes snapshot if present>
/// ```
#[must_use]
#[allow(clippy::doc_markdown)]
pub fn render_manifest_markdown(
    project_id: &str,
    reason: ManifestReason,
    timestamp: &str,
    body: &ManifestBody,
) -> String {
    use std::fmt::Write as _;

    let mut md = String::new();

    // YAML front-matter
    md.push_str("---\n");
    let _ = writeln!(md, "version: {MANIFEST_VERSION}");
    let _ = writeln!(md, "reason: {}", reason.as_db_str());
    let _ = writeln!(md, "project_id: {project_id}");
    let _ = writeln!(md, "timestamp: {timestamp}");
    let _ = writeln!(md, "lifecycle_state: {}", body.lifecycle_state);
    if let Some(wp) = &body.workflow_profile {
        let _ = writeln!(md, "workflow_profile: {wp}");
    }
    md.push_str("---\n\n");

    // Human-readable header
    let _ = writeln!(md, "# Project Manifest — {} — {timestamp}\n", reason.as_db_str());
    let _ = writeln!(md, "**Lifecycle**: {}\n", body.lifecycle_state);

    // Notes snapshot (A8: full text, not a reference)
    if let Some(notes) = &body.notes {
        if !notes.is_empty() {
            md.push_str("## Notes (snapshot)\n\n");
            md.push_str(notes);
            md.push('\n');
        }
    }

    md
}

/// Write a manifest file to disk, creating the `notes/` folder if needed.
///
/// - The target file is `notes_dir / filename`.
/// - Returns `Err` if the directory cannot be created or the file cannot be
///   written (e.g. permission denied).
/// - If a file already exists at the path (very unlikely given timestamp
///   uniqueness), the write is skipped and the existing path is returned.
///
/// # Errors
/// Returns a descriptive string on I/O failure.
///
/// spec 042 (T251): the I/O is synchronous `std::fs`; the function stays
/// `async` so the caller's `.await` and the public signature are unchanged
/// while `project_structure` no longer depends on `tokio`. Behaviour (dir
/// creation, never-overwrite idempotency, written bytes) is identical.
#[allow(clippy::unused_async)]
pub async fn write_manifest_file(
    notes_dir: &Path,
    filename: &str,
    markdown: &str,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(notes_dir)
        .map_err(|e| format!("create notes dir {}: {e}", notes_dir.display()))?;

    let target = notes_dir.join(filename);

    // Constitution II: never overwrite. If the exact same timestamp+reason
    // file already exists we treat it as idempotent success.
    if target.exists() {
        return Ok(target);
    }

    std::fs::write(&target, markdown.as_bytes())
        .map_err(|e| format!("write manifest {}: {e}", target.display()))?;

    Ok(target)
}

/// Build the current UTC timestamp used for both the filename and the DB row.
///
/// Delegates to the canonical `domain_core::ids::Timestamp::now_iso()` (bd astro-plan-kyo7.88).
#[must_use]
pub fn now_utc_iso() -> String {
    domain_core::ids::Timestamp::now_iso()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_reason_roundtrip() {
        for reason in [
            ManifestReason::Created,
            ManifestReason::SourceChange,
            ManifestReason::LifecycleTransition,
            ManifestReason::CleanupApplied,
            ManifestReason::WorkflowRun,
        ] {
            assert_eq!(ManifestReason::parse(reason.as_db_str()).unwrap(), reason);
        }
    }

    #[test]
    fn manifest_relative_path_format() {
        let ts = "2026-04-12T18:01:00Z";
        let path = manifest_relative_path(ManifestReason::Created, ts);
        assert_eq!(path, "notes/manifest-2026-04-12-180100-created.md");
    }

    #[test]
    fn manifest_relative_path_workflow_run() {
        let ts = "2026-05-20T09:30:45Z";
        let path = manifest_relative_path(ManifestReason::WorkflowRun, ts);
        assert_eq!(path, "notes/manifest-2026-05-20-093045-workflow-run.md");
    }

    #[test]
    fn render_manifest_markdown_contains_front_matter() {
        let body = ManifestBody {
            lifecycle_state: "ready".to_owned(),
            notes: Some("My notes".to_owned()),
            ..Default::default()
        };
        let md = render_manifest_markdown(
            "proj-1",
            ManifestReason::Created,
            "2026-01-01T00:00:00Z",
            &body,
        );
        assert!(md.contains("version: 1"));
        assert!(md.contains("reason: created"));
        assert!(md.contains("project_id: proj-1"));
        assert!(md.contains("lifecycle_state: ready"));
        assert!(md.contains("My notes"));
    }

    #[test]
    fn render_manifest_markdown_no_notes_when_empty() {
        let body = ManifestBody {
            lifecycle_state: "setup_incomplete".to_owned(),
            notes: None,
            ..Default::default()
        };
        let md = render_manifest_markdown(
            "proj-2",
            ManifestReason::SourceChange,
            "2026-01-02T00:00:00Z",
            &body,
        );
        assert!(!md.contains("Notes (snapshot)"));
    }

    #[tokio::test]
    async fn write_manifest_file_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let notes_dir = dir.path().join("notes");
        let markdown = "---\nversion: 1\n---\n\n# Test\n";
        let filename = "manifest-2026-01-01-000000-created.md";
        let result = write_manifest_file(&notes_dir, filename, markdown).await.unwrap();
        assert!(result.exists());
        let content = std::fs::read_to_string(&result).unwrap();
        assert_eq!(content, markdown);
    }

    #[tokio::test]
    async fn write_manifest_file_idempotent_if_exists() {
        let dir = tempfile::tempdir().unwrap();
        let notes_dir = dir.path().join("notes");
        let filename = "manifest-2026-01-01-000000-created.md";
        let md = "first";
        write_manifest_file(&notes_dir, filename, md).await.unwrap();
        // Second write with different content — should NOT overwrite.
        write_manifest_file(&notes_dir, filename, "second").await.unwrap();
        let content = std::fs::read_to_string(notes_dir.join(filename)).unwrap();
        assert_eq!(content, "first");
    }
}
