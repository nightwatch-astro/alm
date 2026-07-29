// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * settings-queries.ts — TanStack Query option factory for settings.get (C-8/26).
 *
 * Wraps `getSettings({ scope })` in a `queryOptions` factory so callers can
 * replace manual `useEffect + cancelled + getSettings().then()` with a single
 * `useQuery(settingsQueryOptions(scope))` call. The factory is scope-typed so
 * TypeScript infers the narrowed return type at each call site.
 *
 * Keys live under `['settings', scope]` — prefix-invalidatable via
 * `queryClient.invalidateQueries({ queryKey: ['settings'] })` when a
 * `settingsUpdate` commits. `staleTime: Infinity` prevents background
 * refetches for a value that only changes via explicit mutation; callers that
 * need live updates after a mutation should invalidate the key.
 */

import { queryOptions } from '@tanstack/react-query';
import { getSettings } from '@/features/settings/settingsIpc';
import type { SettingsData } from '@/bindings/index';

/**
 * TanStack Query options for `settings.get(scope)`. Pass the result directly
 * to `useQuery` or `queryClient.fetchQuery`.
 *
 * @param scope  The settings scope string (e.g. `'planner'`, `'framing'`).
 */
export function settingsQueryOptions(
  scope: string,
): ReturnType<typeof queryOptions<SettingsData>> {
  return queryOptions<SettingsData>({
    queryKey: ['settings', scope] as const,
    queryFn: () => getSettings({ scope }),
    // Settings values only change via explicit writes; never refetch in the
    // background — the writer is responsible for invalidating the key.
    staleTime: Infinity,
  });
}
