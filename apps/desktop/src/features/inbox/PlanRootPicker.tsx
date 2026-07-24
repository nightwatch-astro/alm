// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Destination-root picker (spec 041 US8/FR-029), extracted from `PlanPanel`.
 * Surfaced whenever the last confirm needs a root choice; blocks apply until
 * chosen since the plan isn't generated until confirm succeeds with a rootId.
 */

import { Btn } from '@/ui';
import { m } from '@/lib/i18n';
import type { PendingRootPick } from './PlanPanel';
import * as pp from './plan-panel.css';

export interface PlanRootPickerProps {
  pendingRootPick: PendingRootPick;
  onPickDestinationRoot?: (rootId: string) => void;
  rootPickBusy: boolean;
}

export function PlanRootPicker({
  pendingRootPick,
  onPickDestinationRoot,
  rootPickBusy,
}: PlanRootPickerProps) {
  return (
    <div className={pp.rootPicker} data-testid="inbox-root-picker">
      <div className={pp.rootPickerTitle}>
        {m.inbox_choose_dest_root_title()}
      </div>
      <div className={pp.rootPickerDesc}>
        {m.inbox_choose_dest_root_body({ category: pendingRootPick.category })}
      </div>
      <div className={pp.rootPickerOptions}>
        {pendingRootPick.candidates.map((c) => (
          <Btn
            key={c.rootId}
            variant="ghost"
            onClick={() => onPickDestinationRoot?.(c.rootId)}
            disabled={rootPickBusy}
            data-testid={`inbox-root-option-${c.rootId}`}
            aria-label={m.inbox_use_as_destination_root_aria({ path: c.path })}
            className={pp.rootOption}
          >
            <span className={pp.rootOptionInner}>
              <code className={pp.rootOptionPath}>{c.path}</code>
              <span className={pp.rootOptionKind}>{c.kind}</span>
            </span>
          </Btn>
        ))}
      </div>
    </div>
  );
}
