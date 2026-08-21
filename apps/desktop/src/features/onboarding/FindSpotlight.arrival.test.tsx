// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Which signal starts the spotlight's anchor deadline (spec 056 FR-022).
 *
 * Two budgets share one resolve loop: a 3s pre-arrival navigation grace, and a
 * 4s post-arrival anchor wait restarted from the moment the target route is on
 * screen. Recording arrival on the wrong signal silently converts legitimate
 * navigation time into anchor-wait time, so each case below pins WHICH deadline
 * governs by probing at a time that only one of them has passed.
 *
 * Except where a case inserts one, the anchor never exists in these runs, so the
 * loop always ends in the callout; only its timing is under test.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, render, screen } from '@testing-library/react';
import type { OnboardingItemDto, OnboardingPage } from '@/bindings/index';
import {
  FindSpotlight,
  toggleFind,
  clearFind,
  useActiveFindItem,
} from './FindSpotlight';

let pathname = '/sessions';

vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => vi.fn(),
  useRouterState: ({ select }: { select: (s: unknown) => unknown }) =>
    select({ location: { pathname } }),
}));

vi.mock('@tanstack/react-query', () => ({
  useQueryClient: () => ({}),
}));

vi.mock('./joyrideAdapter', () => ({
  OnboardingJoyride: () => null,
}));

const mockFirstSessionId = vi.fn<() => Promise<string | null>>();
vi.mock('@/features/sessions/store', () => ({
  fetchFirstSessionId: () => mockFirstSessionId(),
}));

function item(itemId: string, page: OnboardingPage): OnboardingItemDto {
  return {
    itemId,
    page,
    state: 'unchecked',
    at: '2026-01-01T00:00:00Z',
    source: 'seed',
    prerequisite: null,
    hasAutoTick: false,
  };
}

/** Exposes the toggle-store state so dismissal is observable from the DOM. */
function Harness(): React.ReactElement {
  const active = useActiveFindItem();
  return (
    <>
      <span data-testid="active-item">{active?.itemId ?? 'none'}</span>
      <FindSpotlight />
    </>
  );
}

/**
 * Mount the spotlight for `item` and let the one-shot resolve effect settle.
 *
 * Returns a `navigate` that pushes a new pathname: the mocked `useRouterState`
 * has no subscription, so the re-render has to be driven explicitly.
 */
async function mountFind(
  active: OnboardingItemDto,
): Promise<(to: string) => Promise<void>> {
  toggleFind(active);
  const { rerender } = render(<Harness />);
  await act(async () => {
    await Promise.resolve();
  });
  return async (to: string) => {
    pathname = to;
    await act(async () => {
      rerender(<Harness />);
    });
  };
}

async function advance(ms: number): Promise<void> {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
}

const unavailable = () => screen.queryByTestId('onb-spotlight-unavailable');

beforeEach(() => {
  vi.useFakeTimers();
  pathname = '/sessions';
  mockFirstSessionId.mockResolvedValue('session-1');
});

afterEach(() => {
  clearFind();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('FindSpotlight arrival tracking', () => {
  it('does not start the anchor deadline while only the BASE route is on screen', async () => {
    // `sessions.note-field` resolves to `/sessions/session-1`; `/sessions` is
    // the list page the deep link navigates away from, not the destination.
    await mountFind(item('sessions.add_note', 'sessions'));

    await advance(2000);

    expect(unavailable()).toBeNull();
  });

  it('falls back once the pre-arrival navigation grace expires', async () => {
    await mountFind(item('sessions.add_note', 'sessions'));

    await advance(3200);

    expect(unavailable()).not.toBeNull();
  });

  it('starts the anchor deadline once the pathname reaches the RESOLVED path', async () => {
    const navigate = await mountFind(item('sessions.add_note', 'sessions'));
    await navigate('/sessions/session-1');

    // Past the pre-arrival grace, short of the post-arrival deadline: still
    // waiting proves the clock restarted on arrival.
    await advance(3200);
    expect(unavailable()).toBeNull();

    await advance(1200);
    expect(unavailable()).not.toBeNull();
  });

  it('starts the anchor deadline immediately when the resolved path IS the base route', async () => {
    pathname = '/projects';
    await mountFind(item('projects.create_first', 'projects'));

    await advance(3200);
    expect(unavailable()).toBeNull();

    await advance(1200);
    expect(unavailable()).not.toBeNull();
  });

  it('still finds a data-gated anchor that renders 2.5s after arrival', async () => {
    // The post-arrival phase waits on an IPC query and a render, not on the
    // route: with the budgets ordered the other way round (1.5s post-arrival)
    // this anchor is reported missing before it ever mounts, and that verdict is
    // terminal.
    pathname = '/projects';
    await mountFind(item('projects.create_first', 'projects'));

    await advance(2500);
    const anchor = document.createElement('button');
    anchor.dataset.guideAnchor = 'projects.create-cta';
    document.body.append(anchor);
    await advance(200);

    // Total elapsed now exceeds the post-arrival deadline: had the loop given up
    // before the anchor mounted, the callout would be latched on.
    await advance(2000);
    expect(unavailable()).toBeNull();

    anchor.remove();
  });

  it('dismisses on navigation away from the page even when the deep link never resolved', async () => {
    const navigate = await mountFind(item('sessions.add_note', 'sessions'));

    await navigate('/projects');

    expect(screen.getByTestId('active-item')).toHaveTextContent('none');
  });
});
