// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * The ONE parameterised Getting-started checklist (spec 056, US2 T017–T019;
 * research R10). Its sole host is the flyout in `ChecklistPopover.tsx`, which
 * `Sidebar.tsx` mounts at BOTH sidebar widths — only the trigger differs
 * (labelled row when expanded, bare progress ring when icon-collapsed).
 *
 * T018 originally specified an inline section in the expanded sidebar, with the
 * popover reserved for the collapsed width. That was dropped: rendered inline,
 * the list blended into the sidebar's own surface and read as navigation rather
 * than as a checklist. Both widths now use the flyout, which is portalled to
 * `<body>` because `.pv-sidebar` is `overflow: hidden` and was scissoring the
 * panel away. There is consequently NO inline host — anything asserting on this
 * component (tests included) must open the flyout first.
 *
 * One component, one `checklist.css` class family (tokens only) — never a
 * per-surface clone (`scripts/css-dup-sniff.mjs`).
 *
 * State is read straight from the onboarding store (backend-authoritative,
 * research R5): the component re-renders whenever the projection changes, so
 * prerequisite satisfaction clears live without a reload (FR-010) and auto-ticks
 * surface as soon as the store re-reads on `onboarding:state-changed`.
 *
 * Completion choreography (animation, completed-area move) is layered on by US3
 * T024/T025; this node renders the settled end-state (open items on top,
 * completed greyed at the bottom of their group) plus the manual check and
 * dismiss affordances required for non-event items (FR-017).
 */

import { useState } from 'react';
import { useNavigate, useRouterState } from '@tanstack/react-router';
import { ChevronDown, ChevronRight, MoreHorizontal } from 'lucide-react';
import { clsx } from 'clsx';
import { m } from '@/lib/i18n';
import type { OnboardingPage } from '@/bindings/index';
import './checklist.css';
import { setOnboardingSection, useVisibleOnboardingState } from './store';
// Re-export so existing imports from ChecklistSection still resolve.
export { useVisibleOnboardingState } from './store';
import { useCompletionChoreography } from './choreography';
import { FindSpotlight, clearFind, useActiveFindItem } from './FindSpotlight';
import { PAGE_ORDER, itemLabel, pagePath } from './onboarding-labels';
import { useChecklistGroups } from './useChecklistGroups';
import { ChecklistGroup } from './ChecklistGroup';

// Re-export the label/path helpers so existing callers (FindSpotlight, tests)
// that import these from ChecklistSection still resolve.
export {
  ONBOARDING_PAGE_PATHS,
  itemLabel,
  itemTooltip,
  prerequisiteReason,
} from './onboarding-labels';

// Re-export item helpers and ChecklistItemRow so existing callers that import
// them from ChecklistSection continue to resolve (backward-compat re-exports).
export {
  isChecklistGroupSettled,
  completedChecklistItems,
} from './checklist-item-helpers';
export { ChecklistItemRow } from './ChecklistItemRow';
export type { ChecklistItemRowProps } from './ChecklistItemRow';

function pageForPath(pathname: string): OnboardingPage | null {
  return PAGE_ORDER.find((p) => pathname.startsWith(pagePath(p))) ?? null;
}

interface ChecklistSectionProps {
  /** Disambiguates DOM ids when two hosts could ever mount at once. */
  idPrefix?: string;
}

/** The shared checklist body: overall progress line + per-page accordion. */
export function ChecklistSection({
  idPrefix = 'onb',
}: ChecklistSectionProps): React.ReactElement | null {
  const state = useVisibleOnboardingState();
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const navigate = useNavigate();
  const choreo = useCompletionChoreography(state);
  // While a find spotlight is up, keep the checklist (and its toggle-off find
  // affordance) above the joyride overlay so the FR-023 toggle-dismiss stays
  // clickable — the overlay still dims the rest of the app.
  const spotlightActive = useActiveFindItem() != null;

  // Section-header remove menu (T029): menu → one-line confirm → hide forever.
  const [menuOpen, setMenuOpen] = useState(false);
  const [removeConfirming, setRemoveConfirming] = useState(false);

  const currentPage = pageForPath(pathname);
  const {
    itemsByPage,
    isGroupComplete,
    isGroupExpanded,
    groupProgress,
    toggleGroup,
  } = useChecklistGroups(state, currentPage);

  if (!state) return null;

  const sectionExpanded = !state.flags.sidebarCollapsed;
  const { done, total } = state.progress;
  const progressText = m.onboarding_section_progress({ done, total });

  const toggleSection = () =>
    void setOnboardingSection({ sidebarCollapsed: sectionExpanded });

  // Explicit removal (FR-013): hide the section (and its ring) forever, and
  // drop any spotlight that was open over a now-hidden item (spec edge case).
  const handleRemove = () => {
    clearFind();
    setMenuOpen(false);
    setRemoveConfirming(false);
    void setOnboardingSection({ hidden: true });
  };

  return (
    <section
      className={clsx(
        'pv-onb-checklist',
        spotlightActive && 'pv-onb-checklist--spotlighting',
      )}
      data-testid="onb-checklist"
      aria-label={m.onboarding_section_title()}
    >
      <div className="pv-onb-checklist__head">
        <button
          type="button"
          className="pv-onb-checklist__section-toggle"
          data-testid="onb-checklist-section-toggle"
          aria-expanded={sectionExpanded}
          onClick={toggleSection}
        >
          {sectionExpanded ? (
            <ChevronDown size={14} aria-hidden />
          ) : (
            <ChevronRight size={14} aria-hidden />
          )}
          <span className="pv-onb-checklist__title">
            {m.onboarding_section_title()}
          </span>
        </button>
        <div
          className={clsx(
            'pv-onb-checklist__progress',
            choreo.pulseActive && 'pv-onb-checklist__progress--pulse',
          )}
          data-testid="onb-checklist-progress"
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={total}
          aria-valuenow={done}
          aria-label={progressText}
        >
          <span className="pv-onb-checklist__progress-text">
            {progressText}
          </span>
          <span className="pv-onb-checklist__progress-track" aria-hidden>
            <span
              className="pv-onb-checklist__progress-fill"
              // eslint-disable-next-line no-restricted-syntax -- dynamic: progress width tracks live done/total
              style={{ width: `${total > 0 ? (done / total) * 100 : 0}%` }}
            />
          </span>
        </div>

        {/* Header overflow menu (T029): the single "Remove getting started"
            action behind a one-line confirm (FR-013). */}
        <div className="pv-onb-checklist__menu-wrap">
          <button
            type="button"
            className="pv-onb-checklist__menu-btn"
            aria-label={m.onboarding_section_menu_aria()}
            aria-expanded={menuOpen}
            aria-haspopup="menu"
            onClick={() => {
              setMenuOpen((v) => !v);
              setRemoveConfirming(false);
            }}
          >
            <MoreHorizontal size={14} aria-hidden />
          </button>
          {menuOpen && (
            <div className="pv-onb-checklist__menu" role="menu">
              {removeConfirming ? (
                <div className="pv-onb-checklist__menu-confirm">
                  <p
                    className="pv-onb-checklist__menu-confirm-text"
                    data-testid="onb-checklist-menu-confirm-text"
                  >
                    {m.onboarding_section_remove_confirm()}
                  </p>
                  <div className="pv-onb-checklist__menu-confirm-actions">
                    <button
                      type="button"
                      className="pv-onb-checklist__menu-confirm-yes"
                      data-testid="onb-checklist-menu-confirm-yes"
                      onClick={handleRemove}
                    >
                      {m.common_remove()}
                    </button>
                    <button
                      type="button"
                      className="pv-onb-checklist__menu-confirm-no"
                      data-testid="onb-checklist-menu-confirm-no"
                      onClick={() => {
                        setRemoveConfirming(false);
                        setMenuOpen(false);
                      }}
                    >
                      {m.common_cancel()}
                    </button>
                  </div>
                </div>
              ) : (
                <button
                  type="button"
                  role="menuitem"
                  className="pv-onb-checklist__menu-item"
                  onClick={() => setRemoveConfirming(true)}
                >
                  {m.onboarding_section_menu_remove()}
                </button>
              )}
            </div>
          )}
        </div>
      </div>

      {/* Polite per-tick announcement (T024, WCAG). Always rendered so AT reads
          each completion; text changes when the choreography detector reports a
          fresh unchecked→settled transition. */}
      <div className="pv-visually-hidden" role="status" aria-live="polite">
        {choreo.announceItemId
          ? m.onboarding_announcer_tick({
              item: itemLabel(choreo.announceItemId),
            })
          : ''}
      </div>

      {sectionExpanded && (
        <div
          className="pv-onb-checklist__groups"
          data-testid="onb-checklist-groups"
        >
          {PAGE_ORDER.filter((page) => itemsByPage.has(page)).map((page) => {
            const items = itemsByPage.get(page) ?? [];
            const completingHere = items.some((i) =>
              choreo.completingIds.has(i.itemId),
            );
            // Keep the group open through the choreography before FR-031
            // collapses it to its one-line done header (AS-6).
            const expanded = isGroupExpanded(page) || completingHere;
            return (
              <ChecklistGroup
                key={page}
                page={page}
                items={items}
                completingIds={choreo.completingIds}
                expanded={expanded}
                complete={isGroupComplete(page)}
                groupProgress={groupProgress(page)}
                idPrefix={idPrefix}
                onToggle={() => toggleGroup(page)}
                onJump={(jumpPage) => void navigate({ to: pagePath(jumpPage) })}
              />
            );
          })}
        </div>
      )}

      {/* The single find-it spotlight for this (single) mounted checklist. */}
      <FindSpotlight />
    </section>
  );
}
