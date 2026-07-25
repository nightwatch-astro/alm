// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { useState, useEffect, useCallback, useRef } from 'react';
import { m } from '@/lib/i18n';
import { Dialog } from '@base-ui-components/react/dialog';
import { Command } from 'cmdk';
import { useNavigate, useRouterState } from '@tanstack/react-router';
import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import { openInNewWindow } from '@/lib/window';
import { useHotkeys } from '@/lib/useHotkeys';
import type { SearchResult } from '@/bindings/types';
import type { TargetListItem } from '@/bindings/index';

/** Matcher functions loaded from the Targets page (see {@link loadTargetMatcher}). */
type MatchesSearchFn = (t: TargetListItem, query: string) => boolean;
type NormalizeDesigFn = (s: string) => string;
interface TargetMatcher {
  matchesSearch: MatchesSearchFn;
  normalizeDesig: NormalizeDesigFn;
}

/**
 * Loads the Targets page's alias-aware matcher (#581) via a dynamic import
 * rather than a static one. `TargetsPage.tsx` is route-lazy-loaded (see
 * `router.tsx`'s `lazyRouteComponent`) and pulls in the astronomy engine +
 * moon-phase calculations; CommandPalette is always mounted from `Shell`, so a
 * static import would drag that whole feature into the eager bundle. A
 * dynamic `import()` of the same specifier shares one chunk with the route's
 * own lazy import — it stays lazy either way, and there is still exactly one
 * implementation of the matcher (no forked copy to drift out of sync).
 */
async function loadTargetMatcher(): Promise<TargetMatcher> {
  const mod = await import('@/features/targets/TargetsPage');
  return {
    matchesSearch: mod.matchesSearch,
    normalizeDesig: mod.normalizeDesig,
  };
}

/** Max target results shown (mirrors the backend's per-kind result budget). */
const MAX_TARGET_RESULTS = 8;

/**
 * How long a cached `targetList()` fetch is considered fresh (nJ09c/nJ10a
 * review carry-over). The palette previously refetched the full catalog on
 * every open; there is no target-change event to invalidate on, so a short
 * TTL is the cheapest staleness signal that still keeps the list from going
 * stale for a whole session while cutting the IPC round-trip on rapid
 * re-opens (e.g. Cmd+K, Esc, Cmd+K again).
 */
const TARGET_CACHE_TTL_MS = 60_000;

/**
 * Scores a target match for ranking, mirroring the backend's exact >
 * prefix > contains heuristic (`crates/app/core/src/search.rs::score`) so
 * ordering feels consistent between the palette and `search_global`.
 */
function scoreTarget(
  t: TargetListItem,
  qNorm: string,
  normalizeDesig: NormalizeDesigFn,
): number {
  const label = normalizeDesig(t.effectiveLabel);
  const desig = normalizeDesig(t.primaryDesignation);
  if (label === qNorm || desig === qNorm) return 1;
  if (label.startsWith(qNorm) || desig.startsWith(qNorm)) return 0.92;
  if (label.includes(qNorm) || desig.includes(qNorm)) return 0.75;
  return 0.6; // alias-only match (e.g. "Andromeda" matching M 31 via aliases)
}

/**
 * Client-side target search (#581): `search_global`'s SQL `LIKE` matches
 * `primary_designation` verbatim, so a compact query like "M31" never matches
 * a stored designation like "M 31" — the exact bug report (M31 finds nothing,
 * M finds many unrelated targets). Reusing the Targets page's tested
 * `matchesSearch`/`normalizeDesig` (whitespace/case-insensitive, alias-aware)
 * keeps this in lockstep with the real search users already rely on at
 * `/targets`, instead of hand-rolling a second matcher here.
 */
export function buildTargetResults(
  targets: TargetListItem[],
  query: string,
  matcher: TargetMatcher,
): SearchResult[] {
  const q = query.trim();
  if (!q) return [];
  const qNorm = matcher.normalizeDesig(q);
  return targets
    .filter((t) => matcher.matchesSearch(t, q))
    .map((t) => ({
      id: t.id,
      kind: 'target' as const,
      label: t.effectiveLabel,
      sublabel:
        t.primaryDesignation !== t.effectiveLabel ? t.primaryDesignation : null,
      route: `/targets/${t.id}`,
      score: scoreTarget(t, qNorm, matcher.normalizeDesig),
    }))
    .sort((a, b) => (b.score ?? 0) - (a.score ?? 0))
    .slice(0, MAX_TARGET_RESULTS);
}

// `label` is a render-time thunk so it re-reads the active locale (spec 046 #8).
// Exported so tests can assert against the real source of truth (T007 guard)
// instead of a hand-copied array that could silently drift from production.
export const PAGES: Array<{ label: () => string; route: string }> = [
  { label: () => m.common_sessions(), route: '/sessions' },
  {
    label: () => m.settings_datasources_category_calibration(),
    route: '/calibration',
  },
  { label: () => m.nav_targets(), route: '/targets' },
  { label: () => m.common_projects(), route: '/projects' },
  { label: () => m.settings_page_title(), route: '/settings' },
];

interface PaletteAction {
  /** Render-time thunk so the label re-reads the active locale (spec 046 #8). */
  label: () => string;
  /** Route path for simple navigation actions. */
  route?: string;
  /** Custom handler for actions that need search params or other side effects. */
  onSelect?: () => void;
}

const ACTIONS: Array<PaletteAction> = [
  { label: () => m.projects_create_title(), route: '/projects/new' },
];

/**
 * Developer-only palette entry (spec 021 T013).
 * Appended to the Pages group only when devMode is on.
 */
const DEV_PAGES: Array<{ label: () => string; route: string }> = [
  { label: () => m.cmdk_dev_contracts(), route: '/dev/contracts' },
];

export function CommandPalette() {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<SearchResult[]>([]);
  const [devMode, setDevMode] = useState(false);
  const [allTargets, setAllTargets] = useState<TargetListItem[]>([]);
  const targetsFetchedAtRef = useRef(0);
  const navigate = useNavigate();
  const currentHref = useRouterState({ select: (s) => s.location.href });
  // Owns initial focus explicitly (base-ui `initialFocus`) instead of the
  // input's own `autoFocus`, which raced with the dialog's own focus
  // management and could leave focus on the popup container — arrow keys
  // and Enter never reached cmdk's keydown handler on the input (#581).
  const inputRef = useRef<HTMLInputElement>(null);

  // Load devMode from backend settings on mount (spec 021 T013).
  useEffect(() => {
    let cancelled = false;
    commands
      .settingsGet('advanced')
      .then(unwrap)
      .then((data) => {
        if (cancelled) return;
        const vals = data.values as Record<string, unknown>;
        setDevMode(vals?.devMode === true);
      })
      .catch(() => {
        // Backend unavailable — stay with default false.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Load the full target catalog when the palette opens (#581) so client-side
  // matches are ready by the time the user types. Deferred to open (not
  // mount) because CommandPalette lives in Shell — a mount-time fetch would
  // pull the whole catalog on every app boot even if the palette is never
  // used, and would go stale for targets added mid-session. Cached for
  // TARGET_CACHE_TTL_MS (nJ09c/nJ10a carry-over): re-opening the palette
  // within the TTL reuses `allTargets` instead of re-issuing the IPC call.
  useEffect(() => {
    if (!open) return;
    if (Date.now() - targetsFetchedAtRef.current < TARGET_CACHE_TTL_MS) return;
    let cancelled = false;
    commands
      .targetList(null)
      .then(unwrap)
      .then((items) => {
        if (cancelled) return;
        targetsFetchedAtRef.current = Date.now();
        setAllTargets(items);
      })
      .catch(() => {
        // Backend unavailable — palette falls back to sessions/projects only.
      });
    return () => {
      cancelled = true;
    };
  }, [open]);

  // ⌘/Ctrl+K toggles the palette. `$mod` resolves to Cmd on macOS, Ctrl
  // elsewhere — matching the prior `metaKey || ctrlKey` check. We opt out of the
  // form-field guard so the shortcut still fires while focus is in an input.
  useHotkeys(
    {
      '$mod+KeyK': (e) => {
        e.preventDefault();
        setOpen((v) => !v);
      },
    },
    [],
    { ignoreFormFields: false },
  );

  useEffect(() => {
    if (!query.trim()) {
      setResults([]);
      return;
    }
    const controller = new AbortController();
    // Debounce search by 200ms to avoid excessive API calls
    const timeoutId = setTimeout(() => {
      void Promise.all([
        loadTargetMatcher(),
        commands.searchGlobal(query).then(unwrap),
      ])
        .then(([matcher, backendResults]) => {
          if (controller.signal.aborted) return;
          // Backend target matches are dropped in favor of the client-side,
          // alias-aware matches above — see buildTargetResults for why.
          const nonTargetResults = backendResults.filter(
            (r) => r.kind !== 'target',
          );
          setResults([
            ...buildTargetResults(allTargets, query, matcher),
            ...nonTargetResults,
          ]);
        })
        .catch(() => {
          if (!controller.signal.aborted) setResults([]);
        });
    }, 200);
    return () => {
      clearTimeout(timeoutId);
      controller.abort();
    };
  }, [query, allTargets]);

  const select = useCallback(
    (route: string) => {
      setOpen(false);
      setQuery('');
      void navigate({ to: route });
    },
    [navigate],
  );

  const selectAction = useCallback(
    (action: PaletteAction) => {
      setOpen(false);
      setQuery('');
      if (action.onSelect) {
        action.onSelect();
      } else if (action.route) {
        void navigate({ to: action.route });
      }
    },
    [navigate],
  );

  // Spec 041 FR-051 (T076): "Show ignored items" was removed along with the
  // session review-state machine — sessions no longer have an `ignored`
  // state, so there is nothing left to surface.
  const ALL_ACTIONS: Array<PaletteAction> = [...ACTIONS];

  // All visible pages: standard pages + dev pages when devMode is on.
  const visiblePages = devMode ? [...PAGES, ...DEV_PAGES] : PAGES;

  return (
    <Dialog.Root open={open} onOpenChange={setOpen}>
      <Dialog.Portal>
        <Dialog.Backdrop className="pv-palette-backdrop" />
        <Dialog.Popup
          className="pv-palette"
          aria-label={m.cmdk_aria_label()}
          initialFocus={inputRef}
        >
          <Command shouldFilter={false}>
            <Command.Input
              ref={inputRef}
              className="pv-palette__input"
              placeholder={m.cmdk_placeholder()}
              value={query}
              onValueChange={setQuery}
            />
            <Command.List className="pv-palette__list">
              {query.trim() && results.length === 0 && (
                <Command.Empty className="pv-palette__empty">
                  {m.cmdk_no_results({ query: query.trim() })}
                </Command.Empty>
              )}
              {results.length > 0 && (
                <Command.Group
                  className="pv-palette__group"
                  heading={m.cmdk_group_results()}
                >
                  {results.map((r) => (
                    <Command.Item
                      key={r.id}
                      className="pv-palette__item"
                      onSelect={() => select(r.route)}
                    >
                      <span className="pv-palette__item-kind">{r.kind}</span>
                      <span className="pv-palette__item-label">{r.label}</span>
                      {r.sublabel && (
                        <span className="pv-palette__item-sub">
                          {r.sublabel}
                        </span>
                      )}
                    </Command.Item>
                  ))}
                </Command.Group>
              )}
              <Command.Group
                className="pv-palette__group"
                heading={m.cmdk_group_pages()}
              >
                {visiblePages.map((p) => (
                  <Command.Item
                    key={p.route}
                    className="pv-palette__item"
                    onSelect={() => select(p.route)}
                  >
                    <span className="pv-palette__item-label">{p.label()}</span>
                  </Command.Item>
                ))}
              </Command.Group>
              <Command.Group
                className="pv-palette__group"
                heading={m.cmdk_group_actions()}
              >
                {ALL_ACTIONS.map((a) => (
                  <Command.Item
                    key={a.label()}
                    className="pv-palette__item"
                    onSelect={() => selectAction(a)}
                  >
                    <span className="pv-palette__item-label">{a.label()}</span>
                  </Command.Item>
                ))}
                <Command.Item
                  className="pv-palette__item"
                  onSelect={() => {
                    setOpen(false);
                    setQuery('');
                    void openInNewWindow(currentHref);
                  }}
                >
                  <span className="pv-palette__item-label">
                    {m.cmdk_open_new_window()}
                  </span>
                </Command.Item>
              </Command.Group>
            </Command.List>
          </Command>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
