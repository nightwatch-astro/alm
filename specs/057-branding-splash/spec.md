# Feature Specification: PlateVault Branding & Native Splash Screen

**Feature Branch**: `057-branding-splash`

**Created**: 2026-07-19

**Status**: Partially implemented (post-hoc record; US1/US2/US4 verified 2026-07-28,
US1 and US3 re-checked against `main` on 2026-08-24). The splash window ships in
`apps/desktop/src/splash/main.ts`, and the boot-ready handshake is in
`apps/desktop/src-tauri/src/lib.rs` (splash references at `:215`, `:218`, `:356`,
`:364`, `:435`, `:437`, `:481`). This header read a blanket "Implemented" until
2026-08-24; two of its claims were wrong.

- **There is no minimum-display floor.** `apps/desktop/src/splash/main.ts:13` says so
  outright, and no dwell constant exists under `apps/desktop/src-tauri/src/`
  (`git grep -nEi 'min.?display|dwell' -- apps/desktop/src-tauri/src/` exits 1, no
  matches). US1 acceptance scenario 2 and SC-002, which both
  require the splash to stay up for at least 800 ms, are **not met**. The shipped design
  gates the handover on migration completing before `app:boot-ready` fires instead.
- **US3 is not implemented.** No switchable variant system exists
  (`git grep -lniE 'splash.?variant|SplashVariant|mark.only|live.?status' -- apps/ crates/`
  exits 1), and the status line is static: `pv-status-text` occurs once in the whole
  repo, as literal text at `apps/desktop/splash.html:212`. The shipped splash renders one
  fixed presentation — mark, wordmark, version and a static "Starting…" row
  (`apps/desktop/splash.html:199-213`) — which is the *content* of variants 2 and 3
  without the switchability, the live stage messages, or scenario 4's test-only dwell
  override.

Whether the 800 ms floor and US3 are still wanted is a product decision, tracked in
`astro-plan-yzhd9`. Neither is beaded as work.

**Input**: User description: "PlateVault logo system + native splash screen + docs brand alignment. Native splashscreen window shown at launch while the main window stays hidden; startup work (database open + migrations) runs while the splash is visible; splash closes when startup is complete AND a minimum 800 ms has elapsed. Extensible splash content variants (mark-only / mark+wordmark+version / mark+live-status) with a live startup-status channel. New logo designed externally in Claude Design (single-color-capable master SVG); replace stock app icons, placeholder favicon, About dialog, README header; align docs-site brand assets (favicon, og-image, hero, stale org link). Bundle identifier change explicitly out of scope."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Branded launch with no dead time (Priority: P1)

When a user launches PlateVault, a small branded splash screen appears almost immediately, showing the PlateVault mark on a background that matches the user's chosen theme. Startup work (opening the library database, applying migrations) happens while the splash is visible. When the app is ready — and never before a minimum of 800 ms has passed — the splash disappears and the main window appears fully initialized. If startup fails, the user sees a clear error instead of a stuck splash.

**Why this priority**: Today nothing at all is on screen during startup — on a fresh or large library the user stares at an unresponsive desktop wondering if the app launched. This is the core deliverable: the launch moment becomes branded, responsive, and honest, and it works even before the final logo exists (placeholder mark acceptable).

**Independent Test**: Launch the app on a library requiring migrations; observe splash appears promptly, remains at least 800 ms, main window appears exactly once when ready. Corrupt the database path; observe an error surface instead of a hang.

**Acceptance Scenarios**:

1. **Given** the app is not running, **When** the user launches it, **Then** a frameless branded splash window appears promptly and no unstyled or partially initialized main window is ever shown.
2. **Given** startup completes very quickly (warm start, no migrations), **When** the splash is shown, **Then** it remains visible for at least 800 ms before the main window replaces it — no sub-second flash.
3. **Given** startup takes longer than 800 ms (e.g. pending migrations), **When** the work finishes, **Then** the splash closes and the main window shows without additional artificial delay.
4. **Given** the user's saved theme is a dark theme, **When** the splash appears, **Then** its background matches the dark theme (no white flash); same for light themes.
5. **Given** startup work fails (e.g. database cannot open or a migration errors), **When** the failure occurs, **Then** the splash does not hang indefinitely — the user is shown an error message and the app exits or offers recovery.
6. **Given** the app is already running, **When** the user launches a second instance, **Then** the existing main window is focused (current behavior preserved) and no orphan splash lingers.

---

### User Story 2 - PlateVault mark on every app surface (Priority: P2)

A user looking at their taskbar, dock, window title bar, installer, or the app's About dialog sees a real PlateVault logo instead of the stock framework placeholder icon, and the About dialog shows the mark with the product name and version.

**Why this priority**: The stock Tauri icon and leftover "ALM" favicon undermine the product's credibility on every OS surface. Depends on the final mark existing, so it lands after the splash mechanism.

**Independent Test**: Install/run a build and verify the OS window icon, taskbar/dock icon, installer artwork, in-app favicon, and About dialog all show the PlateVault mark and correct version.

**Acceptance Scenarios**:

1. **Given** a packaged build, **When** the user views the window, taskbar/dock, and installed-programs list, **Then** all show the PlateVault mark (all OS icon sizes regenerated from the master artwork).
2. **Given** the app is running, **When** the user opens the About entry from the app menu, **Then** a dialog shows the mark, product name, and current version.
3. **Given** the repository README, **When** viewed on GitHub, **Then** it opens with the PlateVault lockup as a header image.

---

### User Story 3 - Configurable splash content, including live status (Priority: P3)

The splash's content is one of three switchable presentations: mark only; mark + wordmark + version; or mark + wordmark + a live status line reflecting the real startup stage ("Opening database…", "Running migrations…"). Switching presentation is a configuration change, not a redesign.

**Why this priority**: The variant system is the extensibility ask; the live-status variant is the most honest presentation for long migrations but requires the status channel. The P1 story can ship with any single variant.

**Independent Test**: Build/run with each variant selected; verify content matches the variant and, for live-status, that messages correspond to actual startup stages in order.

**Acceptance Scenarios**:

1. **Given** the mark-only variant is selected, **When** the splash shows, **Then** only the mark appears on the themed background.
2. **Given** the mark+wordmark+version variant is selected, **When** the splash shows, **Then** mark, wordmark, and the current version string appear.
3. **Given** the live-status variant is selected and migrations are pending, **When** startup proceeds, **Then** the status line updates through the real stages in order, never showing fabricated progress.
4. **Given** any variant, **When** automated end-to-end tests run, **Then** a test-only override can reduce or skip the minimum dwell so suites are not slowed, without changing user-facing behavior.

---

### User Story 4 - One brand across app and docs site (Priority: P4)

A user moving between the documentation site and the desktop app recognizes them as one product: the docs favicon, social-share image, and landing hero use the same mark and shape language as the app, even though the two surfaces keep their own color palettes.

**Why this priority**: Brand coherence completes the effort but lives in a separate repository and depends on the final mark; it cannot block the app work.

**Independent Test**: Load the docs site: favicon shows the new mark (correct in light and dark), sharing a link produces a branded preview image, the landing hero visibly shares the mark's shape language, and the site's repository link points to the current organization.

**Acceptance Scenarios**:

1. **Given** the docs site, **When** viewed in a browser, **Then** the favicon is the PlateVault mark and adapts to light/dark color scheme.
2. **Given** a docs link shared on social platforms, **When** the preview renders, **Then** a branded 1200×630 image with the mark and product name appears.
3. **Given** the docs landing page, **When** compared with the app's splash, **Then** both visibly use the same mark/shape language while retaining their own palettes.
4. **Given** the docs site header/social links, **When** followed, **Then** they point at the current `platevault` organization (stale pre-migration link fixed).

---

### Edge Cases

- Startup finishing in under 800 ms (fast/warm start): dwell floor applies; no flash.
- Very long startup (large migration set, slow disk): splash must remain responsive and, in the live-status variant, keep reflecting the current stage; no timeout that kills a legitimately long migration.
- Startup failure at each stage (database locked, migration error, corrupted settings): splash must never hang; error is surfaced.
- Theme not yet saved (first ever run): splash falls back to the system light/dark preference.
- Second-instance launch during the splash window's lifetime: no duplicate splash, no stolen focus loop.
- Automated UI test harnesses attaching to the app: the driver must reach the main window, not the splash; dwell override available.
- Multi-monitor / display scaling: splash appears centered on the primary or cursor display at a sane size.
- The logo must remain legible at 16 px (favicon) and in a single color on both light and dark backgrounds.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: On launch, the system MUST display a dedicated, frameless splash window before startup work begins, and keep the main window hidden until the app is fully ready.
- **FR-002**: Startup work (database open, migrations, cache open) MUST run while the splash is visible rather than before any window exists.
- **FR-003**: The splash MUST close and the main window MUST show only when: startup work is complete AND the frontend is ready AND at least 800 ms have elapsed since the splash appeared.
- **FR-004**: The splash MUST render its background and foreground colors from the user's persisted theme choice (falling back to the system light/dark preference) with no flash of wrong-theme content.
- **FR-005**: The splash content MUST be selectable among three variants — mark only; mark + wordmark + version; mark + wordmark + live status — via configuration, without code redesign.
- **FR-006**: A startup-status channel MUST exist from the moment the splash ships; the live-status variant MUST display real startup stages in order, and other variants MUST simply ignore the channel.
- **FR-007**: If any startup stage fails, the system MUST surface a user-readable error (and exit or offer recovery) rather than leaving the splash on screen indefinitely.
- **FR-008**: Launching a second instance MUST preserve current behavior (focus existing main window) and MUST NOT create a second splash or leave an orphaned one.
- **FR-009**: An automated-test override MUST allow reducing or skipping the minimum dwell, and UI automation MUST still attach to the main window.
- **FR-010**: The application icon set (window, taskbar/dock, installer, all required OS sizes) MUST be regenerated from the master PlateVault artwork, replacing the stock placeholder icons.
- **FR-011**: The in-app favicon placeholder ("ALM") MUST be replaced with the PlateVault mark.
- **FR-012**: The About menu entry MUST open a dialog showing the PlateVault mark, product name, and current version.
- **FR-013**: The repository README MUST open with the PlateVault lockup header.
- **FR-014**: The master logo artwork MUST be vector, MUST read in a single color (usable on any theme surface), and MUST remain legible at 16 px; lockup (mark + wordmark), mark-alone, and favicon-grade reductions MUST be provided.
- **FR-015**: Docs-site brand assets MUST be aligned with the final mark: favicon (light/dark adaptive), 1200×630 social-share image, landing hero shape-language alignment, and correction of the stale pre-migration organization link. (Coordinated work in the docs repository.)
- **FR-016**: The bundle identifier MUST NOT be changed by this feature (changing it affects updater and app-data paths; tracked separately).

### Key Entities

- **Brand asset set**: master vector mark, lockup, favicon reduction, generated OS icon set, social-share image; versioned in-repo as the single source for all surfaces.
- **Splash variant configuration**: which of the three presentations is active; resolvable before any user data is available (the splash precedes database availability).
- **Startup status event**: an ordered, human-readable stage notice ("Opening database…", "Running migrations…", "Starting…") emitted by startup work and optionally rendered by the splash.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a launch requiring migrations, something branded is on screen within 1 second of launch (today: nothing until migrations finish).
- **SC-002**: On a warm start, the splash is visible for at least 800 ms and the total splash overhead beyond real startup work is no more than the dwell floor.
- **SC-003**: In 100% of induced startup-failure scenarios (locked database, failing migration), the user sees an error within 5 seconds of the failure; zero indefinite hangs.
- **SC-004**: Zero occurrences of an unstyled, wrong-theme, or partially initialized window during launch across all four themes and system-preference fallback.
- **SC-005**: All OS-visible icon surfaces (window, taskbar/dock, installer list) show the PlateVault mark after upgrade; zero stock-framework icons remain.
- **SC-006**: The full automated end-to-end suite passes with the dwell override active, with no per-journey slowdown greater than 1 second attributable to the splash.
- **SC-007**: The mark is identifiable at 16 px in a blind check against the docs favicon and app favicon (same mark recognized as same brand).

## Assumptions

- The final logo is produced externally (Claude Design session) and delivered as a master SVG meeting FR-014; the splash ships first with a placeholder mark so the mechanism is not blocked on final art.
- No brand color is committed yet: the mark's single-color core adapts to each surface's palette; app and docs may keep separate palettes provided shape language matches.
- The wordmark typeface follows the docs display face (Space Grotesk), delivered as outlined vector paths so the app need not bundle an additional font.
- The 800 ms dwell is a product decision (brand moment on every launch) and applies to real user launches only; automated tests may bypass it.
- Docs-site changes land in the separate docs repository (`platevault/platevault.github.io`) as coordinated follow-up within this feature's scope; the GitHub organization avatar upload is a manual user action outside the repository.
- Existing first-run setup wizard behavior is unchanged: the splash precedes whatever the main window decides to show (wizard or shell).
- Migration failure handling today is process-abort; this feature upgrades the presentation of that failure, not the recovery semantics.
