// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * moon-avoidance.ts — re-export shim.
 *
 * The Moon-avoidance domain model moved to `@/shared/planner/moon-avoidance`.
 * It is pure domain logic with no imports of its own, and it is consumed from
 * both sides of the settings/targets boundary: the targets feature reads it for
 * the Planner table, and `shared/planner/guidance-settings.ts` needs its band
 * types and defaults. Leaving it under `features/targets` forced
 * `shared → features`, the very edge the settings/targets decoupling exists to
 * remove.
 *
 * This shim keeps the `features/targets/astro/...` path working for the call
 * sites inside that feature. New code should import from
 * `@/shared/planner/moon-avoidance` directly.
 */

export {
  BANDS,
  BROADBAND_BANDS,
  NARROWBAND_BANDS,
  DEFAULT_MOON_AVOIDANCE,
  minSeparationDeg,
  bandViability,
  deriveRecommendation,
  bandTier,
} from '@/shared/planner/moon-avoidance';
export type {
  Band,
  BandParams,
  MoonAvoidanceParams,
  Recommendation,
} from '@/shared/planner/moon-avoidance';
