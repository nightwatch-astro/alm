// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * CommandPalette tests — target search wiring, PAGES route guards, and rendered
 * smoke coverage.
 *
 * Rendered smoke tests (#581 review) mount the real palette with a local
 * ResizeObserver/scrollIntoView stub (cmdk + @base-ui-components/react/dialog
 * need both; jsdom has neither) and assert the pv-palette* class wiring,
 * the initialFocus fix, and ArrowDown+Enter keyboard navigation. Pixel-level
 * visual verification stays with Playwright (WSL constraint).
 *
 * PAGES tests import the constant directly from CommandPalette.tsx (not a
 * hand-copied array) so a route rename/removal in production is caught here.
 *
 * `buildTargetResults` tests cover ranking/shaping only: matching lives in the
 * backend `target.list(search)` endpoint (kyo7.111), and the palette must not
 * re-filter its rows.
 */

import {
  describe,
  it,
  expect,
  vi,
  beforeAll,
  beforeEach,
  afterEach,
} from 'vitest';
import {
  render,
  fireEvent,
  waitFor,
  cleanup,
  act,
} from '@testing-library/react';
import { CommandPalette, PAGES, buildTargetResults } from './CommandPalette';
import { commands } from '@/bindings/index';
import type { TargetListItem } from '@/bindings/index';
import { router } from './router';
import { assertDefined } from '@/test/assertDefined';

// ── Mocks (rendered smoke tests) ──────────────────────────────────────────────

const mockNavigate = vi.fn();

vi.mock('@tanstack/react-router', async (importOriginal) => {
  const actual =
    await importOriginal<typeof import('@tanstack/react-router')>();
  return {
    ...actual,
    useNavigate: () => mockNavigate,
    useRouterState: () => '/',
  };
});

vi.mock('@/bindings/index', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/bindings/index')>();
  return {
    ...actual,
    commands: {
      ...actual.commands,
      settingsGet: vi
        .fn()
        .mockResolvedValue({ status: 'ok', data: { values: {} } }),
      targetList: vi.fn().mockResolvedValue({ status: 'ok', data: [] }),
      searchGlobal: vi.fn().mockResolvedValue({ status: 'ok', data: [] }),
    },
  };
});

// PAGES is imported directly from CommandPalette.tsx (the real source of
// truth) so this test cross-checks production routes instead of a
// hand-copied array that could silently drift (T007 guard).

// ── Tests ─────────────────────────────────────────────────────────────────────

// The former "CommandPalette routing logic (T008)" describe block (tests
// 1-6) asserted properties of the local MOCK_TARGET_RESULTS fixture only —
// it never called production code, and its "matched alias: NGC 224" sublabel
// shape doesn't match what buildTargetResults actually produces (see
// CommandPalette.tsx: sublabel is always primaryDesignation). Real coverage
// for routing/sublabel/sorting behavior lives in the
// 'buildTargetResults (#581 ...)' describe block below, which exercises the
// actual exported function against the real matcher.

describe('CommandPalette PAGES constant (T007 / X-3 guard)', () => {
  it('7. PAGES includes /targets list page', () => {
    expect(PAGES.some((p) => p.route === '/targets')).toBe(true);
  });

  it('8. PAGES does NOT include any /targets/:id or /targets/$id pattern', () => {
    for (const p of PAGES) {
      expect(p.route).not.toMatch(/^\/targets\/.+/);
    }
  });

  it('9. PAGES routes do not contain path params (no : or $ segments)', () => {
    for (const p of PAGES) {
      expect(p.route).not.toContain(':');
      expect(p.route).not.toContain('$');
    }
  });

  it('10. every PAGES label thunk resolves to a non-empty string', () => {
    // Exercises the real label() thunks (spec 046 #8 i18n) so a broken
    // message key would fail this test, not just a route typo.
    for (const p of PAGES) {
      expect(typeof p.label()).toBe('string');
      expect(p.label().length).toBeGreaterThan(0);
    }
  });

  it('11. every PAGES route exists in the real route tree (#617 dead-route guard)', () => {
    // Cross-checks against the production router (not a hand-copied path
    // list) so a palette entry pointing at a removed/renamed route fails
    // here instead of silently redirecting via the router's not-found
    // fallback (the exact #617 bug: /review, /plans, /audit routed nowhere).
    const realPaths = Object.keys(router.routesByPath);
    for (const p of PAGES) {
      expect(realPaths).toContain(p.route);
    }
  });
});

describe('CommandPalette debounce contract', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('11. debounces searchGlobal by 200ms after a query change', async () => {
    // Exercises the real setTimeout(..., 200) in CommandPalette.tsx rather
    // than pinning a local constant — a local constant can never disagree
    // with the component.
    await openPalette();
    const input = assertDefined(
      document.querySelector<HTMLInputElement>('.pv-palette__input'),
      'command palette search input',
    );
    vi.mocked(commands.searchGlobal).mockClear();

    vi.useFakeTimers();
    fireEvent.change(input, { target: { value: 'M31' } });

    await act(async () => {
      vi.advanceTimersByTime(199);
    });
    expect(commands.searchGlobal).not.toHaveBeenCalled();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(commands.searchGlobal).toHaveBeenCalledWith('M31');
  });
});

// ── buildTargetResults (#581, kyo7.111) ───────────────────────────────────────
//
// `target.list(search)` owns matching (alias-aware, over the ~13k-alias
// catalog); buildTargetResults only ranks and shapes the rows it is given. It
// must NOT re-filter: aliases are absent from `TargetListItem`, so a
// client-side filter would drop alias-only hits like "Caldwell 20" -> NGC 7000.

function targetItem(
  id: string,
  primaryDesignation: string,
  effectiveLabel?: string,
): TargetListItem {
  return {
    id,
    effectiveLabel: effectiveLabel ?? primaryDesignation,
    primaryDesignation,
    objectType: 'other',
    raDeg: 0,
    decDeg: 0,
    sessionCount: 0,
  };
}

describe('buildTargetResults (ranks server-filtered target.list rows)', () => {
  const m31 = targetItem('t-m31', 'M 31', 'Andromeda Galaxy');
  const ngc7000 = targetItem('t-ngc7000', 'NGC 7000', 'North America Nebula');
  const targets = [m31, ngc7000];

  it('empty query short-circuits to no results (no crash on blank input)', () => {
    expect(buildTargetResults(targets, '')).toEqual([]);
    expect(buildTargetResults(targets, '   ')).toEqual([]);
  });

  it('exact match: "M 31" scores highest and routes to /targets/<id>', () => {
    const results = buildTargetResults([m31], 'M 31');
    expect(results).toHaveLength(1);
    expect(results[0].id).toBe('t-m31');
    expect(results[0].route).toBe('/targets/t-m31');
    expect(results[0].score).toBe(1);
  });

  it('compact query "M31" still scores the spaced designation "M 31" as exact', () => {
    // Whitespace-collapsing normalization: "M31" === "M 31" for scoring.
    expect(buildTargetResults([m31], 'M31')[0].score).toBe(1);
  });

  it('prefix match scores above a plain contains match', () => {
    const prefixResult = buildTargetResults([ngc7000], 'NGC 70')[0];
    const containsResult = buildTargetResults([ngc7000], 'C 700')[0];
    expect(prefixResult.score ?? 0).toBeGreaterThan(containsResult.score ?? 0);
  });

  it('keeps an alias-only row the query text does not appear in (kyo7.111)', () => {
    // The regression this fix undoes: "Caldwell 20" appears in neither NGC
    // 7000's designation nor its label, so a client-side filter dropped the
    // row the backend had already matched by alias.
    const results = buildTargetResults([ngc7000], 'Caldwell 20');
    expect(results.map((r) => r.id)).toEqual(['t-ngc7000']);
    // Ranked below any designation/label match.
    expect(results[0].score).toBe(0.6);
  });

  it('caps results at the per-kind budget of 8', () => {
    const many = Array.from({ length: 12 }, (_, i) =>
      targetItem(`t-${i}`, `NGC ${7000 + i}`),
    );
    expect(buildTargetResults(many, 'NGC')).toHaveLength(8);
  });

  it('sublabel carries the primary designation when it differs from the label', () => {
    expect(buildTargetResults([m31], 'Andromeda')[0].sublabel).toBe('M 31');
  });

  it('sublabel is null when the designation equals the effective label', () => {
    const bare = targetItem('t-bare', 'Sh2-155', 'Sh2-155');
    expect(buildTargetResults([bare], 'Sh2-155')[0].sublabel).toBeNull();
  });

  it('results are sorted by descending score', () => {
    const results = buildTargetResults(
      [ngc7000, targetItem('t-exact', 'NGC')],
      'NGC',
    );
    expect(results[0].id).toBe('t-exact');
    for (let i = 1; i < results.length; i++) {
      expect(results[i - 1].score ?? 0).toBeGreaterThanOrEqual(
        results[i].score ?? 0,
      );
    }
  });
});

// ── Rendered smoke tests (#581 review) ────────────────────────────────────────
//
// These mount the real palette so the CSS class wiring, the initialFocus fix,
// and cmdk keyboard navigation have regression coverage — the styling blocker
// shipped precisely because nothing rendered the component.

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

beforeAll(() => {
  vi.stubGlobal('ResizeObserver', ResizeObserverStub);
  // cmdk scrolls the selected item into view; jsdom has no scrollIntoView.
  Element.prototype.scrollIntoView = vi.fn();
});

afterEach(() => {
  cleanup();
  mockNavigate.mockClear();
});

/** Renders the palette and opens it via the real Ctrl+K hotkey path. */
async function openPalette() {
  render(<CommandPalette />);
  fireEvent.keyDown(window, { key: 'k', code: 'KeyK', ctrlKey: true });
  await waitFor(() => {
    expect(document.querySelector('.pv-palette')).not.toBeNull();
  });
}

describe('CommandPalette rendered smoke (#581)', () => {
  it('opens on Ctrl+K with the expected pv-palette* class structure', async () => {
    await openPalette();
    expect(document.querySelector('.pv-palette-backdrop')).not.toBeNull();
    expect(document.querySelector('.pv-palette__input')).not.toBeNull();
    expect(document.querySelector('.pv-palette__list')).not.toBeNull();
    // Pages + Actions groups render without a query; each must carry the
    // styled class (the review blocker: cmdk only sets cmdk-group="",
    // so .pv-palette__group CSS was dead without an explicit className).
    const groups = document.querySelectorAll('.pv-palette__group');
    expect(groups.length).toBeGreaterThanOrEqual(2);
    for (const group of groups) {
      expect(group.querySelector('[cmdk-group-heading]')).not.toBeNull();
    }
    expect(
      document.querySelectorAll('.pv-palette__item').length,
    ).toBeGreaterThan(0);
  });

  it('gives the search input initial focus (initialFocus fix)', async () => {
    await openPalette();
    // The focus race left focus on the popup container, which silenced all
    // of cmdk's input-keydown plumbing (arrow keys, Enter, selection).
    const input = assertDefined(
      document.querySelector<HTMLInputElement>('.pv-palette__input'),
      'command palette search input',
    );
    await waitFor(() => {
      expect(document.activeElement).toBe(input);
    });
  });

  it('navigates via ArrowDown + Enter (cmdk keyboard nav reaches the input)', async () => {
    await openPalette();
    const input = assertDefined(
      document.querySelector<HTMLInputElement>('.pv-palette__input'),
      'command palette search input',
    );
    fireEvent.keyDown(input, { key: 'ArrowDown', code: 'ArrowDown' });
    fireEvent.keyDown(input, { key: 'Enter', code: 'Enter' });
    await waitFor(() => {
      expect(mockNavigate).toHaveBeenCalledTimes(1);
    });
    const call = mockNavigate.mock.calls[0][0] as { to: string };
    expect(PAGES.some((p) => p.route === call.to)).toBe(true);
  });

  it('navigates when an item is clicked (click-to-select)', async () => {
    await openPalette();
    const item = assertDefined(
      document.querySelector('.pv-palette__item'),
      'first command palette item',
    );
    // cmdk selects on pointer events, not plain click.
    fireEvent.pointerMove(item);
    fireEvent.pointerUp(item);
    fireEvent.click(item);
    await waitFor(() => {
      expect(mockNavigate).toHaveBeenCalled();
    });
  });
});

// ── Server-side target search (kyo7.111) ──────────────────────────────────────

describe('CommandPalette server-side target search', () => {
  beforeEach(() => {
    vi.mocked(commands.targetList).mockClear();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('does not fetch targets until the user types', async () => {
    await openPalette();
    expect(commands.targetList).not.toHaveBeenCalled();
  });

  it('forwards the query to target.list and shows an alias-only match', async () => {
    // The regression: "Caldwell 20" is an alias of NGC 7000 and appears in
    // neither its designation nor its label, so only the backend can match it.
    vi.mocked(commands.targetList).mockResolvedValue({
      status: 'ok',
      data: [
        {
          id: 't-ngc7000',
          effectiveLabel: 'North America Nebula',
          primaryDesignation: 'NGC 7000',
          objectType: 'other',
          raDeg: 0,
          decDeg: 0,
          sessionCount: 0,
        },
      ],
    } as Awaited<ReturnType<typeof commands.targetList>>);

    await openPalette();
    const input = assertDefined(
      document.querySelector<HTMLInputElement>('.pv-palette__input'),
      'command palette search input',
    );
    fireEvent.change(input, { target: { value: 'Caldwell 20' } });

    await waitFor(() => {
      expect(commands.targetList).toHaveBeenCalledWith('Caldwell 20');
    });
    await waitFor(() => {
      expect(document.body.textContent).toContain('North America Nebula');
    });
  });

  it('debounces target.list by 200ms', async () => {
    await openPalette();
    const input = assertDefined(
      document.querySelector<HTMLInputElement>('.pv-palette__input'),
      'command palette search input',
    );
    vi.mocked(commands.targetList).mockClear();

    vi.useFakeTimers();
    fireEvent.change(input, { target: { value: 'M31' } });
    await act(async () => {
      vi.advanceTimersByTime(199);
    });
    expect(commands.targetList).not.toHaveBeenCalled();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(commands.targetList).toHaveBeenCalledTimes(1);
  });
});
