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

.EXAMPLE
  pwsh -File scripts\win-native-dev.ps1            # real backend, polling on
  pwsh -File scripts\win-native-dev.ps1 -Mocks     # fixtures, no backend
#>
[CmdletBinding()]
param(
  [switch]$Mocks,
  [switch]$NoPolling
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

Write-Host "Repo:        $repoRoot"
Write-Host "Backend:     $(if ($Mocks) { 'MOCKS (fixtures, no Rust backend)' } else { 'REAL (Rust backend wired)' })"
Write-Host "Watch:       $(if ($NoPolling) { 'native notifications' } else { 'polling (WSL-edit safe)' })"
Write-Host "Starting tauri dev... (first build compiles the Rust workspace; later builds are incremental)"

Set-Location $desktop
# Merge the dev-only overlay (src-tauri/tauri.dev.conf.json) which enables
# withGlobalTauri for the MCP Bridge plugin's webview channel. withGlobalTauri
# MUST stay out of the base tauri.conf.json (production security surface); it is
# applied here in dev only — mirrors the `just tauri-dev` invocation.
# `--features dev-tools` compiles the MCP bridge in; without it the plugin is
# not linked and nothing listens on 9223. The bridge binds 127.0.0.1 unless
# PV_MCP_BRIDGE_BIND overrides it (see docs/development/windows-native-rust-dev.md).
pnpm tauri dev --config src-tauri/tauri.dev.conf.json --features dev-tools
