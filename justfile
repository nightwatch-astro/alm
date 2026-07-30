default:
    @just --list

# Run Rust workspace tests and package tests when present.
# cargo-nextest runs the unit/integration suite (faster, per-test output);
# `cargo test --doc` runs doctests, which nextest does not execute.
test:
    cargo nextest run --workspace
    cargo test --workspace --doc
    pnpm -r --if-present test
    node scripts/check-eslint-baseline.test.mjs
    node scripts/check-mock-baseline.test.mjs

# Lint and format. This recipe is the single local definition of the lint set;
# the root package.json `lint` script delegates here so the two cannot drift.
# The extra `rustfmt` call covers bootstrap/specta.rs, which `cargo fmt` cannot
# reach because it is `include!`d, not `mod`-declared. `lint:tests` is listed
# separately because `tests/` sits outside the pnpm workspace globs
# (`apps/*`, `packages/*`), so `pnpm -r lint` never reaches it.
lint:
    cargo fmt --all --check
    rustfmt --edition 2021 --check apps/desktop/src-tauri/src/bootstrap/specta.rs
    cargo clippy --workspace --all-targets -- -D warnings
    # The workspace clippy above never enables `dev-tools`, leaving the
    # developer-mode surface (commands/dev.rs) unlinted (#1165). Release
    # binaries still omit the feature; this only lints the daily dev build.
    cargo clippy -p desktop_shell --features dev-tools --all-targets -- -D warnings
    pnpm -r --if-present lint
    pnpm run lint:tests
    pre-commit run --all-files

# Build the Rust workspace and package workspaces when present.
build:
    cargo build --workspace
    pnpm -r --if-present build

# DB boundary ratchet — fail if production sqlx query/exec sites OUTSIDE
# crates/persistence/* EXCEED the checked-in baseline. Counts may only shrink.
# NOT YET wired into CI (see docs/development/persistence-layer-hardening.md
# and the PR that introduced this target for why). Regenerate the baseline
# after a refactor materially shuffles query sites:
#   bash scripts/check-db-boundary.sh --generate
db-boundary:
    bash scripts/check-db-boundary.sh

# Dead-caller ratchet — fail if a module-level `pub fn` in crates/ has no
# production caller, i.e. only its own tests reach it. rustc cannot catch this:
# `dead_code` does not fire on `pub` items in a lib crate, and `unreachable_pub`
# flags the opposite condition. Shrink-only baseline; refresh after wiring up or
# deleting debt:
#   bash scripts/check-dead-callers.sh --generate
dead-callers:
    bash scripts/check-dead-callers.sh

# Lifecycle string-comparison ratchet — sealed at zero. Raw `.lifecycle == "..."`
# comparisons must use typed ProjectState predicates instead.
lifecycle-strings:
    bash scripts/check-lifecycle-strings.sh

# Hot-read ratchet — fail if per-operation hot-read call sites in the inbox /
# plan-apply / watcher paths exceed the checked-in baseline. Shrink-only:
# any new site fails CI. After removing a site, regenerate the baseline:
#   bash scripts/check-hot-read-ratchet.sh --generate
hot-read:
    bash scripts/check-hot-read-ratchet.sh

# .pv-* CSS selector ratchet — sealed at zero. e2e test files must use
# [data-testid="..."] or [data-kind="..."] selectors, not class selectors.
pv-selector-ratchet:
    bash scripts/check-pv-selector-ratchet.sh

# Periodic hygiene sweeps. NOT CI gates and deliberately so: these surface debt
# to triage, not per-PR correctness, and a noisy blocking gate gets suppressed.
# Run them when you want a health read, not on every push.
#
# Tools install on demand:
#   cargo install cargo-machete cargo-public-api
#
# Deliberately NOT included, so nobody re-adds them:
#   cargo-udeps  - same job as machete, and needs a nightly toolchain
#   cargo-shear  - same job as machete; one dependency checker is enough,
#                  three reporting overlapping findings guarantees all three
#                  get ignored
#   warnalyzer   - NOT dead (an earlier note here said so, wrongly: crates.io
#                  shows a 2021 release, but the repo was pushed 2024-09 and
#                  has a SCIP backend that works on stable). Not wired up only
#                  because the SCIP route is covered below.
#
# WORTH EVALUATING, not yet wired: cargo-workspace-unused-pub does exactly what
# scripts/check-dead-callers.sh approximates, but semantically -- it builds a
# SCIP index via rust-analyzer instead of grepping, so re-exports, out-of-line
# test modules and comments cannot fool it (all four bugs found in that script
# on 2026-07-20 were grep artifacts). Caveats: 0.1.0, ~1.3k downloads, methods
# only, no false-positive suppression yet, and index generation is slow enough
# that it is a periodic sweep rather than a gate. warnalyzer and clippy issue
# 5828 both confirm SCIP + rust-analyzer is the only workable approach --
# rustc's dead_code is per-crate by construction and cannot see a workspace.

# Unused dependencies declared in Cargo.toml but never used.
hygiene-deps:
    @command -v cargo-machete >/dev/null 2>&1 || { echo "cargo-machete not installed: cargo install cargo-machete"; exit 1; }
    cargo machete

# `pub` items that are not actually reachable from outside their crate, i.e.
# should be pub(crate). Narrowing them lets rustc's own dead_code lint fire on
# the unused ones for free -- which is the compiler-native half of what
# scripts/check-dead-callers.sh approximates textually. Measured 0 on
# persistence_core on 2026-07-24 (post-split); other crates are unmeasured.
#
# `pub` items that should be pub(crate), across the workspace.
hygiene-pub:
    cargo clippy --workspace --all-targets --message-format=short -- -A warnings -W unreachable_pub

# Public API surface per crate. Not a pass/fail check -- a read of what each
# crate exposes. Large surface that the workspace never consumes is the same
# debt the dead-caller ratchet tracks, seen from the other side.
#
# Public API surface of one crate (read, not pass/fail).
hygiene-api CRATE:
    @command -v cargo-public-api >/dev/null 2>&1 || { echo "cargo-public-api not installed: cargo install cargo-public-api"; exit 1; }
    cargo public-api -p {{CRATE}}

# All non-interactive hygiene sweeps.
hygiene: hygiene-deps hygiene-pub

# Regenerate the sqlx offline query cache (.sqlx/) for compile-time verification.
# Requires a DATABASE_URL pointing at a migrated SQLite db, or run after the
# crate's migrations have been applied. Commit the resulting .sqlx/ dir so CI can
# build with SQLX_OFFLINE=true. NOTE: as of this writing the codebase uses only
# runtime-checked queries (sqlx::query/query_as, not the query!/query_as! macros),
# so `cargo sqlx prepare` currently has nothing to capture and is a no-op until a
# repository adopts the compile-time-checked macros. See
# docs/development/persistence-layer-hardening.md.
sqlx-prepare:
    cargo sqlx prepare --workspace -- --all-targets

# Type-check TypeScript workspaces when present.
typecheck:
    pnpm -r --if-present typecheck

# Generate contract TypeScript declarations from JSON Schema.
contracts-build:
    pnpm --filter @astro-plan/contracts build

# Verify the vanilla-extract theme contract file is up to date with the DTCG
# token sources in tokens/. Fails with a diff if themes.css.ts has drifted.
ve-themes-check:
    pnpm --filter @astro-plan/desktop ve:themes:check

# Regenerate Rust-derived JSON contract schemas and the tauri-specta
# TypeScript bindings, then fail when either is out of sync with the
# committed tree. Wire this into CI for spec 002 + onward.
check-generated:
    cargo run -q -p contracts_core --bin generate-contracts
    cargo test -q -p desktop_shell --features dev-tools --test bindings
    git diff --exit-code specs/*/contracts/*.generated.json apps/desktop/src/bindings/

# Full pre-merge gate: lint + tests + typecheck + generated-artifact drift +
# DB/dead-caller/hot-read/lifecycle-strings boundary ratchets.
check: lint test typecheck check-generated db-boundary dead-callers hot-read lifecycle-strings

# Placeholder fixture check hook.
fixtures-check:
    pnpm fixtures:check

# Start the desktop frontend dev server.
dev:
    pnpm --filter @astro-plan/desktop dev

# Start the Tauri desktop app in development mode (Rust + frontend).
# The dev overlay enables `withGlobalTauri`, required by the (debug-only) MCP
# bridge plugin; it is never applied to release builds.
tauri-dev:
    cd apps/desktop && pnpm tauri dev --config src-tauri/tauri.dev.conf.json

# Clean build artifacts
clean:
    cargo clean

# Speckit workflows (requires specify CLI)
speckit-full FEATURE INTEGRATION="codex":
    specify workflow run speckit-full -i feature_name={{FEATURE}} -i integration={{INTEGRATION}}

speckit-bugfix ISSUE INTEGRATION="codex":
    specify workflow run speckit-bugfix -i issue_number={{ISSUE}} -i integration={{INTEGRATION}}

speckit-tinyspec FEATURE INTEGRATION="codex":
    specify workflow run speckit-tinyspec -i feature_name={{FEATURE}} -i integration={{INTEGRATION}}

speckit-resume RUN_ID:
    specify workflow resume {{RUN_ID}}

speckit-status:
    specify workflow status

# Run all Rust workspace tests (integration layer).
test-integration:
    cargo nextest run --workspace
    cargo test --workspace --doc

# Run end-to-end tests against the real Tauri backend.
test-e2e:
    cd apps/desktop && pnpm test:e2e:real

# Print a PV_DEV_URL whose port is derived from this checkout's absolute path.
#
# Export it in BOTH halves of a local Layer-2/Layer-3 run — the `vite preview`
# that serves `dist` and the `cargo nextest` that launches the app — so two
# worktrees never resolve the same frontend port. Without it both default to
# `tauri.conf.json`'s devUrl and the second lane's app loads the first lane's
# build, which surfaces as hanging or wrong-branch journeys rather than as a
# port conflict (issue #1409).
#
# The port is a hash of the checkout path, so it is stable across runs of the
# same worktree and differs between worktrees. Serve with `--strictPort` so a
# hash collision fails loudly instead of silently sharing a port.
e2e-dev-url:
    #!/usr/bin/env bash
    set -euo pipefail
    root="$(git rev-parse --show-toplevel)"
    # 5200-5455: above vite's 5173 default and playwright.config.ts's 5183.
    port=$(( 5200 + 0x$(printf %s "$root" | shasum | cut -c1-4) % 256 ))
    echo "http://localhost:${port}"

# Run the inbox perf measurement harness and print baseline JSON lines.
# Set PERF_N to control fixture size (default 500; use 5000 for a deeper run).
# Output is machine-readable JSON — one line per scenario — suitable for
# pasting before/after numbers into PR descriptions.
perf-bench:
    cargo run --release -p perf-bench

# Check measured hot-path query counts against the committed baseline.
# HARD-fails on any sqlx_stmts increase; WARN-only on wall_ms > 1.5× budget.
# Use --generate to record a fresh baseline after a justified query-count change.
perf-check:
    bash scripts/check-perf-baseline.sh

# Measure Rust test coverage via cargo-llvm-cov and print a per-file summary.
# Requires cargo-llvm-cov and llvm-tools-preview:
#   rustup component add llvm-tools-preview
#   cargo install cargo-llvm-cov
# Writes lcov.info to the workspace root (gitignored).
coverage:
    cargo llvm-cov --workspace --lcov --output-path lcov.info --doctests
    cargo llvm-cov report --summary-only

# Background Rust checker: re-runs clippy on every file save via bacon.
# Install: cargo install bacon
# Switch jobs interactively inside bacon with the job keys listed in bacon.toml
# (check / clippy / test / doc).
watch:
    bacon
