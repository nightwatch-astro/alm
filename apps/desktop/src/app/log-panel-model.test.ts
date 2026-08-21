// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { describe, it, expect } from 'vitest';
import { buildEntityPath } from './log-panel-model';

/** Table mirrors the switch this function had at 5f15a992b, case for case. */
describe('buildEntityPath', () => {
  it.each([
    ['plan', 'pl1', null],
    ['project', 'p1', '/projects/p1'],
    ['session', 's1', '/sessions/s1'],
    ['target', 't1', '/targets/t1'],
    ['catalog', 'c1', '/settings/catalogs'],
    ['settings', 'root-3', '/settings/audit?entityType=settings&entityId=root-3'],
    ['', '', '/settings/audit?entityType=&entityId='],
  ])('%s/%s resolves to %s', (entityType, entityId, expected) => {
    expect(buildEntityPath(entityType, entityId)).toBe(expected);
  });

  it('ignores the entity id for catalog', () => {
    expect(buildEntityPath('catalog', 'anything')).toBe('/settings/catalogs');
  });

  it('leaves fallback query values unencoded', () => {
    expect(buildEntityPath('equipment', 'a&b=c')).toBe(
      '/settings/audit?entityType=equipment&entityId=a&b=c',
    );
  });
});
