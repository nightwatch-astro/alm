// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Platform-specific detached process spawning (spec 011 T008).
//!
//! The actual spawn is hidden behind the [`ProcessSpawner`] trait so tests
//! can use a [`FakeSpawner`] that records commands without spawning real
//! processes.
//!
//! Platform rules:
//! - **Windows**: `creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)`.
//! - **macOS**: `open -b <bundle_id> --args <argv>` when `bundle_id` is set.
//!   Plain binary: `process_group(0)` for session detach.
//!   On quarantine/translocation error: `Err(LaunchError::MacOsQuarantine)`.
//! - **Linux**: `process_group(0)` for session detach.
//!
//! No `unsafe` code: `process_group(0)` is a stable safe API since Rust 1.64.
//! PID liveness uses `std::fs::metadata("/proc/<pid>")` on Linux and a
//! best-effort path on other Unix platforms (no libc).

use std::path::Path;

// ── Error type ────────────────────────────────────────────────────────────────

/// Errors that can occur during a spawn attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LaunchError {
    /// The OS spawn call failed with the given I/O error kind.
    SpawnFailed(String),
    /// macOS quarantine/translocation detected (R-MacQuarantine).
    MacOsQuarantine,
}

impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SpawnFailed(msg) => write!(f, "spawn failed: {msg}"),
            Self::MacOsQuarantine => write!(
                f,
                "macOS quarantined this app; run \
                 `xattr -dr com.apple.quarantine <path>` and retry"
            ),
        }
    }
}

// ── SpawnRequest ──────────────────────────────────────────────────────────────

/// Everything needed to spawn a single child process.
#[derive(Clone, Debug)]
pub struct SpawnRequest {
    pub executable: String,
    pub args: Vec<String>,
    pub working_dir: String,
    /// macOS bundle ID for `open -b` dispatch (may be `None`).
    pub bundle_id: Option<String>,
}

/// Result of a successful spawn.
#[derive(Clone, Debug)]
pub struct SpawnResult {
    /// OS PID, when the OS surfaces it before detach completes.
    pub pid: Option<u32>,
}

// ── ProcessSpawner trait ──────────────────────────────────────────────────────

/// Abstraction over the actual process-spawn operation.
///
/// Production code uses [`RealSpawner`]; tests use [`FakeSpawner`].
pub trait ProcessSpawner: Send + Sync {
    /// Spawn a detached process and return the PID if available.
    ///
    /// # Errors
    ///
    /// Returns `LaunchError::SpawnFailed` on OS error.
    /// Returns `LaunchError::MacOsQuarantine` when macOS quarantine is detected.
    fn spawn(&self, req: SpawnRequest) -> Result<SpawnResult, LaunchError>;
}

// ── RealSpawner ───────────────────────────────────────────────────────────────

/// Production spawner that calls into the real OS.
pub struct RealSpawner;

impl ProcessSpawner for RealSpawner {
    fn spawn(&self, req: SpawnRequest) -> Result<SpawnResult, LaunchError> {
        spawn_platform(req)
    }
}

// ── FakeSpawner ───────────────────────────────────────────────────────────────

/// Test-only spawner that records calls without starting real processes.
///
/// Thread-safe via `std::sync::Mutex`.
pub struct FakeSpawner {
    pub calls: std::sync::Mutex<Vec<SpawnRequest>>,
    /// When `Some(err)`, every call returns that error.
    pub error: Option<LaunchError>,
    /// PID to report on success.
    pub pid: Option<u32>,
}

impl FakeSpawner {
    /// Create a spawner that always succeeds, returning `pid = Some(1234)`.
    #[must_use]
    pub fn ok() -> Self {
        Self { calls: std::sync::Mutex::new(vec![]), error: None, pid: Some(1234) }
    }

    /// Create a spawner that always fails with the given error.
    #[must_use]
    pub fn failing(err: LaunchError) -> Self {
        Self { calls: std::sync::Mutex::new(vec![]), error: Some(err), pid: None }
    }

    /// Drain the recorded calls.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    pub fn drain(&self) -> Vec<SpawnRequest> {
        self.calls.lock().unwrap().drain(..).collect()
    }
}

impl ProcessSpawner for FakeSpawner {
    fn spawn(&self, req: SpawnRequest) -> Result<SpawnResult, LaunchError> {
        self.calls.lock().unwrap().push(req);
        if let Some(ref err) = self.error {
            return Err(err.clone());
        }
        Ok(SpawnResult { pid: self.pid })
    }
}

// ── Platform implementations ──────────────────────────────────────────────────

#[cfg(target_os = "windows")]
#[allow(clippy::needless_pass_by_value)] // signature kept uniform across platform variants
fn spawn_platform(req: SpawnRequest) -> Result<SpawnResult, LaunchError> {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    let mut cmd = std::process::Command::new(&req.executable);
    cmd.args(&req.args)
        .current_dir(&req.working_dir)
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    let child = cmd.spawn().map_err(|e| LaunchError::SpawnFailed(e.kind().to_string()))?;
    Ok(SpawnResult { pid: Some(child.id()) })
}

#[cfg(target_os = "macos")]
#[allow(clippy::needless_pass_by_value)] // signature kept uniform across platform variants
fn spawn_platform(req: SpawnRequest) -> Result<SpawnResult, LaunchError> {
    use std::os::unix::process::CommandExt;

    if let Some(ref bid) = req.bundle_id {
        // Use `open -b <bundle_id> --args <argv>` for .app bundles (R-BundleId).
        let mut cmd = std::process::Command::new("open");
        cmd.args(["-b", bid.as_str()]);
        if !req.args.is_empty() {
            cmd.arg("--args");
            cmd.args(&req.args);
        }
        let output = cmd.output().map_err(|e| LaunchError::SpawnFailed(e.kind().to_string()))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("quarantine") || stderr.contains("LSOpenURLsWithRole") {
                return Err(LaunchError::MacOsQuarantine);
            }
            return Err(LaunchError::SpawnFailed(stderr.into_owned()));
        }
        // `open -b` does not return the child PID.
        return Ok(SpawnResult { pid: None });
    }

    // Plain binary: process_group(0) acts as setsid-style detach (safe API since Rust 1.64).
    let mut cmd = std::process::Command::new(&req.executable);
    cmd.args(&req.args).current_dir(&req.working_dir).process_group(0);
    let child = cmd.spawn().map_err(|e| LaunchError::SpawnFailed(e.kind().to_string()))?;
    Ok(SpawnResult { pid: Some(child.id()) })
}

#[cfg(target_os = "linux")]
#[allow(clippy::needless_pass_by_value)] // SpawnRequest is consumed by the Command builder
fn spawn_platform(req: SpawnRequest) -> Result<SpawnResult, LaunchError> {
    use std::os::unix::process::CommandExt;
    // process_group(0) is a safe stable API since Rust 1.64 (setsid-equivalent).
    let mut cmd = std::process::Command::new(&req.executable);
    cmd.args(&req.args).current_dir(&req.working_dir).process_group(0);
    let child = cmd.spawn().map_err(|e| LaunchError::SpawnFailed(e.kind().to_string()))?;
    Ok(SpawnResult { pid: Some(child.id()) })
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn spawn_platform(_req: SpawnRequest) -> Result<SpawnResult, LaunchError> {
    Err(LaunchError::SpawnFailed("unsupported platform".to_owned()))
}

// ── pid_is_alive ──────────────────────────────────────────────────────────────

/// Best-effort check: return `true` when the OS still has a process with `pid`.
///
/// Uses `/proc/<pid>` presence on Linux; `kill -0` is not available without
/// `unsafe` code, so we fall back to a conservative `false` on other platforms.
/// The re-launch guard treats `false` as "no prior instance" (safe to launch).
#[must_use]
pub fn pid_is_alive(pid: u32) -> bool {
    pid_is_alive_impl(pid)
}

#[cfg(target_os = "linux")]
fn pid_is_alive_impl(pid: u32) -> bool {
    // `/proc/<pid>` exists as long as the process is alive on Linux.
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

#[cfg(not(target_os = "linux"))]
fn pid_is_alive_impl(_pid: u32) -> bool {
    // Conservative: assume not alive on platforms we cannot check safely.
    // The re-launch guard only fires when this returns `true`, so false negatives
    // cause the guard to be silently skipped — acceptable for v1.
    false
}

// ── Verify working-dir containment ────────────────────────────────────────────

/// Verify that `working_dir` resolves inside at least one registered library
/// root (R-CwdContain, FR-010).
///
/// The comparison normalizes both sides. A bare `Path::starts_with` compares
/// unnormalized text, which accepted `/mnt/library/../../a` as inside
/// `/mnt/library` whenever the caller's `canonicalize` had failed, as it does
/// for a directory that does not exist yet (astro-plan-3v3r.13.26).
///
/// An empty `library_roots` is refused: with nothing registered there is no root
/// for the working directory to be inside of, and reporting containment made the
/// check depend on configuration.
///
/// # Errors
///
/// Returns `Err("cwd.outside_library_root")` when no root contains the cwd.
pub fn verify_cwd_containment(
    working_dir: &Path,
    library_roots: &[&Path],
) -> Result<(), &'static str> {
    if fs_pathsafe::contain::contained_in_any(working_dir, library_roots) {
        Ok(())
    } else {
        Err("cwd.outside_library_root")
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_req() -> SpawnRequest {
        SpawnRequest {
            executable: "/usr/bin/pixinsight".to_owned(),
            args: vec!["/mnt/library/project".to_owned()],
            working_dir: "/mnt/library/project".to_owned(),
            bundle_id: None,
        }
    }

    #[test]
    fn fake_spawner_records_calls() {
        let spawner = FakeSpawner::ok();
        let req = sample_req();
        let result = spawner.spawn(req.clone());
        assert!(result.is_ok());
        let calls = spawner.drain();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].executable, req.executable);
    }

    #[test]
    fn fake_spawner_returns_pid() {
        let spawner = FakeSpawner::ok();
        let result = spawner.spawn(sample_req()).unwrap();
        assert_eq!(result.pid, Some(1234));
    }

    #[test]
    fn fake_spawner_failing_returns_error() {
        let spawner = FakeSpawner::failing(LaunchError::SpawnFailed("test".to_owned()));
        let result = spawner.spawn(sample_req());
        assert!(result.is_err());
        // Call still recorded even on failure
        assert_eq!(spawner.drain().len(), 1);
    }

    #[test]
    fn fake_spawner_quarantine_error() {
        let spawner = FakeSpawner::failing(LaunchError::MacOsQuarantine);
        let result = spawner.spawn(sample_req()).unwrap_err();
        assert_eq!(result, LaunchError::MacOsQuarantine);
    }

    #[test]
    fn cwd_containment_passes_when_inside_root() {
        let root = PathBuf::from("/mnt/library");
        let cwd = PathBuf::from("/mnt/library/project");
        assert!(verify_cwd_containment(&cwd, &[root.as_path()]).is_ok());
    }

    #[test]
    fn cwd_containment_fails_when_outside_roots() {
        let root = PathBuf::from("/mnt/library");
        let cwd = PathBuf::from("/tmp/scratch");
        assert_eq!(
            verify_cwd_containment(&cwd, &[root.as_path()]),
            Err("cwd.outside_library_root")
        );
    }

    #[test]
    fn cwd_containment_fails_with_no_roots() {
        let cwd = PathBuf::from("/anywhere");
        assert_eq!(verify_cwd_containment(&cwd, &[]), Err("cwd.outside_library_root"));
    }

    /// astro-plan-3v3r.13.26: the text prefix matches, the resolved path does not.
    #[test]
    fn cwd_containment_fails_for_a_path_that_traverses_out_of_a_root() {
        let root = PathBuf::from("/mnt/library");
        let cwd = PathBuf::from("/mnt/library/../../a");
        assert_eq!(
            verify_cwd_containment(&cwd, &[root.as_path()]),
            Err("cwd.outside_library_root")
        );
    }

    /// A working directory the app is about to create is not yet on disk, which
    /// is when the caller's `canonicalize` fails and the raw path arrives here.
    #[test]
    fn cwd_containment_passes_for_a_directory_that_does_not_exist_yet() {
        let root = PathBuf::from("/mnt/library");
        let cwd = PathBuf::from("/mnt/library/project/_source_view");
        assert!(verify_cwd_containment(&cwd, &[root.as_path()]).is_ok());
    }

    #[test]
    fn launch_error_display() {
        let e = LaunchError::SpawnFailed("io error".to_owned());
        assert!(e.to_string().contains("spawn failed"));
        assert!(LaunchError::MacOsQuarantine.to_string().contains("quarantined"));
    }
}
