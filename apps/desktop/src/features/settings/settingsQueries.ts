// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * TanStack Query options for the generic `settings.get` scope reads.
 *
 * `staleTime: Infinity` is safe only because every write path through
 * `updateSettings` / `settingsRestoreDefaults` invalidates
 * `queryKeys.settings.all()`. A writer that calls `commands.settingsUpdate`
 * directly (see `data/theme.ts`, `data/locale.tsx`, `data/persisted-state.ts`,
 * `shared/observing-sites/site-store.ts`, `shared/planner/*-settings.ts`)
 * bypasses that invalidation — those modules own scopes no `useQuery` caller
 * reads, and a new caller for one of their scopes must add invalidation there
 * first.
 */

import { queryOptions } from '@tanstack/react-query';
import { queryKeys } from '@/data/queryKeys';
import { getSettings } from './settingsIpc';
import type { SettingsData } from '@/bindings/index';

export function settingsQueryOptions(scope: string) {
  return queryOptions<SettingsData>({
    queryKey: queryKeys.settings.scope(scope),
    queryFn: () => getSettings({ scope }),
    staleTime: Number.POSITIVE_INFINITY,
  });
}
