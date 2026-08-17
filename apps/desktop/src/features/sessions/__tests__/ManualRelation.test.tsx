// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />

/**
 * ManualRelationDialog component tests.
 *
 * Spec 062 contract guards exercised here:
 *  - reviewReason must be non-whitespace (submit gated on trim length > 0)
 *  - new_reviewed_cross_target scope requires ≥ 2 distinct target IDs
 *  - missing evidence codes disclosed via checkboxes
 *
 * Tests:
 * 1. Dialog renders when open=true.
 * 2. Dialog is absent when open=false.
 * 3. Submit button disabled when reason is blank.
 * 4. Submit button enabled when reason is non-empty (same_target scope).
 * 5. Cross-target scope: submit disabled with < 2 IDs.
 * 6. Cross-target scope: submit enabled with ≥ 2 distinct IDs.
 * 7. Missing-evidence checkbox toggles correctly.
 * 8. Calls create mutation on submit with correct reviewReason.
 */

import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ManualRelationDialog } from '../ManualRelationDialog';

// ── Mock the mutation hook ────────────────────────────────────────────────────

const mockMutateAsync = vi
  .fn()
  .mockResolvedValue({ proposal: {}, auditId: 'a1' });
const mockMutation = {
  mutateAsync: mockMutateAsync,
  isPending: false,
};

vi.mock('../useGroupsStore', async (importOriginal) => {
  const original = await importOriginal<typeof import('../useGroupsStore')>();
  return {
    ...original,
    useRelationProposalManualCreate: () => mockMutation,
  };
});

vi.stubEnv('VITE_USE_MOCKS', 'true');

function renderDialog(
  props: Partial<React.ComponentProps<typeof ManualRelationDialog>> = {},
) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <ManualRelationDialog open={true} onClose={vi.fn()} {...props} />
    </QueryClientProvider>,
  );
}

describe('ManualRelationDialog', () => {
  it('1. renders when open=true', () => {
    renderDialog();
    expect(screen.getByTestId('manual-relation-dialog')).toBeInTheDocument();
  });

  it('2. not rendered when open=false', () => {
    renderDialog({ open: false });
    expect(
      screen.queryByTestId('manual-relation-dialog'),
    ).not.toBeInTheDocument();
  });

  it('3. submit button disabled when reason is blank', () => {
    renderDialog();
    const btn = screen.getByTestId('manual-relation-submit');
    expect(btn).toBeDisabled();
  });

  it('4. submit button enabled when reason has non-whitespace text', () => {
    renderDialog();
    const textarea = screen.getByTestId('manual-relation-reason');
    fireEvent.change(textarea, {
      target: { value: 'These sessions cover the same panel.' },
    });
    const btn = screen.getByTestId('manual-relation-submit');
    expect(btn).not.toBeDisabled();
  });

  it('5. submit disabled with fewer than 2 distinct cross-target IDs', () => {
    renderDialog();
    // Switch to new_reviewed_cross_target scope
    const radios = screen.getAllByRole('radio');
    const newCrossRadio = radios.find(
      (r) => (r as HTMLInputElement).value === 'new_reviewed_cross_target',
    ) as HTMLInputElement;
    fireEvent.click(newCrossRadio);

    // Enter reason
    const textarea = screen.getByTestId('manual-relation-reason');
    fireEvent.change(textarea, { target: { value: 'Valid reason.' } });

    // Enter only one target ID
    const idsInput = screen.getByTestId('cross-target-ids-input');
    fireEvent.change(idsInput, { target: { value: 'target-1' } });

    const btn = screen.getByTestId('manual-relation-submit');
    expect(btn).toBeDisabled();
  });

  it('6. submit enabled with ≥ 2 distinct cross-target IDs', () => {
    renderDialog();
    const radios = screen.getAllByRole('radio');
    const newCrossRadio = radios.find(
      (r) => (r as HTMLInputElement).value === 'new_reviewed_cross_target',
    ) as HTMLInputElement;
    fireEvent.click(newCrossRadio);

    const textarea = screen.getByTestId('manual-relation-reason');
    fireEvent.change(textarea, { target: { value: 'Valid reason.' } });

    const idsInput = screen.getByTestId('cross-target-ids-input');
    fireEvent.change(idsInput, { target: { value: 'target-1\ntarget-2' } });

    const btn = screen.getByTestId('manual-relation-submit');
    expect(btn).not.toBeDisabled();
  });

  it('7. missing-evidence checkbox toggles correctly', () => {
    renderDialog();
    const checkbox = screen.getByTestId(
      'missing-evidence-footprint.unavailable',
    );
    expect(checkbox).not.toBeChecked();
    fireEvent.click(checkbox);
    expect(checkbox).toBeChecked();
    fireEvent.click(checkbox);
    expect(checkbox).not.toBeChecked();
  });

  it('8. calls mutateAsync with the typed review reason on submit', async () => {
    const onClose = vi.fn();
    renderDialog({ onClose });

    const textarea = screen.getByTestId('manual-relation-reason');
    fireEvent.change(textarea, {
      target: { value: 'Intentional cross-panel observation.' },
    });

    // Submit via the submit button
    const submitBtn = screen.getByTestId('manual-relation-submit');
    fireEvent.click(submitBtn);

    // Give the async handler a tick
    await vi.waitFor(() => {
      expect(mockMutateAsync).toHaveBeenCalledWith(
        expect.objectContaining({
          reviewReason: 'Intentional cross-panel observation.',
        }),
      );
    });
  });
});
