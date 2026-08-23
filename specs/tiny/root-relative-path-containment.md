# TinySpec: One rule for resolving a path against a root

**Branch**: fix/containment-core
**Date**: 2026-08-22
**Complexity**: 5 (Full Spec route, speckit-bugfix)
**Findings**: astro-plan-3v3r.1.12, .1.16, .5.20, .13.26 (LIVE);
.8.21 (refuted as filed), .9.10 (already fixed by PR #1705)

## What

`resolve_item_path`, `resolve_working_folder`, `resolve_archive_abs_path`, and
`verify_cwd_containment` each resolve a caller-supplied path against a library,
project, or archive root. Each answers "the root is absent" by returning a path
anyway, and each decides containment by `starts_with` on unnormalized text or
not at all. A path outside every declared root therefore reaches a filesystem
mutation and is recorded as a contained, reviewed action.

| Site | Absent-root answer today | Containment check today |
|------|--------------------------|-------------------------|
| `crates/fs/executor/src/run/dispatch.rs:98` `resolve_item_path` | returns the relative path, resolved against the process cwd | none; the gate in `run/loop_.rs:161` uses a different root choice, so a destination with only `library_root` set is never gated |
| `crates/project/structure/src/lib.rs:225` `resolve_working_folder` | returns the project root | none; an absolute value replaces the root, a relative `..` escapes it |
| `crates/app/core/src/plans/archive.rs:34` `resolve_archive_abs_path` | returns the stored path unrooted, then trashes or deletes it | none; an unresolvable `from_root_id` takes the same branch as no root id |
| `crates/workflow/profiles/src/launch.rs:238` `verify_cwd_containment` | an empty root list is reported contained | `Path::starts_with` on unnormalized text |

## The rule (decision)

**A path is usable only as a `ContainedPath`: the lexical normalization of the
path joined onto the lexical normalization of its root, proven to start with
that normalized root.**

Resolving against an absent root is refused. The one legitimate rootless shape is
an already-absolute, already-normal path, and the caller requests it by name
rather than reaching it through a fallback arm:

| Input | Result |
|-------|--------|
| root present, relative path that stays inside | `Ok`, the normalized join |
| root present, relative path that escapes after normalization | `Err(Escapes)` |
| root present, absolute path inside the root | `Ok`, the path itself |
| root present, absolute path outside the root | `Err(Escapes)` |
| root present but relative | `Err(RootNotAbsolute)` |
| no root, absolute path with `.` or `..` components | `Err(NotNormalized)` |
| no root, absolute normal path | `Ok`, via `resolve_unrooted` only |
| no root, relative path | `Err(NotAbsolute)` from `resolve_unrooted`, or `RootMissing` where the caller knows a root id failed to resolve |

### The verdict is lexical, so a non-existent leaf is contained

`std::fs::canonicalize` follows symlinks, which the Product Constraints forbid,
and it fails on a path that does not exist. The verdict is therefore purely
lexical, using `path-clean` (already a workspace dependency of `path_gate`), and
the root is normalized as well: a `starts_with` against an unnormalized root is
what accepts `/mnt/library/../../a`.

A leaf that does not exist is the normal case for a write, and its lexical form
alone decides containment. Symlink refusal stays where it already is, in the
per-component `lstat` walk in `path_gate::resolve_and_validate`, which stops at
the first component that does not exist because a non-existent component cannot
be a link.

### An absolute root means absolute on the running platform

`Path::is_absolute` requires a drive or UNC prefix on Windows, so the
Unix-shaped literal `/mnt/library` is a relative root there and every entry
point answers `RootNotAbsolute`. Tests that assert a containment verdict on a
made-up path build their roots through `fs_pathsafe::test_support::abs`, which
prefixes `C:` on Windows and returns the path unchanged elsewhere. The module is
gated behind `cfg(test)` plus the default-off `test-fixture` feature, so the
helpers are absent from a release build.

#### Verdict per platform divergence

`resolve_in_root` normalizes both sides with `path-clean` and then compares with
`Path::starts_with`, which matches whole components. Where the filesystem treats
two byte-different spellings as one location, the comparison sees two different
components and reduces the match, so each divergence below refuses a path the
filesystem would have accepted. None admits a path outside the root.

| Divergence | Comparison behaviour | Verdict |
|------------|----------------------|---------|
| NTFS case: root `C:\Library`, path `C:\library\a` | components differ byte-wise | over-refuses (`Escapes`) |
| Separators `\` and `/` on Windows | `std` treats both as component separators, so neither side is favoured | handled |
| Verbatim `\\?\C:\Library` against `C:\Library` | the prefix component differs, so no component matches | over-refuses (`Escapes`) |
| UNC `\\server\share` against a `Disk` prefix | prefix kinds differ | over-refuses (`Escapes`) |
| Drive-relative `C:foo` | `is_absolute` is false | over-refuses (`RootNotAbsolute`) |
| 8.3 short name `PROGRA~1` against the long name | components differ byte-wise | over-refuses (`Escapes`) |
| Unicode NFC against NFD spelling of one filename | components differ byte-wise | over-refuses (`Escapes`) |

The six over-refusals in the table are intended behaviour, and none offers the
user a recovery path. The operation is refused, and re-spelling the input does
not make it pass, because the stored root is the other side of the comparison.

Principle IV requires that tradeoff recorded. A byte-wise comparison cannot
admit a path outside the root. Case-folding or Unicode-folding the comparison
makes two distinct on-disk names compare equal, which admits one of them.
Sibling PR #1712 folds a destination *key* for deduplication, a different
question from a containment verdict.

`verify_cwd_containment` at `crates/app/core/src/tool_launch.rs:282-300` is the
one caller that canonicalizes before comparing, because a working folder is an
existing directory. It canonicalizes both the roots and the working folder with
`dunce`, which strips the Windows verbatim prefix. `std::fs::canonicalize`
returns a verbatim path for an existing root and fails on a working folder that
does not exist yet, and that fallback keeps the raw non-verbatim spelling, so
canonicalizing only one side refuses a legitimate working folder on Windows.

#### The lexical verdict and the `lstat` walk answer different questions

The containment verdict is lexical, so `..` collapses before the comparison. A
lexically contained path can still leave the root at run time when one of its
components is a symlink or an NTFS junction. Refusing that is the job of the
per-component `lstat` walk in `path_gate::resolve_and_validate`, not of
`contain`. Both checks are therefore required, and a side that reaches the
filesystem without passing `path_gate` is not protected by the lexical rule
alone.

### Archive and trash destinations reach the filesystem ungated

`ExecutorItemAction::Archive { archive_destination }` and
`ExecutorItemAction::Trash { fallback_archive_destination }` reach
`archive_op::archive_file` and `trash_op::trash_file` directly from
`crates/fs/executor/src/run/dispatch.rs:49-61`. `run/loop_.rs:179-182` applies
`resolve_side` and `path_gate` to `resolved_src` and `resolved_dst` only, so
neither archive destination passes either. The plan-side check
`resolve_unrooted_utf8` at `crates/app/core/src/plans/archive.rs:56-58` proves
absolute-and-normal, which is not inside-a-root. An absolute out-of-root archive
destination therefore still reaches `move_file`, and no `lstat` walk refuses a
junction component on it.

Closing that requires an archive-root policy decision, since an archive
destination on another drive is a supported user configuration, plus root
context threaded into the executor item loop. It is tracked on
`astro-plan-zboex` and is out of scope here.

### The rule is implemented once, in `fs_pathsafe`

`fs_pathsafe` is the containment crate and is already a dependency of
`fs_executor`, `app_core`, `fs_inventory`, and `app_inbox`. The rule is added
there as `fs_pathsafe::contain`, and `path_gate::resolve_and_validate` keeps its
symlink walk while delegating the join-and-contain step to it. The
`check_assembled_path` guard in `patterns` (PR #1705) checks per-segment token
assembly, not root-relative resolution, and stays where it is.

## Per-finding verdict at head 4948b3ece

| Finding | Verdict | Evidence read at this head |
|---------|---------|----------------------------|
| .1.12 | LIVE | `dispatch.rs:98-110` has both defective arms. `loop_.rs:165` gates the destination against `destination_root` only, while `dispatch.rs:36` joins `destination_root.or(library_root)` |
| .1.16 | LIVE | `lib.rs:225-238` returns an absolute value as-is and joins a relative value unchecked |
| .5.20 | LIVE | `archive.rs:34-43` conflates an unresolvable root id with no root id in `from_root_id.and_then(get)` |
| .13.26 | LIVE | `launch.rs:242-252` returns `Ok` for empty roots and compares unnormalized text. The caller at `tool_launch.rs:281` canonicalizes the cwd but falls back to the raw path when canonicalize fails, which is exactly the not-yet-created destination |
| .8.21 | refuted as filed | `loop_.rs:163,166` call `resolve_and_validate` one layer below the command handlers, by design. The finding's own metadata records "no command-layer caller exists, so TB-1 is a serde shape boundary with no value boundary". Its substance, a mutation reaching the filesystem ungated, is .1.12 and is fixed here |
| .9.10 | already fixed | `resolver.rs:284` `check_assembled_path`, PR #1705 (squash 55229d8d9) |

## Context

| File | Role |
|------|------|
| `crates/fs/pathsafe/src/contain.rs` | Add: `ContainedPath`, `ContainmentError`, `resolve_in_root`, `resolve_unrooted`, `contained_in_any`, `normalize` |
| `crates/fs/executor/src/ops/path_gate.rs` | Modify: `resolve_and_validate` delegates the join-and-contain step, symlink walk unchanged; `lexical_normalize` deleted so one normalizer remains |
| `crates/app/core/src/plan_apply/{paths,reconcile}.rs` | Modify: use `contain::normalize_utf8`; the FR-017 overlap set is compared, never mutated, so an uncontained path is kept rather than refused |
| `crates/fs/executor/src/run/loop_.rs` | Modify: resolve each side once and hand the resolved paths to `execute_item`, so no second root choice exists |
| `crates/fs/executor/src/run/dispatch.rs` | Modify: `execute_item` consumes resolved paths, `resolve_item_path` deleted |
| `crates/project/structure/src/lib.rs` | Modify: `resolve_working_folder` returns `Result` |
| `crates/app/core/src/plans/archive.rs` | Modify: `resolve_archive_abs_path` returns `Result`, an unresolvable `from_root_id` is an error |
| `crates/app/core/src/tool_launch.rs` | Modify: handle the two new `Result` values |
| `crates/workflow/profiles/src/launch.rs` | Modify: `verify_cwd_containment` uses `contained_in_any`, an empty root list is refused |

## Requirements

1. No function resolves a path against a root without returning a containment
   verdict.
2. An absent root refuses. A rootless absolute path is accepted only through the
   explicitly named entry point.
3. Both the path and the root are lexically normalized before the prefix
   comparison.
4. No resolution path calls `canonicalize`, and a non-existent leaf is accepted.
5. The executor resolves each item side exactly once, and `dispatch` performs no
   second join and no second root fallback.
6. A launch whose working directory is outside every registered root, or which
   has no registered root at all, is refused with `cwd.outside_library_root`.
7. `verify_cwd_containment`'s caller canonicalizes the roots and the working
   folder through the same function, so the two sides carry the same Windows
   prefix spelling.
8. Fixture helpers that build platform-absolute roots are absent from a release
   build.

## Tasks

- [x] Add `fs_pathsafe::contain` with a unit test for each of the seven rule rows
- [x] Route `path_gate::resolve_and_validate` through it, keeping the symlink walk
- [x] Resolve once in `run/loop_.rs`, delete `dispatch::resolve_item_path`
- [x] `resolve_working_folder` returns `Result`; update callers and tests
- [x] `resolve_archive_abs_path` returns `Result`; an unresolvable root id errors
- [x] `verify_cwd_containment` uses `contained_in_any`; empty roots refuse
- [x] Canonicalize both sides of the `verify_cwd_containment` comparison with
      `dunce`, so a not-yet-created working folder compares against a
      non-verbatim root
- [x] Gate `fs_pathsafe::test_support` behind `cfg(test)` plus the default-off
      `test-fixture` feature, activated in each consumer's dev-dependencies
- [x] One regression test per LIVE finding, run red before the fix
- [x] `cargo clippy` and `cargo nextest` for the six touched crates

## Done When

- [x] Every LIVE finding has a test that fails at base 4948b3ece and passes here
      (.1.12 5/6, .1.16 2/2, .5.20 2/2, .13.26 2/2 red at base)
- [x] All four rule cases (absent root, escape after normalization, absolute
      where a root is expected, non-existent leaf) have a unit test
- [x] Touched-crate lint and test gates are green
