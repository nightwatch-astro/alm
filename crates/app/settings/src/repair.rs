use std::collections::BTreeSet;

use super::{descriptors, ContractError, ErrorCode, ErrorSeverity, Value};

// ── Partial repair of array-valued settings ───────────────────────────────

/// The split of an array-valued setting whose stored value failed validation.
pub struct Salvage {
    /// The entries that validate, in stored order.
    pub kept: Value,
    /// The entries that were discarded.
    pub dropped: Value,
}

/// Split an invalid array-valued setting into the entries worth keeping and the
/// entries to discard.
///
/// Returns `None` unless every one of these holds, in which case the caller
/// falls back to resetting the whole key:
///
/// - the key's rule is one whose entries are independently valid,
/// - the stored value is an array,
/// - at least one entry fails on its own, so the whole-value failure is
///   accounted for by the entries this pass discards,
/// - the kept remainder passes the key's own validation.
///
/// The last two conditions matter: without them a whole-value rule that no
/// per-entry check can see would let an invalid value through as "repaired".
///
/// Observing sites are the one salvageable key today. They are coordinates a
/// user looked up and typed in, so by the durability test in constitution
/// Principle V they are Tier 1 user knowledge, and discarding the sites that
/// were fine because a sibling is malformed loses knowledge no filesystem scan
/// can recover.
pub fn salvage(key: &str, value: &Value) -> Option<Salvage> {
    let descriptor = descriptors::descriptor_for(key)?;
    let rule = descriptor.validation;
    if !matches!(rule, descriptors::ValidationRule::ObserverSites) {
        return None;
    }

    let entries = value.as_array()?;
    let mut kept: Vec<Value> = Vec::new();
    let mut dropped: Vec<Value> = Vec::new();
    let mut ids: BTreeSet<String> = BTreeSet::new();

    for entry in entries {
        // A duplicate id is only visible against the entries already kept, so
        // the first occurrence survives and later ones are dropped.
        let id = entry.get("id").and_then(Value::as_str).map(str::to_owned);
        let duplicate = id.as_ref().is_some_and(|i| ids.contains(i));
        if duplicate || check(rule, &Value::Array(vec![entry.clone()])).is_err() {
            dropped.push(entry.clone());
            continue;
        }
        if let Some(id) = id {
            ids.insert(id);
        }
        kept.push(entry.clone());
    }

    if dropped.is_empty() {
        return None;
    }
    let kept = Value::Array(kept);
    check(rule, &kept).ok()?;
    Some(Salvage { kept, dropped: Value::Array(dropped) })
}

/// Run a validation rule against a value, discarding the rendered message.
fn check(rule: descriptors::ValidationRule, value: &Value) -> Result<(), ContractError> {
    let invalid = |msg: &str| {
        ContractError::new(ErrorCode::ValueInvalid, msg.to_owned(), ErrorSeverity::Warning, false)
    };
    descriptors::check_rule(rule, value, &invalid)
}
