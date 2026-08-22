// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { describe, it, expect, vi } from 'vitest';
import { entityPath } from './entity-path';

/**
 * Cases derive from the two pre-extraction implementations at 5f15a992b:
 * `AuditLog.tsx` (no fallback) and `log-panel-model.ts` (catalog + audit
 * fallback). Both built paths with bare template literals, so the absence of
 * percent-encoding below is the pinned pre-refactor contract, not an oversight.
 */
describe('entityPath', () => {
  describe('without a fallback (AuditLog boundary)', () => {
    it('resolves the three entity kinds with detail pages', () => {
      expect(entityPath('project', 'p1')).toBe('/projects/p1');
      expect(entityPath('session', 's1')).toBe('/sessions/s1');
      expect(entityPath('target', 't1')).toBe('/targets/t1');
    });

    it('leaves plan unlinked', () => {
      expect(entityPath('plan', 'pl1')).toBeNull();
    });

    it('leaves every other entity type unlinked', () => {
      expect(entityPath('settings', 'x')).toBeNull();
      expect(entityPath('protection', 'x')).toBeNull();
      expect(entityPath('equipment', 'x')).toBeNull();
      expect(entityPath('', '')).toBeNull();
    });
  });

  describe('with a fallback (log-panel boundary)', () => {
    const fallback = (type: string, id: string) =>
      type === 'catalog'
        ? '/settings/catalogs'
        : `/settings/audit?entityType=${type}&entityId=${id}`;

    it('never routes plan through the fallback', () => {
      expect(entityPath('plan', 'pl1', { fallback })).toBeNull();
    });

    it('routes unhandled types through the fallback', () => {
      expect(entityPath('catalog', 'c1', { fallback })).toBe(
        '/settings/catalogs',
      );
      expect(entityPath('settings', 'root-3', { fallback })).toBe(
        '/settings/audit?entityType=settings&entityId=root-3',
      );
    });

    it('does not consult the fallback for kinds with detail pages', () => {
      const spy = vi.fn(fallback);
      expect(entityPath('project', 'p1', { fallback: spy })).toBe(
        '/projects/p1',
      );
      expect(spy).not.toHaveBeenCalled();
    });

    it('passes both arguments through unchanged', () => {
      const spy = vi.fn(() => null);
      entityPath('equipment', 'e 1/2', { fallback: spy });
      expect(spy).toHaveBeenCalledWith('equipment', 'e 1/2');
    });

    it('treats a null fallback result as unlinked', () => {
      expect(
        entityPath('equipment', 'e1', { fallback: () => null }),
      ).toBeNull();
    });
  });

  describe('id boundaries', () => {
    it('interpolates an empty id without encoding it away', () => {
      expect(entityPath('project', '')).toBe('/projects/');
    });

    it('interpolates separator-bearing ids verbatim', () => {
      expect(entityPath('session', 'a/b')).toBe('/sessions/a/b');
      expect(entityPath('target', '../escape')).toBe('/targets/../escape');
    });

    it('interpolates non-ASCII ids verbatim', () => {
      expect(entityPath('target', 'M31 Andrómeda')).toBe(
        '/targets/M31 Andrómeda',
      );
      expect(entityPath('project', '望遠鏡')).toBe('/projects/望遠鏡');
    });
  });
});
