<#
.SYNOPSIS
  Launch Astro Library Manager as a native Windows Tauri app for live development.

.DESCRIPTION
  Standard procedure for native Rust + Tauri development on Windows when the
  canonical repo lives in WSL. Run this from a *Windows* checkout on an NTFS
  drive (e.g. C:\dev\astro-plan) -- NOT from a \\wsl$ / \\wsl.localhost UNC path
  (Windows node/cargo cannot build against UNC working directories).

  See docs/development/windows-native-rust-dev.md for the full runbook.

.PARAMETER Mocks
  When set, runs the frontend against in-memory fixtures (VITE_USE_MOCKS=true)
  and does not exercise the Rust backend. Default is the real backend.

.PARAMETER NoPolling
  Disable Vite file-watch polling. Polling is ON by default so edits made from
  the WSL side (via \\wsl$ or /mnt/c) reliably trigger HMR; turn it off if you
  only ever edit from Windows-native tools and want lower idle CPU.

.PARAMETER McpBridge
  Start the MCP bridge (PV_MCP_BRIDGE_ENABLE=1) so an agent can drive this app.
  Off by default: the bridge is unauthenticated and reaches every command
  handler. Set -McpBridgeBind to widen its 127.0.0.1 bind for a WSL-side agent.

.PARAMETER McpBridgeBind
  Address the MCP bridge binds (PV_MCP_BRIDGE_BIND), e.g. 0.0.0.0 so the WSL NAT
  gateway address reaches it. Requires -McpBridge.

.EXAMPLE
  pwsh -File scripts\win-native-dev.ps1            # real backend, polling on
  pwsh -File scripts\win-native-dev.ps1 -Mocks     # fixtures, no backend
  pwsh -File scripts\win-native-dev.ps1 -McpBridge -McpBridgeBind 0.0.0.0
#>
[CmdletBinding()]
param(
  [switch]$Mocks,
  [switch]$NoPolling,
  [switch]$McpBridge,
  [string]$McpBridgeBind
)

$ErrorActionPreference = 'Stop'

# Resolve repo root from this script's location (scripts/ -> repo root).
$repoRoot = Split-Path -Parent $PSScriptRoot
$desktop  = Join-Path $repoRoot 'apps\desktop'

if ($repoRoot -like '\\*') {
  throw "This repo is on a UNC path ($repoRoot). Clone to a local NTFS path (e.g. C:\dev\astro-plan) and run from there."
}
if (-not (Test-Path $desktop)) { throw "apps\desktop not found under $repoRoot" }

# VITE_USE_MOCKS must be a real environment variable: apps/desktop/vite.config.ts
# resolves it from process.env via a `define`, so the .env file alone is ignored.
$env:VITE_USE_MOCKS = if ($Mocks) { 'true' } else { 'false' }

# Polling makes Vite/chokidar catch writes that arrive over the WSL<->Windows
# filesystem bridge (ReadDirectoryChangesW notifications are unreliable there).
if ($NoPolling) { Remove-Item Env:CHOKIDAR_USEPOLLING -ErrorAction SilentlyContinue }
else { $env:CHOKIDAR_USEPOLLING = 'true' }

if ($McpBridgeBind -and -not $McpBridge) {
  throw "-McpBridgeBind requires -McpBridge (the bridge does not start without it)."
}
if ($McpBridge) {
  $env:PV_MCP_BRIDGE_ENABLE = '1'
  if ($McpBridgeBind) { $env:PV_MCP_BRIDGE_BIND = $McpBridgeBind }
} else {
  Remove-Item Env:PV_MCP_BRIDGE_ENABLE -ErrorAction SilentlyContinue
}

Write-Host "Repo:        $repoRoot"
Write-Host "Backend:     $(if ($Mocks) { 'MOCKS (fixtures, no Rust backend)' } else { 'REAL (Rust backend wired)' })"
Write-Host "Watch:       $(if ($NoPolling) { 'native notifications' } else { 'polling (WSL-edit safe)' })"
Write-Host "MCP bridge:  $(if ($McpBridge) { "ON (bind $(if ($McpBridgeBind) { $McpBridgeBind } else { '127.0.0.1' }))" } else { 'OFF (-McpBridge to enable)' })"
Write-Host "Starting tauri dev... (first build compiles the Rust workspace; later builds are incremental)"

Set-Location $desktop
# Merge the dev-only overlay (src-tauri/tauri.dev.conf.json) which enables
# withGlobalTauri for the MCP Bridge plugin's webview channel. withGlobalTauri
# MUST stay out of the base tauri.conf.json (production security surface); it is
# applied here in dev only — mirrors the `just tauri-dev` invocation.
# `--features dev-tools` compiles the MCP bridge in; without it the plugin is
# not linked. Compiled in is not started: `-McpBridge` sets PV_MCP_BRIDGE_ENABLE=1,
# and nothing listens on 9223 without it (see
# docs/development/windows-native-rust-dev.md).
pnpm tauri dev --config src-tauri/tauri.dev.conf.json --features dev-tools
