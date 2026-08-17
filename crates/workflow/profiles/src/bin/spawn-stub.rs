// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! `spawn-stub` — child process used by the real-spawn integration test
//! (spec 011 T022). Not part of the shipped application.
//!
//! Spawns are detached, so the parent cannot read the child's state directly:
//! the stub instead records what the OS actually handed it — its working
//! directory and its full argv — into the file named by `argv[1]`.
//!
//! The record is written to a `.partial` sibling and renamed into place so a
//! polling reader never observes a half-written record.

fn main() {
    let mut args = std::env::args();
    args.next(); // argv[0]: this executable's own path
    let record = args.next().expect("spawn-stub requires a record path as argv[1]");
    let cwd = std::env::current_dir().expect("child has no readable working directory");

    let mut out = format!("cwd\t{}\n", cwd.display());
    for arg in args {
        out += "arg\t";
        out += &arg;
        out += "\n";
    }

    let partial = format!("{record}.partial");
    std::fs::write(&partial, out).expect("write partial record");
    std::fs::rename(&partial, &record).expect("publish record");
}
