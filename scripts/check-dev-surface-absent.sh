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
# Exit 0 requires a positive determination from every check. Each one first
# asserts that its input exists and that its query returned a record it can
# recognise, and fails when it did not: a missing manifest, an unavailable
# `cargo`, an empty or unrecognisable `cargo tree` result, and an empty
# capability directory are violations, not absence.

set -euo pipefail

cd "$(dirname "$0")/.."

FAILED=0
fail() {
  echo "FAIL: $*" >&2
  FAILED=1
}

MANIFESTS=(apps/desktop/src-tauri/Cargo.toml crates/app/core/Cargo.toml)
# Message-only label for the glob expanded in check 3.
CAPABILITY_GLOB='apps/desktop/src-tauri/capabilities/*.json'
GATED_FEATURES='dev-tools|e2e'
# Plugin crates whose presence in the graph means the surface is linked in.
GATED_CRATES='tauri-plugin-mcp-bridge|tauri-plugin-webdriver'
# Permission prefixes owned by those plugins.
GATED_PERMISSIONS='mcp-bridge:'

# --- 1. static: no default feature enables a gated feature ---
DEFAULT_LINES=""
for manifest in "${MANIFESTS[@]}"; do
  if [ ! -f "$manifest" ]; then
    fail "manifest $manifest is missing, so its default features were never read"
    continue
  fi
  DEFAULT_LINES+=$(grep -InE '^[[:space:]]*default[[:space:]]*=[[:space:]]*\[' "$manifest" || true)
done
if grep -qE "$GATED_FEATURES" <<<"$DEFAULT_LINES"; then
  fail "a default feature enables one of: $GATED_FEATURES"
fi

# --- 2. resolved: the default feature graph of the app crate ---
TREE=$(cargo tree -p desktop_shell -e features -f '{p} {f}' 2>&1) || {
  fail "cargo tree could not resolve the desktop_shell feature graph"
  TREE=""
}
# The root package line is the record that proves the query ran and was
# understood. Its absence means the graph was never observed.
ROOT_LINES=$(grep -E 'desktop_shell v' <<<"$TREE" || true)
if [ -z "$ROOT_LINES" ]; then
  fail "cargo tree returned no desktop_shell entry, so the feature graph was not observed"
else
  if grep -qE "$GATED_FEATURES" <<<"$ROOT_LINES"; then
    fail "the default feature graph resolves one of: $GATED_FEATURES"
  fi
  if grep -qE "($GATED_CRATES) v" <<<"$TREE"; then
    fail "the default feature graph links one of: $GATED_CRATES"
  fi
fi

# --- 3. capabilities: no shipped grant names a gated plugin ---
# build.rs narrows the capability glob to `capabilities/*.json` without
# `dev-tools`, so only top-level files reach a default-feature build.
shopt -s nullglob
CAPABILITIES=(apps/desktop/src-tauri/capabilities/*.json)
shopt -u nullglob
if [ "${#CAPABILITIES[@]}" -eq 0 ]; then
  fail "no capability file matches $CAPABILITY_GLOB, so no grant was read"
elif grep -lE "\"($GATED_PERMISSIONS)" "${CAPABILITIES[@]}" | grep -q .; then
  fail "a shipped capability file grants a gated plugin permission"
fi

if [ "$FAILED" -ne 0 ]; then
  echo "dev surface gate: BLOCKED" >&2
  exit 1
fi
echo "dev surface gate: pass (${#CAPABILITIES[@]} capability files, $(grep -c . <<<"$TREE") feature-graph lines observed)"
