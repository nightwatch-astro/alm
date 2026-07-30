// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * `useTargetSearch` — all stateful logic behind `TargetSearch` (spec 035 US1
 * + US3): the two-phase local/SIMBAD search pipeline, cancel-in-flight
 * generation guard, "search more catalogues" fallback, manual override, and
 * the virtualizer driving the suggestion list. Split out of the component
 * (refactor sweep #996) so `TargetSearch.tsx` stays render-only.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { useDebouncedCallback } from 'use-debounce';
import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import type { TargetSuggestion } from '@/bindings/aliases';
import type { TargetCatalogId, TargetObjectType } from '@/bindings/index';
import { m } from '@/lib/i18n';
import {
  TARGET_SEARCH_CONTRACT_VERSION,
  DEBOUNCE_MS,
  MIN_RESOLVE_LEN,
  OPTION_ESTIMATE,
  WARM_RETRY_INTERVAL_MS,
  WARM_RETRY_BUDGET_MS,
  sleep,
  resolvedToSuggestion,
  mergeDedupe,
} from './helpers';
import type { TargetSearchProps } from './TargetSearch';

export function useTargetSearch({
  onSelect,
  catalogFilter,
  typeFilter,
  limit,
  showFilters = false,
  onOverride,
}: Pick<
  TargetSearchProps,
  | 'onSelect'
  | 'catalogFilter'
  | 'typeFilter'
  | 'limit'
  | 'showFilters'
  | 'onOverride'
> & { limit: number }) {
  // Optional filter state (T029). Seeds from props; "all" = no filter.
  const [typeSel, setTypeSel] = useState<TargetObjectType | ''>(
    typeFilter && typeFilter.length === 1 ? typeFilter[0] : '',
  );
  const [catalogSel, setCatalogSel] = useState<TargetCatalogId | ''>(
    catalogFilter && catalogFilter.length === 1 ? catalogFilter[0] : '',
  );
  // Effective filters sent to the backend (internal control wins when shown).
  const effectiveTypeFilter = showFilters
    ? typeSel
      ? [typeSel]
      : undefined
    : typeFilter;
  const effectiveCatalogFilter = showFilters
    ? catalogSel
      ? [catalogSel]
      : undefined
    : catalogFilter;
  // Stable string keys so the search callback only re-creates on real changes.
  const typeFilterKey = (effectiveTypeFilter ?? []).join(',');
  const catalogFilterKey = (effectiveCatalogFilter ?? []).join(',');

  const [query, setQuery] = useState('');
  const [suggestions, setSuggestions] = useState<TargetSuggestion[]>([]);
  const [loading, setLoading] = useState(false);
  const [resolving, setResolving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  // "Search more catalogues" (spec 052 P2): idle until the user explicitly
  // triggers the Sesame/NED/VizieR fallback; reset on every new query so a
  // fresh search always starts idle again.
  const [harderState, setHarderState] = useState<
    'idle' | 'searching' | 'no-results'
  >('idle');
  // Whether the long-tail SIMBAD phase last reported "offline" — network
  // unreachable OR online resolution disabled by settings (FR-015; both map
  // onto `unresolvedReason = "offline"` in the backend contract). When true,
  // the "search more catalogues" fallback (itself online-only) is suppressed
  // in favour of a plain explanation (#694): otherwise the empty state
  // renders nothing the user can act on.
  const [resolveOffline, setResolveOffline] = useState(false);

  // Cancel-in-flight: a monotonic generation counter. Each query bumps `gen`;
  // only the latest generation may commit results, so a slow response from a
  // superseded query is dropped. (Tauri `invoke` has no AbortSignal, so this
  // generation guard — not an AbortController — is the actual cancel mechanism.)
  const genRef = useRef(0);
  const scrollRef = useRef<HTMLDivElement>(null);

  // Virtualize the suggestion options. Because Base UI internal filtering is
  // disabled (`filter={null}`), the combobox's filtered list equals our own
  // `suggestions` array, so the virtualizer can be driven directly from it.
  const virtualizer = useVirtualizer({
    count: suggestions.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => OPTION_ESTIMATE,
    overscan: 6,
  });

  const runSearch = useCallback(
    // biome-ignore lint/complexity/noExcessiveCognitiveComplexity: search input handler dispatching over query shape and resolver outcome
    async (raw: string) => {
      const trimmed = raw.trim();

      // Supersede any in-flight pipeline by bumping the generation.
      const gen = ++genRef.current;
      const isCurrent = () => gen === genRef.current;
      // A new query always starts the "search more catalogues" affordance idle.
      setHarderState('idle');
      setResolveOffline(false);

      if (!trimmed) {
        setSuggestions([]);
        setLoading(false);
        setResolving(false);
        setError(null);
        return;
      }

      setLoading(true);
      setError(null);

      // ── Phase 1: local seed + cache (instant) ──────────────────────────────
      try {
        const doSearch = async () =>
          unwrap(
            await commands.targetSearch({
              contractVersion: TARGET_SEARCH_CONTRACT_VERSION,
              requestId: crypto.randomUUID(),
              query: trimmed,
              catalogFilter: effectiveCatalogFilter,
              typeFilter: effectiveTypeFilter,
              limit,
            }),
          );
        let res = await doSearch();
        // #818: an empty result while the backend is still warming the
        // shared resolve cache isn't necessarily the settled answer — retry
        // until a suggestion shows up, the backend reports the warm has
        // finished, or the budget runs out. A no-warm miss (the overwhelming
        // common case) never enters this loop.
        const retryDeadline = Date.now() + WARM_RETRY_BUDGET_MS;
        while (
          isCurrent() &&
          res.cacheWarming &&
          res.suggestions.length === 0 &&
          Date.now() < retryDeadline
        ) {
          await sleep(WARM_RETRY_INTERVAL_MS);
          if (!isCurrent()) return;
          res = await doSearch();
        }
        if (!isCurrent()) return; // superseded — drop stale result
        setSuggestions(res.suggestions);
      } catch {
        if (!isCurrent()) return;
        setError(m.targetsearch_search_failed());
        setSuggestions([]);
        setLoading(false);
        return;
      } finally {
        if (isCurrent()) setLoading(false);
      }

      // ── Phase 2: SIMBAD long-tail (debounced, min length) ──────────────────
      if (trimmed.length < MIN_RESOLVE_LEN) return;

      setResolving(true);
      try {
        const res = unwrap(
          await commands.targetResolve({
            contractVersion: TARGET_SEARCH_CONTRACT_VERSION,
            requestId: crypto.randomUUID(),
            query: trimmed,
            override: null,
          }),
        );
        // Cancel-in-flight guard: a newer query must not be overwritten.
        if (!isCurrent()) return;
        if (res.status === 'resolved' && res.target) {
          // Merge against the current list (the Phase-1 local hits for this
          // generation) so the long-tail row is appended, never duplicated.
          const resolved = res.target;
          setSuggestions((prev) =>
            mergeDedupe(prev, resolvedToSuggestion(resolved)),
          );
        } else {
          // `unresolved` (unknown / offline / resolver-disabled) is non-fatal:
          // leave the local hits untouched and surface no error (FR-011/FR-015).
          // Track the "offline" reason specifically (#694) so the empty state
          // can explain itself instead of rendering nothing.
          setResolveOffline(res.unresolvedReason === 'offline');
        }
      } catch {
        // Network/internal resolve failure is non-fatal for the typeahead;
        // the local hits already render. Swallow to avoid error spam, but
        // still treat it as an "offline" outcome for the empty-state message.
        if (!isCurrent()) return;
        setResolveOffline(true);
      } finally {
        if (isCurrent()) setResolving(false);
      }
    },
    // Re-create when the effective filters change. `*FilterKey` are stable
    // string keys derived from the filter arrays; the arrays themselves are
    // read inside the callback (intentionally not listed).
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [catalogFilterKey, typeFilterKey, limit],
  );

  // Debounce query changes. `useDebouncedCallback` cancels any pending call on
  // unmount and whenever a new invocation is scheduled, preserving the prior
  // hand-rolled setTimeout/clearTimeout semantics at the same DEBOUNCE_MS.
  const debouncedSearch = useDebouncedCallback(
    (q: string) => void runSearch(q),
    DEBOUNCE_MS,
  );
  useEffect(() => {
    debouncedSearch(query);
    return () => debouncedSearch.cancel();
  }, [query, debouncedSearch]);

  // "Search more catalogues" (spec 052 P2, FR-008/FR-009): the deliberate
  // resolve action the Sesame/NED/VizieR fallback is gated on — a click, not
  // a keystroke. Shares the same generation guard as `runSearch` so a query
  // change while this is in flight drops its (now-stale) result.
  const handleSearchHarder = useCallback(async () => {
    const trimmed = query.trim();
    if (!trimmed) return;
    const gen = genRef.current;
    setHarderState('searching');
    try {
      const res = unwrap(
        await commands.targetResolveExplicit({
          contractVersion: TARGET_SEARCH_CONTRACT_VERSION,
          requestId: crypto.randomUUID(),
          query: trimmed,
          override: null,
        }),
      );
      if (gen !== genRef.current) return; // superseded by a newer query
      if (res.status === 'resolved' && res.target) {
        const resolved = res.target;
        setSuggestions((prev) =>
          mergeDedupe(prev, resolvedToSuggestion(resolved)),
        );
        setHarderState('idle');
      } else {
        setHarderState('no-results');
      }
    } catch {
      if (gen !== genRef.current) return;
      setError(m.targetsearch_search_failed());
      setHarderState('idle');
    }
  }, [query]);

  const handleSelect = useCallback(
    (s: TargetSuggestion | null) => {
      if (!s) return;
      onSelect(s);
      setOpen(false);
    },
    [onSelect],
  );

  // Manual override (T032, FR-014): bind the current query to the chosen target
  // as `source=user-override`. Persisted server-side and wins over future
  // SIMBAD/seed resolutions for that query.
  const [overriding, setOverriding] = useState<string | null>(null);
  const handleOverride = useCallback(
    async (s: TargetSuggestion) => {
      const trimmed = query.trim();
      if (!trimmed) return;
      setOverriding(s.targetId);
      setError(null);
      try {
        const res = unwrap(
          await commands.targetResolve({
            contractVersion: TARGET_SEARCH_CONTRACT_VERSION,
            requestId: crypto.randomUUID(),
            query: trimmed,
            override: { targetId: s.targetId },
          }),
        );
        const result: TargetSuggestion =
          res.status === 'resolved' && res.target
            ? resolvedToSuggestion(res.target)
            : { ...s, source: 'user-override' };
        (onOverride ?? onSelect)(result);
        setOpen(false);
      } catch {
        setError(m.targetsearch_set_failed());
      } finally {
        setOverriding(null);
      }
    },
    [query, onOverride, onSelect],
  );

  // "Search more catalogues" (spec 052 P2UX): true when the button is the only
  // actionable next step — both prior phases came up empty and the fallback
  // hasn't already been fired/exhausted for this query. Suppressed when the
  // long-tail phase reported "offline" (#694): that fallback is itself
  // online-only, so offering it would just fail again — the offline info
  // line below takes over instead.
  const harderOffered =
    !loading &&
    !error &&
    !resolving &&
    suggestions.length === 0 &&
    harderState === 'idle' &&
    !resolveOffline &&
    query.trim().length >= MIN_RESOLVE_LEN;

  // The offline/disabled empty-state info (#694): same gating as
  // `harderOffered` above, but for the case it excludes.
  const offlineNoticeOffered =
    !loading &&
    !error &&
    !resolving &&
    suggestions.length === 0 &&
    harderState === 'idle' &&
    resolveOffline &&
    query.trim().length >= MIN_RESOLVE_LEN;

  // Keep the popup open while the "search more catalogues" status text is
  // live (#697): Base UI's own Enter handling can otherwise flip `open` back
  // to false via `onOpenChange`, unmounting the status the user just
  // triggered even though our own state (`suggestions`/`harderState`) still
  // has something to show.
  const showList =
    (open || harderState === 'searching' || harderState === 'no-results') &&
    query.trim().length > 0;

  // Keep the highlighted option mounted + visible during keyboard navigation.
  // The virtualizer only mounts the visible window, so an off-screen highlighted
  // option must be scrolled into view as Base UI moves the active index.
  const handleItemHighlighted = useCallback(
    (_value: TargetSuggestion | undefined, details: { index: number }) => {
      if (details.index >= 0 && details.index < suggestions.length) {
        virtualizer.scrollToIndex(details.index, { align: 'auto' });
      }
    },
    [suggestions.length, virtualizer],
  );

  // Base UI uses `value` for selection. We keep the input independent of
  // selection (selecting a target should not stuff its label into the box), so
  // `value` stays null and we react via `onValueChange`.
  const itemToStringLabel = useMemo(
    () => (s: TargetSuggestion) => s.primaryDesignation,
    [],
  );

  return {
    typeSel,
    setTypeSel,
    catalogSel,
    setCatalogSel,
    query,
    setQuery,
    suggestions,
    loading,
    resolving,
    error,
    open,
    setOpen,
    harderState,
    scrollRef,
    virtualizer,
    handleSearchHarder,
    handleSelect,
    handleOverride,
    overriding,
    harderOffered,
    offlineNoticeOffered,
    showList,
    handleItemHighlighted,
    itemToStringLabel,
  };
}
