// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Verifies that `PlanState`'s serde variants exactly match the SQL CHECK
//! constraint in the `plans` table migration (kyo7.85 DS-11).
//!
//! The SQL string is the authoritative on-disk representation; this test
//! ensures the enum and the schema stay in sync without a macro or codegen step.

use std::{collections::BTreeSet, fs, path::PathBuf};

use domain_core::lifecycle::plan::PlanState;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("contract test package should live under tests/contract")
        .to_path_buf()
}

/// Extract the CHECK list from `plans.state` in the initial schema migration.
///
/// Looks for the single-line pattern:
///   `state TEXT NOT NULL CHECK (state IN ('...','...',...)),`
fn sql_plan_state_check() -> BTreeSet<String> {
    let path = repo_root().join("crates/persistence/core/migrations/0001_initial_schema.sql");
    let sql = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read migration {}: {e}", path.display()));

    // Find the line that defines the `state` column inside the `plans` table.
    let state_line = sql
        .lines()
        .find(|l| {
            let t = l.trim();
            t.starts_with("state")
                && t.contains("CHECK")
                && t.contains("IN (")
                && t.contains("'draft'")
        })
        .unwrap_or_else(|| panic!("could not find plans.state CHECK line in migration"));

    // Extract the parenthesized list: `('a','b',...)`.
    let start = state_line.find("IN (").map(|i| i + "IN (".len()).expect("IN ( not found");
    let end = state_line[start..].find(')').map(|i| i + start).expect(") not found");
    let inner = &state_line[start..end];

    inner.split(',').map(|s| s.trim().trim_matches('\'').to_owned()).collect()
}

/// All `PlanState` variants serialised to their serde `snake_case` string.
fn plan_state_serde_values() -> BTreeSet<String> {
    // Exhaustive via PlanState::ALL — adding a variant without updating ALL
    // will fail the compile-time check in domain_core, not silently omit it.
    PlanState::ALL
        .iter()
        .map(|s| {
            serde_json::to_value(s)
                .expect("PlanState should serialize")
                .as_str()
                .expect("PlanState should serialize to a string")
                .to_owned()
        })
        .collect()
}

#[test]
fn plan_state_enum_matches_sql_check_constraint() {
    let sql_states = sql_plan_state_check();
    let enum_states = plan_state_serde_values();

    assert_eq!(
        enum_states,
        sql_states,
        "PlanState serde variants drifted from the `plans.state` SQL CHECK constraint.\n\
         In enum but not SQL: {:?}\n\
         In SQL but not enum: {:?}",
        enum_states.difference(&sql_states).collect::<Vec<_>>(),
        sql_states.difference(&enum_states).collect::<Vec<_>>(),
    );
}
