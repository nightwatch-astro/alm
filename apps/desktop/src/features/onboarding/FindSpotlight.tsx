// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Find-It spotlight (spec 056, US4 T026; FR-022/FR-023; research R11).
 *
 * A single-step, NON-modal joyride run through the T009 adapter. Activating a
 * checklist row's find affordance points a spotlight at the real control on the
 * real page the item is about:
 *   - resolves the item's `data-guide-anchor` (the deterministic single anchor,
 *     spec edge case "two candidate anchors"),
 *   - navigates to the item's page first when needed (FR-022),
 *   - pulses the outline for the first seconds, then settles to a static
 *     outline (`prefers-reduced-motion` ⇒ no pulse),
 *   - stays up until dismissed — NEVER on a timer (FR-023). Dismissal paths:
 *     click the target, click the dimmed overlay, Escape, toggle the affordance
 *     again, or change pages.
 *   - when the control is not on screen (empty state / hidden panel / an anchor
 *     that lives on a per-record page) it explains why instead of spotlighting
 *     nothing (spec edge case).
 *
 * The affordance is a toggle held in a tiny module store so the SINGLE mounted
 * `ChecklistSection` (expanded sidebar OR open popover — never both) owns one
 * spotlight. Removing the section (T029) clears it via {@link clearFind}.
 */

import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { useNavigate, useRouterState } from '@tanstack/react-router';
import { useQueryClient } from '@tanstack/react-query';
import type { QueryClient } from '@tanstack/react-query';
import type { EventData } from 'react-joyride';
import type { OnboardingItemDto, OnboardingPage } from '@/bindings/index';
import { fetchFirstSessionId } from '@/features/sessions/store';
import { m } from '@/lib/i18n';
import { OnboardingJoyride, type OnboardingStep } from './joyrideAdapter';
import { prefersReducedMotion } from './choreography';
import {
  ONBOARDING_PAGE_PATHS,
  itemLabel,
  itemTooltip,
} from './onboarding-labels';

/**
 * Every registry item's real control has a deterministic page-level anchor.
 * Some data-dependent controls are absent until the user creates a record; in
 * that case the resolver shows the unavailable-target explanation rather than
 * spotlighting nothing (spec edge case). Values match the
 * `data-guide-anchor` attributes wired on the real pages.
 */
const ITEM_ANCHORS: Record<string, string> = {
  'inbox.confirm_first': 'inbox.confirm-row',
  'inbox.apply_first_plan': 'inbox.apply-plan-cta',
  'sessions.review_first': 'sessions.review-row',
  'projects.create_first': 'projects.create-cta',
  'projects.launch_tool': 'project.open-in-tool',
  'projects.review_artifacts': 'projects.artifacts-row',
  'targets.resolve_first': 'targets.resolve-cta',
  'targets.add_favourite': 'targets.favourite-toggle',
  'sessions.add_note': 'sessions.note-field',
  'calibration.match_master': 'calibration.match-assign',
  'calibration.review_masters': 'calibration.review-row',
};

/**
 * Whether this ITEM ID maps to a `data-guide-anchor` the spotlight can resolve.
 *
 * This is the raw anchor-map predicate. Row-level gating goes through
 * {@link spotlightTargetFor}, which also covers the blocked case (where the
 * control to point at belongs to the prerequisite, not the item itself).
 */
export function canFindItem(itemId: string): boolean {
  return itemId in ITEM_ANCHORS;
}

/** The control a find activation should point at, and the page it lives on. */
export interface SpotlightTarget {
  /** Whose control this is — the item itself, or its prerequisite. */
  itemId: string;
  anchor: string;
  page: OnboardingPage;
  /** True when we are pointing at the prerequisite's control instead. */
  viaPrerequisite: boolean;
}

/**
 * Resolve what a find activation on `item` should spotlight, or `null` when
 * nothing resolvable exists.
 *
 * A BLOCKED item is redirected to its prerequisite's control: its own control
 * is unreachable until the upstream milestone exists, so pointing at it would
 * be a dead end, while pointing at what to do first is exactly the answer to
 * "show me where". Callers MUST gate the affordance on a non-null result —
 * never offer an affordance that cannot succeed.
 */
export function spotlightTargetFor(
  item: OnboardingItemDto,
): SpotlightTarget | null {
  const prereq = item.prerequisite;
  if (prereq && !prereq.met) {
    const anchor = ITEM_ANCHORS[prereq.upstreamItemId];
    return anchor
      ? {
          itemId: prereq.upstreamItemId,
          anchor,
          page: prereq.jumpPage,
          viaPrerequisite: true,
        }
      : null;
  }
  const anchor = ITEM_ANCHORS[item.itemId];
  return anchor
    ? { itemId: item.itemId, anchor, page: item.page, viaPrerequisite: false }
    : null;
}

// ── Find toggle store ─────────────────────────────────────────────────────────

let activeItem: OnboardingItemDto | null = null;
const findSubs = new Set<() => void>();

function findEmit(): void {
  for (const fn of findSubs) fn();
}

/** Toggle the find spotlight for `item` (pressed-state semantics, T027). */
export function toggleFind(item: OnboardingItemDto): void {
  activeItem = activeItem?.itemId === item.itemId ? null : item;
  findEmit();
}

/** Dismiss any active spotlight (Escape/overlay/route/target/section-remove). */
export function clearFind(): void {
  if (activeItem === null) return;
  activeItem = null;
  findEmit();
}

function findSubscribe(fn: () => void): () => void {
  findSubs.add(fn);
  return () => findSubs.delete(fn);
}

function findSnapshot(): OnboardingItemDto | null {
  return activeItem;
}

/** The item whose find affordance is currently pressed (or `null`). */
export function useActiveFindItem(): OnboardingItemDto | null {
  return useSyncExternalStore(findSubscribe, findSnapshot, findSnapshot);
}

// ── Spotlight ─────────────────────────────────────────────────────────────────

const PULSE_MS = 2500;
// Cross-page navigation + React hydration under full-suite Playwright load can
// take up to ~2s; 3s gives the poll enough headroom without visibly delaying
// the unavailable-target fallback for genuinely absent anchors.
const RESOLVE_TIMEOUT_MS = 3000;
const RESOLVE_POLL_MS = 60;

// react-joyride wire strings (enum imports stay in the adapter — mirrored here).
const DISMISS_ACTIONS = new Set(['close', 'skip']);
const DISMISS_STATUSES = new Set(['finished', 'skipped']);

/** Mounted once by `ChecklistSection`; renders the active item's spotlight. */
export function FindSpotlight(): React.ReactElement | null {
  const item = useActiveFindItem();
  if (!item) return null;
  // Key by item id so a fresh find fully resets the resolve/navigate machine.
  return <SpotlightFor key={item.itemId} item={item} />;
}

type ResolveStatus = 'pending' | 'found' | 'missing';

/**
 * The route that puts `target`'s control on screen, or `null` when no such
 * route exists right now.
 *
 * Almost every anchor sits on its page's own top level, so the page path is the
 * answer. `sessions.note-field` is the exception: it lives on a session's
 * DETAIL pane, so the list route can never resolve it — it needs a real session
 * id. `/sessions/$id` is a redirect route that lands on `/sessions?selected=id`
 * (see `router.tsx`), which keeps the pathname-based dismissal below working.
 */
async function resolveSpotlightPath(
  target: SpotlightTarget,
  queryClient: QueryClient,
): Promise<string | null> {
  const basePath = ONBOARDING_PAGE_PATHS[target.page];
  if (target.anchor !== 'sessions.note-field') return basePath;
  const sessionId = await fetchFirstSessionId(queryClient);
  // No sessions yet ⇒ genuinely nothing to point at; fall to the apology
  // rather than navigating to a route that cannot show the field.
  return sessionId ? `${basePath}/${sessionId}` : null;
}

function SpotlightFor({
  item,
}: {
  item: OnboardingItemDto;
}): React.ReactElement | null {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const target = spotlightTargetFor(item);
  const selector = target ? `[data-guide-anchor="${target.anchor}"]` : null;
  // Falls back to the item's own page so the route-change dismissal below still
  // has a real path to compare against while the apology callout is showing.
  const targetPath = ONBOARDING_PAGE_PATHS[target?.page ?? item.page];
  const [status, setStatus] = useState<ResolveStatus>('pending');
  const arrivedRef = useRef(false);

  // Navigate to the target's page once (FR-022). Items with no resolvable
  // control skip this and fall straight to the unavailable state, as do
  // deep-link targets whose record does not exist yet.
  useEffect(() => {
    if (!target) return;
    let cancelled = false;
    void (async () => {
      const path = await resolveSpotlightPath(target, queryClient);
      if (cancelled) return;
      if (path === null) {
        setStatus('missing');
        return;
      }
      // Deep links (path !== basePath) navigate even when already on the page:
      // being on `/sessions` is not the same as having a session selected.
      if (!pathname.startsWith(targetPath) || path !== targetPath) {
        void navigate({ to: path });
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- intentional one-shot on mount
  }, []);

  // Resolve the (single, deterministic) target once we are on its page.
  useEffect(() => {
    if (!selector) {
      setStatus('missing');
      return;
    }
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const deadline = Date.now() + RESOLVE_TIMEOUT_MS;
    const tick = () => {
      if (cancelled) return;
      if (document.querySelector(selector)) {
        setStatus('found');
        return;
      }
      if (Date.now() > deadline) {
        setStatus('missing');
        return;
      }
      timer = setTimeout(tick, RESOLVE_POLL_MS);
    };
    tick();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [selector]);

  // Dismiss on navigation AWAY from the target page (FR-023). Arriving at the
  // target page is not a dismissal; leaving it after arrival is.
  useEffect(() => {
    if (pathname.startsWith(targetPath)) {
      arrivedRef.current = true;
    } else if (arrivedRef.current) {
      clearFind();
    }
  }, [pathname, targetPath]);

  // Pulse the outline for the first seconds, then static (FR-022). Signalled via
  // a root data-attribute the spotlight CSS keys off — portal-safe and
  // deterministic for tests. Reduced motion never pulses (VC-002 assertion).
  useEffect(() => {
    if (status !== 'found' || prefersReducedMotion()) return;
    const root = document.documentElement;
    root.dataset.onbSpotlightPulse = 'on';
    const t = setTimeout(() => {
      delete root.dataset.onbSpotlightPulse;
    }, PULSE_MS);
    return () => {
      clearTimeout(t);
      delete root.dataset.onbSpotlightPulse;
    };
  }, [status]);

  // Dismiss on clicking the real control (FR-023). The control stays
  // interactive (blockTargetInteraction=false), so this rides its click.
  useEffect(() => {
    if (status !== 'found' || !selector) return;
    const el = document.querySelector(selector);
    if (!el) return;
    const onClick = () => clearFind();
    el.addEventListener('click', onClick, { once: true, capture: true });
    return () => el.removeEventListener('click', onClick, { capture: true });
  }, [status, selector]);

  if (!selector || status === 'missing') {
    return (
      <div
        className="pv-onb-spotlight-unavailable"
        data-testid="onb-spotlight-unavailable"
        role="status"
      >
        <span>
          {m.onboarding_find_unavailable({ item: itemLabel(item.itemId) })}
        </span>
        <button
          type="button"
          className="pv-onb-spotlight-unavailable__close"
          onClick={() => clearFind()}
        >
          {m.common_close()}
        </button>
      </div>
    );
  }

  if (status !== 'found') return null;

  // Title always names the item the user asked about. When the spotlight was
  // redirected to the prerequisite's control, the body says whose control this
  // actually is and why we came here instead.
  const step: OnboardingStep = {
    target: selector,
    title: itemLabel(item.itemId),
    content: target?.viaPrerequisite
      ? m.onboarding_find_prerequisite_first({ item: itemLabel(target.itemId) })
      : itemTooltip(item.itemId),
    placement: 'auto',
    // Open immediately on activation — a single-step tour would otherwise wait
    // on joyride's click-to-open beacon.
    skipBeacon: true,
  };

  const onEvent = (data: EventData) => {
    if (
      DISMISS_ACTIONS.has(data.action) ||
      DISMISS_STATUSES.has(data.status) ||
      data.type === 'tour:end'
    ) {
      clearFind();
    }
  };

  return (
    <OnboardingJoyride
      steps={[step]}
      continuous={false}
      disableFocusTrap
      overlayClickAction="close"
      blockTargetInteraction={false}
      onEvent={onEvent}
    />
  );
}
