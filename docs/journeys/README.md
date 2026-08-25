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
this directory is where current truth now lives: all seventeen journeys
(J01–J17) are migrated and listed in `INDEX.md`.

Cross-cutting validator rules (user-mandated):

- Layout invariant: action bars/headers always visible, only content
  scrolls; verify at 1100×720.
- Reveal-control labels are OS-native ("Show in File Explorer" on Windows).
- Every campaign wave ships per-journey delta docs — Δ entries + run files
  satisfy this rule going forward.
