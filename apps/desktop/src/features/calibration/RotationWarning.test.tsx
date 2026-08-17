// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * RotationWarning tests — spec 041 T080 · FR-040.
 *
 * Verifies the flat↔light rotation warning surface:
 * 1. No warning (null) → renders nothing (exact rotation agreement).
 * 2. Deviation warning → renders the deviation banner with the degree delta.
 * 3. RotationUnavailable warning → renders the "matched without rotation" banner.
 * 4. Warning is non-blocking (warn-variant banner, not danger/error).
 * 5. Degree delta is formatted compactly.
 */

import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { RotationWarningNotice } from './RotationWarning';

describe('RotationWarningNotice', () => {
  it('renders nothing when there is no warning (exact rotation match)', () => {
    const { container } = render(<RotationWarningNotice warning={null} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('renders nothing when warning is undefined', () => {
    const { container } = render(<RotationWarningNotice warning={undefined} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('renders the deviation banner with the degree delta', () => {
    render(<RotationWarningNotice warning={{ kind: 'deviation', deg: 0.6 }} />);
    const banner = screen.getByTestId('rotation-warning-deviation');
    expect(banner).toBeInTheDocument();
    expect(banner).toHaveTextContent('0.6');
    expect(banner).toHaveTextContent(/may not be valid/i);
  });

  it('formats the degree delta compactly (trims trailing zeros)', () => {
    render(<RotationWarningNotice warning={{ kind: 'deviation', deg: 1.5 }} />);
    const banner = screen.getByTestId('rotation-warning-deviation');
    expect(banner).toHaveTextContent('1.5');
    expect(banner).not.toHaveTextContent('1.50');
  });

  it('renders the rotation-unavailable banner', () => {
    render(
      <RotationWarningNotice warning={{ kind: 'rotation_unavailable' }} />,
    );
    const banner = screen.getByTestId('rotation-warning-rotation_unavailable');
    expect(banner).toBeInTheDocument();
    expect(banner).toHaveTextContent(/ROTATANG/);
    expect(banner).toHaveTextContent(/matched without rotation/i);
  });

  it('is non-blocking (warn-variant banner)', () => {
    render(<RotationWarningNotice warning={{ kind: 'deviation', deg: 2 }} />);
    const banner = screen.getByTestId('rotation-warning-deviation');
    // Banner renders with warn variant — verify via role (status for warn).
    expect(banner).toHaveAttribute('role', 'status');
  });

  it('exposes an accessible label on the warning icon', () => {
    render(<RotationWarningNotice warning={{ kind: 'deviation', deg: 0.6 }} />);
    expect(
      screen.getByRole('img', { name: /rotation warning/i }),
    ).toBeInTheDocument();
  });
});
