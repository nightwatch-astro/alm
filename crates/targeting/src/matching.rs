// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Geometry matching policy for immutable light sessions and panel mosaics.

use target_match::{
    compare_footprints, coverage_rotation_intervals, CoverageBand, FootprintComparison,
    RotationInterval, RotationSearch, SkyFootprint,
};

/// Percentage-based geometry thresholds used by one relation class.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeometryThresholds {
    pub coverage_min_percent: f64,
    pub center_separation_max_percent: f64,
    pub rotation_max_deg: f64,
}

/// Inclusive overlap band for accepted mosaic adjacency.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MosaicThresholds {
    pub overlap_min_percent: f64,
    pub overlap_max_percent: f64,
    pub residual_sky_rotation_cap_deg: f64,
}

/// Versioned settings used to construct future relation suggestions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MatchingSettings {
    pub revision: u64,
    pub same_session: GeometryThresholds,
    pub sibling: GeometryThresholds,
    pub mosaic: MosaicThresholds,
}

impl Default for MatchingSettings {
    fn default() -> Self {
        Self {
            revision: 1,
            same_session: GeometryThresholds {
                coverage_min_percent: 95.0,
                center_separation_max_percent: 2.0,
                rotation_max_deg: 1.0,
            },
            sibling: GeometryThresholds {
                coverage_min_percent: 90.0,
                center_separation_max_percent: 5.0,
                rotation_max_deg: 5.0,
            },
            mosaic: MosaicThresholds {
                overlap_min_percent: 5.0,
                overlap_max_percent: 40.0,
                residual_sky_rotation_cap_deg: 10.0,
            },
        }
    }
}

impl MatchingSettings {
    /// Compare solved footprints using this exact settings revision.
    ///
    /// Same-session is tested first and is restricted to the active
    /// materialization. Sibling is therefore never returned for a pair already
    /// classified into the same session. Mosaic overlap is evaluated only when
    /// neither same-panel class applies.
    ///
    /// # Errors
    ///
    /// Returns the upstream typed geometry error when the footprints cannot
    /// share a valid comparison plane or rotation-band calculation fails.
    pub fn evaluate(
        self,
        left: &SkyFootprint,
        right: &SkyFootprint,
        context: RelationContext,
    ) -> target_match::Result<GeometryEvidence> {
        evaluate_relation(left, right, context, self)
    }
}

/// Relation produced by the mutually-exclusive geometry policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomaticRelation {
    SameSession,
    Sibling,
    Mosaic,
}

/// Non-geometric facts required before a relation can be automatic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelationContext {
    pub materialization: MaterializationRelation,
    pub target: Compatibility,
    pub acquisition_geometry: Compatibility,
    pub equipment: Compatibility,
}

/// Whether the pair may form one session inside the active materialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterializationRelation {
    SameWithMatchingDiscriminators,
    DifferentOrDiscriminatorMismatch,
}

/// Compatibility of one independently reviewed relation dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compatibility {
    Compatible,
    Incompatible,
}

/// Measured evidence and the one policy outcome it supports.
#[derive(Debug, Clone, PartialEq)]
pub struct GeometryEvidence {
    pub comparison: FootprintComparison,
    pub allowed_mosaic_rotations: Vec<RotationInterval>,
    pub threshold_snapshot: Vec<ThresholdMeasurement>,
    pub relation: Option<AutomaticRelation>,
}

/// One inclusive measurement retained with a relation proposal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThresholdMeasurement {
    pub key: &'static str,
    pub measured_value: f64,
    pub threshold_value: f64,
    pub comparison: ThresholdComparison,
    pub passed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThresholdComparison {
    GreaterThanOrEqual,
    LessThanOrEqual,
}

/// Search resolution used for the fixed +/-10 degree mosaic cap.
pub const MOSAIC_ROTATION_SAMPLE_DEG: f64 = 0.1;
pub const MOSAIC_ROTATION_TOLERANCE_DEG: f64 = 0.001;

fn evaluate_relation(
    left: &SkyFootprint,
    right: &SkyFootprint,
    context: RelationContext,
    settings: MatchingSettings,
) -> target_match::Result<GeometryEvidence> {
    let comparison = compare_footprints(left, right)?;
    let mosaic_band = CoverageBand::new(
        settings.mosaic.overlap_min_percent / 100.0,
        settings.mosaic.overlap_max_percent / 100.0,
    )?;
    let cap = settings.mosaic.residual_sky_rotation_cap_deg;
    let allowed_mosaic_rotations = coverage_rotation_intervals(
        left,
        right,
        mosaic_band,
        RotationSearch::new(
            target_match::skymath::Angle::from_degrees(-cap),
            target_match::skymath::Angle::from_degrees(cap),
            target_match::skymath::Angle::from_degrees(MOSAIC_ROTATION_SAMPLE_DEG),
            target_match::skymath::Angle::from_degrees(MOSAIC_ROTATION_TOLERANCE_DEG),
        )?,
    )?;

    let (relation, threshold_snapshot) = if context.materialization
        == MaterializationRelation::SameWithMatchingDiscriminators
        && geometry_passes(&comparison, settings.same_session)
    {
        (
            Some(AutomaticRelation::SameSession),
            geometry_threshold_snapshot(&comparison, settings.same_session),
        )
    } else if context.target == Compatibility::Compatible
        && context.acquisition_geometry == Compatibility::Compatible
        && context.equipment == Compatibility::Compatible
        && geometry_passes(&comparison, settings.sibling)
    {
        (
            Some(AutomaticRelation::Sibling),
            geometry_threshold_snapshot(&comparison, settings.sibling),
        )
    } else if context.target == Compatibility::Compatible
        && context.acquisition_geometry == Compatibility::Compatible
        && comparison.parity_match
        && mosaic_band_contains(&comparison, settings.mosaic)
        && residual_in_intervals(
            comparison.residual_sky_rotation.degrees(),
            &allowed_mosaic_rotations,
        )
    {
        (Some(AutomaticRelation::Mosaic), mosaic_threshold_snapshot(&comparison, settings.mosaic))
    } else {
        (None, Vec::new())
    };

    Ok(GeometryEvidence { comparison, allowed_mosaic_rotations, threshold_snapshot, relation })
}

fn geometry_threshold_snapshot(
    comparison: &FootprintComparison,
    thresholds: GeometryThresholds,
) -> Vec<ThresholdMeasurement> {
    vec![
        minimum_measurement(
            "coverage_percent",
            comparison.normalized_coverage * 100.0,
            thresholds.coverage_min_percent,
        ),
        maximum_measurement(
            "center_separation_percent",
            comparison.normalized_centre_separation * 100.0,
            thresholds.center_separation_max_percent,
        ),
        maximum_measurement(
            "residual_sky_rotation_deg",
            comparison.residual_sky_rotation.degrees().abs(),
            thresholds.rotation_max_deg,
        ),
    ]
}

fn mosaic_threshold_snapshot(
    comparison: &FootprintComparison,
    thresholds: MosaicThresholds,
) -> Vec<ThresholdMeasurement> {
    let coverage = comparison.normalized_coverage * 100.0;
    vec![
        minimum_measurement("coverage_percent", coverage, thresholds.overlap_min_percent),
        maximum_measurement("coverage_percent", coverage, thresholds.overlap_max_percent),
        maximum_measurement(
            "residual_sky_rotation_deg",
            comparison.residual_sky_rotation.degrees().abs(),
            thresholds.residual_sky_rotation_cap_deg,
        ),
    ]
}

fn minimum_measurement(
    key: &'static str,
    measured_value: f64,
    threshold_value: f64,
) -> ThresholdMeasurement {
    ThresholdMeasurement {
        key,
        measured_value,
        threshold_value,
        comparison: ThresholdComparison::GreaterThanOrEqual,
        passed: measured_value >= threshold_value,
    }
}

fn maximum_measurement(
    key: &'static str,
    measured_value: f64,
    threshold_value: f64,
) -> ThresholdMeasurement {
    ThresholdMeasurement {
        key,
        measured_value,
        threshold_value,
        comparison: ThresholdComparison::LessThanOrEqual,
        passed: measured_value <= threshold_value,
    }
}

fn geometry_passes(comparison: &FootprintComparison, thresholds: GeometryThresholds) -> bool {
    comparison.parity_match
        && comparison.normalized_coverage * 100.0 >= thresholds.coverage_min_percent
        && comparison.normalized_centre_separation * 100.0
            <= thresholds.center_separation_max_percent
        && comparison.residual_sky_rotation.degrees().abs() <= thresholds.rotation_max_deg
}

fn mosaic_band_contains(comparison: &FootprintComparison, thresholds: MosaicThresholds) -> bool {
    let coverage = comparison.normalized_coverage * 100.0;
    coverage >= thresholds.overlap_min_percent
        && coverage <= thresholds.overlap_max_percent
        && comparison.residual_sky_rotation.degrees().abs()
            <= thresholds.residual_sky_rotation_cap_deg
}

fn residual_in_intervals(residual: f64, intervals: &[RotationInterval]) -> bool {
    intervals
        .iter()
        .any(|interval| residual >= interval.start.degrees() && residual <= interval.end.degrees())
}

/// Immutable membership snapshot used for complete-linkage admission.
#[derive(Debug, Clone, Copy)]
pub struct CompleteLinkage<'a, T> {
    accepted_members: &'a [T],
}

impl<'a, T> CompleteLinkage<'a, T> {
    #[must_use]
    pub fn new(accepted_members: &'a [T]) -> Self {
        Self { accepted_members }
    }

    /// Require the candidate to match every member of the immutable snapshot.
    #[must_use]
    pub fn accepts(&self, candidate: &T, matches: impl Fn(&T, &T) -> bool) -> bool {
        !self.accepted_members.is_empty()
            && self.accepted_members.iter().all(|member| matches(candidate, member))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // target-match embeds skymath 0.6; use its re-export so the types match.
    use target_match::{skymath as tm_skymath, FootprintProvenance, ImageParity};

    fn coordinate(ra: f64, dec: f64) -> tm_skymath::Equatorial {
        tm_skymath::Equatorial::j2000(
            tm_skymath::Angle::from_degrees(ra),
            tm_skymath::Angle::from_degrees(dec),
        )
        .expect("valid coordinate")
    }

    fn footprint(position_angle: f64, parity: ImageParity) -> SkyFootprint {
        SkyFootprint::new(
            coordinate(10.0, 0.0),
            vec![
                coordinate(9.0, -1.0),
                coordinate(11.0, -1.0),
                coordinate(11.0, 1.0),
                coordinate(9.0, 1.0),
            ],
            tm_skymath::Angle::from_degrees(position_angle),
            parity,
            FootprintProvenance::new(format!("{position_angle}-{parity:?}"))
                .expect("valid provenance"),
        )
        .expect("valid footprint")
    }

    #[test]
    fn complete_linkage_does_not_allow_transitive_expansion() {
        let members = [0_i32, 4];
        let linkage = CompleteLinkage::new(&members);
        assert!(!linkage.accepts(&7, |left, right| (left - right).abs() <= 4));
        assert!(linkage.accepts(&3, |left, right| (left - right).abs() <= 4));
        assert!(!CompleteLinkage::new(&[]).accepts(&3, |_, _| true));
    }

    #[test]
    fn inclusive_threshold_helpers_accept_exact_boundaries() {
        let comparison = FootprintComparison {
            anchor: tm_skymath::Equatorial::at_epoch(
                tm_skymath::Angle::from_degrees(0.0),
                tm_skymath::Angle::from_degrees(0.0),
                tm_skymath::Epoch::J2000,
            )
            .expect("valid coordinate"),
            left_area: 1.0,
            right_area: 1.0,
            intersection_area: 0.9,
            normalized_coverage: 0.9,
            centre_separation: tm_skymath::Angle::from_degrees(0.05),
            smaller_diagonal: tm_skymath::Angle::from_degrees(1.0),
            normalized_centre_separation: 0.05,
            residual_sky_rotation: tm_skymath::Angle::from_degrees(-5.0),
            parity_match: true,
        };
        assert!(geometry_passes(&comparison, MatchingSettings::default().sibling));
    }

    #[test]
    fn mosaic_overlap_and_rotation_policy_is_inclusive_only_at_boundaries() {
        let thresholds = MatchingSettings::default().mosaic;
        let mut comparison = FootprintComparison {
            anchor: coordinate(0.0, 0.0),
            left_area: 1.0,
            right_area: 1.0,
            intersection_area: 0.05,
            normalized_coverage: 0.05,
            centre_separation: tm_skymath::Angle::from_degrees(1.0),
            smaller_diagonal: tm_skymath::Angle::from_degrees(2.0),
            normalized_centre_separation: 0.5,
            residual_sky_rotation: tm_skymath::Angle::from_degrees(10.0),
            parity_match: true,
        };
        assert!(mosaic_band_contains(&comparison, thresholds));

        comparison.normalized_coverage = 0.4;
        comparison.intersection_area = 0.4;
        assert!(mosaic_band_contains(&comparison, thresholds));

        comparison.normalized_coverage = 0.05 - 1e-12;
        assert!(!mosaic_band_contains(&comparison, thresholds));
        comparison.normalized_coverage = 0.4 + 1e-12;
        assert!(!mosaic_band_contains(&comparison, thresholds));
        comparison.normalized_coverage = 0.2;
        comparison.residual_sky_rotation = tm_skymath::Angle::from_degrees(10.0 + 1e-12);
        assert!(!mosaic_band_contains(&comparison, thresholds));
    }

    #[test]
    fn upstream_rotation_is_modulo_180_and_parity_stays_separate() {
        let direct = footprint(0.0, ImageParity::Direct);
        let meridian_equivalent = footprint(179.0, ImageParity::Direct);
        let mirrored = footprint(179.0, ImageParity::Mirrored);

        let equivalent = compare_footprints(&direct, &meridian_equivalent).expect("comparison");
        assert!((equivalent.residual_sky_rotation.degrees() + 1.0).abs() < 1e-9);
        assert!(equivalent.parity_match);

        let parity_change = compare_footprints(&direct, &mirrored).expect("comparison");
        assert!((parity_change.residual_sky_rotation.degrees() + 1.0).abs() < 1e-9);
        assert!(!parity_change.parity_match);
    }

    #[test]
    fn modulo_180_and_parity_hold_across_multiple_turns() {
        for angle in (-720..=720).step_by(15) {
            let base = footprint(f64::from(angle), ImageParity::Direct);
            let equivalent = footprint(f64::from(angle + 180), ImageParity::Direct);
            let mirrored = footprint(f64::from(angle + 180), ImageParity::Mirrored);
            let comparison = compare_footprints(&base, &equivalent).expect("comparison");
            assert!(comparison.residual_sky_rotation.degrees().abs() < 1e-9);
            assert!(comparison.parity_match);
            assert!(!compare_footprints(&base, &mirrored).expect("comparison").parity_match);
        }
    }

    #[test]
    fn same_session_wins_exclusively_over_sibling() {
        let left = footprint(0.0, ImageParity::Direct);
        let right = footprint(180.0, ImageParity::Direct);
        let common = RelationContext {
            materialization: MaterializationRelation::SameWithMatchingDiscriminators,
            target: Compatibility::Compatible,
            acquisition_geometry: Compatibility::Compatible,
            equipment: Compatibility::Compatible,
        };
        let evidence =
            MatchingSettings::default().evaluate(&left, &right, common).expect("valid geometry");
        assert_eq!(evidence.relation, Some(AutomaticRelation::SameSession));
        assert_eq!(evidence.threshold_snapshot.len(), 3);
        assert!(evidence.threshold_snapshot.iter().all(|measurement| measurement.passed));

        let sibling = MatchingSettings::default()
            .evaluate(
                &left,
                &right,
                RelationContext {
                    materialization: MaterializationRelation::DifferentOrDiscriminatorMismatch,
                    ..common
                },
            )
            .expect("valid geometry");
        assert_eq!(sibling.relation, Some(AutomaticRelation::Sibling));
    }

    #[test]
    fn complete_linkage_is_order_invariant_and_rejects_long_chains() {
        let candidates = [[0_i32, 4, 8], [8, 4, 0], [4, 0, 8]];
        for members in candidates {
            let linkage = CompleteLinkage::new(&members);
            assert!(!linkage.accepts(&12, |left, right| { (left - right).abs() <= 4 }));
            assert!(linkage.accepts(&4, |left, right| { (left - right).abs() <= 4 }));
        }
    }
}
