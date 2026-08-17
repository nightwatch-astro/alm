// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * FilterBadges — parameterised per-band Moon-avoidance viability pills for the
 * Planner table and detail pane (spec 047, plan D4/T016, FR-009a).
 *
 * ONE shared component (no per-feature clones): renders all seven fixed bands
 * (L, R, G, B, Ha, SII, OIII) as compact pills, each showing whether that band
 * is viable tonight given the real Moon-avoidance Lorentzian rule
 * (`astro/moon-avoidance.ts`), plus the derived summary recommendation label.
 * `viability === null` (unknown coordinates / no observing night) renders a
 * single explicit "unknown" state — never a fabricated recommendation.
 */

import {
  BANDS,
  bandTier,
  type Band,
  type Recommendation,
} from '@/shared/planner/moon-avoidance';
import { m } from '@/lib/i18n';
import * as tt from './targets-table.css';

/** i18n label for each derived recommendation category (render-time thunks). */
const RECOMMENDATION_LABEL: Record<Recommendation, () => string> = {
  'broadband-ok': () => m.targets_filters_broadband_nb(),
  'narrowband-only': () => m.targets_filters_narrowband_only(),
  'avoid-tonight': () => m.targets_filters_avoid_tonight(),
  unknown: () => m.common_unknown(),
};

/** Human label for a recommendation category. */
export function recommendationLabel(recommendation: Recommendation): string {
  return RECOMMENDATION_LABEL[recommendation]();
}

interface Props {
  /** Per-band viability tonight, or `null` when coordinates/night are unknown. */
  viability: Record<Band, boolean> | null;
  /** Derived summary recommendation ('unknown' whenever `viability` is null). */
  recommendation: Recommendation;
}

/**
 * Render the seven-band viability pill strip + the derived recommendation
 * label. Unknown state renders a single muted pill instead of fabricating
 * per-band viability.
 */
export function FilterBadges({ viability, recommendation }: Props) {
  const label = recommendationLabel(recommendation);

  if (viability === null) {
    return (
      <span className={tt.filterBadges} title={label}>
        <span className={`${tt.filterBadge} ${tt.filterBadgeUnknown}`}>
          {label}
        </span>
      </span>
    );
  }

  return (
    <span className={tt.filterBadges} title={label}>
      {BANDS.map((band) => {
        const viable = viability[band];
        return (
          <span
            key={band}
            className={
              `${tt.filterBadge} ${tt.filterBadgeTier[bandTier(band)]} ` +
              (viable ? tt.filterBadgeViable : tt.filterBadgeNotViable)
            }
            aria-label={
              viable
                ? m.targets_filter_badge_viable_aria({ band })
                : m.targets_filter_badge_not_viable_aria({ band })
            }
          >
            {band}
          </span>
        );
      })}
    </span>
  );
}
