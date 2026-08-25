# MCP bridge

The MCP bridge is a WebSocket server inside the desktop app that lets an agent
drive the running UI. `@hypothesi/tauri-mcp-server` connects to it and exposes
tool calls such as `driver_session` and `webview_execute_js`, which is how
automated journeys and manual validation sessions click through the real app
against a real backend. It listens on port 9223 by default, scanning upward from
there when that port is taken. `PV_MCP_BRIDGE_PORT` moves the base, which is how
several app instances run side by side — see
`parallel-journey-instances.md`.

The bridge is compiled only into a build with `--features dev-tools`. Release
binaries omit that feature, so `tauri-plugin-mcp-bridge` is not linked and the
code is absent rather than switched off. `scripts/check-dev-surface-absent.sh`
fails when a default-feature build resolves the plugin crate or a shipped
capability file grants one of its permissions.

## Turning it on

| Variable | Default | Effect |
|---|---|---|
| `PV_MCP_BRIDGE_ENABLE` | unset | `1` starts the bridge. Every other value, including `0`, `true`, and empty, leaves the port closed. |
| `PV_MCP_BRIDGE_BIND` | `127.0.0.1` | Address the bridge binds. Blank falls back to the default. |

A `dev-tools` build started without `PV_MCP_BRIDGE_ENABLE=1` does not open the
port, and logs `MCP bridge not started`.

```sh
cd apps/desktop
PV_MCP_BRIDGE_ENABLE=1 pnpm tauri dev --config src-tauri/tauri.dev.conf.json --features dev-tools
```

The `tauri.dev.conf.json` overlay also enables `withGlobalTauri`, which
`webview_execute_js` needs to reach command handlers.

### Build the binary without the Tauri CLI

`generate_context!` compiles `withGlobalTauri` in rather than reading it at
startup: it embeds the global API script only when the config it sees has the
flag set (`tauri-codegen/src/context.rs`, `config.app.with_global_tauri`). That
config comes from three places:

- `apps/desktop/src-tauri/tauri.conf.json`
- the per-platform overlays, such as `tauri.windows.conf.json`
- the `TAURI_CONFIG` environment variable, merged over both

`tauri.dev.conf.json` is none of those. The `--config` flag of the Tauri CLI is
what puts its contents into `TAURI_CONFIG`.

So a plain `cargo build -p desktop_shell --features dev-tools` produces a binary
with **no** `window.__TAURI__`. The bridge still starts and still answers
`get_window_info`, but every `webview_execute_js` call times out, because the
result-return the plugin injects goes through `window.__TAURI__`. Merge the
overlay explicitly to get a drivable binary:

```sh
cd apps/desktop/src-tauri
TAURI_CONFIG="$(cat tauri.dev.conf.json)" \
  cargo build -p desktop_shell --features dev-tools
```

```powershell
cd apps\desktop\src-tauri
$env:TAURI_CONFIG = Get-Content -Raw tauri.dev.conf.json
cargo build -p desktop_shell --features dev-tools
```

`TAURI_CONFIG` is a `rerun-if-env-changed` input, so switching it in or out
rebuilds `desktop_shell` rather than reusing the previous binary. Before you
launch, confirm that the build picked the overlay up. The global API script is a
literal in the binary:

```sh
grep -ac __TAURI_IIFE__ target/debug/desktop_shell   # 1 with the overlay, 0 without
```

A debug binary loads the UI from `devUrl` (`http://localhost:5173`), so Vite must
stay up for as long as the app does. On Windows, `Start-Process` over SSH reaps
both Vite and the app when the SSH session closes, so launch each under
`schtasks` or another session-persistent launcher.

### Windows host, WSL client

`scripts\win-native-dev.ps1 -McpBridge` launches the same overlay and sets the
variable:

```powershell
cd C:\dev\astro-plan
.\scripts\win-native-dev.ps1 -McpBridge
```

Under mirrored WSL networking, `localhost` inside WSL reaches Windows loopback,
so the default bind is enough and the client connects to `localhost:9223`. Under
NAT networking the client reaches the host through the gateway address, which a
loopback-bound socket does not accept, so the bind has to widen:

```powershell
.\scripts\win-native-dev.ps1 -McpBridge -McpBridgeBind 0.0.0.0
```

## What binding off loopback exposes

The bridge has no authentication, and `webview_execute_js` reaches every command
handler through `window.__TAURI__.core.invoke`. Anything that can open a socket
to the bind address therefore has full control of the app and of every library
the app can write to. `0.0.0.0` accepts connections from every host that can
route to the machine, so on a shared or untrusted network that is the whole
network.

Pick the narrowest address the client can actually reach, and prefer switching
WSL to mirrored networking over widening the bind. The startup log records the
address, and warns rather than informs when it is not loopback.
