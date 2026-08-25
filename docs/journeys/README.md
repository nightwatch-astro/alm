---
config: user-journeys/1
reporter: github-issues
reporter_labels: [bug, phase:build]
fix_loop: dispatch-coder
fix_loop_max_iterations: 3
runs_keep: 20
---

# User journeys

End-to-end user journeys for PlateVault: what a user does, what they must
observe, validated against the running product. `FORMAT.md` is the spec for
every file in this directory; `INDEX.md` is the generated routing table
(regenerate with `journeys.py index`, never hand-edit). This file is the
per-project configuration — frontmatter holds the settings, the sections
below hold the guidance agents need to run journeys here.

Findings filed to GitHub also carry the matching `journey-<n>` label
(existing taxonomy: `journey-1` … `journey-18` correspond to J01…J18).

Two product rules cut across nearly every journey and are stated once here
instead of being repeated per journey:

- **Reviewable filesystem mutation.** Every move, copy, archive, or delete is
  proposed as a plan first; only approving and applying a plan mutates files,
  and every applied action gets an audit record.
- **Every action answers back.** Each mutating step names its success signal
  (toast, navigation, visible state change) and its failure signal (refusal
  reason, per-item error) — a badge changing somewhere else is not sufficient
  evidence of a completed step.

## Interface profiles

### desktop-ui
- kind: desktop-mcp
- exclusive: false

PlateVault ships as a Tauri v2 desktop app; the only real-app validation path
today is the Windows build driven through the Tauri MCP bridge from WSL
(`driver_session host=localhost port=9223` for instance 1, mirrored WSL
networking reaches Windows services via `localhost`). The bridge exists only in a
build launched with `--features dev-tools`, starts only when the launch sets
`PV_MCP_BRIDGE_ENABLE=1`, and binds `127.0.0.1` unless the launch also sets
`PV_MCP_BRIDGE_BIND`. A second concurrent instance additionally needs the `e2e`
feature, which is what allows `PV_E2E_INSTANCE_ID` to skip the single-instance
guard. The Vite/mockIPC runtime fakes backend responses and MUST NOT be used to
validate journeys (see `docs/development/testing.md`).

The profile is not exclusive. N instances run side by side on one host, each with
its own database, app-data root, app-config root and bridge port — allocated by
`crates/e2e-tests/src/instance.rs`, the same module the nextest e2e harness uses.
`docs/development/parallel-journey-instances.md` is canonical for the launch
recipe, for what remains shared across instances, and for the Windows-host
rebuild.

A previous revision of this section claimed exclusivity because "only one
validator can hold the Windows checkout/app process at a time". That was false in
both halves. The bridge port is not fixed: 9223 is only the default base and the
plugin scans upward from it, so the per-instance base port
`9223 + (N - 1) * 100` gives every instance a stable address. And no instance
shares a database with another: `PV_DATA_DIR` alone separates them, before
`PV_DB_URL` is considered. What did make the historical lane unsafe to
parallelise was its own launch procedure — one hardcoded dev database at
`C:\dev\astro-plan\wizard-test.db`, reset by deleting that path — not any
property of the app.

Journeys that still need exclusivity, because they assert or mutate OS-global
state no per-instance root covers:

- **J17** installs a software update, replacing the binary every instance runs.
- **J06, J07, J11, J12** delete to the OS trash, and all instances share one
  Recycle Bin.

`FORMAT.md` has no per-journey exclusivity field, so those four cannot yet be
marked. Until one exists, a run that schedules them concurrently with anything
else is scheduling a known-shared resource; see
`docs/development/parallel-journey-instances.md` for the full table.

Launch, reset, recompile-trap, bridge-connect, native-picker, and blank-screen
mechanics are **canonical in `docs/development/windows-native-rust-dev.md`
§"Validation driving (MCP bridge, reset, recompile trap)"** — treat that doc as
authoritative rather than re-deriving the steps here. Profile-specific rules
that layer on top of it:
- Backend-only IPC probes are not a substitute for UI-level Expects — anything
  visually/interactively observable must be validated in the real webview, not
  IPC-only. Validators announce when a check is backend-only IPC and classify
  findings backend-vs-UI.
- **State-leakage prevention:** validation runs only against the Windows
  checkout's disposable dev database (`wizard-test.db` via `PV_DB_URL`) and
  `tempfile`-style scratch folders — never against real user libraries; this
  repo checkout is never the app's working directory, so no fixture can land in
  it.

Pointers: `docs/development/windows-native-rust-dev.md` §"Validation driving"
(canonical launch/reset/recompile/bridge mechanics),
`docs/development/windows-journeys/` (per-journey Windows validation docs with
exact click sequences and troubleshooting), and `.claude/rules/50-tauri-mcp.md`
(Tauri MCP driving surface, points into the `mcp-tauri` APM context doc).

### desktop-ui-macos
- kind: desktop-mcp
- exclusive: false

The same Tauri v2 app, built and driven natively on macOS through the same MCP
bridge. `desktop-ui` keeps its name and stays the **Windows** lane: two existing
`runs/*.md` records carry `interface: desktop-ui (...)`, and renaming would
invalidate them.

Nothing about the bridge is platform-gated: `--features dev-tools`,
`PV_MCP_BRIDGE_ENABLE=1`, port 9223, and the `127.0.0.1` default unless
`PV_MCP_BRIDGE_BIND` is set are all unconditional
(`apps/desktop/src-tauri/src/lib.rs` `start_mcp_bridge`,
`apps/desktop/src-tauri/src/bootstrap/mod.rs`). `webview_execute_js` still needs
`window.__TAURI__`, so the same `TAURI_CONFIG="$(cat tauri.dev.conf.json)"`
overlay merge applies — see `docs/development/mcp-bridge.md`, which is canonical
for the bridge on every OS. Native pickers cannot be driven here either; the
`VITE_E2E=1` `data-testid` stand-ins are frontend-only and OS-agnostic
(`docs/development/windows-native-rust-dev.md` §"Validation driving"). The
Vite/mockIPC runtime is no more a validation path here than on Windows.

`exclusive: false`: `desktop-ui` is exclusive because there is one shared
Windows host, not because the app forbids concurrency. On macOS, per-instance
app-data isolation works — `PV_DATA_DIR` overrides the app-data root on every
platform, and overriding `HOME` also works on macOS, unlike Windows where the
Known Folder API ignores `APPDATA` entirely
(`apps/desktop/src-tauri/src/data_dir.rs`). The WebView2 user-data-folder
collision documented in that same module is Windows-only; WKWebView has no
equivalent. Concurrency rules for the shared bridge port belong to the
parallel-instance guidance, not to this profile.

**State-leakage prevention:** as on Windows, run against a disposable dev
database (`PV_DB_URL`) and `tempfile`-style scratch folders, never a real
library. Two macOS-specific leaks to close:
- Real OS Trash. `PV_E2E_OS_TRASH_FAKE` exists because the *Windows* Shell trash
  (`IFileOperation::PerformOperations`) hangs on a headless runner
  (`crates/fs/executor/src/ops/trash_op.rs`). A macOS validator sits in a real
  login session, so `trash::delete` succeeds and files land in `~/.Trash` for
  real. Verify there, then empty what the run put there.
- App-data root. Unset, it is `~/Library/Application Support/` +
  `dev.astro-plan.astro-library-manager` (`apps/desktop/src-tauri/tauri.conf.json`
  `identifier`; `specs/029-tauri-backend-wiring/spec.md`), i.e. the real user's
  data. Set `PV_DATA_DIR` (or `HOME`) per run.

Behaviour that genuinely differs from the Windows lane, and what a validator
should therefore expect:
- **Reveal label.** `revealLabel()` returns the `reveal_label_macos` catalog
  string, "Reveal in Finder" (`apps/desktop/src/lib/reveal-label.ts`,
  `apps/desktop/messages/en-GB.json`). Platform comes from the webview
  navigator, so it is correct inside the macOS webview. Journeys phrase this
  Expect as "platform-equivalent"; on this lane the concrete string is the
  Finder one, not the File Explorer one.
- **Tool launch of an `.app` bundle.** Every seeded profile carrying a
  `bundle_id` goes through `/usr/bin/open -b`, which starts the app via Launch
  Services, so the project working folder is *not* applied and no child PID comes
  back (`crates/workflow/profiles/src/launch.rs` `spawn_platform`). J05/S5
  already states the working-folder half. A plain (non-bundle) executable takes
  the `process_group(0)` arm and does return a PID, which is recorded but changes
  no observable behaviour — see "Limitations shared by both desktop lanes".
- **A macOS-only launch failure branch.** `LaunchError::MacOsQuarantine` is
  raised when `open -b` fails with a quarantine / `LSOpenURLsWithRole` error
  (same file) — a refusal reason that has no Windows counterpart.
- **Path case semantics.** APFS/HFS+ default to case-insensitive-but-preserving,
  yet root-overlap detection case-folds only on Windows and compares exact bytes
  everywhere else (`crates/app/core/src/first_run/mod.rs`
  `path_overlap_relationship`, and its own doc comment). So on a default macOS
  volume `~/Foo` and `~/foo` are one directory that the overlap check treats as
  two distinct roots. This is a real divergence from J01/S2's Expect and is
  listed under "Open cross-platform divergences" below rather than silently
  absorbed.
- **Gatekeeper on a packaged build.** Tauri updater signing (minisign) is active
  for all OSes, but macOS Developer ID signing and notarization are wired inert:
  every `APPLE_*` value is gated on `vars.ENABLE_MACOS_SIGNING`
  (`.github/workflows/release-please.yml`), pending an Apple Developer account.
  A downloaded release `.app` is therefore unnotarized and quarantined on first
  launch — clear it deliberately (right-click → Open, or remove the
  `com.apple.quarantine` attribute) and record that as an `environment` step, not
  a product refusal. J17 is affected: it is runnable on macOS because the release
  workflow does build on `macos-latest` and does publish a signed `.app.tar.gz`
  updater artifact in `latest.json` (same file), so the signature-verification
  and install branches are exercisable — only Gatekeeper is extra.
- **No macOS bundle overlay.** There is no `tauri.macos.conf.json` and no
  entitlements file under `apps/desktop/src-tauri/`; the bundle carries Tauri's
  defaults.
- **Single-instance.** No journey exercises single-instance behaviour, and the
  plugin's per-instance override exists only on Linux (`dbus_id`) — not macOS
  (`crates/e2e-tests/README.md`).

CI is not a substitute for this lane. `ci.yml` runs the L1+L2
`cargo test --workspace` matrix on `macos-latest` by default, but the real-UI
WebDriver leg in `e2e.yml` has its **macOS leg removed from the default
matrix** — `tauri-plugin-webdriver` has no working macOS WebDriver on GitHub
runners, and macOS runs only on a `workflow_dispatch` `run_macos` input
(`.github/workflows/e2e.yml` header and `build-app-macos` job `if:`). Journey
validation on macOS goes through the MCP bridge on a real Mac, which is a
different harness and unaffected by that removal.

### Limitations shared by both desktop lanes

Not platform differences: true on Windows and macOS alike. Stated once here so
neither profile reads as the better-supported lane.

- **The re-launch guard never fires.** `pid_is_alive` is implemented only on
  Linux (`/proc/<pid>` presence) and returns a hardcoded `false` on every other
  target (`crates/workflow/profiles/src/launch.rs` `pid_is_alive_impl`), while
  the guard gates on `prior.pid.is_some_and(pid_is_alive)`
  (`crates/app/core/src/tool_launch.rs`). So a recorded PID makes no difference
  on either desktop lane, and a prior `spawned` launch never produces the
  "already launched" warning. No journey asserts that warning, so no journey
  coverage is affected. Tracked as `astro-plan-7wu52`.

### Open cross-platform divergences

Product-behaviour differences found while opening the macOS lane, not yet
adjudicated by the owner. Listed here, and not as journey **Known gaps**, because
a gap entry needs explicit user acceptance (FORMAT.md §"Definition of ready").

- Root-overlap case folding is Windows-only (see `desktop-ui-macos` above), so a
  case-variant duplicate root is rejected on Windows and accepted on a
  case-insensitive macOS volume. J01/S2 describes only the Windows behaviour.

## Surface map

Maps changed file paths to journey `surfaces:` names for changed-only
validation. Agent judgment bridges anything unmapped.

| path glob | surfaces |
|---|---|
| `apps/desktop/src/features/setup/**` | setup, data-sources |
| `apps/desktop/src/features/inventory/**` | data-sources |
| `apps/desktop/src/features/inbox/**` | inbox-confirm |
| `apps/desktop/src/features/sessions/**` | sessions |
| `apps/desktop/src/features/projects/**` | projects |
| `apps/desktop/src/features/guided/**` | onboarding |
| `apps/desktop/src/features/onboarding/**` | onboarding |
| `apps/desktop/src/features/targets/**` | targets |
| `apps/desktop/src/shared/observing-sites/**` | observing-sites |
| `apps/desktop/src/features/calibration/**` | calibration |
| `apps/desktop/src/features/archive/**` | archive |
| `apps/desktop/src/features/plans/**` | plans |
| `apps/desktop/src/features/settings/**` | settings |
| `apps/desktop/src/features/settings/Cleanup*` | cleanup |
| `apps/desktop/src/features/settings/AuditLog*` | audit |
| `apps/desktop/src/features/settings/Equipment*` | equipment |
| `apps/desktop/src/app/**` | shell, activity |
| `crates/fs/executor/**` | plans |
| `crates/fs/inventory/**` | data-sources |
| `crates/calibration/**` | calibration |
| `crates/targeting/**` | targets |
| `crates/audit/**` | audit |

(Globs verified against the tree 2026-07-15; the Cleanup/AuditLog/Equipment
pages live as files inside `features/settings/`, so those rows are
file-prefix globs that refine the broader `settings` row. Validators should
still trust the repo over this table and propose corrections.)

## Intent-evidence sources

Where an agent should look for proof that a behavior change was intentional
(amendment gating, see FORMAT.md), in this repo's actual conventions:

- Merged PRs (`gh pr list --state merged`, `gh pr view <n>`).
- `specs/NNN-*/` SpecKit feature artifacts (spec.md, plan.md, tasks.md).
- Grilling decision docs under `docs/product/`.
- `docs/development/*handover*` and orchestration/campaign logs under
  `docs/development/`.
- `CHANGELOG.md` and commit messages on `main`.

A "refactor"/"cleanup" commit is NOT intent evidence for a behavior change.

## Notes

Legacy journey history (pre-migration) lives under
`docs/product/journeys/JNN-slug/` — baseline narratives plus per-task
`deltas/*.md`, and the pre-format Wave-0 rerun sheets
(`wave0-rerun-plan.md`, `wave0-task-index.md`). Those files are frozen;
this directory is where current truth now lives: all eighteen journeys
(J01–J18) are migrated and listed in `INDEX.md`.

Cross-cutting validator rules (user-mandated):

- Layout invariant: action bars/headers always visible, only content
  scrolls; verify at 1100×720.
- Reveal-control labels are OS-native ("Show in File Explorer" on Windows,
  "Reveal in Finder" on macOS; `apps/desktop/src/lib/reveal-label.ts`).
- Every campaign wave ships per-journey delta docs — Δ entries + run files
  satisfy this rule going forward.
