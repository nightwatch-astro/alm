// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Validation and atomic write for a webview-supplied export destination.
//!
//! Constitution II ("MUST never overwrite silently") makes an existing
//! destination a rejection, not a replacement.
//!
//! The write target is always `canonical_parent.join(file_name)`, so no segment
//! of the caller's string can move the write outside the directory that was
//! canonicalized and checked. The temporary file is created inside that same
//! directory with `create_new`, which refuses to follow a symlink planted after
//! validation; a symlink planted at the final path after validation is replaced
//! by `rename` rather than written through.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// Why an export destination is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DestinationRejected {
    /// The path is relative. Export destinations cross a process boundary, so a
    /// path that resolves against the app's working directory is refused.
    NotAbsolute,
    /// The path has no parent component.
    NoParent,
    /// The path has no final component (it ends in `.` , `..`, or a separator).
    NoFileName,
    /// The parent directory does not exist or cannot be canonicalized.
    ParentMissing,
    /// The parent path exists but is not a directory.
    ParentNotDirectory,
    /// The final component is a Windows reserved device name.
    ReservedName,
    /// Something already exists at the destination.
    AlreadyExists,
}

impl DestinationRejected {
    /// Stable message for the caller's own error envelope.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::NotAbsolute => "export destination must be an absolute path",
            Self::NoParent => "export destination has no parent directory",
            Self::NoFileName => "export destination has no file name",
            Self::ParentMissing => "export destination parent directory does not exist",
            Self::ParentNotDirectory => "export destination parent is not a directory",
            Self::ReservedName => "export destination is a reserved device name",
            Self::AlreadyExists => "export destination already exists",
        }
    }
}

/// A destination that passed [`validate_export_destination`].
#[derive(Clone, Debug)]
pub struct ExportDestination {
    dir: PathBuf,
    final_path: PathBuf,
}

/// Validate `dest` as a write destination.
///
/// # Errors
/// Returns the first [`DestinationRejected`] rule the path violates.
pub fn validate_export_destination(dest: &Path) -> Result<ExportDestination, DestinationRejected> {
    if !dest.is_absolute() {
        return Err(DestinationRejected::NotAbsolute);
    }
    let parent = dest.parent().ok_or(DestinationRejected::NoParent)?;
    let file_name = dest.file_name().ok_or(DestinationRejected::NoFileName)?;

    // A verbatim Windows path (`\\?\C:\...`) gives `..` no special meaning, so
    // `file_name` returns it as an ordinary component and the traversal check
    // has to reject it here.
    let name = file_name.to_string_lossy();
    safe_filename::step3_traversal_check(&name).map_err(|_| DestinationRejected::NoFileName)?;

    // Windows resolves a device name from the text before the FIRST dot, so
    // `CON.foo.json` names the console where `file_stem` would keep `CON.foo`.
    let device = name.split('.').next().unwrap_or(&name);
    safe_filename::step4_reserved_name_check(device)
        .map_err(|_| DestinationRejected::ReservedName)?;

    let dir = parent.canonicalize().map_err(|_| DestinationRejected::ParentMissing)?;
    if !dir.is_dir() {
        return Err(DestinationRejected::ParentNotDirectory);
    }

    let final_path = dir.join(file_name);
    if final_path.symlink_metadata().is_ok() {
        return Err(DestinationRejected::AlreadyExists);
    }

    Ok(ExportDestination { dir, final_path })
}

/// Distinguishes a refused destination from a failure while writing to it.
#[derive(Debug)]
pub enum ExportWriteError {
    Rejected(DestinationRejected),
    Io(io::Error),
}

impl From<DestinationRejected> for ExportWriteError {
    fn from(value: DestinationRejected) -> Self {
        Self::Rejected(value)
    }
}

impl From<io::Error> for ExportWriteError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

/// Counter that keeps two concurrent exports in one directory off the same
/// temporary name; `create_new` is what makes a collision an error rather than
/// a silent share.
static TEMP_SEQ: AtomicU32 = AtomicU32::new(0);

impl ExportDestination {
    /// The validated write target.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.final_path
    }

    /// Write through a temporary file inside the validated directory, then
    /// rename it onto the destination. Returns the byte count written.
    ///
    /// The temporary file is removed when `write` fails.
    ///
    /// # Errors
    /// Returns [`ExportWriteError::Rejected`] with
    /// [`DestinationRejected::AlreadyExists`] when the destination appeared
    /// after validation, and [`ExportWriteError::Io`] for filesystem and
    /// serialisation failures.
    pub fn write_atomically<W>(&self, write: W) -> Result<u64, ExportWriteError>
    where
        W: FnOnce(&mut File) -> io::Result<()>,
    {
        let (temp_path, mut file) = self.create_temp()?;

        if let Err(e) = write(&mut file).and_then(|()| file.sync_all()) {
            drop(file);
            let _ = std::fs::remove_file(&temp_path);
            return Err(ExportWriteError::Io(e));
        }
        let bytes = file.metadata().map_or(0, |m| m.len());
        drop(file);

        if self.final_path.symlink_metadata().is_ok() {
            let _ = std::fs::remove_file(&temp_path);
            return Err(DestinationRejected::AlreadyExists.into());
        }
        if let Err(e) = std::fs::rename(&temp_path, &self.final_path) {
            let _ = std::fs::remove_file(&temp_path);
            return Err(ExportWriteError::Io(e));
        }
        Ok(bytes)
    }

    fn create_temp(&self) -> io::Result<(PathBuf, File)> {
        let pid = std::process::id();
        let mut last = None;
        for _ in 0..8 {
            let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
            let candidate = self.dir.join(format!(".export-{pid}-{seq}.tmp"));
            match File::options().write(true).create_new(true).open(&candidate) {
                Ok(file) => return Ok((candidate, file)),
                Err(e) => last = Some(e),
            }
        }
        Err(last.unwrap_or_else(|| io::Error::other("no temporary file name available")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// The canonical path matters on macOS, where `/var` is a symlink to
    /// `/private/var` and the validator returns the resolved parent.
    fn tempdir() -> (tempfile::TempDir, PathBuf) {
        let guard = tempfile::tempdir().expect("temp dir");
        let path = guard.path().canonicalize().expect("canonicalize temp dir");
        (guard, path)
    }

    fn write_hello(dest: &ExportDestination) -> Result<u64, ExportWriteError> {
        dest.write_atomically(|f| f.write_all(b"hello"))
    }

    #[test]
    fn relative_path_with_parent_escape_is_refused() {
        let err = validate_export_destination(Path::new("../../etc/passwd")).unwrap_err();
        assert_eq!(err, DestinationRejected::NotAbsolute);
    }

    #[test]
    fn absolute_path_outside_any_root_is_accepted_but_never_overwrites() {
        let (_guard, dir) = tempdir();
        let dest = dir.join("out.json");
        let validated = validate_export_destination(&dest).expect("fresh destination");
        assert_eq!(write_hello(&validated).expect("write"), 5);

        let err = validate_export_destination(&dest).unwrap_err();
        assert_eq!(err, DestinationRejected::AlreadyExists);
        assert_eq!(std::fs::read(&dest).expect("read back"), b"hello");
    }

    #[test]
    fn dotdot_inside_an_absolute_path_cannot_move_the_write_out_of_the_parent() {
        let (_guard, dir) = tempdir();
        let nested = dir.join("nested");
        std::fs::create_dir_all(&nested).expect("create nested");
        let dest = nested.join("..").join("out.json");

        let validated = validate_export_destination(&dest).expect("valid");
        assert_eq!(validated.path().parent(), Some(dir.as_path()));
        write_hello(&validated).expect("write");
        assert!(dir.join("out.json").exists());
    }

    #[test]
    fn parent_that_is_not_a_directory_is_refused() {
        let (_guard, dir) = tempdir();
        let file = dir.join("regular");
        std::fs::write(&file, b"x").expect("write file");

        let err = validate_export_destination(&file.join("child.json")).unwrap_err();
        assert!(
            matches!(
                err,
                DestinationRejected::ParentMissing | DestinationRejected::ParentNotDirectory
            ),
            "{err:?}"
        );
    }

    #[test]
    fn missing_parent_is_refused() {
        let (_guard, dir) = tempdir();
        let err = validate_export_destination(&dir.join("absent").join("out.json")).unwrap_err();
        assert_eq!(err, DestinationRejected::ParentMissing);
    }

    #[test]
    fn path_without_a_file_name_is_refused() {
        let (_guard, dir) = tempdir();
        // `join` collapses `..` on a verbatim Windows path, which would leave the
        // parent directory rather than a path terminating in `..`.
        let dest = PathBuf::from(format!("{}{}..", dir.display(), std::path::MAIN_SEPARATOR));

        let err = validate_export_destination(&dest).unwrap_err();
        assert_eq!(err, DestinationRejected::NoFileName);
    }

    #[test]
    fn windows_reserved_device_name_is_refused_on_every_platform() {
        let (_guard, dir) = tempdir();
        for name in ["CON", "con.json", "NUL.txt", "com1.json", "CON.foo.json", "nul.a.b"] {
            let err = validate_export_destination(&dir.join(name)).unwrap_err();
            assert_eq!(err, DestinationRejected::ReservedName, "{name}");
        }
    }

    #[test]
    fn existing_destination_is_refused_before_anything_is_written() {
        let (_guard, dir) = tempdir();
        let dest = dir.join("keep.json");
        std::fs::write(&dest, b"original").expect("seed");

        let err = validate_export_destination(&dest).unwrap_err();
        assert_eq!(err, DestinationRejected::AlreadyExists);
        assert_eq!(std::fs::read(&dest).expect("read"), b"original");
    }

    #[test]
    fn temporary_file_lands_inside_the_validated_directory() {
        let (_guard, dir) = tempdir();
        let validated = validate_export_destination(&dir.join("out.json")).expect("valid");
        validated
            .write_atomically(|f| {
                let seen: Vec<PathBuf> = std::fs::read_dir(&validated.dir)
                    .expect("read dir")
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .collect();
                assert_eq!(seen.len(), 1, "{seen:?}");
                assert_eq!(seen[0].parent(), Some(validated.dir.as_path()));
                f.write_all(b"x")
            })
            .expect("write");
    }

    /// Pins the behaviour the validator replaces: a bare `create` + `rename`
    /// pair reports success while destroying whatever was at the destination.
    /// Both halves must hold, so reverting either sink to that sequence fails
    /// this test.
    #[test]
    fn the_unvalidated_create_and_rename_sequence_replaces_an_existing_file() {
        let (_guard, dir) = tempdir();
        let dest = dir.join("keep.json");
        std::fs::write(&dest, b"user data").expect("seed");

        let temp = dir.join("keep.json.tmp");
        std::fs::File::create(&temp)
            .expect("create temp")
            .write_all(b"export")
            .expect("write temp");
        std::fs::rename(&temp, &dest).expect("rename replaces silently");
        assert_eq!(std::fs::read(&dest).expect("read"), b"export");

        assert_eq!(
            validate_export_destination(&dest).unwrap_err(),
            DestinationRejected::AlreadyExists
        );
    }

    #[test]
    fn a_failed_write_leaves_no_temporary_file_behind() {
        let (_guard, dir) = tempdir();
        let validated = validate_export_destination(&dir.join("out.json")).expect("valid");
        let err = validated
            .write_atomically(|_| Err(io::Error::other("serialise failed")))
            .expect_err("write fails");

        assert!(matches!(err, ExportWriteError::Io(_)), "{err:?}");
        assert_eq!(std::fs::read_dir(&dir).expect("read dir").count(), 0);
    }

    #[test]
    fn a_destination_appearing_after_validation_is_not_replaced() {
        let (_guard, dir) = tempdir();
        let dest = dir.join("out.json");
        let validated = validate_export_destination(&dest).expect("valid");

        let err = validated
            .write_atomically(|f| {
                std::fs::write(&dest, b"raced").expect("racing writer");
                f.write_all(b"export")
            })
            .expect_err("rename refused");

        assert!(
            matches!(err, ExportWriteError::Rejected(DestinationRejected::AlreadyExists)),
            "{err:?}"
        );
        assert_eq!(std::fs::read(&dest).expect("read"), b"raced");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_at_the_destination_is_refused_rather_than_followed() {
        let (_guard, dir) = tempdir();
        let (_outside_guard, outside_dir) = tempdir();
        let outside = outside_dir.join("outside.json");
        std::fs::write(&outside, b"outside").expect("seed outside");
        let link = dir.join("out.json");
        std::os::unix::fs::symlink(&outside, &link).expect("symlink");

        let err = validate_export_destination(&link).unwrap_err();
        assert_eq!(err, DestinationRejected::AlreadyExists);
        assert_eq!(std::fs::read(&outside).expect("read"), b"outside");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_parent_is_resolved_before_the_write() {
        let (_real_guard, real) = tempdir();
        let (_link_guard, link_parent) = tempdir();
        let link_dir = link_parent.join("link");
        std::os::unix::fs::symlink(&real, &link_dir).expect("symlink dir");

        let validated =
            validate_export_destination(&link_dir.join("out.json")).expect("valid through link");
        assert_eq!(validated.path().parent(), Some(real.as_path()));
        write_hello(&validated).expect("write");
        assert!(real.join("out.json").exists());
    }

    /// `create_temp` relies on `create_new` refusing a name already taken by a
    /// symlink. Naming the temporary path this test occupies would depend on
    /// [`TEMP_SEQ`], which other tests share whenever they run in one process.
    #[cfg(unix)]
    #[test]
    fn create_new_refuses_a_symlink_instead_of_writing_through_it() {
        let (_guard, dir) = tempdir();
        let (_outside_guard, outside_dir) = tempdir();
        let outside = outside_dir.join("target.json");
        std::fs::write(&outside, b"outside").expect("seed");
        let planted = dir.join(".export-planted.tmp");
        std::os::unix::fs::symlink(&outside, &planted).expect("plant symlink");

        let err = File::options()
            .write(true)
            .create_new(true)
            .open(&planted)
            .expect_err("create_new refuses the planted symlink");

        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&outside).expect("read"), b"outside");
    }

    #[cfg(windows)]
    #[test]
    fn a_destination_differing_only_by_case_is_refused() {
        let (_guard, dir) = tempdir();
        std::fs::write(dir.join("Export.json"), b"original").expect("seed");

        let err = validate_export_destination(&dir.join("export.JSON")).unwrap_err();
        assert_eq!(err, DestinationRejected::AlreadyExists);
    }

    #[cfg(windows)]
    #[test]
    fn a_unc_path_to_a_missing_share_is_refused() {
        let err = validate_export_destination(Path::new(r"\\127.0.0.1\no-such-share\out.json"))
            .unwrap_err();
        assert_eq!(err, DestinationRejected::ParentMissing);
    }

    #[cfg(not(windows))]
    #[test]
    fn a_unc_path_is_refused_as_relative_off_windows() {
        let err = validate_export_destination(Path::new(r"\\server\share\out.json")).unwrap_err();
        assert_eq!(err, DestinationRejected::NotAbsolute);
    }
}
