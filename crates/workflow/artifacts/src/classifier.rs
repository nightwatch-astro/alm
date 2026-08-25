// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Rule-driven artifact classifier (spec 012 T013).
//!
//! Applies an ordered list of `ArtifactRule` entries to a file name and
//! returns the winning `(kind, confidence, source)` triple.
//!
//! Unknown files fall back to `intermediate` with confidence < 0.2 so they
//! surface as "needs review" rather than being silently dropped (spec 012 US2-3).
//!
//! Constitution III: classification is based on filename / extension only.
//! The classifier NEVER opens or reads file contents.

use crate::rules::{ArtifactKind, ArtifactRule};

/// Classification source, matching the DB constraint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClassificationSource {
    Rule,
    Fallback,
}

impl ClassificationSource {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rule => "rule",
            Self::Fallback => "fallback",
        }
    }
}

/// Result of classifying a single file name.
#[derive(Clone, Debug)]
pub struct ClassificationResult {
    pub kind: ArtifactKind,
    pub confidence: f64,
    pub source: ClassificationSource,
    /// Rule id that matched, `None` on fallback.
    pub matched_rule_id: Option<&'static str>,
}

/// Classify `file_name` (the name component of a path, not the full path)
/// against the supplied rule list.
///
/// The rule with the **highest priority** that matches wins. On a tie the
/// rule earlier in the slice wins. If no rule matches the
/// fallback is returned: `kind=intermediate`, `confidence=0.1`.
///
/// Rows with `classification_source = manual_override` are skipped by the
/// caller (T015) before invoking this function.
#[must_use]
pub fn classify(file_name: &str, rules: &[ArtifactRule]) -> ClassificationResult {
    // `max_by_key` returns the LAST maximum, so iterate in reverse to make the
    // earliest declared rule win a priority tie.
    let winner = rules.iter().filter(|r| r.matches(file_name)).rev().max_by_key(|r| r.priority);

    match winner {
        Some(rule) => ClassificationResult {
            kind: rule.kind,
            confidence: rule.confidence,
            source: ClassificationSource::Rule,
            matched_rule_id: Some(rule.id),
        },
        None => ClassificationResult {
            kind: ArtifactKind::Intermediate,
            confidence: 0.1,
            source: ClassificationSource::Fallback,
            matched_rule_id: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{ArtifactRule, MatchKind};

    fn make_rules() -> Vec<ArtifactRule> {
        crate::default_rules::all()
    }

    #[test]
    fn pixinsight_master_dark_classified_as_master() {
        let rules = make_rules();
        let result = classify("MasterDark_Bin1x1_-10C.xisf", &rules);
        assert_eq!(result.kind, ArtifactKind::Master);
        assert!(result.confidence >= 0.85, "confidence={}", result.confidence);
        assert!(matches!(result.source, ClassificationSource::Rule));
    }

    #[test]
    fn pixinsight_integration_classified_as_intermediate() {
        let rules = make_rules();
        let result = classify("integration_M31_Ha.xisf", &rules);
        assert_eq!(result.kind, ArtifactKind::Intermediate);
        assert!(matches!(result.source, ClassificationSource::Rule));
    }

    #[test]
    fn pixinsight_combined_suffix_classified_as_final() {
        let rules = make_rules();
        let result = classify("M31_combined.xisf", &rules);
        assert_eq!(result.kind, ArtifactKind::Final);
    }

    #[test]
    fn unknown_file_falls_back_to_intermediate_low_confidence() {
        let rules = make_rules();
        let result = classify("some_random_output.xisf", &rules);
        assert_eq!(result.kind, ArtifactKind::Intermediate);
        assert!(result.confidence < 0.2, "confidence={}", result.confidence);
        assert!(matches!(result.source, ClassificationSource::Fallback));
        assert!(result.matched_rule_id.is_none());
    }

    #[test]
    fn equal_priority_tie_goes_to_the_earlier_rule() {
        // `default_rules` has genuine priority ties and its own sort is stable,
        // so declaration order is the only thing separating tied rules.
        let rules = vec![
            ArtifactRule {
                id: "earlier",
                tool: "test",
                match_kind: MatchKind::Prefix,
                pattern: "master",
                kind: ArtifactKind::Master,
                confidence: 0.9,
                priority: 100,
            },
            ArtifactRule {
                id: "later",
                tool: "test",
                match_kind: MatchKind::Prefix,
                pattern: "master",
                kind: ArtifactKind::Intermediate,
                confidence: 0.5,
                priority: 100,
            },
        ];
        let result = classify("master_dark.xisf", &rules);
        assert_eq!(result.matched_rule_id, Some("earlier"));
        assert_eq!(result.kind, ArtifactKind::Master);
    }

    #[test]
    fn higher_priority_rule_wins_over_lower() {
        let rules = vec![
            ArtifactRule {
                id: "low",
                tool: "test",
                match_kind: MatchKind::Prefix,
                pattern: "master",
                kind: ArtifactKind::Intermediate, // wrong kind on purpose
                confidence: 0.5,
                priority: 1,
            },
            ArtifactRule {
                id: "high",
                tool: "test",
                match_kind: MatchKind::Prefix,
                pattern: "master",
                kind: ArtifactKind::Master,
                confidence: 0.95,
                priority: 100,
            },
        ];
        let result = classify("master_dark.xisf", &rules);
        assert_eq!(result.kind, ArtifactKind::Master);
        assert_eq!(result.matched_rule_id, Some("high"));
    }

    #[test]
    fn siril_master_bias_classified_as_master() {
        let rules = make_rules();
        let result = classify("master_bias.fit", &rules);
        assert_eq!(result.kind, ArtifactKind::Master);
    }

    #[test]
    fn siril_result_classified_as_final() {
        let rules = make_rules();
        let result = classify("result.fit", &rules);
        assert_eq!(result.kind, ArtifactKind::Final);
    }
}
