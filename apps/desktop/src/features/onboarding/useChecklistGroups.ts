// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * useChecklistGroups — per-page group state for the onboarding checklist
 * (spec 056, FR-007/FR-031).
 *
 * Extracted from ChecklistSection.tsx (refactor sweep kyo7.104) so the
 * expansion/completion logic can be reasoned about independently of rendering.
 * Owns: items-by-page derivation, session-local expansion overrides, completion
 * query, and the toggle handler.
 */

import { useMemo, useState } from 'react';
import type {
  OnboardingItemDto,
  OnboardingPage,
  OnboardingStateDto,
} from '@/bindings/index';
import { isChecklistGroupSettled } from './checklist-item-helpers';

export interface ChecklistGroupState {
  /** Items mapped per page (built from state.items). */
  itemsByPage: Map<OnboardingPage, OnboardingItemDto[]>;
  /**
   * True if all items on `page` are settled (auto_checked / manually_checked /
   * dismissed). A complete group collapses to its one-line done header (FR-031).
   */
  isGroupComplete: (page: OnboardingPage) => boolean;
  /**
   * FR-007 auto-expand the current page's group; FR-031 takes precedence.
   * A manual toggle (`groupOverrides`) always wins and never re-enables
   * auto-expand.
   */
  isGroupExpanded: (page: OnboardingPage) => boolean;
  /** Group progress scalar (done/total) from state.progress.perPage. */
  groupProgress: (page: OnboardingPage) => { done: number; total: number };
  /** Toggle the expansion override for `page`. */
  toggleGroup: (page: OnboardingPage) => void;
}

/**
 * Derives and manages per-page group expansion state for ChecklistSection.
 *
 * @param state - Live onboarding state from the store.
 * @param currentPage - The page whose group auto-expands (FR-007).
 */
export function useChecklistGroups(
  state: OnboardingStateDto | null,
  currentPage: OnboardingPage | null,
): ChecklistGroupState {
  // Session-local expansion overrides on top of the route-driven default.
  const [groupOverrides, setGroupOverrides] = useState<
    Partial<Record<OnboardingPage, boolean>>
  >({});

  const itemsByPage = useMemo(() => {
    const map = new Map<OnboardingPage, OnboardingItemDto[]>();
    if (state) {
      for (const item of state.items) {
        const list = map.get(item.page) ?? [];
        list.push(item);
        map.set(item.page, list);
      }
    }
    return map;
  }, [state]);

  const isGroupComplete = (page: OnboardingPage): boolean =>
    isChecklistGroupSettled(itemsByPage.get(page) ?? []);

  // FR-007 auto-expand the current page's group; FR-031 takes precedence — a
  // complete group stays a one-line done header even on its own page. A manual
  // toggle always wins and never re-enables auto-expand.
  const isGroupExpanded = (page: OnboardingPage): boolean => {
    const override = groupOverrides[page];
    if (override !== undefined) return override;
    return page === currentPage && !isGroupComplete(page);
  };

  const toggleGroup = (page: OnboardingPage) =>
    setGroupOverrides((prev) => ({ ...prev, [page]: !isGroupExpanded(page) }));

  const groupProgress = (page: OnboardingPage) =>
    state?.progress.perPage.find((p) => p.page === page) ?? {
      done: 0,
      total: 0,
    };

  return {
    itemsByPage,
    isGroupComplete,
    isGroupExpanded,
    groupProgress,
    toggleGroup,
  };
}
