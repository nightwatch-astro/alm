// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />

import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { RouteError } from './RouteError';

describe('RouteError', () => {
  it('renders with data-testid="route-error"', () => {
    render(<RouteError error={new Error('boom')} reset={() => {}} />);
    expect(screen.getByTestId('route-error')).toBeInTheDocument();
  });

  it('shows the error message', () => {
    render(
      <RouteError error={new Error('Something exploded')} reset={() => {}} />,
    );
    expect(screen.getByText('Something exploded')).toBeInTheDocument();
  });

  it('calls reset when the retry button is clicked', () => {
    const reset = vi.fn();
    render(<RouteError error={new Error('err')} reset={reset} />);
    fireEvent.click(screen.getByTestId('route-error-reset'));
    expect(reset).toHaveBeenCalledOnce();
  });
});
