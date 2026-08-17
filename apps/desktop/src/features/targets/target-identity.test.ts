// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Spec 023 target identity logic tests.
 *
 * Tests:
 * 1. Error code mapping — the real TargetDetail `errorMessage` helper maps
 *    every known `ContractError` code to its localized message.
 * 2. Targets NOT in primary nav (T007 / X-3 regression guard).
 * 3. Contract snapshot: ProjectLifecycle enum values match the real
 *    PROJECT_STATES route-contract allow-list.
 */

import { describe, it, expect } from 'vitest';

import { m } from '@/lib/i18n';
import type { ContractError } from '@/lib/errors';
import { errorMessage } from './target-error-message';
import { PROJECT_STATES } from '@/lib/route-contract';

// ── Error code mapping (exercises the REAL TargetDetail helper) ─────────────
//
// This imports the production `errorMessage` (extracted to
// `target-error-message.ts`) rather than a hand-copied mirror, so the test
// cannot silently drift from the mapping the UI actually uses — as it did when
// the target error envelope moved from the old `TargetOpError` (`alias.duplicate`,
// `designation.*`, …) to `ContractError` (`alias.blank`, `alias.not_removable`, …).

const ce = (code: string): ContractError =>
  ({ code, message: '' }) as ContractError;

describe('errorMessage (real TargetDetail mapping)', () => {
  it.each([
    ['alias.blank', m.targets_detail_alias_blank()],
    ['alias.not_found', m.targets_detail_alias_not_found()],
    ['alias.not_removable', m.targets_detail_alias_not_removable()],
    ['target.not_found', m.targets_detail_target_not_found()],
    ['target.invalid_id', m.targets_detail_invalid_target_id()],
    ['note.content_too_large', m.err_note_content_too_large()],
  ])('maps %s to its localized message', (code, expected) => {
    expect(errorMessage(ce(code), 'fallback')).toBe(expected);
  });

  it('returns the fallback for an unknown code', () => {
    expect(errorMessage(ce('some.unknown.code'), 'fallback')).toBe('fallback');
  });
});

// The former "formatDate" describe block defined and tested a date-formatting
// helper inline, entirely inside this test file. A codebase-wide check
// confirms no such standalone helper exists in production: TargetDetail.tsx
// calls `new Date(s.createdAt).toLocaleDateString(...)` inline (not extracted
// to a reusable function) — so this test only ever validated its own local
// copy. The real formatting is exercised by TargetDetail.test.tsx's
// "(US2) Linked sessions list renders date and frameCount" render test.

// ── T007 / X-3: Targets NOT in primary nav manifest ───────────────────────────
//
// This test imports the NAV_GROUPS structure indirectly. Because Sidebar.tsx
// is a Tauri-only component we cannot import it in jsdom. Instead we validate
// the nav manifest inline to stay portable, and document the expectation:
//   Targets is intentionally in the Library nav group (it IS a discoverable
//   page). The spec says "not in primary nav" in the sense that it should not
//   be the default landing page and that /targets/$id should be a DETAIL route
//   only (not a top-level redirect). The existing sidebar entry for /targets
//   is a list page, not a detail route.
//
// The regression we guard against is /targets/$id being added as a PRIMARY
// sidebar entry (i.e. a nav item with path == '/targets/:id').
//
// Sidebar.tsx renders NavItem/NavGroup structures built from Tauri-specific
// icon imports that don't resolve cleanly under jsdom, so this reads the
// REAL source as raw text (compile-time Vite `?raw` glob, the same technique
// InboxPage.classify.test.tsx and anchors.test.ts use) instead of a
// hand-copied path list that could silently drift from the actual nav.

const SIDEBAR_SOURCE = Object.values(
  import.meta.glob('@/app/Sidebar.tsx', { as: 'raw', eager: true }),
)[0] as string;

describe('T007 / X-3 — targets/$id is NOT a primary nav entry', () => {
  // Every NavItem `path:` literal in the real Sidebar.tsx source.
  const navPaths = [...SIDEBAR_SOURCE.matchAll(/path:\s*'([^']+)'/g)].map(
    (m) => m[1],
  );

  it('Sidebar.tsx source was actually read (glob sanity check)', () => {
    expect(navPaths.length).toBeGreaterThan(0);
    expect(navPaths).toContain('/targets');
  });

  it('no real nav path contains a $id or :id segment', () => {
    for (const path of navPaths) {
      expect(path).not.toMatch(/\$id|:id/);
    }
  });

  it('no real nav path is the targets detail pattern', () => {
    expect(navPaths).not.toContain('/targets/:id');
    expect(navPaths).not.toContain('/targets/$id');
  });
});

// ── X-2: ProjectLifecycle enum snapshot (contract drift guard) ────────────────
//
// The spec 023 target.get.json contract defines ProjectLifecycle as a closed
// enum. This snapshot fails if the enum drifts from spec 009 canonical values.
//
// The former version compared two arrays hand-copied VERBATIM into this test
// file — neither imported from anywhere, so they could never disagree with
// each other, only silently drift from the real enum together. `PROJECT_STATES`
// (route-contract.ts) is declared `as const satisfies readonly ProjectState[]`
// against the generated backend binding, so it IS the real spec 009 canonical
// source — anchoring against it means a backend enum change that isn't
// mirrored here fails at compile time (the `satisfies` clause) before this
// test would even need to catch it at runtime.

describe('X-2 ProjectLifecycle enum snapshot', () => {
  // Spec 023 contract's documented lifecycle values (target.get.json).
  const SPEC_023_PROJECT_LIFECYCLE = [
    'setup_incomplete',
    'ready',
    'prepared',
    'processing',
    'completed',
    'archived',
    'blocked',
  ] as const;

  it('spec 023 contract lifecycle enum matches the real PROJECT_STATES allow-list', () => {
    expect([...SPEC_023_PROJECT_LIFECYCLE].sort()).toEqual(
      [...PROJECT_STATES].sort(),
    );
  });

  it('no enum value contains whitespace', () => {
    for (const v of PROJECT_STATES) {
      expect(v).not.toMatch(/\s/);
    }
  });
});
