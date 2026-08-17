// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Hierarchical, tuple-based query key factory for TanStack Query.
 *
 * Each key is a const tuple to enable fine-grained or prefix-based invalidation.
 * Import `queryKeys` and call the relevant factory in `useQuery`/`useMutation`.
 */

export const queryKeys = {
  projects: {
    all: () => ['projects'] as const,
    detail: (id: string) => ['projects', id] as const,
    artifacts: (id: string) => ['projects', id, 'artifacts'] as const,
    history: (id: string) => ['projects', id, 'history'] as const,
  },
  inventory: {
    // No filters → prefix-only key, so `invalidateQueries({ queryKey:
    // inventory.all() })` fuzzy-matches every inventory query regardless of
    // its filters (TanStack Query's partial-match compares own array indices,
    // so a trailing `undefined` element would NOT match a filtered entry).
    all: (filters?: object) =>
      filters ? (['inventory', filters] as const) : (['inventory'] as const),
    rootConfig: (rootId: string) =>
      ['inventory', 'rootConfig', rootId] as const,
  },
  sessions: {
    all: () => ['sessions'] as const,
    calendar: (start: string, end: string) =>
      ['sessions', 'calendar', start, end] as const,
    // spec 062
    panelGroup: (id: string) => ['sessions', 'panelGroup', id] as const,
    panelGroupList: (filters?: object) =>
      filters
        ? (['sessions', 'panelGroups', filters] as const)
        : (['sessions', 'panelGroups'] as const),
    mosaic: (id: string) => ['sessions', 'mosaic', id] as const,
    proposals: (filters?: object) =>
      filters
        ? (['sessions', 'proposals', filters] as const)
        : (['sessions', 'proposals'] as const),
    proposal: (id: string) => ['sessions', 'proposal', id] as const,
    matchingSettings: (revision?: number) =>
      revision != null
        ? (['sessions', 'matchingSettings', revision] as const)
        : (['sessions', 'matchingSettings'] as const),
  },
  inbox: {
    list: (rootId: string) => ['inbox', rootId] as const,
    metadata: (itemId: string) => ['inbox', 'metadata', itemId] as const,
    coneSearch: (framesetId: string) =>
      ['inbox', 'coneSearch', framesetId] as const,
    attributionSuggest: (itemId: string) =>
      ['inbox', 'attributionSuggest', itemId] as const,
  },
  calibration: {
    masters: () => ['calibration', 'masters'] as const,
    master: (id: string) => ['calibration', 'masters', id] as const,
    matches: (sid: string) => ['calibration', 'matches', sid] as const,
    settings: () => ['calibration', 'settings'] as const,
    // spec-062 US4 — candidate / handoff / equipment keys (ic9h.20 will
    // replace these seam calls with generated bindings; keep key shapes stable)
    candidates: (requirementId: string) =>
      ['calibration', 'candidates', requirementId] as const,
    handoff: (handoffId: string) =>
      ['calibration', 'handoff', handoffId] as const,
    handoffOperation: (operationId: string) =>
      ['calibration', 'handoff', 'operation', operationId] as const,
    equipmentResolution: (sessionId: string) =>
      ['calibration', 'equipment', 'resolution', sessionId] as const,
  },
  targets: {
    list: () => ['targets'] as const,
    detail: (id: string) => ['targets', id] as const,
    sessions: (id: string) => ['targets', id, 'sessions'] as const,
    projects: (id: string) => ['targets', id, 'projects'] as const,
    notes: (id: string) => ['targets', id, 'notes'] as const,
    astroFormat: (id: string) => ['targets', id, 'astroFormat'] as const,
    favourites: () => ['targets', 'favourites'] as const,
  },
  setup: {
    sources: () => ['setup', 'sources'] as const,
  },
  status: {
    summary: () => ['status'] as const,
  },
  archive: {
    list: () => ['archive'] as const,
    audit: (entityId: string) => ['archive', 'audit', entityId] as const,
  },
  plans: {
    detail: (id: string) => ['plans', 'detail', id] as const,
    freeSpaceEstimate: (id: string) =>
      ['plans', 'freeSpaceEstimate', id] as const,
  },
  roots: {
    all: () => ['roots'] as const,
  },
} as const;
