// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Pure display-formatting helpers for `TargetDetail` (refactor sweep
 * #982): no React, no state — split out so the format rules can be reasoned
 * about (and, if needed, unit-tested) independently of the component.
 */

import type { SeparationFigure } from './planner-derive';
import { m } from '@/lib/i18n';

/**
 * #758/FR-020: format one of the three real target↔Moon separation figures —
 * a whole-degree value, or the explicit "Moon not up" state (never a
 * fabricated number when the Moon is below the horizon at that reference).
 */
export function formatSeparationFigure(figure: SeparationFigure): string {
  return figure === 'moon-not-up'
    ? m.targets_moon_not_up()
    : `${Math.round(figure)}°`;
}

/** Map an AliasKind string to a human label for the badge. */
export function kindLabel(kind: string): string {
  switch (kind) {
    case 'designation':
      return m.targets_alias_kind_designation();
    case 'common_name':
      return m.targets_alias_kind_name();
    case 'user':
      return m.targets_alias_kind_user();
    default:
      return kind;
  }
}
