// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * AppErrorBoundary — spec 028 C2.
 *
 * React class component error boundary. Catches uncaught render errors in the
 * component tree and shows a recoverable fallback instead of a white screen.
 *
 * Usage:
 *   - App shell: wraps <RouterProvider /> in main.tsx
 *   - Per-route: can be used around any lazy-loaded page component
 *
 * React error boundaries must be class components (hooks cannot catch errors).
 */

import { Component } from 'react';
import type { ReactNode, ErrorInfo } from 'react';
import { m } from '@/lib/i18n';
import { logError } from '@/lib/diagnosticLog';

interface Props {
  children: ReactNode;
  /** Optional custom fallback. Receives the error for display. */
  fallback?: (error: Error, reset: () => void) => ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class AppErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    // Log to console so dev tools and the bottom log viewer can surface it.
    console.error(
      '[AppErrorBoundary] Uncaught render error:',
      error,
      info.componentStack,
    );
    logError(
      `[AppErrorBoundary] ${error.name}: ${error.message}${info.componentStack ?? ''}`,
    );
  }

  handleReset = () => {
    this.setState({ hasError: false, error: null });
  };

  render() {
    const { hasError, error } = this.state;
    const { children, fallback } = this.props;

    if (hasError && error) {
      if (fallback) {
        return fallback(error, this.handleReset);
      }
      return (
        <div
          role="alert"
          data-testid="app-error-boundary-fallback"
          className="pv-error-boundary__overlay"
        >
          <h1 className="pv-error-boundary__heading">
            {m.shell_error_heading()}
          </h1>
          <p className="pv-error-boundary__body">{m.shell_error_body()}</p>
          {error.message && (
            <pre className="pv-error-boundary__detail">{error.message}</pre>
          )}
          <button
            type="button"
            onClick={this.handleReset}
            data-testid="app-error-boundary-reset"
            className="pv-error-boundary__reset-btn"
          >
            {m.common_try_again()}
          </button>
        </div>
      );
    }

    return children;
  }
}
