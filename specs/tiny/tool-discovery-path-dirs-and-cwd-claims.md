# TinySpec: Discovery searches only absolute directories, and the product claims a working directory only where one is applied

**Branch**: fix/path3-tool-discovery-cwd
**Date**: 2026-08-23
**Status**: implemented
**Complexity**: small
**Findings**: astro-plan-3v3r.13.39 (LIVE), astro-plan-3v3r.13.42 (LIVE),
astro-plan-3v3r.13.41 (already remediated at head)

## What

Two independent defects in `crates/workflow/profiles`, joined only by the working
directory.

Auto-discovery on Linux searches `PATH` by splitting on `:` and joining each
element with the program name. POSIX defines an empty element as the current
working directory, so a leading, trailing or doubled colon in the user's `PATH`
yields a candidate that `Path::exists` resolves against whatever directory the
app was started from. The module doc at `crates/workflow/profiles/src/discover.rs:19`
states discovery never returns a relative path.

On macOS a tool with a bundle id launches through `/usr/bin/open`, which cannot
carry a working directory — Launch Services ignores the caller's `current_dir`
(recorded at `crates/workflow/profiles/tests/real_spawn_stub.rs:12-16`). Two of
the three shipped seed profiles set a bundle id (`seed.rs:20`, `seed.rs:42`) and
take that arm; `planetary_suite` (`seed.rs:59`) does not, and takes the
plain-binary arm, which does apply the project working directory
(`launch.rs:187`). The product nevertheless tells the user the working directory
is anchored to the project whenever a tool declares
`supports_open_folder = false`, in a one-time hint and in a source comment, and
that is false for a tool launched through its bundle id.

## The rule (decision)

### Discovery search directories are absolute, and the caller supplies the variable

Every search of a `PATH`-style variable goes through one helper that takes the
variable's value as a parameter and yields only directories for which
`Path::is_absolute` holds. An empty element parses to an empty path, which
`is_absolute` rejects, so the same filter covers empty and relative elements.
The helper is compiled wherever a test can reach it, so its tests run on every
CI lane rather than only the Linux one, and it uses `std::env::split_paths` so
the separator and quoting rules are the host's.

No caller of the helper resolves a candidate relative to the process working
directory.

### The product states only what holds on the platform it is running on

The one-time launch hint for a tool that takes no folder argument says that and
nothing more. It does not assert that a working directory is anchored to the
project, because the hint is gated on `supports_open_folder` alone while the
working directory is applied per platform arm, so the assertion is false for a
tool launched on macOS through its bundle id.

The macOS bundle arm carries a comment recording that it applies no working
directory and why one cannot be applied.

### No `current_dir` is added to the `open` command

`Command::current_dir` on `/usr/bin/open` changes the working directory of
`open` itself, not of the application Launch Services starts. Adding one would
make the code look correct without changing what the launched tool sees.

## Per-finding verdict at head `5b1fdf702f7ad12d52ea429a255cc69874ef5a2b`

| Finding | Verified at | Reachable with real inputs | Verdict |
| --- | --- | --- | --- |
| `astro-plan-3v3r.13.39` | `discover.rs:208-209` (`resolve_executable`), `discover.rs:360-364` (`discover_from_path`) | Yes, on Linux. A distro `.desktop` `Exec=siril` reaches `resolve_executable` (`discover.rs:186`); a malformed user `PATH` then produces a relative candidate. | Fix. Impact is narrower than the finding states: `update_tool` rejects a non-absolute `executable_path` (`crates/app/core/src/tool_launch.rs:564-569`) and is the only writer of that setting, so the observable effect is auto-detect pre-filling a path that the save then refuses, not a launch of an unintended binary. |
| `astro-plan-3v3r.13.42` | `launch.rs:159-176` (bundle arm, no `current_dir`), `seed.rs:25-27` (comment), `apps/desktop/messages/en-GB.json:1570` and `pt-BR.json:1511`, `apps/desktop/src/features/projects/tool-launch.ts:213-220` (hint gated on `supportsOpenFolder === false` alone) | Yes, on macOS with default settings. Two of the three seed profiles set a bundle id (`seed.rs:20`, `seed.rs:42`), defaulted at `crates/app/core/src/tool_launch.rs:499-502`; the arm is selected by the presence of a bundle id, not by `detach_strategy`. `planetary_suite` (`seed.rs:59`) sets none and does receive the project working directory (`launch.rs:187`), yet triggers the same hint, which is gated on `supportsOpenFolder === false` alone. | Correct the claim. The working directory cannot be delivered through `open`, so this is a copy-and-comment remediation with no behaviour change. |
| `astro-plan-3v3r.13.41` | `launch.rs:245-253` delegates to `fs_pathsafe::contain::contained_in_any` (`crates/fs/pathsafe/src/contain.rs:153`), which is `false` for an empty root slice; regression test at `contain.rs:303`; refusal stated at `launch.rs:238-240`. The `tests/proptest_cwd_containment.rs` the finding cites does not exist. | No | Already remediated at head. No change, no test — a test would pass at base. |

## Context

- `crates/workflow/profiles/src/discover.rs` — the two `PATH` search sites.
- `crates/workflow/profiles/src/launch.rs` — per-platform spawn arms.
- `crates/workflow/profiles/src/seed.rs` — the three shipped profiles.
- `apps/desktop/src/features/projects/tool-launch.ts`, `apps/desktop/messages/*.json` — the hint.
- `crates/workflow/profiles/tests/spawn_program_paths.rs` — the crate's existing
  source-text assertions for `cfg`-gated arms that no test can execute.

Constitution III makes tool launches detached and unsupervised: no test may
spawn a real `.app` bundle, so the macOS bundle arm is verified by reading and by
source-text assertion, not by execution.

## Requirements

1. A helper in `discover.rs` takes a `PATH`-style value as a parameter and yields
   only absolute directories.
2. The helper drops empty elements, relative elements, and both leading and
   trailing separators, on the host separator.
3. `resolve_executable` returns `None` rather than a relative path when every
   `PATH` element is relative or empty.
4. `discover_from_path` yields no result derived from a relative or empty `PATH`
   element.
5. The helper and its tests compile on macOS, Linux and Windows — it is gated
   `#[cfg(any(target_os = "linux", test))]`, not `cfg(unix)`, because
   `Path::is_absolute` is platform-dependent and the fixtures are built through
   `fs_pathsafe::test_support` and `std::env::join_paths`.
6. No test of requirements 1-4 depends on any tool being installed, and none
   mutates the process environment.
7. The `projects_tool_cwd_anchored_hint` string in every message catalogue states
   that the tool accepts no folder to open and makes no working-directory claim.
8. The comment at `seed.rs:25-27` no longer asserts that `cwd` is anchored for
   PixInsight.
9. The macOS bundle arm documents that it applies no working directory.
10. The two docstrings in `apps/desktop/src/features/projects/tool-launch.ts`
    that describe the hint state that the tool receives no folder argument and
    that the project folder reaches it as a working directory only on the
    platform arms that apply one.

## Tasks

- [x] Add `absolute_path_dirs` to `discover.rs`, compiled on all platforms
- [x] Route `resolve_executable` and `discover_from_path` through it
- [x] Unit tests: absolute-only fixture, malformed fixture with leading,
      trailing and doubled separators, all-relative fixture
- [x] Reword `projects_tool_cwd_anchored_hint` in `en-GB.json` and `pt-BR.json`
- [x] Correct the `seed.rs` comment; document the bundle arm at `launch.rs:159`
- [x] Reword the two `tool-launch.ts` docstrings that repeat the same claim
- [x] `cargo clippy -p workflow_profiles --all-targets`, `cargo nextest run -p
      workflow_profiles`, `cargo fmt --all --check`, desktop typecheck and vitest
- [x] `cargo check -p workflow_profiles --target x86_64-unknown-linux-gnu
      --all-targets` — every changed `discover.rs` function is behind
      `cfg(target_os = "linux")` and so is unbuilt by a macOS-only gate run

## Done When

- [x] Requirements 1-6 each have a test that fails with the `is_absolute` filter
      removed and the helper kept; requirements 3-4 additionally guarded by a
      source-text assertion that fails when either call site splits `PATH` by hand
- [x] `git grep -nE 'split\(.:.\)|split\(.;.\)'` over `*.rs` matches only
      `crates/workflow/profiles/tests/spawn_program_paths.rs:40`, the guard test's
      own literals, so no executed `PATH`-style split outside `absolute_path_dirs`
      remains in the workspace
- [x] Requirements 7-10 verified by reading the changed lines; no behaviour change
      and therefore no test
- [x] Touched-crate lint and test gates green; desktop typecheck and vitest green
