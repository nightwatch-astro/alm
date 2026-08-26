// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Real-process liveness for the re-launch guard (astro-plan-7wu52).
//!
//! Every assertion here is made against a process the OS genuinely has or
//! genuinely does not have. A test that only asserted `pid_is_alive` returns
//! `false` would have passed against the hardcoded-`false` implementation this
//! file exists to prevent, so the alive direction is what carries the proof.

use workflow_profiles::launch::{bundle_is_running, pid_is_alive, prior_launch_is_alive};

/// The test process itself is unarguably alive on every platform.
///
/// This is the assertion the previous `#[cfg(target_os = "linux")]`
/// implementation failed on macOS and Windows.
#[test]
fn the_running_test_process_is_alive() {
    assert!(pid_is_alive(std::process::id()), "the process making this call is running");
}

/// A child that has exited and been reaped is gone.
#[test]
fn a_reaped_child_is_not_alive() {
    let dir = tempfile::tempdir().expect("temp dir");
    let record = dir.path().join("record.txt");
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_spawn-stub"))
        .arg(&record)
        .spawn()
        .expect("spawn stub");
    let pid = child.id();
    child.wait().expect("reap stub");
    assert!(!pid_is_alive(pid), "a reaped child leaves no live process");
}

/// An unreaped child has exited and must read as dead, not as a running tool.
///
/// Unix-only because a zombie is a Unix process state; Windows has no
/// equivalent, so there is nothing to assert there.
#[cfg(unix)]
#[test]
fn an_unreaped_child_is_not_alive() {
    let dir = tempfile::tempdir().expect("temp dir");
    let record = dir.path().join("record.txt");
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_spawn-stub"))
        .arg(&record)
        .spawn()
        .expect("spawn stub");
    let pid = child.id();

    // Wait for the stub to publish its record, which it does immediately before
    // exiting, then leave the child unreaped.
    for _ in 0..200 {
        if record.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(record.exists(), "stub published its record");
    std::thread::sleep(std::time::Duration::from_millis(50));

    assert!(!pid_is_alive(pid), "an exited-but-unreaped child is not a running tool");
    child.wait().expect("reap stub");
}

/// A pid far above the OS maximum can never belong to a process.
#[test]
fn an_impossible_pid_is_not_alive() {
    assert!(!pid_is_alive(u32::MAX - 1));
}

/// A recorded pid decides the answer, so a stale bundle id cannot override it.
#[test]
fn a_live_pid_wins_over_the_bundle_id_fallback() {
    assert!(prior_launch_is_alive(Some(std::process::id()), None));
    assert!(prior_launch_is_alive(Some(std::process::id()), Some("com.example.absent")));
}

/// A launch that recorded neither identity cannot be shown alive.
#[test]
fn no_recorded_identity_is_not_alive() {
    assert!(!prior_launch_is_alive(None, None));
}

/// An application that is not installed is not running.
///
/// Passes off macOS for a different reason — `lsappinfo` does not exist there —
/// which is the intended answer on a platform without bundle identifiers.
#[test]
fn an_unknown_bundle_id_is_not_running() {
    assert!(!bundle_is_running("com.example.definitely-not-installed"));
}

/// An application started the way production starts one — `open -b <bundle_id>`,
/// the arm that records no pid — is reported running, and stops being reported
/// once it quits.
///
/// `TextEdit` ships with every macOS install. When the developer already has it
/// open the round trip would close their editor, so that case asserts only the
/// positive direction and leaves the application alone.
///
/// Requires a GUI login session, so this runs on a developer machine and on the
/// dispatch-only macOS CI chain, never on the Linux runner that gates pull
/// requests.
#[cfg(target_os = "macos")]
#[test]
fn an_application_launched_by_bundle_id_is_reported_running() {
    const BUNDLE_ID: &str = "com.apple.TextEdit";

    fn wait_for(want: bool) -> bool {
        for _ in 0..100 {
            if bundle_is_running(BUNDLE_ID) == want {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        false
    }

    if bundle_is_running(BUNDLE_ID) {
        assert!(prior_launch_is_alive(None, Some(BUNDLE_ID)), "an open TextEdit reads as alive");
        return;
    }

    let opened = std::process::Command::new("/usr/bin/open")
        .args(["-g", "-b", BUNDLE_ID])
        .status()
        .expect("run open");
    assert!(opened.success(), "open -b {BUNDLE_ID} succeeded");

    let running = wait_for(true);

    let quit = std::process::Command::new("/usr/bin/osascript")
        .args(["-e", &format!(r#"tell application id "{BUNDLE_ID}" to quit"#)])
        .status();

    assert!(running, "Launch Services reports TextEdit while it is open");
    assert!(quit.is_ok_and(|s| s.success()), "TextEdit quit cleanly");
    assert!(wait_for(false), "Launch Services stops reporting TextEdit once it quits");
}

/// The guard reaches the bundle-id fallback when the launch recorded no pid,
/// which is every `open -b` launch on macOS.
#[cfg(target_os = "macos")]
#[test]
fn a_pidless_launch_falls_back_to_the_bundle_id() {
    // Finder is the one application that is always running in a GUI session.
    assert_eq!(
        prior_launch_is_alive(None, Some("com.apple.finder")),
        bundle_is_running("com.apple.finder"),
        "the pid-less path delegates to the bundle-id query"
    );
    assert!(bundle_is_running("com.apple.finder"), "Finder runs in every GUI session");
}
