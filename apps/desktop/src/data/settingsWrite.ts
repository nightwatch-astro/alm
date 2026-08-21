// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * The single write path for `settings.update`.
 *
 * `settingsQueryOptions` caches scope reads with `staleTime: Infinity`, so a
 * scope that is written without invalidating `queryKeys.settings.scope(scope)`
 * is read from cache for the rest of the session. Nine call sites wrote
 * `commands.settingsUpdate` directly, two of them on scopes a `useQuery` caller
 * already reads (`advanced` from the log panel, `cleanup` from the setup
 * wizard), which left those panes showing a value the user had just changed.
 *
 * This module holds the invalidation so no caller can forget it, and
 * `settingsWrite.chokepoint.test.ts` fails when a module outside it calls
 * `commands.settingsUpdate`. It stays separate from `features/settings/
 * settingsIpc.ts` so the lazy writers in `data/` can reach the chokepoint
 * through a dynamic import without pulling the whole IPC surface into the boot
 * bundle.
 */

import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import { queryClient } from '@/data/queryClient';
import { queryKeys } from '@/data/queryKeys';

/** Write one settings scope and invalidate its cached read. Throws on IPC or
 *  validation failure; callers that treat the write as best-effort catch. */
export async function updateSettings(args: {
  scope: string;
  values: Record<string, unknown>;
}): Promise<void> {
  unwrap(await commands.settingsUpdate(args.scope, args.values));
  await queryClient.invalidateQueries({
    queryKey: queryKeys.settings.scope(args.scope),
  });
}
