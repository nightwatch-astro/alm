// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * One ingestion group's row in `PlanPanel`'s scrollable plan list: the
 * aligned group-header grid (select/expand, plan name, composition summary,
 * destination, file count, per-group actions), live apply progress, the
 * stale banner, and the expandable per-file detail rows. Extracted from
 * `PlanPanel` — shares the column grid template with the panel's list head,
 * so it renders exactly the same markup as before the split.
 */

import { Banner, Btn } from '@/ui';
import { m } from '@/lib/i18n';
import type { PlanApplyProgress } from '@/features/plans/usePlanApplyProgress';
import * as pp from './plan-panel.css';
import type { InboxOpenPlan } from './store';
import {
  basename,
  breakdownDestination,
  buildGroupBreakdown,
  buildGroupSummary,
  frameTypeCountLabel,
  frameTypeLabel,
  pluralLabel,
} from './planPanelHelpers';

export interface PlanGroupRowProps {
  plan: InboxOpenPlan;
  /** Starting global action index for this plan's rows (stable testids). */
  rowOffset: number;
  checked: boolean;
  isExpanded: boolean;
  onToggleExpanded: () => void;
  onToggleGroup: () => void;
  onApplyOne?: (planId: string) => void;
  onCancel: (inboxItemId: string) => void;
  busy: boolean;
  confirmedPlanIds: Set<string>;
  progress?: PlanApplyProgress | null;
  progressPlanId?: string | null;
  absoluteByFromPath?: Record<string, string>;
  frameTypeByItemId?: Record<string, string>;
  breakdownByItemId?: Record<
    string,
    ReadonlyArray<{ kind: string; count: number }>
  >;
}

export function PlanGroupRow({
  plan,
  rowOffset,
  checked,
  isExpanded,
  onToggleExpanded,
  onToggleGroup,
  onApplyOne,
  onCancel,
  busy,
  confirmedPlanIds,
  progress = null,
  progressPlanId = null,
  absoluteByFromPath,
  frameTypeByItemId,
  breakdownByItemId,
}: PlanGroupRowProps) {
  // Collapsed-by-default summary. PREFERRED: the ingestion's frame-type
  // BREAKDOWN (the per-type bias/dark/flat/light/master tally derived
  // from the inbox item) → ONE line "10 bias · 21 dark · 12 light →
  // <dest>". This is the #75 fix — plan actions carry no per-file frame
  // type and a single hint mislabels a MIXED folder. FALLBACK (no
  // breakdown): one line per (frame type → destination) inferred from
  // each action's path keyword + the per-ingestion hint.
  const breakdownEntries = buildGroupBreakdown(
    breakdownByItemId?.[plan.inboxItemId],
  );
  const summaryLines =
    breakdownEntries.length > 0
      ? []
      : buildGroupSummary(
          plan.actions,
          absoluteByFromPath,
          frameTypeByItemId?.[plan.inboxItemId],
        );
  const breakdownDest =
    breakdownEntries.length > 0
      ? breakdownDestination(plan.actions, plan.itemName, absoluteByFromPath)
      : null;
  // A plan is "catalogued in place" when no file moves — every action is a
  // catalogue (destination equals source). We surface that explicitly
  // instead of an arrow-to-folder, which reads as a move.
  const allInPlace =
    plan.actions.length > 0 &&
    plan.actions.every((a) => a.action === 'catalogue');
  // Count of files that actually move (for the at-a-glance plan summary).
  const moveCount = plan.actions.filter((a) => a.action === 'move').length;
  const rowsId = `plan-group-rows-${plan.inboxItemId}`;

  return (
    <section
      className={pp.group}
      data-testid={`plan-group-${plan.inboxItemId}`}
    >
      {/* Group header — an aligned grid row (shares its column template
          with the list head so every plan's columns line up). */}
      <div className={pp.groupHeader}>
        {/* Col 1: select + expand */}
        <span className={pp.groupLead}>
          <input
            type="checkbox"
            checked={checked}
            disabled={plan.stale}
            onChange={onToggleGroup}
            aria-label={m.inbox_select_plan_aria({ name: plan.itemName })}
            data-testid={`plan-group-check-${plan.inboxItemId}`}
          />
          <Btn
            variant="ghost"
            size="sm"
            onClick={onToggleExpanded}
            aria-expanded={isExpanded}
            aria-controls={rowsId}
            aria-label={
              isExpanded
                ? m.inbox_hide_files_aria({ name: plan.itemName })
                : m.inbox_show_files_aria({ name: plan.itemName })
            }
            data-testid={`plan-group-toggle-${plan.inboxItemId}`}
            className={pp.expand}
          >
            <span
              className={
                isExpanded ? `${pp.chevron} ${pp.chevronOpen}` : pp.chevron
              }
              aria-hidden="true"
            >
              ▸
            </span>
          </Btn>
        </span>

        {/* Col 2: plan / source folder ("(root)" for the library root) */}
        <span
          className={pp.groupName}
          title={plan.itemName || m.inbox_list_root_label()}
        >
          {plan.itemName || m.inbox_list_root_label()}
        </span>

        {/* Col 3: composition breakdown (aligned). */}
        <span
          className={pp.groupBreakdown}
          data-testid={`plan-group-summary-${plan.inboxItemId}`}
        >
          {(breakdownEntries.length > 0
            ? breakdownEntries
            : summaryLines.map((l) => ({
                key: l.key,
                frameType: l.frameType,
                count: l.count,
              }))
          ).map((entry, i) => {
            // A KNOWN canonical frame type (light/dark/flat/bias/dark
            // flat/master) gets a localized count-variant message that
            // already embeds the number — render it as one node. An
            // UNKNOWN frame type (e.g. the `actionLabel(...)` fallback)
            // keeps the existing count + `pluralLabel` rendering.
            const knownLabel = frameTypeCountLabel(
              entry.frameType,
              entry.count,
            );
            return (
              <span
                key={entry.key}
                className={pp.summaryType}
                data-testid="plan-summary-type"
              >
                {i > 0 && (
                  <span className={pp.summarySep} aria-hidden="true">
                    ·{' '}
                  </span>
                )}
                {knownLabel !== null ? (
                  <span className={pp.summaryTypeName}>{knownLabel}</span>
                ) : (
                  <>
                    <span className={pp.summaryTypeCount}>{entry.count}</span>{' '}
                    <span className={pp.summaryTypeName}>
                      {pluralLabel(entry.frameType, entry.count)}
                    </span>
                  </>
                )}
              </span>
            );
          })}
        </span>

        {/* Col 4: destination (aligned across all plans). In-place
            catalogues read "In place · <folder>"; moves read "→ <dest>". */}
        <span
          className={pp.groupDest}
          data-testid={`plan-group-dest-${plan.inboxItemId}`}
        >
          {allInPlace ? (
            <>
              <span
                className={pp.inplace}
                data-testid="plan-panel-inplace"
              >
                {m.inbox_inplace_label()}
              </span>
              <code
                className={pp.summaryDest}
                title={breakdownDest?.full ?? plan.itemName}
              >
                {breakdownDest?.short ?? plan.itemName}
              </code>
            </>
          ) : (
            <>
              <span
                className={pp.summaryArrow}
                data-testid="plan-panel-summary-arrow"
                aria-hidden="true"
              >
                →
              </span>
              <code
                className={pp.summaryDest}
                title={
                  breakdownDest?.full ?? summaryLines[0]?.destinationFull ?? ''
                }
              >
                {breakdownDest?.short ??
                  summaryLines[0]?.destinationShort ??
                  '—'}
              </code>
            </>
          )}
        </span>

        {/* Col 5: file count (+ move/in-place split in the tooltip). */}
        <span
          className={pp.groupCount}
          title={
            moveCount > 0
              ? m.inbox_plan_file_count_tooltip_mixed({
                  moved: moveCount,
                  inPlace: plan.actions.length - moveCount,
                })
              : m.inbox_plan_file_count_tooltip_inplace({
                  count: plan.actions.length,
                })
          }
        >
          {m.inbox_list_file_count({ count: plan.actions.length })}
        </span>

        {/* Col 6: stale badge + per-group apply (live progress) + discard */}
        <span className={pp.groupActions}>
          {plan.stale && (
            <span
              className={pp.staleBadge}
              data-testid={`plan-stale-${plan.inboxItemId}`}
            >
              {m.inbox_stale()}
            </span>
          )}
          {/* Apply just this ingestion's plan with live per-item
              progress streamed over the OperationEvent channel
              (spec 042 US16 / FR-021). */}
          {onApplyOne && (
            <Btn
              variant="ghost"
              size="sm"
              onClick={() => onApplyOne(plan.planId)}
              disabled={
                busy ||
                plan.stale ||
                (plan.actions.some((a) => a.requiresDestructiveConfirm) &&
                  !confirmedPlanIds.has(plan.planId))
              }
              data-testid={`plan-apply-one-${plan.inboxItemId}`}
              aria-label={m.inbox_apply_plan_live_aria({ name: plan.itemName })}
            >
              {m.inbox_apply_action()}
            </Btn>
          )}
          <Btn
            variant="ghost"
            size="sm"
            onClick={() => onCancel(plan.inboxItemId)}
            disabled={busy}
            data-testid={`plan-cancel-${plan.inboxItemId}`}
            aria-label={m.inbox_discard_plan_aria({ name: plan.itemName })}
          >
            {m.inbox_discard()}
          </Btn>
        </span>
      </div>

      {/* Live long-op progress for the plan currently streaming over
          the OperationEvent channel (spec 042 US16 / FR-021). */}
      {progress && progressPlanId === plan.planId && (
        <div
          className={pp.progress}
          data-testid={`plan-progress-${plan.inboxItemId}`}
          role="status"
          aria-live="polite"
        >
          {(() => {
            // Count label ("N" or "N of M") — kept as a sub-message so the
            // "of" connector stays translatable; passed into the plural
            // variant messages below as {countText}.
            const countText =
              progress.total != null
                ? m.inbox_progress_count_of({
                    applied: progress.applied,
                    total: progress.total,
                  })
                : String(progress.applied);
            if (progress.terminal === 'completed') {
              return m.inbox_progress_completed({
                applied: progress.applied,
                countText,
              });
            }
            if (progress.terminal === 'failed') {
              return m.inbox_progress_failed({
                applied: progress.applied,
                failed: progress.failed,
              });
            }
            const failedText =
              progress.failed > 0
                ? m.inbox_progress_failed_suffix({ failed: progress.failed })
                : '';
            return m.inbox_progress_running({
              applied: progress.applied,
              countText,
              failedText,
            });
          })()}
        </div>
      )}

      {plan.stale && (
        <Banner variant="danger" className={pp.staleBanner}>
          {m.inbox_stale_plan_warning()}
        </Banner>
      )}

      {/* Per-file detail — grid rows that share the PARENT column
          template, so File aligns under "Plan", action under
          "Composition", and the destination under "Destination". Hidden
          until expanded. */}
      {isExpanded && (
        <div className={pp.fileRows} id={rowsId}>
          {plan.actions.map((a, actionPos) => {
            const rowIdx = rowOffset + actionPos;
            // FR-031: prefer the absolute destination path from the last
            // confirm response (keyed by source path); fall back to the
            // root-relative preview for plans without a captured absolute.
            const absolute = absoluteByFromPath?.[a.fromPath];
            const destText = absolute ?? a.destinationPreview;
            const inPlace =
              a.action === 'catalogue' || a.destinationPreview === a.fromPath;
            return (
              <div
                key={a.index}
                className={pp.fileRow}
                data-testid={`plan-file-row-${rowOffset + a.index}`}
              >
                <span aria-hidden="true" />
                <span className={pp.fileName} title={a.fromPath}>
                  {basename(a.fromPath)}
                </span>
                <span className={pp.fileAction}>
                  {/* Per-file frame type (composition), inferred from the
                      path / item hint — not the repetitive action kind. */}
                  {frameTypeLabel(
                    a,
                    destText,
                    frameTypeByItemId?.[plan.inboxItemId],
                  )}
                  {a.requiresDestructiveConfirm && (
                    <span className={pp.fileFlag}>
                      {m.inbox_destructive_flag()}
                    </span>
                  )}
                </span>
                {/* #606 (Constitution II — a filesystem mutation must be
                    reviewable): show BOTH sides of the move, not just the
                    destination. The source was previously only reachable by
                    hovering the file-name cell's tooltip, so a reviewer could
                    not see what a move was actually moving. Mirrors
                    PlanReviewOverlay's From/To columns. In-place catalogue
                    actions have no second side — the single path is the
                    item's current (and final) location. */}
                <span className={pp.fileDest}>
                  <code
                    className={pp.path}
                    data-testid={`inbox-source-absolute-${rowIdx}`}
                    title={`${m.plans_review_col_from()}: ${a.fromPath}`}
                  >
                    {a.fromPath}
                  </code>
                  {inPlace ? (
                    <span
                      className={pp.inplace}
                      data-testid="plan-panel-inplace"
                    >
                      {m.inbox_inplace_label()}
                    </span>
                  ) : (
                    <>
                      <span className={pp.summaryArrow} aria-hidden="true">
                        {' '}
                        →{' '}
                      </span>
                      <code
                        className={pp.path}
                        data-testid={`inbox-dest-absolute-${rowIdx}`}
                        title={`${m.plans_review_col_to()}: ${destText}`}
                      >
                        {destText}
                      </code>
                    </>
                  )}
                </span>
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}
