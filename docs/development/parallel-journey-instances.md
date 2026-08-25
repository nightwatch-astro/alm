<!--
Copyright (C) 2024-2026 Sjors Robroek
SPDX-License-Identifier: AGPL-3.0-only
-->

# Driving N app instances in parallel

Journey validation drives a real app through the Tauri MCP bridge. Several
instances of that app run side by side on one host, each with its own database,
app-data root, app-config root and bridge port.

## What isolates an instance

`crates/e2e-tests/src/instance.rs` derives every per-instance path and port from
a 1-based instance number and a shared base directory. It is the single source
for both drivers: `crates/e2e-tests/tests/common/boot.rs::InstanceEnv` (nextest)
and the `journey-instance` binary (MCP-bridge driving).

| variable | instance N gets |
|---|---|
| `PV_DATA_DIR` | `<base>/pv-instance-N/appdata` |
| `PV_DB_URL` | `sqlite://<base>/pv-instance-N/e2e-test.db?mode=rwc` |
| `HOME` (macOS) / `XDG_DATA_HOME`, `XDG_CONFIG_HOME` (Linux) / `APPDATA`, `LOCALAPPDATA`, `WEBVIEW2_USER_DATA_FOLDER` (Windows) | under `<base>/pv-instance-N` |
| `PV_E2E_INSTANCE_ID` | `journey-N` |
| `PV_MCP_BRIDGE_PORT` | `9223 + (N - 1) * 100` |
| `TAURI_WEBDRIVER_PORT` | `4445 + (N - 1)` |

`PV_DATA_DIR` alone already separates the databases: with no `PV_DB_URL` the app
opens `<data dir>/alm.db` (`apps/desktop/src-tauri/src/main.rs`). `PV_DB_URL` is
set anyway so the file is named and disposable rather than inferred.

`PV_E2E_INSTANCE_ID` is what lets a second instance start at all: it skips the
single-instance plugin, and only in a build carrying the `e2e` feature
(`apps/desktop/src-tauri/src/bootstrap/mod.rs`).

## Launching an instance

```bash
export PV_JOURNEY_ROOT="$HOME/pv-journeys"      # optional; defaults under $TMPDIR
eval "$(just journey-instance 2 --reset)"       # --reset wipes instance 2 only
```

Stdout is a shell `export` block and nothing else. The bridge `host:port` and
the resolved paths go to stderr:

```
instance 2
  root      /Users/you/pv-journeys/pv-instance-2
  database  /Users/you/pv-journeys/pv-instance-2/e2e-test.db
  config    /Users/you/pv-journeys/pv-instance-2/Library/Application Support
  bridge    127.0.0.1:9323
  connect   driver_session host=127.0.0.1 port=9323
```

`journey-instance` refuses to print a block whose port is already bound, so a
printed port is never a guess.

Measured on macOS with three instances of a `dev-tools,e2e` build running at
once: bridge ports 9223, 9323 and 9423, WebDriver ports 4445, 4446 and 4447,
three separate database files, and no panic in any of the three.

Then launch the app in that environment, and serve the frontend once for all
instances:

```bash
export PV_DEV_URL="$(just e2e-dev-url)"
VITE_E2E=1 VITE_USE_MOCKS=false pnpm --filter @astro-plan/desktop preview \
  --port "${PV_DEV_URL##*:}" --strictPort &
cargo run -p desktop_shell --features dev-tools,e2e > "instance-2.log" 2>&1 &
```

One frontend server for N instances is intended: it is served read-only, and
each instance keeps its own `localStorage` because each has its own webview
profile. Distinct `PV_DEV_URL` values are needed per *checkout*, not per
instance (issue #1409).

### Why a fixed base port per instance rather than auto-allocation

`tauri_plugin_mcp_bridge::discovery::find_available_port` scans
`base_port..base_port + 100` and takes the first free port, so with one shared
base the port an instance ends up on depends on launch order and on which
instances are alive. A validator would have to read a log to learn where to
connect, and a restart could move it.

The stride is therefore 100 — the plugin's own scan width — so instance N's scan
window cannot reach instance N+1's advertised port, and instance N is at
`9223 + (N - 1) * 100` for the life of the host. Instance 1 stays on 9223, where
every existing journey run file says it is.

The port the app actually bound is still recoverable: the plugin prints
`[MCP][PLUGIN][INFO] MCP Bridge plugin initialized for ... on <bind>:<port>` on
stdout, which is why each instance above is launched with its stdout redirected
to a per-instance log.

## What is still shared

Per-instance isolation covers the app's own state. It does not cover OS-global
state.

| resource | shared or isolated | journeys that care |
|---|---|---|
| SQLite database | isolated (`PV_DB_URL`, and `PV_DATA_DIR` even without it) | all |
| app-data root, resolve cache, webview storage/`localStorage` | isolated (`PV_DATA_DIR`, per-OS location vars) | all |
| app-config root (window state) | isolated (per-OS location vars) | J10, J16 |
| MCP bridge port | isolated (`PV_MCP_BRIDGE_PORT`, stride 100) | all |
| WebDriver server port | isolated (`TAURI_WEBDRIVER_PORT`) — unset, the `e2e` build's server panics its thread on 4445 for every instance after the first | all, indirectly |
| single-instance guard | bypassed per instance (`PV_E2E_INSTANCE_ID`, `e2e` build) | none exercises it |
| **OS trash / Recycle Bin** | **SHARED** — one bin per OS user | J06, J07, J11, J12 |
| **scratch/library folders on disk** | **shared unless each instance is given its own** | J02, J03, J06, J07, J08 |
| **detached processing-tool launches** | **SHARED** — one PixInsight/Siril per machine, and the app never supervises it (constitution III) | J05, J14 |
| **OS notifications** | **SHARED** — one notification centre, and the notification names no instance | any asserting a completion toast |
| **native file pickers, window focus, z-order** | **SHARED** — a modal picker in one instance steals focus from the others | J02, J03, J15, J16 |
| **frontend dev/preview server** | shared by design, read-only | all |
| **installed application** (`tauri-plugin-updater`) | **SHARED** — an install replaces the binary all instances run | J17 |

Consequences a validator must plan around:

- **Trash.** Two instances deleting to trash observe one bin. A journey that
  asserts "the file is in the Recycle Bin" or counts its contents cannot be
  trusted while another destructive journey runs. `PV_E2E_OS_TRASH_FAKE` exists
  for the headless case (`crates/fs/executor/src/ops/trash_op.rs`, behind the
  default-off `e2e-trash-fake` feature) and redirects the deletion away from the
  real bin; a real interactive desktop does not need it, and setting it means
  the journey no longer validates real trash behaviour.
- **Scratch roots.** `journey-instance` gives each instance a root but does not
  populate library fixtures. Fixture folders must be created under that
  instance's root, never in a location another instance also scans.
- **Tool launches.** Two instances launching the same tool produce one process
  and two launch rows. Attribution is by launch row, not by process
  (`crates/app/core/src/tool_launch.rs`), so the rows stay correct, but a
  journey that asserts "the tool opened" cannot distinguish which instance
  opened it.
- **Updates.** J17 installs a build. Nothing else may run during it.

## Rebuild and relaunch on the Windows validation host

This is the recipe for the host at `172.20.10.10`. It is written to be executed
by a unit with access to that machine.

Data boundary: mock data only, per-instance throwaway roots, and no real
astrophotography file or live library location is read, copied, moved or
deleted. The old PlateVault deployment there — checkout, `target/`,
`node_modules/`, app database and config — may be discarded; astrophotography
files may not.

### 1. A release build can never satisfy this

The bridge needs `dev-tools`; a second instance needs `e2e`. Those are exactly
the two features `scripts/check-dev-surface-absent.sh` asserts are absent from a
default-feature build, and a dev-surface leak into a release binary is treated as
a defect. Journey driving therefore always runs a purpose-built dev binary.

### 2. Check what the existing binary has before rebuilding

The host's binary predates the parallel-instance work and is very likely
`dev-tools` only, because that is all the bridge itself needed. Determine it
rather than assume it — from the binary, not from shell history. Run this from
WSL against the mounted binary, where `strings` is available:

```bash
bin=/mnt/c/dev/astro-plan/target/debug/desktop_shell.exe
# dev-tools: this string exists only inside the dev-tools-gated bridge builder.
strings -a "$bin" | grep -c 'MCP bridge not started'
# e2e: the webdriver plugin is an e2e-only dependency.
strings -a "$bin" | grep -c 'tauri-plugin-webdriver'
# withGlobalTauri: the global API bundle is embedded only when it is set.
strings -a "$bin" | grep -c '__TAURI_IIFE__'
```

A zero on any of the three means a rebuild. Expect a zero on at least the second
and third.

Measured on a macOS `desktop_shell` built with `--features dev-tools,e2e` and no
config overlay: 1, 27, and 0 respectively. The third is the trap in the next
section.

### 3. `withGlobalTauri` is a mandatory gate, not a footnote

`tauri.dev.conf.json` is not a filename any build reads: `generate_context!`
reads `tauri.conf.json`, the per-platform overlays, and the `TAURI_CONFIG`
environment variable. A plain `cargo build` therefore drops the overlay
silently, and the failure is misleading rather than loud — `execute_js` times out
while `get_window_info` keeps succeeding, because the bridge is up and only page
JavaScript is missing its `window.__TAURI__`.

`TAURI_CONFIG` is a JSON merge patch applied over `tauri.conf.json` by
`tauri-codegen`, and is how the Tauri CLI passes `--config` through to the Rust
build. Setting it directly keeps the build a plain `cargo build`:

```powershell
cd apps\desktop\src-tauri
$env:TAURI_CONFIG = Get-Content -Raw tauri.dev.conf.json
cargo build -p desktop_shell --features dev-tools,e2e
```

Then gate on the marker before launching anything, from WSL:

```bash
strings -a /mnt/c/dev/astro-plan/target/debug/desktop_shell.exe \
  | grep -c '__TAURI_IIFE__'
```

Zero means stop. Every `execute_js`-based journey step will fail, and it will not
look like a build problem.

Measured on macOS with the same feature set, same checkout, only `TAURI_CONFIG`
differing: 0 without the overlay, 1 with it.

### 4. Per-launch environment

`journey-instance` is the only place the environment is written. From WSL, with
the checkout mounted:

```bash
cargo run -q -p e2e_tests --bin journey-instance -- 2 --reset
```

It prints POSIX `export` lines; translate to `$env:` assignments for PowerShell
or run the app from WSL-invoked `cmd.exe`. Set `PV_MCP_BRIDGE_BIND=0.0.0.0` when
the driving agent is in WSL rather than on the host — that opens unauthenticated
control of the app to anything that can reach the address.

Launch each instance with its stdout captured, then read the bound port from it:

```powershell
Start-Process -FilePath ..\..\target\debug\desktop_shell.exe `
  -RedirectStandardOutput "instance-2.log" -NoNewWindow
Select-String -Path "instance-2.log" -Pattern 'MCP Bridge plugin initialized'
```

The logged port is authoritative. It equals the printed `PV_MCP_BRIDGE_PORT`
unless something outside these instances took the port.

### 5. Cost and end state

The only recorded measurement for this host is in
`docs/development/windows-native-rust-dev.md`: the first `cargo tauri dev` build
after a pull takes ~90-110 s. Adding `--features e2e` changes the feature graph,
so nothing in a `target/` built without it is reused for the crates below
`desktop_shell` — budget a full rebuild rather than that figure, and treat any
estimate beyond "minutes, not seconds" as unmeasured.

The host is left with: a debug `desktop_shell.exe` carrying `dev-tools`, `e2e`
and `withGlobalTauri`; a `target/` directory of several GiB; one instance root
per instance number under `PV_JOURNEY_ROOT`; and N running app processes that
nothing supervises. Instance roots are disposable — `journey-instance N --reset`
removes one, and only one.
