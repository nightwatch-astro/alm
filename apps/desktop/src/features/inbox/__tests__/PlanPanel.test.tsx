// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * PlanPanel aggregate-surface tests (spec 041, #1/#2 + US7 T041).
 *
 * Covers:
 * - Renders ALL open plans at once, grouped by ingestion, with the total count.
 * - Per-group selection checkboxes + select-all/none.
 * - Apply selected passes exactly the checked ingestion ids.
 * - Apply all + per-group Discard callbacks fire.
 * - Stale plans show a badge, cannot be selected, and are excluded from select-all.
 * - Destructive control shown only when an action requires destructive confirm,
 *   defaults to archive, and reports 'trash' (the backend-accepted value).
 * - Empty plan list renders nothing.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

const { mockPlansConfirmDestructive } = vi.hoisted(() => ({
  mockPlansConfirmDestructive: vi.fn(),
}));

vi.mock('@/bindings/index', () => ({
  commands: {
    plansConfirmDestructive: mockPlansConfirmDestructive,
  },
}));

// Per-signature spy types. `vi.fn<Fn>()` yields a callable `Mock<Fn>` that the
// project's strict types accept as a plain function prop — unlike the bare
// `vi.fn()` whose `Mock<Procedure | Constructable>` is not callable at the
// type level here.
type DestSpy = ReturnType<typeof vi.fn<(d: DestructiveDestination) => void>>;
type ApplySelectedSpy = ReturnType<typeof vi.fn<(ids: string[]) => void>>;
type ApplyAllSpy = ReturnType<typeof vi.fn<() => void>>;
type CancelSpy = ReturnType<typeof vi.fn<(id: string) => void>>;
import type { ComponentProps } from 'react';
import { PlanPanel } from '../PlanPanel';
import type { DestructiveDestination } from '../PlanPanel';
import type { InboxOpenPlan, InboxPlanAction } from '../store';

// ── Fixtures ──────────────────────────────────────────────────────────────────

function makeAction(overrides: Partial<InboxPlanAction> = {}): InboxPlanAction {
  return {
    index: 0,
    action: 'move',
    fromPath: '/root/lights/img001.fits',
    toPath: '/dest/lights/img001.fits',
    destinationPreview: '/dest/lights/img001.fits',
    requiresDestructiveConfirm: false,
    ...overrides,
  };
}

function makePlan(overrides: Partial<InboxOpenPlan> = {}): InboxOpenPlan {
  return {
    inboxItemId: 'item-1',
    itemName: '2026-06-01/NGC7000',
    planId: 'plan-1',
    state: 'open',
    stale: false,
    actions: [makeAction()],
    ...overrides,
  };
}

// PlanPanel's fn props accept any `Mock` (a Mock is callable), so the test
// types its callbacks as `Mock` to sidestep the project's strict `vi.fn()`
// inference (`Mock<Procedure | Constructable>`) that won't narrow to a
// specific call signature.
interface RenderOpts {
  destructiveDestination?: DestructiveDestination;
  onDestructiveDestinationChange?: DestSpy;
  onApplySelected?: ApplySelectedSpy;
  onApplyAll?: ApplyAllSpy;
  onCancel?: CancelSpy;
  busy?: boolean;
}

function renderPanel(
  plans: InboxOpenPlan[],
  {
    destructiveDestination = 'archive',
    onDestructiveDestinationChange = vi.fn<
      (d: DestructiveDestination) => void
    >(),
    onApplySelected = vi.fn<(ids: string[]) => void>(),
    onApplyAll = vi.fn<() => void>(),
    onCancel = vi.fn<(id: string) => void>(),
    busy = false,
  }: RenderOpts = {},
) {
  const totalActions = plans.reduce((n, p) => n + p.actions.length, 0);
  return render(
    <PlanPanel
      plans={plans}
      totalActions={totalActions}
      destructiveDestination={destructiveDestination}
      onDestructiveDestinationChange={onDestructiveDestinationChange}
      onApplySelected={onApplySelected}
      onApplyAll={onApplyAll}
      onCancel={onCancel}
      busy={busy}
    />,
  );
}

// ── Tests ─────────────────────────────────────────────────────────────────────

// This suite renders PlanPanel with up to 41 action rows plus per-type
// aggregation; the default 5s vitest timeout has been observed to be too
// tight on slower (Windows CI) runners even though every individual test
// completes in well under 100ms locally — raise it explicitly rather than
// papering over slow-runner variance test by test.
describe('PlanPanel (aggregate surface)', { timeout: 15_000 }, () => {
  let onApplySelected: ApplySelectedSpy;
  let onApplyAll: ApplyAllSpy;
  let onCancel: CancelSpy;
  let onDestructiveChange: DestSpy;

  beforeEach(() => {
    onApplySelected = vi.fn<(ids: string[]) => void>();
    onApplyAll = vi.fn<() => void>();
    onCancel = vi.fn<(id: string) => void>();
    onDestructiveChange = vi.fn<(d: DestructiveDestination) => void>();
    mockPlansConfirmDestructive.mockReset();
    mockPlansConfirmDestructive.mockResolvedValue({
      status: 'ok',
      data: { planId: 'plan-1', itemsConfirmed: 1 },
    });
  });

  // ── Aggregate rendering ────────────────────────────────────────────────────

  it('renders nothing when there are no open plans', () => {
    const { container } = renderPanel([]);
    expect(container.firstChild).toBeNull();
  });

  it('renders every open plan grouped by ingestion, with the total count', () => {
    const plans = [
      makePlan({
        inboxItemId: 'a',
        itemName: 'Night-A',
        actions: [makeAction({ index: 0 })],
      }),
      makePlan({
        inboxItemId: 'b',
        itemName: 'Night-B',
        actions: [
          makeAction({ index: 0 }),
          makeAction({ index: 1, action: 'catalogue' }),
        ],
      }),
    ];
    renderPanel(plans);
    expect(screen.getByTestId('plan-group-a')).toBeInTheDocument();
    expect(screen.getByTestId('plan-group-b')).toBeInTheDocument();
    expect(screen.getByText('Night-A')).toBeInTheDocument();
    expect(screen.getByText('Night-B')).toBeInTheDocument();
    // 2 plans · 3 actions total.
    expect(screen.getByTestId('plan-total-count')).toHaveTextContent('2 plans');
    expect(screen.getByTestId('plan-total-count')).toHaveTextContent(
      '3 actions',
    );
  });

  it('renders the action destination preview and basename per row when expanded', () => {
    const plans = [
      makePlan({
        inboxItemId: 'item-1',
        actions: [
          makeAction({
            fromPath: '/deep/path/ngc1234.fits',
            destinationPreview: '/dest/lights/ngc1234.fits',
          }),
        ],
      }),
    ];
    renderPanel(plans);
    // Rows are collapsed by default — file basename + dest only appear once the
    // group is expanded.
    expect(screen.queryByText('ngc1234.fits')).toBeNull();
    fireEvent.click(screen.getByTestId('plan-group-toggle-item-1'));
    expect(screen.getByText('ngc1234.fits')).toBeInTheDocument();
    expect(screen.getByText('/dest/lights/ngc1234.fits')).toBeInTheDocument();
  });

  // #606: the collapsed group header (the default view — no expand needed)
  // must show source (the ingestion name) AND destination together in the
  // SAME row, without requiring the reviewer to expand file rows.
  it('#606: the collapsed group header shows source and destination together', () => {
    const plans = [
      makePlan({
        inboxItemId: 'from-to',
        itemName: '2026-06-01/NGC7000',
        actions: [
          makeAction({
            fromPath: '/deep/path/ngc1234.fits',
            destinationPreview: '/dest/lights/ngc1234.fits',
          }),
        ],
      }),
    ];
    renderPanel(plans);
    const group = screen.getByTestId('plan-group-from-to');
    expect(group).toHaveTextContent('2026-06-01/NGC7000');
    expect(group).toHaveTextContent('lights');
  });

  it('collapses file rows by default and shows a per-group summary line', () => {
    const plans = [
      makePlan({
        inboxItemId: 'g1',
        actions: [
          makeAction({
            index: 0,
            fromPath: '/in/d1.fits',
            destinationPreview: '/lib/masters/darks/d1.fits',
          }),
          makeAction({
            index: 1,
            fromPath: '/in/d2.fits',
            destinationPreview: '/lib/masters/darks/d2.fits',
          }),
        ],
      }),
    ];
    renderPanel(plans);
    // No per-file rows by default.
    expect(screen.queryByText('d1.fits')).toBeNull();
    // Summary present: "2 darks" in the composition cell, the destination in
    // its own aligned cell ("…/masters/darks").
    const summary = screen.getByTestId('plan-group-summary-g1');
    expect(summary).toHaveTextContent('2 darks');
    const group = screen.getByTestId('plan-group-g1');
    expect(screen.getByTestId('plan-group-dest-g1')).toHaveTextContent(
      'masters/darks',
    );
  });

  // ── #75: frame-type-hint aggregation for catalogue actions ───────────────────

  it('aggregates catalogue actions by FRAME TYPE using frameTypeByItemId (no per-file degeneration)', () => {
    // Catalogue actions keep files in place, so the destination path carries no
    // frame keyword. Without the item hint these collapse to "N catalogue"; with
    // it they aggregate to the real frame type from the item's classification.
    const plans = [
      makePlan({
        inboxItemId: 'cat',
        itemName: '(root)',
        actions: [
          makeAction({
            index: 0,
            action: 'catalogue',
            fromPath: '/lib/cam/a.fits',
            destinationPreview: '/lib/cam/a.fits',
          }),
          makeAction({
            index: 1,
            action: 'catalogue',
            fromPath: '/lib/cam/b.fits',
            destinationPreview: '/lib/cam/b.fits',
          }),
          makeAction({
            index: 2,
            action: 'catalogue',
            fromPath: '/lib/cam/c.fits',
            destinationPreview: '/lib/cam/c.fits',
          }),
        ],
      }),
    ];
    render(
      <PlanPanel
        plans={plans}
        totalActions={3}
        destructiveDestination="archive"
        onDestructiveDestinationChange={vi.fn()}
        onApplySelected={vi.fn()}
        onApplyAll={vi.fn()}
        onCancel={vi.fn()}
        frameTypeByItemId={{ cat: 'bias' }}
      />,
    );
    const summary = screen.getByTestId('plan-group-summary-cat');
    // Single aggregated frame-type line — "3 bias", NOT three "catalogue" rows.
    expect(summary).toHaveTextContent('3 bias');
    expect(summary).not.toHaveTextContent('catalogue');
    // Still collapsed: no per-file rows visible.
    expect(screen.queryByText('a.fits')).toBeNull();
  });

  // ── #75: per-type BREAKDOWN summary (collapsed by default, not per-file) ─────

  it('is COLLAPSED by default and renders the per-type breakdown (not per-file rows)', () => {
    // A MIXED ingestion: the plan actions carry no per-file frame type, so the
    // single hint would mislabel every file. The breakdown supplies the true
    // per-type tally, rendered as ONE summary line.
    const plans = [
      makePlan({
        inboxItemId: 'mix',
        itemName: '(root)',
        actions: [
          makeAction({
            index: 0,
            action: 'catalogue',
            fromPath: '/lib/cam/bias_001.fits',
            destinationPreview: '/lib/cam/bias_001.fits',
          }),
          makeAction({
            index: 1,
            action: 'catalogue',
            fromPath: '/lib/cam/dark_001.fits',
            destinationPreview: '/lib/cam/dark_001.fits',
          }),
          makeAction({
            index: 2,
            action: 'catalogue',
            fromPath: '/lib/cam/light_001.fits',
            destinationPreview: '/lib/cam/light_001.fits',
          }),
        ],
      }),
    ];
    render(
      <PlanPanel
        plans={plans}
        totalActions={3}
        destructiveDestination="archive"
        onDestructiveDestinationChange={vi.fn()}
        onApplySelected={vi.fn()}
        onApplyAll={vi.fn()}
        onCancel={vi.fn()}
        breakdownByItemId={{
          mix: [
            { kind: 'bias', count: 10 },
            { kind: 'dark', count: 21 },
            { kind: 'light', count: 12 },
          ],
        }}
      />,
    );
    // COLLAPSED by default: no per-file rows are rendered.
    expect(screen.queryByText('bias_001.fits')).toBeNull();
    expect(screen.queryByText('dark_001.fits')).toBeNull();
    // The summary shows the per-type breakdown — each type with its REAL count
    // (10 bias · 21 dark · 12 light), NOT one mislabelled per-file line and NOT
    // everything collapsed to a single wrong type.
    const summary = screen.getByTestId('plan-group-summary-mix');
    expect(summary).toHaveTextContent('10');
    expect(summary).toHaveTextContent('bias');
    expect(summary).toHaveTextContent('21');
    expect(summary).toHaveTextContent('dark');
    expect(summary).toHaveTextContent('12');
    expect(summary).toHaveTextContent('light');
    // It is a SINGLE composition cell (all types inline), not per-type rows.
    expect(
      summary.querySelectorAll('[data-testid="plan-summary-type"]'),
    ).toHaveLength(3);

    // The ▸ toggle expands the per-file rows on demand.
    fireEvent.click(screen.getByTestId('plan-group-toggle-mix'));
    expect(screen.getByText('bias_001.fits')).toBeInTheDocument();
  });

  // ── #98: PRELOADED breakdown makes the collapsed summary accurate for an
  // UNSELECTED mixed folder (no dominant-type degeneration) ───────────────────

  it('renders the accurate per-type breakdown for an UNSELECTED item when its breakdown is preloaded (#98)', () => {
    // A mixed in-place catalogue folder. The plan actions carry NO per-file
    // frame type and NO frame keyword in their (in-place) destinations, so the
    // action-only fallback would degenerate to one dominant guess (e.g.
    // "41 catalogue"/"41 darks"). PlanPanel receives no `frameTypeByItemId` for
    // this id (it is not the selected item) — only the PRELOADED breakdown,
    // which the parent now fetches for every open plan regardless of selection.
    const plans = [
      makePlan({
        inboxItemId: 'unsel',
        itemName: '(root)',
        actions: Array.from({ length: 41 }, (_, i) =>
          makeAction({
            index: i,
            action: 'catalogue',
            fromPath: `/lib/cam/frame_${i}.fits`,
            destinationPreview: `/lib/cam/frame_${i}.fits`,
          }),
        ),
      }),
    ];
    render(
      <PlanPanel
        plans={plans}
        totalActions={41}
        destructiveDestination="archive"
        onDestructiveDestinationChange={vi.fn()}
        onApplySelected={vi.fn()}
        onApplyAll={vi.fn()}
        onCancel={vi.fn()}
        // No frameTypeByItemId — item is NOT selected. Only the preloaded
        // authoritative breakdown is supplied (the #98 fix).
        breakdownByItemId={{
          unsel: [
            { kind: 'bias', count: 10 },
            { kind: 'dark', count: 16 },
            { kind: 'flat', count: 15 },
          ],
        }}
      />,
    );
    const summary = screen.getByTestId('plan-group-summary-unsel');
    // Accurate per-type tally — NOT "41 darks" or "41 catalogue".
    expect(summary).toHaveTextContent('10');
    expect(summary).toHaveTextContent('bias');
    expect(summary).toHaveTextContent('16');
    expect(summary).toHaveTextContent('dark');
    expect(summary).toHaveTextContent('15');
    expect(summary).toHaveTextContent('flat');
    expect(summary).not.toHaveTextContent('41');
    expect(summary).not.toHaveTextContent('catalogue');
    // SINGLE collapsed composition cell with the three aligned type tokens.
    expect(
      summary.querySelectorAll('[data-testid="plan-summary-type"]'),
    ).toHaveLength(3);
  });

  // ── Selection ──────────────────────────────────────────────────────────────

  it('toggles a single group selection', () => {
    const plans = [
      makePlan({ inboxItemId: 'a' }),
      makePlan({ inboxItemId: 'b', itemName: 'B' }),
    ];
    renderPanel(plans, { onApplySelected });
    fireEvent.click(screen.getByTestId('plan-group-check-a'));
    fireEvent.click(screen.getByTestId('plan-apply-selected'));
    expect(onApplySelected).toHaveBeenCalledWith(['a']);
  });

  it('select-all checks every selectable plan and Apply selected receives all ids', () => {
    const plans = [
      makePlan({ inboxItemId: 'a' }),
      makePlan({ inboxItemId: 'b', itemName: 'B' }),
    ];
    renderPanel(plans, { onApplySelected });
    fireEvent.click(screen.getByTestId('plan-select-all'));
    fireEvent.click(screen.getByTestId('plan-apply-selected'));
    expect(onApplySelected).toHaveBeenCalledWith(['a', 'b']);
  });

  it('select-all toggles off when everything is already selected', () => {
    const plans = [makePlan({ inboxItemId: 'a' })];
    renderPanel(plans, { onApplySelected });
    const selectAll = screen.getByTestId('plan-select-all') as HTMLInputElement;
    fireEvent.click(selectAll); // select
    fireEvent.click(selectAll); // deselect
    const applyBtn = screen.getByTestId(
      'plan-apply-selected',
    ) as HTMLButtonElement;
    expect(applyBtn.disabled).toBe(true);
  });

  it('Apply selected is disabled when nothing is selected', () => {
    renderPanel([makePlan()], { onApplySelected });
    const btn = screen.getByTestId('plan-apply-selected') as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
  });

  // ── Apply all + Discard ────────────────────────────────────────────────────

  it('calls onApplyAll when Apply all is clicked', () => {
    renderPanel([makePlan()], { onApplyAll });
    fireEvent.click(screen.getByTestId('plan-apply-all'));
    expect(onApplyAll).toHaveBeenCalledOnce();
  });

  it('calls onCancel with the group id when Discard is clicked', () => {
    renderPanel([makePlan({ inboxItemId: 'zz' })], { onCancel });
    fireEvent.click(screen.getByTestId('plan-cancel-zz'));
    expect(onCancel).toHaveBeenCalledWith('zz');
  });

  // ── Stale handling ─────────────────────────────────────────────────────────

  it('shows a stale badge and disables selection for stale plans', () => {
    const plans = [makePlan({ inboxItemId: 'a', stale: true })];
    renderPanel(plans);
    expect(screen.getByTestId('plan-stale-a')).toBeInTheDocument();
    const check = screen.getByTestId('plan-group-check-a') as HTMLInputElement;
    expect(check.disabled).toBe(true);
  });

  it('select-all ignores stale plans (only selectable ids are applied)', () => {
    const plans = [
      makePlan({ inboxItemId: 'good' }),
      makePlan({ inboxItemId: 'stale', stale: true }),
    ];
    renderPanel(plans, { onApplySelected });
    fireEvent.click(screen.getByTestId('plan-select-all'));
    fireEvent.click(screen.getByTestId('plan-apply-selected'));
    expect(onApplySelected).toHaveBeenCalledWith(['good']);
  });

  // ── Destructive control ────────────────────────────────────────────────────

  it('does NOT show the destructive control when no action requires destructive confirm', () => {
    renderPanel([
      makePlan({
        actions: [makeAction({ requiresDestructiveConfirm: false })],
      }),
    ]);
    expect(screen.queryByTestId('plan-destructive-archive')).toBeNull();
  });

  it('shows the destructive control + defaults to archive when destructive confirm is required', () => {
    renderPanel(
      [
        makePlan({
          actions: [makeAction({ requiresDestructiveConfirm: true })],
        }),
      ],
      { destructiveDestination: 'archive' },
    );
    const archive = screen.getByTestId(
      'plan-destructive-archive',
    ) as HTMLInputElement;
    const trash = screen.getByTestId(
      'plan-destructive-trash',
    ) as HTMLInputElement;
    expect(archive.checked).toBe(true);
    expect(trash.checked).toBe(false);
  });

  it('reports the backend-accepted "trash" value when System Trash is chosen', () => {
    renderPanel(
      [
        makePlan({
          actions: [makeAction({ requiresDestructiveConfirm: true })],
        }),
      ],
      {
        destructiveDestination: 'archive',
        onDestructiveDestinationChange: onDestructiveChange,
      },
    );
    fireEvent.click(screen.getByTestId('plan-destructive-trash'));
    expect(onDestructiveChange).toHaveBeenCalledWith('trash');
  });

  // ── Destructive-confirm gate (FR-003, D9, issue #741) ──────────────────────

  it('blocks Apply all until the destructive-confirm checkbox is confirmed', () => {
    renderPanel([
      makePlan({
        planId: 'plan-1',
        actions: [makeAction({ requiresDestructiveConfirm: true })],
      }),
    ]);
    expect(
      (screen.getByTestId('plan-apply-all') as HTMLButtonElement).disabled,
    ).toBe(true);
    const confirmBox = screen.getByTestId(
      'plan-destructive-confirm',
    ) as HTMLInputElement;
    expect(confirmBox.checked).toBe(false);
  });

  it('confirming destructive items calls plans.confirm.destructive and unblocks Apply all', async () => {
    renderPanel([
      makePlan({
        planId: 'plan-1',
        actions: [makeAction({ requiresDestructiveConfirm: true })],
      }),
    ]);
    fireEvent.click(screen.getByTestId('plan-destructive-confirm'));
    await waitFor(() =>
      expect(mockPlansConfirmDestructive).toHaveBeenCalledWith('plan-1'),
    );
    await waitFor(() =>
      expect(
        (screen.getByTestId('plan-apply-all') as HTMLButtonElement).disabled,
      ).toBe(false),
    );
  });

  it('does not gate Apply all on destructive-confirm when no action requires it', () => {
    renderPanel([makePlan({ actions: [makeAction()] })]);
    expect(
      (screen.getByTestId('plan-apply-all') as HTMLButtonElement).disabled,
    ).toBe(false);
  });

  // ── Busy ───────────────────────────────────────────────────────────────────

  it('disables apply controls and Discard when busy', () => {
    const plans = [makePlan({ inboxItemId: 'a' })];
    renderPanel(plans, { busy: true });
    // Pre-select so Apply selected would otherwise be enabled.
    expect(
      (screen.getByTestId('plan-apply-all') as HTMLButtonElement).disabled,
    ).toBe(true);
    expect(
      (screen.getByTestId('plan-cancel-a') as HTMLButtonElement).disabled,
    ).toBe(true);
  });
});

// ── spec 041 US8/FR-029/FR-031: destination-root picker + absolute path ───────

describe('PlanPanel destination-root picker (US8)', { timeout: 15_000 }, () => {
  const pendingRootPick = {
    category: 'light_frames',
    candidates: [
      { rootId: 'root-a', path: '/lib/A', kind: 'light_frames' },
      { rootId: 'root-b', path: '/lib/B', kind: 'light_frames' },
    ],
  };

  function renderWith(extra: Partial<ComponentProps<typeof PlanPanel>>) {
    const plans = extra.plans ?? [];
    const totalActions = plans.reduce((n, p) => n + p.actions.length, 0);
    return render(
      <PlanPanel
        plans={plans}
        totalActions={totalActions}
        destructiveDestination="archive"
        onDestructiveDestinationChange={vi.fn()}
        onApplySelected={vi.fn()}
        onApplyAll={vi.fn()}
        onCancel={vi.fn()}
        {...extra}
      />,
    );
  }

  it('renders one option per candidate even with zero open plans (FR-029)', () => {
    renderWith({ plans: [], pendingRootPick, onPickDestinationRoot: vi.fn() });
    expect(screen.getByTestId('inbox-root-picker')).toBeInTheDocument();
    expect(screen.getByTestId('inbox-root-option-root-a')).toBeInTheDocument();
    expect(screen.getByTestId('inbox-root-option-root-b')).toBeInTheDocument();
  });

  it('fires onPickDestinationRoot with the chosen rootId', () => {
    const onPick = vi.fn<(rootId: string) => void>();
    renderWith({ plans: [], pendingRootPick, onPickDestinationRoot: onPick });
    fireEvent.click(screen.getByTestId('inbox-root-option-root-b'));
    expect(onPick).toHaveBeenCalledWith('root-b');
  });

  it('shows the absolute destination path from absoluteByFromPath (FR-031)', () => {
    const plan = makePlan({
      inboxItemId: 'item-1',
      actions: [
        makeAction({
          fromPath: '/inbox/img.fits',
          destinationPreview: 'lights/img.fits',
        }),
      ],
    });
    renderWith({
      plans: [plan],
      absoluteByFromPath: { '/inbox/img.fits': '/lib/A/lights/img.fits' },
    });
    // Per-file rows (and their absolute-dest cell) are collapsed until expanded.
    fireEvent.click(screen.getByTestId('plan-group-toggle-item-1'));
    expect(screen.getByTestId('inbox-dest-absolute-0')).toHaveTextContent(
      '/lib/A/lights/img.fits',
    );
  });

  it('falls back to the relative preview when no absolute path is captured', () => {
    const plan = makePlan({
      inboxItemId: 'item-1',
      actions: [
        makeAction({
          fromPath: '/inbox/img.fits',
          destinationPreview: 'lights/img.fits',
        }),
      ],
    });
    renderWith({ plans: [plan] });
    fireEvent.click(screen.getByTestId('plan-group-toggle-item-1'));
    expect(screen.getByTestId('inbox-dest-absolute-0')).toHaveTextContent(
      'lights/img.fits',
    );
  });
});
