# AGENT-RUNNER — Tauri-MCP runner mechanics for PlateVault agentic scenarios

Shared, once-written mechanics profile for every scenario under
`e2e-agentic-test/`. Scenarios reference this file instead of repeating launch,
reset, bridge, and capture mechanics. If a scenario contradicts this file, the
scenario wins (it may pin a branch- or feature-specific variant).

Every scenario in this tree is **two-stage**:

1. **Stage 1 — Agent validation via Tauri MCP**: an agent drives the REAL
   Windows app (real backend, real SQLite — mock mode is forbidden) through the
   Tauri MCP bridge and asserts concrete selectors, IPC payloads, screenshots,
   and log lines. Stage 1 must fully PASS before Stage 2 starts.
2. **Stage 2 — Final Claude Desktop pass**: a human-judgment visual/UX pass
   (Claude Desktop / computer-use) that signs off on look, feel, copy, i18n,
   and layout. It never runs against a build that failed Stage 1.

## Checkouts and where the app serves from

- WSL repo (edit/commit/push): `/home/sjors/dev/astro-plan`.
- Windows app checkout: `C:\dev\astro-plan` (= `/mnt/c/dev/astro-plan` from
  WSL). The dev app builds and serves **only** from the Windows checkout.
- To deploy a branch to the app: commit → push → then on the Windows checkout:

  ```
  cd C:\dev\astro-plan
  git fetch origin
  git reset --hard origin/<branch>     # run as its OWN command
  ```

## Launching the Windows dev app (never `cmd`/`start` over /mnt/c)

Launch **detached** from WSL with `powershell.exe`. A `run-dev.bat` at the
Windows repo root holds, each on its own line (no `&&`):

```
cd /d C:\dev\astro-plan
set PV_DB_URL=sqlite://C:\dev\astro-plan\wizard-test.db?mode=rwc
set PV_MCP_BRIDGE_ENABLE=1
set PV_MCP_BRIDGE_BIND=0.0.0.0
cargo tauri dev --config src-tauri\tauri.dev.conf.json --features dev-tools
```

Launch command (from WSL):

```
powershell.exe -NoProfile -Command "Start-Process -FilePath 'cmd.exe' -ArgumentList '/k','C:\dev\astro-plan\run-dev.bat' -WorkingDirectory 'C:\dev\astro-plan'"
```

- The `--config src-tauri\tauri.dev.conf.json` overlay enables `withGlobalTauri`,
  and `--features dev-tools` compiles the MCP bridge WebSocket server in. Without
  the feature the plugin is not linked and nothing listens on 9223.
- `PV_MCP_BRIDGE_ENABLE=1` starts the bridge. `1` is the only value that starts
  it; without the variable a `dev-tools` build opens no port.
- The bridge binds `127.0.0.1:9223`. `PV_MCP_BRIDGE_BIND=0.0.0.0` binds all
  interfaces, which is what a WSL agent connecting through the NAT gateway needs.
  Anything that reaches that address drives the app without authentication, so set
  it only for a validation run on a trusted network.
- Scenarios in this tree REQUIRE the bridge, so always launch with the overlay,
  the feature, and both bridge variables.
- App process = `desktop_shell.exe`; Vite dev server on `localhost:5173`.
- `.env` in the Windows checkout must have `VITE_USE_MOCKS=false` (real
  backend). **Verify this before running any scenario** — a scenario executed
  against mocks is automatically INVALID.
- Kill: `powershell.exe -NoProfile -Command "Get-Process desktop_shell,cargo -ErrorAction SilentlyContinue | Stop-Process -Force"`.

### VITE_E2E input conventions

Native folder pickers **cannot** be driven through the bridge. When a scenario
needs to enter paths (setup wizard, Data Sources add form), launch with
`VITE_E2E=1` by adding this line to `run-dev.bat` before `cargo tauri dev`:

```
set VITE_E2E=1
```

That exposes deterministic path inputs in the setup wizard's Source Folders
step, one per source kind (`light_frames`, `calibration`, `project`, `inbox`):

- `data-testid="e2e-add-by-path-<kind>"` — wrapper span
- `data-testid="e2e-path-input-<kind>"` — text input
- `data-testid="e2e-add-path-btn-<kind>"` — "Add" button

Because the input is React-controlled, set its value via the **native value
setter** and dispatch an `input` event, then click the add button in a
**separate** bridge call so React state commits first:

```js
// call 1 — set the value
(function () {
  const el = document.querySelector('[data-testid="e2e-path-input-light_frames"]');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
  setter.call(el, 'C:\\dev\\astro-plan\\test-data\\raw-lights');
  el.dispatchEvent(new Event('input', { bubbles: true }));
  return el.value;
})();
```

```js
// call 2 — commit
document.querySelector('[data-testid="e2e-add-path-btn-light_frames"]').click();
```

## Resetting to a clean first-run

The **database is the first-run source of truth**. Clearing localStorage alone
causes a `/` ↔ `/setup` redirect loop / navigation-throttle hang. Reset = kill
the app, then:

```
powershell.exe -NoProfile -Command "Remove-Item 'C:\dev\astro-plan\wizard-test.db*' -Force -ErrorAction SilentlyContinue"
```

then relaunch. Also clear the wizard's localStorage buffer if a previous run
left it mid-flow (via bridge JS): `localStorage.removeItem('alm-setup-wizard-state')`.

## The RECOMPILE TRAP (after every `git reset --hard`)

`git reset --hard` restores file content but leaves an **old mtime**, so cargo
skips recompiling and the app stays stale (symptom: a command that IS in the
code returns "not found"). After any reset that includes Rust changes, force a
rebuild by touching the changed `.rs` files, then relaunch:

```
powershell.exe -NoProfile -Command "Get-ChildItem C:\dev\astro-plan\crates -Recurse -Filter *.rs | Where-Object { $_.FullName -match '<changed-area>' } | ForEach-Object { $_.LastWriteTime = Get-Date }"
```

Cheap catch-all when unsure: touch `apps/desktop/src-tauri/src/lib.rs` and the
persistence crate's `lib.rs` (new SQL migrations are embedded by a proc-macro
that does not re-read the migrations dir on its own — touching
`crates/persistence/db/src/lib.rs` forces the re-embed). Frontend-only changes
need only a hard refresh (Ctrl+R); Rust changes REQUIRE the cargo-tauri-dev
relaunch.

## Connecting the Tauri MCP bridge (NAT host:9223)

From WSL, the Windows host is reached via the NAT gateway, not `localhost`:

```
gateway=$(ip route show default | awk '{print $3}')   # e.g. 172.23.112.1
```

Connect: `mcp__tauri__driver_session` with `host=<gateway>` `port=9223`.
The Windows firewall already allows 9223. A gateway connection reaches the bridge
only when the app was launched with `PV_MCP_BRIDGE_ENABLE=1` and
`PV_MCP_BRIDGE_BIND=0.0.0.0`; the default `127.0.0.1` bind accepts connections
from the Windows host alone, and without the enable variable no port is open at
all. If the session fails, confirm the app was launched with the
`tauri.dev.conf.json` overlay, `--features dev-tools`, and both variables. The
startup log records which case applies: `MCP bridge starting` or `MCP bridge not
started`.

## Driving and asserting

- **DOM**: `mcp__tauri__webview_find_element` / `webview_dom_snapshot` /
  `webview_select_element` with the scenario's `data-testid` selectors;
  `webview_interact` for clicks; `webview_keyboard` for typing/Escape;
  `webview_wait_for` for async settle.
- **Invoke a command directly**: the bridge rejects many commands via
  `ipc_execute_command`; the reliable path is `webview_execute_js` →
  `window.__TAURI__.core.invoke('<snake_command>', { ...args })`. Command
  invoke targets are the Rust **fn names** (snake_case, e.g. `roots_list`,
  `firstrun_restart`, `projects_create`) — never the dotted specta rename.
- **Capture IPC traffic**: start `mcp__tauri__ipc_monitor` (capture on) before
  the interaction, perform the UI action, then `mcp__tauri__ipc_get_captured`
  and assert the expected command name, request payload (camelCase keys), and
  response. Scenarios state exact command/payload assertions.
- **Screenshots**: `mcp__tauri__webview_screenshot` at every checkpoint a
  scenario marks `[SCREENSHOT]`. Name them `<scenario>/<checkpoint-id>.png` in
  your run report.
- **Logs**: `mcp__tauri__read_logs` for backend tracing output; the in-app
  bottom log panel (expand via the bottom strip) for user-visible
  settings/audit entries.
- **Window size**: scenarios validate layout at **1100×720**. Set it first:
  `mcp__tauri__manage_window` (resize to 1100×720). Action bars/headers must
  stay visible; only content scrolls (`.alm-page` / `.alm-page__bar` /
  `.alm-page__scroll` convention).
- **DB spot-checks (read-only, from WSL)**: python `sqlite3` against
  `file:/mnt/c/dev/astro-plan/wizard-test.db?mode=ro&immutable=1`.
  Base64-encode any output you will quote (tool output can mangle string
  literals). Never write to the DB directly.

## Theme switching (layout checks in ≥2 themes)

Settings → Appearance (nav group "Application", pane `Appearance`) hosts the
theme picker. Scenarios that require a two-theme layout check mean: run the
listed `[SCREENSHOT]` checkpoints once in the default theme and once in a
contrasting theme, and compare per the scenario's layout expectations.

## Blank screen (empty `#root`) recovery

Two Vite/pnpm causes: (1) a mid-session `pnpm install` re-optimized deps →
restart the dev server; (2) cold Vite start hits the
`@rollup/rollup-win32-x64-msvc` optional-native bug → in the Windows checkout
run `pnpm install` with `$env:CI="true"`, then relaunch.

## Stage-2 handoff protocol

Stage 2 runs only after Stage 1 reports PASS on every test. The Stage-2
operator receives: the scenario file, the Stage-1 run report (per-test
PASS/FAIL + screenshots), and an app already deployed on the correct branch
with the scenario's fixtures still in place. Stage 2 re-uses the same launch /
reset mechanics above when it needs a fresh state.
