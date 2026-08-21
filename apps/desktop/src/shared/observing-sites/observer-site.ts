// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * observer-site.ts — the ObserverSite value type + coercion (spec 044 Track B).
 *
 * `ObserverSite` mirrors the `observingSites` settings key (a static
 * `SettingsState` field, not a specta-exported binding — the settings transport
 * is `settings.get/update` scope/values, so this type is defined here rather
 * than imported from `@/bindings`). It is the single frontend definition of a
 * named observing location the planner computes observability against
 * (data-model.md §1). Pure data + coercion; no React, no astronomy import.
 */

/**
 * Inclusive bounds for the range-checked site fields, mirroring the backend
 * `observingSites` validation in `crates/app/settings/src/descriptors.rs`
 * (`check_observer_sites`). These must stay equal to the Rust values: a value the
 * UI accepts but the backend rejects fails the save, and a stored value the
 * backend rejects makes read-time repair discard the whole site list.
 */
export const SITE_LAT_DEG_MIN = -90;
export const SITE_LAT_DEG_MAX = 90;
export const SITE_LON_DEG_MIN = -180;
export const SITE_LON_DEG_MAX = 180;
export const SITE_MIN_HORIZON_ALT_DEG_MIN = 0;
export const SITE_MIN_HORIZON_ALT_DEG_MAX = 90;

/** Per-site night definition. `astronomical` = Sun −18°, `nautical` = Sun −12°. */
export type Twilight = 'astronomical' | 'nautical';

/** A named observing location (mirrors the `observingSites` settings entries). */
export interface ObserverSite {
  /** Stable, immutable identity; referenced by default/active pointers. */
  id: string;
  /** User label (non-empty). */
  name: string;
  /** Latitude in decimal degrees, [−90, 90]. */
  latitudeDeg: number;
  /** Longitude in decimal degrees, [−180, 180]; east-positive. */
  longitudeDeg: number;
  /** Elevation in metres; optional. */
  elevationM: number | null;
  /** IANA timezone id (e.g. `Europe/Amsterdam`). */
  timezone: string;
  /** Per-site night definition. */
  twilight: Twilight;
  /** Local-obstruction floor in degrees, [0, 90]; refraction still applied. */
  minHorizonAltDeg: number;
}

function clampNum(
  n: unknown,
  lo: number,
  hi: number,
  fallback: number,
): number {
  const v = Number(n);
  if (!Number.isFinite(v)) return fallback;
  return Math.max(lo, Math.min(hi, v));
}

/**
 * Coerce an unknown persisted entry into a clean {@link ObserverSite}, or `null`
 * when it lacks the required identity/name (unusable). Out-of-range coordinates
 * are clamped; unknown twilight falls back to `astronomical`.
 */
export function coerceSite(value: unknown): ObserverSite | null {
  if (!value || typeof value !== 'object') return null;
  const src = value as Record<string, unknown>;
  const id = typeof src['id'] === 'string' ? (src['id'] as string) : '';
  const name = typeof src['name'] === 'string' ? (src['name'] as string) : '';
  if (id === '' || name === '') return null;
  const elevationRaw = src['elevationM'];
  const elevationM =
    elevationRaw === null ||
    elevationRaw === undefined ||
    !Number.isFinite(Number(elevationRaw))
      ? null
      : Number(elevationRaw);
  const twilight: Twilight =
    src['twilight'] === 'nautical' ? 'nautical' : 'astronomical';
  return {
    id,
    name,
    latitudeDeg: clampNum(
      src['latitudeDeg'],
      SITE_LAT_DEG_MIN,
      SITE_LAT_DEG_MAX,
      0,
    ),
    longitudeDeg: clampNum(
      src['longitudeDeg'],
      SITE_LON_DEG_MIN,
      SITE_LON_DEG_MAX,
      0,
    ),
    elevationM,
    timezone:
      typeof src['timezone'] === 'string' ? (src['timezone'] as string) : 'UTC',
    twilight,
    minHorizonAltDeg: clampNum(
      src['minHorizonAltDeg'],
      SITE_MIN_HORIZON_ALT_DEG_MIN,
      SITE_MIN_HORIZON_ALT_DEG_MAX,
      0,
    ),
  };
}

/** Coerce an unknown persisted array into a clean list of usable sites. */
export function coerceSites(value: unknown): ObserverSite[] {
  if (!Array.isArray(value)) return [];
  const out: ObserverSite[] = [];
  for (const entry of value) {
    const site = coerceSite(entry);
    if (site !== null) out.push(site);
  }
  return out;
}
