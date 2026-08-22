// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Playwright mock-e2e: Calibration masters + matching (Journey 8 of the E2E
 * revalidation, Phase B / Batch 4). Specs 040 (per-tool MasterDetector →
 * individual master items surfaced on the Calibration page) and 007
 * (calibration matching/reuse with per-candidate confidence + configurable
 * tolerances). This journey previously had ZERO Playwright coverage — see
 * `docs/development/e2e-mock-coverage-audit-2026-07-05.md` (§ Batch 4).
 *
 * ── Surfaces exercised (mock-reachable) ──────────────────────────────────────
 *   1. Calibration page (`/#/calibration`) — MastersTable driven by
 *      `calibration_masters_list` → `masters` fixture (m-1…m-11,
 *      `CalibrationMaster_Serialize[]`). Proves spec 040 US2/US3: masters are
 *      LISTED AS INDIVIDUAL ITEMS distinguished by type + filter + exposure,
 *      with kind-CONDITIONAL fingerprint columns (Filter only for flats,
 *      Exposure only for darks — MastersTable.filterCell/exposureCell).
 *   2. MasterDetail (right detail pane) — `calibration_masters_get` +
 *      `sessions_list`. Fingerprint PropertyTable + Used-by/Compatible session
 *      popovers + the aging "Replace master" affordance (FR-023 aging).
 *   3. Project "Calibration readiness" panel (`/#/projects` → select project)
 *      — CalibrationMatchPanel driven by `calibration_match_suggest_batch` for
 *      the project's linked source sessions. Proves spec 007 FR-006/SC-002:
 *      ranked suggestions carry an explicit per-candidate CONFIDENCE and a
 *      status pill per (session, calibration type).
 *   4. Settings → Calibration "Matching criteria" pane (`/#/settings/cal`) —
 *      `calibration_tolerances_get`/`_update`. Proves spec 007 FR-002/FR-003/
 *      SC-001: matching tolerances are configurable, and the edit persists
 *      through the update→get seam (the mock's `calibration_tolerances`
 *      singleton is stateful, mirroring the real upsert-then-return repo).
 *
 * ── Layer-2 / real-backend-only (NOT faked here) ─────────────────────────────
 *   - MASTER DETECTION itself (spec 040 US1 FR-001…FR-004: SirilDetector /
 *     PixInsightDetector deciding `is_master` + base frame type from IMAGETYP/
 *     STACKCNT/name). Detection is a boolean classification with NO confidence
 *     score (confidence is a MATCHING concept, not a detection one), and runs
 *     in the Rust `calibration/master-detect` crate at ingest `classify` — it
 *     is not reachable through any mock IPC seam. Covered by that crate's
 *     table-driven unit tests + `calibration_master_detect` integration tests.
 *   - The confirm→register→appear-on-Calibration-page ROUND-TRIP (spec 040
 *     US3/SC-003): `inbox_list` and `calibration_masters_list` are independent
 *     static fixtures with no cross-command state, so confirming a master in
 *     the Inbox cannot mutate the Calibration list in mock mode. This spec
 *     asserts the LIST OUTCOME (masters present on the page) directly; the
 *     mutation edge is Layer-2. (Master items appearing individually in the
 *     Inbox at ingest are already covered by `inbox_ingest_confirm.spec.ts`.)
 *   - Tolerance changes RE-RANKING live matches (the "immediate matching-view
 *     effect"): the mock `calibration_match_suggest*` fixtures are static and
 *     do not recompute confidence from tolerances — that needs the real
 *     `calibration_core` ranking engine (Layer-2).
 *
 * Fixture facts asserted against (apps/desktop/src/data/fixtures/calibration.ts
 * + apps/desktop/src/api/mocks.ts), aging threshold = 90 days (the
 * `useCalibrationSettings` default; `settings_get('calibration')` returns the
 * generic mock scope with no override):
 *   m-1  · dark · exposureS 120 · filter null · ageDays 245 → "Master Dark · 120s", AGING
 *   m-5  · dark · exposureS 300 · filter null · ageDays 38  → fresh (no aging pill)
 *   m-8  · flat · exposureS 3   · filter "Ha"  · ageDays 246 → "Master Flat · Ha", AGING
 *   m-10 · bias · exposureS null· filter null · ageDays 38  → "Master Bias", fresh
 * MastersTable column order (stable): 0 master · 1 camera · 2 filter · 3 gain ·
 * 4 exposure · 5 temp · 6 binning · 7 usage · 8 created.
 * Project `proj-001` (mockProjectDetail008) links sources inv-001 / inv-002;
 * `calibration_match_suggest_batch` returns one `dark` result per session with
 * top-candidate confidence 0.97 → "97%".
 */
import {
  test,
  expect,
  seedSetupComplete,
  disableOnboarding,
} from './support/harness';

test.describe('calibration · masters listing + matching (spec 040 / 007)', () => {
  // ── Journey 8 · Calibration page lists detected masters individually with
  //    kind-conditional fingerprint columns (spec 040 US2 FR-006 / US3 FR-007;
  //    the "format/type-distinguished" master surface). ──────────────────────
  test('Calibration page lists masters as individual items with kind-conditional Filter/Exposure columns (spec 040 US2/US3 FR-006/FR-007)', async ({
    page,
  }) => {
    seedSetupComplete(page);
    await page.goto('/#/calibration');
    await disableOnboarding(page);

    await expect(
      page.getByTestId('app-error-boundary-fallback'),
    ).not.toBeVisible();

    // The dense full-width masters table renders (not the loading/empty state).
    const table = page.locator('[data-testid="calib-table"]');
    await expect(table).toBeVisible({ timeout: 8_000 });

    // Each master is its OWN row, labelled by type + its discriminator
    // (exposure for darks, filter for flats) — FR-005/FR-006 "individual
    // items distinguished by filter/exposure", not a folder lump.
    const darkRow = page
      .locator('[data-kind="calib-table-row"]')
      .filter({ hasText: 'Master Dark · 120s' });
    const flatRow = page
      .locator('[data-kind="calib-table-row"]')
      .filter({ hasText: 'Master Flat · Ha' });
    const biasRow = page
      .locator('[data-kind="calib-table-row"]')
      .filter({ hasText: 'Master Bias' })
      .first();
    await expect(darkRow).toBeVisible();
    await expect(flatRow).toBeVisible();
    await expect(biasRow).toBeVisible();

    // Kind pills discriminate the base frame type (spec 040 FR-004 classify).
    await expect(darkRow.getByText('DARK', { exact: true })).toBeVisible();
    await expect(flatRow.getByText('FLAT', { exact: true })).toBeVisible();
    await expect(biasRow.getByText('BIAS', { exact: true })).toBeVisible();

    // Applicability-driven columns (MastersTable.filterCell / exposureCell,
    // spec-030 Q16 field-applicability matrix): cell order → 2 = Filter,
    // 4 = Exposure. "—" is the NOT-APPLICABLE marker (a missing-but-
    // applicable value would render the "Unresolved" chip instead).
    //   DARK: Exposure "120s"; Filter not applicable to darks → "—".
    await expect(darkRow.locator('td').nth(4)).toHaveText('120s');
    await expect(darkRow.locator('td').nth(2)).toHaveText('—');
    //   FLAT: Filter "Ha"; Exposure applies to flats (matrix) and the
    //   fixture carries a real 3.0s FlatWizard exposure → "3s", no longer
    //   suppressed to a dash by the old kind-hardcoded cell.
    await expect(flatRow.locator('td').nth(2)).toHaveText('Ha');
    await expect(flatRow.locator('td').nth(4)).toHaveText('3s');
    //   BIAS: neither Filter nor Exposure applies → both "—".
    await expect(biasRow.locator('td').nth(2)).toHaveText('—');
    await expect(biasRow.locator('td').nth(4)).toHaveText('—');
  });

  // ── Journey 8 · Aging masters are flagged (FR-023); selecting a master opens
  //    its fingerprint detail with Used-by / Compatible session context and the
  //    aging "Replace master" affordance (spec 040 metadata display). ─────────
  test("aging masters carry an 'aging' pill and a fresh master does not; selecting one opens its fingerprint detail (spec 040 US3 / spec 007 FR-023)", async ({
    page,
  }) => {
    seedSetupComplete(page);
    await page.goto('/#/calibration');
    await disableOnboarding(page);
    await expect(page.locator('[data-testid="calib-table"]')).toBeVisible({
      timeout: 8_000,
    });

    // m-1 (ageDays 245 > 90-day threshold) is flagged as aging IN-ROW.
    const agingRow = page
      .locator('[data-kind="calib-table-row"]')
      .filter({ hasText: 'Master Dark · 120s' });
    await expect(agingRow.getByText('aging 245d')).toBeVisible();

    // m-5 (ageDays 38 < threshold) is fresh — no aging pill on its row.
    const freshRow = page
      .locator('[data-kind="calib-table-row"]')
      .filter({ hasText: 'Master Dark · 300s' })
      .first();
    await expect(freshRow).toBeVisible();
    await expect(freshRow.getByText(/aging \d+d/)).toHaveCount(0);

    // Select the aging master → the right-side MasterDetail pane mounts.
    await agingRow.click();
    const detail = page.locator('[data-testid="listpage-detail"]');
    await expect(detail).toBeVisible({ timeout: 5_000 });

    // Fingerprint PropertyTable exposes the master's real metadata
    // (Poseidon-C PRO · gain 125 · 120s) — the fields that later drive
    // spec 007 dimension matching.
    await expect(detail).toContainText('Poseidon-C PRO');
    await expect(detail).toContainText('125');
    await expect(detail).toContainText('120s');

    // Session-context popovers render for both relationships (FR-006 usage).
    await expect(detail.getByText('Used by', { exact: true })).toBeVisible();
    await expect(detail.getByText('Compatible', { exact: true })).toBeVisible();

    // The aging master offers the destructive "Replace master" action
    // alongside "Use in project" (a fresh master would omit Replace).
    await expect(
      detail.getByRole('button', { name: 'Use in project' }),
    ).toBeVisible();
    await expect(
      detail.getByRole('button', { name: 'Replace master' }),
    ).toBeVisible();

    await expect(
      page.getByTestId('app-error-boundary-fallback'),
    ).not.toBeVisible();
  });

  // ── Journey 8 · Calibration matching/reuse: project readiness surfaces
  //    ranked suggestions carrying an explicit per-candidate CONFIDENCE for
  //    each linked light session (spec 007 US1/US6 FR-006 / SC-002). ──────────
  test("project 'Calibration readiness' shows a confidence-carried match status per linked session (spec 007 FR-006/SC-002)", async ({
    page,
  }) => {
    seedSetupComplete(page);
    await page.goto('/#/projects');
    await disableOnboarding(page);
    await expect(
      page.getByTestId('app-error-boundary-fallback'),
    ).not.toBeVisible();

    // Open proj-001's detail — this mounts ProjectBottomDetail, which renders
    // the CalibrationMatchPanel for the project's linked source sessions
    // (inv-001 / inv-002 from mockProjectDetail008).
    const projectRow = page
      .locator('[data-kind="projects-table-row"]')
      .filter({ hasText: 'NGC 7000 Narrowband' })
      .first();
    await expect(projectRow).toBeVisible({ timeout: 8_000 });
    await projectRow.click();

    // The read-only "Calibration readiness" section (batch-suggest driven).
    const panel = page.getByTestId('cal-panel');
    await expect(panel).toBeVisible({ timeout: 8_000 });

    // One row per linked source session — the panel is per-session (US6 batch).
    await expect(panel.getByTestId('cal-session-inv-001')).toBeVisible();
    await expect(panel.getByTestId('cal-session-inv-002')).toBeVisible();

    // Each (session, type) result carries a status pill + an explicit
    // confidence readout — the "recommendations carry per-candidate
    // confidence" contract (FR-006). The mock's top dark candidate is 0.97.
    const darkResult = panel.getByTestId('cal-type-dark-inv-001');
    await expect(darkResult).toBeVisible();
    await expect(darkResult.getByText('match', { exact: true })).toBeVisible();
    await expect(panel.getByTestId('cal-confidence-dark-inv-001')).toHaveText(
      '97%',
    );

    // The panel is advisory (FR-007): it points at the Calibration page for
    // the actual assign, rather than mutating from here.
    await expect(panel).toContainText(/assign calibration masters/i);
  });

  // ── Journey 8 · Matching TOLERANCES are configurable and persist through the
  //    update→get seam (spec 007 FR-002/FR-003 / SC-001). ─────────────────────
  test('Calibration matching-criteria tolerances are configurable and the edit persists across a remount (spec 007 FR-002/FR-003/SC-001)', async ({
    page,
  }) => {
    seedSetupComplete(page);
    await page.goto('/#/settings/cal');
    await disableOnboarding(page);
    await expect(
      page.getByTestId('app-error-boundary-fallback'),
    ).not.toBeVisible();

    // The "Matching criteria" pane loads the persisted tolerances singleton.
    await expect(page.getByText('Matching criteria')).toBeVisible({
      timeout: 8_000,
    });

    // Hard-required dimensions are exposed as toggles (spec 007 FR-012 hard
    // rules): Binning / Gain / Offset, all defaulting on. Camera carries no
    // toggle — the ranking model has no camera-identity dimension to gate
    // (crates/calibration/core/src/ranking.rs).
    await expect(page.getByText('Binning', { exact: true })).toBeVisible();

    // Soft tolerances are numeric inputs seeded from the persisted singleton
    // (temperatureToleranceC 5, agingLimitDays 365).
    const tempInput = page.getByRole('spinbutton', {
      name: /Sensor temperature tolerance/i,
    });
    const ageInput = page.getByRole('spinbutton', {
      name: /Dark and bias age limit/i,
    });
    await expect(tempInput).toHaveValue('5');
    await expect(ageInput).toHaveValue('365');

    // Edit the sensor-temperature tolerance — this fires
    // `calibration_tolerances_update`, which the stateful mock persists.
    await tempInput.fill('8');
    await expect(tempInput).toHaveValue('8');

    // Switch to another settings pane and back via the in-app nav (an SPA
    // pane swap, NOT a document reload — so the persisted singleton survives).
    // The pane switch unmounts CalibrationMatching and remounts it, forcing a
    // fresh `calibration_tolerances_get`; the edited value must round-trip.
    const nav = page.getByRole('navigation', { name: /settings/i });
    await nav.getByRole('button', { name: 'Appearance' }).click();
    await expect(page.getByText('Matching criteria')).toHaveCount(0);
    await nav.getByRole('button', { name: 'Calibration Matching' }).click();
    await expect(page.getByText('Matching criteria')).toBeVisible({
      timeout: 8_000,
    });

    const tempInputAfter = page.getByRole('spinbutton', {
      name: /Sensor temperature tolerance/i,
    });
    await expect(tempInputAfter).toHaveValue('8');
  });
});
