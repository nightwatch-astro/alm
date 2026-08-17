// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * site-store.ts — settings-backed observing-site store (spec 044 Track B, US1/US3).
 *
 * The observing sites, the default/active pointers, and the global usable-altitude
 * threshold live in the spec-018 settings store under the `'observing'` scope
 * (keys `observingSites`, `observingDefaultSiteId`, `observingActiveSiteId`,
 * `usableAltitudeDeg`). This module is the single frontend access point: a live
 * cache hydrated from the backend, React hooks for components, and non-hook
 * getters for comparators / tests. It mirrors the shape of spec 047's
 * `guidance-settings.ts` so the two planner settings stores stay consistent.
 *
 * When the backend is unavailable or nothing is persisted, callers fall back to
 * the no-site state (empty sites, `null` active) and the default 30° threshold.
 *
 * NOTE (spec 047 coordination): 047 ships `astro/site-gate.ts` as a documented
 * false-stub with a single flip point. When 044 + 047 merge, flip
 * `readSiteExists()` there to `activeSite() !== null` (one line) — this store is
 * the real source it should read.
 */

import { useSyncExternalStore } from 'react';
import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import { type ObserverSite, coerceSites } from './observer-site';

/** Settings scope + keys for the observing-site model. */
export const OBSERVING_SCOPE = 'observing';
export const SITES_KEY = 'observingSites';
export const DEFAULT_SITE_ID_KEY = 'observingDefaultSiteId';
export const ACTIVE_SITE_ID_KEY = 'observingActiveSiteId';
export const USABLE_ALTITUDE_KEY = 'usableAltitudeDeg';

/** Default usable-altitude threshold (degrees) — mirrors the Rust default. */
export const DEFAULT_USABLE_ALTITUDE_DEG = 30;
/** Usable-altitude valid range (degrees). */
export const USABLE_ALTITUDE_MIN = 0;
export const USABLE_ALTITUDE_MAX = 90;

/** The resolved observing-site state used across the planner. */
export interface ObservingState {
  sites: ObserverSite[];
  defaultSiteId: string | null;
  activeSiteId: string | null;
  usableAltitudeDeg: number;
}

const EMPTY_STATE: ObservingState = {
  sites: [],
  defaultSiteId: null,
  activeSiteId: null,
  usableAltitudeDeg: DEFAULT_USABLE_ALTITUDE_DEG,
};

function clampThreshold(n: unknown): number {
  const v = Number(n);
  if (!Number.isFinite(v)) return DEFAULT_USABLE_ALTITUDE_DEG;
  return Math.max(USABLE_ALTITUDE_MIN, Math.min(USABLE_ALTITUDE_MAX, v));
}

function coerceIdRef(value: unknown, sites: ObserverSite[]): string | null {
  if (typeof value !== 'string') return null;
  return sites.some((s) => s.id === value) ? value : null;
}

/** Coerce a raw `observing`-scope values bag into a clean {@link ObservingState}. */
export function coerceObservingState(
  values: Record<string, unknown>,
): ObservingState {
  const sites = coerceSites(values[SITES_KEY]);
  return {
    sites,
    defaultSiteId: coerceIdRef(values[DEFAULT_SITE_ID_KEY], sites),
    activeSiteId: coerceIdRef(values[ACTIVE_SITE_ID_KEY], sites),
    usableAltitudeDeg: clampThreshold(values[USABLE_ALTITUDE_KEY]),
  };
}

/** Resolve the active site object from a state (active pointer, else null). */
export function resolveActiveSite(state: ObservingState): ObserverSite | null {
  if (state.activeSiteId === null) return null;
  return state.sites.find((s) => s.id === state.activeSiteId) ?? null;
}

// ── Live cache + subscribers ─────────────────────────────────────────────────

let current: ObservingState = EMPTY_STATE;
const listeners = new Set<() => void>();

// Mirrors the `writeGen` guard in ../guidance-settings.ts (#836). `Shell.tsx`
// kicks off `loadObservingState()` at boot; on a slow backend that read can
// still be in flight once the user reaches Targets and saves a site, and a read
// that started before the write is stale by the time it resolves. A load only
// applies if no save has committed since the load started.
let writeGen = 0;

function emit(): void {
  for (const fn of listeners) fn();
}

function subscribe(fn: () => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

function snapshot(): ObservingState {
  return current;
}

/**
 * Load the observing state from the backend into the live cache. Safe to call
 * on planner mount; falls back to the no-site state when the backend is
 * unavailable.
 */
export async function loadObservingState(): Promise<ObservingState> {
  const genAtStart = writeGen;
  try {
    const data = unwrap(await commands.settingsGet(OBSERVING_SCOPE));
    if (writeGen === genAtStart) {
      current = coerceObservingState(data.values as Record<string, unknown>);
    }
  } catch {
    if (writeGen === genAtStart) {
      current = EMPTY_STATE;
    }
  }
  emit();
  return current;
}

/**
 * Persist the full site collection + default/active pointers, updating the live
 * cache so the planner recomputes immediately.
 */
export async function saveSites(
  sites: ObserverSite[],
  defaultSiteId: string | null,
  activeSiteId: string | null,
): Promise<void> {
  const next: ObservingState = {
    sites,
    defaultSiteId:
      defaultSiteId !== null && sites.some((s) => s.id === defaultSiteId)
        ? defaultSiteId
        : null,
    activeSiteId:
      activeSiteId !== null && sites.some((s) => s.id === activeSiteId)
        ? activeSiteId
        : null,
    usableAltitudeDeg: current.usableAltitudeDeg,
  };
  writeGen += 1;
  unwrap(
    await commands.settingsUpdate(OBSERVING_SCOPE, {
      [SITES_KEY]: next.sites,
      [DEFAULT_SITE_ID_KEY]: next.defaultSiteId,
      [ACTIVE_SITE_ID_KEY]: next.activeSiteId,
    }),
  );
  current = next;
  emit();
}

/** Persist just the active-site pointer (US3 active switch). */
export async function saveActiveSiteId(
  activeSiteId: string | null,
): Promise<void> {
  await saveSites(current.sites, current.defaultSiteId, activeSiteId);
}

/**
 * Persist a new usable-altitude threshold and update the live cache
 * (SC-003/SC-006). Updates the cache + notifies subscribers OPTIMISTICALLY,
 * synchronously before the backend round-trip settles, so slider drags and
 * Settings-pane edits reflect instantly (SC-003) rather than waiting on IPC
 * latency; the backend write still happens (durability, SC-006), it just
 * isn't gating the UI update.
 *
 * On backend failure the snapshot is restored so the UI and DB stay
 * consistent; a stale rollback is suppressed if a newer write has since
 * committed (writeGen guard).
 */
export async function saveUsableAltitude(degrees: number): Promise<void> {
  const clamped = clampThreshold(degrees);
  const prev = current;
  writeGen += 1;
  const gen = writeGen;
  current = { ...current, usableAltitudeDeg: clamped };
  emit();
  try {
    unwrap(
      await commands.settingsUpdate(OBSERVING_SCOPE, {
        [USABLE_ALTITUDE_KEY]: clamped,
      }),
    );
  } catch (err) {
    if (writeGen === gen) {
      current = prev;
      emit();
    }
    throw err;
  }
}

/** Non-hook read of the current cached state (comparators, tests). */
export function getObservingState(): ObservingState {
  return current;
}

/** Non-hook read of the active site (comparators, tests). */
export function activeSite(): ObserverSite | null {
  return resolveActiveSite(current);
}

/** Non-hook read of the usable-altitude threshold. */
export function getUsableAltitude(): number {
  return current.usableAltitudeDeg;
}

/** Test-only: set the cache directly. */
export function __setObservingStateForTest(
  state: Partial<ObservingState>,
): void {
  current = { ...EMPTY_STATE, ...state };
  emit();
}

/** React hook: the live observing state. */
export function useObservingState(): ObservingState {
  return useSyncExternalStore(subscribe, snapshot, snapshot);
}

/** React hook: the active observing site (or null in the no-site state). */
export function useActiveSite(): ObserverSite | null {
  return resolveActiveSite(useObservingState());
}

/** React hook: the usable-altitude threshold. */
export function useUsableAltitude(): number {
  return useObservingState().usableAltitudeDeg;
}

export type { ObserverSite };
