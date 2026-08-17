// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * ITEM_ANCHORS contract (spec 056 follow-up): each mapped item id must (a)
 * report findable via `canFindItem`, and (b) point at a `data-guide-anchor`
 * string that is actually present on the control it claims to resolve.
 *
 * These are source-level assertions on purpose (mirrors
 * `walk.contract.test.tsx`): the anchored controls live deep in
 * data-dependent, non-trivially-mountable feature components (a virtualized
 * targets table, a session detail pane, a calibration match table), so
 * asserting against the real rendered DOM here would mean rebuilding most of
 * those pages' data plumbing without adding real coverage — the actual
 * resolution behavior (querySelector against the live DOM) is already
 * exercised by `FindSpotlight`'s own resolve-loop logic. What this test
 * closes is the drift case: someone renames or removes the anchor attribute
 * in the component without updating `ITEM_ANCHORS` (or vice versa), which
 * would silently turn a working spotlight into a permanent "missing" dead end.
 */

import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, it, expect } from 'vitest';
import type { OnboardingItemDto } from '@/bindings/index';
import { canFindItem, spotlightTargetFor } from './FindSpotlight';

const DIR = join(__dirname);

const ITEM_ANCHOR_SOURCES: Array<{
  itemId: string;
  anchor: string;
  sourceFile: string;
}> = [
  {
    itemId: 'targets.add_favourite',
    anchor: 'targets.favourite-toggle',
    sourceFile: '../targets/TargetsTable.tsx',
  },
  {
    itemId: 'sessions.add_note',
    anchor: 'sessions.note-field',
    sourceFile: '../sessions/SessionNotesSection.tsx',
  },
  {
    itemId: 'calibration.match_master',
    anchor: 'calibration.match-assign',
    sourceFile: '../calibration/MatchCandidatesPanel.tsx',
  },
  {
    // The "Add target" CTA opens the SIMBAD resolve flow. Anchoring it is what
    // lets the items that depend on `targets.resolve_first` spotlight it.
    itemId: 'targets.resolve_first',
    anchor: 'targets.resolve-cta',
    sourceFile: '../targets/TargetsPage.tsx',
  },
  {
    itemId: 'inbox.confirm_first',
    anchor: 'inbox.confirm-row',
    sourceFile: '../inbox/InboxTopBar.tsx',
  },
  {
    itemId: 'inbox.apply_first_plan',
    anchor: 'inbox.apply-plan-cta',
    sourceFile: '../inbox/PlanPanel.tsx',
  },
  {
    itemId: 'sessions.review_first',
    anchor: 'sessions.review-row',
    sourceFile: '../sessions/SessionsTable.tsx',
  },
  {
    itemId: 'projects.create_first',
    anchor: 'projects.create-cta',
    sourceFile: '../projects/ProjectsPage.tsx',
  },
  {
    itemId: 'projects.launch_tool',
    anchor: 'project.open-in-tool',
    sourceFile: '../projects/ProjectDetail.tsx',
  },
  {
    itemId: 'projects.review_artifacts',
    anchor: 'projects.artifacts-row',
    sourceFile: '../projects/ToolLaunchesAccordion.tsx',
  },
  {
    itemId: 'calibration.review_masters',
    anchor: 'calibration.review-row',
    sourceFile: '../calibration/MastersTable.tsx',
  },
];

describe('FindSpotlight ITEM_ANCHORS — every registry item', () => {
  for (const { itemId, anchor, sourceFile } of ITEM_ANCHOR_SOURCES) {
    it(`canFindItem('${itemId}') is true`, () => {
      expect(canFindItem(itemId)).toBe(true);
    });

    it(`'${anchor}' is wired as a data-guide-anchor in ${sourceFile}`, () => {
      const source = readFileSync(join(DIR, sourceFile), 'utf8');
      expect(source).toContain(anchor);
    });
  }

  it('covers all 11 registry items', () => {
    expect(ITEM_ANCHOR_SOURCES).toHaveLength(11);
    expect(new Set(ITEM_ANCHOR_SOURCES.map(({ itemId }) => itemId)).size).toBe(
      11,
    );
  });

  it('passes row-level anchors through the shared table primitive', () => {
    const source = readFileSync(join(DIR, '../../ui/Table.tsx'), 'utf8');
    expect(source).toContain('data-guide-anchor={row._guideAnchor}');
  });
});

describe('spotlightTargetFor — blocked items point at the prerequisite', () => {
  const item = (
    itemId: string,
    prerequisite: OnboardingItemDto['prerequisite'],
  ): OnboardingItemDto => ({
    itemId,
    page: 'targets',
    state: 'unchecked',
    at: '2026-01-01T00:00:00Z',
    source: 'seed',
    prerequisite,
    hasAutoTick: false,
  });

  const unmet = {
    upstreamItemId: 'targets.resolve_first',
    met: false,
    reasonKey: 'onboarding.prerequisite.targets.resolve_first',
    jumpPage: 'targets' as const,
  };

  it('redirects a blocked item to the upstream control and page', () => {
    expect(spotlightTargetFor(item('targets.add_favourite', unmet))).toEqual({
      itemId: 'targets.resolve_first',
      anchor: 'targets.resolve-cta',
      page: 'targets',
      viaPrerequisite: true,
    });
  });

  it('uses the item’s own control once the prerequisite is met', () => {
    expect(
      spotlightTargetFor(
        item('targets.add_favourite', { ...unmet, met: true }),
      ),
    ).toEqual({
      itemId: 'targets.add_favourite',
      anchor: 'targets.favourite-toggle',
      page: 'targets',
      viaPrerequisite: false,
    });
  });
});
