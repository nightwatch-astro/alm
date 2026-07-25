// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * altitude-settings.ts — user-configurable usable-altitude threshold (spec 044).
 *
 * The "usable altitude" is the minimum elevation above the horizon (in degrees)
 * that the user considers acceptable for imaging. It gates the
 * `hoursAboveUsable`/`totalImagingMinutes` and `visibleTonight` columns in the
 * Planner table and the guide line in AltitudeSparkline.
 *
 * T012b (spec 044 Track B): the threshold is now persisted through the
 * settings-backed `observing-sites/site-store.ts` (`usableAltitudeDeg` key,
 * `observing` scope — T004–T008), NOT localStorage. This module is kept as a
 * thin, name-stable adapter over `site-store.ts` so existing call sites
 * (`PlannerSettings.tsx`, `TargetsPage.tsx`) do not need to change; it exists
 * to preserve the settings-durability requirement (FR-004/SC-006 — the
 * threshold now survives relaunch and is not device-local-only).
 */

import {
  useUsableAltitude,
  getUsableAltitude,
  saveUsableAltitude,
} from '@/shared/observing-sites/site-store';
import { USABLE_ALT_DEG } from '@/features/targets/planner-altitude';

/** Minimum allowed threshold value (degrees). */
export const ALTITUDE_THRESHOLD_MIN = 0;

/** Maximum allowed threshold value (degrees). */
export const ALTITUDE_THRESHOLD_MAX = 90;

/**
 * Persist a new threshold through the settings store. Fire-and-forget: the
 * live cache in `site-store.ts` updates optimistically and notifies
 * subscribers immediately (SC-003 instant-derivation), independent of the
 * backend round-trip completing.
 */
export function setAltitudeThreshold(degrees: number): void {
  // Best-effort persist; the live cache already reflects the change optimistically
  // (SC-003). Swallow the backend-write rejection here so a failed or unavailable
  // IPC round-trip never escapes as an unhandled promise rejection (matches the
  // fire-and-forget settings-update convention in CalibrationMatching).
  saveUsableAltitude(degrees).catch(() => {
    // Intentionally ignored — optimistic UI already updated.
  });
}

/**
 * React hook: subscribe to the user-configured usable-altitude threshold.
 * Updates automatically whenever `setAltitudeThreshold` is called anywhere in
 * the app (settings-backed live cache, not per-tab localStorage).
 */
export function useAltitudeThreshold(): number {
  return useUsableAltitude();
}

/** Non-hook read for use outside React (e.g., sort comparators, tests). */
export function getAltitudeThreshold(): number {
  return getUsableAltitude();
}

export { USABLE_ALT_DEG };
