// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * SessionsPage stale-selection gating (#735 item 1).
 *
 * Page-level WIRING, deliberately not hook logic: `use-stale-selection.test.tsx`
 * feeds the hook explicit booleans, so it structurally cannot catch a page that
 * derives `found` from a query result that is still empty because the list IPC
 * has not resolved yet. On a cold reload that misreads a perfectly valid
 * `?selected=` as stale and rewrites the URL without it, breaking the spec 020
 * SC-002 guarantee that a reload lands on the same selection.
 *
 * Both directions are asserted so the fix cannot regress into a gate that is
 * simply held open forever.
 */

import { render } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { InventorySource, InventorySession } from '@/bindings/index';

const sourcesState: {
  data: { sources: InventorySource[] } | undefined;
  loading: boolean;
  error: Error | undefined;
} = { data: { sources: [] }, loading: false, error: undefined };

vi.mock('./store', () => ({
  useInventorySources: () => sourcesState,
}));

// The detail pane drags in the whole session/calibration stack; the gate under
// test lives on the page, so a stub keeps this focused (and cheap).
vi.mock('./SessionDetail', () => ({
  SessionDetail: () => <div data-testid="session-detail-stub" />,
}));

// Spec 062: RelationProposalList, ManualRelationDialog, PanelGroupView, and
// MatchingSettingsPanel use TanStack Query; stub them so this test doesn't
// need a QueryClientProvider (the gate under test is the stale-selection
// wiring, not the groups surface).
vi.mock('./RelationProposalList', () => ({
  RelationProposalList: () => <div data-testid="proposal-list-stub" />,
}));
vi.mock('./ManualRelationDialog', () => ({
  ManualRelationDialog: () => null,
}));
vi.mock('./PanelGroupView', () => ({
  PanelGroupView: () => null,
}));
vi.mock('./MatchingSettingsPanel', () => ({
  MatchingSettingsPanel: () => null,
}));

const mockNavigate = vi.fn();
const mockSelectedId = { current: undefined as string | undefined };

vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => mockNavigate,
  useSearch: () => ({ selected: mockSelectedId.current }),
}));

import { SessionsPage } from './SessionsPage';

function makeSource(sessionId: string): InventorySource {
  const session = {
    id: sessionId,
    name: 'Session — 2026-07-11',
    sourceId: 'src-1',
    frames: 42,
    type: 'light',
    target: 'NGC 7000',
    filter: 'Ha',
    exposure: '300s',
    camera: 'ASI2600MM',
    gain: '100',
    binning: '1x1',
    setTemp: '-10',
    capturedOn: '2026-07-11',
    provenance: null,
    linked: null,
    relativePath: 'lights/2026-07-11',
    notes: null,
  } as unknown as InventorySession;

  return {
    id: 'src-1',
    path: 'D:/Astro',
    kind: 'library',
    state: 'connected',
    sessions: [session],
  } as unknown as InventorySource;
}

beforeEach(() => {
  vi.clearAllMocks();
  mockSelectedId.current = undefined;
  sourcesState.data = { sources: [] };
  sourcesState.loading = false;
  sourcesState.error = undefined;
});

describe('SessionsPage stale-selection gating (#735)', () => {
  it('keeps a valid ?selected= while the inventory query is still loading', () => {
    sourcesState.loading = true;
    sourcesState.data = undefined;
    mockSelectedId.current = 'ses-1';

    render(<SessionsPage />);

    expect(mockNavigate).not.toHaveBeenCalled();
  });

  it('still clears a genuinely absent id once the list has settled', () => {
    sourcesState.data = { sources: [makeSource('ses-other')] };
    mockSelectedId.current = 'ses-gone';

    render(<SessionsPage />);

    expect(mockNavigate).toHaveBeenCalledWith(
      expect.objectContaining({ replace: true }),
    );
  });
});
