# TinySpec: A test invocation that selects zero tests fails its CI job

**Branch**: fix/gate1-ci-zero-test-selection
**Date**: 2026-08-23
**Status**: draft
**Complexity**: small

## What

Seven CI test invocations exit 0 when their selection matches zero tests, so the
job reports success having executed nothing. Six are `cargo nextest`
invocations in `.github/workflows/e2e.yml` carrying `--no-tests=warn`; one is
the scoped-crate `cargo nextest` invocation in `.github/workflows/ci.yml`
carrying `--no-tests=pass`. The frontend lane has the same defect one layer
down: `apps/desktop/vitest.config.ts` sets `passWithNoTests: true`, and the
shard-coverage integrity check in `e2e.yml` compares two counts that are equal
when both are zero.

This is a CI-signal defect. No runtime path in the shipped application is
affected.

## The rule (decision)

**Every CI test invocation either fails when its selection is empty, or asserts
a positive selected count. `--no-tests=pass` survives only on a path that
announces the exemption by name.**

### Rust test invocations spell `--no-tests=fail`

All six `e2e.yml` invocations carry an explicit `--no-tests=fail`, including the
dispatch-only macOS shard. `fail` is written out rather than left to nextest's
`auto` default because `taiki-e/install-action@nextest` installs the latest
release, so the default is not pinned by the workflow.

### The scoped `ci.yml` invocation asserts a count before it may pass

A scoped crate set can legitimately select zero runnable tests (an
`e2e_tests`-only change, whose tests are `#[ignore]`d for `e2e.yml`). The step
therefore lists the scoped selection first, prints the count, and takes the
zero-count path only as a named, log-announced exemption. The general path
carries no `--no-tests=pass`.

### Vitest fails on an empty selection

`passWithNoTests` is absent from `apps/desktop/vitest.config.ts`, so an empty
selection exits non-zero. The `vitest related` fallback in `ci.yml` keys on the
`No test files found` message, not on the exit code, so it continues to fall
back to the full suite rather than propagating the failure.

### The shard-coverage check asserts a positive total

`TOTAL` must be greater than zero before `TOTAL` and `UNION` are compared, so an
empty or truncated archive fails the check instead of satisfying it by equality.

## Per-finding verdict at head `efaaf57a0d800a91efab5437d6d4eac32cc78ad5`

| Bead | Site at head | Reachable | Verdict |
| --- | --- | --- | --- |
| `astro-plan-3v3r.17.20` | `.github/workflows/e2e.yml:830` (`smoke-ubuntu`) | YES — renaming any of the three journeys in the `--filter-expr` matches zero tests; `--no-tests=warn` exits 0 and the branch-protection-required check `Real-UI smoke (L3) — ubuntu-latest` reports success. The smoke job has no shard-coverage check. | FIXED |
| `astro-plan-3v3r.17.20` | `.github/workflows/e2e.yml:1002, 1180, 1195, 1210, 1305` | YES — a stale or truncated `e2e-archive.tar.zst` makes every `--partition` shard select zero tests; all exit 0 and the aggregate gate turns green. | FIXED |
| `astro-plan-3v3r.17.20` | `.github/workflows/ci.yml:560` (`integration`, scoped branch) | YES — a `scripts/ci-affected-crates.sh` output naming only crates without runnable tests selects zero tests and exits 0. | FIXED — count assertion, exemption announced |
| `astro-plan-3v3r.15.8` | duplicate of `.17.20`, same seven sites | YES | FIXED by the same change |
| M1 (no bead; found in this unit) | `apps/desktop/vitest.config.ts:53` | YES — if the `include` glob `src/**/*.{test,spec}.{ts,tsx}` stops matching (specs move out of `src/`, or the naming convention changes) the full suite runs zero specs and exits 0, defeating the `ci.yml` `No test files found` fallback. | FIXED |
| M2 (no bead; found in this unit) | `.github/workflows/e2e.yml:1155-1167` | YES — both counts are 0 on an empty archive, they compare equal, and the integrity check passes. | FIXED |
| M3 | `.github/workflows/ci.yml:614` `pnpm ... -r --if-present test` | Latent, not live — the only matched package is `packages/contracts`, whose `test` script is an `&&` chain of named node scripts and fails loudly. | DEFERRED to `astro-plan-f6qg1`; removing `--if-present` would redden a newly added script-less package |

Census-closing non-defects at the same head:

- `ci.yml:550` `cargo nextest run --workspace --profile ci` (full-suite branch) — unfiltered; nextest's default exits 4 on zero tests.
- `ci.yml:567` `cargo test --workspace --doc` — unfiltered; zero doctests is not a selection bug.
- `apps/desktop` Playwright — `apps/desktop/playwright.config.ts` sets no `passWithNoTests`, so Playwright exits 1 when it finds no tests.
- `release-gate.yml:176` `cargo test --workspace` — unfiltered and `continue-on-error: true` by design.

## Context

| File | Role |
| --- | --- |
| `.github/workflows/e2e.yml` | Modify — six `--no-tests` values, the two comments defending `warn`, the shard-coverage check |
| `.github/workflows/ci.yml` | Modify — scoped-crate step: list-and-count before running |
| `apps/desktop/vitest.config.ts` | Modify — delete `passWithNoTests` |

## Requirements

1. No `--no-tests=warn` remains in `.github/workflows/`.
2. Each of the six `e2e.yml` nextest invocations carries `--no-tests=fail`.
3. Comments describing `--no-tests=warn` as a defensive fallback are removed.
4. The `ci.yml` scoped step prints the selected test count and exits non-zero on a zero count unless it announces the exemption by name in the log.
5. `apps/desktop/vitest.config.ts` sets no `passWithNoTests`; `pnpm exec vitest run <no-match>` exits non-zero.
6. The `ci.yml` `vitest related` fallback still runs the full suite when no dependent spec exists, keyed on the `No test files found` message rather than the exit code.
7. The shard-coverage check exits non-zero when `TOTAL` is 0, with a distinct `::error::` message.
8. Every edited step remains shell-portable and keeps its `shell: bash`.

## Tasks

- [x] Replace `--no-tests=warn` with `--no-tests=fail` at the six `e2e.yml` sites
- [x] Remove the two comments defending `--no-tests=warn`
- [x] Add the list-and-count guard to the `ci.yml` scoped-crate step
- [x] Delete `passWithNoTests` from `apps/desktop/vitest.config.ts`
- [x] Add the `TOTAL -gt 0` assertion to the shard-coverage check
- [x] Record the zero-selection exit code at each changed site, before and after
- [x] File a bead for M3 (`astro-plan-f6qg1`)

## Done When

- [ ] `git grep -c 'no-tests=warn' .github/workflows/` returns no matches
- [ ] `git grep -c passWithNoTests apps packages` returns no matches
- [ ] `cargo nextest run -p safe-filename -E 'test(=zzz_no_such_test)' --no-tests=fail` exits 4
- [ ] `pnpm exec vitest run zzz-no-such-spec` in `apps/desktop` exits non-zero
- [ ] The shard-coverage body run with `TOTAL=0 UNION=0` exits 1
- [ ] `actionlint` accepts both edited workflows
