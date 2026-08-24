# Research: End-to-End & Integration Testing (Full App Coverage)

**Feature**: 037-e2e-integration-testing
**Date**: 2026-06-19
**Phase**: 0 (Outline & Research)

This document resolves the open questions deferred from the spec and records the
technology decisions that the plan and tasks depend on. Format per decision:
**Decision / Rationale / Alternatives considered**.

---

## D1 — Layer 1 (real-backend integration) approach

**Decision**: Implement Layer 1 as Rust integration tests that exercise the real
`app_core` use cases through the real `SqliteLifecycleRepository` / `AppState`
against a **real, file-backed SQLite database** created per test in a
`tempfile::tempdir()`, with real `sqlx::migrate!()` migrations applied. The
`#[tauri::command]` shells stay thin (they already are — they delegate to
`app_core::*`), so tests target the use-case layer directly without booting any
Tauri runtime. Tests live in crate-level `tests/` dirs (notably
`crates/app/core/tests/`, `crates/persistence/*/tests/`) and run under
`cargo test --workspace`.

**Rationale**: The command handlers are already thin adapters
(`apps/desktop/src-tauri/src/commands/*.rs` delegate to `app_core`), so the real
behavior lives in `app_core` + `persistence/db` and is fully testable without a
webview or event loop. File-backed SQLite (vs `sqlite::memory:`) exercises real
migration application and on-disk semantics, closer to production. `cargo test`
runs natively on all three OS — this layer needs no platform-specific tooling and
carries the bulk of the assertions (cheap, deterministic).

**Alternatives considered**:
- *Tauri mock runtime (`get_ipc_response`)* — runs real command/serde/state but
  stubs the webview. Rejected as the primary mechanism because it adds Tauri test
  scaffolding for little gain over calling `app_core` directly; the IPC
  dispatch/serialization path is instead proven by Layer 2 (real binary).
- *In-memory SQLite only* — already used in unit tests; kept for fast unit cases
  but Layer 1 prefers file-backed to catch migration/disk issues.

---

## D2 — External network boundary (SIMBAD)

**Decision**: For backend code that calls the external SIMBAD service (target
resolution, spec 035), stub **only at the HTTP boundary** using `wiremock`
(already in the Rust ecosystem; add as dev-dependency), so the application's own
request-building, parsing, caching, and fallback logic executes for real. Provide
canned SIMBAD response fixtures (success, ambiguous, not-found, network-error).

**Rationale**: Keeps integration tests deterministic and offline-capable
(edge case + FR-003, SC-006) while still exercising real resolver logic — not a
function-level mock that would hide wiring bugs. Aligns with the recommendation's
"mock only at the network boundary" principle.

**Alternatives considered**:
- *Live SIMBAD calls* — spec 035 already added one live SC-004 smoke test; keep
  that as a separately-tagged, network-gated test, but Layer 1 must not depend on
  live network for determinism.
- *Mocking the resolver trait* — rejected; hides the exact serialization/parsing
  path we want to protect.

---

## D3 — Layer 2 (E2E) approach on Linux & Windows: reuse the 033 scaffold

> **Reconciliation note (2026-08-24)**: every `apps/desktop/e2e/` path named in
> this decision has since been deleted. The tree existed when this research was
> written; the WebdriverIO half went at `7a4f70446` (2026-06-20) and the
> remaining Playwright real-backend half at `a504a7fce` (2026-07-11). Layer 2
> now ships as the `thirtyfour` Rust suite under `crates/e2e-tests/tests/`
> (shared fixtures in `crates/e2e-tests/tests/common/`), run by
> `.github/workflows/e2e.yml` against the built `desktop_shell` under the `e2e`
> feature. The separate Playwright suite at `tests/e2e/` is mock-mode only and
> is not Layer 2. The decision text below is kept as written.

**Decision**: Complete and reuse the existing real-backend E2E scaffold rather
than rebuilding it. The scaffold already present:
- `apps/desktop/e2e/helpers/tauri-app.ts` — launches `tauri-driver` (port 4444)
  wrapping the built app, under `xvfb-run` on Linux.
- `apps/desktop/e2e/playwright.real-backend.config.ts` — Playwright config
  driving the WebDriver endpoint; Vite on `localhost:1420`.
- `apps/desktop/e2e/real-backend/*.spec.ts` — four journey skeletons, currently
  `test.skip()` pending wiring.
- `apps/desktop/e2e/helpers/db.ts` — SQLite assertion helper, currently a
  placeholder pending a query library.

Work: replace the `db.ts` placeholder with a real read-only SQLite reader
(`better-sqlite3` as a dev-dependency) pointed at the app DB
(`~/.local/share/dev.astro-plan.astro-library-manager/alm.db` and the Windows/mac
equivalents), un-skip and flesh out the journeys, and add a fresh-DB reset in
`beforeAll`. Linux uses official `tauri-driver` + `WebKitWebDriver`
(`webkit2gtk-driver`) under `xvfb`; Windows uses official `tauri-driver` +
version-matched `msedgedriver.exe`.

**Rationale**: The hard part (driver lifecycle, Playmaker config, xvfb wrapping)
already exists from spec 033. Reuse avoids duplicate harnesses and honors the
assumption in the spec. Official `tauri-driver` covers exactly Linux + Windows.

**Alternatives considered**:
- *WebdriverIO* (as in the upstream Tauri docs) — rejected; the repo already
  standardized on Playwright for E2E, and a second runner adds maintenance.
- *Rebuild from scratch* — rejected; wasteful given working scaffold.

**REVISION (US3 spike, 2026-06-20)**: The "Playwright drives `tauri-driver`"
premise does not hold — Playwright cannot connect to an external W3C WebDriver
endpoint (it only drives its own bundled browsers). The Playwright
`real-backend` config therefore only covers the Vite shell smoke (no real IPC).
Real UI→IPC→backend journeys use **WebdriverIO `remote()` + `tauri-driver`**
(the upstream Tauri approach), now proven in CI
(`apps/desktop/e2e/wdio/tauri-spike.mjs`). Notes for the journeys:
- Drive a built `desktop_shell` binary via `tauri:options.application`; do **not**
  set `browserName` (WebKitWebDriver rejects the session otherwise).
- The binary loads its frontend from the Tauri `devUrl` (:5173) even for release
  builds, so the runner serves the built `dist` there (`vite preview`); the
  frontend is real and uses real IPC.
- Linux (WebKitWebDriver) first; Windows (msedgedriver) once green. macOS stays
  best-effort per D4.

**REVISION (thirtyfour adoption, 2026-06-20)**: **thirtyfour (Rust W3C client)
+ cargo-nextest** in `crates/e2e-tests` supersedes the WebdriverIO interim.
Rationale: thirtyfour is the Rust equivalent of WebdriverIO's `remote()` against
tauri-driver — it accepts the same `tauri:options.application` capability, does
not set `browserName`, and connects to the same tauri-driver endpoint on :4444.
Choosing thirtyfour keeps the entire E2E stack in Rust alongside Layer-1,
consistent with the `rust.tauri` steering convention, and eliminates the Node.js
runner + WebdriverIO dependency from the real-UI path. The proven tauri-driver
setup from the US3 spike is unchanged: build frontend with `VITE_E2E=1`, serve
`dist` on :5173 via `vite preview` (now an explicit CI background step rather
than embedded in the wdio harness), build `desktop_shell`, then tauri-driver
drives the built binary. The real-UI journeys are scaffolded as `#[ignore]` stubs
in `crates/e2e-tests/tests/` (harness in `tests/common/`); wiring is deferred
while backend stubs clear (see D9). The legacy `apps/desktop/e2e/wdio/`
WebdriverIO scaffold remains on disk as a reference and is not deleted; CI no
longer invokes it (e2e.yml now runs `cargo nextest run -p e2e_tests --profile
e2e`). The `apps/desktop/e2e/` Playwright real-backend config also remains as a
structural reference.

---

## D4 — macOS Layer 2 (the deferred decision)

**Context**: Verified current as of 2026 — Apple ships no WebDriver for embedded
WKWebView, so the official `tauri-driver` supports **only Linux and Windows**.
Third-party W3C plugins now fill the gap by embedding a WebDriver server inside
the app (most mature: Choochmeque `tauri-plugin-webdriver` + `tauri-webdriver`
CLI; also danielraffel's two-crate `tauri-plugin-webdriver-automation`; CrabNebula
Cloud is hosted/paid). For agent-interactive debugging (not CI) the
`tauri-plugin-mcp` (P3GLEG) plugin provides a dev-only MCP interface into the
running app — gated behind the same debug/dev-only compile flag; it is NOT a CI
gate and is NOT required for Layer-2 E2E.

**Decision**:
1. **macOS Layer 1 (integration) is REQUIRED** and runs in CI on every change —
   it needs no special tooling (`cargo test`).
2. **macOS Layer 2 (real-UI E2E) is BEST-EFFORT / non-merge-blocking** for this
   feature. The plan provisions it as an **optional CI job** evaluating
   `tauri-plugin-webdriver` (Choochmeque) + `tauri-webdriver` CLI, with the same
   **thirtyfour** Rust W3C client connecting to the embedded WebDriver server
   (same `tauri:options.application` cap, no `browserName`), gated behind a
   **debug/dev-only feature flag** so the embedded WebDriver server is **never
   present in release builds** (mirrors the existing `dev-tools` compile-gate
   pattern from spec 021, per CLAUDE.md). `tauri-plugin-mcp` (P3GLEG) is an
   additional dev-only path for agent-interactive debugging, NOT a CI gate.
   If the plugin path proves stable it can be promoted to required in a
   follow-up; until then macOS Layer-2 failures do not block merge, and SC-002's
   "every required platform" treats macOS-E2E as not-required (FR-013's explicit
   not-applicable reporting applies if the plugin path is not yet adopted).

**Rationale**: Honors "decide in research" with a concrete recommended default.
Embedding a WebDriver server in the app is a real attack-surface/maintenance cost,
so gating it behind debug-only build config respects Constitution Principle V and
the release-build discipline already established for dev-tools. Making it
best-effort first avoids blocking the whole cross-platform pipeline on the most
fragile, newest-tooling path while still delivering required macOS *integration*
coverage immediately.

**Alternatives considered**:
- *Require macOS E2E now via a plugin* — rejected initially: newest tooling,
  highest flakiness risk, ships an automation surface; revisit once proven.
- *Skip macOS entirely* — rejected: macOS is a supported product OS and at minimum
  needs the required integration layer.
- *CrabNebula Cloud* — rejected for now: paid/hosted dependency; revisit only if
  self-hosted plugin proves inadequate.

---

## D5 — CI/CD topology

**Decision**: Two GitHub Actions workflows under `.github/workflows/`:

**`ci.yml`** — 3-OS matrix (`ubuntu-latest`, `windows-latest`, `macos-latest`):
- **Stage A (all 3 OS, required)**: `cargo build --workspace`, lint, then the
  Layer 1 integration suite (`cargo test --workspace`) + frontend unit tests.

Stage A gates any E2E work (fast failures first, FR-012). Caching for cargo and
pnpm. Concurrency-cancel on superseding pushes.

**`e2e.yml`** — Layer-2 real-UI E2E (Linux first; Windows Stage B to follow):
- **Layer 2 (Linux, required)**: install tauri-driver@2.0.6 via
  `taiki-e/install-action`, install cargo-nextest via `taiki-e/install-action`,
  build frontend with `VITE_E2E=1`, serve `dist` on :5173 via `vite preview`
  (background step), build `desktop_shell`, then run
  `xvfb-run -a cargo nextest run -p e2e_tests --profile e2e --no-tests=warn`.
  `--no-tests=warn` keeps the job green while all journeys are `#[ignore]` stubs;
  remove once a journey is un-ignored.
- **Stage C (macOS, future / `continue-on-error`)**: best-effort via
  `tauri-plugin-webdriver` (D4); deferred until plugin proves stable.

**Rationale**: Matches FR-010/FR-011/FR-012. Separating ci.yml (Layer 1, 3-OS,
required) from e2e.yml (Layer 2, real-UI, heavy build) keeps Layer-1 feedback
fast and Layer-2 isolated. thirtyfour + nextest keeps the test runner in Rust,
consistent with the rust.tauri steering convention.

**Alternatives considered**:
- *Single combined job* — rejected; loses per-platform attribution and fast-fail
  ordering.
- *Non-GitHub CI* — N/A; repo is on GitHub.

---

## D6 — Local developer ergonomics

**Decision**: Add/standardize task-runner targets so each layer is one command,
matching CI:
- `just test-integration` → Layer 1 (`cargo test --workspace`, integration-tagged).
- `just test-e2e` → Layer 2 smoke (`pnpm --filter @astro-plan/desktop test:e2e:real`).
- Keep existing `just test` (unit + workspace) and `just check`.
- `package.json` keeps `test:e2e:real` (already present); add a matching
  integration script if useful.
Each target preconditions on documented prerequisites and fails with a named,
actionable message when a prerequisite (e.g. `WebKitWebDriver`, `msedgedriver`,
`tauri-driver`) is missing (FR-015).

**Rationale**: FR-014/FR-015, SC-005 — same commands locally and in CI, clear
missing-prerequisite errors.

**Alternatives considered**:
- *CI-only execution* — rejected; defeats the local-feedback goal (US4).

---

## D7 — Implemented feature-area enumeration (coverage mapping basis)

**Decision**: The coverage mapping (FR-019, SC-001) is anchored to this concrete
list of implemented feature areas, derived from merged specs (001–035) and the
desktop UI feature folders (`apps/desktop/src/features/*`). Each area needs ≥1
Layer-1 test; primary areas additionally appear in a Layer-2 smoke journey.

| # | Feature area | Source specs | Layer 2 smoke? |
|---|---|---|---|
| 1 | First-run source setup | 003, 010 | Yes |
| 2 | Native filesystem controls | 004 | via plans |
| 3 | Inbox mixed-folder split | 005, 013 | Yes |
| 4 | Inventory / data lifecycle state | 002, 006 | Yes |
| 5 | Calibration matching & masters | 007 | Yes |
| 6 | Sessions (acquisition/calibration) | 001 | Yes |
| 7 | Projects: create/onboard/edit | 008 | Yes |
| 8 | Project lifecycle model | 009 | Yes |
| 9 | Project manifests & notes | 024 | Yes |
| 10 | Processing tool launch | 011 | smoke (no real launch) |
| 11 | Processing artifact observation | 012 | Yes |
| 12 | Target lookup from FITS OBJECT | 013 | Yes |
| 13 | Target identity, history, notes | 023 | Yes |
| 14 | SIMBAD target resolution | 035 | Yes (boundary-mocked) |
| 15 | Token pattern builder | 015 | Yes |
| 16 | Source protection defaults | 016 | via plans |
| 17 | Cleanup & archive review plans | 017 | Yes |
| 18 | Filesystem plan application | 025 | Yes (mutation+audit) |
| 19 | Settings / configuration model | 018 | Yes |
| 20 | Bottom log viewer | 019 | Yes |
| 21 | Router & URL state | 020 | Yes (all screens load) |
| 22 | Audit event model | (cross-cutting) | asserted in #18 |

Catalog index licensing (014), developer contract diagnostics (021), and the
design/UI-implementation specs (022, 026–032) are infrastructure/UI-only and are
covered implicitly via screen-load smoke (#21) and the design system; they do not
get dedicated Layer-1 backend tests. Spec 033 (validation bugfix) and 036 (legacy
target retirement) are remediation — their behaviors fold into the areas above.

**Rationale**: Makes "100% of implemented feature areas" auditable (resolves the
spec's fuzzy "roughly 001–035"). The exact per-test mapping is finalized in
`tasks.md`.

**Alternatives considered**:
- *Per-spec coverage (one test per spec number)* — rejected; many specs are UI
  iterations of the same area, which would inflate and confuse the matrix.

---

## D8 — Seeded-regression validation scope (SC-007)

**Decision**: Scope SC-007 to a **one-time, representative validation during
implementation**, not an ongoing mutation-testing gate. For ~3–5 covered
behaviors (e.g. drop a persisted field, rename a payload key, skip an audit
write), temporarily introduce the regression, confirm a Layer-1 or Layer-2 test
fails, then revert. Record the exercise in the feature's implementation notes.

**Rationale**: Proves the suites actually catch regressions (the whole point)
without committing to maintaining a mutation-testing harness, which is
out-of-scope cost. Keeps SC-007 verifiable and bounded.

**Alternatives considered**:
- *Full mutation testing (e.g. `cargo-mutants`)* — rejected as scope creep for
  this feature; could be a future enhancement.
- *Drop SC-007* — rejected; the catch-rate guarantee is the feature's reason to
  exist.

---

## D9 — W3 backend-wiring smoke outcome (2026-06-19)

**Context**: Before building the US3 real-UI journeys, a reconnaissance smoke
(review finding W3) assessed whether the backend is wired enough for real-signal
E2E. Findings:

- **Harness: READY.** `tauri-driver`, `WebKitWebDriver`, `xvfb-run` all present;
  `playwright.real-backend.config.ts` correct; `cargo build -p desktop_shell`
  passes; the Chromium project path (`chromium-real-env`, Vite with
  `VITE_USE_MOCKS=false`) loads the app end-to-end and a smoke assertion passed.
- **US3 data commands are STUBS.** `search.global` (`commands/search.rs`),
  `sessions.list`/`sessions.get` (`commands/sessions.rs`), and
  `calibration.masters.list`/`get` (`commands/calibration.rs`) explicitly return
  hardcoded fixture data ("until the real persistence layer is wired"). Real-UI
  E2E over these would assert fixtures, i.e. **false positives**.
- **US1 `plan_apply` commands look fully wired** (no stubs).
- **US2 subscriber startup gaps** documented in the spec-033 scaffold headers
  (`spawn_workflow_run_subscriber`, artifact watcher, guided auto-advance).
- The webkit/`tauri-driver` IPC project is **not yet defined** in the config and
  a real IPC run (3–5 min cold build under xvfb) was **not verified**.

**Decision**:
1. **Do NOT author US3 journeys over stubbed commands.** That would violate the
   feature's core principle (no false passes). The stub→real wiring for
   `search.global`/`sessions`/`calibration.masters` is **product work belonging to
   specs 033/035** (ties to spec-035 gap #2 per-image ingest), not to 037. 037
   records this as an explicit, documented dependency/gap (FR-002, FR-013) rather
   than faking coverage.
2. **US3 is gated**: the real-UI E2E journeys are deferred until (a) the webkit
   Tauri project is wired into the Playwright config and verified once, and (b)
   the dependent commands are de-stubbed. The only real-signal journey ready now
   is **US1 `plan_apply`** (wiring complete) — recommended as the first journey,
   to be executed in CI (Linux) where the heavy build runs, since it can't be
   verified in this WSL sandbox.
3. **Per-OS execution**: real IPC E2E runs in CI/Windows per the standing
   "validate the Windows UI" preference; not in this dev sandbox.

**Rationale**: Honest coverage over fake-green. The harness investment from
spec-033 is sound; the blocker is backend stubs, which 037 must surface, not
paper over.

## Open items carried to tasks

- Confirm exact app-DB paths per OS for the Layer-2 DB reader (`db.ts`).
- ~~Confirm `msedgedriver` acquisition step on `windows-latest` runners.~~ **CLOSED** — msedgedriver no longer needed; tauri-plugin-webdriver handles Windows.
- Decide tagging mechanism to separate network-gated live-SIMBAD test from the
  deterministic default suite.

---

## D10 — Standardize on tauri-plugin-webdriver (all OS) + retire WebdriverIO (2026-06-20)

**Context**: Prior decisions (D3/D4) adopted `tauri-driver` for Linux+Windows
and deferred macOS real-UI E2E as best-effort via `tauri-plugin-webdriver`. This
required two distinct CI paths and per-OS native WebDriver binaries
(`webkit2gtk-driver` on Linux, `msedgedriver` on Windows). After evaluating the
plugin's cross-platform maturity and confirming it compiles against Tauri 2.11,
the decision is to **standardize on `tauri-plugin-webdriver` everywhere** and
retire WebdriverIO.

**Decision**: Adopt `tauri-plugin-webdriver` (Choochmeque) v0.2 as the **sole
real-UI WebDriver automation path** on Linux, Windows, and macOS:

1. Add `tauri-plugin-webdriver = { version = "0.2", optional = true }` to
   `desktop_shell` and gate it behind a new `e2e` Cargo feature (mirrors the
   existing `dev-tools` compile-gate from spec 021). Release builds MUST omit
   `--features e2e` (Constitution Principle V).
2. Register the plugin in `build_app()` under `#[cfg(feature = "e2e")]`.
3. The embedded plugin server listens on `127.0.0.1:4445`. Clients connect via
   the `tauri-webdriver` CLI proxy (`cargo install tauri-webdriver --locked`)
   on `:4444`, which manages the app binary lifecycle via the
   `tauri:options.application` W3C capability (same capability name as before —
   no harness capability changes required).
4. **Retire WebdriverIO**: delete `apps/desktop/e2e/wdio/`, remove the
   `test:e2e:wdio` script and `webdriverio` devDep from `apps/desktop/package.json`.
5. **e2e.yml**: rewrite to a uniform 3-OS matrix (`ubuntu-latest`,
   `windows-latest`, `macos-latest`). Drop `webkit2gtk-driver`, `msedgedriver`,
   and `WebKitWebDriver` install steps. Keep `xvfb` (Linux headless display).
   Retain `--no-tests=warn` while all journeys are `#[ignore]` stubs.

**Supersedes**: D3 revision ("tauri-driver on Linux/Windows") and D4
("macOS best-effort only"). D4's rationale for a debug-only compile-gate is
adopted and extended to all OS.

**Rationale**: One harness, one CI path across all three OS. Eliminates
msedgedriver version-matching, WebKitWebDriver apt dependency, and the split
"Linux/Windows required, macOS best-effort" CI topology. The plugin is
cross-platform, compile-gated so the automation surface is absent from release
binaries, and uses the same `tauri:options.application` capability the existing
thirtyfour harness already sends. WebdriverIO is retired because the entire
real-UI stack now lives in Rust (thirtyfour + cargo-nextest), consistent with
the `rust.tauri` steering convention.

**Tradeoffs / mitigations**:
- Newest tooling (v0.2): mitigated by the suite being `#[ignore]` stubs — the
  CI run proves compilation and plugin registration, not journey assertions.
  Fallback: if the plugin regresses, re-enable the old `tauri-driver` path
  (the thirtyfour harness capability shape is identical).
- macOS promoted from best-effort to required matrix member: acceptable because
  the journeys are stubs; when real assertions land the matrix can demote macOS
  to `continue-on-error` if needed.

**Alternatives considered**:
- *Keep split path (tauri-driver Linux/Windows + plugin macOS-only)* — rejected;
  two CI paths, two install steps, msedgedriver version-matching burden.
- *Use plugin on Linux/Windows only, keep macOS deferred* — rejected; if the
  plugin is good enough to adopt, it should replace the old path on all OS for
  consistency.
- *WebdriverIO retention* — rejected; the thirtyfour+nextest Rust stack is the
  chosen runner, and WebdriverIO adds a Node.js runner dependency with no benefit.
