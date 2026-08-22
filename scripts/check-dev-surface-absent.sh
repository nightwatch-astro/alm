#!/usr/bin/env bash
# Copyright (C) 2024-2026 Sjors Robroek
# SPDX-License-Identifier: AGPL-3.0-only
#
# Asserts the developer-mode and automation surfaces are absent from a
# default-feature build of the desktop app (Constitution Principle V).
#
# Checks, all of which exit non-zero on violation:
#   1. no `default = [...]` feature list enables `dev-tools` or `e2e`
#   2. the resolved default feature graph of `desktop_shell` pulls neither
#      feature and neither gated plugin crate
#   3. no shipped capability file grants a permission from a gated plugin
#
# `cargo tree` failure is a violation, not a pass: a gate that cannot observe
# the feature graph reports nothing rather than reporting absence.

set -euo pipefail

cd "$(dirname "$0")/.."

FAILED=0
fail() {
  echo "FAIL: $*" >&2
  FAILED=1
}

MANIFESTS=(apps/desktop/src-tauri/Cargo.toml crates/app/core/Cargo.toml)
GATED_FEATURES='dev-tools|e2e'
# Plugin crates whose presence in the graph means the surface is linked in.
GATED_CRATES='tauri-plugin-mcp-bridge|tauri-plugin-webdriver'
# Permission prefixes owned by those plugins.
GATED_PERMISSIONS='mcp-bridge:'

# --- 1. static: no default feature enables a gated feature ---
if grep -RInE '^[[:space:]]*default[[:space:]]*=[[:space:]]*\[' "${MANIFESTS[@]}" \
  | grep -qE "$GATED_FEATURES"; then
  fail "a default feature enables one of: $GATED_FEATURES"
fi

# --- 2. resolved: the default feature graph of the app crate ---
TREE=$(cargo tree -p desktop_shell -e features -f '{p} {f}') || {
  fail "cargo tree could not resolve the desktop_shell feature graph"
  TREE=""
}
if [ -n "$TREE" ]; then
  if grep -E 'desktop_shell v' <<<"$TREE" | grep -qE "$GATED_FEATURES"; then
    fail "the default feature graph resolves one of: $GATED_FEATURES"
  fi
  if grep -qE "($GATED_CRATES) v" <<<"$TREE"; then
    fail "the default feature graph links one of: $GATED_CRATES"
  fi
fi

# --- 3. capabilities: no shipped grant names a gated plugin ---
# build.rs narrows the capability glob to `capabilities/*.json` without
# `dev-tools`, so only top-level files reach a default-feature build.
if grep -lE "\"($GATED_PERMISSIONS)" apps/desktop/src-tauri/capabilities/*.json 2>/dev/null \
  | grep -q .; then
  fail "a shipped capability file grants a gated plugin permission"
fi

if [ "$FAILED" -ne 0 ]; then
  echo "dev surface gate: BLOCKED" >&2
  exit 1
fi
echo "dev surface gate: pass (no gated feature, crate, or capability in the default build)"
