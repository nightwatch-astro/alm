// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * ChecklistGroup — one per-page accordion group in the onboarding checklist
 * (spec 056, FR-007/FR-031/T024).
 *
 * Extracted from ChecklistSection.tsx (refactor sweep kyo7.104). Renders the
 * group header button (label + progress count + done checkmark), the open
 * (unchecked) item list via ChecklistItemRow, and the completed sub-list.
 * Expansion state and the toggle callback come from the parent via
 * useChecklistGroups.
 */

import { Check, ChevronDown, ChevronRight } from 'lucide-react';
import { clsx } from 'clsx';
import type { OnboardingItemDto, OnboardingPage } from '@/bindings/index';
import { setOnboardingItemState } from './store';
import { ChecklistItemRow } from './ChecklistItemRow';
import { completedChecklistItems } from './checklist-item-helpers';
import { itemLabel, pageLabel } from './onboarding-labels';

interface ChecklistGroupProps {
  page: OnboardingPage;
  items: OnboardingItemDto[];
  /** Item ids currently playing the completion choreography (T024). */
  completingIds: ReadonlySet<string>;
  expanded: boolean;
  complete: boolean;
  groupProgress: { done: number; total: number };
  idPrefix: string;
  onToggle: () => void;
  onJump: (page: OnboardingPage) => void;
}

/**
 * One per-page accordion group: header button (with progress counts and
 * done checkmark) and — when expanded — the open + completed item lists.
 */
export function ChecklistGroup({
  page,
  items,
  completingIds,
  expanded,
  complete,
  groupProgress,
  idPrefix,
  onToggle,
  onJump,
}: ChecklistGroupProps) {
  // A settling item stays in the OPEN list (animating in place) until
  // its completing window ends, then drops to the completed area.
  const open = items.filter(
    (i) => i.state === 'unchecked' || completingIds.has(i.itemId),
  );
  const completed = completedChecklistItems(items).filter(
    (i) => !completingIds.has(i.itemId),
  );

  return (
    <div
      className={clsx(
        'pv-onb-checklist__group',
        complete && 'pv-onb-checklist__group--complete',
      )}
      data-testid="onb-checklist-group"
    >
      <button
        type="button"
        className="pv-onb-checklist__group-header"
        data-testid="onb-checklist-group-header"
        aria-expanded={expanded}
        onClick={onToggle}
      >
        {expanded ? (
          <ChevronDown size={13} aria-hidden />
        ) : (
          <ChevronRight size={13} aria-hidden />
        )}
        <span className="pv-onb-checklist__group-label">{pageLabel(page)}</span>
        {complete && (
          <Check
            size={13}
            aria-hidden
            className="pv-onb-checklist__group-done"
            data-testid="onb-checklist-group-done"
          />
        )}
        <span className="pv-onb-checklist__group-count">
          {groupProgress.done}/{groupProgress.total}
        </span>
      </button>

      {expanded && (
        <ul className="pv-onb-checklist__items">
          {open.map((item) => (
            <ChecklistItemRow
              key={item.itemId}
              item={item}
              idPrefix={idPrefix}
              completing={completingIds.has(item.itemId)}
              onJump={onJump}
            />
          ))}
          {completed.length > 0 && (
            <li
              className="pv-onb-checklist__completed"
              data-testid="onb-checklist-completed"
            >
              <ul className="pv-onb-checklist__items">
                {completed.map((item) => {
                  const doneLabelId = `${idPrefix}-done-${item.itemId.replaceAll('.', '_')}`;
                  return (
                    <li
                      key={item.itemId}
                      className="pv-onb-checklist__item pv-onb-checklist__item--done"
                      data-item-id={item.itemId}
                      data-state={item.state}
                    >
                      <button
                        type="button"
                        role="checkbox"
                        aria-checked
                        aria-labelledby={doneLabelId}
                        className="pv-onb-checklist__check pv-onb-checklist__check--done"
                        onClick={() =>
                          void setOnboardingItemState(item.itemId, 'unchecked')
                        }
                      >
                        <Check
                          size={14}
                          aria-hidden
                          className="pv-onb-checklist__check-icon"
                        />
                      </button>
                      <span
                        id={doneLabelId}
                        className="pv-onb-checklist__item-label"
                        data-testid="onb-checklist-item-label"
                      >
                        {itemLabel(item.itemId)}
                      </span>
                    </li>
                  );
                })}
              </ul>
            </li>
          )}
        </ul>
      )}
    </div>
  );
}
