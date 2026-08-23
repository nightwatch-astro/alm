// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Real-spawn integration tests (spec 011 T022).
//!
//! `FakeSpawner` only proves which `SpawnRequest` the launch path *intended*;
//! it cannot prove the OS actually started a process in the requested working
//! directory with the requested argv. These tests spawn the compiled
//! `spawn-stub` binary through [`RealSpawner`] and read back what the child
//! itself observed.
//!
//! Coverage boundary: `bundle_id` is `None` in every case, so the direct-exec
//! path is exercised on all three platforms. macOS `open -b <bundle_id>`
//! dispatch is deliberately not covered — it needs an installed `.app` bundle
//! and Launch Services ignores the caller's `current_dir`, so there is no
//! portable cwd/argv assertion to make against it.

use std::path::Path;
use std::time::{Duration, Instant};

use project_structure::resolve_working_folder;
use workflow_profiles::args::{render, RenderContext};
use workflow_profiles::launch::{ProcessSpawner, RealSpawner, SpawnRequest};
use workflow_profiles::{seed, ArgsToken};

/// What the child process reported about its own environment.
struct Observed {
    cwd: String,
    args: Vec<String>,
}

/// Spawn `spawn-stub` detached with `tool_args`, anchored at `working_dir`,
/// and wait for the record it writes.
fn spawn_and_observe(working_dir: &Path, tool_args: &[String], record: &Path) -> Observed {
    let mut args = vec![record.to_string_lossy().into_owned()];
    args.extend(tool_args.iter().cloned());

    RealSpawner
        .spawn(SpawnRequest {
            executable: env!("CARGO_BIN_EXE_spawn-stub").to_owned(),
            args,
            working_dir: working_dir.to_string_lossy().into_owned(),
            bundle_id: None,
        })
        .expect("real spawn of the stub binary");

    read_record(record)
}

/// Poll for the stub's record file. The stub is detached, so there is no child
/// handle to wait on.
fn read_record(record: &Path) -> Observed {
    let deadline = Instant::now() + Duration::from_secs(10);
    let raw = loop {
        if let Ok(text) = std::fs::read_to_string(record) {
            break text;
        }
        assert!(Instant::now() < deadline, "stub never wrote {}", record.display());
        std::thread::sleep(Duration::from_millis(20));
    };

    let mut cwd = None;
    let mut args = Vec::new();
    for line in raw.lines() {
        let (key, value) = line.split_once('\t').expect("record line is key<TAB>value");
        match key {
            "cwd" => cwd = Some(value.to_owned()),
            "arg" => args.push(value.to_owned()),
            other => panic!("unexpected record key {other}"),
        }
    }
    Observed { cwd: cwd.expect("record carries a cwd line"), args }
}

/// The child's reported cwd and the expected path may differ by symlink
/// resolution (`/tmp` → `/private/tmp` on macOS), so compare canonical forms.
fn assert_same_dir(observed: &str, expected: &Path) {
    let lhs = Path::new(observed).canonicalize().expect("canonicalize observed cwd");
    let rhs = expected.canonicalize().expect("canonicalize expected cwd");
    assert_eq!(lhs, rhs, "child cwd {observed} != {}", expected.display());
}

#[test]
fn real_spawn_anchors_cwd_to_project_root_when_no_source_view() {
    let tmp = tempfile::tempdir().unwrap();
    let project_root = tmp.path().join("my_project");
    std::fs::create_dir_all(&project_root).unwrap();

    // PixInsight: supports_open_folder = false, so argv carries no folder and
    // the project folder reaches the tool only as its cwd (R-CwdContain).
    let profile = seed::find("pixinsight").unwrap();
    assert!(!profile.supports_open_folder);
    let working_dir = resolve_working_folder(&project_root, None).unwrap();
    let rendered = render(&profile.args_template, &RenderContext::default()).unwrap();

    let observed = spawn_and_observe(&working_dir, &rendered, &tmp.path().join("pi.record"));

    assert_same_dir(&observed.cwd, &project_root);
    assert!(observed.args.is_empty(), "unexpected tool argv: {:?}", observed.args);
}

#[test]
fn real_spawn_anchors_cwd_and_argv_to_source_view_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let project_root = tmp.path().join("my_project");
    let source_view = project_root.join("_source_view");
    std::fs::create_dir_all(&source_view).unwrap();

    // Siril: supports_open_folder = true, so the resolved source-view folder
    // appears both as the cwd and as the rendered `{folder}` argument.
    let profile = seed::find("siril").unwrap();
    assert_eq!(profile.args_template, vec![ArgsToken::Folder]);
    let working_dir = resolve_working_folder(&project_root, Some("_source_view")).unwrap();
    let folder = working_dir.to_string_lossy().into_owned();
    let rendered =
        render(&profile.args_template, &RenderContext { folder: Some(&folder), file: None })
            .unwrap();

    let observed = spawn_and_observe(&working_dir, &rendered, &tmp.path().join("siril.record"));

    assert_same_dir(&observed.cwd, &source_view);
    assert_eq!(observed.args, vec![folder]);
}
