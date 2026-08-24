// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Acquisition and calibration session modeling boundaries.

pub mod camera_body;
pub mod clustering;
pub mod identity;
pub mod key;
pub mod observing_night;
pub mod optic_train;

pub use camera_body::camera_body_id;
pub use clustering::{
    angular_separation_deg, circular_mean_deg, derive_clustering, fov_diagonal_deg,
    rotation_axial_distance_deg, Assignment, ClusteringResult, ExistingFraming, NewFramingGroup,
    SessionGeometry, ToleranceParams, UnassignedReason,
};
pub use identity::{
    dark_exposure_tolerance_us, BiasSessionDiscriminators, CameraGeometryIdentity,
    CaptureDiscriminators, DarkSessionDiscriminators, DarkTemperatureMode, FlatOrientationIdentity,
    FlatSessionDiscriminators, IdentityValue, ImageParity, LightGeometryIdentity,
    LightSessionDiscriminators, RasterDimensions, SessionDiscriminators, SessionIdentity,
    SessionKind,
};
pub use key::{observing_night, KeyError, ObserverContext, SessionKey, SessionKeyParts};
pub use observing_night::{ObservingNight, ObservingNightDerivation, ObservingNightError};
pub use optic_train::optic_train_key;

pub const CRATE_NAME: &str = "sessions";

#[cfg(test)]
mod tests {
    use super::CRATE_NAME;

    #[test]
    fn exposes_crate_name() {
        // Source of truth is Cargo.toml's package name, not a second hand-typed
        // literal in this file — catches CRATE_NAME drifting from the manifest.
        assert_eq!(CRATE_NAME, env!("CARGO_PKG_NAME"));
    }
}
