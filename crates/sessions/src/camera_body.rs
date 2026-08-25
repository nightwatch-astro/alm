// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Physical camera-body identity derived from the FITS/XISF `CAMERAID` keyword.
//!
//! The single canonical derivation shared by the two write paths that persist a
//! calibration fingerprint — `app_core_targets::ingest_sessions` for light
//! sessions and `app_core_inbox::plan_listener` for registered masters. A
//! divergent implementation in either place would let a light and a master from
//! the same body key under different identities, which excludes them from each
//! other as a camera-body conflict.

use crate::optic_train::normalize_text;

/// Body identity for one frame, or `None` when the headers prove nothing.
///
/// `CAMERAID` is written as `vendor_model_serial`, and serial population is
/// vendor-dependent: Player One writes a real serial, ZWO writes either the bare
/// `INSTRUME` model or a trailing empty serial segment, and Dwarf omits the
/// keyword. Only a value carrying a non-empty segment after the last `_`, and
/// differing from `INSTRUME`, distinguishes two bodies of the same model; every
/// other shape yields `None` so the matcher skips the dimension rather than
/// keying two bodies under one model string.
#[must_use]
pub fn camera_body_id(cameraid: Option<&str>, instrume: Option<&str>) -> Option<String> {
    let id = normalize_text(cameraid)?;
    let serial = id.rsplit_once('_')?.1;
    if serial.is_empty() {
        return None;
    }
    if normalize_text(instrume).is_some_and(|model| model == id) {
        return None;
    }
    Some(id)
}

#[cfg(test)]
mod tests {
    use super::camera_body_id;

    /// The three vendor shapes sampled from the owner's library (600 FITS files
    /// under `/mnt/d/astrophotography`), recorded on `astro-plan-ugux2`.
    #[test]
    fn only_a_cameraid_carrying_a_serial_yields_a_body_id() {
        assert_eq!(
            camera_body_id(Some("Player One_CAMD2282B4C061209000"), Some("Poseidon-C PRO")),
            Some("player one_camd2282b4c061209000".to_owned()),
            "a populated serial is the one shape that identifies a body"
        );
        assert_eq!(
            camera_body_id(Some("ZWO ASI2600MM Pro"), Some("ZWO ASI2600MM Pro")),
            None,
            "N.I.N.A. + ZWO repeats INSTRUME, which cannot separate two bodies"
        );
        assert_eq!(
            camera_body_id(Some("ZWOptical_ZWO ASI2600MM Pro_"), Some("ZWO ASI2600MM Pro")),
            None,
            "ASIDeepStack exposes the field with an empty serial segment"
        );
        assert_eq!(camera_body_id(None, None), None, "Dwarf 3 omits the keyword");
    }

    #[test]
    fn a_blank_or_separatorless_cameraid_yields_no_body_id() {
        assert_eq!(camera_body_id(Some("   "), Some("Poseidon-C PRO")), None);
        assert_eq!(camera_body_id(Some("PoseidonC"), Some("Poseidon-C PRO")), None);
    }

    /// Two bodies of one model differ only past the last `_`, which is the whole
    /// point of the dimension.
    #[test]
    fn two_bodies_of_the_same_model_get_different_ids() {
        let a = camera_body_id(Some("Player One_SERIAL-A"), Some("Poseidon-C PRO"));
        let b = camera_body_id(Some("Player One_SERIAL-B"), Some("Poseidon-C PRO"));
        assert!(a.is_some() && b.is_some());
        assert_ne!(a, b);
    }

    /// Case and internal whitespace come from the writing tool, not the
    /// hardware, so they must not split one body into two identities.
    #[test]
    fn normalization_matches_the_optic_train_key_rules() {
        assert_eq!(
            camera_body_id(Some("  PLAYER   One_CAMD228  "), None),
            camera_body_id(Some("player one_camd228"), None)
        );
    }
}
