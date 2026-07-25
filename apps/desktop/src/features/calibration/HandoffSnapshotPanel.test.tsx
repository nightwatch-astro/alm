// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * HandoffSnapshotPanel tests — spec 062 US4.
 *
 * Tests:
 * 1. Renders requirement, selection, and frame counts.
 * 2. Warning codes rendered as pills.
 * 3. no_automatic_candidate warning renders informational banner.
 * 4. Add-reviewed button is present when onAddReviewed is supplied and isHead=true.
 * 5. Add-reviewed button is disabled when isHead=false (stale snapshot).
 * 6. Clicking add-reviewed calls the callback.
 * 7. Other warning codes rendered as pills (not the no_auto_candidate banner).
 */

import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { HandoffSnapshotPanel } from './HandoffSnapshotPanel';
import type { CalibrationHandoffSnapshot } from './calibrationHandoffTypes';

function makeSnapshot(
  overrides: Partial<CalibrationHandoffSnapshot> = {},
): CalibrationHandoffSnapshot {
  return {
    handoffId: 'handoff-001',
    handoffHeadGeneration: 1,
    snapshotId: 'snap-001',
    projectId: 'proj-001',
    externalProcessor: 'pixinsight_wbpp',
    requirementCount: 3,
    selectionCount: 2,
    frameCount: 150,
    sourceByteCount: 1_073_741_824,
    maximumSourceBytes: 17_592_186_044_416,
    matchingSettingsRevision: 1,
    evaluationAt: '2026-07-25T00:00:00Z',
    createdAt: '2026-07-25T00:00:00Z',
    createdBy: 'user-001',
    basisFingerprint: 'fp-001',
    warningCodes: [],
    ...overrides,
  };
}

describe('HandoffSnapshotPanel', () => {
  it('renders requirement, selection, and frame counts', () => {
    render(<HandoffSnapshotPanel snapshot={makeSnapshot()} isHead={true} />);
    expect(screen.getByTestId('handoff-requirement-count').textContent).toBe(
      '3',
    );
    expect(screen.getByTestId('handoff-selection-count').textContent).toBe('2');
    expect(screen.getByTestId('handoff-frame-count').textContent).toBe('150');
  });

  it('renders warning codes as pills', () => {
    render(
      <HandoffSnapshotPanel
        snapshot={makeSnapshot({
          warningCodes: [
            'calibration.no_automatic_candidate',
            'calibration.other_warn',
          ],
        })}
        isHead={true}
      />,
    );
    // no_automatic_candidate → banner, not pills
    expect(screen.getByTestId('handoff-no-auto-candidate')).toBeInTheDocument();
    // calibration.other_warn → pill
    expect(
      screen.getByTestId('handoff-warning-calibration.other_warn'),
    ).toBeInTheDocument();
  });

  it('renders no_automatic_candidate informational banner', () => {
    render(
      <HandoffSnapshotPanel
        snapshot={makeSnapshot({
          warningCodes: ['calibration.no_automatic_candidate'],
        })}
        isHead={true}
      />,
    );
    expect(screen.getByTestId('handoff-no-auto-candidate')).toBeInTheDocument();
  });

  it('shows add-reviewed button when onAddReviewed is supplied and isHead=true', () => {
    render(
      <HandoffSnapshotPanel
        snapshot={makeSnapshot()}
        isHead={true}
        onAddReviewed={vi.fn()}
      />,
    );
    const btn = screen.getByTestId('handoff-add-reviewed-btn');
    expect(btn).toBeInTheDocument();
    expect(btn).not.toBeDisabled();
  });

  it('disables add-reviewed button when isHead=false (stale snapshot)', () => {
    render(
      <HandoffSnapshotPanel
        snapshot={makeSnapshot()}
        isHead={false}
        onAddReviewed={vi.fn()}
      />,
    );
    const btn = screen.getByTestId('handoff-add-reviewed-btn');
    expect(btn).toBeDisabled();
    expect(btn).toHaveAttribute('aria-disabled', 'true');
  });

  it('calls onAddReviewed when button is clicked', () => {
    const onAddReviewed = vi.fn();
    render(
      <HandoffSnapshotPanel
        snapshot={makeSnapshot()}
        isHead={true}
        onAddReviewed={onAddReviewed}
      />,
    );
    fireEvent.click(screen.getByTestId('handoff-add-reviewed-btn'));
    expect(onAddReviewed).toHaveBeenCalledOnce();
  });

  it('hides add-reviewed button when no callback is supplied', () => {
    render(<HandoffSnapshotPanel snapshot={makeSnapshot()} isHead={true} />);
    expect(screen.queryByTestId('handoff-add-reviewed-btn')).toBeNull();
  });
});
