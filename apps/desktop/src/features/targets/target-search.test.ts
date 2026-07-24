// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * target-search.test.ts — designation/label search normalization (#103b, spec-043).
 *
 * Validates that normalizeDesig collapses whitespace and folds case, and that
 * matchesSearch matches "M31", "M 31", and "Andromeda" to the same target via
 * designation and effectiveLabel.  Alias-based search moved to backend
 * (GF-11 / DS-16) — `target.list(search)` filters aliases server-side; the
 * client-side `matchesSearch` no longer consults an aliases array.
 */

import { describe, it, expect } from 'vitest';
import type { TargetListItem } from '@/bindings/index';
import { normalizeDesig, matchesSearch } from './TargetsPage';

function item(
  primaryDesignation: string,
  effectiveLabel?: string,
  objectType = 'other',
): TargetListItem {
  return {
    id: primaryDesignation,
    effectiveLabel: effectiveLabel ?? primaryDesignation,
    primaryDesignation,
    objectType,
    raDeg: 0,
    decDeg: 0,
    sessionCount: 0,
  };
}

describe('normalizeDesig', () => {
  it('lowercases the string', () => {
    expect(normalizeDesig('M 31')).toBe('m31');
    expect(normalizeDesig('NGC 7000')).toBe('ngc7000');
  });

  it('collapses internal whitespace', () => {
    expect(normalizeDesig('M 31')).toBe(normalizeDesig('M31'));
    expect(normalizeDesig('NGC  7000')).toBe(normalizeDesig('NGC7000'));
  });

  it('handles leading/trailing whitespace', () => {
    expect(normalizeDesig('  M 31  ')).toBe('m31');
  });

  it('preserves non-whitespace characters', () => {
    expect(normalizeDesig('Sh2-155')).toBe('sh2-155');
  });
});

describe('matchesSearch — designation + label (#103b)', () => {
  // effectiveLabel is the bare designation; aliases carry the proper names.
  const m31 = item('M 31', 'Andromeda Galaxy', 'galaxy', [
    'M 31',
    'NGC 224',
    'Andromeda Galaxy',
  ]);
  const ngc7000 = item('NGC 7000', 'North America Nebula', 'emission_nebula', [
    'NGC 7000',
    'North America Nebula',
  ]);

  // Compact (no-space) form matches spaced designation
  it('"M31" matches target with primaryDesignation "M 31"', () => {
    expect(matchesSearch(m31, 'M31')).toBe(true);
  });

  it('"m31" matches "M 31" (case-insensitive + whitespace-insensitive)', () => {
    expect(matchesSearch(m31, 'm31')).toBe(true);
  });

  it('"M 31" matches "M 31" directly', () => {
    expect(matchesSearch(m31, 'M 31')).toBe(true);
  });

  // Proper-name substring matching via effectiveLabel
  it('"Andromeda" matches effectiveLabel "Andromeda Galaxy"', () => {
    expect(matchesSearch(m31, 'Andromeda')).toBe(true);
  });

  it('"andromeda" case-insensitive matches "Andromeda Galaxy"', () => {
    expect(matchesSearch(m31, 'andromeda')).toBe(true);
  });

  // NGC spaced/compact
  it('"NGC7000" matches target with primaryDesignation "NGC 7000"', () => {
    expect(matchesSearch(ngc7000, 'NGC7000')).toBe(true);
  });

  it('"ngc7000" matches "NGC 7000" (case + whitespace insensitive)', () => {
    expect(matchesSearch(ngc7000, 'ngc7000')).toBe(true);
  });

  // Non-matches
  it('"M31" does NOT match "NGC 7000"', () => {
    expect(matchesSearch(ngc7000, 'M31')).toBe(false);
  });

  it('"Andromeda" does NOT match "NGC 7000" or its label', () => {
    expect(matchesSearch(ngc7000, 'Andromeda')).toBe(false);
  });

  // Partial prefix matching
  it('"NGC" matches "NGC 7000" by prefix', () => {
    expect(matchesSearch(ngc7000, 'NGC')).toBe(true);
  });

  it('"North America" matches effectiveLabel "North America Nebula"', () => {
    expect(matchesSearch(ngc7000, 'North America')).toBe(true);
  });
});

// Alias-aware search (#29 / GF-11) moved to backend via `target.list(search)`.
// The `matchesSearch` helper is now designation+label only; alias coverage
// is tested at the Rust layer in `crates/app/targets/src/target_management/`.
describe('matchesSearch — alias search is backend-only (GF-11)', () => {
  it('"M 31" designation still matches via matchesSearch', () => {
    expect(matchesSearch(item('M 31', 'M 31'), 'M31')).toBe(true);
  });

  it('"Andromeda Galaxy" label still matches via matchesSearch', () => {
    expect(matchesSearch(item('M 31', 'Andromeda Galaxy'), 'Andromeda')).toBe(
      true,
    );
  });

  it('alias-only query ("NGC 224") does NOT match on the client — backend search needed', () => {
    // m31 with bare designation label; "NGC 224" is an alias not in effectiveLabel.
    expect(matchesSearch(item('M 31', 'M 31'), 'NGC 224')).toBe(false);
  });
});
