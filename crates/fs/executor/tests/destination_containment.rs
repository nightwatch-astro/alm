// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Destination containment (astro-plan-3v3r.1.12).
//!
//! The run loop gated a destination against `destination_root` while dispatch
//! joined it onto `destination_root.or(library_root)`. An item carrying only a
//! `library_root` therefore reached `move`, `mkdir`, and `write_manifest` with a
//! destination nobody had checked: a `../` or absolute value mutated outside
//! every library root and the item was recorded `succeeded`.
//!
//! Each test asserts on the filesystem outside the root, not only on the event,
//! because the defect's signature was a mutation that happened and was reported
//! as a contained, reviewed action.

use camino::Utf8PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;

use fs_executor::ops::cas_check::CasSnapshot;
use fs_executor::run::{
    execute_plan, CancellationToken, ExecutorCallbacks, ExecutorItem, ExecutorItemAction,
    ItemProgressEvent, RetryQueue, SkipSet,
};

fn utf8(p: &std::path::Path) -> Utf8PathBuf {
    Utf8PathBuf::from_path_buf(p.to_path_buf()).expect("temp dir path is UTF-8")
}

#[derive(Default, Clone)]
struct RecordingCallbacks {
    events: Arc<Mutex<Vec<ItemProgressEvent>>>,
}

impl ExecutorCallbacks for RecordingCallbacks {
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

/// An item with a `library_root` and no `destination_root`, which is the shape
/// the run loop left ungated.
fn item_with_library_root_only(
    id: &str,
    action: ExecutorItemAction,
    source_path: Option<Utf8PathBuf>,
    destination_path: Option<Utf8PathBuf>,
    library_root: Utf8PathBuf,
) -> ExecutorItem {
    ExecutorItem {
        id: id.to_owned(),
        plan_id: "plan-containment".to_owned(),
        action,
        source_path,
        destination_path,
        library_root: Some(library_root),
        destination_root: None,
        cas_snapshot: CasSnapshot { approved_mtime: None, approved_size_bytes: None },
        is_protected: false,
        requires_destructive_confirm: false,
        destructive_confirmed: false,
        current_state: "pending".to_owned(),
    }
}

async fn apply(item: ExecutorItem) -> Vec<ItemProgressEvent> {
    let cb = RecordingCallbacks::default();
    execute_plan(vec![item], &cb, &CancellationToken::new(), &SkipSet::new(), &RetryQueue::new())
        .await;
    let events = cb.events.lock().await.clone();
    events
}

fn assert_refused(events: &[ItemProgressEvent]) {
    let last = events.last().expect("at least one event");
    assert_eq!(
        last.new_state, "refused",
        "uncontained destination must be refused, got state '{}'",
        last.new_state
    );
}

#[tokio::test]
async fn a_traversing_move_destination_never_escapes_its_root() {
    let root_dir = tempfile::tempdir().unwrap();
    let outside_dir = tempfile::tempdir().unwrap();
    let root = utf8(root_dir.path());
    std::fs::write(root_dir.path().join("frame.fits"), b"data").unwrap();

    // `../<outside-dir-name>/stolen.fits` relative to the root, so the escape
    // target is a real directory this test owns rather than a path under `/`.
    let escape = Utf8PathBuf::from(format!(
        "../{}/stolen.fits",
        outside_dir.path().file_name().unwrap().to_str().unwrap()
    ));

    let events = apply(item_with_library_root_only(
        "move-escape",
        ExecutorItemAction::Move,
        Some(Utf8PathBuf::from("frame.fits")),
        Some(escape),
        root,
    ))
    .await;

    assert_refused(&events);
    assert!(
        !outside_dir.path().join("stolen.fits").exists(),
        "move wrote outside every library root"
    );
    assert!(root_dir.path().join("frame.fits").exists(), "source must be left in place");
}

#[tokio::test]
async fn an_absolute_move_destination_never_replaces_its_root() {
    let root_dir = tempfile::tempdir().unwrap();
    let outside_dir = tempfile::tempdir().unwrap();
    let root = utf8(root_dir.path());
    std::fs::write(root_dir.path().join("frame.fits"), b"data").unwrap();

    let absolute_outside = utf8(&outside_dir.path().join("stolen.fits"));

    let events = apply(item_with_library_root_only(
        "move-absolute",
        ExecutorItemAction::Move,
        Some(Utf8PathBuf::from("frame.fits")),
        Some(absolute_outside),
        root,
    ))
    .await;

    assert_refused(&events);
    assert!(
        !outside_dir.path().join("stolen.fits").exists(),
        "an absolute destination replaced the root"
    );
}

#[tokio::test]
async fn a_traversing_mkdir_destination_never_escapes_its_root() {
    let root_dir = tempfile::tempdir().unwrap();
    let outside_dir = tempfile::tempdir().unwrap();
    let root = utf8(root_dir.path());

    let escape = Utf8PathBuf::from(format!(
        "../{}/created",
        outside_dir.path().file_name().unwrap().to_str().unwrap()
    ));

    let events = apply(item_with_library_root_only(
        "mkdir-escape",
        ExecutorItemAction::Mkdir,
        None,
        Some(escape),
        root,
    ))
    .await;

    assert_refused(&events);
    assert!(!outside_dir.path().join("created").exists(), "mkdir created a directory outside");
}

#[tokio::test]
async fn a_traversing_manifest_destination_never_escapes_its_root() {
    let root_dir = tempfile::tempdir().unwrap();
    let outside_dir = tempfile::tempdir().unwrap();
    let root = utf8(root_dir.path());

    let escape = Utf8PathBuf::from(format!(
        "../{}/marker.json",
        outside_dir.path().file_name().unwrap().to_str().unwrap()
    ));

    let events = apply(item_with_library_root_only(
        "manifest-escape",
        ExecutorItemAction::WriteManifest { project_id: "proj-1".to_owned() },
        None,
        Some(escape),
        root,
    ))
    .await;

    assert_refused(&events);
    assert!(!outside_dir.path().join("marker.json").exists(), "manifest written outside");
}

/// A rootless relative destination resolved against the process working
/// directory, which is the repository checkout under test.
#[tokio::test]
async fn a_rootless_relative_destination_is_refused_rather_than_cwd_relative() {
    let root_dir = tempfile::tempdir().unwrap();
    std::fs::write(root_dir.path().join("frame.fits"), b"data").unwrap();

    let item = ExecutorItem {
        id: "rootless".to_owned(),
        plan_id: "plan-containment".to_owned(),
        action: ExecutorItemAction::Mkdir,
        source_path: None,
        destination_path: Some(Utf8PathBuf::from("cwd-relative-dir")),
        library_root: None,
        destination_root: None,
        cas_snapshot: CasSnapshot { approved_mtime: None, approved_size_bytes: None },
        is_protected: false,
        requires_destructive_confirm: false,
        destructive_confirmed: false,
        current_state: "pending".to_owned(),
    };

    let events = apply(item).await;

    assert_refused(&events);
    assert!(
        !std::path::Path::new("cwd-relative-dir").exists(),
        "a rootless relative destination resolved against the process cwd"
    );
}

/// A destination whose parent does not exist yet is the normal case for a write:
/// containment is lexical, so it must resolve rather than be refused for absence.
#[tokio::test]
async fn a_destination_whose_leaf_does_not_exist_yet_is_contained() {
    let root_dir = tempfile::tempdir().unwrap();
    let root = utf8(root_dir.path());

    let events = apply(item_with_library_root_only(
        "new-leaf",
        ExecutorItemAction::Mkdir,
        None,
        Some(Utf8PathBuf::from("brand/new/leaf")),
        root,
    ))
    .await;

    let last = events.last().expect("at least one event");
    assert_eq!(last.new_state, "succeeded", "a non-existent leaf must not be refused");
    assert!(root_dir.path().join("brand/new/leaf").is_dir());
}
