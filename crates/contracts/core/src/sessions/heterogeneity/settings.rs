// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Matching-settings contracts.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use specta::Type;

use super::shared::{
    BoundedList, CanonicalId, FiniteDecimal, MutationContext, Rfc3339Timestamp, SafeText,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SettingsSeverity {
    Yellow,
    Red,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationAgeKind {
    Dark,
    Bias,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GeometryThresholds {
    pub coverage_min_percent: FiniteDecimal,
    pub center_separation_max_percent: FiniteDecimal,
    pub rotation_max_deg: FiniteDecimal,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MosaicThresholds {
    pub overlap_min_percent: FiniteDecimal,
    pub overlap_max_percent: FiniteDecimal,
    pub residual_sky_rotation_cap_deg: FiniteDecimal,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DarkThermalThresholds {
    pub moderate_deg: FiniteDecimal,
    pub severe_deg: FiniteDecimal,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationAgePolicy {
    pub camera_id: CanonicalId,
    pub kind: CalibrationAgeKind,
    pub fresh_through_days: u32,
    pub red_after_days: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FlatOrientationThresholds {
    pub normal_through_deg: FiniteDecimal,
    pub red_above_deg: FiniteDecimal,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FlatAgeThresholds {
    pub red_after_nights: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FixedMatchingRules {
    #[schemars(range(min = 5, max = 5))]
    pub optical_profile_same_max_percent: u32,
    #[schemars(range(min = 10, max = 10))]
    pub optical_profile_review_max_percent: u32,
    #[schemars(range(min = 10, max = 10))]
    pub optical_profile_evidence_conflict_percent: u32,
    #[schemars(range(min = 1, max = 1))]
    pub flat_same_night_fresh_max_nights: u32,
    #[schemars(range(min = 2, max = 2))]
    pub flat_yellow_starts_nights: u32,
}

impl Default for FixedMatchingRules {
    fn default() -> Self {
        Self {
            optical_profile_same_max_percent: 5,
            optical_profile_review_max_percent: 10,
            optical_profile_evidence_conflict_percent: 10,
            flat_same_night_fresh_max_nights: 1,
            flat_yellow_starts_nights: 2,
        }
    }
}

impl<'de> Deserialize<'de> for FixedMatchingRules {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Wire {
            optical_profile_same_max_percent: u32,
            optical_profile_review_max_percent: u32,
            optical_profile_evidence_conflict_percent: u32,
            flat_same_night_fresh_max_nights: u32,
            flat_yellow_starts_nights: u32,
        }

        let wire = Wire::deserialize(deserializer)?;
        let rules = Self {
            optical_profile_same_max_percent: wire.optical_profile_same_max_percent,
            optical_profile_review_max_percent: wire.optical_profile_review_max_percent,
            optical_profile_evidence_conflict_percent: wire
                .optical_profile_evidence_conflict_percent,
            flat_same_night_fresh_max_nights: wire.flat_same_night_fresh_max_nights,
            flat_yellow_starts_nights: wire.flat_yellow_starts_nights,
        };
        if rules != Self::default() {
            return Err(serde::de::Error::custom("fixed matching rules cannot be changed"));
        }
        Ok(rules)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MatchingSettings {
    pub revision: u64,
    pub same_session: GeometryThresholds,
    pub sibling: GeometryThresholds,
    pub mosaic: MosaicThresholds,
    pub dark_thermal: DarkThermalThresholds,
    pub calibration_age: BoundedList<CalibrationAgePolicy, 500>,
    pub flat_orientation: FlatOrientationThresholds,
    pub flat_age: FlatAgeThresholds,
    pub fixed_rules: FixedMatchingRules,
    pub updated_at: Rfc3339Timestamp,
    pub updated_by: CanonicalId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SettingsValue {
    pub field_path: SafeText,
    #[serde(flatten)]
    pub value: SettingsScalar,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SettingsScalar {
    Decimal(FiniteDecimal),
    Unsigned(u32),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SettingsIssue {
    pub code: SafeText,
    pub severity: SettingsSeverity,
    pub field_paths: BoundedList<SafeText, 50>,
    pub values: BoundedList<SettingsValue, 50>,
    pub message_key: SafeText,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SettingsValidation {
    pub valid: bool,
    pub issues: BoundedList<SettingsIssue, 500>,
    pub effective: MatchingSettings,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GeometryThresholdPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage_min_percent: Option<FiniteDecimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub center_separation_max_percent: Option<FiniteDecimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation_max_deg: Option<FiniteDecimal>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MatchingSettingsPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub same_session: Option<GeometryThresholdPatch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sibling: Option<GeometryThresholdPatch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mosaic: Option<MosaicThresholds>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dark_thermal: Option<DarkThermalThresholds>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calibration_age: Option<BoundedList<CalibrationAgePolicy, 500>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flat_orientation: Option<FlatOrientationThresholds>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flat_age: Option<FlatAgeThresholds>,
}

impl MatchingSettings {
    #[must_use]
    pub fn apply_patch(&self, patch: &MatchingSettingsPatch) -> Self {
        let mut effective = self.clone();
        if let Some(value) = &patch.same_session {
            apply_geometry_patch(&mut effective.same_session, value);
        }
        if let Some(value) = &patch.sibling {
            apply_geometry_patch(&mut effective.sibling, value);
        }
        if let Some(value) = &patch.mosaic {
            effective.mosaic = value.clone();
        }
        if let Some(value) = &patch.dark_thermal {
            effective.dark_thermal = value.clone();
        }
        if let Some(value) = &patch.calibration_age {
            effective.calibration_age = value.clone();
        }
        if let Some(value) = &patch.flat_orientation {
            effective.flat_orientation = value.clone();
        }
        if let Some(value) = &patch.flat_age {
            effective.flat_age = value.clone();
        }
        effective
    }

    #[must_use]
    /// # Panics
    ///
    /// Panics only if the fixed set of validation rules produces more than 500 issues.
    pub fn validate(&self) -> SettingsValidation {
        /// (`field_path`, value index, min, max, optional warning boundary + direction)
        const RANGE_CHECKS: &[(&str, usize, f64, f64, Option<(f64, bool)>)] = &[
            ("sameSession.coverageMinPercent", 0, 90.0, 99.5, Some((93.0, false))),
            ("sameSession.centerSeparationMaxPercent", 1, 0.5, 5.0, Some((3.0, true))),
            ("sameSession.rotationMaxDeg", 2, 0.25, 3.0, Some((2.0, true))),
            ("sibling.coverageMinPercent", 3, 80.0, 95.0, Some((85.0, false))),
            ("sibling.centerSeparationMaxPercent", 4, 2.0, 15.0, Some((10.0, true))),
            ("sibling.rotationMaxDeg", 5, 1.0, 15.0, Some((10.0, true))),
            ("mosaic.overlapMinPercent", 6, 1.0, 20.0, Some((3.0, false))),
            ("mosaic.overlapMaxPercent", 7, 20.0, 60.0, Some((50.0, true))),
            ("darkThermal.moderateDeg", 8, 0.1, 2.0, Some((1.0, true))),
            ("darkThermal.severeDeg", 9, 0.5, 5.0, Some((3.0, true))),
            ("flatOrientation.normalThroughDeg", 10, 0.5, 5.0, Some((3.0, true))),
            ("flatOrientation.redAboveDeg", 11, 0.5, 15.0, Some((8.0, true))),
        ];

        let values: [FiniteDecimal; 12] = [
            self.same_session.coverage_min_percent,
            self.same_session.center_separation_max_percent,
            self.same_session.rotation_max_deg,
            self.sibling.coverage_min_percent,
            self.sibling.center_separation_max_percent,
            self.sibling.rotation_max_deg,
            self.mosaic.overlap_min_percent,
            self.mosaic.overlap_max_percent,
            self.dark_thermal.moderate_deg,
            self.dark_thermal.severe_deg,
            self.flat_orientation.normal_through_deg,
            self.flat_orientation.red_above_deg,
        ];

        let mut issues = Vec::new();
        for &(field, idx, min, max, warning) in RANGE_CHECKS {
            check_range(&mut issues, field, values[idx], min, max, warning);
        }
        if (self.mosaic.residual_sky_rotation_cap_deg.get() - 10.0).abs() > f64::EPSILON {
            issues.push(issue(
                "settings.fixed_rule_changed",
                SettingsSeverity::Red,
                "mosaic.residualSkyRotationCapDeg",
            ));
        }
        if !(7..=365).contains(&self.flat_age.red_after_nights) {
            issues.push(issue(
                "settings.out_of_bounds",
                SettingsSeverity::Red,
                "flatAge.redAfterNights",
            ));
        } else if self.flat_age.red_after_nights > 90 {
            issues.push(issue(
                "settings.warning.flat_age",
                SettingsSeverity::Yellow,
                "flatAge.redAfterNights",
            ));
        }
        for policy in &self.calibration_age {
            if policy.fresh_through_days > 1_795 || !(30..=1_825).contains(&policy.red_after_days) {
                issues.push(issue(
                    "settings.out_of_bounds",
                    SettingsSeverity::Red,
                    "calibrationAge",
                ));
            }
            if policy.red_after_days < policy.fresh_through_days.saturating_add(30) {
                issues.push(issue(
                    "settings.calibration_age_gap",
                    SettingsSeverity::Red,
                    "calibrationAge",
                ));
            } else if policy.red_after_days > 730 {
                issues.push(issue(
                    "settings.warning.calibration_age",
                    SettingsSeverity::Yellow,
                    "calibrationAge",
                ));
            }
        }
        if self.fixed_rules != FixedMatchingRules::default() {
            issues.push(issue("settings.fixed_rule_changed", SettingsSeverity::Red, "fixedRules"));
        }
        cross_constraints(self, &mut issues);
        let valid = !issues.iter().any(|item| item.severity == SettingsSeverity::Red);
        SettingsValidation {
            valid,
            issues: BoundedList::try_new(issues).expect("settings checks are bounded"),
            effective: self.clone(),
        }
    }
}

fn apply_geometry_patch(target: &mut GeometryThresholds, patch: &GeometryThresholdPatch) {
    if let Some(value) = patch.coverage_min_percent {
        target.coverage_min_percent = value;
    }
    if let Some(value) = patch.center_separation_max_percent {
        target.center_separation_max_percent = value;
    }
    if let Some(value) = patch.rotation_max_deg {
        target.rotation_max_deg = value;
    }
}

fn safe(value: &str) -> SafeText {
    SafeText::try_new(value).expect("static settings text is safe")
}

fn issue(code: &str, severity: SettingsSeverity, field_path: &str) -> SettingsIssue {
    SettingsIssue {
        code: safe(code),
        severity,
        field_paths: BoundedList::try_new(vec![safe(field_path)]).expect("one field path"),
        values: BoundedList::default(),
        message_key: safe(code),
    }
}

fn check_range(
    issues: &mut Vec<SettingsIssue>,
    field: &str,
    value: FiniteDecimal,
    min: f64,
    max: f64,
    warning: Option<(f64, bool)>,
) {
    if !(min..=max).contains(&value.get()) {
        issues.push(issue("settings.out_of_bounds", SettingsSeverity::Red, field));
    } else if let Some((boundary, warn_above)) = warning {
        if (warn_above && value.get() > boundary) || (!warn_above && value.get() < boundary) {
            issues.push(issue("settings.warning.risky_value", SettingsSeverity::Yellow, field));
        }
    }
}

fn cross_constraints(settings: &MatchingSettings, issues: &mut Vec<SettingsIssue>) {
    if settings.sibling.coverage_min_percent > settings.same_session.coverage_min_percent {
        issues.push(issue(
            "settings.sibling_coverage_stricter",
            SettingsSeverity::Red,
            "sibling.coverageMinPercent",
        ));
    }
    if settings.sibling.center_separation_max_percent
        < settings.same_session.center_separation_max_percent
    {
        issues.push(issue(
            "settings.sibling_center_stricter",
            SettingsSeverity::Red,
            "sibling.centerSeparationMaxPercent",
        ));
    }
    if settings.sibling.rotation_max_deg < settings.same_session.rotation_max_deg {
        issues.push(issue(
            "settings.sibling_rotation_stricter",
            SettingsSeverity::Red,
            "sibling.rotationMaxDeg",
        ));
    }
    if settings.mosaic.overlap_min_percent >= settings.mosaic.overlap_max_percent {
        issues.push(issue(
            "settings.mosaic_overlap_order",
            SettingsSeverity::Red,
            "mosaic.overlapMinPercent",
        ));
    }
    if settings.mosaic.overlap_max_percent.get()
        > settings.sibling.coverage_min_percent.get() - 10.0
    {
        issues.push(issue(
            "settings.mosaic_sibling_gap",
            SettingsSeverity::Red,
            "mosaic.overlapMaxPercent",
        ));
    }
    if settings.dark_thermal.severe_deg.get() < settings.dark_thermal.moderate_deg.get() + 0.5 {
        issues.push(issue(
            "settings.dark_thermal_gap",
            SettingsSeverity::Red,
            "darkThermal.severeDeg",
        ));
    }
    if settings.flat_orientation.red_above_deg <= settings.flat_orientation.normal_through_deg {
        issues.push(issue(
            "settings.flat_orientation_order",
            SettingsSeverity::Red,
            "flatOrientation.redAboveDeg",
        ));
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MatchingSettingsGetRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MatchingSettingsValidateRequest {
    pub base_revision: u64,
    pub patch: MatchingSettingsPatch,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MatchingSettingsUpdateRequest {
    pub expected_revision: u64,
    pub patch: MatchingSettingsPatch,
    pub acknowledged_warning_codes: BoundedList<SafeText, 500>,
    pub mutation_context: MutationContext,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MatchingSettingsUpdateResponse {
    pub settings: MatchingSettings,
    pub warnings: BoundedList<SettingsIssue, 500>,
    pub audit_id: CanonicalId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MatchingSettingsUpdatedEvent {
    pub previous_revision: u64,
    pub revision: u64,
    pub changed_field_paths: BoundedList<SafeText, 500>,
    pub warning_codes: BoundedList<SafeText, 500>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(tag = "event")]
pub enum MatchingSettingsEvent {
    #[serde(rename = "matching_settings.updated")]
    Updated(MatchingSettingsUpdatedEvent),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(tag = "operation")]
pub enum MatchingSettingsQuery {
    #[serde(rename = "matching_settings.get")]
    Get(MatchingSettingsGetRequest),
    #[serde(rename = "matching_settings.validate")]
    Validate(MatchingSettingsValidateRequest),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(tag = "operation")]
pub enum MatchingSettingsCommand {
    #[serde(rename = "matching_settings.update")]
    Update(MatchingSettingsUpdateRequest),
}
