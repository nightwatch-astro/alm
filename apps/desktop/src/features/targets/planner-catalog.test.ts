// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * planner-catalog.test.ts — RESTRICT-catalog filter for the Target Planner
 * (task #40, spec 043 §4).
 *
 * The Planner must surface only the allowed catalogs (Messier/NGC/IC/Sh2/LBN/
 * LDN/Caldwell/Barnard) and drop the ~13k SIMBAD double stars the raw list
 * endpoint returns.
 */

import { describe, it, expect } from 'vitest';
import type { TargetListItem } from '@/bindings/index';
import {
  filterPlannerCatalog,
  isPlannerCatalogTarget,
} from './planner-catalog';

function item(
  primaryDesignation: string,
  objectType = 'other',
): TargetListItem {
  return {
    id: primaryDesignation,
    effectiveLabel: primaryDesignation,
    primaryDesignation,
    objectType,
    raDeg: 0,
    decDeg: 0,
    sessionCount: 0,
  };
}

describe('planner-catalog', () => {
  it('accepts allowed catalog designations', () => {
    const allowed = [
      'M 31',
      'NGC 7000',
      'IC 1396',
      'Sh2-155',
      'LBN 552',
      'LDN 1235',
      'C 14',
      'B 33',
    ];
    for (const d of allowed) {
      expect(isPlannerCatalogTarget(item(d))).toBe(true);
    }
  });

  it('accepts designations with no space before the number', () => {
    expect(isPlannerCatalogTarget(item('M31'))).toBe(true);
    expect(isPlannerCatalogTarget(item('NGC7000'))).toBe(true);
  });

  it('rejects designations outside the allowed catalogs', () => {
    const rejected = [
      'HD 209458', // double-star / Henry Draper — the 13k dump we must drop
      'TYC 1234-5-1',
      'Gaia DR3 12345',
      'WDS J00057+4549', // Washington Double Star
      'ICRS', // must not match the IC prefix (no digit follows)
      'Cygnus', // must not match the C prefix (no digit follows)
      '2MASS J0000', // leading digit, no catalog prefix
    ];
    for (const d of rejected) {
      expect(isPlannerCatalogTarget(item(d))).toBe(false);
    }
  });

  it('filters a mixed list down to only catalog targets', () => {
    const list = [
      item('M 42', 'emission_nebula'),
      item('HD 1', 'double_star'),
      item('NGC 224', 'galaxy'),
      item('WDS J12345', 'double_star'),
      item('B 142', 'dark_nebula'),
    ];
    const result = filterPlannerCatalog(list);
    expect(result.map((t) => t.primaryDesignation)).toEqual([
      'M 42',
      'NGC 224',
      'B 142',
    ]);
  });

  it('matching is case-insensitive', () => {
    expect(isPlannerCatalogTarget(item('ngc 891'))).toBe(true);
    expect(isPlannerCatalogTarget(item('m 13'))).toBe(true);
  });
});
