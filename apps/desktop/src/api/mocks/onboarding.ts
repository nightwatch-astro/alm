// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// ── Onboarding (spec 056) — static seed data ──────────────────────────────────
//
// Static mock state mirroring the Rust ITEM_REGISTRY's shape (11 items, five
// FR-006 pages). The backend-authoritative auto-tick event path (research R5)
// is a documented no-op in mock mode (VC-002 limit): mock mode can never fake
// an `auto_checked` item — only the real bus subscriber produces them. Manual
// actions (`set_item_state`, `section_set`, `restore`) round-trip through the
// in-memory cache in mocks.ts so mock-mode checklist specs can exercise
// check-off, dismiss, remove, and restore without a backend.

import type { OnboardingFlagsDto, OnboardingItemDto } from '@/bindings/index';

export type MockOnboardingItemSeed = [
  itemId: string,
  page: OnboardingItemDto['page'],
  hasAutoTick: boolean,
  /** Upstream registry item id, mirroring the Rust `PrerequisiteDef`. */
  upstreamItemId?: string,
  /** Page that satisfies the prerequisite (defaults to the upstream's page). */
  jumpPage?: OnboardingItemDto['page'],
];

export const MOCK_ONBOARDING_ITEMS: MockOnboardingItemSeed[] = [
  ['inbox.confirm_first', 'inbox', true],
  ['inbox.apply_first_plan', 'inbox', true, 'inbox.confirm_first', 'inbox'],
  ['sessions.review_first', 'sessions', false, 'inbox.confirm_first', 'inbox'],
  ['sessions.add_note', 'sessions', false, 'inbox.confirm_first', 'inbox'],
  [
    'calibration.match_master',
    'calibration',
    false,
    'inbox.confirm_first',
    'inbox',
  ],
  ['calibration.review_masters', 'calibration', false],
  ['targets.resolve_first', 'targets', true],
  [
    'targets.add_favourite',
    'targets',
    false,
    'targets.resolve_first',
    'targets',
  ],
  ['projects.create_first', 'projects', true, 'inbox.confirm_first', 'inbox'],
  [
    'projects.launch_tool',
    'projects',
    true,
    'projects.create_first',
    'projects',
  ],
  [
    'projects.review_artifacts',
    'projects',
    false,
    'projects.launch_tool',
    'projects',
  ],
];

/**
 * Item ids seeded as BLOCKED (`met: false`).
 *
 * The real backend computes `met` from library milestones, not from checklist
 * state, and the mock library ships populated (confirmed inventory, resolved
 * targets, a project) — so the faithful default is "satisfied", which is also
 * what every pre-existing mock spec assumes. This escape hatch lets a spec seed
 * a genuinely blocked row (`localStorage`, before boot) to exercise the
 * prerequisite paths; `prerequisite` used to be flatly `null`, which made the
 * blocked branch untestable in mock mode at all.
 */
export const E2E_ONBOARDING_UNMET_STORE_ID = 'alm-e2e-onboarding-unmet';

/** Boolean e2e toggle read from `localStorage`; false when unset/unavailable. */
export function isE2EFlagSet(key: string): boolean {
  try {
    return (
      typeof localStorage !== 'undefined' &&
      localStorage.getItem(key) === 'true'
    );
  } catch {
    return false;
  }
}

/** Makes `inventory.list` report an empty library (see the handler below). */
export const E2E_EMPTY_INVENTORY_STORE_ID = 'alm-e2e-empty-inventory';

export function unmetPrerequisiteIds(): Set<string> {
  try {
    if (typeof localStorage === 'undefined') return new Set();
    const raw = localStorage.getItem(E2E_ONBOARDING_UNMET_STORE_ID);
    return raw ? new Set(JSON.parse(raw) as string[]) : new Set();
  } catch {
    return new Set();
  }
}

export function freshMockOnboardingItems(): OnboardingItemDto[] {
  const unmet = unmetPrerequisiteIds();
  return MOCK_ONBOARDING_ITEMS.map(
    ([itemId, page, hasAutoTick, upstreamItemId, jumpPage]) => ({
      itemId,
      page,
      state: 'unchecked',
      at: new Date().toISOString(),
      source: 'seed',
      prerequisite: upstreamItemId
        ? {
            upstreamItemId,
            met: !unmet.has(itemId),
            reasonKey: `onboarding.prerequisite.${upstreamItemId}`,
            jumpPage: jumpPage ?? page,
          }
        : null,
      hasAutoTick,
    }),
  );
}

// Persist the onboarding flags + settled item states across a `page.reload()`
// (module state alone re-initialises on reload). Mirrors the
// `E2E_OBSERVING_SEED_STORE_ID` single-JSON-blob round-trip above: hydrate once
// on first read, persist after every mutation. This is what makes the mock
// faithful to the real backend's durable persistence, so the cross-restart
// walk / collapse / removal specs (FR-004/FR-012/FR-013) are exercisable, and
// lets a test seed a pre-settled state via `localStorage` before boot.
export const E2E_ONBOARDING_STORE_ID = 'alm-e2e-onboarding';

export interface OnboardingSeed {
  flags?: Partial<OnboardingFlagsDto>;
  items?: Record<
    string,
    { state: OnboardingItemDto['state']; source?: OnboardingItemDto['source'] }
  >;
}
