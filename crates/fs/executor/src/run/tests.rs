// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

use std::sync::Arc;

use camino::{Utf8Path, Utf8PathBuf};
use tokio::sync::Mutex;

use super::*;

fn utf8(p: &std::path::Path) -> Utf8PathBuf {
    Utf8PathBuf::from_path_buf(p.to_path_buf()).expect("temp dir path is UTF-8")
}

// ── Fake callbacks ────────────────────────────────────────────────────────

#[derive(Default)]
struct FakeCallbacks {
    events: Arc<Mutex<Vec<ItemProgressEvent>>>,
}

impl ExecutorCallbacks for FakeCallbacks {
    fn on_item_start(
        &self,
        _item_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(async {})
    }

    fn on_item_progress(
        &self,
        event: ItemProgressEvent,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        let events = self.events.clone();
        Box::pin(async move {
            events.lock().await.push(event);
        })
    }
}

fn make_move_item(id: &str, src: &Utf8Path, dst: &Utf8Path) -> ExecutorItem {
    ExecutorItem {
        id: id.to_owned(),
        plan_id: "p1".to_owned(),
        action: ExecutorItemAction::Move,
        // No library_root: pass absolute paths as-is (legacy mode).
        source_path: Some(src.to_path_buf()),
        destination_path: Some(dst.to_path_buf()),
        library_root: None,
        destination_root: None,
        cas_snapshot: CasSnapshot { approved_mtime: None, approved_size_bytes: None },
        is_protected: false,
        requires_destructive_confirm: false,
        destructive_confirmed: false,
        current_state: "pending".to_owned(),
    }
}

#[tokio::test]
async fn happy_path_all_succeed() {
    let dir = tempfile::tempdir().unwrap();
    let root = utf8(dir.path());
    let src = root.join("file.fits");
    let dst = root.join("dest.fits");
    std::fs::write(&src, b"data").unwrap();

    let item = make_move_item("item-1", &src, &dst);
    let callbacks = FakeCallbacks::default();
    let cancel = CancellationToken::new();
    let skip = SkipSet::new();
    let retry = RetryQueue::new();

    let outcome = execute_plan(vec![item], &callbacks, &cancel, &skip, &retry).await;

    let events = callbacks.events.lock().await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].new_state, "succeeded");
    drop(events);

    match outcome {
        ApplyOutcome::Completed(counts) => {
            assert_eq!(counts.succeeded, 1);
            assert_eq!(counts.failed, 0);
        }
        other => panic!("expected Completed, got {other:?}"),
    }
    assert!(dst.exists());
    assert!(!src.exists());
}

/// astro-plan-3v3r.9.29: the source gate cannot see a destination that escapes
/// its own root, so this is the last line of defence -- it must refuse even
/// when handed an escaping destination directly, with layers 1 and 2 bypassed.
#[tokio::test]
async fn destination_outside_destination_root_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let root = utf8(dir.path());
    let view_root = root.join("source-views/plan-1");
    std::fs::create_dir_all(&view_root).unwrap();
    let src = view_root.join("frame.fits");
    std::fs::write(&src, b"data").unwrap();
    let escaping = Utf8PathBuf::from("../../../outside.fits");

    let mut item = make_move_item("item-1", Utf8Path::new("frame.fits"), &escaping);
    item.library_root = Some(view_root.clone());
    item.destination_root = Some(view_root.clone());

    let callbacks = FakeCallbacks::default();
    let outcome = execute_plan(
        vec![item],
        &callbacks,
        &CancellationToken::new(),
        &SkipSet::new(),
        &RetryQueue::new(),
    )
    .await;

    let events = callbacks.events.lock().await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].new_state, "refused");
    assert_eq!(
        events[0].failure.as_ref().map(|f| f.code),
        Some(crate::failure::FailureCode::RootEscape)
    );
    drop(events);

    match outcome {
        ApplyOutcome::Completed(counts) => assert_eq!(counts.failed, 1),
        other => panic!("expected Completed, got {other:?}"),
    }
    assert!(src.exists(), "the source must be untouched");
    assert!(!root.join("outside.fits").exists());
    assert!(!dir.path().parent().unwrap().join("outside.fits").exists());
}

/// The destination root outranks the source root: a source-view item joins its
/// destination against the picked root, not the root its frames came from.
#[tokio::test]
async fn destination_under_destination_root_is_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let root = utf8(dir.path());
    let source_root = root.join("library");
    let view_root = root.join("views");
    std::fs::create_dir_all(&source_root).unwrap();
    std::fs::create_dir_all(&view_root).unwrap();
    std::fs::write(source_root.join("frame.fits"), b"data").unwrap();

    let mut item =
        make_move_item("item-1", Utf8Path::new("frame.fits"), Utf8Path::new("lights/frame.fits"));
    item.library_root = Some(source_root);
    item.destination_root = Some(view_root.clone());

    let callbacks = FakeCallbacks::default();
    let outcome = execute_plan(
        vec![item],
        &callbacks,
        &CancellationToken::new(),
        &SkipSet::new(),
        &RetryQueue::new(),
    )
    .await;

    let events = callbacks.events.lock().await;
    assert_eq!(events[0].new_state, "succeeded", "failure: {:?}", events[0].failure);
    drop(events);
    match outcome {
        ApplyOutcome::Completed(counts) => assert_eq!(counts.succeeded, 1),
        other => panic!("expected Completed, got {other:?}"),
    }
    assert!(view_root.join("lights/frame.fits").exists());
}

fn make_archive_item(id: &str, src: &Utf8Path, archive_destination: &Utf8Path) -> ExecutorItem {
    ExecutorItem {
        id: id.to_owned(),
        plan_id: "p1".to_owned(),
        action: ExecutorItemAction::Archive {
            archive_destination: archive_destination.to_path_buf(),
        },
        source_path: Some(src.to_path_buf()),
        destination_path: None,
        library_root: None,
        destination_root: None,
        cas_snapshot: CasSnapshot { approved_mtime: None, approved_size_bytes: None },
        is_protected: false,
        requires_destructive_confirm: false,
        destructive_confirmed: false,
        current_state: "pending".to_owned(),
    }
}

/// astro-plan-zboex: the archive destination is a third destination field and
/// was neither resolved nor gated, so it reached `move_file` and wrote outside
/// the library root the user granted.
#[tokio::test]
async fn archive_destination_outside_the_library_root_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let outer = utf8(dir.path());
    let root = outer.join("library");
    std::fs::create_dir_all(&root).unwrap();
    let src = root.join("frame.fits");
    std::fs::write(&src, b"data").unwrap();

    let mut item = make_archive_item(
        "item-1",
        Utf8Path::new("frame.fits"),
        Utf8Path::new("../escaped/frame.fits"),
    );
    item.library_root = Some(root.clone());

    let callbacks = FakeCallbacks::default();
    let outcome = execute_plan(
        vec![item],
        &callbacks,
        &CancellationToken::new(),
        &SkipSet::new(),
        &RetryQueue::new(),
    )
    .await;

    let events = callbacks.events.lock().await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].new_state, "refused", "failure: {:?}", events[0].failure);
    assert_eq!(
        events[0].failure.as_ref().map(|f| f.code),
        Some(crate::failure::FailureCode::RootEscape)
    );
    drop(events);

    match outcome {
        ApplyOutcome::Completed(counts) => assert_eq!(counts.failed, 1),
        other => panic!("expected Completed, got {other:?}"),
    }
    assert!(src.exists(), "the source must be untouched");
    assert!(
        !outer.join("escaped/frame.fits").exists(),
        "the archive must not have escaped the library root"
    );
}

/// The companion direction: gating the third field must not refuse the ordinary
/// root-relative archive destination both generators write.
#[tokio::test]
async fn archive_destination_under_the_library_root_is_applied() {
    let dir = tempfile::tempdir().unwrap();
    let root = utf8(dir.path());
    let src = root.join("frame.fits");
    std::fs::write(&src, b"data").unwrap();

    let mut item = make_archive_item(
        "item-1",
        Utf8Path::new("frame.fits"),
        Utf8Path::new(".astro-plan-archive/p1/item-1-frame.fits"),
    );
    item.library_root = Some(root.clone());

    let callbacks = FakeCallbacks::default();
    let outcome = execute_plan(
        vec![item],
        &callbacks,
        &CancellationToken::new(),
        &SkipSet::new(),
        &RetryQueue::new(),
    )
    .await;

    let events = callbacks.events.lock().await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].new_state, "succeeded", "failure: {:?}", events[0].failure);
    drop(events);

    match outcome {
        ApplyOutcome::Completed(counts) => assert_eq!(counts.succeeded, 1),
        other => panic!("expected Completed, got {other:?}"),
    }
    assert!(!src.exists(), "the source must have moved");
    assert!(root.join(".astro-plan-archive/p1/item-1-frame.fits").exists());
}

#[tokio::test]
async fn item_in_failed_state_is_skipped_by_executor() {
    let item = ExecutorItem {
        id: "item-1".to_owned(),
        plan_id: "p1".to_owned(),
        action: ExecutorItemAction::NoOp,
        source_path: None,
        destination_path: None,
        library_root: None,
        destination_root: None,
        cas_snapshot: CasSnapshot { approved_mtime: None, approved_size_bytes: None },
        is_protected: false,
        requires_destructive_confirm: false,
        destructive_confirmed: false,
        current_state: "failed".to_owned(), // already terminal
    };

    let callbacks = FakeCallbacks::default();
    let cancel = CancellationToken::new();
    let skip = SkipSet::new();
    let retry = RetryQueue::new();

    let outcome = execute_plan(vec![item], &callbacks, &cancel, &skip, &retry).await;

    // No events should be emitted for already-terminal items.
    assert!(callbacks.events.lock().await.is_empty());
    match outcome {
        ApplyOutcome::Completed(counts) => {
            assert_eq!(counts.succeeded, 0);
        }
        other => panic!("expected Completed, got {other:?}"),
    }
}

#[tokio::test]
async fn cancellation_halts_before_next_item() {
    let dir = tempfile::tempdir().unwrap();
    let root = utf8(dir.path());
    let src1 = root.join("a.fits");
    let dst1 = root.join("a_dst.fits");
    let src2 = root.join("b.fits");
    let dst2 = root.join("b_dst.fits");
    std::fs::write(&src1, b"a").unwrap();
    std::fs::write(&src2, b"b").unwrap();

    let cancel = CancellationToken::new();
    // Pre-signal cancellation.
    cancel.cancel();

    let items =
        vec![make_move_item("item-1", &src1, &dst1), make_move_item("item-2", &src2, &dst2)];
    let callbacks = FakeCallbacks::default();
    let skip = SkipSet::new();
    let retry = RetryQueue::new();

    let outcome = execute_plan(items, &callbacks, &cancel, &skip, &retry).await;

    // No items executed (cancel was signalled before the loop started).
    match outcome {
        ApplyOutcome::Cancelled(counts) => {
            assert_eq!(counts.succeeded, 0);
        }
        other => panic!("expected Cancelled, got {other:?}"),
    }
    // Both sources still exist.
    assert!(src1.exists());
    assert!(src2.exists());
}

#[tokio::test]
async fn user_skip_set_prevents_execution() {
    let dir = tempfile::tempdir().unwrap();
    let root = utf8(dir.path());
    let src = root.join("skip.fits");
    let dst = root.join("skip_dst.fits");
    std::fs::write(&src, b"data").unwrap();

    let item = make_move_item("item-skip", &src, &dst);
    let callbacks = FakeCallbacks::default();
    let cancel = CancellationToken::new();
    let skip = SkipSet::new();
    skip.insert("item-skip");
    let retry = RetryQueue::new();

    let outcome = execute_plan(vec![item], &callbacks, &cancel, &skip, &retry).await;

    let events = callbacks.events.lock().await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].new_state, "skipped");
    drop(events);

    match outcome {
        ApplyOutcome::Completed(counts) => assert_eq!(counts.skipped, 1),
        other => panic!("expected Completed, got {other:?}"),
    }
    // Source not moved.
    assert!(src.exists());
}

#[tokio::test]
async fn stale_source_triggers_pause() {
    let dir = tempfile::tempdir().unwrap();
    let root = utf8(dir.path());
    let src = root.join("stale.fits");
    std::fs::write(&src, b"data").unwrap();
    let dst = root.join("dst.fits");

    let item = ExecutorItem {
        id: "item-stale".to_owned(),
        plan_id: "p1".to_owned(),
        action: ExecutorItemAction::Move,
        source_path: Some(src.clone()),
        destination_path: Some(dst),
        library_root: None,
        destination_root: None,
        cas_snapshot: CasSnapshot {
            approved_mtime: None,
            approved_size_bytes: Some(999), // wrong size → stale
        },
        is_protected: false,
        requires_destructive_confirm: false,
        destructive_confirmed: false,
        current_state: "pending".to_owned(),
    };

    let callbacks = FakeCallbacks::default();
    let cancel = CancellationToken::new();
    let skip = SkipSet::new();
    let retry = RetryQueue::new();

    let outcome = execute_plan(vec![item], &callbacks, &cancel, &skip, &retry).await;
    match outcome {
        ApplyOutcome::Paused { reason, .. } => {
            assert!(reason.contains("stale"));
        }
        other => panic!("expected Paused, got {other:?}"),
    }
}

/// Callbacks that, on seeing `item-1` fail, clear the conflicting
/// destination that caused the failure and files a retry — mirroring
/// what `retry_plan_item` + a user fix do in the real app (issue #742).
#[derive(Clone)]
struct RetryOnFailureCallbacks {
    events: Arc<Mutex<Vec<ItemProgressEvent>>>,
    retry_queue: RetryQueue,
    conflicting_destination: Utf8PathBuf,
}

impl ExecutorCallbacks for RetryOnFailureCallbacks {
    fn on_item_start(
        &self,
        _item_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(async {})
    }

    fn on_item_progress(
        &self,
        event: ItemProgressEvent,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        let events = self.events.clone();
        let retry_queue = self.retry_queue.clone();
        let conflicting_destination = self.conflicting_destination.clone();
        Box::pin(async move {
            if event.item_id == "item-1" && event.new_state == "failed" {
                let _ = std::fs::remove_file(&conflicting_destination);
                // The run is still live here, so the queue must accept. Asserting
                // rather than discarding keeps this fixture honest: a silently
                // refused push is exactly the failure `push`'s return value now
                // reports, and the test would otherwise pass without retrying.
                assert!(retry_queue.push("item-1"), "queue must accept while the run is live");
            }
            events.lock().await.push(event);
        })
    }
}

#[tokio::test]
#[allow(
    clippy::significant_drop_tightening,
    reason = "guard held across assertions on borrowed data"
)]
async fn mid_run_retry_reexecutes_already_passed_item() {
    let dir = tempfile::tempdir().unwrap();
    let root = utf8(dir.path());
    let src1 = root.join("a.fits");
    let dst1 = root.join("a_dst.fits");
    let src2 = root.join("b.fits");
    let dst2 = root.join("b_dst.fits");
    std::fs::write(&src1, b"a").unwrap();
    std::fs::write(&src2, b"b").unwrap();
    // item-1's destination already exists, so its first attempt fails
    // with a non-pausing conflict — exactly the class of failure a user
    // fixes and retries mid-run.
    std::fs::write(&dst1, b"stale").unwrap();

    let item1 = make_move_item("item-1", &src1, &dst1);
    let item2 = make_move_item("item-2", &src2, &dst2);

    let retry = RetryQueue::new();
    let callbacks = RetryOnFailureCallbacks {
        events: Arc::new(Mutex::new(Vec::new())),
        retry_queue: retry.clone(),
        conflicting_destination: dst1.clone(),
    };
    let cancel = CancellationToken::new();
    let skip = SkipSet::new();

    let outcome = execute_plan(vec![item1, item2], &callbacks, &cancel, &skip, &retry).await;

    match outcome {
        ApplyOutcome::Completed(counts) => {
            // item-1's original failure is still counted; its retry
            // succeeds, and item-2 succeeds normally.
            assert_eq!(counts.succeeded, 2);
            assert_eq!(counts.failed, 1);
        }
        other => panic!("expected Completed, got {other:?}"),
    }

    // item-1 actually moved once the conflicting destination was cleared
    // and the retry re-executed the real filesystem operation — not just
    // a DB-state flip with no corresponding work (the original bug).
    assert!(dst1.exists());
    assert!(!src1.exists());

    let events = callbacks.events.lock().await;
    let item1_events: Vec<_> = events.iter().filter(|e| e.item_id == "item-1").collect();
    assert_eq!(item1_events.len(), 2, "expected a failed then a succeeded event");
    assert_eq!(item1_events[0].new_state, "failed");
    assert_eq!(item1_events[1].new_state, "succeeded");
    // The retry's prior_state reflects the DB row `retry_plan_item`
    // already transitioned to `applying` — not the original "pending".
    assert_eq!(item1_events[1].prior_state, "applying");
}

/// Callbacks that, on seeing `item-1` succeed, queue a retry for
/// `item-2` and signal cancellation in the same tick — mirroring a user
/// clicking Cancel right as a mid-run retry is filed (review fix for
/// #742's retry-drain loop).
#[derive(Clone)]
struct CancelDuringRetryDrainCallbacks {
    events: Arc<Mutex<Vec<ItemProgressEvent>>>,
    retry_queue: RetryQueue,
    cancel: CancellationToken,
}

impl ExecutorCallbacks for CancelDuringRetryDrainCallbacks {
    fn on_item_start(
        &self,
        _item_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(async {})
    }

    fn on_item_progress(
        &self,
        event: ItemProgressEvent,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        let events = self.events.clone();
        let retry_queue = self.retry_queue.clone();
        let cancel = self.cancel.clone();
        Box::pin(async move {
            if event.item_id == "item-1" && event.new_state == "succeeded" {
                // Pushed BEFORE cancelling, so the queue is still open and must
                // accept. Ordering matters: after `cancel()` the queue closes and
                // this push would legitimately return false.
                assert!(retry_queue.push("item-2"), "queue must accept before cancellation");
                cancel.cancel();
            }
            events.lock().await.push(event);
        })
    }
}

#[tokio::test]
#[allow(
    clippy::significant_drop_tightening,
    reason = "guard held across assertions on borrowed data"
)]
async fn cancellation_is_observed_between_retry_items_not_just_forward_items() {
    let dir = tempfile::tempdir().unwrap();
    let root = utf8(dir.path());
    let src1 = root.join("a.fits");
    let dst1 = root.join("a_dst.fits");
    let src2 = root.join("b.fits");
    let dst2 = root.join("b_dst.fits");
    std::fs::write(&src1, b"a").unwrap();
    std::fs::write(&src2, b"b").unwrap();

    let item1 = make_move_item("item-1", &src1, &dst1);
    // item-2 carries `current_state: "failed"` — mirrors a pre-pause
    // failed item a resumed run now carries forward purely for
    // `item_by_id` lookup purposes (review fix for resume/retry item-set
    // agreement); the forward loop skips it as already-terminal, so the
    // ONLY path that could execute it is the retry-drain below.
    let item2 = ExecutorItem {
        current_state: "failed".to_owned(),
        ..make_move_item("item-2", &src2, &dst2)
    };

    let cancel = CancellationToken::new();
    let retry = RetryQueue::new();
    let callbacks = CancelDuringRetryDrainCallbacks {
        events: Arc::new(Mutex::new(Vec::new())),
        retry_queue: retry.clone(),
        cancel: cancel.clone(),
    };
    let skip = SkipSet::new();

    let outcome = execute_plan(vec![item1, item2], &callbacks, &cancel, &skip, &retry).await;

    match outcome {
        ApplyOutcome::Cancelled(counts) => {
            assert_eq!(counts.succeeded, 1, "only item-1's normal forward pass");
        }
        other => panic!("expected Cancelled, got {other:?}"),
    }

    // item-2's queued retry must NOT have executed: cancellation is
    // checked between retry items too, same as forward items.
    assert!(src2.exists(), "item-2's retry must not have run after cancel");
    assert!(!dst2.exists());
    let events = callbacks.events.lock().await;
    assert!(
        events.iter().all(|e| e.item_id != "item-2"),
        "no progress event should have been emitted for the cancelled-out retry"
    );
}

#[test]
fn terminal_state_all_succeeded() {
    let c = TerminalCounts { succeeded: 5, failed: 0, skipped: 0, cancelled: 0 };
    assert_eq!(c.terminal_state(false), "applied");
}

#[test]
fn terminal_state_partial() {
    let c = TerminalCounts { succeeded: 3, failed: 2, skipped: 0, cancelled: 0 };
    assert_eq!(c.terminal_state(false), "partially_applied");
}

#[test]
fn terminal_state_all_failed() {
    let c = TerminalCounts { succeeded: 0, failed: 3, skipped: 0, cancelled: 0 };
    assert_eq!(c.terminal_state(false), "failed");
}

#[test]
fn terminal_state_cancelled_overrides() {
    let c = TerminalCounts { succeeded: 3, failed: 0, skipped: 0, cancelled: 2 };
    assert_eq!(c.terminal_state(true), "cancelled");
}

#[test]
fn cancellation_token_default_not_cancelled() {
    let tok = CancellationToken::new();
    assert!(!tok.is_cancelled());
    tok.cancel();
    assert!(tok.is_cancelled());
}

// ── retry-queue lifecycle (astro-plan-ts1z) ───────────────────────────────

#[test]
fn a_closed_queue_refuses_pushes() {
    let q = RetryQueue::new();
    assert!(q.push("item-1"), "an open queue accepts");
    assert_eq!(q.drain_all(), vec!["item-1".to_owned()]);

    assert!(q.close_if_empty(), "an empty queue closes");
    assert!(
        !q.push("item-2"),
        "a closed queue must REFUSE, not silently accept an id nothing will drain — \
         a silently accepted retry is swept to 'cancelled' without executing"
    );
    assert!(q.drain_all().is_empty(), "the refused id must not be queued");
    assert!(q.take_orphaned().is_empty(), "a refused push is not an orphan; it never became one");
}

#[test]
fn close_if_empty_refuses_to_close_over_a_queued_retry() {
    let q = RetryQueue::new();
    assert!(q.push("item-1"));
    assert!(
        !q.close_if_empty(),
        "closing over a queued retry would drop it; the caller must drain and re-try the close"
    );
    assert!(q.push("item-2"), "the queue is still open, so pushes still land");
}

#[test]
fn unconditional_close_reports_undrained_ids_as_orphans() {
    let q = RetryQueue::new();
    assert!(q.push("item-1"));
    q.close();

    assert!(!q.push("item-2"), "close is a one-way door");
    assert_eq!(
        q.take_orphaned(),
        vec!["item-1".to_owned()],
        "an accepted-but-unexecuted retry must be reported so its DB row can be restored"
    );
    assert!(q.take_orphaned().is_empty(), "take_orphaned drains, so two callers cannot both act");
}

/// Callbacks that file a retry for `chained` while `first` is being
/// re-executed from the retry queue.
///
/// `drain_retries` works from a snapshot of the queue, so an id pushed during a
/// re-execution is NOT in the batch being drained. It can only run if something
/// drains again after the forward loop is done — which is the joint
/// close-with-final-drain.
#[derive(Clone)]
struct ChainedRetryCallbacks {
    events: Arc<Mutex<Vec<ItemProgressEvent>>>,
    retry_queue: RetryQueue,
    /// Filed when the forward pass finishes this item.
    first: String,
    /// Filed while `first` is being re-executed from the queue.
    chained: String,
    accepted: Arc<Mutex<Vec<(String, bool)>>>,
}

impl ExecutorCallbacks for ChainedRetryCallbacks {
    fn on_item_start(
        &self,
        _item_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(async {})
    }

    fn on_item_progress(
        &self,
        event: ItemProgressEvent,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        let events = self.events.clone();
        let retry_queue = self.retry_queue.clone();
        let first = self.first.clone();
        let chained = self.chained.clone();
        let accepted = self.accepted.clone();
        Box::pin(async move {
            if event.new_state == "succeeded" {
                // The forward pass of the last plain item files the first retry.
                if event.item_id == "item-forward" {
                    let ok = retry_queue.push(&first);
                    accepted.lock().await.push((first.clone(), ok));
                }
                // `first` re-executing means we are inside a drain batch that
                // was already snapshotted, so this push lands outside it.
                else if event.item_id == first && event.prior_state == "applying" {
                    let ok = retry_queue.push(&chained);
                    accepted.lock().await.push((chained.clone(), ok));
                }
            }
            events.lock().await.push(event);
        })
    }
}

/// astro-plan-ts1z windows (b) and (c): a retry the queue ACCEPTED must be
/// executed by the run that accepted it. There must be no interval in which
/// acceptance succeeds and nothing drains the id.
///
/// `item-chained` is pushed while `item-first` is being re-executed, i.e. after
/// `drain_retries` took its batch snapshot and after the forward loop's last
/// drain point. Before the joint close-with-final-drain, that id sat in a queue
/// nobody read again: the run completed, and the caller's completion sweep
/// converted the item's `applying` row to `cancelled` without it ever running.
///
/// Both retry targets carry `current_state: "failed"` — a pre-pause failure a
/// resumed run holds only for `item_by_id` lookup — so the forward loop skips
/// them as terminal and the retry drain is the only path that can execute them.
#[tokio::test]
async fn a_retry_accepted_after_the_last_drain_snapshot_is_still_executed() {
    let dir = tempfile::tempdir().unwrap();
    let root = utf8(dir.path());
    let src_fwd = root.join("forward.fits");
    let dst_fwd = root.join("forward_dst.fits");
    let src_first = root.join("first.fits");
    let dst_first = root.join("first_dst.fits");
    let src_chained = root.join("chained.fits");
    let dst_chained = root.join("chained_dst.fits");
    for p in [&src_fwd, &src_first, &src_chained] {
        std::fs::write(p, b"x").unwrap();
    }

    let forward = make_move_item("item-forward", &src_fwd, &dst_fwd);
    let first = ExecutorItem {
        current_state: "failed".to_owned(),
        ..make_move_item("item-first", &src_first, &dst_first)
    };
    let chained = ExecutorItem {
        current_state: "failed".to_owned(),
        ..make_move_item("item-chained", &src_chained, &dst_chained)
    };

    let retry = RetryQueue::new();
    let callbacks = ChainedRetryCallbacks {
        events: Arc::new(Mutex::new(Vec::new())),
        retry_queue: retry.clone(),
        first: "item-first".to_owned(),
        chained: "item-chained".to_owned(),
        accepted: Arc::new(Mutex::new(Vec::new())),
    };
    let cancel = CancellationToken::new();
    let skip = SkipSet::new();

    let outcome =
        execute_plan(vec![forward, first, chained], &callbacks, &cancel, &skip, &retry).await;

    assert_eq!(
        *callbacks.accepted.lock().await,
        vec![("item-first".to_owned(), true), ("item-chained".to_owned(), true)],
        "both retries were ACCEPTED, so both must be executed — an accepted retry that never \
         runs is swept to 'cancelled' and reads as a cancellation the user asked for"
    );
    match outcome {
        ApplyOutcome::Completed(counts) => assert_eq!(
            counts.succeeded, 3,
            "the forward item plus BOTH accepted retries; a chained retry accepted outside the \
             drain snapshot must not be dropped"
        ),
        other => panic!("expected Completed, got {other:?}"),
    }
    assert!(!src_chained.exists(), "the chained retry's source really moved");
    assert!(dst_chained.exists(), "the chained retry's destination really exists");
    assert!(
        retry.take_orphaned().is_empty(),
        "an executed retry is not an orphan; nothing needs restoring"
    );
    assert!(
        !retry.push("item-first"),
        "execute_plan must leave the queue CLOSED, so a later retry is refused rather than \
         accepted onto a queue nobody will read"
    );
}

/// The halting paths (cancel, pause) must leave an accepted-but-unexecuted
/// retry visible as an orphan rather than dropping it: its DB row is already
/// `applying`, and only the caller can restore it.
#[tokio::test]
async fn a_retry_accepted_but_not_executed_before_a_cancel_becomes_an_orphan() {
    let dir = tempfile::tempdir().unwrap();
    let root = utf8(dir.path());
    let src1 = root.join("a.fits");
    let dst1 = root.join("a_dst.fits");
    std::fs::write(&src1, b"a").unwrap();

    let retry = RetryQueue::new();
    assert!(retry.push("item-never-run"), "the retry is accepted before the run halts");

    let cancel = CancellationToken::new();
    cancel.cancel();
    let callbacks = FakeCallbacks::default();
    let skip = SkipSet::new();

    let outcome = execute_plan(
        vec![make_move_item("item-1", &src1, &dst1)],
        &callbacks,
        &cancel,
        &skip,
        &retry,
    )
    .await;

    assert!(matches!(outcome, ApplyOutcome::Cancelled(_)), "got {outcome:?}");
    assert_eq!(
        retry.take_orphaned(),
        vec!["item-never-run".to_owned()],
        "the caller must be able to see the retry it accepted but never ran, or the item is \
         swept to 'cancelled' with no record that a retry was owed"
    );
    assert!(!retry.push("late"), "a halted run's queue is closed");
}
