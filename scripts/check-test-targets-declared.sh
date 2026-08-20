#!/usr/bin/env bash
# Test-target declaration guard — ensures no test file is silently never built.
#
# Invariant: every `*.rs` file at the ROOT of a test package (a package whose
# integration tests live beside its Cargo.toml rather than under `tests/`) has a
# matching `[[test]] path = "..."` entry in that Cargo.toml.
#
# Why this cannot be left to Cargo: auto-discovery only finds `<pkg>/tests/*.rs`.
# For `tests/contract/`, the test files ARE the package root, so an undeclared
# file compiles never, runs never, and reports nothing. Three files sat that way
# for two months, one of them hiding a wrong constant in a shipped validator.
#
# Usage:
#   bash scripts/check-test-targets-declared.sh   # exits 0 on pass, 1 on fail
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

# Test packages keeping their test targets at the package root.
PACKAGES=(tests/contract)

fail=0

for pkg in "${PACKAGES[@]}"; do
  manifest="$pkg/Cargo.toml"
  if [ ! -f "$manifest" ]; then
    echo "ERROR: $manifest does not exist; update PACKAGES in $0."
    fail=1
    continue
  fi

  # Declared paths: the value of every `path = "..."` under a [[test]] table.
  declared=$(awk '
    /^\[\[test\]\]/ { in_test = 1; next }
    /^\[/           { in_test = 0 }
    in_test && /^[[:space:]]*path[[:space:]]*=/ {
      match($0, /"[^"]+"/)
      print substr($0, RSTART + 1, RLENGTH - 2)
    }
  ' "$manifest" | sort)

  # Candidates: *.rs directly in the package root. `support/` and any other
  # subdirectory is shared helper code pulled in with `#[path]`, not a target.
  found=$(find "$pkg" -maxdepth 1 -name '*.rs' -type f -exec basename {} \; | sort)

  undeclared=$(comm -23 <(echo "$found") <(echo "$declared") || true)
  missing=$(comm -13 <(echo "$found") <(echo "$declared") || true)

  if [ -n "$undeclared" ]; then
    echo "ERROR: $pkg has test file(s) with no [[test]] entry in Cargo.toml."
    echo "       Cargo never builds these, so their assertions never run:"
    echo "$undeclared" | sed 's/^/         /'
    fail=1
  fi

  if [ -n "$missing" ]; then
    echo "ERROR: $manifest declares [[test]] path(s) that do not exist:"
    echo "$missing" | sed 's/^/         /'
    fail=1
  fi
done

if [ "$fail" -ne 0 ]; then
  exit 1
fi

echo "OK: every root-level test file in ${PACKAGES[*]} is declared."
