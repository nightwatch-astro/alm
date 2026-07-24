// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Targets store — TanStack Query hooks (spec `tiny/targets-tanstack-query-migration`).
 *
 * Mirrors `features/sessions/store.ts`: local `unwrap(await commands.X(req))`
 * helpers + `useQuery`/`useMutation` hooks keyed via `queryKeys.targets`.
 * Mutations invalidate the affected key(s) instead of the old manual `load()`
 * refetch calls that `TargetDetailV2`/`TargetsPage` used to thread through
 * props (`onMutated`).
 */

import {
  useQuery,
  useMutation,
  useQueryClient,
  keepPreviousData,
} from '@tanstack/react-query';
import { queryKeys } from '@/data/queryKeys';
import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import type {
  TargetListItem,
  TargetSessionItem,
  TargetProjectItem,
  TargetAstroFormat,
  TargetAliasAddRequest,
  TargetAliasAddResult,
  TargetAliasRemoveRequest,
  TargetAliasRemoveResult,
  TargetDisplayAliasSetRequest,
  TargetDisplayAliasClearRequest,
  TargetNoteUpdateRequest,
  TargetNoteUpdateResult_Serialize as TargetNoteUpdateResult,
} from '@/bindings/index';
import type { TargetDetailV3 } from '@/bindings/aliases';

export type { TargetDetailV3 };

// Local IPC helpers — mirrors the rest of the store layer (unwrap turns the
// generated Result into the throw-on-error contract useQuery/useMutation rely on).

async function getTargetDetail(targetId: string): Promise<TargetDetailV3> {
  return unwrap(await commands.targetGet({ targetId })) as TargetDetailV3;
}

async function listTargetSessions(
  targetId: string,
): Promise<TargetSessionItem[]> {
  return unwrap(await commands.targetSessionsList({ targetId }));
}

async function listTargetProjects(
  targetId: string,
): Promise<TargetProjectItem[]> {
  return unwrap(await commands.targetProjectsList({ targetId }));
}

async function getTargetNotes(targetId: string): Promise<string | null> {
  const { notes } = unwrap(await commands.targetNoteGet({ targetId }));
  return notes ?? null;
}

// Query state shape (matches the other stores' backward-compat surface).

export interface QueryState<T> {
  data: T | undefined;
  loading: boolean;
  error: Error | undefined;
}

/** `useTargets()`'s state plus a manual refetch (e.g. after "Add target"). */
export interface TargetsListState extends QueryState<TargetListItem[]> {
  refetch: () => void;
}

// ── Query hooks ───────────────────────────────────────────────────────────────

/**
 * Subscribe to the targets (Planner catalogue) list.
 *
 * `search` is forwarded to the backend `target.list` endpoint (GF-11 / DS-16)
 * once the binding supports it (perf/ipc-surface, #1543) so alias-aware
 * filtering happens server-side. The query key includes the normalized search
 * so each distinct query is cached independently. Until #1543 lands the
 * backend ignores the arg and the client-side filter in useTargetsPageFilters
 * covers alias matching.
 *
 * `refetch` replaces the old manual `load()` re-fetch — `TargetsPage` calls it
 * after "Add target" and passes it as `TargetDetailV2`'s `onMutated` so an
 * alias/display-alias edit refreshes the list's search/label data too.
 */
export function useTargets(search?: string): TargetsListState {
  const normalizedSearch = search?.trim() || null;
  const { data, isLoading, error, refetch } = useQuery({
    // Include normalizedSearch in the key so each query is cached independently;
    // keepPreviousData prevents the table from flashing a skeleton while a
    // new search key resolves — the previous page stays visible.
    queryKey: [...queryKeys.targets.list(), normalizedSearch],
    queryFn: async () => unwrap(await commands.targetList()),
    placeholderData: keepPreviousData,
  });
  return {
    data,
    // isLoading is true only on the very first load (no data yet); isFetching
    // would be true on every background refetch including search key changes,
    // which would flash a skeleton during type-ahead with keepPreviousData.
    loading: isLoading,
    error: error ?? undefined,
    refetch: () => void refetch(),
  };
}

/** Subscribe to a single target's gen-3 detail. */
export function useTargetDetail(targetId: string): QueryState<TargetDetailV3> {
  const { data, isFetching, error } = useQuery({
    queryKey: queryKeys.targets.detail(targetId),
    queryFn: () => getTargetDetail(targetId),
    enabled: !!targetId,
  });
  return { data, loading: isFetching, error: error ?? undefined };
}

/** Subscribe to a target's linked sessions (US2). */
export function useTargetSessions(
  targetId: string,
): QueryState<TargetSessionItem[]> {
  const { data, isFetching, error } = useQuery({
    queryKey: queryKeys.targets.sessions(targetId),
    queryFn: () => listTargetSessions(targetId),
    enabled: !!targetId,
  });
  return { data, loading: isFetching, error: error ?? undefined };
}

/** Subscribe to a target's linked projects (US3). */
export function useTargetProjects(
  targetId: string,
): QueryState<TargetProjectItem[]> {
  const { data, isFetching, error } = useQuery({
    queryKey: queryKeys.targets.projects(targetId),
    queryFn: () => listTargetProjects(targetId),
    enabled: !!targetId,
  });
  return { data, loading: isFetching, error: error ?? undefined };
}

/** Subscribe to a target's observing notes (US4). */
export function useTargetNotes(targetId: string): QueryState<string | null> {
  const { data, isFetching, error } = useQuery({
    queryKey: queryKeys.targets.notes(targetId),
    queryFn: () => getTargetNotes(targetId),
    enabled: !!targetId,
  });
  return { data, loading: isFetching, error: error ?? undefined };
}

/**
 * Sexagesimal RA/Dec formatting for one target (batched endpoint, N=1 here).
 * Keyed on the target id — coordinates are immutable for the life of a
 * catalog entry, so no re-derivation is needed across mounts. `enabled` lets
 * the caller gate this on its own detail query having resolved (mirrors the
 * pre-migration effect's `loadState.status === 'loaded'` guard).
 */
export function useTargetAstroFormat(
  targetId: string,
  raDeg: number | null,
  decDeg: number | null,
  enabled: boolean,
): QueryState<TargetAstroFormat | null> {
  const { data, isFetching, error } = useQuery({
    queryKey: queryKeys.targets.astroFormat(targetId),
    queryFn: async () => {
      const { formatted } = unwrap(
        await commands.targetAstroFormatBatch({
          targets: [{ id: targetId, raDeg, decDeg }],
        }),
      );
      return formatted[0] ?? null;
    },
    enabled: enabled && !!targetId,
  });
  return { data, loading: isFetching, error: error ?? undefined };
}

// ── Mutation hooks ────────────────────────────────────────────────────────────
//
// Every mutation below invalidates its target's `detail` key so `TargetDetailV2`
// refetches without a manual `load()` (spec `tiny/targets-tanstack-query-migration`).
// The LIST payload also carries alias/display-label search terms, but is
// refreshed via the `onMutated` callback prop (TargetsPage's `useTargets()`
// `refetch`) rather than a second `invalidateQueries({queryKey: ['targets']})`
// here — that broader key is a PREFIX of `detail(id)` (`['targets', id]`), so
// invalidating both in the same mutation double-invalidates (and, worse, a
// direct `setQueryData(detail(id), ...)` write below would get silently
// clobbered by the list invalidation's own background refetch of the SAME
// prefix-matched detail query).
//
// `exact: true` is required here too: `detail(id)` (`['targets', id]`) is
// ITSELF a prefix of `sessions(id)`/`projects(id)`/`notes(id)`/`astroFormat(id)`
// (all `['targets', id, ...]`) — an unqualified invalidate would fuzzy-match
// and refetch all four every alias/display-alias mutation, not just detail.

function invalidateTarget(
  queryClient: ReturnType<typeof useQueryClient>,
  targetId: string,
) {
  void queryClient.invalidateQueries({
    queryKey: queryKeys.targets.detail(targetId),
    exact: true,
  });
}

export function useAddTargetAlias() {
  const queryClient = useQueryClient();
  return useMutation<TargetAliasAddResult, Error, TargetAliasAddRequest>({
    mutationFn: async (req) => unwrap(await commands.targetAliasAdd(req)),
    onSuccess: (_data, variables) =>
      invalidateTarget(queryClient, variables.targetId),
  });
}

export function useRemoveTargetAlias() {
  const queryClient = useQueryClient();
  return useMutation<TargetAliasRemoveResult, Error, TargetAliasRemoveRequest>({
    mutationFn: async (req) => unwrap(await commands.targetAliasRemove(req)),
    onSuccess: (_data, variables) =>
      invalidateTarget(queryClient, variables.targetId),
  });
}

export function useSetTargetDisplayAlias() {
  const queryClient = useQueryClient();
  return useMutation<TargetDetailV3, Error, TargetDisplayAliasSetRequest>({
    mutationFn: async (req) =>
      unwrap(await commands.targetDisplayAliasSet(req)) as TargetDetailV3,
    onSuccess: (data, variables) => {
      queryClient.setQueryData(
        queryKeys.targets.detail(variables.targetId),
        data,
      );
    },
  });
}

export function useClearTargetDisplayAlias() {
  const queryClient = useQueryClient();
  return useMutation<TargetDetailV3, Error, TargetDisplayAliasClearRequest>({
    mutationFn: async (req) =>
      unwrap(await commands.targetDisplayAliasClear(req)) as TargetDetailV3,
    onSuccess: (data, variables) => {
      queryClient.setQueryData(
        queryKeys.targets.detail(variables.targetId),
        data,
      );
    },
  });
}

export function useUpdateTargetNotes() {
  const queryClient = useQueryClient();
  return useMutation<TargetNoteUpdateResult, Error, TargetNoteUpdateRequest>({
    mutationFn: async (req) => unwrap(await commands.targetNoteUpdate(req)),
    onSuccess: (data, variables) => {
      queryClient.setQueryData(
        queryKeys.targets.notes(variables.targetId),
        data.notes ?? null,
      );
    },
  });
}
