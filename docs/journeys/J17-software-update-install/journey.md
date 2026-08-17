---
id: J17
title: Learn about, install, and restart into a signed software update
version: 2
status: active
last_reviewed: 2026-07-24
actors: [astrophotographer]
surfaces: [settings]
interfaces: [desktop-ui]
trace:
  - pre-migration journey.md @ git 42c596d6
  - specs/051-tauri-shell-integration/spec.md US10 (FR-029..FR-032, SC-009)
  - GitHub #845 (open — running version never displayed)
  - GitHub #762 (open — tamper-rejection test coverage gap)
  - docs/development/journey-run-2026-07-14.md (2026-07-14 Windows validation pass)
---

## Goal
The user learns a newer PlateVault release exists, installs it through a
signature-verified download, and ends up running the new version — without
the background check ever interrupting library work, and without an
unverifiable package ever getting installed. Done means: Settings → Advanced
reads "up to date" again after a successful install-and-relaunch, and every
failure branch (unreachable feed, bad signature, bad download) leaves the
previously running install completely untouched.

## Preconditions
- P1: A PlateVault build with the updater plugin registered. The frontend
  subscription/install path is a no-op in mock mode (`VITE_USE_MOCKS=true`)
  — that mode cannot exercise any part of this journey. The passive check
  (S1) does not require an installed/packaged build: the 2026-07-14 Windows
  validation run exercised it against an unbundled `run-dev-mcp.bat` dev
  build and observed `window.__TAURI__.updater.check()` completing normally
  (docs/development/journey-run-2026-07-14.md). Actually completing an
  install-and-relaunch (S2) is expected to need a genuinely installed build,
  since `downloadAndInstall()`/`relaunch()` replace the running executable —
  that specific requirement was not exercised live in that run (no update
  was available to install).
  Mock-mode coverage for the UI phases (up-to-date, ready, check-failed) is
  now available via `window.__PV_TEST__.updateState` — set by the Playwright
  spec `tests/e2e/settings_update_check.spec.ts` before navigation so
  `startUpdateSubscription()` seeds the requested phase without a real Tauri
  host.
- P2: The configured update endpoint (GitHub Releases `latest.json`) is
  reachable at startup. An endpoint that is unreachable from the app's very
  first check is indistinguishable from a genuine "up to date" result (see
  S1 Expect (negative); `check_for_app_update` only logs on error and emits
  no frontend event either way) — it does not by itself exercise a visible
  failure branch. To exercise S3's network-failure branch, the endpoint
  must go unreachable after the startup check has already surfaced an
  available update and before the user clicks Install (the install action
  re-checks live before downloading).
- P3: For S2, a genuinely newer release signed with the app's release
  keypair is published at that endpoint; for the signature-failure branch
  in S3, a tampered or unsigned artifact is placed there instead (the
  startup check must still detect *an* update so the Install control
  renders — the tampering is only caught during the install-time
  download/verify).

## Steps
### S1 — Learn the update state {#S1}
- **Do:** Open Settings → Advanced and locate the Software Update section.
- **Expect:** The section reads either "You're running the latest version."
  or "Update available: version {version}", reflecting a single background
  check performed automatically once at app startup; the check never blocks
  the UI, never repeats later in the session, and never interrupts
  in-progress library work.
- **Expect (negative):** A check that fails (unreachable feed, network error)
  is now a distinct "check failed" state (#873 resolved) and is surfaced with
  the specific error message inline — it is never conflated with "up to date".
  The section displays the currently running app version number alongside
  the update status (#845 resolved).

### S2 — Install an available update {#S2}
- **Do:** With "Update available" shown, choose Install & Restart.
- **Expect:** The app downloads the release artifact and verifies its
  signature against the app's embedded public key; only after verification
  succeeds does it install the update and relaunch into the new version.
  This is a single atomic, explicitly user-initiated action — nothing is
  ever downloaded, staged, or installed as a side effect of the passive
  startup check.
- **Expect (negative):** An artifact whose signature does not verify
  against the embedded key is never installed and never triggers a
  relaunch; the currently running version keeps executing, completely
  unchanged, and no partial install is left on disk.

### S3 — Recover from a failed check or failed install {#S3}
- **Do:** With "Update available" shown, choose Install & Restart after the
  update endpoint has gone unreachable since detection (network-failure
  branch), or while a tampered/unsigned artifact is now present at the
  endpoint (signature-failure branch) — the Install click re-runs the check
  live before downloading, so either failure surfaces at that point.
- **Expect:** The failure is reported inline in the Software Update section
  as "Update failed: {message}", carrying the specific underlying error
  rather than a bare generic phrase; the Install action stays available to
  retry.
- **Expect (negative):** No crash and no partial or corrupted install
  results from any failure branch; the previously running install is left
  fully usable and unchanged, and the next app launch's background check
  still runs normally regardless of the prior failure.

### S4 — Confirm the new version after restart {#S4}
- **Do:** After a successful install-and-relaunch, reopen Settings →
  Advanced.
- **Expect:** The Software Update section reads "You're running the latest
  version." again, reflecting the freshly-installed release; no
  update-available prompt reappears for that same release.

## Success criteria
- SC1: The Software Update section always shows exactly one of the two
  defined states, and the startup check that produces it never blocks or
  interrupts any in-progress library action (S1).
- SC2: 100% of tampered or unsigned artifacts placed at the update endpoint
  are rejected before install; a genuinely newer signed release is
  detected and installable end-to-end (S2) — mirrors spec 051 SC-009.
- SC3: Every failure branch (unreachable feed, failed signature, failed
  download) surfaces its specific underlying error inline and leaves the
  previously running install completely unchanged (S3).
- SC4: After a successful install, the section reads "up to date" with no
  repeat prompt for that release (S4).

## Known gaps
- G1: (dissolved 2026-07-15) — tracked as issue #845; running app version never displayed.
- G2: (dissolved 2026-07-15) — tracked as issue #873; failed update check not surfaced distinctly.
- G3: (dissolved 2026-07-15) — tracked as issue #762; missing tamper/unsigned-artifact test coverage.
- G4: (dissolved 2026-07-15) — tracked as issue #888; owner decided: build staged flow.
- G5: (dissolved 2026-07-15) — tracked as issue #881; validation-campaign coverage tracker.

## Delta log
- 2026-07-24 (v2): Status promoted to active. Added `window.__PV_TEST__.updateState`
  injection hook to `updateSubscription.ts` for mock-mode coverage. Added
  `PV_E2E_VERSION_OVERRIDE` dev-tools-gated Rust hook so a test run can spoof a
  lower current version against a fixture endpoint. New e2e spec
  `tests/e2e/settings_update_check.spec.ts` covers up-to-date, ready, and
  check-failed UI phases (spec 051). S1 "negative" updated: failed checks are now
  a distinct `check-failed` state (resolved gap G2 / #873).
