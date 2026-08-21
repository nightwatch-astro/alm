// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * TanStack Query options for the generic `settings.get` scope reads.
 *
 * `staleTime: Infinity` is safe only because every write invalidates the scope
 * it wrote: `data/settingsWrite.ts` is the one `settings.update` path, and
 * `settingsRestoreDefaults` invalidates `queryKeys.settings.all()`.
 * `data/settingsWrite.chokepoint.test.ts` is the gate on that.
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
