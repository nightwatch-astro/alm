// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { useSyncExternalStore, useCallback } from 'react';
import { z } from 'zod';
import { readLocalStorage, writeLocalStorage } from '@/lib/local-storage';
import type { AppPreferences } from '@/bindings/types';

const STORAGE_KEY = 'alm-preferences';

/**
 * Validation schema for the persisted `AppPreferences` bag, mirroring the
 * generated type in `@/bindings/types`.
 *
 * Every field is `.optional().catch(undefined)`: a bag written by an older
 * build legitimately lacks keys added since, and a field whose type has changed
 * must be dropped on its own rather than discarding every valid sibling. The
 * reader spreads the surviving subset over `defaults`, the same merge the
 * previous unvalidated `JSON.parse` performed — the difference being that a
 * wrong-typed field now yields its default instead of reaching the UI.
 */
const DensitySchema = z.enum(['compact', 'comfortable', 'spacious']);
const ViewModeSchema = z.enum(['center', 'pipeline', 'combined']);
const DetailDockPrefSchema = z.object({
  placement: z.enum(['side', 'bottom']).nullable(),
  width: z.number().nullable(),
});

const optional = <T extends z.ZodType>(schema: T) =>
  schema.optional().catch(undefined);

const AppPreferencesSchema = z.object({
  sidebarCollapsed: optional(z.boolean()),
  density: optional(DensitySchema),
  projectViewModes: optional(z.record(z.string(), ViewModeSchema)),
  defaultProjectView: optional(ViewModeSchema),
  sessionsGroupBy: optional(
    z.enum(['none', 'target', 'month', 'filter', 'train']),
  ),
  sessionsView: optional(z.enum(['list', 'calendar'])),
  setupCompleted: optional(z.boolean()),
  detailDock: optional(z.record(z.string(), DetailDockPrefSchema)),
});

type Listener = () => void;

const listeners = new Set<Listener>();
let cachedPreferences: AppPreferences | undefined;

const defaults: AppPreferences = {
  sidebarCollapsed: false,
  density: 'comfortable',
  projectViewModes: {},
  defaultProjectView: 'combined',
  sessionsGroupBy: 'none',
  sessionsView: 'list',
  setupCompleted: false,
  detailDock: {},
};

function notify(): void {
  for (const listener of listeners) {
    listener();
  }
}

/**
 * Reads preferences from localStorage, merging with defaults.
 */
export function getPreferences(): AppPreferences {
  if (cachedPreferences !== undefined) {
    return cachedPreferences;
  }
  const stored = readLocalStorage(STORAGE_KEY, AppPreferencesSchema, {});
  // A dropped field is present with value `undefined`, which would override its
  // default in the spread below.
  const present = Object.fromEntries(
    Object.entries(stored).filter(([, v]) => v !== undefined),
  );
  const result: AppPreferences = { ...defaults, ...present };
  cachedPreferences = result;
  return result;
}

/**
 * Persists updated preferences to localStorage and notifies subscribers.
 */
function persistPreferences(prefs: AppPreferences): void {
  cachedPreferences = prefs;
  // Storage full or unavailable is non-fatal; state is still in memory.
  writeLocalStorage(STORAGE_KEY, prefs);
  notify();
}

/**
 * Sets a single preference key and persists.
 */
export function setPreference<K extends keyof AppPreferences>(
  key: K,
  value: AppPreferences[K],
): void {
  const current = getPreferences();
  persistPreferences({ ...current, [key]: value });
}

/**
 * Resets all preferences to defaults.
 */
export function resetPreferences(): void {
  cachedPreferences = undefined;
  try {
    localStorage.removeItem(STORAGE_KEY);
  } catch {
    // Intentional ignore: localStorage may be unavailable (private mode / quota);
    // the in-memory cache was already cleared above, so this is best-effort.
  }
  notify();
}

/**
 * Subscribes to preference changes outside React (components use the hooks
 * below). Lets the appearance runtime (data/theme.ts) re-apply density when
 * ANY caller writes it — Settings, the Setup wizard's usePreference — so the
 * app-wide token rescale never depends on a per-call-site applyDensity (#587).
 */
export function subscribePreferences(listener: Listener): () => void {
  return subscribe(listener);
}

// --- Hooks ---

function subscribe(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function getSnapshot(): AppPreferences {
  return getPreferences();
}

/**
 * Hook: subscribes to all preferences. Re-renders on any preference change.
 */
export function usePreferences(): AppPreferences {
  return useSyncExternalStore(subscribe, getSnapshot);
}

/**
 * Hook: subscribes to a single preference key. Returns [value, setter] tuple.
 */
export function usePreference<K extends keyof AppPreferences>(
  key: K,
): [AppPreferences[K], (value: AppPreferences[K]) => void] {
  const prefs = useSyncExternalStore(subscribe, getSnapshot);
  const setter = useCallback(
    (value: AppPreferences[K]) => {
      setPreference(key, value);
    },
    [key],
  );
  return [prefs[key], setter];
}
