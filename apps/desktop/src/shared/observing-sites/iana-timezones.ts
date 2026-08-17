// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * iana-timezones.ts — offline IANA timezone id list for the site picker (spec 044 R7, T003).
 *
 * The observing-site editor (spec 044 US3) lets the user pick an IANA timezone
 * id (e.g. `Europe/Amsterdam`) for each site; the timezone drives local-time
 * rendering and DST for that site's ephemeris. The list MUST be available fully
 * offline (FR-027) — no network lookup.
 *
 * Source of truth is the platform's own zone database via
 * `Intl.supportedValuesOf('timeZone')`, which every modern WebView2 / WKWebView
 * exposes and which stays current with the OS tz database — no bundled asset to
 * go stale. A small curated fallback covers the (rare) engine that lacks
 * `supportedValuesOf`, so the picker is never empty. Pure data + a memoized
 * getter; no React, no astronomy import.
 */

/**
 * Curated fallback list of common IANA timezone ids, used only when the runtime
 * lacks `Intl.supportedValuesOf('timeZone')`. Deliberately compact — one
 * representative zone per major UTC offset the typical astrophotographer sits in
 * — so the picker is usable even on the fallback path.
 */
export const FALLBACK_IANA_TIMEZONES: readonly string[] = [
  'UTC',
  'Pacific/Honolulu',
  'America/Anchorage',
  'America/Los_Angeles',
  'America/Denver',
  'America/Chicago',
  'America/New_York',
  'America/Halifax',
  'America/Sao_Paulo',
  'Atlantic/Azores',
  'Europe/London',
  'Europe/Amsterdam',
  'Europe/Paris',
  'Europe/Berlin',
  'Europe/Madrid',
  'Europe/Athens',
  'Europe/Moscow',
  'Asia/Dubai',
  'Asia/Karachi',
  'Asia/Kolkata',
  'Asia/Bangkok',
  'Asia/Shanghai',
  'Asia/Tokyo',
  'Australia/Perth',
  'Australia/Adelaide',
  'Australia/Sydney',
  'Pacific/Auckland',
];

let cached: readonly string[] | null = null;

/**
 * The full sorted list of IANA timezone ids available offline.
 *
 * Prefers the platform zone database (`Intl.supportedValuesOf('timeZone')`);
 * falls back to {@link FALLBACK_IANA_TIMEZONES} when unavailable. Memoized —
 * the list does not change within a session.
 */
export function ianaTimezones(): readonly string[] {
  if (cached !== null) return cached;
  const supported = (
    Intl as unknown as { supportedValuesOf?: (key: string) => string[] }
  ).supportedValuesOf;
  if (typeof supported === 'function') {
    try {
      const zones = supported('timeZone');
      if (Array.isArray(zones) && zones.length > 0) {
        cached = [...zones].sort((a, b) => a.localeCompare(b));
        return cached;
      }
    } catch {
      // fall through to the curated list
    }
  }
  cached = [...FALLBACK_IANA_TIMEZONES];
  return cached;
}

/** Whether a string is a selectable IANA timezone id in the offline list. */
export function isKnownTimezone(id: string): boolean {
  return ianaTimezones().includes(id);
}

/**
 * The machine's own IANA timezone id (best-effort), used to preselect a sensible
 * default when adding a site. Falls back to `'UTC'`.
 */
export function localTimezone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC';
  } catch {
    return 'UTC';
  }
}

/** Test-only: clear the memoized list. */
export function __resetTimezoneCacheForTest(): void {
  cached = null;
}
