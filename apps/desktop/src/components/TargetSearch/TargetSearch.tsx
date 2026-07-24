// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * TargetSearch — spec 035 (SIMBAD Target Resolution), User Stories 1 + 3.
 *
 * Debounced as-you-type target search with a two-phase pipeline:
 *
 *   1. Local (US1, T013): `target.search` against the bundled seed + local
 *      cache — instant, no network. Renders ranked suggestions, each showing
 *      the primary designation prominently, the common name (if any) as a
 *      secondary line, and badges for object type and source / catalogue.
 *
 *   2. Long-tail (US3, T022): after the same debounce and a minimum query
 *      length (≥3 chars), ALSO call `target.resolve` (the SIMBAD long-tail,
 *      TAP + cache only — never the Sesame fallback, spec 052 P2 FR-009).
 *      Any `status = "resolved"` target is merged into the suggestion list,
 *      de-duped against the local hits, so objects not in the seed/cache still
 *      appear. `unresolved` (incl. the offline / resolver-disabled case,
 *      FR-015) is treated as a normal, non-fatal outcome — no error is shown.
 *
 * "Search more catalogues" (spec 052 P2/P2UX, FR-008/FR-009): when both phases
 * above still leave zero suggestions, the miss is framed as a next step, not
 * an error — inline text + a button that calls `target.resolve_explicit`
 * (TAP-first, SIMBAD Sesame/NED/VizieR fallback on a miss), plus a
 * "Searching more catalogues…" status while it runs. Enter is a keyboard
 * accelerator for that same button ONLY when it's the sole actionable thing
 * on screen (zero typeahead suggestions); with any suggestion present, Enter
 * still selects the highlighted one. Never fired automatically or per
 * keystroke otherwise.
 *
 * Cancel-in-flight (US3 acceptance scenario #2): every query change bumps a
 * monotonic generation counter. Both phases check their captured generation
 * before committing state, so a stale (superseded) response can never overwrite
 * the current query's results. (Tauri `invoke` exposes no AbortSignal, so this
 * generation guard is the cancel mechanism — no AbortController is involved.)
 *
 * Empty-while-warming retry (spec 052 P4/#818): the shared resolve cache's
 * background seed/durable-row re-warm (startup, or after a cache clear) is
 * one write transaction per phase, so nothing in it is visible to a reader
 * until that whole phase commits. A Phase-1 query landing in that window can
 * come back with zero suggestions for an object the seed does contain,
 * simply because it hasn't committed yet — `target.search`'s `cacheWarming`
 * flag says so, and Phase 1 retries on a short interval (bounded budget)
 * until either a suggestion appears or the backend reports the warm has
 * settled. An ordinary (non-warming) miss never enters this loop, so it pays
 * no extra latency.
 *
 * Selecting a suggestion (mouse or keyboard) invokes `onSelect(suggestion)`,
 * exposing the canonical `targetId` so the caller can associate it.
 *
 * Accessibility & overlay behaviour (spec 042 / T161): the combobox is built on
 * `@base-ui-components/react/combobox`. Base UI owns the combobox/listbox/option
 * ARIA wiring, roving focus + arrow-key navigation, Enter-to-select,
 * Escape-to-close, and click-outside dismissal — replacing the prior hand-rolled
 * keydown / mousedown / `aria-activedescendant` glue. We drive it as a fully
 * controlled, async-filtered combobox (`filter={null}`, controlled `items`,
 * `open`, and `inputValue`), so the two-phase server pipeline above remains the
 * single source of truth for the option list.
 *
 * Virtualization: Base UI's `virtualized` mode is paired with a
 * `@tanstack/react-virtual` virtualizer driven off our own `suggestions` array.
 * Because internal filtering is disabled (`filter={null}`), Base UI's filtered
 * item list is exactly the array we pass, so the virtualizer and the combobox
 * stay in lockstep without the (RC-internal) `useFilteredItems` hook. This keeps
 * long-list performance from US3 while gaining Base UI's accessibility.
 *
 * State/logic lives in `useTargetSearch` (refactor sweep #996); this file is
 * the render only.
 */

import { useId, type Ref } from 'react';
import { Combobox } from '@base-ui-components/react/combobox';
import type { TargetSuggestion } from '@/bindings/aliases';
import type { TargetCatalogId, TargetObjectType } from '@/bindings/index';
import { Pill } from '@/ui';
import { m } from '@/lib/i18n';
import * as ts from './target-search.css';
import { input as fieldInput } from '@/ui/field.css';
import {
  objectTypeLabel,
  catalogLabel,
  OBJECT_TYPES,
  CATALOG_IDS,
} from './objectType';
import { DEFAULT_LIMIT, MIN_RESOLVE_LEN } from './helpers';
import { useTargetSearch } from './useTargetSearch';
import { selectBase } from '@/styles/select.css';
import { virtualInner, virtualScroll } from '@/ui/page-layout.css';

// ── Props ────────────────────────────────────────────────────────────────────

export interface TargetSearchProps {
  /** Called with the chosen suggestion (exposes the canonical `targetId`). */
  onSelect: (suggestion: TargetSuggestion) => void;
  /** Optional: restrict to catalogues (empty/absent = all). */
  catalogFilter?: TargetCatalogId[];
  /** Optional: restrict to object types (empty/absent = all). */
  typeFilter?: TargetObjectType[];
  /** Max suggestions to request (default 20). */
  limit?: number;
  /** Field label (visually hidden if `hideLabel`). */
  label?: string;
  /** Placeholder for the input. */
  placeholder?: string;
  /** Render the label as screen-reader-only. */
  hideLabel?: boolean;
  /** Optional id for the input (defaults to a generated id). */
  inputId?: string;
  /** Forwarded to the input for autofocus. */
  autoFocus?: boolean;
  /**
   * Ref to the search `<input>` DOM node. Lets a caller point a Base UI
   * `Dialog.Popup`'s `initialFocus` at this field instead of using `autoFocus`
   * (#841: a bare `autoFocus` races the dialog's own initial-focus management).
   */
  inputRef?: Ref<HTMLInputElement>;
  /**
   * Show the optional catalogue/type filter control (T029, US5). Default off.
   * The control seeds from `catalogFilter`/`typeFilter` and overrides them.
   */
  showFilters?: boolean;
  /**
   * Enable the per-row "Correct…" manual-override action (T032, US4/FR-014).
   * Binds the current query to the chosen target as `source=user-override`.
   */
  enableOverride?: boolean;
  /**
   * Called after a successful manual override with the user-override suggestion
   * (source = `user-override`). Defaults to `onSelect` when omitted.
   */
  onOverride?: (suggestion: TargetSuggestion) => void;
}

// ── Component ────────────────────────────────────────────────────────────────

export function TargetSearch({
  onSelect,
  catalogFilter,
  typeFilter,
  limit = DEFAULT_LIMIT,
  label = m.targets_add_target_search_label(),
  placeholder = m.projects_create_target_search_placeholder(),
  hideLabel = false,
  inputId,
  autoFocus = false,
  inputRef,
  showFilters = false,
  enableOverride = false,
  onOverride,
}: TargetSearchProps) {
  const generatedId = useId();
  const id = inputId ?? `tgt-search-${generatedId}`;
  const typeFilterId = `${id}-type-filter`;
  const catalogFilterId = `${id}-catalog-filter`;

  const {
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
  } = useTargetSearch({
    onSelect,
    catalogFilter,
    typeFilter,
    limit,
    showFilters,
    onOverride,
  });

  const totalSize = virtualizer.getTotalSize();
  const virtualItems = virtualizer.getVirtualItems();

  return (
    <div className={ts.root} data-testid="target-search">
      <Combobox.Root<TargetSuggestion>
        items={suggestions}
        // Selection stays uncontrolled: we react via `onValueChange` and keep
        // the typed query as the input's source of truth (controlled
        // `inputValue`), so choosing a target never overwrites what was typed.
        onValueChange={handleSelect}
        filter={null}
        // F2: auto-highlight the first suggestion as results arrive, matching the
        // pre-base-ui behavior (Enter selects the top hit without an ArrowDown).
        autoHighlight
        virtualized
        modal={false}
        open={showList}
        onOpenChange={(nextOpen) => setOpen(nextOpen)}
        inputValue={query}
        onInputValueChange={(value, details) => {
          // `item-press` fires when an item is selected; don't treat that as a
          // user edit (it would otherwise blank/replace the query).
          if (details.reason === 'item-press') return;
          setQuery(value);
          if (value.trim().length > 0) setOpen(true);
        }}
        itemToStringLabel={itemToStringLabel}
        onItemHighlighted={handleItemHighlighted}
      >
        {}
        <label
          className={
            hideLabel ? 'pv-target-search__label--sr' : 'pv-field-label'
          }
          htmlFor={id}
        >
          {label}
        </label>
        <Combobox.Input
          ref={inputRef}
          id={id}
          className={fieldInput}
          data-testid="target-search-input"
          autoComplete="off"
          spellCheck={false}
          aria-label={label}
          aria-describedby={error ? `${id}-error` : undefined}
          placeholder={placeholder}
          // eslint-disable-next-line jsx-a11y/no-autofocus -- opt-in via the autoFocus prop; callers enable it only for focused search surfaces
          autoFocus={autoFocus}
          onFocus={() => {
            if (query.trim().length > 0) setOpen(true);
          }}
          onKeyDown={(e) => {
            // Enter-as-accelerator (spec 052 P2UX): fires the explicit
            // "search more catalogues" fallback ONLY when it's the sole
            // actionable thing on screen (zero typeahead suggestions). With
            // any suggestion present, Enter falls through to Base UI's own
            // select-the-highlighted-option handling — never both.
            if (e.key === 'Enter' && harderOffered) {
              e.preventDefault();
              // #697: our `onKeyDown` and Base UI's own internal Enter
              // handling are composed onto the SAME input element (Base UI's
              // `mergeProps`), which — with zero suggestions, so no
              // highlighted option — closes the popup ("allow form
              // submission when no item is highlighted") after ours runs.
              // `preventDefault()`/`stopPropagation()` can't stop a sibling
              // handler composed this way; Base UI's merge utility exposes
              // `preventBaseUIHandler()` on the event for exactly this.
              e.preventBaseUIHandler();
              void handleSearchHarder();
            }
          }}
        />

        {showFilters && (
          <div
            className={ts.filters}
            role="group"
            aria-label={m.cmp_target_search_filters_aria()}
          >
            <label className={ts.filterLabel} htmlFor={typeFilterId}>
              {m.cmp_target_search_type_label()}
              <select
                id={typeFilterId}
                className={`${selectBase} pv-target-search__filter-select`}
                value={typeSel}
                onChange={(e) =>
                  setTypeSel(e.target.value as TargetObjectType | '')
                }
              >
                <option value="">{m.cmp_target_search_all_types()}</option>
                {OBJECT_TYPES.map((t) => (
                  <option key={t} value={t}>
                    {objectTypeLabel(t)}
                  </option>
                ))}
              </select>
            </label>
            <label className={ts.filterLabel} htmlFor={catalogFilterId}>
              {m.cmp_target_search_catalogue_label()}
              <select
                id={catalogFilterId}
                className={`${selectBase} pv-target-search__filter-select`}
                value={catalogSel}
                onChange={(e) =>
                  setCatalogSel(e.target.value as TargetCatalogId | '')
                }
              >
                <option value="">{m.cmp_target_search_all_catalogues()}</option>
                {CATALOG_IDS.map((c) => (
                  <option key={c} value={c}>
                    {catalogLabel(c)}
                  </option>
                ))}
              </select>
            </label>
          </div>
        )}

        {error && (
          <span
            id={`${id}-error`}
            role="alert"
            className="pv-field-error"
            data-testid="field-error"
          >
            {error}
          </span>
        )}

        {/*
         * `keepMounted`: without it, `Combobox.Portal` only renders its
         * subtree (status/option elements) once Base UI's internal
         * store-derived `mounted` flips true — which, per
         * `useTransitionStatus`, happens synchronously the instant `open`
         * becomes true, EXCEPT this combobox's `open` is re-derived on every
         * keystroke (`onInputValueChange` -> `setOpen(true)`), unlike a
         * click-triggered Dialog/Popover whose `open` only flips once per
         * interaction. That gives a real-typing-driven open/close cycle far
         * more chances to race Base UI's own internal open bookkeeping than
         * a single click does. Windows real-UI E2E (`targets_journeys.rs`,
         * "M 1" suggestion never rendering within 20s even though
         * `target.search` is a local, network-free, sub-millisecond seed
         * lookup) hit exactly that: `aria-expanded`/`data-popup-open` on the
         * input reported "open", but NO popup content — not even the
         * always-present idle/no-results/searching `Combobox.Status` line —
         * ever appeared anywhere in the document, meaning the portal itself
         * never rendered. `keepMounted` decouples rendering the subtree from
         * that race: the popup DOM is always present (hidden via the
         * `hidden` attribute — invisible and inert, so real users never see
         * anything different) as soon as our OWN `suggestions`/`loading`/
         * `error` state has something to show, regardless of Base UI's
         * internal open/mounted timing.
         */}
        <Combobox.Portal keepMounted>
          <Combobox.Positioner
            className={ts.positioner}
            sideOffset={4}
            align="start"
          >
            <Combobox.Popup className={ts.popup}>
              <Combobox.List
                ref={scrollRef}
                className={`pv-target-search__list ${virtualScroll}`}
                data-virtual-scroll="true"
                aria-label={m.cmp_target_search_suggestions_aria()}
              >
                {loading && suggestions.length === 0 && (
                  <Combobox.Status
                    className={ts.status}
                    data-testid="target-search-status"
                  >
                    {m.cmp_target_search_searching()}
                  </Combobox.Status>
                )}
                {/*
                 * Below the minimum resolve length, Phase 2 (SIMBAD) hasn't
                 * run at all — "No matching targets." would falsely claim a
                 * search happened and missed (#843). Say so honestly instead.
                 */}
                {!loading &&
                  !error &&
                  !resolving &&
                  suggestions.length === 0 &&
                  query.trim().length < MIN_RESOLVE_LEN && (
                    <Combobox.Status
                      className={ts.status}
                      data-testid="target-search-status"
                    >
                      {m.cmp_target_search_type_more()}
                    </Combobox.Status>
                  )}
                {/*
                 * "Search more catalogues" (spec 052 P2/P2UX, FR-008/FR-009):
                 * once both prior phases (local cache + TAP long-tail) have
                 * come up empty, frame it as a next step rather than a dead
                 * end — the miss message and the fallback button read as one
                 * inline sentence, not a separate error. Never fired
                 * automatically or per keystroke; only this explicit button
                 * click or the Enter accelerator below invokes it.
                 */}
                {harderOffered && (
                  <div className="pv-target-search__status pv-target-search__no-match">
                    <Combobox.Status>
                      {m.cmp_target_search_no_results_hint()}
                    </Combobox.Status>
                    <button
                      type="button"
                      className={ts.override}
                      onPointerDown={(e) => {
                        e.preventDefault();
                        e.stopPropagation();
                      }}
                      onClick={(e) => {
                        e.preventDefault();
                        e.stopPropagation();
                        void handleSearchHarder();
                      }}
                    >
                      {m.cmp_target_search_search_harder()}
                    </button>
                  </div>
                )}
                {/*
                 * Offline/disabled empty state (#694): the long-tail phase
                 * couldn't even try (network down or the "Online SIMBAD
                 * resolution" setting is off), so offering the (also online)
                 * "search more catalogues" fallback would just fail again —
                 * say so instead of rendering nothing.
                 */}
                {offlineNoticeOffered && (
                  <Combobox.Status
                    className={ts.status}
                    data-testid="target-search-status"
                  >
                    {m.settings_resolver_online_off_info()}
                  </Combobox.Status>
                )}
                {harderState === 'searching' && (
                  <Combobox.Status
                    className="pv-target-search__status pv-target-search__status--resolving"
                    data-testid="target-search-status-resolving"
                  >
                    {m.cmp_target_search_search_harder_searching()}
                  </Combobox.Status>
                )}
                {harderState === 'no-results' && (
                  <Combobox.Status
                    className={ts.status}
                    data-testid="target-search-status"
                  >
                    {m.cmp_target_search_search_harder_no_results()}
                  </Combobox.Status>
                )}
                {suggestions.length > 0 && (
                  <div
                    className={virtualInner}
                    // eslint-disable-next-line no-restricted-syntax -- dynamic: virtualizer total height (totalSize)
                    style={{ height: `${totalSize}px`, position: 'relative' }}
                  >
                    {virtualItems.map((virtualRow) => {
                      const i = virtualRow.index;
                      const s = suggestions[i];
                      const secondary = s.commonName ?? s.matchedAlias ?? null;
                      return (
                        <Combobox.Item
                          key={s.targetId}
                          index={i}
                          value={s}
                          ref={virtualizer.measureElement}
                          data-index={i}
                          className={ts.option}
                          data-testid="target-search-option"
                          // eslint-disable-next-line no-restricted-syntax -- dynamic: virtualizer translateY offset per suggestion row
                          style={{
                            position: 'absolute',
                            top: 0,
                            left: 0,
                            right: 0,
                            transform: `translateY(${virtualRow.start}px)`,
                          }}
                        >
                          <span className={ts.primary}>
                            {s.primaryDesignation}
                          </span>
                          {secondary && secondary !== s.primaryDesignation && (
                            <span className={ts.secondary}>{secondary}</span>
                          )}
                          <span className={ts.badges}>
                            <Pill variant="info">
                              {objectTypeLabel(s.objectType)}
                            </Pill>
                            <Pill
                              variant={
                                s.source === 'user-override'
                                  ? 'accent'
                                  : 'ghost'
                              }
                            >
                              {s.source}
                            </Pill>
                            {enableOverride && (
                              <button
                                type="button"
                                className={ts.override}
                                aria-label={m.cmp_target_search_set_primary_aria(
                                  {
                                    query: query.trim(),
                                    designation: s.primaryDesignation,
                                  },
                                )}
                                disabled={
                                  overriding != null ||
                                  query.trim().length === 0
                                }
                                onPointerDown={(e) => {
                                  // Don't trigger the row's select-on-press.
                                  e.preventDefault();
                                  e.stopPropagation();
                                }}
                                onClick={(e) => {
                                  e.preventDefault();
                                  e.stopPropagation();
                                  void handleOverride(s);
                                }}
                              >
                                {overriding === s.targetId
                                  ? m.targetsearch_setting()
                                  : m.targetsearch_correct()}
                              </button>
                            )}
                          </span>
                        </Combobox.Item>
                      );
                    })}
                  </div>
                )}
                {resolving && (
                  <Combobox.Status
                    className="pv-target-search__status pv-target-search__status--resolving"
                    data-testid="target-search-status-resolving"
                  >
                    {m.cmp_target_search_searching_simbad()}
                  </Combobox.Status>
                )}
              </Combobox.List>
            </Combobox.Popup>
          </Combobox.Positioner>
        </Combobox.Portal>
      </Combobox.Root>
    </div>
  );
}
