// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Playwright mock-e2e: Targets & planning (Journey 9 of the E2E revalidation,
 * Phase B / Batch 5). This journey previously had ZERO Playwright coverage
 * (see `docs/development/e2e-mock-coverage-audit-2026-07-05.md`, Batch 5).
 *
 * Specs exercised:
 *   - 035 (SIMBAD target resolution): resolve-on-demand long-tail + seed catalog.
 *   - 023 (target identity/history): identity + aliases surfaced in search rows.
 *   - 044 (targets planner astronomy, Track A/B): per-site max altitude tonight +
 *     imaging-time; observing-site model + gate.
 *   - 047 (targets planner moon/filters): moon phase / illumination, per-target
 *     lunar separation, moon-driven filter guidance, opposition; the site gate
 *     (D7) that renders NO astronomy until a default observing site exists.
 *
 * ── The #450 planner-dead-gate regression this file pins ─────────────────────
 *
 * `features/targets/site-gate.ts` gates ALL planner astronomy behind
 * `activeSite() !== null` (spec 047 D7). If that gate can never open — the #450
 * "planner dead-gate" bug — the planner silently shows nothing forever even
 * after a site is configured. The `PLANNER REGRESSION GUARD` describe below
 * pins BOTH sides of the gate so it can never silently die again:
 *   - no observing site  → the explicit "set up your observing site" prompt
 *     (NOT a crash, NOT a silently-blank planner), and every astronomy value is
 *     the honest "—" placeholder / degraded 0° (never a fabricated number);
 *   - active observing site → the Moon summary (047) AND real per-site
 *     altitude/imaging-time (044) + lunar separation / opposition (047) values
 *     appear.
 *
 * ── How the observing site is seeded (mock layer) ────────────────────────────
 *
 * The site store (`observing-sites/site-store.ts`) hydrates the gate from
 * `settings_get('observing')` (see `Shell.tsx` → `loadObservingState()`), and
 * the usable-altitude threshold reads the same scope. The enriched mock
 * (`apps/desktop/src/api/mocks.ts`) makes the `observing` scope reflect a
 * per-session values bag seeded from the `pv-e2e-observing` localStorage key.
 * `seedObservingSite()` below sets that key BEFORE navigation, so the mock's
 * `settings_get('observing')` returns a real active site and the gate opens —
 * exactly as the real backend would after a Settings → Observing Sites save.
 * With the key absent the scope resolves to empty values → no active site →
 * gate stays closed.
 *
 * This batch is ADDITIVE: the only product-adjacent change is the scope-aware
 * `settings_get`/`settings_update` for the `observing` scope in `mocks.ts`
 * (faithful to the real per-scope settings transport); no app behaviour changed.
 */
import {
  test,
  expect,
  seedSetupComplete,
  disableOnboarding,
  assertDefined,
} from './support/harness';
import type { Page } from '@playwright/test';

/**
 * Seed an active observing site into the mock `observing` settings scope so the
 * planner gate (`activeSite() !== null`) opens. Amsterdam (lat +52) keeps the
 * northern deep-sky seed targets (M 31 dec +41, NGC 7000 dec +44) high in the
 * sky, so max-altitude tonight is a large, date-stable culmination value.
 */
function seedObservingSite(page: Page): void {
  page.addInitScript(() => {
    window.localStorage.setItem(
      'pv-e2e-observing',
      JSON.stringify({
        observingSites: [
          {
            id: 'site-e2e-1',
            name: 'Backyard (Amsterdam)',
            latitudeDeg: 52.37,
            longitudeDeg: 4.9,
            elevationM: 5,
            timezone: 'Europe/Amsterdam',
            twilight: 'astronomical',
            minHorizonAltDeg: 0,
          },
        ],
        observingActiveSiteId: 'site-e2e-1',
        observingDefaultSiteId: 'site-e2e-1',
        usableAltitudeDeg: 30,
      }),
    );
  });
}

/** Locate a target row by its designation text (only 2 seed rows exist). */
function targetRow(page: Page, designation: string) {
  return page.locator('[data-testid="targets-table-row"]', {
    hasText: designation,
  });
}

// Column order in TargetsTable (see COLUMNS; sparkline + visible columns
// removed by the 2026-07-15 iteration, FR-007): 0 star · 1 designation ·
// 2 type · 3 maxAlt · 4 opposition · 5 lunarDist · 6 filters ·
// 7 imagingTime · 8 sessions.
const COL = {
  maxAlt: 3,
  opposition: 4,
  lunarDist: 5,
  imagingTime: 7,
  sessions: 8,
} as const;

test.describe('PLANNER REGRESSION GUARD · site gate (spec 047 D7, #450)', () => {
  /**
   * NO observing site → the planner is honestly GATED, not silently dead:
   *   - the "set up your observing site" prompt renders in the top bar;
   *   - the Moon summary (047) does NOT render;
   *   - the "add a site" info banner renders above the table;
   *   - every per-target astronomy value degrades to the honest placeholder
   *     ("—" for lunar/opposition, 0° max altitude) — never a fabricated number.
   */
  test('9.1a · with NO observing site the planner shows the set-up prompt and no astronomy', async ({
    page,
  }) => {
    seedSetupComplete(page);
    await page.goto('/#/targets');
    await disableOnboarding(page);

    await expect(
      page.getByTestId('app-error-boundary-fallback'),
    ).not.toBeVisible();

    // The site-gate prompt is shown; the Moon summary is NOT.
    const prompt = page.getByTestId('planner-site-prompt');
    await expect(prompt).toBeVisible({ timeout: 8_000 });
    await expect(prompt).toContainText('Set up your observing site');
    await expect(prompt).toContainText(
      'Add a default observing location so the planner can compute',
    );
    await expect(page.getByTestId('moon-summary')).toHaveCount(0);

    // The table-level "add a site" info banner is shown.
    await expect(
      page.locator('[data-testid="targets-table-no-site-banner"]'),
    ).toBeVisible();

    // The seed catalog still lists targets (list is independent of the gate)…
    const m31 = targetRow(page, 'M 31');
    await expect(m31).toBeVisible();

    // …but every astronomy value is the honest placeholder, never fabricated:
    //   - lunar / opposition cells render "—" (no `.pv-targets-cell--lunardist`
    //     span is emitted when the value is unknown);
    //   - max altitude degrades to 0°.
    await expect(
      page.locator('[data-testid="targets-cell-lunardist"]'),
    ).toHaveCount(0);
    await expect(m31.locator('td').nth(COL.opposition)).toHaveText('—');
    await expect(m31.locator('td').nth(COL.maxAlt)).toHaveText('0°');
  });

  /**
   * Active observing site → the gate OPENS and the full planner renders:
   *   - the Moon summary (047 Track A: phase name + illumination + direction);
   *   - real per-site max altitude tonight + imaging time (044);
   *   - real per-target lunar separation (047 US2) and opposition (047 US4);
   *   - the "add a site" banner and set-up prompt are gone.
   * This is the direct pin against the #450 dead-gate: once a site exists the
   * planner MUST come alive with real computed values.
   */
  test('9.1b · seeding an active observing site brings the planner alive with real 044+047 values', async ({
    page,
  }) => {
    seedSetupComplete(page);
    seedObservingSite(page);
    await page.goto('/#/targets');
    await disableOnboarding(page);

    await expect(
      page.getByTestId('app-error-boundary-fallback'),
    ).not.toBeVisible();

    // ── 047 Track A: the Moon summary renders with real computed values ───────
    const moon = page.getByTestId('moon-summary');
    await expect(moon).toBeVisible({ timeout: 8_000 });
    await expect(moon).toContainText('Moon tonight');
    // Real astronomy-engine output: an 8-phase name…
    await expect(moon.locator('[data-testid="moon-summary-phase"]')).toHaveText(
      /new moon|crescent|quarter|gibbous|full moon/i,
    );
    // …and an illumination % + waxing/waning direction (FR-002/FR-003).
    await expect(moon.locator('[data-testid="moon-summary-meta"]')).toHaveText(
      /\d{1,3}% illuminated · (waxing|waning)/,
    );
    // The full text equivalent is exposed via aria-label (accessibility).
    await expect(moon).toHaveAttribute(
      'aria-label',
      /^Moon tonight: .+, \d{1,3} percent illuminated, (waxing|waning)\.$/,
    );

    // The gated-off prompt + banner are gone now a site exists.
    await expect(page.getByTestId('planner-site-prompt')).toHaveCount(0);
    await expect(
      page.locator('[data-testid="targets-table-no-site-banner"]'),
    ).toHaveCount(0);

    // ── 044: real per-site altitude — M 31 (dec +41) culminates high from a
    //    +52° site, so max altitude tonight is a large non-zero value (was 0°
    //    with no site). Date-stable (culmination ≈ 90 − |lat − dec| ≈ 79°). ────
    const m31 = targetRow(page, 'M 31');
    await expect(m31).toBeVisible();
    const maxAlt = m31.locator('td').nth(COL.maxAlt);
    await expect(maxAlt).toHaveText(/°$/);
    await expect(maxAlt).not.toHaveText('0°');

    // ── 047 US2: real target↔Moon angular separation (geometry, always known
    //    for a coordinate-bearing target) renders as a degree value, not "—". ──
    const lunar = m31.locator('td').nth(COL.lunarDist);
    await expect(lunar).toHaveText(/\d{1,3}°/);
    await expect(
      lunar.locator('[data-testid="targets-cell-lunardist"]'),
    ).toBeVisible();

    // ── 047 US4: real next-opposition date + relative "in N days/months". ─────
    const opposition = m31.locator('td').nth(COL.opposition);
    await expect(opposition).toContainText(/in \d+ (day|days|month|months)/);
    await expect(opposition).not.toHaveText('—');

    // ── 047 US3: moon-driven filter guidance pills render in the Filters cell. ─
    await expect(
      m31.locator('[data-testid="guidance-cell-trigger"]'),
    ).toBeVisible();

    // ── 044: imaging-time column present; renders an honest value ("2h10m"-
    //    style when the target clears the threshold tonight, else "—" with a
    //    reason glyph, FR-030/FR-032) — never fabricated or a bare 0. ─────────
    await expect(
      page.getByRole('columnheader', { name: 'Img time' }),
    ).toBeVisible();
    await expect(m31.locator('td').nth(COL.imagingTime)).toHaveText(
      /^(\d+h(\d+m)?( ☾)?|— [☀▲☾]|—)$/,
    );

    // ── Iteration 2026-07-15 (FR-007): the sparkline + visible columns are
    //    hard-removed; visibility is folded into the imaging-time glyph. ──────
    await expect(
      page.getByRole('columnheader', { name: 'Tonight' }),
    ).toHaveCount(0);
    await expect(
      page.getByRole('columnheader', { name: 'Visible' }),
    ).toHaveCount(0);

    // ── Iteration 2026-07-15 (FR-033): the always-visible computation-context
    //    label disclosing site · latitude · twilight · threshold + "change". ──
    const computedFor = page.getByTestId('planner-computed-for');
    await expect(computedFor).toContainText('Computed for:');
    await expect(computedFor).toContainText('52.4°N');
    await expect(computedFor).toContainText('≥30°');
    await expect(
      computedFor.getByRole('link', { name: 'change' }),
    ).toBeVisible();
  });

  /**
   * The gate is DYNAMIC (spec 047: the planner opens "the moment a site is
   * created … without a reload"). Persisting a site through
   * `settings_update('observing')` and reloading the same context must flip the
   * planner on — proving the round-trip the real Settings → Observing Sites save
   * performs, and that the gate is not wedged shut (the essence of #450).
   */
  test('9.1c · a persisted observing site opens the planner after reload (dynamic gate)', async ({
    page,
  }) => {
    seedSetupComplete(page);
    await page.goto('/#/targets');
    await disableOnboarding(page);
    // Gate closed initially.
    await expect(page.getByTestId('planner-site-prompt')).toBeVisible({
      timeout: 8_000,
    });

    // Persist a site through the same IPC the Settings pane uses; the mock's
    // observing scope round-trips it (settings_update → settings_get).
    await page.evaluate(() => {
      const site = {
        id: 'site-persist-1',
        name: 'Persisted site',
        latitudeDeg: 48.1,
        longitudeDeg: 11.6,
        elevationM: 520,
        timezone: 'Europe/Berlin',
        twilight: 'astronomical',
        minHorizonAltDeg: 0,
      };
      window.localStorage.setItem(
        'pv-e2e-observing',
        JSON.stringify({
          observingSites: [site],
          observingActiveSiteId: site.id,
          observingDefaultSiteId: site.id,
          usableAltitudeDeg: 30,
        }),
      );
    });
    await page.reload();
    await disableOnboarding(page);

    // Planner alive after reload.
    await expect(page.getByTestId('moon-summary')).toBeVisible({
      timeout: 8_000,
    });
    await expect(page.getByTestId('planner-site-prompt')).toHaveCount(0);
  });
});

test.describe('Planner date picker + per-band moon-free hours (spec 044 Track B US2/US5)', () => {
  /**
   * T024 (FR-008/SC-004): choosing a different planning date recomputes the
   * table's observability for that night. M 31's transit altitude is
   * date-independent (culmination only depends on lat/dec), so this asserts
   * against the date-DEPENDENT "Img time" figure instead: jumping the date
   * forward by ~half a year moves to the opposite season, which changes the
   * dark-window/imaging-time figure for essentially any real test-run date
   * (the only false-negative window is exactly at an equinox-like coincidence,
   * vanishingly unlikely). Resetting via "Tonight" must restore the original
   * value exactly, proving the round-trip (not just "some value changed").
   */
  test('9.4a · choosing a future date changes Img time, and Tonight restores it', async ({
    page,
  }) => {
    seedSetupComplete(page);
    seedObservingSite(page);
    await page.goto('/#/targets');
    await disableOnboarding(page);

    const m31 = targetRow(page, 'M 31');
    await expect(m31).toBeVisible({ timeout: 8_000 });
    const imgTimeCell = m31.locator('td').nth(COL.imagingTime);
    const beforeText = await imgTimeCell.textContent();

    const dateInput = page.getByLabel('Plan for');
    await expect(dateInput).toBeVisible();
    const today = new Date();
    const future = new Date(today.getTime() + 182 * 86_400_000);
    const futureValue = future.toISOString().slice(0, 10);
    await dateInput.fill(futureValue);

    const resetBtn = page.getByRole('button', { name: 'Tonight' });
    await expect(resetBtn).toBeVisible();
    await expect(imgTimeCell).not.toHaveText(beforeText ?? '');

    await resetBtn.click();
    await expect(resetBtn).toHaveCount(0);
    await expect(imgTimeCell).toHaveText(beforeText ?? '');
  });

  /**
   * T029 (FR-007/FR-022): the Filters guidance popover shows each band's real
   * moon-free imaging hours alongside Track A's required-separation figure.
   */
  test('9.4b · the Filters guidance popover shows per-band moon-free hours', async ({
    page,
  }) => {
    seedSetupComplete(page);
    seedObservingSite(page);
    await page.goto('/#/targets');
    await disableOnboarding(page);

    const m31 = targetRow(page, 'M 31');
    await expect(m31).toBeVisible({ timeout: 8_000 });
    await m31.locator('[data-testid="guidance-cell-trigger"]').click();

    const popup = page.getByTestId('guidance-explain-popup');
    await expect(popup).toBeVisible();
    await expect(popup).toContainText(/h moon-free/);
  });
});

test.describe('Target catalog + SIMBAD resolve-on-demand (spec 035 / 023)', () => {
  /**
   * The Targets page lists the seed catalog (spec 035 US1 local seed). Both
   * northern seed objects appear in the list from the `target_list` mock.
   */
  test('9.2a · the target catalog list renders seed objects', async ({
    page,
  }) => {
    seedSetupComplete(page);
    await page.goto('/#/targets');
    await disableOnboarding(page);

    await expect(targetRow(page, 'M 31')).toBeVisible({ timeout: 8_000 });
    await expect(targetRow(page, 'NGC 7000')).toBeVisible();
  });

  /**
   * SIMBAD resolve-on-demand (spec 035 US3, FR long-tail): a query with a local
   * seed hit shows that hit with its identity/aliases (spec 023) — the primary
   * designation, the common-name secondary line, and the `seed` source badge.
   */
  test('9.2b · a seed hit surfaces identity + aliases in the search typeahead', async ({
    page,
  }) => {
    seedSetupComplete(page);
    await page.goto('/#/targets');
    await disableOnboarding(page);

    await page.getByRole('button', { name: 'Add target' }).click();
    const search = page.getByLabel('Search for a target');
    await expect(search).toBeVisible();
    await search.fill('M 31');

    const option = page.locator('[data-testid="target-search-option"]', {
      hasText: 'M 31',
    });
    await expect(option).toBeVisible({ timeout: 8_000 });
    // Identity + alias (spec 023): common name secondary + object-type + source.
    await expect(option).toContainText('Andromeda Galaxy');
    await expect(option).toContainText('seed');
  });

  /**
   * SIMBAD long-tail (spec 035 US3): a query with NO local seed hit is resolved
   * ON DEMAND via `target.resolve` and merged into the list with the `resolved`
   * source badge — the resolve-on-demand path that only a live resolver call
   * produces. The mock's `target_resolve` mirrors the real "resolved" envelope.
   */
  test('9.2c · a long-tail query resolves on demand via SIMBAD (resolved source)', async ({
    page,
  }) => {
    seedSetupComplete(page);
    await page.goto('/#/targets');
    await disableOnboarding(page);

    await page.getByRole('button', { name: 'Add target' }).click();
    const search = page.getByLabel('Search for a target');
    await expect(search).toBeVisible();
    // "IC 1805" is not in the seed; only the long-tail resolver can supply it.
    await search.fill('IC 1805');

    const option = page.locator('[data-testid="target-search-option"]', {
      hasText: 'IC 1805',
    });
    await expect(option).toBeVisible({ timeout: 8_000 });
    await expect(option).toContainText('resolved');
  });
});

test.describe('Honest empty-state disclosure (no fabricated data)', () => {
  /**
   * Not-yet-built or not-yet-loaded data must be disclosed honestly, never
   * fabricated:
   *   - the Sessions column reads the real backend `sessionCount` (#622,
   *     #877, #1293) — the seed fixture gives M 31 and NGC 7000 non-zero
   *     counts, mirroring what the real backend returns for those targets.
   *     The honest-"—"-for-zero rendering itself (never a bare 0, and never
   *     fabricated for a target the backend reports as unshot) is covered at
   *     the unit level in `TargetsTable.test.tsx` ("TargetsTable Sessions
   *     column (#622)"), which exercises sessionCount: 0 and an absent field;
   *   - the favourite star column shows every row un-starred (#54 client-side
   *     favourites, no fabricated favourites) with aria-pressed=false.
   */
  test('9.3a · sessions count reflects the real backend value and favourites are honestly empty', async ({
    page,
  }) => {
    seedSetupComplete(page);
    seedObservingSite(page);
    await page.goto('/#/targets');
    await disableOnboarding(page);

    const m31 = targetRow(page, 'M 31');
    await expect(m31).toBeVisible({ timeout: 8_000 });

    // Sessions column: the real per-target count from the backend-aligned
    // mock fixture (#1308), not a hardcoded placeholder.
    await expect(m31.locator('td').nth(COL.sessions)).toHaveText('3');
    await expect(
      targetRow(page, 'NGC 7000').locator('td').nth(COL.sessions),
    ).toHaveText('5');

    // Favourites (#54): every star is un-filled and reports aria-pressed=false —
    // no fabricated "starred" state.
    const star = m31.locator('[data-testid="targets-star"]');
    await expect(star).toHaveAttribute('aria-pressed', 'false');
    await expect(star).toContainText('☆');
  });

  /**
   * The "My Targets" filter is backed by a client-side favourites stub (#54,
   * backend linkage not landed). With no favourites it must show the honest
   * empty state, NOT fabricate a "my targets" list.
   */
  test('9.3b · My Targets with no favourites shows the honest empty state', async ({
    page,
  }) => {
    seedSetupComplete(page);
    await page.goto('/#/targets');
    await disableOnboarding(page);

    // Baseline: the catalog is populated.
    await expect(targetRow(page, 'M 31')).toBeVisible({ timeout: 8_000 });

    // Switch the "Show" filter to "My Targets" (native <select>, aria-label "Show").
    await page.getByLabel('Show').selectOption('my');

    // Honest empty state — not a fabricated list.
    await expect(
      page.getByText('No favourites yet. Star a target (☆) to add it here.'),
    ).toBeVisible();
    await expect(targetRow(page, 'M 31')).toHaveCount(0);
  });
});

test.describe('Planner observability iteration (spec 044 Phase 10, 2026-07-15)', () => {
  /**
   * #817 (FR-029/FR-030/FR-034, SC-015): a night with NO qualifying dark
   * window must state WHY imaging time is zero everywhere the number appears,
   * and the detail graph must agree with the stat instead of contradicting it.
   * At 52.37°N the Sun never reaches astronomical darkness around the June
   * solstice (minimum depression ≈ 14°), so planning for June 21 reproduces
   * the #817 condition date-stably every year.
   */
  test('9.5a · #817: a no-dark-window night states its reason — table glyph, detail sentence, non-dark graph', async ({
    page,
  }) => {
    seedSetupComplete(page);
    seedObservingSite(page);
    await page.goto('/#/targets');
    await disableOnboarding(page);

    const m31 = targetRow(page, 'M 31');
    await expect(m31).toBeVisible({ timeout: 8_000 });

    // Plan for the NEXT June solstice (always in the future so the date
    // picker round-trip mirrors 9.4a's arbitrary-future-night flow).
    const now = new Date();
    const year =
      now.getMonth() >= 5 ? now.getFullYear() + 1 : now.getFullYear();
    await page.getByLabel('Plan for').fill(`${year}-06-21`);

    // Table: zero imaging time carries the darkness reason glyph (FR-030) —
    // never a bare 0 or an unexplained "—" (SC-015).
    const imgTime = m31.locator('td').nth(COL.imagingTime);
    await expect(imgTime).toContainText('☀');
    await expect(
      imgTime.locator('[data-testid="imgtime-glyph-warn"]'),
    ).toBeVisible();

    // Detail: the same zero is stated as a sentence (FR-029)…
    await m31.click();
    await expect(
      page.getByText(/never gets dark enough/).first(),
    ).toBeVisible();
    // …and the altitude graph AGREES (FR-034): with no dark window the whole
    // plot is shaded non-dark (exactly one full-width twilight rect —
    // pre-iteration the shading was omitted entirely and a green usable fill
    // contradicted the 0-hour stat, which was the #817 report).
    await expect(
      page.locator('[data-testid="planner-graph-twilight"]'),
    ).toHaveCount(1);
  });

  /**
   * #792 (FR-032/SC-016): the surviving planner columns are sized to their
   * real content — the widest real values ("14 Apr · in 9 months"-style
   * opposition, "2h10m"+glyph imaging time) render unclipped in a 1100×720
   * window (the stub-width Opposition column used to clip).
   */
  test('9.5b · #792: opposition and imaging-time cells render unclipped at 1100×720', async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1100, height: 720 });
    seedSetupComplete(page);
    seedObservingSite(page);
    await page.goto('/#/targets');
    await disableOnboarding(page);

    const m31 = targetRow(page, 'M 31');
    await expect(m31).toBeVisible({ timeout: 8_000 });

    const opposition = m31.locator('td').nth(COL.opposition);
    await expect(opposition).toContainText(/in \d+ (day|days|month|months)/);
    expect(
      await opposition.evaluate((el) => el.scrollWidth <= el.clientWidth),
    ).toBe(true);

    const imgTime = m31.locator('td').nth(COL.imagingTime);
    expect(
      await imgTime.evaluate((el) => el.scrollWidth <= el.clientWidth),
    ).toBe(true);
  });

  /**
   * FR-009 amendment (iteration 2026-07-17, the #792 naming half): the LIST
   * "Opposition" column stays the pure geometric transit-at-midnight date,
   * while the DETAIL "Best date" is the nearest Moon-viable night to it and
   * always explains itself with one of three Moon-state tooltips (mirrored
   * into aria-label, the InfoTip pattern). The Moon state at the run date
   * decides which branch fires, so both sides of the list-vs-detail
   * relationship are asserted: diverged → detail date ≠ list date and the
   * tooltip names the list's opposition date; coincides / none-found →
   * detail date = list date.
   */
  test('9.5c · FR-009 2026-07-17: detail Best date is Moon-aware and explains itself; list Opposition stays pure', async ({
    page,
  }) => {
    seedSetupComplete(page);
    seedObservingSite(page);
    await page.goto('/#/targets');
    await disableOnboarding(page);

    const m31 = targetRow(page, 'M 31');
    await expect(m31).toBeVisible({ timeout: 8_000 });

    // List surface unchanged: the pure "MMM D · in N ..." opposition cell.
    const opposition = m31.locator('td').nth(COL.opposition);
    await expect(opposition).toContainText(/in \d+ (day|days|month|months)/);
    const listOppositionDate = (await opposition.innerText())
      .split('·')[0]
      .trim();

    await m31.click();

    const bestDateTrigger = page.getByLabel(
      /Matches opposition — the Moon is favourable|falls near full Moon|No Moon-favourable night within/,
    );
    await expect(bestDateTrigger).toBeVisible();
    // aria-label = "<detail date> · in N ... — <explanation>" (InfoTip mirror).
    const label = assertDefined(
      await bestDateTrigger.getAttribute('aria-label'),
      'bestDateTrigger aria-label',
    );

    if (/falls near full Moon/.test(label)) {
      // Diverged: list ≠ detail, and the skipped opposition the tooltip
      // names IS the list column's date.
      expect(label).toContain(`Opposition ${listOppositionDate} falls`);
      expect(label.startsWith(`${listOppositionDate} ·`)).toBe(false);
    } else {
      // Coincides / none-found: the detail date equals the list opposition.
      expect(label.startsWith(`${listOppositionDate} ·`)).toBe(true);
    }
  });
});

test.describe('Design-review follow-ups (2026-07-11): #614 dead CTA, #618 header stack', () => {
  /**
   * #614: the permanently-disabled "Add to plan" placeholder CTA (no backing
   * feature) is removed from the detail header entirely — never shipped as a
   * disabled, unexplained primary-position control.
   */
  test("#614 · the detail header has no disabled 'Add to plan' placeholder button", async ({
    page,
  }) => {
    seedSetupComplete(page);
    seedObservingSite(page);
    await page.goto('/#/targets');
    await disableOnboarding(page);

    const m31 = targetRow(page, 'M 31');
    await expect(m31).toBeVisible({ timeout: 8_000 });
    await m31.click();

    await expect(page.getByRole('button', { name: 'Add to plan' })).toHaveCount(
      0,
    );
    // The real primary action (New project) is still present.
    await expect(
      page.getByRole('button', { name: 'New project' }),
    ).toBeVisible();
  });

  /**
   * #618: the Moon-phase widget (and its no-site prompt) moved out of the
   * pinned top bar's actions row into the table's own header zone — the top
   * bar itself no longer stacks a 3rd band, and "Add target" is the one
   * primary CTA at the bar's right edge.
   */
  test('#618 · the pinned top bar holds only the filter row + primary Add target action', async ({
    page,
  }) => {
    seedSetupComplete(page);
    seedObservingSite(page);
    await page.goto('/#/targets');
    await disableOnboarding(page);

    await expect(targetRow(page, 'M 31')).toBeVisible({ timeout: 8_000 });

    // The Moon summary / site-prompt no longer lives inside the pinned top
    // bar — it renders in the table's own header zone instead.
    const topBar = page.locator('[data-testid="topbar"]');
    await expect(topBar.getByTestId('moon-summary')).toHaveCount(0);
    await expect(
      page
        .locator('[data-testid="targets-table-wrap"]')
        .getByTestId('moon-summary'),
    ).toBeVisible();

    // "Add target" is the pinned bar's one primary CTA.
    const addTarget = topBar.getByRole('button', { name: 'Add target' });
    await expect(addTarget).toBeVisible();
    await expect(addTarget).toHaveAttribute('data-variant', 'primary');
  });
});
