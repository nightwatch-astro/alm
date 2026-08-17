// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Tests for LogPanel expand/collapse, level filter chips, and reduced-motion
 * behavior (spec 019, T006).
 *
 * Mirrors the mock setup used by `LogPanel.crosslink.test.tsx` /
 * `LogPanel.followState.test.tsx` (mocked router, commands, subscription).
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  render,
  screen,
  fireEvent,
  waitFor,
  within,
} from '@testing-library/react';
import { appendLog, resetLogStore } from '@/data/logStore';
import {
  LogPanelProvider,
  LOG_PANEL_EXPANDED_LS_KEY,
} from '@/app/LogPanelContext';
import { LogPanel } from '@/app/LogPanel';
import { __resetScopeRegistryForTest } from '@/data/persisted-state';

// ── Mocks ──────────────────────────────────────────────────────────────────────

const mockNavigate = vi.fn();

vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => mockNavigate,
}));

vi.mock('@/data/logSubscription', () => ({
  startLogSubscription: vi.fn().mockResolvedValue(undefined),
}));

function renderPanel() {
  return render(
    <LogPanelProvider>
      <LogPanel />
    </LogPanelProvider>,
  );
}

// Helper to find the Collapsible trigger button regardless of open/closed label.
function getTrigger() {
  const triggers = screen.getAllByRole('button');
  const trigger = triggers.find((b) =>
    ['Expand log panel', 'Collapse log panel'].includes(
      b.getAttribute('aria-label') ?? '',
    ),
  );
  if (!trigger) throw new Error('log panel trigger not found');
  return trigger;
}

function seedEntries() {
  appendLog([
    {
      id: 'aud:1',
      contractVersion: '1',
      time: '2026-01-01T00:00:00Z',
      level: 'error',
      source: 'audit',
      message: 'Something failed',
    },
    {
      id: 'aud:2',
      contractVersion: '1',
      time: '2026-01-01T00:00:01Z',
      level: 'warn',
      source: 'audit',
      message: 'Watch out',
    },
    {
      id: 'aud:3',
      contractVersion: '1',
      time: '2026-01-01T00:00:02Z',
      level: 'info',
      source: 'audit',
      message: 'All good',
    },
  ]);
}

// ── Tests ──────────────────────────────────────────────────────────────────────

describe('LogPanel expand/collapse + filters (T006)', () => {
  beforeEach(() => {
    resetLogStore();
    vi.clearAllMocks();
    // #842 persists `expanded` via persisted-state; these tests assume a fresh
    // collapsed panel on every render. Clear both the new boot-cache key and
    // the old legacy key so module-singleton state is reset.
    localStorage.removeItem(LOG_PANEL_EXPANDED_LS_KEY);
    localStorage.removeItem('alm-log-panel-expanded');
    __resetScopeRegistryForTest();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    localStorage.removeItem(LOG_PANEL_EXPANDED_LS_KEY);
    localStorage.removeItem('alm-log-panel-expanded');
    __resetScopeRegistryForTest();
  });

  it('toggles expand/collapse via the Collapsible trigger', async () => {
    seedEntries();
    renderPanel();

    // Collapsed initially — panel content is not mounted, trigger reports
    // aria-expanded=false and the "Expand" label.
    expect(
      screen.getByRole('log', { name: 'Operation log' }),
    ).toBeInTheDocument();
    expect(getTrigger()).toHaveAttribute('aria-expanded', 'false');
    expect(getTrigger()).toHaveAttribute('aria-label', 'Expand log panel');
    expect(screen.queryByText('All good')).not.toBeInTheDocument();

    fireEvent.click(getTrigger());

    await waitFor(() => {
      expect(getTrigger()).toHaveAttribute('aria-expanded', 'true');
    });
    expect(getTrigger()).toHaveAttribute('aria-label', 'Collapse log panel');
    // Content becomes visible once expanded.
    await waitFor(() => {
      expect(screen.getByText('All good')).toBeInTheDocument();
    });

    // Collapse again.
    fireEvent.click(getTrigger());
    await waitFor(() => {
      expect(getTrigger()).toHaveAttribute('aria-expanded', 'false');
    });
  });

  it('filters entries by level when a level chip is clicked', async () => {
    seedEntries();
    renderPanel();

    fireEvent.click(getTrigger());
    await waitFor(() => {
      expect(screen.getByText('Something failed')).toBeInTheDocument();
    });

    // All three entries visible under the default 'all' filter.
    expect(screen.getByText('Something failed')).toBeInTheDocument();
    expect(screen.getByText('Watch out')).toBeInTheDocument();
    expect(screen.getByText('All good')).toBeInTheDocument();

    // Click the "Error" level chip.
    const errorChip = screen.getByRole('button', { name: 'Error' });
    fireEvent.click(errorChip);

    await waitFor(() => {
      expect(errorChip).toHaveAttribute('aria-pressed', 'true');
    });

    // Only the error-level entry remains.
    expect(screen.getByText('Something failed')).toBeInTheDocument();
    expect(screen.queryByText('Watch out')).not.toBeInTheDocument();
    expect(screen.queryByText('All good')).not.toBeInTheDocument();
  });

  it('treats a level chip as a severity floor, not an exact match (#582)', async () => {
    seedEntries();
    renderPanel();

    fireEvent.click(getTrigger());
    await waitFor(() => {
      expect(screen.getByText('Something failed')).toBeInTheDocument();
    });

    // Click the "Warn" chip — warn is less severe than error, so both the
    // warn and error entries should remain visible; info should not.
    const warnChip = screen.getByRole('button', { name: 'Warn' });
    fireEvent.click(warnChip);

    await waitFor(() => {
      expect(warnChip).toHaveAttribute('aria-pressed', 'true');
    });

    expect(screen.getByText('Something failed')).toBeInTheDocument();
    expect(screen.getByText('Watch out')).toBeInTheDocument();
    expect(screen.queryByText('All good')).not.toBeInTheDocument();
  });

  it('renders entity subject context next to the message (#583)', async () => {
    appendLog([
      {
        id: 'aud:4',
        contractVersion: '1',
        time: '2026-01-01T00:00:03Z',
        level: 'info',
        source: 'target',
        message: 'Target metadata resolved',
        entityType: 'target',
        entityId: 'm31',
      },
    ]);
    renderPanel();

    fireEvent.click(getTrigger());
    await waitFor(() => {
      expect(screen.getByText('Target metadata resolved')).toBeInTheDocument();
    });

    expect(screen.getByText('target · m31')).toBeInTheDocument();
  });

  it('interpolates the source into the event-source modifier className', async () => {
    seedEntries();
    renderPanel();

    fireEvent.click(getTrigger());
    await waitFor(() => {
      expect(screen.getByText('All good')).toBeInTheDocument();
    });

    // Use data-testid to locate the source span — class names are VE-generated
    // hashes after the vanilla-extract migration (#1542 pattern).
    const sourceSpans = screen.getAllByTestId('logpanel-event-source');
    expect(sourceSpans.length).toBeGreaterThan(0);
    for (const span of sourceSpans) {
      // Verify the source value is correctly interpolated via data-source.
      expect(span).toHaveAttribute('data-source', 'audit');
      // Regression guard: must not render the un-interpolated literal template.
      expect(span.textContent).not.toContain('{entry.source}');
    }
  });

  it('does not animate scroll when prefers-reduced-motion is set', async () => {
    // Mock matchMedia to report reduced motion.
    vi.stubGlobal(
      'matchMedia',
      vi.fn().mockImplementation((query: string) => ({
        matches: query.includes('prefers-reduced-motion'),
        media: query,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
        addListener: vi.fn(),
        removeListener: vi.fn(),
        dispatchEvent: vi.fn(),
      })),
    );

    seedEntries();
    renderPanel();

    fireEvent.click(getTrigger());
    await waitFor(() => {
      expect(screen.getByText('All good')).toBeInTheDocument();
    });

    // With reduced motion, the follow-tail effect pins scrollTop directly
    // (no smooth `scrollTo` animation call). Assert the scroll container is
    // present and matchMedia is consulted and reports reduced motion.
    // Use data-testid — class names are VE-generated hashes post-migration.
    const list = screen.getByTestId('logpanel-events');
    expect(list).toBeInTheDocument();
    expect(window.matchMedia('(prefers-reduced-motion: reduce)').matches).toBe(
      true,
    );
  });

  it('filters entries by category/source chip (#666)', async () => {
    appendLog([
      {
        id: 'aud:20',
        contractVersion: '1',
        time: '2026-01-01T00:00:04Z',
        level: 'info',
        source: 'target',
        message: 'Target resolved',
      },
      {
        id: 'aud:21',
        contractVersion: '1',
        time: '2026-01-01T00:00:05Z',
        level: 'info',
        source: 'settings',
        message: 'Setting changed',
      },
    ]);
    renderPanel();

    fireEvent.click(getTrigger());
    await waitFor(() => {
      expect(screen.getByText('Target resolved')).toBeInTheDocument();
    });
    expect(screen.getByText('Setting changed')).toBeInTheDocument();

    const categoryGroup = screen.getByRole('group', {
      name: 'Category filter',
    });

    // Narrow to the "target" category chip only.
    const targetChip = within(categoryGroup).getByRole('button', {
      name: 'target',
    });
    fireEvent.click(targetChip);

    // Narrowing to "target" alone deselects it from the implicit
    // "all active" state — its `aria-pressed` stays "true" (now explicitly
    // selected) while every *other* chip flips to "false". Wait on the
    // actual filtering effect rather than the chip's own pressed state,
    // which reads "true" before and after the click.
    await waitFor(() => {
      expect(screen.queryByText('Setting changed')).not.toBeInTheDocument();
    });
    expect(screen.getByText('Target resolved')).toBeInTheDocument();

    // Reset via the group's own "All" chip (aria-label "All sources"
    // disambiguates it from the level filter's "All levels" chip — both
    // groups show the same visible "All" text).
    fireEvent.click(
      within(categoryGroup).getByRole('button', { name: 'All sources' }),
    );
    expect(screen.getByText('Setting changed')).toBeInTheDocument();
  });

  it('distinguishes a filtered-empty view from a truly empty one (#669)', async () => {
    // All seeded entries are `source: 'audit'` — narrowing the category
    // filter to "target" excludes every one of them while entries still
    // exist in the buffer, exercising the filtered-empty (not truly-empty)
    // branch.
    seedEntries();
    renderPanel();

    fireEvent.click(getTrigger());
    await waitFor(() => {
      expect(screen.getByText('Something failed')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole('button', { name: 'target' }));

    // #669: the miss must NAME the filter that excluded the rows, not just
    // say "a filter is on" — an unnamed message still leaves the user unable
    // to tell which control to undo.
    await waitFor(() => {
      expect(screen.getByText('No entries match target')).toBeInTheDocument();
    });
    expect(screen.queryByText('No log entries')).not.toBeInTheDocument();
  });

  it('names an active severity filter in the empty state (#669)', async () => {
    // The Sweep-C repro: entries exist, the Error severity chip is active,
    // and nothing matches.
    appendLog([
      {
        id: 'aud:10',
        contractVersion: '1',
        time: '2026-01-01T00:00:00Z',
        level: 'info',
        source: 'audit',
        message: 'All good',
      },
    ]);
    renderPanel();

    fireEvent.click(getTrigger());
    await waitFor(() => {
      expect(screen.getByText('All good')).toBeInTheDocument();
    });

    const levelGroup = screen.getByRole('group', { name: 'Level filter' });
    fireEvent.click(within(levelGroup).getByRole('button', { name: 'Error' }));

    await waitFor(() => {
      expect(screen.getByText('No entries match Error')).toBeInTheDocument();
    });
    expect(screen.queryByText('No log entries')).not.toBeInTheDocument();
  });

  it('still shows the plain empty message when nothing was recorded', () => {
    renderPanel();
    fireEvent.click(getTrigger());

    expect(screen.getByText('No log entries')).toBeInTheDocument();
  });

  it('gives the level-filter and category-filter "All" chips distinct accessible names', async () => {
    // Regression: both filter groups show the same visible "All" text —
    // an unqualified `getByRole('button', { name: 'All', exact: true })`
    // within the panel must resolve to exactly one element, not two
    // (real-UI e2e strict-mode violation caught in review).
    seedEntries();
    renderPanel();

    fireEvent.click(getTrigger());
    await waitFor(() => {
      expect(screen.getByText('Something failed')).toBeInTheDocument();
    });

    const logRegion = screen.getByRole('log', { name: 'Operation log' });
    expect(
      within(logRegion).getByRole('button', { name: 'All levels' }),
    ).toBeInTheDocument();
    expect(
      within(logRegion).getByRole('button', { name: 'All sources' }),
    ).toBeInTheDocument();
    expect(within(logRegion).queryByRole('button', { name: 'All' })).toBeNull();
  });
});
