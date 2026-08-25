// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! OS notifications for long-running task completions (spec 051 US8).
//!
//! Three completions notify: an approved filesystem plan finishing its apply
//! run, a tool workflow run being attributed as complete, and an
//! ingest-resolution drain pass that resolved at least one queued target. All
//! three are operations the user starts and then leaves, which is why they
//! notify and shorter foreground work does not.

use audit::bus::EventBus;
use audit::event_bus::{
    PlanApplyingCompleted, WorkflowRunCompleted, TOPIC_PLAN_APPLYING_COMPLETED,
    TOPIC_WORKFLOW_RUN_COMPLETED,
};
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::MAIN_WINDOW_LABEL;

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
/// notify on each: plan-apply terminal state and tool workflow-run completion.
///
/// Workflow runs notify off `workflow.run_completed`, not off the
/// `manifest.write.success` the manifest subscriber publishes downstream. The
/// manifest subscriber replays historical `workflow.run_completed` rows from the
/// durable events table when it lags, and each replay writes a manifest and
/// publishes a fresh success event, so notifying on that topic turns one lag
/// into a burst of notifications for runs the user finished long ago.
/// `workflow.run_completed` is published once, live, by the attribution pass.
pub fn spawn_completion_notifier(app: AppHandle, bus: &EventBus) -> tokio::task::JoinHandle<()> {
    use tokio::sync::broadcast::error::RecvError;

    let mut rx = bus.subscribe();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(env) => {
                    if let Some((title, body)) = notification_for(&env.topic, &env.payload) {
                        completed(&app, title, &body);
                    }
                }
                // Lag drops completions rather than replaying them: a
                // notification for an event the user has long since seen in
                // the UI is noise, and the outcome is durable in the events
                // table either way.
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => break,
            }
        }
    })
}

/// The notification a bus envelope earns, or `None` for every topic and payload
/// that must stay silent.
///
/// Split from [`spawn_completion_notifier`] so topic dispatch is testable
/// without a Tauri runtime.
fn notification_for(topic: &str, payload: &serde_json::Value) -> Option<(&'static str, String)> {
    match topic {
        TOPIC_PLAN_APPLYING_COMPLETED => Some(("Plan apply finished", plan_apply_body(payload)?)),
        TOPIC_WORKFLOW_RUN_COMPLETED => {
            Some(("Workflow run finished", workflow_run_body(payload)?))
        }
        _ => None,
    }
}

/// Summarise a `plan.applying.completed` payload for the notification body, or
/// `None` when the payload is not a full typed completion.
///
/// The topic carries one degraded publisher: when `list_pending_items` fails
/// twice, `cancel_pending_items` writes an aggregate audit row carrying only
/// `planId` and `cancelledCount`, and `handle_cancelled` publishes the real
/// completion afterwards. Defaulting the absent counts would report that
/// bookkeeping row as `done: 0 applied, 0 failed, 0 skipped` and then notify a
/// second time with the true summary, so a payload that does not deserialise is
/// dropped instead.
fn plan_apply_body(payload: &serde_json::Value) -> Option<String> {
    let event: PlanApplyingCompleted = serde_json::from_value(payload.clone()).ok()?;
    Some(format!(
        "{}: {} applied, {} failed, {} skipped.",
        event.terminal_state, event.items_applied, event.items_failed, event.items_skipped
    ))
}

/// Summarise a `workflow.run_completed` payload, or `None` when it does not
/// deserialise.
fn workflow_run_body(payload: &serde_json::Value) -> Option<String> {
    let event: WorkflowRunCompleted = serde_json::from_value(payload.clone()).ok()?;
    Some(format!(
        "{} run recorded: {} new artifact(s) attributed.",
        event.tool_id,
        event.artifact_ids.len()
    ))
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

    /// A full typed `PlanApplyingCompleted` payload, as `handle_terminal` and
    /// `handle_cancelled` publish it.
    fn plan_completion_payload() -> serde_json::Value {
        serde_json::json!({
            "planId": "p-1",
            "runId": "r-1",
            "terminalState": "partially_applied",
            "itemsApplied": 3,
            "itemsFailed": 1,
            "itemsSkipped": 2,
            "itemsCancelled": 0,
            "at": "2026-08-20T00:00:00Z",
        })
    }

    #[test]
    fn plan_apply_body_summarises_counts() {
        assert_eq!(
            plan_apply_body(&plan_completion_payload()).as_deref(),
            Some("partially_applied: 3 applied, 1 failed, 2 skipped.")
        );
    }

    #[test]
    fn the_degraded_bulk_cancel_payload_raises_no_notification() {
        // `cancel_pending_items` publishes this shape on the same topic when
        // `list_pending_items` fails twice. Defaulting its absent counts would
        // announce a successful-looking apply that never happened.
        let degraded = serde_json::json!({ "planId": "p-1", "cancelledCount": 4 });
        assert_eq!(plan_apply_body(&degraded), None);
        assert_eq!(plan_apply_body(&serde_json::json!({})), None);
    }

    #[test]
    fn workflow_run_body_names_the_tool_and_counts_artifacts() {
        let payload = serde_json::json!({
            "projectId": "prj-1",
            "toolId": "siril",
            "toolLaunchId": "tl-1",
            "completedAt": "2026-08-20T00:00:00Z",
            "artifactIds": ["a-1", "a-2"],
        });
        assert_eq!(
            workflow_run_body(&payload).as_deref(),
            Some("siril run recorded: 2 new artifact(s) attributed.")
        );
    }

    #[test]
    fn workflow_run_body_rejects_an_untyped_payload() {
        assert_eq!(workflow_run_body(&serde_json::json!({ "toolId": "siril" })), None);
    }

    fn workflow_run_payload() -> serde_json::Value {
        serde_json::json!({
            "projectId": "prj-1",
            "toolId": "siril",
            "toolLaunchId": "tl-1",
            "completedAt": "2026-08-20T00:00:00Z",
            "artifactIds": ["a-1"],
        })
    }

    #[test]
    fn the_plan_apply_topic_dispatches_to_the_plan_apply_notification() {
        assert_eq!(
            notification_for(TOPIC_PLAN_APPLYING_COMPLETED, &plan_completion_payload()),
            Some((
                "Plan apply finished",
                "partially_applied: 3 applied, 1 failed, 2 skipped.".to_owned()
            ))
        );
    }

    #[test]
    fn the_workflow_run_topic_dispatches_to_the_workflow_run_notification() {
        assert_eq!(
            notification_for(TOPIC_WORKFLOW_RUN_COMPLETED, &workflow_run_payload()),
            Some((
                "Workflow run finished",
                "siril run recorded: 1 new artifact(s) attributed.".to_owned()
            ))
        );
    }

    #[test]
    fn an_unsubscribed_topic_raises_no_notification() {
        // Every other topic on the shared bus reaches this subscriber.
        assert_eq!(notification_for("plan.applying.started", &plan_completion_payload()), None);
        assert_eq!(notification_for("manifest.write.success", &workflow_run_payload()), None);
    }

    #[test]
    fn a_subscribed_topic_with_an_undeserialisable_payload_raises_no_notification() {
        let degraded = serde_json::json!({ "planId": "p-1", "cancelledCount": 4 });
        assert_eq!(notification_for(TOPIC_PLAN_APPLYING_COMPLETED, &degraded), None);
        assert_eq!(notification_for(TOPIC_WORKFLOW_RUN_COMPLETED, &degraded), None);
    }
}
