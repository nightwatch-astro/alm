// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! App-owned project envelope and folder-structure rules (spec 008 F-1).
//!
//! Defines the expected on-disk folder layout for each `ProcessingTool` and
//! the format of the app-owned project marker file.
//!
//! Constitution III: this crate does NOT invoke `PixInsight` or any processing
//! tool. It only specifies the folder shape the app creates.
//!
//! Spec 024 adds:
//! - `manifest` — manifest writer (filename, markdown rendering, disk write).
//! - `notes` — notes file adapter (atomic read/write of `notes/project-notes.md`).

pub mod manifest;
pub mod notes;

pub use manifest::{
    manifest_relative_path, now_utc_iso, render_manifest_markdown, write_manifest_file,
    ManifestBody, ManifestReason, ManifestWriteResult, MANIFEST_VERSION,
};
pub use notes::{NotesFileAdapter, RealNotesAdapter, NOTES_FILENAME};

pub const CRATE_NAME: &str = "project_structure";

/// The processing tool a project targets.
///
/// Values must stay in sync with `domain_core::lifecycle::project::ProcessingTool`
/// and the JSON Schema `ProcessingTool` enum in the contracts.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProcessingTool {
    PixInsight,
    Siril,
    PlanetarySuite,
}

impl ProcessingTool {
    /// Parse from the canonical string representation stored in the DB.
    ///
    /// # Errors
    ///
    /// Returns `Err` when the string is not a known tool value.
    pub fn parse(s: &str) -> Result<Self, UnknownTool> {
        match s {
            "PixInsight" => Ok(Self::PixInsight),
            "Siril" => Ok(Self::Siril),
            "Planetary Suite" => Ok(Self::PlanetarySuite),
            other => Err(UnknownTool(other.to_owned())),
        }
    }

    /// Canonical string representation (matches DB and contract enum).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PixInsight => "PixInsight",
            Self::Siril => "Siril",
            Self::PlanetarySuite => "Planetary Suite",
        }
    }
}

/// Error returned when parsing an unknown tool string.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnknownTool(pub String);

impl std::fmt::Display for UnknownTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown processing tool: {}", self.0)
    }
}

// ── Folder layout ─────────────────────────────────────────────────────────────

/// A relative folder path that should exist inside a project directory.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequiredFolder(pub String);

/// Return the set of sub-folders that the app should create inside the project
/// root for the given tool.
///
/// Paths are relative to the project root (no leading slash).
///
/// `PixInsight` layout follows the WBPP-centric convention from research R3:
///   `lights/`, `darks/`, `flats/`, `bias/`, `output/`, `processing/`
///
/// Siril layout follows the Siril sequence convention:
///   `lights/`, `darks/`, `flats/`, `bias/`, `output/`
///
/// Planetary Suite layout is minimal (planetary stacking uses a flat folder):
///   `captures/`, `output/`
#[must_use]
pub fn required_folders(tool: ProcessingTool) -> Vec<RequiredFolder> {
    match tool {
        ProcessingTool::PixInsight => vec![
            RequiredFolder("lights".to_owned()),
            RequiredFolder("darks".to_owned()),
            RequiredFolder("flats".to_owned()),
            RequiredFolder("bias".to_owned()),
            RequiredFolder("output".to_owned()),
            RequiredFolder("processing".to_owned()),
        ],
        ProcessingTool::Siril => vec![
            RequiredFolder("lights".to_owned()),
            RequiredFolder("darks".to_owned()),
            RequiredFolder("flats".to_owned()),
            RequiredFolder("bias".to_owned()),
            RequiredFolder("output".to_owned()),
        ],
        ProcessingTool::PlanetarySuite => {
            vec![RequiredFolder("captures".to_owned()), RequiredFolder("output".to_owned())]
        }
    }
}

// ── Project marker ────────────────────────────────────────────────────────────

/// The filename written as the app-owned project marker inside the project root.
///
/// The file is JSON; its presence identifies the folder as an app-managed
/// project. The content is versioned so onboard reconciliation can detect old
/// format markers from previous app versions.
pub const MARKER_FILENAME: &str = ".astro-plan-project.json";

/// Marker format version. Bump when the marker schema changes.
pub const MARKER_VERSION: &str = "1";

/// On-disk shape of the project marker (spec 042, T207).
///
/// `version` and `projectId` are both optional at the deserialize level so a
/// malformed-but-valid-JSON marker yields a structured `MissingField` error
/// rather than a serde decode error, preserving the prior parser's error
/// semantics (version checked before projectId).
#[derive(serde::Serialize, serde::Deserialize)]
struct MarkerDoc {
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(rename = "projectId", skip_serializing_if = "Option::is_none")]
    project_id: Option<String>,
}

/// Render a project marker as a JSON string to be written into
/// `<project_root>/<MARKER_FILENAME>`.
///
/// The marker intentionally contains only the project id and format version;
/// the authoritative metadata lives in the database (Constitution V).
///
/// spec 042 (T207): serialized via `serde_json` (pretty-printed, 2-space
/// indent, trailing newline) instead of a hand-rolled `format!`, keeping the
/// human-readable on-disk shape and round-trip with [`parse_marker`].
#[must_use]
pub fn render_marker(project_id: &str) -> String {
    let doc = MarkerDoc {
        version: Some(MARKER_VERSION.to_owned()),
        project_id: Some(project_id.to_owned()),
    };
    // `to_string_pretty` cannot fail for this fixed, finite struct.
    let mut s = serde_json::to_string_pretty(&doc).unwrap_or_default();
    s.push('\n');
    s
}

/// Parse the `projectId` field from a marker string.
///
/// Returns `Err(ParseMarkerError)` when the string is not valid JSON, the
/// format version is unrecognised, or the required fields are missing.
///
/// # Errors
///
/// Returns `Err(ParseMarkerError::InvalidJson)` on non-JSON input.
/// Returns `Err(ParseMarkerError::UnknownVersion)` on unrecognised version.
/// Returns `Err(ParseMarkerError::MissingField)` when `version`/`projectId` is absent.
pub fn parse_marker(raw: &str) -> Result<ParsedMarker, ParseMarkerError> {
    // spec 042 (T207): parse via serde_json instead of the hand-rolled string
    // scanner. Error ordering is preserved: version absence/mismatch is reported
    // before a missing projectId.
    let doc: MarkerDoc = serde_json::from_str(raw).map_err(|_| ParseMarkerError::InvalidJson)?;
    let version = doc.version.ok_or(ParseMarkerError::MissingField("version"))?;
    if version != MARKER_VERSION {
        return Err(ParseMarkerError::UnknownVersion(version));
    }
    let project_id = doc.project_id.ok_or(ParseMarkerError::MissingField("projectId"))?;
    Ok(ParsedMarker { project_id })
}

/// Extracted fields from a project marker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedMarker {
    pub project_id: String,
}

/// Errors from [`parse_marker`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParseMarkerError {
    /// The marker is not valid JSON.
    InvalidJson,
    MissingField(&'static str),
    UnknownVersion(String),
}

impl std::fmt::Display for ParseMarkerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidJson => write!(f, "marker is not valid JSON"),
            Self::MissingField(name) => write!(f, "marker missing required field: {name}"),
            Self::UnknownVersion(v) => write!(f, "unrecognised marker version: {v}"),
        }
    }
}

// ── Working-folder resolution ─────────────────────────────────────────────────

/// Resolve the working folder to pass to a processing tool for the given
/// project (spec 011 T019, US3).
///
/// Resolution rule:
/// 1. When the project has a generated source-view folder (spec 026), prefer
///    it.  The caller passes an `Option<&str>` for this — typically loaded
///    from the project's `source_view_folder` DB column.
/// 2. Otherwise fall back to the project root.
///
/// The source-view folder is configuration, so it is resolved inside the project
/// envelope rather than trusted: a relative `..` escaped the project root and an
/// absolute value replaced it outright, taking every later project artifact
/// (prepared view, manifest, notes) outside the project with it
/// (astro-plan-3v3r.1.16).
///
/// # Errors
///
/// [`fs_pathsafe::contain::ContainmentError`] when the source-view folder does
/// not resolve inside `project_root`.
pub fn resolve_working_folder(
    project_root: &std::path::Path,
    source_view_folder: Option<&str>,
) -> Result<std::path::PathBuf, fs_pathsafe::contain::ContainmentError> {
    match source_view_folder.filter(|s| !s.trim().is_empty()) {
        Some(sv) => fs_pathsafe::contain::resolve_in_root(project_root, std::path::Path::new(sv))
            .map(fs_pathsafe::contain::ContainedPath::into_path_buf),
        None => Ok(project_root.to_path_buf()),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_crate_name() {
        // Source of truth is Cargo.toml's package name, not a second hand-typed
        // literal in this file — catches CRATE_NAME drifting from the manifest.
        assert_eq!(CRATE_NAME, env!("CARGO_PKG_NAME"));
    }

    #[test]
    fn pixinsight_layout_has_six_folders() {
        let folders = required_folders(ProcessingTool::PixInsight);
        assert_eq!(folders.len(), 6);
        let names: Vec<&str> = folders.iter().map(|f| f.0.as_str()).collect();
        assert!(names.contains(&"lights"));
        assert!(names.contains(&"processing"));
    }

    #[test]
    fn siril_layout_has_five_folders() {
        let folders = required_folders(ProcessingTool::Siril);
        assert_eq!(folders.len(), 5);
        assert!(!folders.iter().map(|f| f.0.as_str()).any(|n| n == "processing"));
    }

    #[test]
    fn planetary_suite_layout_has_two_folders() {
        let folders = required_folders(ProcessingTool::PlanetarySuite);
        assert_eq!(folders.len(), 2);
    }

    #[test]
    fn marker_round_trips() {
        let marker = render_marker("proj-abc-123");
        let parsed = parse_marker(&marker).unwrap();
        assert_eq!(parsed.project_id, "proj-abc-123");
    }

    #[test]
    fn parse_marker_rejects_unknown_version() {
        let bad = r#"{ "version": "99", "projectId": "x" }"#;
        assert!(matches!(parse_marker(bad), Err(ParseMarkerError::UnknownVersion(_))));
    }

    #[test]
    fn parse_marker_rejects_missing_project_id() {
        let bad = r#"{ "version": "1" }"#;
        assert_eq!(parse_marker(bad), Err(ParseMarkerError::MissingField("projectId")));
    }

    #[test]
    fn processing_tool_from_str_roundtrips() {
        for s in ["PixInsight", "Siril", "Planetary Suite"] {
            let tool = ProcessingTool::parse(s).unwrap();
            assert_eq!(tool.as_str(), s);
        }
    }

    #[test]
    fn processing_tool_from_str_rejects_unknown() {
        assert!(ProcessingTool::parse("Photoshop").is_err());
    }

    #[test]
    fn resolve_working_folder_uses_project_root_when_no_source_view() {
        let root = std::path::Path::new("/mnt/library/my_project");
        let result = resolve_working_folder(root, None).unwrap();
        assert_eq!(result, root);
    }

    #[test]
    fn resolve_working_folder_uses_source_view_when_absolute() {
        let root = std::path::Path::new("/mnt/library/my_project");
        let sv = "/mnt/library/my_project/_source_view";
        let result = resolve_working_folder(root, Some(sv)).unwrap();
        assert_eq!(result, std::path::Path::new(sv));
    }

    #[test]
    fn resolve_working_folder_joins_relative_source_view() {
        let root = std::path::Path::new("/mnt/library/my_project");
        let result = resolve_working_folder(root, Some("_source_view")).unwrap();
        assert_eq!(result, root.join("_source_view"));
    }

    #[test]
    fn resolve_working_folder_treats_blank_as_missing() {
        let root = std::path::Path::new("/mnt/library/my_project");
        let result = resolve_working_folder(root, Some("  ")).unwrap();
        assert_eq!(result, root);
    }

    /// astro-plan-3v3r.1.16: a relative source-view folder that traverses out of
    /// the project takes every later project artifact with it.
    #[test]
    fn a_relative_source_view_folder_never_escapes_the_project_root() {
        let root = std::path::Path::new("/mnt/library/my_project");
        let err = resolve_working_folder(root, Some("../../elsewhere")).unwrap_err();
        assert!(matches!(err, fs_pathsafe::contain::ContainmentError::Escapes { .. }), "{err}");
    }

    /// astro-plan-3v3r.1.16: an absolute source-view folder replaced the project
    /// root entirely, because `Path::join` discards the base.
    #[test]
    fn an_absolute_source_view_folder_must_not_replace_the_project_root() {
        let root = std::path::Path::new("/mnt/library/my_project");
        let err = resolve_working_folder(root, Some("/etc")).unwrap_err();
        assert!(matches!(err, fs_pathsafe::contain::ContainmentError::Escapes { .. }), "{err}");
    }

    /// A source-view folder is resolved before it exists, so absence is not a
    /// refusal: the verdict is lexical.
    #[test]
    fn a_source_view_folder_that_does_not_exist_yet_resolves() {
        let root = std::path::Path::new("/mnt/library/my_project");
        let result = resolve_working_folder(root, Some("_source_view/not/created")).unwrap();
        assert_eq!(result, root.join("_source_view/not/created"));
    }
}
