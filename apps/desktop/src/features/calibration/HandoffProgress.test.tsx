// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * HandoffProgress tests — spec 062 US4.
 *
 * Tests:
 * 1. Verifying state renders progress bar and state label.
 * 2. Cancel button renders when cancelSafe is true and state is verifying.
 * 3. Cancel button is absent when cancelSafe is false.
 * 4. Cancel button calls onCancel with the operationId.
 * 5. Cancelling state renders state label; no cancel button.
 * 6. Applied state renders applied label; no progress bar.
 * 7. Failed state renders failure banner for each failure code.
 * 8. Progress bar has correct ARIA attributes.
 */

import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { HandoffProgress } from './HandoffProgress';
import type { CalibrationHandoffOperation } from './calibrationHandoffTypes';

function makeOp(
  overrides: Partial<CalibrationHandoffOperation> = {},
): CalibrationHandoffOperation {
  return {
    operationId: 'op-001',
    handoffId: 'handoff-001',
    state: 'verifying',
    verifiedFrameCount: 50,
    totalFrameCount: 200,
    verifiedSourceBytes: 0,
    totalSourceBytes: 0,
    cancelSafe: true,
    updatedAt: '2026-07-25T00:00:00Z',
    ...overrides,
  };
}

describe('HandoffProgress', () => {
  it('renders state label in verifying state', () => {
    render(<HandoffProgress operation={makeOp()} onCancel={vi.fn()} />);
    expect(screen.getByTestId('handoff-state-label').textContent).toContain(
      'Verifying',
    );
  });

  it('renders progress bar when verifying', () => {
    render(<HandoffProgress operation={makeOp()} onCancel={vi.fn()} />);
    const bar = screen.getByTestId('handoff-progress-bar');
    expect(bar).toBeInTheDocument();
    expect(bar).toHaveAttribute('aria-valuenow', '50');
    expect(bar).toHaveAttribute('aria-valuemax', '200');
  });

  it('shows cancel button when cancelSafe is true and state is verifying', () => {
    render(
      <HandoffProgress
        operation={makeOp({ cancelSafe: true })}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByTestId('handoff-cancel-btn')).toBeInTheDocument();
  });

  it('hides cancel button when cancelSafe is false', () => {
    render(
      <HandoffProgress
        operation={makeOp({ cancelSafe: false })}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.queryByTestId('handoff-cancel-btn')).toBeNull();
  });

  it('calls onCancel with the operationId when cancel is clicked', () => {
    const onCancel = vi.fn();
    render(<HandoffProgress operation={makeOp()} onCancel={onCancel} />);
    fireEvent.click(screen.getByTestId('handoff-cancel-btn'));
    expect(onCancel).toHaveBeenCalledWith('op-001');
  });

  it('hides cancel button when cancelling prop is true', () => {
    render(
      <HandoffProgress
        operation={makeOp({ cancelSafe: true })}
        onCancel={vi.fn()}
        cancelling
      />,
    );
    expect(screen.queryByTestId('handoff-cancel-btn')).toBeNull();
  });

  it('renders cancelling state without cancel button', () => {
    render(
      <HandoffProgress
        operation={makeOp({ state: 'cancelling', cancelSafe: false })}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByTestId('handoff-state-label').textContent).toContain(
      'Cancelling',
    );
    expect(screen.queryByTestId('handoff-cancel-btn')).toBeNull();
  });

  it('renders applied state without progress bar', () => {
    render(
      <HandoffProgress
        operation={makeOp({ state: 'applied', cancelSafe: false })}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByTestId('handoff-state-label').textContent).toContain(
      'Applied',
    );
    expect(screen.queryByTestId('handoff-progress-bar')).toBeNull();
  });

  it('renders failure banner for source_unavailable', () => {
    render(
      <HandoffProgress
        operation={makeOp({
          state: 'failed',
          cancelSafe: false,
          failureCode: 'calibration.source_unavailable',
        })}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByTestId('handoff-failure-banner')).toBeInTheDocument();
    expect(screen.getByTestId('handoff-failure-banner').textContent).toContain(
      'source frames',
    );
  });

  it('renders failure banner for handoff_too_large', () => {
    render(
      <HandoffProgress
        operation={makeOp({
          state: 'failed',
          cancelSafe: false,
          failureCode: 'calibration.handoff_too_large',
        })}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByTestId('handoff-failure-banner').textContent).toContain(
      'maximum handoff size',
    );
  });
});
