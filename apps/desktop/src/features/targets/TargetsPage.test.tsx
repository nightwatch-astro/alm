// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * TargetsPage tests — spec 043 shared list-page adoption (task #73).
 *
 * The page now uses the shared layout system: a pinned PageTopBar (FilterToolbar
 * with My Targets filter + search + catalogues + group-by) over a ListPageLayout
 * whose primary content is the full-width TargetsTable, with TargetDetailV2 in
 * the detail pane that mounts only on selection.
 *
 * Tests:
 *  1. Shows a loading footer while listTargets is in flight.
 *  2. Renders target rows from listTargets backend response.
 *  3. Detail pane mounts only on selection (no empty centered dashboard).
 *  4. Clicking a row triggers navigate with the target id.
 *  5. When selected UUID provided, TargetDetailV2 mounts and calls getTargetDetail.
 *  6. effectiveLabel from backend renders in the detail pane.
 *  7. Shows error state when listTargets rejects.
 *  8. Target count appears in the table footer.
 *  P1. "All targets" (default) filters to allowed planner catalogs.
 *  P2. Selecting "My Targets" shows a STUB empty state (no backend linkage).
 *  H1/H2/H3. Toolbar search filters the table by designation / label.
 *  H4. Search "M31" matches "M 31" (alias-aware whitespace normalization). (#103b)
 *  H5. Search "m31" matches "M 31" (case + whitespace insensitive). (#103b)
 *  MT1. My Targets filter toggle activates and deactivates via the select. (#91)
 *  G1. "Add target" button opens the add dialog.
 *  S1. Clicking a column header sorts the table.
 */

import {
  render as rtlRender,
  screen,
  fireEvent,
  waitFor,
  within,
  act,
} from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactElement } from 'react';

// TargetsPage (list load + the nested TargetDetailV2/useFavourites) is now
// TanStack-Query-backed — every render needs a QueryClientProvider ancestor.
// Shadowing `render` keeps every call site in this file unchanged.
function render(ui: ReactElement) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return rtlRender(ui, {
    wrapper: ({ children }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    ),
  });
}

// ── Hoist mocks ───────────────────────────────────────────────────────────────

const {
  mockListTargets,
  mockGetTargetDetail,
  mockSearchTargets,
  mockResolveTarget,
  mockAddTargetAlias,
  mockRemoveTargetAlias,
  mockSetDisplayAlias,
  mockClearDisplayAlias,
  mockListTargetSessions,
  mockListTargetProjects,
  mockGetTargetNote,
  mockUpdateTargetNote,
  mockAstroFormatBatch,
  mockMoonOppositionBatch,
} = vi.hoisted(() => ({
  mockListTargets: vi.fn(),
  mockGetTargetDetail: vi.fn(),
  mockSearchTargets: vi.fn(),
  mockResolveTarget: vi.fn(),
  mockAddTargetAlias: vi.fn(),
  mockRemoveTargetAlias: vi.fn(),
  mockSetDisplayAlias: vi.fn(),
  mockClearDisplayAlias: vi.fn(),
  mockListTargetSessions: vi.fn(),
  mockListTargetProjects: vi.fn(),
  mockGetTargetNote: vi.fn(),
  mockUpdateTargetNote: vi.fn(),
  mockAstroFormatBatch: vi.fn(),
  // #634: TargetsTable's batched Moon-separation/opposition call.
  mockMoonOppositionBatch: vi.fn(),
}));

/** Wrap a value in the generated `{ status: 'ok' }` Result envelope. */
const ok = <T,>(data: T) => ({ status: 'ok' as const, data });

vi.mock('@/bindings/index', () => ({
  commands: {
    targetList: mockListTargets,
    targetGet: mockGetTargetDetail,
    targetSearch: mockSearchTargets,
    targetResolve: mockResolveTarget,
    targetAliasAdd: mockAddTargetAlias,
    targetAliasRemove: mockRemoveTargetAlias,
    targetDisplayAliasSet: mockSetDisplayAlias,
    targetDisplayAliasClear: mockClearDisplayAlias,
    targetSessionsList: mockListTargetSessions,
    targetProjectsList: mockListTargetProjects,
    targetNoteGet: mockGetTargetNote,
    targetNoteUpdate: mockUpdateTargetNote,
    targetAstroFormatBatch: mockAstroFormatBatch,
    targetMoonOppositionBatch: mockMoonOppositionBatch,
  },
}));

mockAddTargetAlias.mockResolvedValue(
  ok({ alias: { id: 'a', alias: 'x', kind: 'user' } }),
);
mockRemoveTargetAlias.mockResolvedValue(ok({ removed: true }));
mockSetDisplayAlias.mockResolvedValue(ok({}));
mockClearDisplayAlias.mockResolvedValue(ok({}));
mockListTargetSessions.mockResolvedValue(ok([]));
mockListTargetProjects.mockResolvedValue(ok([]));
mockGetTargetNote.mockResolvedValue(ok({ notes: null }));
mockUpdateTargetNote.mockResolvedValue(ok({ notes: null }));
mockAstroFormatBatch.mockResolvedValue(ok({ formatted: [] }));
// #634: default to "no result yet" for every requested id — rows render the
// existing explicit-unknown "—" state, matching the pre-#634 behavior these
// tests already assert on (no test here depends on a specific separation/
// opposition value).
mockMoonOppositionBatch.mockImplementation(
  async (req: { targets: Array<{ id: string }> }) =>
    ok({
      results: req.targets.map((t) => ({
        id: t.id,
        moonSeparationDeg: null,
        opposition: null,
      })),
    }),
);

const mockNavigate = vi.fn();
const mockSelectedId = { current: undefined as string | undefined };

vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => mockNavigate,
  useSearch: () => ({ selected: mockSelectedId.current }),
  // The no-site banner (spec 044 US3) links to Settings via `Link`, which
  // needs a router context this test doesn't provide. Stub it as a plain
  // anchor, consistent with TargetsTable.test.tsx/TargetDetailV2.test.tsx.
  Link: ({
    children,
    to,
    ...rest
  }: {
    children?: import('react').ReactNode;
    to: string;
  }) => (
    <a href={to} {...rest}>
      {children}
    </a>
  ),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockRejectedValue(new Error('no tauri in tests')),
}));

// ── Import under test ─────────────────────────────────────────────────────────

import { TargetsPage } from './TargetsPage';
import { __setSiteExistsForTest } from './site-gate';

// ── Fixtures ──────────────────────────────────────────────────────────────────

const TARGET_ID = '550e8400-e29b-41d4-a716-446655440201';

const listItems = [
  {
    id: TARGET_ID,
    effectiveLabel: 'NGC 7000',
    primaryDesignation: 'NGC 7000',
    objectType: 'emission_nebula',
  },
  {
    id: '550e8400-e29b-41d4-a716-446655440202',
    effectiveLabel: 'M 31',
    primaryDesignation: 'M 31',
    objectType: 'galaxy',
  },
];

function makeDetail() {
  return {
    id: TARGET_ID,
    primaryDesignation: 'NGC 7000',
    effectiveLabel: 'NGC 7000',
    objectType: 'emission_nebula',
    raDeg: 314.75,
    decDeg: 44.37,
    simbadOid: 2_222_222,
    source: 'resolved',
    aliases: [],
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  __setSiteExistsForTest(null); // default to the real (false) site binding
  mockSelectedId.current = undefined;
  mockListTargets.mockResolvedValue(ok(listItems));
  mockGetTargetDetail.mockResolvedValue(ok(makeDetail()));
  mockSearchTargets.mockResolvedValue(
    ok({ contractVersion: '1.0', requestId: 'r', suggestions: [] }),
  );
  mockResolveTarget.mockResolvedValue(
    ok({
      contractVersion: '1.0',
      requestId: 'r',
      status: 'unresolved',
      target: null,
      unresolvedReason: 'offline',
      error: null,
    }),
  );
});

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('TargetsPage', () => {
  it('1. shows a loading skeleton while listTargets is in flight', () => {
    mockListTargets.mockReturnValue(new Promise(() => {}));
    render(<TargetsPage />);
    // Loading now renders a skeleton (role="status") instead of a text footer.
    expect(screen.getByTestId('skeleton')).toBeInTheDocument();
  });

  it('2. renders target rows from backend response', async () => {
    render(<TargetsPage />);
    await waitFor(() => {
      expect(screen.getByText('NGC 7000')).toBeInTheDocument();
      expect(screen.getByText('M 31')).toBeInTheDocument();
    });
  });

  it('3. detail pane mounts only when a target is selected', async () => {
    const { rerender } = render(<TargetsPage />);
    await waitFor(() => screen.getByText('NGC 7000'));
    // No selection → no detail pane region.
    expect(
      screen.queryByRole('complementary', { name: 'Target details' }),
    ).not.toBeInTheDocument();

    mockSelectedId.current = TARGET_ID;
    rerender(<TargetsPage />);
    await waitFor(() =>
      expect(
        screen.getByRole('complementary', { name: 'Target details' }),
      ).toBeInTheDocument(),
    );
  });

  it('4. clicking a row triggers navigate', async () => {
    render(<TargetsPage />);
    await waitFor(() => screen.getByText('NGC 7000'));

    const cell = screen.getByText('NGC 7000');
    fireEvent.click(cell.closest('tr') ?? cell);

    expect(mockNavigate).toHaveBeenCalled();
  });

  it('5. when selected UUID provided, getTargetDetail is called', async () => {
    mockSelectedId.current = TARGET_ID;
    render(<TargetsPage />);
    await waitFor(() =>
      expect(mockGetTargetDetail).toHaveBeenCalledWith({ targetId: TARGET_ID }),
    );
  });

  it('6. effectiveLabel from backend renders in detail pane', async () => {
    mockSelectedId.current = TARGET_ID;
    render(<TargetsPage />);
    await waitFor(() => {
      const items = screen.getAllByText('NGC 7000');
      expect(items.length).toBeGreaterThanOrEqual(1);
    });
  });

  it('7. shows error message when listTargets rejects', async () => {
    mockListTargets.mockRejectedValue(new Error('db error'));
    render(<TargetsPage />);
    await waitFor(() =>
      expect(screen.getByText('Could not load targets.')).toBeInTheDocument(),
    );
  });

  it('8. target count appears in the table footer', async () => {
    render(<TargetsPage />);
    // Default tab is Planner; both NGC 7000 and M 31 are allowed catalogs.
    await waitFor(() =>
      expect(screen.getByText('2 targets')).toBeInTheDocument(),
    );
  });

  // ── P: My Targets vs Planner filter (task #40, task #91) ────────────────────

  it('P1. "All targets" (default) filters to allowed planner catalogs', async () => {
    mockListTargets.mockResolvedValue(
      ok([
        ...listItems,
        // double-star dump entries that must NOT show in the Planner
        {
          id: 'hd1',
          effectiveLabel: 'HD 1',
          primaryDesignation: 'HD 1',
          objectType: 'double_star',
        },
        {
          id: 'wds1',
          effectiveLabel: 'WDS J1',
          primaryDesignation: 'WDS J00057+4549',
          objectType: 'double_star',
        },
      ]),
    );
    render(<TargetsPage />);
    await waitFor(() =>
      expect(screen.getByText('NGC 7000')).toBeInTheDocument(),
    );

    expect(screen.getByText('M 31')).toBeInTheDocument();
    expect(screen.queryByText('HD 1')).not.toBeInTheDocument();
    expect(screen.queryByText('WDS J1')).not.toBeInTheDocument();
    // footer counts only the catalog targets
    expect(screen.getByText('2 targets')).toBeInTheDocument();
  });

  it('P2. selecting "My Targets" shows a STUB empty state (no backend linkage)', async () => {
    render(<TargetsPage />);
    await waitFor(() => screen.getByText('NGC 7000'));

    // The "Show" select is the My Targets filter; select the 'my' option.
    const showSelect = screen.getByRole('combobox', { name: 'Show' });
    fireEvent.change(showSelect, { target: { value: 'my' } });

    // task #18: new empty message when no favourites are starred
    expect(screen.getByText(/No favourites yet/i)).toBeInTheDocument();
    // Planner-only catalog items are gone from the list
    expect(screen.queryByText('NGC 7000')).not.toBeInTheDocument();
  });

  // ── H: Toolbar search filters ────────────────────────────────────────────────

  it('H1. search input filters by primaryDesignation', async () => {
    render(<TargetsPage />);
    await waitFor(() => screen.getByText('NGC 7000'));

    const searchInput = screen.getByPlaceholderText('Search targets…');
    fireEvent.change(searchInput, { target: { value: 'NGC' } });

    expect(screen.getByText('NGC 7000')).toBeInTheDocument();
    expect(screen.queryByText('M 31')).not.toBeInTheDocument();
  });

  it('H2. search input filters by effectiveLabel (case-insensitive)', async () => {
    render(<TargetsPage />);
    await waitFor(() => screen.getByText('M 31'));

    const searchInput = screen.getByPlaceholderText('Search targets…');
    fireEvent.change(searchInput, { target: { value: 'm 31' } });

    expect(screen.getByText('M 31')).toBeInTheDocument();
    expect(screen.queryByText('NGC 7000')).not.toBeInTheDocument();
  });

  it('H3. clearing search restores the full list', async () => {
    render(<TargetsPage />);
    await waitFor(() => screen.getByText('NGC 7000'));

    const searchInput = screen.getByPlaceholderText('Search targets…');
    fireEvent.change(searchInput, { target: { value: 'NGC' } });
    expect(screen.queryByText('M 31')).not.toBeInTheDocument();

    fireEvent.change(searchInput, { target: { value: '' } });
    expect(screen.getByText('NGC 7000')).toBeInTheDocument();
    expect(screen.getByText('M 31')).toBeInTheDocument();
  });

  it('H4. search "M31" matches "M 31" (alias-aware whitespace normalization)', async () => {
    render(<TargetsPage />);
    await waitFor(() => screen.getByText('M 31'));

    const searchInput = screen.getByPlaceholderText('Search targets…');
    fireEvent.change(searchInput, { target: { value: 'M31' } });

    expect(screen.getByText('M 31')).toBeInTheDocument();
    expect(screen.queryByText('NGC 7000')).not.toBeInTheDocument();
  });

  it('H5. search "m31" matches "M 31" (case + whitespace insensitive)', async () => {
    render(<TargetsPage />);
    await waitFor(() => screen.getByText('M 31'));

    const searchInput = screen.getByPlaceholderText('Search targets…');
    fireEvent.change(searchInput, { target: { value: 'm31' } });

    expect(screen.getByText('M 31')).toBeInTheDocument();
    expect(screen.queryByText('NGC 7000')).not.toBeInTheDocument();
  });

  // H6: alias-only query — the server search path (GF-11 / DS-16) requires that
  // typing an alias that is NOT the primary designation or effectiveLabel still
  // resolves the target.  On main the aliases field on TargetListItem covers
  // this client-side; once perf/ipc-surface (#1543) lands the backend filters
  // server-side.  The wiring (search lifted above useTargets) must hold in
  // both worlds.
  it('H6. alias-only query "Caldwell 20" returns the NGC 7000 target via alias match', async () => {
    mockListTargets.mockResolvedValue(
      ok([
        {
          id: TARGET_ID,
          effectiveLabel: 'NGC 7000',
          primaryDesignation: 'NGC 7000',
          objectType: 'emission_nebula',
          raDeg: 314.75,
          decDeg: 44.37,
          // Caldwell 20 is the Caldwell alias for NGC 7000.
          aliases: ['NGC 7000', 'Caldwell 20', 'C 20', 'North America Nebula'],
          sessionCount: 0,
        },
        {
          id: '550e8400-e29b-41d4-a716-446655440202',
          effectiveLabel: 'M 31',
          primaryDesignation: 'M 31',
          objectType: 'galaxy',
          aliases: ['M 31', 'NGC 224', 'Andromeda Galaxy'],
          sessionCount: 0,
        },
      ]),
    );
    render(<TargetsPage />);
    await waitFor(() => screen.getByText('NGC 7000'));

    const searchInput = screen.getByPlaceholderText('Search targets…');
    fireEvent.change(searchInput, { target: { value: 'Caldwell 20' } });

    // NGC 7000 matches via its alias; M 31 must NOT appear.
    expect(screen.getByText('NGC 7000')).toBeInTheDocument();
    expect(screen.queryByText('M 31')).not.toBeInTheDocument();
  });

  // ── MT: My Targets filter (#91) ──────────────────────────────────────────────

  it('MT1. My Targets filter toggles between full catalog and stub empty state', async () => {
    render(<TargetsPage />);
    await waitFor(() => screen.getByText('NGC 7000'));

    const showSelect = screen.getByRole('combobox', { name: 'Show' });

    // Default: "All targets" (empty value) — catalog rows visible.
    expect(screen.getByText('NGC 7000')).toBeInTheDocument();

    // Switch to My Targets — stub empty state (task #18: favourites not yet starred).
    fireEvent.change(showSelect, { target: { value: 'my' } });
    expect(screen.queryByText('NGC 7000')).not.toBeInTheDocument();
    expect(screen.getByText(/No favourites yet/i)).toBeInTheDocument();

    // Switch back to All — catalog rows return.
    fireEvent.change(showSelect, { target: { value: '' } });
    await waitFor(() =>
      expect(screen.getByText('NGC 7000')).toBeInTheDocument(),
    );
  });

  // ── G: Add target button ───────────────────────────────────────────────────

  it('G1. "Add target" button opens the add dialog', async () => {
    render(<TargetsPage />);
    await waitFor(() => screen.getByText('NGC 7000'));

    const addBtn = screen.getByRole('button', { name: /Add target/i });
    fireEvent.click(addBtn);

    await waitFor(() =>
      expect(
        screen.getByRole('dialog', { name: /Add target/i }),
      ).toBeInTheDocument(),
    );
  });

  // ── S: Sortable column headers ───────────────────────────────────────────────

  it('S1. clicking a column header sorts the table rows', async () => {
    render(<TargetsPage />);
    await waitFor(() => screen.getByText('NGC 7000'));

    const table = screen.getByRole('table');
    const designationHeader = screen.getByRole('button', {
      name: 'Sort by Designation',
    });

    // Default sort is designation asc → "M 31" before "NGC 7000".
    let rowText = within(table).getAllByText(/NGC 7000|M 31/);
    expect(rowText[0]).toHaveTextContent('M 31');

    // Toggle to desc → "NGC 7000" first.
    fireEvent.click(designationHeader);
    rowText = within(table).getAllByText(/NGC 7000|M 31/);
    expect(rowText[0]).toHaveTextContent('NGC 7000');
  });

  // ── Site gate (spec 047 D7) ───────────────────────────────────────────────

  // #618: the Moon summary / site prompt moved from the pinned top bar into
  // TargetsTable's own header zone (still unconditionally visible — required
  // by the #450 dead-gate regression guard, which asserts the prompt/summary
  // WITHOUT selecting any row).

  it('SG1. shows the observing-site prompt when no site exists (gated off)', async () => {
    __setSiteExistsForTest(false);
    render(<TargetsPage />);
    await waitFor(() => screen.getByText('NGC 7000'));
    // The planner bar shows the set-up-site prompt, not the Moon summary.
    expect(screen.getByTestId('planner-site-prompt')).toBeInTheDocument();
    expect(screen.queryByTestId('moon-summary')).not.toBeInTheDocument();
    expect(screen.getByText('Set up your observing site')).toBeInTheDocument();
  });

  it('SG2. renders the Moon summary when a site exists (gate open)', async () => {
    __setSiteExistsForTest(true);
    render(<TargetsPage />);
    await waitFor(() => screen.getByText('NGC 7000'));
    // Astronomy renders: the Moon summary is present, the prompt is gone.
    expect(screen.getByTestId('moon-summary')).toBeInTheDocument();
    expect(screen.queryByTestId('planner-site-prompt')).not.toBeInTheDocument();
  });

  // ── Filter-by-recommendation (spec 047 US3, FR-011) ──────────────────────────

  it('FR1. filtering to "Unknown" keeps only targets without coordinates', async () => {
    __setSiteExistsForTest(true);
    // NGC 7000 gets real coordinates; M 31 has none — its recommendation is
    // deterministically 'unknown' regardless of tonight's real Moon state.
    mockListTargets.mockResolvedValue(
      ok([
        { ...listItems[0], raDeg: 314.75, decDeg: 44.37 },
        { ...listItems[1], raDeg: null, decDeg: null },
      ]),
    );
    render(<TargetsPage />);
    await waitFor(() => screen.getByText('NGC 7000'));

    fireEvent.click(screen.getByLabelText('Unknown'));

    await waitFor(() => {
      expect(screen.queryByText('NGC 7000')).not.toBeInTheDocument();
      expect(screen.getByText('M 31')).toBeInTheDocument();
    });
  });

  it('FR2. deselecting the recommendation filter restores the full list', async () => {
    __setSiteExistsForTest(true);
    mockListTargets.mockResolvedValue(
      ok([
        { ...listItems[0], raDeg: 314.75, decDeg: 44.37 },
        { ...listItems[1], raDeg: null, decDeg: null },
      ]),
    );
    render(<TargetsPage />);
    await waitFor(() => screen.getByText('NGC 7000'));

    const unknownCheckbox = screen.getByLabelText('Unknown');
    fireEvent.click(unknownCheckbox);
    await waitFor(() =>
      expect(screen.queryByText('NGC 7000')).not.toBeInTheDocument(),
    );

    fireEvent.click(unknownCheckbox);
    await waitFor(() => {
      expect(screen.getByText('NGC 7000')).toBeInTheDocument();
      expect(screen.getByText('M 31')).toBeInTheDocument();
    });
  });
});

// ── Progressive reveal of a large catalogue (#573) ───────────────────────────
//
// TargetsTable's per-row astronomy pass over the WHOLE catalogue synchronously
// froze the app. TargetsPage now caps what it hands the table to REVEAL_CHUNK
// (300) rows on first paint, then grows the revealed set toward the full total
// on setTimeout macrotask boundaries so the browser can paint between chunks.
// The table footer ("{count} targets") reflects exactly the revealed count, so
// these tests drive the reveal timers with fake timers and assert on it.
// Valid Planner (NGC) rows so filterByCatalogues keeps them and the cap
// actually engages (a 2-row fixture never exceeds REVEAL_CHUNK). Module-scoped
// so the stale-selection suite can reuse it.
function ngcItems(n: number) {
  return Array.from({ length: n }, (_, i) => ({
    id: `ngc-${i}`,
    effectiveLabel: `NGC ${7000 + i}`,
    primaryDesignation: `NGC ${7000 + i}`,
    objectType: 'emission_nebula',
  }));
}

describe('TargetsPage — progressive reveal (#573)', () => {
  // Flush the listTargets promise chain (microtasks only) WITHOUT firing the
  // reveal setTimeout, so the first-paint cap is observable before any growth.
  async function flushLoad() {
    await act(async () => {
      for (let i = 0; i < 6; i++) await Promise.resolve();
    });
  }

  // Fire the delay-0 reveal timers (and their re-scheduled successors) while
  // NOT advancing far enough to trip observing-night's hourly interval.
  async function drainRevealTimers() {
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
  }

  it('caps first paint to REVEAL_CHUNK rows, then grows to the full catalogue on timers', async () => {
    vi.useFakeTimers();
    try {
      mockListTargets.mockResolvedValue(ok(ngcItems(350)));
      render(<TargetsPage />);
      await flushLoad();

      // First paint is capped — never the full 350-row synchronous burst.
      expect(screen.getByText('300 targets')).toBeInTheDocument();
      expect(screen.queryByText('350 targets')).not.toBeInTheDocument();

      // Draining the reveal timers grows the visible set to the full catalogue.
      await drainRevealTimers();
      expect(screen.getByText('350 targets')).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  // #919: search must find a target that exists in the full catalogue but
  // hasn't been revealed yet by the progressive-reveal loader — bypassing the
  // reveal cap for search, the same carve-out "My Targets" already gets.
  it('#919 search finds a target beyond the revealed prefix during the reveal window', async () => {
    vi.useFakeTimers();
    try {
      mockListTargets.mockResolvedValue(ok(ngcItems(350)));
      render(<TargetsPage />);
      await flushLoad();

      // Confirm the reveal cap is still in effect (row 349 not yet revealed).
      expect(screen.getByText('300 targets')).toBeInTheDocument();
      expect(screen.queryByText('NGC 7349')).not.toBeInTheDocument();

      // NGC 7349 is index 349 — past REVEAL_CHUNK (300) — but search must
      // still find it immediately, not wait for reveal to catch up.
      const search = screen.getByPlaceholderText('Search targets…');
      fireEvent.change(search, { target: { value: 'NGC 7349' } });
      await flushLoad();

      expect(screen.getByText('NGC 7349')).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it('does not reset the revealed count on a sort interaction', async () => {
    vi.useFakeTimers();
    try {
      mockListTargets.mockResolvedValue(ok(ngcItems(350)));
      render(<TargetsPage />);
      await flushLoad();
      await drainRevealTimers();
      expect(screen.getByText('350 targets')).toBeInTheDocument();

      // A sort toggle re-renders but must NOT send the reveal back to the first
      // chunk — only a fresh load() resets it.
      fireEvent.click(
        screen.getByRole('button', { name: 'Sort by Designation' }),
      );
      await flushLoad();

      expect(screen.getByText('350 targets')).toBeInTheDocument();
      expect(screen.queryByText('300 targets')).not.toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });
});

// ── Stale-selection cleanup (#735 item 2) ─────────────────────────────────────

/**
 * Targets was the one ledger page never wired for stale-id cleanup — tasks.md
 * T030 claimed all six were. Both directions are pinned: a valid id must
 * survive the in-flight list load (the #735 item 1 cold-reload race), and a
 * genuinely absent id must still be cleared once the list settles.
 */
describe('TargetsPage stale-selection cleanup (#735)', () => {
  it('keeps a valid ?selected= while targetList is still in flight', () => {
    mockListTargets.mockReturnValue(new Promise(() => {}));
    mockSelectedId.current = TARGET_ID;

    render(<TargetsPage />);

    expect(mockNavigate).not.toHaveBeenCalled();
  });

  it('leaves a selection that is present but not yet progressively revealed', async () => {
    // The page caps what reaches TargetsTable and grows it on a timer; an
    // unrevealed target is present, not stale, so the gate matches against the
    // FULL query data rather than the revealed slice.
    mockListTargets.mockResolvedValue(ok(ngcItems(350)));
    mockSelectedId.current = 'ngc-349';

    render(<TargetsPage />);

    await waitFor(() => expect(mockListTargets).toHaveBeenCalled());
    expect(mockNavigate).not.toHaveBeenCalledWith(
      expect.objectContaining({ replace: true }),
    );
  });

  it('clears a stale ?selected= once the list has settled without it', async () => {
    mockSelectedId.current = '550e8400-e29b-41d4-a716-4466554409ff';

    render(<TargetsPage />);

    await waitFor(() =>
      expect(mockNavigate).toHaveBeenCalledWith(
        expect.objectContaining({ replace: true }),
      ),
    );
  });
});
