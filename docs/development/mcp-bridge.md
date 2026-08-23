# MCP bridge

The MCP bridge is a WebSocket server inside the desktop app that lets an agent
drive the running UI. `@hypothesi/tauri-mcp-server` connects to it and exposes
tool calls such as `driver_session` and `webview_execute_js`, which is how
automated journeys and manual validation sessions click through the real app
against a real backend. It listens on port 9223.

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
