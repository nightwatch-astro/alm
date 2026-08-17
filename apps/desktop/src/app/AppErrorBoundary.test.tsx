// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * AppErrorBoundary tests — spec 028 C2.
 *
 * Tests:
 *  1. Throwing child renders the error fallback UI.
 *  2. Fallback shows the error message in a <pre>.
 *  3. "Try again" button resets the boundary and re-mounts children.
 *  4. Custom fallback prop is used when provided.
 *  5. Non-throwing children render normally.
 */

import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
// Convention: use userEvent for user-driven interactions; fireEvent for synthetic/edge cases.
// See src/test/userEvent.ts for the project setup helper.
import { setupUser } from '../test/userEvent';
import { AppErrorBoundary } from './AppErrorBoundary';
import { logError } from '@/lib/diagnosticLog';

vi.mock('@/lib/diagnosticLog', () => ({
  logError: vi.fn(),
}));

// Suppress console.error for expected errors in these tests.
let consoleErrorSpy: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
  consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
});

afterEach(() => {
  consoleErrorSpy.mockRestore();
});

// A component that throws on render.
// Returns `never` so TypeScript allows use as JSX.
function ThrowingChild({
  message = 'Test render error',
}: {
  message?: string;
}): never {
  throw new Error(message);
}

// A component that renders normally.
function SafeChild() {
  return <div data-testid="safe-child">Hello</div>;
}

describe('AppErrorBoundary', () => {
  it('1. renders fallback UI when a child throws', () => {
    render(
      <AppErrorBoundary>
        <ThrowingChild />
      </AppErrorBoundary>,
    );
    expect(
      screen.getByTestId('app-error-boundary-fallback'),
    ).toBeInTheDocument();
  });

  it('2. shows the error message in the fallback', () => {
    render(
      <AppErrorBoundary>
        <ThrowingChild message="Boom!" />
      </AppErrorBoundary>,
    );
    expect(screen.getByText('Boom!')).toBeInTheDocument();
  });

  it('3. "Try again" button resets the boundary', async () => {
    const user = setupUser();
    let shouldThrow = true;

    function MaybeThrow() {
      if (shouldThrow) throw new Error('initial error');
      return <div data-testid="recovered">recovered</div>;
    }

    const { rerender } = render(
      <AppErrorBoundary>
        <MaybeThrow />
      </AppErrorBoundary>,
    );

    // Fallback is shown
    expect(
      screen.getByTestId('app-error-boundary-fallback'),
    ).toBeInTheDocument();

    // Fix the error condition before reset
    shouldThrow = false;

    // Click reset
    await user.click(screen.getByTestId('app-error-boundary-reset'));

    // Re-render to trigger the non-throwing path
    rerender(
      <AppErrorBoundary>
        <MaybeThrow />
      </AppErrorBoundary>,
    );

    expect(screen.getByTestId('recovered')).toBeInTheDocument();
    expect(
      screen.queryByTestId('app-error-boundary-fallback'),
    ).not.toBeInTheDocument();
  });

  it('4. uses custom fallback prop when provided', () => {
    const customFallback = (error: Error, reset: () => void) => (
      <div data-testid="custom-fallback">
        <span>{error.message}</span>
        <button onClick={reset} data-testid="custom-reset">
          Reset
        </button>
      </div>
    );

    render(
      <AppErrorBoundary fallback={customFallback}>
        <ThrowingChild message="Custom error" />
      </AppErrorBoundary>,
    );

    expect(screen.getByTestId('custom-fallback')).toBeInTheDocument();
    expect(screen.getByText('Custom error')).toBeInTheDocument();
    expect(
      screen.queryByTestId('app-error-boundary-fallback'),
    ).not.toBeInTheDocument();
  });

  it('6. forwards render errors to the diagnostics log', () => {
    render(
      <AppErrorBoundary>
        <ThrowingChild message="log-me" />
      </AppErrorBoundary>,
    );
    expect(logError).toHaveBeenCalledWith(expect.stringContaining('log-me'));
  });

  it('5. non-throwing children render normally', () => {
    render(
      <AppErrorBoundary>
        <SafeChild />
      </AppErrorBoundary>,
    );
    expect(screen.getByTestId('safe-child')).toBeInTheDocument();
    expect(
      screen.queryByTestId('app-error-boundary-fallback'),
    ).not.toBeInTheDocument();
  });
});
