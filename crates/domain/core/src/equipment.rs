// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Equipment contract DTOs for the Tauri IPC surface.

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum FilterCategory {
    Narrowband,
    Broadband,
    DualBand,
    Other,
    Custom,
}

/// Camera sensor type (spec 044 iteration 2026-07-15, FR-035): `mono`
/// per-filter imaging vs `osc` (one-shot color) single-pass imaging.
/// Absence (`None` on [`Camera::sensor_type`]) means unknown, which MUST
/// behave as mono downstream (FR-038).
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum SensorType {
    Mono,
    Osc,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Camera {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub auto_detected: bool,
    /// FR-035: `None` = unknown (behaves as mono, FR-038).
    pub sensor_type: Option<SensorType>,
    /// FR-035: narrowband set for an OSC dual/tri-band filter (e.g.
    /// `["Ha","OIII"]`); `None` = plain color camera (`rgb` default). Only
    /// meaningful when `sensor_type` is `Osc`.
    pub passband: Option<Vec<String>>,
    /// Pixel pitch in micrometres; `None` = not recorded. Square pixels are
    /// assumed on both axes, matching `sessions::fov_diagonal_deg`.
    #[serde(default)]
    pub pixel_size_um: Option<f64>,
    /// Unbinned sensor width in pixels (FITS `NAXIS1`); `None` = not recorded.
    #[serde(default)]
    pub sensor_width_px: Option<i64>,
    /// Unbinned sensor height in pixels (FITS `NAXIS2`); `None` = not recorded.
    #[serde(default)]
    pub sensor_height_px: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Telescope {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub focal_length_mm: Option<i32>,
    pub auto_detected: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OpticalTrain {
    pub id: String,
    pub name: String,
    pub telescope_id: Option<String>,
    pub camera_id: Option<String>,
    pub focal_length_mm: i32,
    /// Diagonal field of view in degrees, derived from this train's focal
    /// length plus the linked camera's sensor geometry.
    ///
    /// `None` whenever any operand is absent or non-positive — no camera
    /// linked, or a camera with no recorded geometry. Absent MUST stay absent:
    /// a fabricated `0.0` would read as a real (degenerate) field of view.
    /// Derived on read, never stored.
    #[serde(default)]
    pub fov_diagonal_deg: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
    pub id: String,
    pub name: String,
    pub category: FilterCategory,
    pub auto_detected: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateCamera {
    pub name: String,
    pub aliases: Vec<String>,
    /// FR-035; `#[serde(default)]` keeps pre-iteration payloads valid.
    #[serde(default)]
    pub sensor_type: Option<SensorType>,
    #[serde(default)]
    pub passband: Option<Vec<String>>,
    /// Sensor geometry; `#[serde(default)]` keeps pre-0079 payloads valid.
    #[serde(default)]
    pub pixel_size_um: Option<f64>,
    #[serde(default)]
    pub sensor_width_px: Option<i64>,
    #[serde(default)]
    pub sensor_height_px: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCamera {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    /// FR-035; `#[serde(default)]` keeps pre-iteration payloads valid.
    #[serde(default)]
    pub sensor_type: Option<SensorType>,
    #[serde(default)]
    pub passband: Option<Vec<String>>,
    /// Sensor geometry; `#[serde(default)]` keeps pre-0079 payloads valid.
    #[serde(default)]
    pub pixel_size_um: Option<f64>,
    #[serde(default)]
    pub sensor_width_px: Option<i64>,
    #[serde(default)]
    pub sensor_height_px: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateTelescope {
    pub name: String,
    pub aliases: Vec<String>,
    pub focal_length_mm: Option<i32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTelescope {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub focal_length_mm: Option<i32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateOpticalTrain {
    pub name: String,
    pub telescope_id: Option<String>,
    pub camera_id: Option<String>,
    pub focal_length_mm: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOpticalTrain {
    pub id: String,
    pub name: String,
    pub telescope_id: Option<String>,
    pub camera_id: Option<String>,
    pub focal_length_mm: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateFilter {
    pub name: String,
    pub category: FilterCategory,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateFilter {
    pub id: String,
    pub name: String,
    pub category: FilterCategory,
}
