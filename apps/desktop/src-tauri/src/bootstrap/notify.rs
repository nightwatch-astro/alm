// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! OS notifications for long-running task completions (spec 051 US8).
//!
//! Three completions notify: an approved filesystem plan finishing its apply
//! run, a workflow-run manifest being written, and an ingest-resolution drain
//! pass that resolved at least one queued target. All three are operations the
//! user starts and then leaves, which is why they notify and shorter
//! foreground work does not.

use audit::bus::EventBus;
use audit::event_bus::TOPIC_PLAN_APPLYING_COMPLETED;
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::MAIN_WINDOW_LABEL;

/// Audit topic published by `app_core::project_manifests::write` on a
/// successful manifest write. Not a `TOPIC_*` constant in `audit-types`: the
/// manifest writer publishes it as a literal.
const TOPIC_MANIFEST_WRITE_SUCCESS: &str = "manifest.write.success";

/// The `manifests.reason` value written when a workflow-run completion
/// triggered the manifest (`project_structure::manifest::ManifestReason`).
const MANIFEST_REASON_WORKFLOW_RUN: &str = "workflow_run";

/// Whether a completed task should raise an OS notification.
///
/// `focused` is `None` when window focus could not be read (no main window
/// yet, or the runtime call failed); an unknown focus notifies, because losing
/// the notification is the worse failure of the two.
pub const fn should_notify(focused: Option<bool>, did_meaningful_work: bool) -> bool {
    did_meaningful_work && !matches!(focused, Some(true))
}

/// Read the main window's focus state, or `None` if it cannot be determined.
fn main_window_focused<R: tauri::Runtime>(app: &AppHandle<R>) -> Option<bool> {
    app.get_webview_window(MAIN_WINDOW_LABEL)?.is_focused().ok()
}

/// Show an OS notification unless the main window is focused.
///
/// Every failure path is a `debug` log and a return: on macOS and Windows the
/// user may have denied notifications, and on Linux there may be no
/// notification daemon at all, and none of those may disturb the task being
/// reported on (FR-025).
pub fn completed<R: tauri::Runtime>(app: &AppHandle<R>, title: &str, body: &str) {
    if !should_notify(main_window_focused(app), true) {
        return;
    }
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        tracing::debug!("OS notification not delivered: {e}");
    }
}

/// Notify about a finished ingest-resolution drain pass.
///
/// The periodic drain ticks every 30s and is usually a no-op, so only a pass
/// that resolved at least one queued target notifies.
pub fn ingest_drain_completed<R: tauri::Runtime>(
    app: &AppHandle<R>,
    summary: &app_core::ingest_resolution::DrainSummary,
) {
    if !should_notify(main_window_focused(app), summary.resolved > 0) {
        return;
    }
    let body = format!("Resolved {} queued target(s).", summary.resolved);
    if let Err(e) =
        app.notification().builder().title("Target resolution finished").body(body).show()
    {
        tracing::debug!("OS notification not delivered: {e}");
    }
}

/// Subscribe to the two completion topics that already exist on the bus and
/// notify on each: plan-apply terminal state and workflow-run manifest write.
pub fn spawn_completion_notifier(app: AppHandle, bus: &EventBus) -> tokio::task::JoinHandle<()> {
    use tokio::sync::broadcast::error::RecvError;

    let mut rx = bus.subscribe();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(env) if env.topic == TOPIC_PLAN_APPLYING_COMPLETED => {
                    completed(&app, "Plan apply finished", &plan_apply_body(&env.payload));
                }
                Ok(env) if env.topic == TOPIC_MANIFEST_WRITE_SUCCESS => {
                    if manifest_reason(&env.payload) == Some(MANIFEST_REASON_WORKFLOW_RUN) {
                        completed(
                            &app,
                            "Workflow run recorded",
                            "A project manifest was written for the completed workflow run.",
                        );
                    }
                }
                // Lag drops completions rather than replaying them: a
                // notification for an event the user has long since seen in
                // the UI is noise, and the outcome is durable in the events
                // table either way.
                Ok(_) | Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => break,
            }
        }
    })
}

/// Summarise a `plan.applying.completed` payload for the notification body.
fn plan_apply_body(payload: &serde_json::Value) -> String {
    let count = |key: &str| payload.get(key).and_then(serde_json::Value::as_i64).unwrap_or(0);
    let state = payload.get("terminalState").and_then(serde_json::Value::as_str).unwrap_or("done");
    format!(
        "{state}: {} applied, {} failed, {} skipped.",
        count("itemsApplied"),
        count("itemsFailed"),
        count("itemsSkipped")
    )
}

fn manifest_reason(payload: &serde_json::Value) -> Option<&str> {
    payload.get("reason").and_then(serde_json::Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focused_window_suppresses_the_notification() {
        assert!(!should_notify(Some(true), true));
    }

    #[test]
    fn unfocused_window_notifies() {
        assert!(should_notify(Some(false), true));
    }

    #[test]
    fn unknown_focus_notifies() {
        assert!(should_notify(None, true));
    }

    #[test]
    fn an_empty_drain_pass_never_notifies() {
        assert!(!should_notify(Some(false), false));
        assert!(!should_notify(None, false));
    }

    #[test]
    fn plan_apply_body_summarises_counts() {
        let payload = serde_json::json!({
            "terminalState": "partially_applied",
            "itemsApplied": 3,
            "itemsFailed": 1,
            "itemsSkipped": 2,
        });
        assert_eq!(plan_apply_body(&payload), "partially_applied: 3 applied, 1 failed, 2 skipped.");
    }

    #[test]
    fn plan_apply_body_tolerates_a_payload_missing_every_field() {
        assert_eq!(
            plan_apply_body(&serde_json::json!({})),
            "done: 0 applied, 0 failed, 0 skipped."
        );
    }

    #[test]
    fn manifest_reason_selects_only_workflow_run_writes() {
        let workflow = serde_json::json!({ "reason": "workflow_run" });
        let created = serde_json::json!({ "reason": "created" });
        assert_eq!(manifest_reason(&workflow), Some(MANIFEST_REASON_WORKFLOW_RUN));
        assert_ne!(manifest_reason(&created), Some(MANIFEST_REASON_WORKFLOW_RUN));
        assert_eq!(manifest_reason(&serde_json::json!({})), None);
    }
}
