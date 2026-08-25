// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Archive operation primitive (spec 025, research R2).
//!
//! Delegates to `move_op::move_file` with the configured archive root as the
//! destination. The archive path is pre-computed at plan generation time and
//! stored in `plan_items.archive_path`.

use camino::Utf8Path;
#[cfg(test)]
use camino::Utf8PathBuf;

use crate::failure::PlanItemFailure;
use crate::ops::move_op::{move_file, MoveResult};

/// Archive a file by moving it to `archive_destination`.
///
/// `archive_destination` is the path already resolved and proven contained by
/// `run::loop_::resolve_item_paths` (e.g.
/// `<library_root>/.astro-plan-archive/<planId>/...`). Its stored form comes
/// from the item's `archive_path` and may be root-relative, so passing the
/// stored value straight in bypasses the containment rule (astro-plan-zboex).
///
/// # Errors
///
/// Propagates move failures with the same error contract as `move_op`.
pub fn archive_file(
    source: &Utf8Path,
    archive_destination: &Utf8Path,
) -> Result<(), (PlanItemFailure, MoveResult)> {
    move_file(source, archive_destination)
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_moves_to_destination() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        let src = root.join("raw.fits");
        let dst = root.join(".astro-plan-archive/p1/raw.fits");
        std::fs::write(&src, b"fits data").unwrap();

        archive_file(&src, &dst).unwrap();

        assert!(!src.exists());
        assert!(dst.exists());
    }
}
