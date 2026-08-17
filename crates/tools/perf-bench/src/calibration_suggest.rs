// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Scenario: `calibration_suggest_1k_masters` — spec 007 T033.
//!
//! Seeds `MASTERS_N` (default 1,000) calibration master fingerprints plus one
//! light session, then times a single
//! `app_core::calibration::suggest` call. Masters are split across
//! dark/flat/bias so all three rule families run, and gains are varied so a
//! realistic share is excluded by the gain hard rule rather than every
//! candidate being scored.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use app_core::calibration::suggest;
use contracts_core::calibration_match::CalibrationMatchSuggestRequest;
use persistence_calibration::test_support::{
    seed_calibration_master, seed_light_session, SeedFingerprint,
};
use persistence_core::Database;

use crate::support::{env_size, print_result};

const SESSION_ID: &str = "perf-light-session";
const OPTIC_TRAIN: &str = "ASI2600MM+Esprit100";

/// Run the calibration-suggest scenario against its own tempdir database.
pub async fn run(counter: &Arc<AtomicU64>) {
    let n = env_size("MASTERS_N", 1_000);

    let db_dir = tempfile::tempdir().expect("calibration db tempdir");
    let db_path = db_dir.path().join("calibration.db");
    let db = Database::connect(&format!("sqlite://{}?mode=rwc", db_path.display()))
        .await
        .expect("db connect");
    db.migrate().await.expect("migrations");

    let session_fp = SeedFingerprint {
        gain: Some(100.0),
        offset_val: Some(50.0),
        exposure_s: Some(300.0),
        temp_c: Some(-10.0),
        filter_name: Some("Ha"),
        binning: Some("1x1"),
        optic_train: Some(OPTIC_TRAIN),
        observing_night_date: Some("2026-07-01"),
    };
    seed_light_session(db.pool(), SESSION_ID, &session_fp).await.expect("seed_light_session");

    let kinds = ["dark", "flat", "bias"];
    for i in 0..n {
        let kind = kinds[i % kinds.len()];
        // Every third master shares the session's gain (and so survives the
        // gain hard rule); the rest are excluded, mirroring a real library
        // where most masters belong to other equipment configurations.
        let gain = if i % 3 == 0 {
            100.0
        } else {
            100.0 + f64::from(u32::try_from(i % 7 + 1).unwrap_or(1)) * 10.0
        };
        let fp = SeedFingerprint {
            gain: Some(gain),
            offset_val: Some(50.0),
            exposure_s: Some(300.0 + f64::from(u32::try_from(i % 5).unwrap_or(0))),
            temp_c: Some(-10.0 + f64::from(u32::try_from(i % 3).unwrap_or(0))),
            filter_name: Some("Ha"),
            binning: Some("1x1"),
            optic_train: Some(OPTIC_TRAIN),
            observing_night_date: Some("2026-07-01"),
        };
        seed_calibration_master(db.pool(), &format!("perf-master-{i:05}"), kind, &fp)
            .await
            .expect("seed_calibration_master");
    }

    let req = CalibrationMatchSuggestRequest {
        contract_version: contracts_core::calibration_match::SUGGEST_CONTRACT_VERSION.to_owned(),
        request_id: "perf-suggest".to_owned(),
        session_id: SESSION_ID.to_owned(),
        calibration_types: None,
    };

    counter.store(0, Ordering::Relaxed);
    let t0 = Instant::now();
    let resp = suggest(db.pool(), req).await.expect("suggest");
    let wall_ms = t0.elapsed().as_millis();
    let stmts = counter.load(Ordering::Relaxed);

    print_result(
        "calibration_suggest_1k_masters",
        n,
        wall_ms,
        &serde_json::json!({
            "matches": resp.matches.as_ref().map_or(0, Vec::len),
            "suggest_status": resp.suggest_status.map(|s| format!("{s:?}")),
            "sqlx_stmts": stmts,
        }),
    );
}
