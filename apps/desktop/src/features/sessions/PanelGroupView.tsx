// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * PanelGroupView — shows the accepted panel-group head and, when the group
 * belongs to a mosaic, the full MosaicDetail for that mosaic.
 *
 * Rendered in SessionsPage when the user activates a panel group badge in
 * SessionDetail. Replaces the session detail pane with group/mosaic context.
 */

import { DetailPanel, FactsKV } from '@/components';
import { Skeleton, EmptyState } from '@/ui';
import { m } from '@/lib/i18n';
import { usePanelGroup } from './useGroupsStore';
import { MosaicDetail } from './MosaicDetail';

export interface PanelGroupViewProps {
  panelGroupId: string;
}

export function PanelGroupView({ panelGroupId }: PanelGroupViewProps) {
  const { data, isLoading, isError } = usePanelGroup(panelGroupId);

  if (isLoading) {
    return (
      <div role="status" aria-live="polite" data-testid="panel-group-view">
        <Skeleton variant="block" />
      </div>
    );
  }

  if (isError || !data) {
    return (
      <EmptyState
        title={m.mosaic_error_title()}
        description={m.mosaic_error_desc()}
      />
    );
  }

  const { acceptedHead } = data;

  return (
    <DetailPanel
      title={m.sessions_panel_group_heading()}
      subtitle={panelGroupId.slice(0, 8)}
      data-testid={`panel-group-view-${panelGroupId}`}
    >
      <div className="pv-detailpanel__kv-grid">
        <FactsKV
          label={m.sessions_group_badge_label({
            count: acceptedHead.sessionCount,
          })}
          value={String(acceptedHead.sessionCount)}
        />
        <FactsKV
          label={m.mosaic_revision_label()}
          value={String(acceptedHead.revisionNumber)}
        />
        <FactsKV
          label={m.mosaic_accepted_at_label()}
          value={acceptedHead.acceptedAt}
        />
        {acceptedHead.canonicalTargetId && (
          <FactsKV
            label={m.mosaic_target_label()}
            value={<code>{acceptedHead.canonicalTargetId}</code>}
          />
        )}
      </div>

      {/* Show MosaicDetail when this panel group's representative is part of a
          mosaic. TODO(ic9h.20): once mosaic membership is returned in the
          PanelGroupRevision DTO (mosaicId field), render MosaicDetail directly.
          Until then the mosaic surface is reachable through RelationProposalDetail
          for accepted mosaic proposals. */}
      {acceptedHead.crossTargetAssociationId && (
        <MosaicDetail mosaicId={acceptedHead.crossTargetAssociationId} />
      )}
    </DetailPanel>
  );
}
