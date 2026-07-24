// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * TargetDetailV2 component tests — spec 036 gen-3 detail pane + spec 023 US2/US3/US4.
 *
 * Tests:
 *  1. Shows loading state while fetch is in flight.
 *  2. Renders effectiveLabel (displayAlias ?? primaryDesignation) in header.
 *  3. Renders primaryDesignation in identity section.
 *  4. Renders alias rows with kind badge.
 *  5. User aliases show a remove × button; SIMBAD aliases do not.
 *  6. Clicking × on a user alias calls removeTargetAlias with alias id.
 *  7. Add-alias form calls addTargetAlias with target id and alias text.
 *  8. Add-alias Enter keydown triggers addTargetAlias.
 *  9. Blank alias shows inline error without calling addTargetAlias.
 * 10. alias.blank error shows inline error message.
 * 11. alias.not_removable error shows inline error message.
 * 12. Shows error state when getTargetDetail rejects.
 * 13. Sessions empty-state renders when no linked sessions.
 * 14. Projects empty-state renders when no linked projects.
 * 15. Reloads detail after successful alias add.
 * 16. Reloads detail after successful alias remove.
 * 17. Display-alias Set/Edit button is visible.
 * 18. Setting display alias updates effectiveLabel.
 * 19. Clearing display alias reverts effectiveLabel to primaryDesignation.
 * 20. (US2) Linked sessions list renders date and frameCount.
 * 21. (US2) Clicking session row navigates to /sessions with selected=id.
 * 22. (US3) Linked projects list renders name and lifecycle.
 * 23. (US3) Clicking project row navigates to /projects with search: { selected: id }.
 * 24. (US4) Observing notes: empty state renders placeholder.
 * 25. (US4) Observing notes: existing notes body renders.
 * 26. (US4) Edit → save calls updateTargetNote and reflects result.
 * 27. (US4) Edit → cancel restores original notes.
 * 28. (US4) Save error shows banner message.
 */

import {
  configure,
  render as rtlRender,
  screen,
  fireEvent,
  waitFor,
  within,
} from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactElement } from 'react';
import { m } from '@/lib/i18n';

// TargetDetailV2 is now backed by TanStack Query (store.ts) — every render
// needs a QueryClientProvider ancestor. Shadowing `render` here (instead of
// touching every one of this file's ~30 call sites) keeps the diff mechanical.
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

// Windows-CI headroom (same flake class as PR #412's settings hydration races):
// every test in this file waits on content that only renders after mocked async
// hydration (detail/sessions/projects/notes effects). The waits are already
// deterministic — they target non-default, post-hydration content — but the
// RTL default asyncUtilTimeout (1s) and vitest default testTimeout (5s) are
// too tight for the very slow windows-latest runners (test 21 flaked there
// while identical siblings passed). Raise both file-wide instead of per-test
// so no sibling is left behind on tight defaults. Both settings are scoped to
// this file (vitest isolates test files; vi.setConfig is per-runtime).
configure({ asyncUtilTimeout: 10_000 });
vi.setConfig({ testTimeout: 15_000 });

// ── Hoist mocks ───────────────────────────────────────────────────────────────

const {
  mockGetTargetDetail,
  mockAddTargetAlias,
  mockRemoveTargetAlias,
  mockSetDisplayAlias,
  mockClearDisplayAlias,
  mockListTargetSessions,
  mockListTargetProjects,
  mockGetTargetNote,
  mockUpdateTargetNote,
  mockAstroFormatBatch,
} = vi.hoisted(() => ({
  mockGetTargetDetail: vi.fn(),
  mockAddTargetAlias: vi.fn(),
  mockRemoveTargetAlias: vi.fn(),
  mockSetDisplayAlias: vi.fn(),
  mockClearDisplayAlias: vi.fn(),
  mockListTargetSessions: vi.fn(),
  mockListTargetProjects: vi.fn(),
  mockGetTargetNote: vi.fn(),
  mockUpdateTargetNote: vi.fn(),
  mockAstroFormatBatch: vi.fn(),
}));

/** Wrap a value in the generated `{ status: 'ok' }` Result envelope. */
const ok = <T,>(data: T) => ({ status: 'ok' as const, data });

vi.mock('@/bindings/index', () => ({
  commands: {
    targetGet: mockGetTargetDetail,
    targetAliasAdd: mockAddTargetAlias,
    targetAliasRemove: mockRemoveTargetAlias,
    targetDisplayAliasSet: mockSetDisplayAlias,
    targetDisplayAliasClear: mockClearDisplayAlias,
    targetSessionsList: mockListTargetSessions,
    targetProjectsList: mockListTargetProjects,
    targetNoteGet: mockGetTargetNote,
    targetNoteUpdate: mockUpdateTargetNote,
    targetAstroFormatBatch: mockAstroFormatBatch,
  },
}));

const mockNavigate = vi.fn();
vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => mockNavigate,
  // Minimal stub: the no-site banner's "Add a site" link (spec 044 US3) just
  // needs to render as a link, not exercise real routing under test.
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

// FR-009 amendment (2026-07-17): switchable override for the Moon-aware best
// date so each tooltip state renders deterministically; `null` = the real
// (deterministic-ephemeris) computation for every other test.
const bestMoonMock = vi.hoisted(() => ({
  override: null as
    | null
    | (() => import('./astro/best-moon-date').BestMoonDate | null),
}));
vi.mock('./astro/best-moon-date', async (importOriginal) => {
  const actual =
    await importOriginal<typeof import('./astro/best-moon-date')>();
  return {
    ...actual,
    bestMoonDate: (
      ...args: Parameters<typeof actual.bestMoonDate>
    ): ReturnType<typeof actual.bestMoonDate> =>
      bestMoonMock.override
        ? bestMoonMock.override()
        : actual.bestMoonDate(...args),
  };
});

// ── Import under test (after mocks) ──────────────────────────────────────────

import { TargetDetailV2 } from './TargetDetailV2';
import { assertDefined } from '@/test/assertDefined';

// ── Fixtures ──────────────────────────────────────────────────────────────────

const TARGET_ID = '550e8400-e29b-41d4-a716-446655440201';

function makeDetail(overrides?: {
  displayAlias?: string | null;
  aliases?: Array<{
    id: string;
    alias: string;
    kind: 'designation' | 'common_name' | 'user';
  }>;
  raDeg?: number | null;
  decDeg?: number | null;
}) {
  const displayAlias = overrides?.displayAlias ?? null;
  const primaryDesignation = 'NGC 7000';
  return {
    id: TARGET_ID,
    primaryDesignation,
    displayAlias: displayAlias ?? undefined,
    effectiveLabel: displayAlias ?? primaryDesignation,
    objectType: 'emission_nebula',
    // `??` would treat an explicit `raDeg: null` override (the #757 fixture)
    // the same as "not provided" and silently fall back to the default —
    // check for presence instead so `null` is preserved.
    raDeg: overrides?.raDeg !== undefined ? overrides.raDeg : 314.75,
    decDeg: overrides?.decDeg !== undefined ? overrides.decDeg : 44.37,
    simbadOid: 2_222_222,
    source: 'resolved',
    aliases: overrides?.aliases ?? [
      { id: 'alias-desig-1', alias: 'NGC 7000', kind: 'designation' as const },
      {
        id: 'alias-cn-1',
        alias: 'North America Nebula',
        kind: 'common_name' as const,
      },
      { id: 'alias-user-1', alias: 'My Nebula', kind: 'user' as const },
    ],
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  mockNavigate.mockResolvedValue(undefined);
  mockGetTargetDetail.mockResolvedValue(ok(makeDetail()));
  mockAddTargetAlias.mockResolvedValue(
    ok({
      alias: { id: 'alias-user-new', alias: 'New Alias', kind: 'user' },
    }),
  );
  mockRemoveTargetAlias.mockResolvedValue(ok({ removed: true }));
  mockSetDisplayAlias.mockResolvedValue(
    ok(makeDetail({ displayAlias: 'My NGC 7000' })),
  );
  mockClearDisplayAlias.mockResolvedValue(
    ok(makeDetail({ displayAlias: null })),
  );
  // US2/US3/US4 defaults: empty lists, no notes.
  mockListTargetSessions.mockResolvedValue(ok([]));
  mockListTargetProjects.mockResolvedValue(ok([]));
  mockGetTargetNote.mockResolvedValue(ok({ notes: null }));
  mockUpdateTargetNote.mockResolvedValue(ok({ notes: null }));
  mockAstroFormatBatch.mockResolvedValue(
    ok({
      formatted: [
        {
          id: TARGET_ID,
          raSexagesimal: '20:59:00',
          decSexagesimal: '+44:22:12',
        },
      ],
    }),
  );
});

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('TargetDetailV2', () => {
  it('1. shows loading state while fetch is in flight', () => {
    mockGetTargetDetail.mockReturnValue(new Promise(() => {}));
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    // Loading now renders a skeleton (role="status") instead of text.
    expect(screen.getByTestId('skeleton')).toBeInTheDocument();
  });

  it('2. renders effectiveLabel in header', async () => {
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    // NGC 7000 appears in header and identity section; either is fine
    await waitFor(() => {
      const els = screen.getAllByText('NGC 7000');
      expect(els.length).toBeGreaterThanOrEqual(1);
    });
  });

  it('2b. when displayAlias is set, effectiveLabel shows it', async () => {
    mockGetTargetDetail.mockResolvedValue(
      ok(makeDetail({ displayAlias: 'My NGC 7000' })),
    );
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() => {
      const els = screen.getAllByText('My NGC 7000');
      expect(els.length).toBeGreaterThanOrEqual(1);
    });
  });

  it('3. renders primaryDesignation in identity section', async () => {
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    // Scoped to the identity PropertyTable's "Designation" row specifically
    // (not "NGC 7000 appears somewhere on the page", which test 2 already
    // covers for the header) — this fails if primaryDesignation stopped
    // reaching the identity section even though the header still shows it.
    await waitFor(() => {
      const label = screen.getByText(m.targets_col_designation());
      const row = label.closest('[role="row"]') as HTMLElement;
      expect(within(row).getByRole('cell')).toHaveTextContent('NGC 7000');
    });
  });

  it('4. renders alias rows with kind badge', async () => {
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() => {
      // NGC 7000 appears multiple times (header + alias + identity); just confirm presence
      expect(screen.getAllByText('NGC 7000').length).toBeGreaterThanOrEqual(1);
      expect(screen.getByText('North America Nebula')).toBeInTheDocument();
      expect(screen.getByText('My Nebula')).toBeInTheDocument();
    });
  });

  it('5. user aliases show remove button; SIMBAD aliases do not', async () => {
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() => screen.getByLabelText('Remove alias My Nebula'));

    expect(screen.getByLabelText('Remove alias My Nebula')).toBeInTheDocument();
    expect(
      screen.queryByLabelText('Remove alias NGC 7000'),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByLabelText('Remove alias North America Nebula'),
    ).not.toBeInTheDocument();
  });

  it('6. clicking × on user alias calls removeTargetAlias with alias id', async () => {
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() => screen.getByLabelText('Remove alias My Nebula'));

    fireEvent.click(screen.getByLabelText('Remove alias My Nebula'));

    await waitFor(() =>
      expect(mockRemoveTargetAlias).toHaveBeenCalledWith({
        targetId: TARGET_ID,
        aliasId: 'alias-user-1',
      }),
    );
  });

  it('7. add-alias form calls addTargetAlias with correct args', async () => {
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() => screen.getByRole('textbox', { name: /new alias/i }));

    fireEvent.change(screen.getByRole('textbox', { name: /new alias/i }), {
      target: { value: 'Pelican Region' },
    });
    fireEvent.click(screen.getByRole('button', { name: /^add$/i }));

    await waitFor(() =>
      expect(mockAddTargetAlias).toHaveBeenCalledWith({
        targetId: TARGET_ID,
        alias: 'Pelican Region',
      }),
    );
  });

  it('8. add-alias Enter keydown triggers addTargetAlias', async () => {
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() => screen.getByRole('textbox', { name: /new alias/i }));

    fireEvent.change(screen.getByRole('textbox', { name: /new alias/i }), {
      target: { value: 'Pelican Region' },
    });
    fireEvent.keyDown(screen.getByRole('textbox', { name: /new alias/i }), {
      key: 'Enter',
    });

    await waitFor(() => expect(mockAddTargetAlias).toHaveBeenCalled());
  });

  it('9. blank alias shows inline error without calling addTargetAlias', async () => {
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() => screen.getByRole('button', { name: /^add$/i }));

    fireEvent.click(screen.getByRole('button', { name: /^add$/i }));

    expect(mockAddTargetAlias).not.toHaveBeenCalled();
    expect(screen.getByText('Alias must not be blank.')).toBeInTheDocument();
  });

  it('10. alias.blank error from backend shows inline message', async () => {
    mockAddTargetAlias.mockRejectedValueOnce({
      code: 'alias.blank',
      message: 'blank',
    });
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() => screen.getByRole('textbox', { name: /new alias/i }));

    fireEvent.change(screen.getByRole('textbox', { name: /new alias/i }), {
      target: { value: 'x' },
    });
    fireEvent.click(screen.getByRole('button', { name: /^add$/i }));

    await waitFor(() =>
      expect(screen.getByText('Alias must not be blank.')).toBeInTheDocument(),
    );
  });

  it('11. alias.not_removable error shows inline message', async () => {
    mockRemoveTargetAlias.mockRejectedValueOnce({
      code: 'alias.not_removable',
      message: 'not removable',
    });
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() => screen.getByLabelText('Remove alias My Nebula'));

    fireEvent.click(screen.getByLabelText('Remove alias My Nebula'));

    await waitFor(() =>
      expect(
        screen.getByText('Only user-added aliases can be removed.'),
      ).toBeInTheDocument(),
    );
  });

  it('12. shows error state when getTargetDetail rejects', async () => {
    mockGetTargetDetail.mockRejectedValueOnce(new Error('network error'));
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() =>
      expect(screen.getByText('Could not load target.')).toBeInTheDocument(),
    );
  });

  it('13. sessions empty-state renders (single mid-page surface)', async () => {
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() =>
      expect(screen.getByText(/No linked sessions yet/i)).toBeInTheDocument(),
    );
    // The duplicate bottom "No sessions linked" section has been removed.
    expect(screen.queryByText('No sessions linked')).not.toBeInTheDocument();
  });

  it('14. projects empty-state renders (single mid-page surface)', async () => {
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() =>
      expect(screen.getByText('No projects linked yet.')).toBeInTheDocument(),
    );
    // #670: the duplicate bottom "Projects" section (same list + a redundant
    // EmptyState restating the same sentence as its own title) was removed —
    // same precedent as the Sessions section above.
    expect(screen.queryAllByText('No projects linked yet.').length).toBe(1);
  });

  it('15. reloads detail after successful alias add', async () => {
    const updated = makeDetail({
      aliases: [
        { id: 'alias-desig-1', alias: 'NGC 7000', kind: 'designation' },
        {
          id: 'alias-cn-1',
          alias: 'North America Nebula',
          kind: 'common_name',
        },
        { id: 'alias-user-1', alias: 'My Nebula', kind: 'user' },
        { id: 'alias-user-new', alias: 'New Alias', kind: 'user' },
      ],
    });
    mockGetTargetDetail
      .mockResolvedValueOnce(ok(makeDetail()))
      .mockResolvedValueOnce(ok(updated));

    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() => screen.getByRole('textbox', { name: /new alias/i }));

    fireEvent.change(screen.getByRole('textbox', { name: /new alias/i }), {
      target: { value: 'New Alias' },
    });
    fireEvent.click(screen.getByRole('button', { name: /^add$/i }));

    await waitFor(() => expect(mockGetTargetDetail).toHaveBeenCalledTimes(2));
    await waitFor(() =>
      expect(screen.getByText('New Alias')).toBeInTheDocument(),
    );
  });

  it('15b. (#658) calls onMutated after a successful alias add, so the host list can refresh', async () => {
    const onMutated = vi.fn();
    // Relies on the beforeEach persistent default for both the initial mount
    // fetch and the post-add reload — avoids polluting the mockResolvedValueOnce
    // queue for later tests (clearAllMocks doesn't drain unconsumed once-values).

    render(<TargetDetailV2 targetId={TARGET_ID} onMutated={onMutated} />);
    await waitFor(() => screen.getByRole('textbox', { name: /new alias/i }));

    fireEvent.change(screen.getByRole('textbox', { name: /new alias/i }), {
      target: { value: 'Sweep C Alias' },
    });
    fireEvent.click(screen.getByRole('button', { name: /^add$/i }));

    await waitFor(() => expect(onMutated).toHaveBeenCalledTimes(1));
  });

  it('16. reloads detail after successful alias remove', async () => {
    const updated = makeDetail({
      aliases: [
        { id: 'alias-desig-1', alias: 'NGC 7000', kind: 'designation' },
        {
          id: 'alias-cn-1',
          alias: 'North America Nebula',
          kind: 'common_name',
        },
      ],
    });
    mockGetTargetDetail
      .mockResolvedValueOnce(ok(makeDetail()))
      .mockResolvedValueOnce(ok(updated));

    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() => screen.getByLabelText('Remove alias My Nebula'));

    fireEvent.click(screen.getByLabelText('Remove alias My Nebula'));

    await waitFor(() => expect(mockGetTargetDetail).toHaveBeenCalledTimes(2));
    await waitFor(() =>
      expect(screen.queryByText('My Nebula')).not.toBeInTheDocument(),
    );
  });

  it('16b. alias add/remove refetch ONLY the detail query, not sessions/projects/notes (invalidation-prefix regression)', async () => {
    // `queryKeys.targets.detail(id)` (['targets', id]) is itself a PREFIX of
    // sessions(id)/projects(id)/notes(id)/astroFormat(id) (['targets', id,
    // ...]) — an unqualified invalidateQueries() on detail(id) would fuzzy-
    // match and refetch all four. Asserts store.ts's invalidateTarget() uses
    // `exact: true` so an alias mutation costs exactly one extra detail fetch.
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() => screen.getByRole('textbox', { name: /new alias/i }));
    expect(mockListTargetSessions).toHaveBeenCalledTimes(1);
    expect(mockListTargetProjects).toHaveBeenCalledTimes(1);
    expect(mockGetTargetNote).toHaveBeenCalledTimes(1);

    fireEvent.change(screen.getByRole('textbox', { name: /new alias/i }), {
      target: { value: 'Regression Alias' },
    });
    fireEvent.click(screen.getByRole('button', { name: /^add$/i }));
    await waitFor(() => expect(mockGetTargetDetail).toHaveBeenCalledTimes(2));

    expect(mockListTargetSessions).toHaveBeenCalledTimes(1);
    expect(mockListTargetProjects).toHaveBeenCalledTimes(1);
    expect(mockGetTargetNote).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByLabelText('Remove alias My Nebula'));
    await waitFor(() => expect(mockGetTargetDetail).toHaveBeenCalledTimes(3));

    expect(mockListTargetSessions).toHaveBeenCalledTimes(1);
    expect(mockListTargetProjects).toHaveBeenCalledTimes(1);
    expect(mockGetTargetNote).toHaveBeenCalledTimes(1);
  });

  it('17. display-alias Set/Edit button is visible', async () => {
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() =>
      expect(
        screen.getByRole('button', { name: /^set$/i }),
      ).toBeInTheDocument(),
    );
  });

  it('18. setting display alias updates effectiveLabel', async () => {
    // handleDisplayAliasSet applies mockSetDisplayAlias's response directly
    // (no second getTargetDetail fetch) — only the mount fetch needs mocking;
    // an unused second mockResolvedValueOnce here would leak into later tests
    // (clearAllMocks doesn't drain the once-queue).
    mockGetTargetDetail.mockResolvedValueOnce(ok(makeDetail()));

    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() => screen.getByRole('button', { name: /^set$/i }));

    fireEvent.click(screen.getByRole('button', { name: /^set$/i }));

    await waitFor(() =>
      screen.getByRole('textbox', { name: /display label/i }),
    );
    fireEvent.change(screen.getByRole('textbox', { name: /display label/i }), {
      target: { value: 'My NGC 7000' },
    });
    fireEvent.click(screen.getByRole('button', { name: /^save$/i }));

    await waitFor(() =>
      expect(mockSetDisplayAlias).toHaveBeenCalledWith({
        targetId: TARGET_ID,
        displayAlias: 'My NGC 7000',
      }),
    );
    // The API call succeeding isn't the same as the UI reflecting it — assert
    // the header actually re-renders with the new effectiveLabel (component
    // applies mockSetDisplayAlias's response to loadState).
    await waitFor(() =>
      expect(screen.getByRole('heading', { level: 2 })).toHaveTextContent(
        'My NGC 7000',
      ),
    );
  });

  it('18b. (#658) calls onMutated after setting a display alias, so the host list can refresh', async () => {
    const onMutated = vi.fn();
    // Relies on the beforeEach persistent defaults (mockGetTargetDetail for the
    // mount fetch, mockSetDisplayAlias for the save) — avoids polluting the
    // mockResolvedValueOnce queue for later tests.

    render(<TargetDetailV2 targetId={TARGET_ID} onMutated={onMutated} />);
    await waitFor(() => screen.getByRole('button', { name: /^set$/i }));
    fireEvent.click(screen.getByRole('button', { name: /^set$/i }));
    await waitFor(() =>
      screen.getByRole('textbox', { name: /display label/i }),
    );
    fireEvent.change(screen.getByRole('textbox', { name: /display label/i }), {
      target: { value: 'Sweep C Label' },
    });
    fireEvent.click(screen.getByRole('button', { name: /^save$/i }));

    await waitFor(() => expect(onMutated).toHaveBeenCalledTimes(1));
  });

  it('19. clearing display alias reverts effectiveLabel to primaryDesignation', async () => {
    mockGetTargetDetail.mockResolvedValue(
      ok(makeDetail({ displayAlias: 'My NGC 7000' })),
    );
    mockClearDisplayAlias.mockResolvedValue(
      ok(makeDetail({ displayAlias: null })),
    );

    render(<TargetDetailV2 targetId={TARGET_ID} />);
    // Wait for at least one Edit button (display-alias + notes may both show Edit)
    await waitFor(() =>
      expect(
        screen.getAllByRole('button', { name: /^edit$/i }).length,
      ).toBeGreaterThanOrEqual(1),
    );

    // The display-alias Edit button is the first in DOM order
    fireEvent.click(screen.getAllByRole('button', { name: /^edit$/i })[0]);
    await waitFor(() => screen.getByRole('button', { name: /^clear$/i }));

    fireEvent.click(screen.getByRole('button', { name: /^clear$/i }));

    await waitFor(() =>
      expect(mockClearDisplayAlias).toHaveBeenCalledWith({
        targetId: TARGET_ID,
      }),
    );
    // Assert the visible result, not just that the API was called — the
    // header must actually revert to primaryDesignation once
    // mockClearDisplayAlias's { displayAlias: null } response is applied.
    await waitFor(() =>
      expect(screen.getByRole('heading', { level: 2 })).toHaveTextContent(
        'NGC 7000',
      ),
    );
  });

  // ── US2: Linked sessions ───────────────────────────────────────────────────

  // The former test 20 ("sessions empty-state renders 'No linked sessions
  // yet.'") duplicated test 13 exactly: same default mock (ok([]) from
  // beforeEach), same assertion. Deleted rather than kept — it added no
  // coverage test 13 doesn't already provide.

  it('21. (US2) linked session rows render date and frameCount', async () => {
    mockListTargetSessions.mockResolvedValue(
      ok([
        {
          id: 'sess-1',
          sessionKey: '{}',
          createdAt: '2026-03-15T22:00:00Z',
          frameCount: 42,
          filter: '',
        },
      ]),
    );
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    // Slow-runner headroom now comes from the file-wide configure/setConfig
    // above (this test flaked on windows-latest while 22 below passed).
    await waitFor(() =>
      expect(screen.getByText(/42 frames/i)).toBeInTheDocument(),
    );
  });

  it('21b. (#739) linked session row renders the filter alongside date and frame count', async () => {
    mockListTargetSessions.mockResolvedValue(
      ok([
        {
          id: 'sess-ha',
          sessionKey: 'target|Ha|1x1|100|2026-03-15',
          createdAt: '2026-03-15T22:00:00Z',
          frameCount: 10,
          filter: 'Ha',
        },
      ]),
    );
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() => expect(screen.getByText('Ha')).toBeInTheDocument());
  });

  it('22. (US2) clicking session row navigates to /sessions with selected=id', async () => {
    mockListTargetSessions.mockResolvedValue(
      ok([
        {
          id: 'sess-abc',
          sessionKey: '{}',
          createdAt: '2026-03-15T22:00:00Z',
          frameCount: 5,
          filter: '',
        },
      ]),
    );
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() =>
      expect(screen.getByText(/5 frames/i)).toBeInTheDocument(),
    );

    fireEvent.click(
      assertDefined(
        screen.getByText(/5 frames/i).closest('button'),
        '"5 frames" button ancestor',
      ),
    );

    await waitFor(() =>
      expect(mockNavigate).toHaveBeenCalledWith({
        to: '/sessions',
        search: { selected: 'sess-abc' },
      }),
    );
  });

  // ── US3: Linked projects ───────────────────────────────────────────────────

  // The former test 23 ("projects empty-state renders 'No projects
  // linked.'") duplicated test 14 exactly: same default mock (ok([]) from
  // beforeEach), same assertion. Deleted for the same reason as test 20 above.

  it('24. (US3) linked project rows render name and lifecycle', async () => {
    mockListTargetProjects.mockResolvedValue(
      ok([{ id: 'proj-1', name: 'Horsehead 2026', lifecycle: 'ready' }]),
    );
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() =>
      expect(
        screen.getAllByText('Horsehead 2026').length,
      ).toBeGreaterThanOrEqual(1),
    );
    // #739 US3-AC2: lifecycle renders via the shared StatusTag, so it shows
    // the localized label ("Ready") rather than the raw stored state.
    expect(screen.getAllByText('Ready').length).toBeGreaterThanOrEqual(1);
    expect(screen.queryByText('ready')).toBeNull();
  });

  it('24b. (US3) archived projects carry the shared archived lifecycle tone', async () => {
    mockListTargetProjects.mockResolvedValue(
      ok([{ id: 'proj-1', name: 'Old Horsehead', lifecycle: 'archived' }]),
    );
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() =>
      expect(
        screen.getAllByText('Old Horsehead').length,
      ).toBeGreaterThanOrEqual(1),
    );
    // StatusTag renders the label text with a data-component attribute.
    const tagEl = screen.getAllByText('Archived')[0].closest('[data-component="status-tag"]');
    expect(tagEl).toHaveAttribute('data-variant', 'ok');
  });

  it('25. (US3) clicking project row navigates to /projects with selected=id (mid-page link row)', async () => {
    mockListTargetProjects.mockResolvedValue(
      ok([{ id: 'proj-1', name: 'Horsehead 2026', lifecycle: 'ready' }]),
    );
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() =>
      expect(
        screen.getAllByText('Horsehead 2026').length,
      ).toBeGreaterThanOrEqual(1),
    );

    // Click the first project button (mid-page link row)
    fireEvent.click(
      assertDefined(
        screen.getAllByText('Horsehead 2026')[0].closest('button'),
        '"Horsehead 2026" mid-page button ancestor',
      ),
    );

    await waitFor(() =>
      expect(mockNavigate).toHaveBeenCalledWith({
        to: '/projects',
        search: { selected: 'proj-1' },
      }),
    );
  });

  it('25b. (US3) clicking project row in bottom section navigates to /projects with selected=id', async () => {
    mockListTargetProjects.mockResolvedValue(
      ok([{ id: 'proj-1', name: 'Horsehead 2026', lifecycle: 'ready' }]),
    );
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() =>
      expect(
        screen.getAllByText('Horsehead 2026').length,
      ).toBeGreaterThanOrEqual(1),
    );

    // Click the last project button (bottom Projects section)
    const btns = screen
      .getAllByText('Horsehead 2026')
      .map((el) =>
        assertDefined(el.closest('button'), '"Horsehead 2026" button ancestor'),
      );
    fireEvent.click(btns[btns.length - 1]);

    await waitFor(() =>
      expect(mockNavigate).toHaveBeenCalledWith({
        to: '/projects',
        search: { selected: 'proj-1' },
      }),
    );
  });

  // ── #612: "+ New project here" carries the target id ──────────────────────

  it('25c. (#612) "+ New project here" navigates to /projects/new with the target id', async () => {
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() => screen.getByText('+ New project here'));

    fireEvent.click(screen.getByText('+ New project here'));

    expect(mockNavigate).toHaveBeenCalledWith({
      to: '/projects/new',
      search: { targetId: TARGET_ID },
    });
  });

  // ── #796: header pill row labels non-primary designation aliases ──────────

  it('25d. (#796) header catalog pills show a kind badge, matching the Aliases section', async () => {
    mockGetTargetDetail.mockResolvedValue(
      ok(
        makeDetail({
          aliases: [
            {
              id: 'alias-desig-primary',
              alias: 'NGC 7000',
              kind: 'designation',
            },
            {
              id: 'alias-desig-cross',
              alias: '2MASX J13295269+4711429',
              kind: 'designation',
            },
          ],
        }),
      ),
    );
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() =>
      expect(
        screen.getAllByText('2MASX J13295269+4711429', { exact: false }).length,
      ).toBeGreaterThanOrEqual(1),
    );

    // The header pill and the Aliases-section pill for the same alias both
    // carry the "[desig]" kind badge — no more unlabeled raw strings.
    const badges = screen.getAllByText('desig', { exact: false });
    expect(badges.length).toBeGreaterThanOrEqual(2);
  });

  // ── US4: Observing notes ───────────────────────────────────────────────────

  it('26. (US4) notes empty placeholder renders when no notes', async () => {
    mockGetTargetNote.mockResolvedValue(ok({ notes: null }));
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() =>
      expect(screen.getByTestId('target-notes-empty')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('target-notes-empty')).toHaveTextContent(
      'No notes yet.',
    );
  });

  it('27. (US4) existing notes body renders', async () => {
    mockGetTargetNote.mockResolvedValue(
      ok({ notes: 'Great transparency last night.' }),
    );
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() =>
      expect(screen.getByTestId('target-notes-body')).toHaveTextContent(
        'Great transparency last night.',
      ),
    );
  });

  it('28. (US4) edit → save calls updateTargetNote and reflects result', async () => {
    mockGetTargetNote.mockResolvedValue(ok({ notes: 'Old note' }));
    mockUpdateTargetNote.mockResolvedValue(ok({ notes: 'Updated note' }));
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() => screen.getByTestId('target-notes-body'));

    // Click Edit button (label reuses projects_detail_edit_btn = "Edit")
    fireEvent.click(screen.getByRole('button', { name: /^edit$/i }));
    await waitFor(() => screen.getByTestId('target-notes-textarea'));

    fireEvent.change(screen.getByTestId('target-notes-textarea'), {
      target: { value: 'Updated note' },
    });
    fireEvent.click(screen.getByRole('button', { name: /^save$/i }));

    await waitFor(() =>
      expect(mockUpdateTargetNote).toHaveBeenCalledWith({
        targetId: TARGET_ID,
        notes: 'Updated note',
      }),
    );
    await waitFor(() =>
      expect(screen.getByTestId('target-notes-body')).toHaveTextContent(
        'Updated note',
      ),
    );
  });

  it('29. (US4) edit → cancel restores original notes without calling update', async () => {
    mockGetTargetNote.mockResolvedValue(ok({ notes: 'Original note' }));
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() => screen.getByTestId('target-notes-body'));

    fireEvent.click(screen.getByRole('button', { name: /^edit$/i }));
    await waitFor(() => screen.getByTestId('target-notes-textarea'));

    fireEvent.change(screen.getByTestId('target-notes-textarea'), {
      target: { value: 'Changed but cancelled' },
    });
    fireEvent.click(screen.getByRole('button', { name: /^cancel$/i }));

    await waitFor(() =>
      expect(screen.getByTestId('target-notes-body')).toHaveTextContent(
        'Original note',
      ),
    );
    expect(mockUpdateTargetNote).not.toHaveBeenCalled();
  });

  it('30. (US4) save error shows banner message', async () => {
    mockGetTargetNote.mockResolvedValue(ok({ notes: null }));
    mockUpdateTargetNote.mockRejectedValue(new Error('db error'));
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() => screen.getByTestId('target-notes-empty'));

    // Open edit with the notes "Edit" button (last Edit button on page is notes section)
    const editBtns = screen.getAllByRole('button', { name: /^edit$/i });
    fireEvent.click(editBtns[editBtns.length - 1]);
    await waitFor(() => screen.getByTestId('target-notes-textarea'));

    fireEvent.change(screen.getByTestId('target-notes-textarea'), {
      target: { value: 'some notes' },
    });
    fireEvent.click(screen.getByRole('button', { name: /^save$/i }));

    await waitFor(() =>
      expect(screen.getByText('Could not save notes.')).toBeInTheDocument(),
    );
  });
});

// ── spec 044 Track B: US6/T015 no-site prompt, T018 tests ──────────────────────

describe('TargetDetailV2 — no-site prompt (US6/T015/T018)', () => {
  beforeEach(async () => {
    const { __setObservingStateForTest } = await import(
      './observing-sites/site-store'
    );
    __setObservingStateForTest({});
  });

  it('31. shows a no-site prompt in the Tonight column when there is no active site', async () => {
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() => {
      const els = screen.getAllByText(
        /Add an observing site.*see tonight's real altitude/i,
      );
      expect(els.length).toBeGreaterThanOrEqual(1);
    });
  });

  it('32. hides the no-site prompt and shows real tonight stats once a site is active', async () => {
    const { __setObservingStateForTest } = await import(
      './observing-sites/site-store'
    );
    __setObservingStateForTest({
      sites: [
        {
          id: 'site-1',
          name: 'Test Site',
          latitudeDeg: 52.37,
          longitudeDeg: 4.9,
          elevationM: 0,
          timezone: 'Europe/Amsterdam',
          twilight: 'astronomical',
          minHorizonAltDeg: 0,
        },
      ],
      activeSiteId: 'site-1',
      defaultSiteId: 'site-1',
    });
    render(<TargetDetailV2 targetId={TARGET_ID} />);
    await waitFor(() => {
      expect(
        screen.queryByText(
          /Add an observing site.*see tonight's real altitude/i,
        ),
      ).not.toBeInTheDocument();
      // Real max-alt stat renders once a site is active.
      expect(screen.getByText(/^Max alt/)).toBeInTheDocument();
    });
  });
});

// ── #757: null-coordinates crash + distinct un-plannable state ─────────────────
// ── #758: FR-020 Moon-separation trio rendered in the detail pane ──────────────

const SITE_744 = {
  id: 'site-744',
  name: 'Test Site',
  latitudeDeg: 52.37,
  longitudeDeg: 4.9,
  elevationM: 0,
  timezone: 'Europe/Amsterdam',
  twilight: 'astronomical' as const,
  minHorizonAltDeg: 0,
};

describe('TargetDetailV2 — needs-coordinates degrade + Moon-separation trio (#757/#758)', () => {
  beforeEach(async () => {
    const { __setObservingStateForTest } = await import(
      './observing-sites/site-store'
    );
    __setObservingStateForTest({
      sites: [SITE_744],
      activeSiteId: SITE_744.id,
      defaultSiteId: SITE_744.id,
    });
  });

  afterEach(() => {
    bestMoonMock.override = null;
  });

  it('33. (#757) renders a distinct "needs coordinates" banner instead of crashing when a site is active and the target has no RA/Dec', async () => {
    mockGetTargetDetail.mockResolvedValue(
      ok(makeDetail({ raDeg: null, decDeg: null })),
    );
    render(<TargetDetailV2 targetId={TARGET_ID} />);

    await waitFor(() =>
      expect(
        screen.getByText(
          /has no catalogued coordinates yet.*altitude, visibility, and imaging time can't be computed/i,
        ),
      ).toBeInTheDocument(),
    );
    // Not the no-site banner, and no fabricated tonight stats.
    expect(
      screen.queryByText(/Add an observing site.*see tonight's real altitude/i),
    ).not.toBeInTheDocument();
    expect(screen.queryByText(/^Max alt/)).not.toBeInTheDocument();
  });

  it('35. (FR-009 2026-07-17) diverged best date renders the moon-adjusted date with the both-nights explanation', async () => {
    bestMoonMock.override = () => ({
      dateMs: Date.UTC(2026, 0, 26),
      inDays: 25,
      state: 'diverged',
      oppositionDateMs: Date.UTC(2026, 1, 2),
      moonAtBest: { illumPct: 48, sepDeg: 98.4 },
      moonAtOpposition: { illumPct: 100, sepDeg: 2.2 },
    });
    render(<TargetDetailV2 targetId={TARGET_ID} />);

    await waitFor(() =>
      expect(screen.getByText('Best date')).toBeInTheDocument(),
    );
    // The shown date is the moon-adjusted night, not the opposition…
    const row = screen.getByText('Best date').closest('[role="row"]');
    expect(row?.textContent).toContain('Jan 26 · in 25 days');
    // …and the tooltip/aria-label explains both nights (InfoTip mirror).
    expect(
      screen.getByLabelText(
        /Opposition Feb 2 falls near full Moon \(100% lit, 2° away\)\. Best night within ±2 weeks: Jan 26 — Moon 48% lit, 98° from target\./,
      ),
    ).toBeInTheDocument();
  });

  it('36. (FR-009 2026-07-17) coinciding best date explains the favourable Moon on the opposition night', async () => {
    bestMoonMock.override = () => ({
      dateMs: Date.UTC(2026, 0, 18),
      inDays: 17,
      state: 'coincides',
      oppositionDateMs: Date.UTC(2026, 0, 18),
      moonAtBest: { illumPct: 1, sepDeg: 169.1 },
      moonAtOpposition: { illumPct: 1, sepDeg: 169.1 },
    });
    render(<TargetDetailV2 targetId={TARGET_ID} />);

    await waitFor(() =>
      expect(screen.getByText('Best date')).toBeInTheDocument(),
    );
    const row = screen.getByText('Best date').closest('[role="row"]');
    expect(row?.textContent).toContain('Jan 18 · in 17 days');
    expect(
      screen.getByLabelText(
        /Matches opposition — the Moon is favourable that night \(1% lit, 169° away\)\./,
      ),
    ).toBeInTheDocument();
  });

  it('37. (FR-009 2026-07-17) no viable night falls back to the opposition date with an explicit disclosure', async () => {
    bestMoonMock.override = () => ({
      dateMs: Date.UTC(2026, 1, 2),
      inDays: 32,
      state: 'none-viable',
      oppositionDateMs: Date.UTC(2026, 1, 2),
      moonAtBest: { illumPct: 99, sepDeg: 10 },
      moonAtOpposition: { illumPct: 99, sepDeg: 10 },
    });
    render(<TargetDetailV2 targetId={TARGET_ID} />);

    await waitFor(() =>
      expect(screen.getByText('Best date')).toBeInTheDocument(),
    );
    const row = screen.getByText('Best date').closest('[role="row"]');
    expect(row?.textContent).toContain('Feb 2 · in 32 days');
    expect(
      screen.getByLabelText(
        /No Moon-favourable night within ±2 weeks of opposition; showing the opposition date\./,
      ),
    ).toBeInTheDocument();
  });

  it('34. (#758) renders the Moon-separation trio (at transit / min over dark / dark midpoint) once tonight is available', async () => {
    render(<TargetDetailV2 targetId={TARGET_ID} />);

    await waitFor(() =>
      expect(screen.getByText('Moon separation')).toBeInTheDocument(),
    );
    for (const label of [
      'At transit',
      'Min over dark window',
      'At dark midpoint',
    ]) {
      const row = screen.getByText(label).closest('[role="row"]');
      expect(row).not.toBeNull();
      // Each figure is either a whole-degree value or the explicit
      // "Moon not up" state — never blank/fabricated (FR-020).
      expect(row?.textContent).toMatch(/\d+°|Moon not up/);
    }
  });
});
