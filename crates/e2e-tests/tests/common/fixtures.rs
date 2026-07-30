// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Public fixture helpers: FITS file writers, scan helpers, and the
//! shared first-run-redirect settler.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde_json::Value;

use super::app::E2eApp;

// ---------------------------------------------------------------------------
// FITS fixture writer
// ---------------------------------------------------------------------------

/// Write a minimal single-block (2880-byte) FITS file with the given header
/// cards, so journeys can drive the real inbox classify/confirm/ingest
/// pipeline against real files on disk (no product code touched).
///
/// Mirrors the proven fixture writer already used by
/// `crates/app/inbox/src/confirm.rs` tests and
/// `crates/app/core/tests/ingest_sessions_integration.rs` (T045/T046) — same
/// card set, same padding — so the real classifier/session-grouping code
/// accepts it exactly as it does at Layer 1.
///
/// Writes **no** `EXPTIME` card, so every frame type routes to the
/// `__needs_review__` sentinel (T070 mandatory-attribute gate: lights need
/// `OBJECT`+`FILTER`+`EXPTIME`, darks need `EXPTIME`+`GAIN`). That is what the
/// needs-review journeys want; a journey that needs a frame to actually
/// CLASSIFY must use [`write_minimal_fits_with_exposure`].
pub fn write_minimal_fits(
    dir: &Path,
    name: &str,
    imagetyp: &str,
    object: Option<&str>,
    filter: Option<&str>,
    date_obs: Option<&str>,
) -> Result<PathBuf> {
    write_minimal_fits_with_exposure(dir, name, imagetyp, object, filter, date_obs, None)
}

/// [`write_minimal_fits`] plus an optional `EXPTIME` card.
///
/// `EXPTIME` is a hard mandatory attribute for lights AND darks
/// (`mandatory_set_for`, `crates/app/inbox/src/classify.rs`), so it is the
/// difference between a fixture that classifies into a real grouping bucket
/// and one that collapses into the single `__needs_review__` sentinel bucket.
/// Header set matches the Layer-1 `t066_mixed_folder_produces_n_sub_items`
/// fixtures (`EXPTIME=300.0`, `GAIN=100`), which prove a light + a dark
/// materialize as two distinct single-type sub-items.
pub fn write_minimal_fits_with_exposure(
    dir: &Path,
    name: &str,
    imagetyp: &str,
    object: Option<&str>,
    filter: Option<&str>,
    date_obs: Option<&str>,
    exposure_s: Option<f64>,
) -> Result<PathBuf> {
    let path = dir.join(name);
    let mut block = vec![b' '; 2880];
    let mut idx = 0usize;
    let mut write_card = |card: &str| {
        let bytes = card.as_bytes();
        let len = bytes.len().min(80);
        block[idx * 80..idx * 80 + len].copy_from_slice(&bytes[..len]);
        idx += 1;
    };
    write_card(&format!("{:<80}", format!("IMAGETYP= '{imagetyp}'")));
    if let Some(o) = object {
        write_card(&format!("{:<80}", format!("OBJECT  = '{o}'")));
    }
    if let Some(f) = filter {
        write_card(&format!("{:<80}", format!("FILTER  = '{f}'")));
    }
    if let Some(d) = date_obs {
        write_card(&format!("{:<80}", format!("DATE-OBS= '{d}'")));
    }
    if let Some(e) = exposure_s {
        write_card(&format!("{:<80}", format!("EXPTIME = {e}")));
    }
    write_card(&format!("{:<80}", "GAIN    = 100"));
    write_card(&format!("{:<80}", "XBINNING= 1"));
    write_card(&format!("{:<80}", "YBINNING= 1"));
    block[idx * 80..idx * 80 + 3].copy_from_slice(b"END");
    std::fs::write(&path, &block).with_context(|| format!("write fixture FITS {path:?}"))?;
    Ok(path)
}

/// Scan a root through IPC and return the id of the inbox item it yields.
///
/// Spec 058 T012/FR-015 changed the shape every scan-seeded journey depended
/// on: `inbox.scan.folder` no longer writes a placeholder `inbox_items` row,
/// so an ordinary folder now comes back as `items: []` plus a source-group
/// row. Reading `scan["items"][0]` therefore fails with an empty-items error
/// that reads like "the scan found nothing" when the scan in fact worked.
///
/// Classification is what materializes the real single-type item rows, so
/// this seeds the way the product now does: scan, then classify the group the
/// scan recorded, then take the item.
///
/// Master-only folders still come back with items directly (a detected master
/// is its own item row with no source group), so the direct hit is preferred
/// when present rather than treated as an error.
pub async fn scan_and_classify_one_item(
    app: &E2eApp,
    root_id: &str,
    root_absolute_path: &str,
) -> Result<String> {
    let scan: Value = app
        .invoke(
            "inbox_scan_folder",
            serde_json::json!({
                "req": {
                    "rootId": root_id,
                    "rootAbsolutePath": root_absolute_path,
                }
            }),
        )
        .await?;

    if let Some(id) = scan["items"][0]["inboxItemId"].as_str() {
        return Ok(id.to_owned());
    }

    let list: Value =
        app.invoke("inbox_list", serde_json::json!({ "req": { "limit": 500 } })).await?;
    let group_id = list["sourceGroups"]
        .as_array()
        .and_then(|groups| {
            groups.iter().find(|g| g["rootId"].as_str() == Some(root_id)).or_else(|| groups.first())
        })
        .and_then(|g| g["sourceGroupId"].as_str())
        .ok_or_else(|| {
            anyhow::anyhow!("scan recorded no source group to classify: scan={scan} list={list}")
        })?
        .to_owned();

    let _: Value = app
        .invoke(
            "inbox_classify_source_group",
            serde_json::json!({
                "req": {
                    "sourceGroupId": group_id,
                    "rootAbsolutePath": root_absolute_path,
                }
            }),
        )
        .await?;

    let after: Value =
        app.invoke("inbox_list", serde_json::json!({ "req": { "limit": 500 } })).await?;
    after["items"]
        .as_array()
        .and_then(|items| {
            items.iter().find(|i| i["rootId"].as_str() == Some(root_id)).or_else(|| items.first())
        })
        .and_then(|i| i["inboxItemId"].as_str())
        .map(str::to_owned)
        .ok_or_else(|| {
            anyhow::anyhow!("classifying source group {group_id} materialized no item: {after}")
        })
}

/// Wait for the index route's async first-run redirect to land on `/setup`
/// BEFORE navigating anywhere.
///
/// A fresh DB (the harness resets it every launch) makes
/// `checkFirstRunComplete` redirect `/` → `/setup` from an async
/// `beforeLoad`; if a journey `goto_route`s while that redirect is still
/// pending, the late-resolving redirect can yank the app off the target
/// route.
pub async fn settle_first_run_redirect(app: &E2eApp) -> Result<()> {
    app.wait_url_contains("/setup", Duration::from_secs(15))
        .await
        .map(drop)
        .map_err(|e| anyhow!("expected a fresh DB to redirect to /setup: {e}"))
}
