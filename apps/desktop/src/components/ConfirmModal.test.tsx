// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * ConfirmModal tests — the confirm-dialog chrome contract, including the
 * `hideClose` default. Six flows share this component, three of which had no
 * title-bar close button before adopting it, so both states are pinned here.
 */

import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { ConfirmModal } from './ConfirmModal';

function renderConfirm(
  props: Partial<React.ComponentProps<typeof ConfirmModal>> = {},
) {
  const onClose = vi.fn();
  const onConfirm = vi.fn();
  render(
    <ConfirmModal
      open
      onClose={onClose}
      onConfirm={onConfirm}
      title="Archive master"
      message="This master is in use."
      actionLabel="Archive"
      {...props}
    />,
  );
  return { onClose, onConfirm };
}

describe('ConfirmModal', () => {
  it('renders the title, message, and action label', () => {
    renderConfirm();
    expect(screen.getByText('Archive master')).toBeInTheDocument();
    expect(screen.getByText('This master is in use.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Archive' })).toBeInTheDocument();
  });

  it('calls onConfirm from the action button', () => {
    const { onConfirm } = renderConfirm();
    fireEvent.click(screen.getByRole('button', { name: 'Archive' }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it('calls onClose from the Cancel button', () => {
    const { onClose } = renderConfirm();
    const cancel = screen
      .getAllByRole('button')
      .find((b) => b.textContent !== 'Archive' && b.textContent !== '');
    expect(cancel).toBeDefined();
    if (cancel) fireEvent.click(cancel);
    expect(onClose).toHaveBeenCalled();
  });

  it('disables both footer buttons while busy', () => {
    renderConfirm({ busy: true });
    expect(screen.getByRole('button', { name: 'Archive' })).toBeDisabled();
  });

  it('renders an inline error below the message', () => {
    renderConfirm({ error: 'root.has_dependents' });
    expect(screen.getByText('root.has_dependents')).toBeInTheDocument();
  });

  it('renders extra children between the message and the footer', () => {
    renderConfirm({ children: <input aria-label="fallback site" /> });
    expect(screen.getByLabelText('fallback site')).toBeInTheDocument();
  });

  it('hides the title-bar close button by default', () => {
    renderConfirm();
    expect(
      screen.queryByRole('button', { name: 'Close' }),
    ).not.toBeInTheDocument();
  });

  it('shows the title-bar close button when hideClose is false', () => {
    renderConfirm({ hideClose: false });
    expect(screen.getByRole('button', { name: 'Close' })).toBeInTheDocument();
  });

  it('wires the close button to onClose', () => {
    const { onClose } = renderConfirm({ hideClose: false });
    fireEvent.click(screen.getByRole('button', { name: 'Close' }));
    expect(onClose).toHaveBeenCalled();
  });

  it('applies the data-testid to the popup and derives the confirm button id', () => {
    renderConfirm({ 'data-testid': 'archive-confirm' });
    expect(
      screen.getByTestId('archive-confirm-confirm-btn'),
    ).toBeInTheDocument();
  });
});
