# Windows-Native Rust/Tauri Development (canonical repo in WSL)

**Standard procedure** for live development and troubleshooting of the Astro
Library Manager desktop app as a **native Windows Tauri application**, when your
canonical working copy lives inside WSL.

## Why this setup exists

Running the app from the WSL checkout fails or is painful for two independent
reasons:

1. **UNC path problem.** Windows `node`/`pnpm`/`cargo` cannot use a
   `\\wsl$\…` / `\\wsl.localhost\…` path as a working directory (`cmd.exe`
   refuses UNC CWDs; many tools break). So you cannot drive the build from
   Windows against the WSL filesystem.
2. **WSL↔Windows networking** — historical: under NAT networking a Vite dev
   server bound inside WSL was often unreachable from a Windows browser.
   This host now runs WSL in **mirrored networking mode**
   (`networkingMode=mirrored` in `.wslconfig`), so `localhost` is shared in
   both directions and this reason no longer applies — but note the flip
   side: WSL and Windows now share one port space, so a stray WSL process on
   `:5173` (or `:9223`) directly conflicts with the Windows app's ports.

Reason 1 alone still mandates the fix: keep a **second checkout on a native Windows
NTFS drive** (e.g. `C:\dev\astro-plan`) and build/run there. node, cargo, Vite,
and the WebView2 window are all Windows-native — nothing crosses the WSL
boundary. The WSL copy stays as the canonical repo; the Windows copy is the
runtime/preview mirror. An agent (or you) can edit the Windows copy from the WSL
side via `/mnt/c/dev/astro-plan`, and Vite still hot-reloads (see Auto-update).

```
WSL  /home/<you>/dev/astro-plan      <- canonical git checkout (agent works here too)
Win  C:\dev\astro-plan               <- native build + run + live preview
      = /mnt/c/dev/astro-plan from inside WSL (edit here to drive HMR)
```

## One-time prerequisites (Windows side)

Verify the Windows toolchain. From PowerShell:

```powershell
node --version            # any current LTS
cargo --version           # rustup default host must be x86_64-pc-windows-msvc
rustup show               # confirm stable-x86_64-pc-windows-msvc
git --version
# MSVC linker (required by the msvc Rust toolchain):
& "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe" `
  -latest -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property displayName
```

Required:

- **Rust (MSVC toolchain)** — `stable-x86_64-pc-windows-msvc`.
- **Visual Studio / Build Tools 2022** with the **“Desktop development with
  C++”** workload (provides `link.exe`; Rust finds it via the registry, no
  `vcvars` needed).
- **Node.js** (LTS) and **pnpm** — enable pnpm via Corepack:
  ```powershell
  corepack enable
  corepack prepare pnpm@10.33.0 --activate
  ```
- **WebView2 Runtime** — preinstalled on Windows 11; otherwise install the
  Evergreen runtime.
- **git** — and allow it to read the WSL repo if you clone from there:
  ```powershell
  git config --global --add safe.directory '*'
  ```

## One-time setup (create the Windows checkout)

Clone from GitHub, or directly from the WSL repo to capture local-only branches
and uncommitted history base:

```powershell
mkdir C:\dev -Force
# Option A: from GitHub (has everything merged into main)
git clone https://github.com/nightwatch-astro/alm.git C:\dev\astro-plan
# Option B: from the WSL working copy (captures local branches too)
git clone \\wsl.localhost\Ubuntu\home\<you>\dev\astro-plan C:\dev\astro-plan
```

Then, from inside WSL, copy over any **uncommitted** app changes you want to see
(the clone only has committed state):

```bash
SRC=/home/<you>/dev/astro-plan ; DST=/mnt/c/dev/astro-plan
cp "$SRC/apps/desktop/src/features/.../Changed.tsx" "$DST/apps/desktop/src/features/.../Changed.tsx"
```

Install dependencies and pre-build esbuild (pnpm blocks its build script by
default; Vite needs it):

```powershell
cd C:\dev\astro-plan
pnpm install
pnpm rebuild esbuild
```

## Daily run

Use the launch script (real backend + WSL-edit-safe HMR by default):

```powershell
cd C:\dev\astro-plan
pwsh -File scripts\win-native-dev.ps1            # real Rust backend
pwsh -File scripts\win-native-dev.ps1 -Mocks     # UI only, in-memory fixtures
```

Or run it by hand (the env vars are the important part):

```powershell
cd C:\dev\astro-plan\apps\desktop
$env:VITE_USE_MOCKS    = 'false'   # real backend; 'true' = fixtures
$env:CHOKIDAR_USEPOLLING = 'true'  # so WSL-side edits trigger HMR
pnpm tauri dev
```

First launch compiles the whole Rust workspace (~1–2 min on a warm machine);
subsequent launches are incremental (~20 s). The native window opens
automatically; Vite serves the frontend on `http://127.0.0.1:5173`.

### Mock vs. real backend — important gotcha

`apps/desktop/vite.config.ts` resolves the flag from **`process.env`**, not the
`.env` file:

```ts
define: {
  "import.meta.env.VITE_USE_MOCKS":
    JSON.stringify(process.env.VITE_USE_MOCKS ?? "true"),
}
```

Consequences:

- **You must export `VITE_USE_MOCKS` as an environment variable.** Editing
  `apps/desktop/.env` alone does **not** wire the backend — the `define`
  overrides it, defaulting to `"true"` (mocks) when the env var is unset. This is
  the usual cause of “the native app is showing mock data.”
- `VITE_USE_MOCKS=false` ⇒ `isTauri()` is true, the frontend calls real Tauri
  `invoke` commands, and data comes from the SQLite store at
  `%APPDATA%\dev.astro-plan.astro-library-manager\alm.db`.
- `VITE_USE_MOCKS=true` ⇒ data comes from `apps/desktop/src/data/fixtures/*`.

## Auto-update (HMR) and where to edit

- Edit on the **Windows side** (VS Code on `C:\…`, etc.) → native file
  notifications → instant HMR.
- Edit from the **WSL side** via `/mnt/c/dev/astro-plan/…` → Windows file-change
  notifications across the WSL bridge are unreliable, so **polling**
  (`CHOKIDAR_USEPOLLING=true`, set by the launch script) is what guarantees Vite
  sees the change. Verified: a `/mnt/c` edit produces
  `[vite] (client) page reload …` and the window updates.
- Component edits hot-swap in place; editing entry files (`main.tsx`) triggers a
  full page reload. Rust source changes under `crates/…` or `src-tauri/…` cause
  `tauri dev` to rebuild and relaunch the app automatically.

### Driving it from WSL (agent workflow)

An agent operating in WSL can run the whole thing without a human at the Windows
terminal:

- Edit files via the `/mnt/c/dev/astro-plan/…` paths.
- Launch/stop/inspect the build via `powershell.exe -NoProfile -Command '…'`.
- Launch detached and tee logs so the build can be watched without blocking:
  ```powershell
  Start-Process cmd.exe -WindowStyle Hidden -WorkingDirectory C:\dev\astro-plan\apps\desktop `
    -ArgumentList '/c','set VITE_USE_MOCKS=false&& set CHOKIDAR_USEPOLLING=true&& pnpm tauri dev > C:\dev\astro-plan\tauri-dev.log 2>&1'
  ```
  Then read `/mnt/c/dev/astro-plan/tauri-dev.log` for progress.

### Validation driving (MCP bridge, reset, recompile trap)

Canonical mechanics for driving the **real running app** during validation —
manual computer-use ("cowork") scenarios and automated journeys alike. Each
block below is self-contained: copy the relevant one verbatim into a scenario or
run doc, since a zero-context agent must be able to execute it without following
a link.

**Launch with a throwaway database (bridge on).** For validation, point the app
at a disposable DB inside the checkout so a reset never touches a real library,
and launch with the dev overlay that enables the MCP bridge:

```powershell
cd C:\dev\astro-plan\apps\desktop
$env:VITE_USE_MOCKS = 'false'                                        # real backend
$env:PV_DB_URL     = 'sqlite://C:\dev\astro-plan\wizard-test.db?mode=rwc'
pnpm tauri dev --config src-tauri\tauri.dev.conf.json --features dev-tools
```

The overlay enables `withGlobalTauri`; `--features dev-tools` compiles the MCP
bridge in. A build without the feature links no bridge, so nothing listens on
9223.

`scripts\win-native-dev.ps1` launches the same overlay with the real backend;
add the `PV_DB_URL` line above when you need a disposable DB. Any `run-dev*.bat`
referenced elsewhere is an optional local convenience wrapper — **not** tracked
in the repo, so the tracked launcher above is the source of truth. App process is
`desktop_shell.exe`; Vite on `http://127.0.0.1:5173`.

**Reset to a clean first-run.** The DB is the first-run source of truth —
clearing `localStorage` alone triggers a `/`↔`/setup` redirect loop, not a
reset. Delete the throwaway DB and relaunch:

```powershell
Remove-Item 'C:\dev\astro-plan\wizard-test.db*' -Force
```

**Recompile (mtime) trap.** Deploy a branch as its own command, then force a
rebuild when any Rust changed — `git reset --hard` restores file content but
keeps the old mtime, so cargo thinks the binary is current and runs a **stale**
build (symptom: a command that IS in the code returns "not found"):

```powershell
cd C:\dev\astro-plan
git fetch origin
git reset --hard origin/<branch>        # its OWN command (a guard rejects it bundled)
Get-ChildItem <changed>.rs | ForEach-Object { $_.LastWriteTime = Get-Date }
```

Then relaunch (`tauri dev` recompiles). A frontend-only change needs only a hard
refresh (Ctrl+R). Confirm the binary mtime is newer than the source.

**Connect the MCP bridge.** The bridge needs `--features dev-tools` to be
compiled in and `PV_MCP_BRIDGE_ENABLE=1` to start; without the variable a
`dev-tools` build opens no port, and the startup log says `MCP bridge not
started`. It serves a WebSocket on `127.0.0.1:9223`. `scripts/win-native-dev.ps1
-McpBridge` sets the variable. This host runs WSL in mirrored networking, so
`localhost` reaches Windows loopback directly — connect with `driver_session
host=localhost port=9223`. The old NAT gateway-IP lookup (`ip route show
default`) is **obsolete** here; under NAT networking, set
`PV_MCP_BRIDGE_BIND=0.0.0.0` before launching so the gateway address reaches the
bridge. Anything that reaches that address controls the app without
authentication. Invoke a command directly with `webview_execute_js` →
`window.__TAURI__.core.invoke('<snake_command>', {args})` (`ipc_execute_command`
rejects many real commands but is fine for scripted backend probes).

**Native pickers can't be driven.** Relaunch with `VITE_E2E=1` to expose
`data-testid="e2e-path-input-<kind>"` / `e2e-add-path-btn-<kind>` stand-ins
(kinds: `light_frames`, `calibration`, `project`, `inbox`). Set the
React-controlled input via the native value setter + dispatch `input`, then click
the button in a **separate** call so React state commits.

**Driving quirks.** JS eval has a ~2 s timeout (avoid zombie loops); navigate in
two steps (nav, then probe); **never** send modifier-key combos (renderer
freeze). Recovery: kill `desktop_shell`, relaunch.

**Blank screen (empty `#root`).** Two Vite causes: a mid-session `pnpm install`
re-optimized deps → restart the dev server; or a cold start hit the
`@rollup/rollup-win32-x64-msvc` optional-native bug → `pnpm install` with
`$env:CI='true'`, then relaunch.

**Fidelity honesty.** Backend-only IPC probes do not substitute for UI-level
checks: anything visually or interactively observable must be validated in the
real webview, not IPC-only. State which fidelity you actually drove.

The test-layer and coverage story (Layer-1 integration, Layer-2 `tauri-driver`,
the coverage matrix) lives in `docs/development/testing.md` and
`specs/037-e2e-integration-testing/contracts/coverage-matrix.md` — it is not
restated here.

## Stop / cleanup

```powershell
Get-Process desktop_shell,cargo,rustc -ErrorAction SilentlyContinue | Stop-Process -Force
# free the Vite port if a stray dev server is holding it:
(Get-NetTCPConnection -LocalPort 5173 -State Listen -EA SilentlyContinue).OwningProcess |
  Sort-Object -Unique | ForEach-Object { Stop-Process -Id $_ -Force -EA SilentlyContinue }
```

## Troubleshooting

| Symptom | Cause / fix |
| --- | --- |
| Native app shows **mock data** | `VITE_USE_MOCKS` env var not set to `false`. The `.env` file alone is ignored (see gotcha above). Relaunch with the env var. |
| `fatal: detected dubious ownership` when cloning from WSL | `git config --global --add safe.directory '*'` on Windows. |
| `Port 5173 is already in use` | A stray Vite is holding the port — kill it (see Stop/cleanup). `strictPort` is on, so Vite won’t auto-pick another. |
| `Ignored build scripts: esbuild` after `pnpm install` | Run `pnpm rebuild esbuild` (or `pnpm approve-builds`). |
| `UNC paths are not supported. Defaulting to Windows directory` | Harmless — `powershell.exe` was started with a WSL CWD. Always `Set-Location C:\dev\astro-plan` (or use the script) before building. |
| `link.exe`/`cl` “not found” at the Rust link step | Install VS Build Tools 2022 “Desktop development with C++”. |
| WSL-side edits don’t hot-reload | Ensure `CHOKIDAR_USEPOLLING=true` (default in the launch script). |
| Keeping the two checkouts in sync | Treat WSL as canonical: `git push origin main` from WSL, then on Windows `git fetch origin; git reset --hard origin/main` (the mirror tree is often dirty/divergent so a plain `git pull` won't fast-forward — `git stash` first to keep any local mods). **Do NOT** pull the Windows checkout from the WSL repo over a `\\wsl.localhost\…` UNC path: `node`/`pnpm` fail over UNC and git is flaky there. Keep all git + node + cargo on the native `C:\` filesystem. |
| `tauri dev` fails with `ERR_PNPM_RECURSIVE_RUN_FIRST_FAIL … Exit status 4294967295` | The `beforeDevCommand` (Vite) couldn't bind `:5173`. Under mirrored networking WSL and Windows share one port space, so a Vite dev server left running **inside WSL** on `:5173` occupies the port outright, and Vite uses `strictPort`. Kill the WSL `:5173` server (`lsof -ti :5173 \| xargs kill`) before launching on Windows, or change the port. |
