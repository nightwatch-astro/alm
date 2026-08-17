// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * planner-sensor.ts — derive the planner's camera SensorConfig from the
 * equipment list (spec 044 iteration 2026-07-15, FR-035/FR-036/FR-038, T046).
 *
 * The planner consumes the equipment Camera sensor-type ephemerally (no new
 * parameter store, data-model.md §4). Selection rule when several cameras
 * exist: the OSC single-pass model applies only when the configured cameras
 * are UNAMBIGUOUSLY OSC — every camera with a known sensor type is 'osc'.
 * Any mono camera, or no known sensor type at all, keeps today's per-filter
 * model (FR-038: unknown behaves as mono; the change never regresses mono
 * users). Multiple OSC cameras use the first one's passband (list is
 * name-ordered and the passband differs only for narrowband filters).
 */

import { useEffect, useMemo, useState } from 'react';
import { equipmentCamerasList } from '@/features/settings/settingsIpc';
import type { Camera } from '@/features/settings/settingsIpc';
import { BANDS, type Band } from '@/shared/planner/moon-avoidance';
import type { SensorConfig } from './planner-derive';

function isBand(value: string): value is Band {
  return (BANDS as readonly string[]).includes(value);
}

/** Pure derivation — exported for unit tests. */
export function derivePlannerSensorConfig(
  cameras: Camera[],
): SensorConfig | null {
  const known = cameras.filter((c) => c.sensorType !== null);
  if (known.length === 0) return null;
  if (known.some((c) => c.sensorType === 'mono')) return null;
  const osc = known.find((c) => c.sensorType === 'osc');
  if (!osc) return null;
  const bands = (osc.passband ?? []).filter(isBand);
  return {
    sensorType: 'osc',
    passband: bands.length > 0 ? bands : 'rgb',
  };
}

/**
 * The active planner sensor configuration, loaded once per mount. `null`
 * (mono/unknown/ambiguous/load-error) keeps the pre-iteration per-filter
 * model byte-identical (FR-038/SC-017).
 */
export function usePlannerSensorConfig(): SensorConfig | null {
  const [cameras, setCameras] = useState<Camera[] | null>(null);
  useEffect(() => {
    let cancelled = false;
    equipmentCamerasList()
      .then((list) => {
        if (!cancelled) setCameras(list);
      })
      .catch(() => {
        // Equipment unavailable → unknown → mono behavior (FR-038).
        if (!cancelled) setCameras([]);
      });
    return () => {
      cancelled = true;
    };
  }, []);
  return useMemo(
    () => (cameras ? derivePlannerSensorConfig(cameras) : null),
    [cameras],
  );
}
