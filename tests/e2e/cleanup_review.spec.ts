// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Playwright mock-mode smoke: Journey 6 (spec 017 WP-E) — Cleanup
 * scan → review candidates → generate plan → protection gate → approve
 * & apply.
 *
 * Phase B / batch 2 of the E2E revalidation (docs/development/
 * e2e-mock-coverage-audit-2026-07-05.md) — this journey previously had
 * ZERO Playwright coverage (only a vitest component test,
 * `OutputsCleanupSections.test.tsx`).
 *
 * What this test proves:
 *  1. The Cleanup section (Projects page → project row → bottom detail
 *     panel) renders the constitution-II protected-category list even
 *     before any scan runs (protected categories MUST be documented
 *     before a cleanup plan can be generated).
 *  2. "Scan for cleanup candidates" (`cleanup.scan`, read-only, D11 step 1)
 *     populates grouped candidate rows carrying a parsed confidence
 *     percentage and a per-row protection state — a protected master file
 *     is clearly marked and carries NO affordance to include it anyway.
 *  3. "Generate cleanup plan" (`cleanup.plan.generate`, D11 step 2)
 *     materialises a reviewable plan and opens the shared
 *     {@link PlanReviewOverlay} — nothing is mutated until the user
 *     explicitly approves (FR-001/FR-002).
 *  4. The spec-016 protection gate blocks "Approve & apply" until the
 *     protected item is acknowledged; acknowledging unlocks it.
 *  5. Approve & apply drives plans.approve → plans.apply and streams live
 *     progress to a "completed" terminal state with a success toast.
 *  6. The destructive-destination choice uses the canonical `archive|trash`
 *     vocabulary (spec 033 vocab split) — never the legacy `os_trash`
 *     term — in both the destination picker and the plan-review subtitle.
 *
 * Mock wiring (apps/desktop/src/api/mocks.ts):
 *   cleanup_scan            → 3 candidates: 2 "intermediate" (90% confidence,
 *                             normal protection) + 1 "master" (95% confidence,
 *                             protected), totalReclaimableBytes 536_870_912
 *                             (512.0 MB) — the two normal intermediates only,
 *                             the protected master contributes nothing.
 *   cleanup_plan_generate   → { planId: 'plan-cleanup-mock', itemCount: 3,
 *                             protectedItemCount: 1 }.
 *   plans_get               → static `planDetail` fixture (11 cleanup items,
 *                             destructiveDestination: 'archive').
 *   plan_protection_check_cmd → always one protected item (spec-016 gate).
 *   plans_apply_real        → streams item_started/item_applied/completed
 *                             over the live operation channel (1 item).
 *
 * Fixture project: proj-001 "NGC 7000 Narrowband" (mockProjectDetail008 —
 * `projects.get` always returns this fixture regardless of the clicked
 * project's id, same as mockProjectSummaries[0]).
 *
 * First-run seeding:
 *   Reads `alm-preferences.setupCompleted` from localStorage.
 */
// The Tauri `Channel` polyfill this journey's approve-and-apply path needs is
// installed globally by the shared harness `test` (see
// `tests/e2e/support/harness.ts`) — no per-spec shim required.
import {
  test,
  expect,
  seedSetupComplete,
  disableOnboarding,
} from './support/harness';

test.beforeEach(async ({ page }) => {
  await disableOnboarding(page);
});

test.describe('cleanup review (spec 017 WP-E / Journey 6)', () => {
  test('scan → review candidates with confidence + protection → generate plan → protection gate → approve & apply', async ({
    page,
  }) => {
    seedSetupComplete(page);
    await page.goto('/#/projects');

    // ── 1. Page renders without error boundary ────────────────────────────────
    await expect(
      page.getByTestId('app-error-boundary-fallback'),
    ).not.toBeVisible();

    // ── 2. Select the fixture project → bottom detail panel mounts ───────────
    const projectRow = page
      .locator('[data-kind="projects-table-row"]')
      .filter({ hasText: 'NGC 7000 Narrowband' })
      .first();
    await expect(projectRow).toBeVisible({ timeout: 8_000 });
    await projectRow.click();

    const cleanupSection = page.getByTestId('project-cleanup-preview');
    await expect(cleanupSection).toBeVisible({ timeout: 8_000 });

    // ── 3. Protected categories are ALWAYS documented, even pre-scan
    //       (constitution II — protected categories/cleanup exclusions MUST
    //       be documented before any cleanup plan can be generated) ──────────
    const protectedBlock = cleanupSection.getByTestId('cleanup-protected');
    await expect(protectedBlock).toBeVisible();
    await expect(protectedBlock.getByText('Accepted outputs')).toBeVisible();
    await expect(
      protectedBlock.getByText('Master calibration frames'),
    ).toBeVisible();
    await expect(
      protectedBlock.getByText('Source acquisition frames'),
    ).toBeVisible();

    // Before any scan, the section shows the scan prompt, not candidates.
    await expect(
      cleanupSection.getByText(
        'Scan this project to preview cleanup candidates.',
      ),
    ).toBeVisible();

    // ── 4. Scan (read-only preview, D11 step 1) ───────────────────────────────
    const scanBtn = cleanupSection.getByTestId('cleanup-scan-btn');
    await expect(scanBtn).toBeVisible();
    await scanBtn.click();

    // Reclaimable total renders once the scan resolves. It nets out the
    // protected master: 536_870_912 B = 512.0 MB, not the 1.0 GB gross sum.
    await expect(cleanupSection.getByTestId('cleanup-reclaimable')).toHaveText(
      '512.0 MB reclaimable',
      { timeout: 5_000 },
    );

    // ── 5. Candidates render grouped by classification with parsed confidence ─
    const intermediateGroup = cleanupSection.getByTestId(
      'cleanup-group-intermediate',
    );
    await expect(intermediateGroup).toBeVisible();
    // Two intermediate candidates, both 90% confidence, normal protection.
    await expect(intermediateGroup.getByText('90%')).toHaveCount(2);

    const masterGroup = cleanupSection.getByTestId('cleanup-group-master');
    await expect(masterGroup).toBeVisible();
    await expect(masterGroup.getByText('95%')).toBeVisible();

    // The master candidate is protected: locked, NO affordance to include it,
    // clearly marked with the shared protected pill.
    const protectedRow = masterGroup.getByTestId('cleanup-candidate-0');
    await expect(protectedRow).toHaveClass(/pv-cleanup-scan__row--protected/);
    await expect(protectedRow.getByText('Protected')).toBeVisible();

    // ── 6. Destructive-destination picker uses canonical archive|trash vocab
    //       (spec 033 vocab split) — never the legacy "os_trash" wording ──────
    // exact:true — the destination hint text ("App-managed archive
    // folder — reversible…") also contains the substring "archive folder".
    await expect(
      cleanupSection.getByText('Archive folder', { exact: true }),
    ).toBeVisible();
    await expect(
      cleanupSection.getByText('System trash', { exact: true }),
    ).toBeVisible();
    await expect(cleanupSection.getByText(/os_trash/i)).toHaveCount(0);

    // ── 7. Generate the reviewable plan (D11 step 2) — no mutation yet ───────
    const generateBtn = cleanupSection.getByTestId('cleanup-generate-btn');
    await generateBtn.click();

    await expect(
      page.getByText(
        'Cleanup plan created with 3 items — review before anything is applied.',
      ),
    ).toBeVisible({ timeout: 5_000 });

    // ── 8. The shared PlanReviewOverlay opens automatically ──────────────────
    const overlay = page.getByTestId('plan-review-overlay');
    await expect(overlay).toBeVisible({ timeout: 5_000 });
    await expect(overlay.getByText('Archive folder')).toBeVisible();
    await expect(
      overlay.getByText(/Nothing has been changed on disk/),
    ).toBeVisible();

    // Every proposed item is listed before approval (FR-003/SC-001).
    await expect(overlay.getByTestId('plan-review-items')).toBeVisible();
    // exact:true — the item's "from" path cell also contains this filename
    // as a substring (…/registered/Ha_300s_r_0001.xisf).
    await expect(
      overlay.getByText('Ha_300s_r_0001.xisf', { exact: true }),
    ).toBeVisible();

    // ── 9. Spec-016 protection gate blocks approval until acknowledged ───────
    const approveBtn = overlay.getByTestId('plan-review-approve-apply');
    await expect(approveBtn).toBeDisabled();

    await overlay.getByRole('button', { name: 'Acknowledge' }).click();

    // ── 9b. Destructive-confirm gate (FR-003, D9, issue #741): the shared
    //        plan fixture carries `delete` items, so approval also stays
    //        locked behind an explicit destructive-confirm checkbox ───────────
    // `.click()` (not `.check()`) — the checkbox's `onChange` awaits a mock
    // IPC round-trip before flipping `checked`, and `.check()`'s single
    // post-click snapshot races that async update.
    await overlay.getByTestId('plan-review-confirm-destructive').click();
    await expect(
      overlay.getByTestId('plan-review-confirm-destructive'),
    ).toBeChecked({ timeout: 5_000 });
    await expect(approveBtn).toBeEnabled({ timeout: 5_000 });

    // ── 10. Approve & apply → plans.approve → plans.apply, live progress ────
    await approveBtn.click();

    await expect(overlay.getByTestId('plan-review-progress')).toBeVisible({
      timeout: 5_000,
    });
    await expect(overlay.getByText('1 item applied')).toBeVisible();
    await expect(page.getByText('Cleanup plan applied.')).toBeVisible({
      timeout: 5_000,
    });
  });
});
