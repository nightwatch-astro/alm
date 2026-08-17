// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * `render` for components that call `useQuery`/`useQueryClient`.
 *
 * Each call gets a fresh `QueryClient` so cached data cannot leak between
 * tests, and `retry: false` so a rejected `queryFn` surfaces on the first
 * attempt instead of after the default backoff.
 */

import type { ReactElement } from 'react';
import { render as rtlRender } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

export function renderWithQuery(ui: ReactElement) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return rtlRender(ui, {
    wrapper: ({ children }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    ),
  });
}
