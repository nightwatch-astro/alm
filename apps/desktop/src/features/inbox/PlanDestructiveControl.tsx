// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Destructive-destination control (relocated from the deleted ActionSidebar)
 * + the destructive-confirm gate, extracted from `PlanPanel`. Rendered only
 * when at least one open plan has a destructive action.
 */

import { Banner } from '@/ui';
import { m } from '@/lib/i18n';
import type { DestructiveDestination } from './PlanPanel';
import * as pp from './plan-panel.css';

export interface PlanDestructiveControlProps {
  destructiveDestination: DestructiveDestination;
  onDestructiveDestinationChange: (d: DestructiveDestination) => void;
  allDestructiveConfirmed: boolean;
  confirmingDestructive: boolean;
  confirmDestructiveError: string | null;
  onConfirmDestructive: () => void;
}

export function PlanDestructiveControl({
  destructiveDestination,
  onDestructiveDestinationChange,
  allDestructiveConfirmed,
  confirmingDestructive,
  confirmDestructiveError,
  onConfirmDestructive,
}: PlanDestructiveControlProps) {
  return (
    <div className={pp.destructive}>
      <div className={pp.destructiveTitle}>
        {m.inbox_where_source_files_go()}
      </div>
      <div className={pp.destOptions}>
        {}
        <label className={pp.destLabel}>
          <input
            type="radio"
            name="destructive-destination"
            value="archive"
            checked={destructiveDestination === 'archive'}
            onChange={() => onDestructiveDestinationChange('archive')}
            aria-label={m.inbox_archive_folder()}
            data-testid="plan-destructive-archive"
          />
          <span>
            <strong>{m.inbox_archive_folder()}</strong>
            <span className={pp.destLabelHint}>{m.inbox_archive_hint()}</span>
          </span>
        </label>
        {}
        <label className={pp.destLabel}>
          <input
            type="radio"
            name="destructive-destination"
            value="trash"
            checked={destructiveDestination === 'trash'}
            onChange={() => onDestructiveDestinationChange('trash')}
            aria-label={m.inbox_system_trash()}
            data-testid="plan-destructive-trash"
          />
          <span>{m.inbox_system_trash()}</span>
        </label>
      </div>

      {/* Destructive-confirm gate (FR-003, D9, issue #741): destructive
          items (trash/delete) were previously refused permanently at
          apply time — `destructive_confirmed` had no writer. Plan-level
          (not per-item — `InboxPlanAction` carries no item id). */}
      <label className={pp.destLabel}>
        <input
          type="checkbox"
          checked={allDestructiveConfirmed}
          disabled={confirmingDestructive || allDestructiveConfirmed}
          onChange={onConfirmDestructive}
          aria-label={m.inbox_confirm_destructive_aria()}
          data-testid="plan-destructive-confirm"
        />
        <span>
          {confirmingDestructive
            ? m.inbox_confirm_destructive_confirming()
            : m.inbox_confirm_destructive_label()}
        </span>
      </label>
      {confirmDestructiveError !== null && (
        <Banner variant="danger">{confirmDestructiveError}</Banner>
      )}
    </div>
  );
}
