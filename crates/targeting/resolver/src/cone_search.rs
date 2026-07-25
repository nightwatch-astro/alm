// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Cone-search candidate ranking (spec 052 P3, D9 + OQ-1/OQ-2 — see
//! `specs/052-simbad-caching-dual-lookup-cone-search/research.md`).
//!
//! Pure, resolver-fed domain logic over `simbad_resolver::ResolvedIdentity`
//! (the upstream crate's type, which carries `otype_raw`/`common_name`/
//! `aliases` — astro-plan's own [`crate::ResolvedIdentity`] intentionally
//! drops `otype_raw`, so this module works directly against the upstream
//! type and converts to astro-plan's local type only at the confirm/persist
//! boundary, same as [`crate::simbad`]).
//!
//! No I/O: `crates/app/targets`/`crates/app/inbox` own the TAP cone-search
//! call, the rotation-aware field-footprint check (`target-match`), and the
//! `canonical_target` write.

use simbad_resolver::{AliasKind, ObjectType, ResolvedIdentity};

/// One cone-search hit: a resolved identity plus its angular separation from
/// the query centre (degrees).
#[derive(Clone, Debug)]
pub struct ConeCandidate {
    pub identity: ResolvedIdentity,
    pub separation_deg: f64,
}

/// Catalogue-prominence tier (OQ-1). Declared low → high so `derive(Ord)`'s
/// declaration-order comparison matches "more prominent wins": `Messier`/
/// common-name outranks `Ngc`, which outranks `Ic`, then the Sharpless/
/// Barnard/LBN/LDN mid-tier, then `Caldwell`, then everything else.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ProminenceTier {
    Niche,
    Caldwell,
    ShBarnardLbnLdn,
    Ic,
    Ngc,
    MessierOrCommonName,
}

/// Classify one normalized designation alias by catalogue prefix (OQ-1: a
/// deterministic prefix classifier, not a curated object list).
///
/// Operates on [`simbad_resolver::ResolvedAlias::normalized`], which
/// `simbad_resolver::normalize` already lowercases/collapses and expands
/// (`m31` → `m 31`, `sh2-155` → `sh2 155`, `barnard33` → `barnard 33`, …), so
/// a plain prefix check is sufficient and can't confuse e.g. `mel 15`
/// (Melotte) with the `m ` (Messier) prefix.
fn designation_tier(normalized: &str) -> ProminenceTier {
    if normalized.starts_with("m ") {
        ProminenceTier::MessierOrCommonName
    } else if normalized.starts_with("ngc ") {
        ProminenceTier::Ngc
    } else if normalized.starts_with("ic ") {
        ProminenceTier::Ic
    } else if normalized.starts_with("sh2 ")
        || normalized.starts_with("sharpless ")
        || normalized.starts_with("barnard ")
        || normalized.starts_with("b ")
        || normalized.starts_with("lbn ")
        || normalized.starts_with("ldn ")
    {
        ProminenceTier::ShBarnardLbnLdn
    } else if normalized.starts_with("c ") || normalized.starts_with("caldwell ") {
        ProminenceTier::Caldwell
    } else {
        ProminenceTier::Niche
    }
}

/// The prominence tier of a resolved identity: the best tier across all its
/// `Designation` aliases, promoted to the top tier when it carries a curated
/// SIMBAD common name (OQ-1 — "Veil Nebula" (NGC 6960) lands top-tier because
/// SIMBAD carries its `NAME`; there is no static common-name list, the
/// resolver's own `common_name` presence is the signal).
#[must_use]
pub fn prominence_tier(identity: &ResolvedIdentity) -> ProminenceTier {
    if identity.common_name.is_some() {
        return ProminenceTier::MessierOrCommonName;
    }
    identity
        .aliases
        .iter()
        .filter(|a| a.kind == AliasKind::Designation)
        .map(|a| designation_tier(&a.normalized))
        .max()
        .unwrap_or(ProminenceTier::Niche)
}

/// SIMBAD raw `otype` codes for the star family + non-DSO point-source types
/// excluded by default from primary-object selection (OQ-2).
///
/// A deterministic rule table, not an exhaustive/canonical SIMBAD vocabulary
/// dump (research.md: "curate the exact otype set... at impl time") — the
/// common single/variable/peculiar-star and non-optical-detection codes.
/// `**`/`Asterism` are handled separately via `object_type` (already a closed
/// enum astro-plan maps every raw otype through).
const EXCLUDED_OTYPES: &[&str] = &[
    // Single/variable/peculiar stars and stellar remnants (point sources,
    // not extended DSOs).
    "*", "V*", "V*?", "Pe*", "PM*", "HB*", "WR*", "Em*", "Be*", "BS*", "RG*", "AB*", "C*", "S*",
    "sg*", "s*r", "s*y", "s*b", "HS*", "WD*", "N*", "BH", "Psr", "Ae*", "TT*", "Ir*", "Or*", "RR*",
    "Ce*", "dS*", "RV*", "El*", "LP*", "SN*", "SN", "Sy*", "XB*", "LX*", "HX*",
    // Non-DSO detection points (radio/X-ray/lensing sources, unresolved
    // errors) — no resolvable optical extent of their own.
    "Radio", "Rad", "mR", "cm", "mm", "IR", "UV", "X", "gLens", "err", "?",
];

/// Whether `identity` is in the default OQ-2 primary-object exclusion set.
///
/// Named/notable objects are retained regardless of type (OQ-2: `common_name`
/// presence is the resolver's own "notable" signal — a genuine imaging
/// target like a named variable star stays selectable while an incidental
/// field star drops out). Excluded candidates are still returned/shown by
/// the caller (`excluded: true`, FR-015) — never omitted outright, only kept
/// out of automatic primary selection.
#[must_use]
pub fn is_default_excluded(identity: &ResolvedIdentity) -> bool {
    if identity.common_name.is_some() {
        return false;
    }
    matches!(identity.object_type, ObjectType::DoubleStar | ObjectType::Asterism)
        || EXCLUDED_OTYPES.contains(&identity.otype_raw.trim())
}

/// Collapse duplicate cone-search hits of one physical object (OQ-1) before
/// ranking: primary key `simbad_oid` (same oid ⇒ one object); identities
/// lacking one (seed/offline) dedup on the normalized `primary_designation`
/// (FR-007); a secondary guard treats two candidates sharing any normalized
/// alias as the same object. Keeps the highest-prominence representative of
/// each group (ties broken by the smaller separation, deterministically).
///
/// # Panics
///
/// Never panics: every group is built from at least one input candidate.
#[must_use]
pub fn dedup_candidates(candidates: Vec<ConeCandidate>) -> Vec<ConeCandidate> {
    let mut groups: Vec<Vec<ConeCandidate>> = Vec::new();
    'outer: for c in candidates {
        for g in &mut groups {
            if same_object(&g[0].identity, &c.identity) {
                g.push(c);
                continue 'outer;
            }
        }
        groups.push(vec![c]);
    }
    groups
        .into_iter()
        .map(|g| {
            g.into_iter()
                .min_by(|a, b| {
                    prominence_tier(&b.identity).cmp(&prominence_tier(&a.identity)).then_with(
                        || {
                            a.separation_deg
                                .partial_cmp(&b.separation_deg)
                                .unwrap_or(std::cmp::Ordering::Equal)
                        },
                    )
                })
                .expect("groups are never empty")
        })
        .collect()
}

/// Select up to `limit` in-field candidate indices for the suggestion
/// response (spec 052 P3; issue #698 — "distance-ordered top-N fetch can
/// drop the actual target").
///
/// A dense in-galaxy field routinely returns far more default-excluded
/// point sources (stars, radio/X-ray detections) *and* bare `Niche`-tier
/// substructure catalogue entries (HII knots, molecular-cloud complexes,
/// embedded clusters with no recognised catalogue prefix or common name)
/// nearer to the query centre than the actual, independently-catalogued
/// target — live SIMBAD data around M 51 shows 1000+ such entries within
/// 0.02 deg. A plain nearest-first cut silently drops the real target before
/// the user ever sees it, even though [`prominence_tier`]/[`is_default_excluded`]
/// correctly rank it once considered — the defeat happens at truncation, not
/// ranking. Order candidates by (not-excluded first, then prominence tier
/// descending, then separation ascending) before truncating, so excluded/
/// niche clutter can never crowd out a higher-tier candidate regardless of
/// its raw separation; then guarantee `primary` — `primary_index`'s own
/// nearest-to-centre, prominence-tie-broken pick (OQ-1, unchanged by this
/// function) — survives the cut even in the residual case where `limit`
/// non-excluded, equal-or-higher-tier candidates all sit nearer to centre
/// than it.
///
/// This only changes *which* candidates make the cut, never their relative
/// order for display — callers re-sort the returned subset by separation for
/// that (nearest-first display, FR-013, is unaffected).
#[must_use]
pub fn select_for_display(
    candidates: &[ConeCandidate],
    in_field: &[usize],
    primary: Option<usize>,
    limit: usize,
) -> Vec<usize> {
    let mut ranked: Vec<usize> = in_field.to_vec();
    let rank_key = |i: usize| {
        let identity = &candidates[i].identity;
        (is_default_excluded(identity), std::cmp::Reverse(prominence_tier(identity)))
    };
    ranked.sort_by(|&a, &b| {
        rank_key(a).cmp(&rank_key(b)).then_with(|| {
            candidates[a]
                .separation_deg
                .partial_cmp(&candidates[b].separation_deg)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    if ranked.len() > limit {
        if let Some(pos) = primary.and_then(|p| ranked.iter().position(|&i| i == p)) {
            if pos >= limit {
                ranked.swap(limit - 1, pos);
            }
        }
        ranked.truncate(limit);
    }
    ranked
}

/// Whether `a` and `b` are the same physical object (OQ-1 dedup key).
fn same_object(a: &ResolvedIdentity, b: &ResolvedIdentity) -> bool {
    if let (Some(x), Some(y)) = (a.simbad_oid, b.simbad_oid) {
        return x == y;
    }
    if simbad_resolver::normalize::normalize(&a.primary_designation)
        == simbad_resolver::normalize::normalize(&b.primary_designation)
    {
        return true;
    }
    a.aliases.iter().any(|x| b.aliases.iter().any(|y| x.normalized == y.normalized))
}

#[cfg(test)]
mod tests {
    use super::*;
    use simbad_resolver::{ResolvedAlias, TargetSource};

    fn identity(
        oid: Option<i64>,
        designation: &str,
        common_name: Option<&str>,
        object_type: ObjectType,
        otype_raw: &str,
        aliases: &[&str],
    ) -> ResolvedIdentity {
        let mut all_aliases: Vec<ResolvedAlias> =
            aliases.iter().map(|a| ResolvedAlias::new(*a, AliasKind::Designation)).collect();
        if let Some(name) = common_name {
            all_aliases.push(ResolvedAlias::new(name, AliasKind::CommonName));
        }
        ResolvedIdentity {
            simbad_oid: oid,
            primary_designation: designation.to_owned(),
            common_name: common_name.map(str::to_owned),
            object_type,
            otype_raw: otype_raw.to_owned(),
            ra_deg: 0.0,
            dec_deg: 0.0,
            v_mag: None,
            aliases: all_aliases,
            source: TargetSource::Resolved,
        }
    }

    fn candidate(identity: ResolvedIdentity, separation_deg: f64) -> ConeCandidate {
        ConeCandidate { identity, separation_deg }
    }

    // ── prominence_tier (OQ-1) ────────────────────────────────────────────────

    #[test]
    fn messier_designation_is_top_tier() {
        let m31 = identity(None, "M 31", None, ObjectType::Galaxy, "G", &["M 31", "NGC 224"]);
        assert_eq!(prominence_tier(&m31), ProminenceTier::MessierOrCommonName);
    }

    #[test]
    fn common_name_promotes_to_top_tier_even_for_a_bare_ngc_number() {
        // Veil Nebula = NGC 6960, no Messier alias — SIMBAD's NAME carries the
        // common name, which alone must promote it to the top tier.
        let veil = identity(
            None,
            "NGC 6960",
            Some("Veil Nebula"),
            ObjectType::SupernovaRemnant,
            "SNR",
            &["NGC 6960"],
        );
        assert_eq!(prominence_tier(&veil), ProminenceTier::MessierOrCommonName);
    }

    #[test]
    fn ngc_outranks_ic_outranks_sharpless_family_outranks_caldwell_outranks_niche() {
        let ngc = identity(None, "NGC 1", None, ObjectType::Galaxy, "G", &["NGC 1"]);
        let ic = identity(None, "IC 1", None, ObjectType::Galaxy, "G", &["IC 1"]);
        let sh2 = identity(None, "Sh2 155", None, ObjectType::EmissionNebula, "HII", &["Sh2 155"]);
        let barnard = identity(None, "B 33", None, ObjectType::DarkNebula, "DNe", &["B 33"]);
        let caldwell = identity(None, "C 14", None, ObjectType::OpenCluster, "OpC", &["C 14"]);
        let niche = identity(None, "vdB 1", None, ObjectType::Other, "RNe", &["vdB 1"]);

        assert!(prominence_tier(&ngc) > prominence_tier(&ic));
        assert!(prominence_tier(&ic) > prominence_tier(&sh2));
        assert_eq!(prominence_tier(&sh2), prominence_tier(&barnard));
        assert!(prominence_tier(&sh2) > prominence_tier(&caldwell));
        assert!(prominence_tier(&caldwell) > prominence_tier(&niche));
    }

    #[test]
    fn best_tier_wins_across_multiple_aliases() {
        // A niche vdB alias plus a Messier alias on the same object: the
        // Messier alias must win (best tier across ALL Designation aliases).
        let obj =
            identity(None, "vdB 1", None, ObjectType::ReflectionNebula, "RNe", &["vdB 1", "M 78"]);
        assert_eq!(prominence_tier(&obj), ProminenceTier::MessierOrCommonName);
    }

    // ── is_default_excluded (OQ-2) ────────────────────────────────────────────

    #[test]
    fn incidental_field_star_is_excluded() {
        let star = identity(None, "HD 12345", None, ObjectType::Other, "*", &["HD 12345"]);
        assert!(is_default_excluded(&star));
    }

    #[test]
    fn named_variable_star_is_retained() {
        let mira = identity(None, "omi Cet", Some("Mira"), ObjectType::Other, "V*", &["omi Cet"]);
        assert!(!is_default_excluded(&mira), "a genuine named imaging target must stay selectable");
    }

    #[test]
    fn double_star_and_asterism_object_types_are_excluded() {
        let dbl = identity(None, "STF 1", None, ObjectType::DoubleStar, "**", &["STF 1"]);
        assert!(is_default_excluded(&dbl));
        let ast = identity(None, "Coathanger", None, ObjectType::Asterism, "As*", &["Coathanger"]);
        assert!(is_default_excluded(&ast));
    }

    #[test]
    fn galaxy_nebula_cluster_are_never_excluded() {
        let m31 = identity(None, "M 31", None, ObjectType::Galaxy, "G", &["M 31"]);
        assert!(!is_default_excluded(&m31));
    }

    // ── dedup_candidates (OQ-1) ───────────────────────────────────────────────

    #[test]
    fn same_oid_dedups_to_one_keeping_higher_prominence() {
        let a = candidate(identity(Some(1), "M 31", None, ObjectType::Galaxy, "G", &["M 31"]), 0.1);
        let b = candidate(
            identity(Some(1), "NGC 224", None, ObjectType::Galaxy, "G", &["NGC 224"]),
            0.2,
        );
        let out = dedup_candidates(vec![a, b]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].identity.primary_designation, "M 31");
    }

    #[test]
    fn missing_oid_dedups_on_normalized_designation() {
        let a = candidate(identity(None, "M31", None, ObjectType::Galaxy, "G", &["M31"]), 0.1);
        let b = candidate(identity(None, "M 31", None, ObjectType::Galaxy, "G", &["M 31"]), 0.15);
        let out = dedup_candidates(vec![a, b]);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn shared_alias_across_distinct_candidates_dedups_via_secondary_guard() {
        let a = candidate(
            identity(None, "NGC 224", None, ObjectType::Galaxy, "G", &["NGC 224", "M 31"]),
            0.1,
        );
        let b = candidate(identity(None, "M 31", None, ObjectType::Galaxy, "G", &["M 31"]), 0.2);
        let out = dedup_candidates(vec![a, b]);
        assert_eq!(out.len(), 1, "sharing the 'm 31' normalized alias must collapse to one object");
    }

    #[test]
    fn distinct_objects_are_not_merged() {
        let m31 =
            candidate(identity(Some(1), "M 31", None, ObjectType::Galaxy, "G", &["M 31"]), 0.1);
        let m110 =
            candidate(identity(Some(2), "M 110", None, ObjectType::Galaxy, "G", &["M 110"]), 0.6);
        let out = dedup_candidates(vec![m31, m110]);
        assert_eq!(out.len(), 2);
    }

    // ── select_for_display (#698) ─────────────────────────────────────────────

    // Every niche entry sits nearer to centre than the galaxy (0.000_833).
    const NICHE_SEPARATIONS_DEG: [f64; 8] =
        [0.0001, 0.00011, 0.00012, 0.00013, 0.00014, 0.00015, 0.00016, 0.00017];

    #[test]
    fn prominent_galaxy_survives_a_dense_niche_field_that_outnumbers_the_response_limit() {
        // Reproduces #698's M 51 shape: 8 in-galaxy niche catalogue entries
        // (HII knots etc. — no recognised catalogue prefix or common name,
        // so they land in `ProminenceTier::Niche`) all nearer to the query
        // centre than the actual, Messier-catalogued galaxy. A plain
        // nearest-first truncate(8) would show only the 8 niche entries and
        // drop the galaxy entirely, exactly as reported live.
        let galaxy = identity(
            Some(1),
            "M 51",
            None,
            ObjectType::Galaxy,
            "Sy2", // SIMBAD classifies M 51's nucleus as a Seyfert 2, not "G"
            &["M 51", "NGC 5194"],
        );
        let mut candidates = vec![candidate(galaxy, 0.000_833)];
        for (n, sep) in NICHE_SEPARATIONS_DEG.into_iter().enumerate() {
            let designation = format!("[HL2008] {n}");
            let niche = identity(
                Some(100 + i64::try_from(n).unwrap()),
                &designation,
                None,
                ObjectType::EmissionNebula,
                "HII",
                &[&designation],
            );
            candidates.push(candidate(niche, sep));
        }

        let in_field: Vec<usize> = (0..candidates.len()).collect();
        // `primary` mirrors what `app_core_inbox::cone_search::primary_index`
        // would compute in production (separation-first among non-excluded
        // candidates, prominence only tie-breaking — OQ-1, unchanged by this
        // fix): the nearest niche knot, not the galaxy. This proves the
        // display-selection fix does not depend on the primary pick being
        // "correct" — the galaxy must show up in the suggestion list
        // regardless of what gets marked primary/preselected.
        let primary = Some(1);

        let selected = select_for_display(&candidates, &in_field, primary, 8);
        assert_eq!(selected.len(), 8);
        assert!(
            selected.contains(&0),
            "the Messier-catalogued galaxy must survive the response-limit cut"
        );
    }

    #[test]
    fn homogeneous_tier_field_keeps_nearest_first_order_unchanged() {
        // When every in-field candidate shares the same prominence tier
        // (the common case — no notable object nearby), selection must not
        // change today's nearest-first behaviour.
        let candidates = vec![
            candidate(identity(None, "HD 1", None, ObjectType::Other, "*", &["HD 1"]), 0.3),
            candidate(identity(None, "HD 2", None, ObjectType::Other, "*", &["HD 2"]), 0.1),
            candidate(identity(None, "HD 3", None, ObjectType::Other, "*", &["HD 3"]), 0.2),
        ];
        let in_field = vec![0, 1, 2];
        let selected = select_for_display(&candidates, &in_field, None, 2);
        assert_eq!(
            selected,
            vec![1, 2],
            "nearest two by separation, same as a plain sort+truncate"
        );
    }

    #[test]
    fn primary_is_restored_even_when_pushed_past_the_limit_by_equal_tier_neighbours() {
        // Residual case documented on `select_for_display`: all three
        // candidates are non-excluded and share the same (Niche) tier, so
        // tier-bucketing alone cannot protect `primary` from the two nearer
        // neighbours — only the explicit swap-in guarantee does.
        let candidates = vec![
            candidate(
                identity(None, "Niche A", None, ObjectType::EmissionNebula, "HII", &[]),
                0.01,
            ),
            candidate(
                identity(None, "Niche B", None, ObjectType::EmissionNebula, "HII", &[]),
                0.02,
            ),
            candidate(
                identity(None, "Niche C (primary)", None, ObjectType::EmissionNebula, "HII", &[]),
                0.5,
            ),
        ];
        let in_field = vec![0, 1, 2];
        let selected = select_for_display(&candidates, &in_field, Some(2), 2);
        assert_eq!(selected.len(), 2);
        assert!(
            selected.contains(&2),
            "the resolved primary must survive truncation even among same-tier neighbours"
        );
    }
}
