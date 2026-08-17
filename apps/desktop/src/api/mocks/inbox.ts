// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// ── Inbox plan surface (spec 041) — seed fixtures ─────────────────────────────
//
// `inbox_plan_list_open` previously had no case, so `useOpenInboxPlans` always
// resolved to `[]` and the top-bar "Review plans" overlay was unreachable in
// mock mode. These seed plans make the overlay reachable AND make the
// move-vs-catalogue-in-place distinction observable at the plan-review layer
// (spec 041 FR-017/FR-018/SC-007):
//   plan-move-002     → every action is a `move` (unorganized source relocates
//                       into the library) → PlanPanel renders "→ <dest>".
//   plan-inplace-org  → every action is a `catalogue` (toPath == fromPath;
//                       already-organized source, no file moves) → PlanPanel
//                       renders the "In place · <folder>" label.
// Apply/cancel MUTATE mockInboxOpenPlans (in mocks.ts) so the aggregate surface
// refresh + auto-close behaviour round-trips like the backend.
//
// Shapes are pinned to the generated bindings (`InboxOpenPlan`/`InboxPlanAction`)
// so a contract change the mock fails to mirror is a compile error.

import type {
  InboxPlanAction,
  InboxOpenPlan,
  IngestionAttributionCandidateDto_Serialize,
} from '@/bindings/index';

/** A `move` action: source relocates to a distinct destination path. */
export function mockMoveAction(index: number, file: string): InboxPlanAction {
  return {
    index,
    action: 'move',
    fromPath: `/astro/raw/2025-10-10/darks/${file}`,
    toPath: `/astro/library/darks/2025-10-10/${file}`,
    destinationPreview: 'library/darks/2025-10-10/',
    requiresDestructiveConfirm: false,
  };
}

/** A `catalogue` action: file stays put (toPath == fromPath), no move. */
export function mockCatalogueAction(
  index: number,
  file: string,
): InboxPlanAction {
  const path = `/astro/library/NGC7000/${file}`;
  return {
    index,
    action: 'catalogue',
    fromPath: path,
    toPath: path,
    destinationPreview: 'library/NGC7000/',
    requiresDestructiveConfirm: false,
  };
}

export function seedInboxOpenPlans(): InboxOpenPlan[] {
  return [
    {
      inboxItemId: 'item-002',
      itemName: '2025-10-10/darks',
      planId: 'plan-move-002',
      state: 'plan_open',
      stale: false,
      actions: [
        mockMoveAction(1, 'dark_001.fits'),
        mockMoveAction(2, 'dark_002.fits'),
      ],
    },
    {
      inboxItemId: 'item-organized-inplace',
      itemName: 'Library/NGC7000',
      planId: 'plan-inplace-org',
      state: 'plan_open',
      stale: false,
      actions: [
        mockCatalogueAction(1, 'NGC7000_Ha_001.fits'),
        mockCatalogueAction(2, 'NGC7000_Ha_002.fits'),
      ],
    },
  ];
}

/**
 * Inbox item ids whose source is already ORGANIZED — `inbox_confirm` produces a
 * catalogue-in-place result (zero moves) for these, and a move plan otherwise
 * (spec 041 US4 FR-017/FR-018). Mirrors the backend's per-source
 * `organization_state` branch. `item-organized-inplace` is the seed plan's item;
 * confirming it (or any id here) yields the catalogue-in-place shape.
 */
export const MOCK_ORGANIZED_ITEM_IDS = new Set<string>([
  'item-organized-inplace',
]);

/**
 * Ranked attribution suggestions (spec 008 US7/FR-019). Ordered by descending
 * `matchScore`, ending in the always-present zero-score `new_project`
 * fallback, so the picker can be exercised without a real library: an
 * in-tolerance framing match, a completed-project match that offers reopen,
 * an optic-train mismatch, and the fallback.
 */
export const MOCK_ATTRIBUTION_CANDIDATES: IngestionAttributionCandidateDto_Serialize[] =
  [
    {
      kind: 'add_to_framing',
      projectId: 'proj-001',
      framingId: 'framing-001',
      targetId: 'target-ngc7000',
      matchScore: 0.94,
      reopen: false,
      opticMismatch: false,
    },
    {
      kind: 'new_framing',
      projectId: 'proj-002',
      framingId: null,
      targetId: 'target-ngc7000',
      matchScore: 0.61,
      reopen: true,
      opticMismatch: false,
    },
    {
      kind: 'flag_optic_difference',
      projectId: 'proj-003',
      framingId: null,
      targetId: 'target-ngc7000',
      matchScore: 0.33,
      reopen: false,
      opticMismatch: true,
    },
    {
      kind: 'new_project',
      projectId: null,
      framingId: null,
      targetId: null,
      matchScore: 0,
      reopen: false,
      opticMismatch: false,
    },
  ];

/** Plan-required lifecycle edges (mirrors `lifecycle-actions.ts` `requiresPlan`). */
export const MOCK_PLAN_REQUIRED_EDGES = new Set<string>([
  'ready→prepared',
  'prepared→ready',
  'completed→archived',
  'blocked→archived',
  'archived→ready',
  'archived→processing',
]);
