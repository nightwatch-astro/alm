// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Spec 037 Layer-2 real-UI journey: source-view generation (spec 049).
//!
//! Real backend REAL: `roots.register`, `inbox.scan.folder`,
//! `inbox.classify`, `inbox.confirm`, `inbox.plan.apply` (catalogue-in-place —
//! see below), `projects.create`, `projects.source.add`, `sourceview.generate`
//! (driven through the real `GenerateSourceViewDialog` UI, not the invoke
//! bridge), `plans.list`, `plans.get`, `plans.approve`.
//!
//! Real DOM: select the project row (`project-row-<id>`), open the
//! `SourceViewsSection`'s "Generate source view" dialog
//! (`generate-source-view-btn` → `generate-source-view-dialog`), and submit it
//! (`generate-source-view-submit`) — the actual product click path a user
//! takes, not an IPC-only round trip.
//!
//! Catalogue-in-place (not move): `roots.register` defaults non-inbox
//! categories to `organized` (`apps/desktop/src-tauri/src/commands/roots.rs`),
//! so `inbox.confirm` emits a `catalogue` action (`from == to`, no move —
//! `crates/app/inbox/src/confirm.rs`). This journey never calls
//! `sources.set_organization_state`, so the fixture FITS file stays at its
//! original path — real, but simpler to reason about than the move variant
//! the existing `plan_review_apply_with_audit` journey already covers.
//!
//! RESOLVED FINDING (#1218): `projects.source.add` and `projects.create`'s
//! `initialSources` path (`crates/app/projects/src/project_setup.rs`) used to
//! hardcode `filter_snapshot`/`exposure_snapshot` to `""`, so
//! `sourceview.generate`'s WBPP `{date}/{filter}/{exposure}` layout
//! (`crates/app/projects/src/source_view_generate.rs`) landed EVERY real
//! project-linked session in the pattern's `nofilter`/`unknown-exposure`
//! fallback buckets. Both fields are now snapshotted from the session, so
//! this journey asserts the frame's real filter below.
//!
//! Exposure resolves to the frame's real `300s`. It used to be
//! `unknown-exposure` here, because the fixture wrote no `EXPTIME` card — but
//! spec 058 made that combination unreachable end to end: an ordinary folder
//! now yields real item rows only through classification, and a light frame
//! without `EXPTIME` fails the mandatory-attribute gate before any plan
//! exists. The fixture therefore supplies `EXPTIME`, and the assertion below
//! expects the token that produces.
//!
//! No coverage is lost: `crates/app/core/tests/project_source_snapshots.rs`
//! still pins the missing-exposure combination at Layer 1
//! (`missing_exposure_metadata_keeps_the_real_filter`), which is the right
//! layer for it now that Layer 3 cannot construct it.
//!
//! KNOWN GAP (documented, not faked — mirrors `cleanup_plan_review` in
//! `journeys.rs`): materializing the real symlink/junction on disk requires
//! `plans.apply_real`, whose `tauri::ipc::Channel` argument only the product
//! frontend's `applyPlan` helper (`apps/desktop/src/features/plans/
//! planApply.ts`) constructs today, via `usePlanApplyProgress` →
//! `PlanReviewOverlay`. Nothing wires the `sourceview.generate` plan id into
//! that overlay: `SourceViewsSection.handleGenerated` calls
//! `onPlanCreated?.(planId)`, but its only real mount point,
//! `ProjectBottomDetail`, never passes an `onPlanCreated` prop — so both that
//! callback and the toast's "View plan" action are silent no-ops
//! (`git grep -n onPlanCreated apps/desktop/src/features/projects` shows the
//! prop is optional and unconnected on this path). Fabricating a Channel from
//! a WebDriver script instead would mean reaching into product frontend/Tauri
//! -internal plumbing beyond a thin test hook — the same call already made
//! for `cleanup_plan_review` — so this journey stops at `approved` via the
//! real, channel-free `plans.approve` command. Real, on-disk symlink/junction
//! proof for spec 049 is BLOCKED on either (a) a channel-free apply command
//! (matching `inbox.plan.apply`'s precedent), or (b) wiring
//! `PlanReviewOverlay` (or an equivalent) into `SourceViewsSection` so a real
//! UI Apply button exists to click.
//!
//! Run (CI): `cargo nextest run -p e2e_tests --profile e2e --run-ignored all`
//! (serial, `.config/nextest.toml`). See `crates/e2e-tests/tests/journeys.rs`
//! module docs and `README.md` for the full local run procedure.

mod common;

use std::time::Duration;

use common::{settle_first_run_redirect, write_minimal_fits_with_exposure, E2eApp};
use serde_json::json;

const INVOKE_TIMEOUT: Duration = Duration::from_secs(30);

/// Registers a disposable `project`-category root purely to satisfy
/// `firstrun.complete`'s precondition (one `light_frames` root — this
/// journey's own ingest root satisfies that half — AND one `project` root,
/// `crates/persistence/lifecycle/src/repositories/first_run`), then routes
/// through the real gate. A `projects.create` Project entity (this journey
/// creates one below) is a DIFFERENT concept from a registered `project`
/// source root and does not satisfy this precondition on its own. Without
/// this, `Shell.tsx`'s client-side `setupCompleted` gate bounces every
/// `goto_route` to a Shell-wrapped page (`/projects`) back to `/setup`
/// indefinitely (mirrors the proven `inbox_ui_journeys.rs` pattern).
async fn complete_first_run(app: &E2eApp) -> anyhow::Result<()> {
    let project_dir = tempfile::tempdir()?;
    let _: serde_json::Value = app
        .invoke(
            "roots_register",
            json!({ "path": project_dir.path().to_string_lossy(), "category": "project", "scanSettings": null }),
        )
        .await?;
    app.complete_first_run_gate().await
}

/// Generate Source View dialog → real reviewable `prepared_view_generation`
/// plan with the WBPP per-tool layout → approve.
#[tokio::test]
#[ignore = "Layer-2 real-UI journey: needs tauri-webdriver CLI + desktop_shell --features e2e + served frontend; run via e2e.yml (--run-ignored all)"]
async fn generate_source_view_creates_reviewable_wbpp_plan() -> anyhow::Result<()> {
    let app = E2eApp::launch().await?;
    app.wait_bridge_ready(Duration::from_secs(30)).await?;
    settle_first_run_redirect(&app).await?;

    // ── 1. Real ingest precondition: one real, catalogued-in-place light frame ──
    let root_dir = tempfile::tempdir()?;
    let file_name = "light_m33_001.fits";
    let light_path = write_minimal_fits_with_exposure(
        root_dir.path(),
        file_name,
        "Light Frame",
        Some("M 33"),
        Some("Ha"),
        Some("2026-01-12T22:00:00"),
        Some(300.0),
    )?;
    anyhow::ensure!(light_path.exists(), "fixture FITS file was not written");

    let register: serde_json::Value = app
        .invoke(
            "roots_register",
            json!({
                "path": root_dir.path().to_string_lossy(),
                "category": "light_frames",
                "scanSettings": null,
            }),
        )
        .await?;
    let root_id = register["sourceId"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("roots.register returned no sourceId: {register}"))?
        .to_owned();

    // Spec 058 T012: scan records a source group and no placeholder item;
    // classification materializes the real rows. See
    // `common::scan_and_classify_one_item`.
    let inbox_item_id = common::scan_and_classify_one_item(
        &app,
        &root_id,
        root_dir.path().to_string_lossy().as_ref(),
    )
    .await?;

    let classify: serde_json::Value = app
        .invoke(
            "inbox_classify",
            json!({
                "req": {
                    "inboxItemId": inbox_item_id,
                    "forceRescan": false,
                    "rootAbsolutePath": root_dir.path().to_string_lossy(),
                }
            }),
        )
        .await?;
    let content_signature = classify["contentSignature"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("inbox.classify returned no contentSignature: {classify}"))?
        .to_owned();

    let confirm: serde_json::Value = app
        .invoke(
            "inbox_confirm",
            json!({
                "req": {
                    "inboxItemId": inbox_item_id,
                    "contentSignature": content_signature,
                    "destructiveDestination": null,
                    "rootAbsolutePath": root_dir.path().to_string_lossy(),
                    "rootId": null,
                }
            }),
        )
        .await?;
    anyhow::ensure!(
        confirm["planId"].as_str().is_some_and(|s| !s.is_empty()),
        "expected a real (non-empty) plan id from inbox.confirm: {confirm}"
    );

    // Constitution II (astro-plan-tykek): `inbox.plan.apply` refuses a plan the
    // caller has not approved, mirroring the UI's approve-then-apply gesture.
    let _: serde_json::Value =
        app.invoke("plans_approve", json!({ "id": confirm["planId"] })).await?;
    let _apply: serde_json::Value =
        app.invoke("inbox_plan_apply", json!({ "inboxItemId": inbox_item_id })).await?;
    anyhow::ensure!(
        light_path.exists(),
        "catalogue-in-place (organized default) must never move the file: {light_path:?}"
    );

    // Event-driven session grouping (spec 035 US4 plan_listener).
    let sessions: serde_json::Value = app
        .invoke_until("sessions_list", json!({}), INVOKE_TIMEOUT, |v: &serde_json::Value| {
            v.as_array().is_some_and(|arr| arr.iter().any(|s| s["sessionKey"]["target"] == "M 33"))
        })
        .await?;
    let session = sessions
        .as_array()
        .and_then(|arr| arr.iter().find(|s| s["sessionKey"]["target"] == "M 33"))
        .ok_or_else(|| anyhow::anyhow!("no M 33 session found: {sessions}"))?;
    let session_id = session["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("session has no id: {session}"))?
        .to_owned();

    // ── 2. Real project + real project-source link (setup precondition) ──
    //
    // `sourceview.generate` reads `project_sources`, populated here via the
    // same real `projects.source.add` backend the "Add sources" UI drives —
    // this journey's own DOM focus is the Generate Source View surface (the
    // companion `inventory_journeys.rs` drives the Add-sources UI itself), so
    // the link is set up over the invoke bridge like every other journey's
    // preconditions (project creation, root registration, etc).
    let project_dir = tempfile::tempdir()?;
    let create: serde_json::Value = app
        .invoke(
            "projects_create",
            json!({
                "req": {
                    "requestId": "e2e-sourceview-create",
                    "name": "E2E Source View Project",
                    "tool": "PixInsight",
                    "path": project_dir.path().to_string_lossy(),
                    "initialSources": [],
                    "notes": null,
                    "canonicalTargetId": null,
                }
            }),
        )
        .await?;
    let project_id = create["projectId"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("projects.create returned no projectId: {create}"))?
        .to_owned();

    let _add_source: serde_json::Value = app
        .invoke(
            "projects_source_add",
            json!({
                "req": {
                    "requestId": "e2e-sourceview-add-source",
                    "projectId": project_id,
                    "inventorySessionId": session_id,
                }
            }),
        )
        .await?;

    complete_first_run(&app).await?;

    // ── 3. Real UI: select the project, open Generate Source View, submit ──
    app.goto_route("/projects").await?;
    app.wait_bridge_ready(Duration::from_secs(15)).await?;
    app.wait_testid(&format!("project-row-{project_id}"), Duration::from_secs(15))
        .await?
        .click()
        .await?;

    app.wait_testid("generate-source-view-btn", Duration::from_secs(15)).await?.click().await?;
    app.wait_testid("generate-source-view-dialog", Duration::from_secs(10)).await?;

    // Round 4/5 (#470) root cause: the dialog's own diagnostics dump showed
    // a real `pv-banner--danger` reading "No usable link method is
    // available on this filesystem. Allow copying to proceed instead." —
    // the CI runner's tempdir filesystem cannot symlink OR hardlink at all,
    // `sourceview.generate` correctly refuses with `no_link_kind`
    // (`domain_core::source_view::resolve_link_kind`), and the dialog's
    // error Banner rendered exactly as designed. The product was behaving
    // correctly; this journey just never opted in to the documented copy
    // fallback. Always check the copy opt-in checkbox rather than
    // conditionally reacting to the banner (racier — the banner render lags
    // the settings/capability fetch) or trying to make the fixture's
    // tempdirs support real links (CI tmpfs/overlay link support is
    // runner-/OS-dependent and not something this repo controls, so making
    // that work would just trade one environment-fragile assumption for
    // another): `copy_opt_in` is consulted only as a last-resort fallback
    // (`resolve_link_kind`) after every real link kind has already been
    // tried, so checking it here never changes behavior on a runner where
    // linking genuinely works — it only unblocks the runners where it
    // doesn't.
    app.wait_testid("generate-view-copy-opt-in", Duration::from_secs(10)).await?.click().await?;

    app.wait_testid_enabled("generate-source-view-submit", Duration::from_secs(10)).await?;
    app.find_testid("generate-source-view-submit").await?.click().await?;

    // `GenerateSourceViewDialog.handleSubmit` only calls `onClose()` after
    // `sourceview.generate` resolves successfully, so the dialog closing is
    // real, DOM-visible proof the submit actually reached and completed the
    // backend call — a much sharper signal than the `plans_list` poll below
    // if the click never registered or the call errored. This is
    // diagnostic-only (never gates the journey): a slow/absent close
    // animation without a genuine submit failure would otherwise produce a
    // false negative here even though the plan really was created, and the
    // real assertion is the `plans_list` read below regardless. On failure,
    // dump the dialog's own DOM (it renders the submit error inline via a
    // `Banner`, `GenerateSourceViewDialog.tsx`) plus any buffered uncaught
    // errors, and fold that evidence into the `plans_list` timeout message
    // if the plan never shows up either.
    let dialog_close_diagnostics =
        match app.wait_testid_gone("generate-source-view-dialog", Duration::from_secs(15)).await {
            Ok(()) => None,
            Err(e) => {
                let diagnostics = app.dump_testid_diagnostics("generate-source-view-dialog").await;
                Some(format!(
                    "the dialog never closed after submit — sourceview.generate likely errored or \
                 the submit click never registered: {e}\ndiagnostics: {diagnostics}"
                ))
            }
        };

    // ── 4. Real backend proof: a real, reviewable plan was created ──
    //
    // No product UI routes this plan id back for review (KNOWN GAP, module
    // docs), so find it via the real `plans.list` read path instead of a
    // fabricated return value.
    let plans_poll = app
        .invoke_until(
            "plans_list",
            json!({
                "stateFilter": null,
                "originFilter": ["prepared_view_generation"],
                "createdAfter": null,
                "limit": null,
            }),
            INVOKE_TIMEOUT,
            |v: &serde_json::Value| {
                v["plans"]
                    .as_array()
                    .is_some_and(|arr| arr.iter().any(|p| p["originPath"] == json!(project_id)))
            },
        )
        .await;
    let plans: serde_json::Value = match plans_poll {
        Ok(v) => v,
        Err(e) => {
            // Round 4 (#470): the previous round's raw-payload dump showed
            // `{"plans": []}` — no `prepared_view_generation` plan exists AT
            // ALL, even though the dialog closed (a real success response).
            // `generate_source_view` (`crates/app/projects/src/
            // source_view_generate.rs`) has no early-return success path
            // that skips `insert_plan` — every branch either errors before
            // persisting anything or persists the plan and advances it to
            // `ready_for_review` before returning `Ok`. So a genuine silent
            // "success with nothing persisted" would itself be a real
            // product bug worth proving directly rather than guessing.
            // Gather three more pieces of ground truth, all best-effort
            // (never masking the real error below):
            // (a) plans.list with NO origin filter — sanity-checks the read
            //     path itself still works and shows whatever plans DO exist
            //     (e.g. the earlier inbox-confirm plan).
            // (b) sessions.list — the project's linked session/frame state
            //     at THIS moment, to rule out something unlinking it
            //     between setup and submit.
            // (c) a second, direct bridge invoke of the SAME
            //     `sourceview.generate` call with the same project id: if
            //     it also resolves, the exact response (including any
            //     warnings) proves what the real call returns; if it now
            //     errors, that error is the real root cause the UI's first
            //     call hit too (masked from the test only by the dialog's
            //     resolved-promise-closes-unconditionally-on-success logic).
            let all_plans: serde_json::Value = app
                .invoke(
                    "plans_list",
                    json!({
                        "stateFilter": null,
                        "originFilter": null,
                        "createdAfter": null,
                        "limit": null,
                    }),
                )
                .await
                .unwrap_or_else(|e2| json!({ "plans_list_no_filter_error": e2.to_string() }));
            let sessions_now: serde_json::Value = app
                .invoke("sessions_list", json!({}))
                .await
                .unwrap_or_else(|e2| json!({ "sessions_list_error": e2.to_string() }));
            let regenerate: serde_json::Value = app
                .invoke(
                    "sourceview_generate",
                    json!({ "req": { "projectId": project_id, "copyOptIn": true, "strict": false, "profileId": null, "destinationOverride": null } }),
                )
                .await
                .unwrap_or_else(|e2| json!({ "sourceview_generate_retry_error": e2.to_string() }));
            let dialog_evidence = dialog_close_diagnostics
                .as_deref()
                .unwrap_or("(dialog closed — submit's promise resolved)");
            return Err(anyhow::anyhow!(
                "{e}\n\nsubmit-step evidence: {dialog_evidence}\n\ndiagnostic plans.list (no \
                 origin filter): {all_plans}\n\ndiagnostic sessions.list at failure time: \
                 {sessions_now}\n\ndiagnostic direct retry of sourceview.generate for the same \
                 project: {regenerate}"
            ));
        }
    };
    let plan = plans["plans"]
        .as_array()
        .and_then(|arr| arr.iter().find(|p| p["originPath"] == json!(project_id)))
        .ok_or_else(|| {
            anyhow::anyhow!("no prepared_view_generation plan found for {project_id}: {plans}")
        })?;
    let plan_id =
        plan["id"].as_str().ok_or_else(|| anyhow::anyhow!("plan has no id: {plan}"))?.to_owned();
    anyhow::ensure!(
        plan["state"] == "ready_for_review",
        "expected the generated plan to be ready_for_review: {plan}"
    );

    // ── Real per-tool layout proof (spec 049 US2) ──
    //
    // The WBPP/PixInsight default profile groups lights 3 levels deep
    // (night / filter / exposure) under `<project>/source-views/<plan_id>/`.
    // Filter is the fixture frame's real "Ha" (#1218); exposure is the frame's
    // real "300s" — see the module docs for why this is no longer the
    // "unknown-exposure" fallback.
    let detail: serde_json::Value = app.invoke("plans_get", json!({ "id": plan_id })).await?;
    let link_item = detail["items"]
        .as_array()
        .and_then(|items| items.iter().find(|i| i["action"] == "link"))
        .ok_or_else(|| anyhow::anyhow!("generated plan has no link item: {detail}"))?;
    let to_path = link_item["to"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("link item has no destination path: {link_item}"))?;
    let expected_prefix =
        format!("{}/source-views/{plan_id}/", project_dir.path().to_string_lossy());
    anyhow::ensure!(
        to_path.starts_with(&expected_prefix),
        "expected the generated destination under the project's own source-views/<plan_id> \
         tree: {to_path} (expected prefix {expected_prefix})"
    );
    let layout_tail = to_path.strip_prefix(&expected_prefix).ok_or_else(|| {
        anyhow::anyhow!("prefix check above should guarantee this strip succeeds: {to_path}")
    })?;
    let layout_segments: Vec<&str> = layout_tail.split('/').collect();
    anyhow::ensure!(
        layout_segments.len() == 4 && layout_segments[3] == file_name,
        "expected the WBPP 3-level {{date}}/{{filter}}/{{exposure}} layout ending in the real \
         frame's basename, got: {layout_tail} (from {to_path})"
    );
    anyhow::ensure!(
        layout_segments[1] == "Ha",
        "the linked session's real filter must reach the layout (#1218 regression — a \
         `nofilter` here means the project source snapshot went empty again): {layout_tail}"
    );
    anyhow::ensure!(
        layout_segments[2] == "300s",
        "the fixture's real EXPTIME must reach the layout; `unknown-exposure` here means the \
         exposure stopped being snapshotted from the session, the same class of regression \
         #1218 fixed for filter: {layout_tail}"
    );

    // Document which materialization path this run actually exercised
    // (`domain_core::source_view::resolve_link_kind`): a real link kind
    // (symlink/hardlink/junction) when the runner's filesystem supports one,
    // or the `copy` fallback this journey's `copyOptIn` check unblocks when
    // it doesn't (observed on the CI ubuntu runner, round 4/5 — see the
    // copy-opt-in comment above). Either is a fully reviewable plan; this
    // only asserts the value is a real, known materialization rather than
    // requiring one specific kind, since runner filesystem capability is
    // environment-dependent and not something this journey controls.
    let materialization = link_item["provenance"]
        .as_array()
        .and_then(|entries| entries.iter().find(|e| e["label"] == "materialization"))
        .and_then(|e| e["value"].as_str())
        .ok_or_else(|| {
            anyhow::anyhow!("link item has no materialization provenance: {link_item}")
        })?;
    anyhow::ensure!(
        matches!(materialization, "symlink" | "hardlink" | "junction" | "copy"),
        "unexpected materialization value on the generated link item: {materialization:?} \
         ({link_item})"
    );

    // ── 5. Real, channel-free step available today: approve ──
    let approve: serde_json::Value = app.invoke("plans_approve", json!({ "id": plan_id })).await?;
    anyhow::ensure!(
        approve["planId"] == json!(plan_id) && approve["newState"] == "approved",
        "expected plans.approve to move the generated plan to approved: {approve}"
    );

    // Apply (real symlink/junction materialization) is BLOCKED — see the
    // KNOWN GAP in this file's module docs.

    app.shutdown().await
}
