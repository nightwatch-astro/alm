// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Pushes the inbox folder/master count and per-frame-type breakdown into the
 * global status bar's page-contextual slot. The slot is cleared automatically
 * when the page unmounts (route change).
 */

import { useMemo } from 'react';
import { useSetPageStatus } from '@/app/PageStatusContext';
import { m } from '@/lib/i18n';
import { InboxStatsSummary } from './InboxStatsSummary';
import { deriveInboxStats } from './inboxStatsFromItems';
import type { InboxListItem, InboxSourceGroupListItem } from './store';

export interface UseInboxPageStatusOptions {
  /** Page-filtered items — the same array `InboxList` renders. */
  filteredItems: InboxListItem[];
  /** Page-filtered source groups — the same array `InboxList` renders. */
  filteredSourceGroups: InboxSourceGroupListItem[];
  listLoading: boolean;
  isCapped: boolean;
  /** Backend row cap reported by `inbox.list`. */
  limit: number | undefined;
}

export function useInboxPageStatus({
  filteredItems,
  filteredSourceGroups,
  listLoading,
  isCapped,
  limit,
}: UseInboxPageStatusOptions): void {
  // spec 041 US6: aggregate inbox queue stats. Derived from the SAME item list
  // the header/footer count from (distinct-folder counting) so the stats strip,
  // header, and footer always reconcile — a mixed folder counts once overall.
  //
  // spec 058 T022 / SC-004 (owner decision, 2026-07-20): derived from
  // `filteredItems`, NOT `items`. `InboxList` renders `filteredItems`, so
  // deriving the summary from the unfiltered array made the strip report more
  // rows than the list showed whenever a lane or kind filter was active. A
  // summary sitting above a filtered list and disagreeing with it is the same
  // class of lie this feature exists to remove, so the counts now describe what
  // the user is actually looking at.
  // T022 / CHK010: source-group rows are counted, and from the same filtered
  // arrays `InboxList` renders — otherwise the strip and the list disagree the
  // moment a scanned-but-unclassified folder exists.
  const derivedStats = useMemo(
    () => deriveInboxStats(filteredItems, filteredSourceGroups),
    [filteredItems, filteredSourceGroups],
  );

  // Summary count (no page title — top-bar convention): folders / masters.
  //
  // spec 058 SC-004: counted off the SAME filtered arrays as `derivedStats`
  // above, and off the same ones `InboxList` renders. These two surfaces sit
  // beside each other and had drifted apart in two independent ways:
  //
  //   - the stats strip moved onto `filteredItems` for SC-004 while the header
  //     stayed on the unfiltered `items`, so with any lane/kind/search filter
  //     active the two adjacent numbers disagreed;
  //   - `deriveInboxStats` then began counting source-group rows (CHK010) while
  //     the header still ignored them, so a scanned-but-unclassified folder was
  //     counted by one surface and not the other.
  //
  // Both are the same defect SC-004 names — a summary that disagrees with the
  // list under it — so both are fixed at the one site rather than the header
  // being taught the source-group rule separately.
  const folderCount =
    filteredItems.filter((it) => !it.isMaster).length +
    filteredSourceGroups.length;
  const masterCount = filteredItems.filter((it) => it.isMaster).length;
  const summary = useMemo(() => {
    if (listLoading) return m.common_loading();
    const parts: string[] = [];
    if (folderCount > 0)
      parts.push(m.inbox_count_folders({ count: folderCount }));
    if (masterCount > 0)
      parts.push(m.inbox_count_masters({ count: masterCount }));
    const base =
      parts.length > 0 ? parts.join(' · ') : m.inbox_summary_zero_detections();
    return isCapped
      ? m.inbox_summary_capped({ base, limit: String(limit ?? 500) })
      : base;
  }, [listLoading, folderCount, masterCount, isCapped, limit]);

  // #557: this JSX MUST be memoised. `useSetPageStatus` re-runs its effect
  // whenever the node's identity changes; a bare JSX literal gets a fresh
  // identity on every render, so the effect fired every render, called
  // `setNode` on the shell-level `PageStatusProvider`, which re-rendered this
  // page and created another new-identity node — an infinite render loop
  // ("Maximum update depth exceeded") for as long as the Inbox was open.
  const pageStatusNode = useMemo(
    () => (
      <span className="pv-inbox-summary" data-testid="statusbar-inbox-summary">
        <span className="pv-inbox-summary__count">{summary}</span>
        {!listLoading && <InboxStatsSummary stats={derivedStats} />}
      </span>
    ),
    [summary, listLoading, derivedStats],
  );
  useSetPageStatus(pageStatusNode);
}
