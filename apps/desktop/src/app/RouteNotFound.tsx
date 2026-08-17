// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * RouteNotFound — TanStack Router notFoundComponent.
 *
 * Rendered by TanStack Router when a navigation matches no registered route.
 * Uses a Link to navigate home, avoiding the need for a useNavigate mock in tests.
 */

import { Link } from '@tanstack/react-router';
import { m } from '@/lib/i18n';

export function RouteNotFound() {
  return (
    <div
      role="alert"
      data-testid="route-not-found"
      className="pv-error-boundary__overlay"
    >
      <h1 className="pv-error-boundary__heading">
        {m.route_not_found_heading()}
      </h1>
      <p className="pv-error-boundary__body">{m.route_not_found_body()}</p>
      <Link to="/" className="pv-error-boundary__reset-btn">
        {m.route_not_found_home()}
      </Link>
    </div>
  );
}
