// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * RouteError — TanStack Router errorComponent.
 *
 * Receives `{ error, reset }` from TanStack Router when a route throws during
 * render. Reuses the AppErrorBoundary CSS classes so it is visually consistent
 * with the app-level boundary without duplicating markup.
 */

import { m } from '@/lib/i18n';

interface RouteErrorProps {
  error: Error;
  reset: () => void;
}

export function RouteError({ error, reset }: RouteErrorProps) {
  return (
    <div
      role="alert"
      data-testid="route-error"
      className="pv-error-boundary__overlay"
    >
      <h1 className="pv-error-boundary__heading">{m.shell_error_heading()}</h1>
      <p className="pv-error-boundary__body">{m.shell_error_body()}</p>
      {error.message && (
        <pre className="pv-error-boundary__detail">{error.message}</pre>
      )}
      <button
        type="button"
        onClick={reset}
        data-testid="route-error-reset"
        className="pv-error-boundary__reset-btn"
      >
        {m.common_try_again()}
      </button>
    </div>
  );
}
