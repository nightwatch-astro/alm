// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Token registry (data-model.md §TokenRegistry v1, spec 015 T3.2).
//!
//! Defines the v1 token vocabulary: name, source_field, fallback, and
//! transform. The static [`V1_REGISTRY`] constant is consumed by the resolver.

use std::collections::HashMap;

// ── TokenTransform ─────────────────────────────────────────────────────────

/// Post-sanitization transform applied to a resolved token value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenTransform {
    /// No transform beyond sanitization.
    SanitizeOnly,
    /// Format as ISO local date `YYYY-MM-DD`. Source field provides the date
    /// string; the resolver accepts `YYYY-MM-DD` and passes it through, or
    /// uses the fallback when absent.
    DateIso,
    /// Convert value to lowercase.
    Lower,
    /// Convert value to uppercase.
    Upper,
}

// ── TokenDefinition ────────────────────────────────────────────────────────

/// Describes one token in the registry (data-model.md §TokenDefinition).
#[derive(Clone, Debug)]
pub struct TokenDefinition {
    /// Token name as it appears in a `PatternPart` value, e.g. `"target"`.
    pub name: &'static str,
    /// The key in [`crate::resolver::MetadataBundle`] that this token reads from.
    pub source_field: &'static str,
    /// Value emitted when the source field is absent or sanitizes to empty.
    pub fallback: &'static str,
    /// Post-sanitization transform.
    pub transform: TokenTransform,
}

// ── TokenRegistry ──────────────────────────────────────────────────────────

/// A map from token name to its definition.
pub struct TokenRegistry {
    entries: HashMap<&'static str, TokenDefinition>,
}

impl TokenRegistry {
    /// Construct a registry from a slice of definitions.
    #[must_use]
    pub fn from_slice(defs: &'static [TokenDefinition]) -> Self {
        let entries = defs.iter().map(|d| (d.name, d.clone())).collect();
        Self { entries }
    }

    /// Return the definition for a token name, if present.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&TokenDefinition> {
        self.entries.get(name)
    }

    /// Return `true` if the token name is registered.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }
}

// ── V1 token definitions (data-model.md §TokenRegistry v1) ────────────────

static V1_DEFINITIONS: &[TokenDefinition] = &[
    TokenDefinition {
        name: "target",
        source_field: "target",
        fallback: "unclassified",
        transform: TokenTransform::SanitizeOnly,
    },
    TokenDefinition {
        name: "filter",
        source_field: "filter",
        fallback: "nofilter",
        transform: TokenTransform::SanitizeOnly,
    },
    // `date_obs_local` is the key in the MetadataBundle. When the caller
    // cannot compute a local date (no observer_location), it substitutes the
    // UTC date string and adds "date" to missing_tokens. (Ref: R-Date-1)
    TokenDefinition {
        name: "date",
        source_field: "date",
        fallback: "undated",
        transform: TokenTransform::DateIso,
    },
    TokenDefinition {
        name: "frame_type",
        source_field: "frame_type",
        fallback: "unknown",
        transform: TokenTransform::Lower,
    },
    TokenDefinition {
        name: "camera",
        source_field: "camera",
        fallback: "unknown-camera",
        transform: TokenTransform::SanitizeOnly,
    },
    TokenDefinition {
        name: "exposure",
        source_field: "exposure",
        fallback: "unknown-exposure",
        transform: TokenTransform::SanitizeOnly,
    },
    TokenDefinition {
        name: "gain",
        source_field: "gain",
        fallback: "unknown-gain",
        transform: TokenTransform::SanitizeOnly,
    },
    TokenDefinition {
        name: "binning",
        source_field: "binning",
        fallback: "1x1",
        transform: TokenTransform::SanitizeOnly,
    },
    TokenDefinition {
        name: "set_temp",
        source_field: "set_temp",
        fallback: "untempered",
        transform: TokenTransform::SanitizeOnly,
    },
];

/// The static v1 token registry. All resolver and validator calls use this.
pub static V1_REGISTRY: std::sync::LazyLock<TokenRegistry> =
    std::sync::LazyLock::new(|| TokenRegistry::from_slice(V1_DEFINITIONS));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_registry_contains_all_tokens() {
        let names = [
            "target",
            "filter",
            "date",
            "frame_type",
            "camera",
            "exposure",
            "gain",
            "binning",
            "set_temp",
        ];
        for name in names {
            assert!(V1_REGISTRY.contains(name), "missing token: {name}");
        }
    }

    #[test]
    fn v1_registry_fallbacks_match_data_model() {
        assert_eq!(V1_REGISTRY.get("target").unwrap().fallback, "unclassified");
        assert_eq!(V1_REGISTRY.get("filter").unwrap().fallback, "nofilter");
        assert_eq!(V1_REGISTRY.get("date").unwrap().fallback, "undated");
        assert_eq!(V1_REGISTRY.get("frame_type").unwrap().fallback, "unknown");
        assert_eq!(V1_REGISTRY.get("camera").unwrap().fallback, "unknown-camera");
        assert_eq!(V1_REGISTRY.get("exposure").unwrap().fallback, "unknown-exposure");
        assert_eq!(V1_REGISTRY.get("gain").unwrap().fallback, "unknown-gain");
        assert_eq!(V1_REGISTRY.get("binning").unwrap().fallback, "1x1");
        assert_eq!(V1_REGISTRY.get("set_temp").unwrap().fallback, "untempered");
    }

    #[test]
    fn unknown_token_returns_none() {
        assert!(V1_REGISTRY.get("telescope").is_none());
        assert!(!V1_REGISTRY.contains("freeform"));
    }
}
