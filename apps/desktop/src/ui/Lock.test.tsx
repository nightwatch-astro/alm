// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * Lock keyboard-reachability tests (WCAG 1.4.13 / 2.1.1).
 *
 * Same defect class as #1103: the shared `Tooltip` renders its trigger as a
 * bare `<span>` and base-ui adds no `tabIndex`, so a caller that relies on the
 * trigger alone for reveal is pointer-only. `Lock`'s reason is not redundant
 * prose — it states a consequence shown nowhere else — so it must be reachable
 * and exposed to assistive tech.
 */

import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';

import { Lock } from './Lock';

describe('Lock accessibility', () => {
  it('exposes the reason to assistive tech via a role that supports naming', () => {
    render(<Lock reason="Protected — needs acknowledgement" />);

    // A role-less <span> does not reliably expose aria-label, so the role is
    // what makes the name reachable at all.
    const trigger = screen.getByRole('note', {
      name: 'Protected — needs acknowledgement',
    });
    expect(trigger).toBeInTheDocument();
  });

  it('is keyboard focusable so the tooltip is not pointer-only', () => {
    render(<Lock reason="Protected — needs acknowledgement" />);

    const trigger = screen.getByRole('note');
    expect(trigger).toHaveAttribute('tabindex', '0');

    trigger.focus();
    expect(trigger).toHaveFocus();
  });

  it('falls back to the generic protected label when no reason is given', () => {
    render(<Lock />);
    expect(screen.getByRole('note', { name: 'Protected' })).toBeInTheDocument();
  });

  // The decorative variant is the deliberate exception: it is only correct
  // where the reason is stated in text nearby AND is identical for every
  // instance, so N focus stops would announce the same sentence N times.
  it('decorative locks are hidden from assistive tech and out of the tab order', () => {
    const { container } = render(<Lock decorative />);

    expect(screen.queryByRole('note')).not.toBeInTheDocument();

    const glyph = container.querySelector('[data-testid="lock-glyph"]');
    expect(glyph).not.toBeNull();
    expect(glyph).toHaveAttribute('aria-hidden', 'true');
    expect(glyph).not.toHaveAttribute('tabindex');
  });

  it('decorative locks still render the padlock glyph', () => {
    const { container } = render(<Lock decorative />);
    expect(container.querySelector('[data-testid="lock-glyph"]')?.textContent).toBe('\u{1F512}');
  });
});
