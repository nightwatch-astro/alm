// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * SessionGroupBadge — panel-group membership chip for SessionDetail.
 *
 * Spec 062 US2: every light session belongs to exactly one active panel group.
 * This badge resolves the group by the session ID (via panel_group.list with
 * sessionId filter) and shows the group's session count.
 *
 * Two modes:
 *  - `panelGroupId` — use when the caller already has the group ID (fast).
 *  - `sessionId` — use when the caller only has the session; the hook
 *    resolves the group ID internally. The badge renders nothing for
 *    calibration sessions (which have no panel group).
 *
 * Renders a Skeleton while loading; renders nothing when the session has no
 * panel membership (calibration frames, or not yet resolved).
 *
 * Clicking calls `onOpen` with the resolved panel group ID so the caller can
 * open the group detail view.
 */

import { Pill, Skeleton } from '@/ui';
import { usePanelGroup, usePanelGroupList } from './useGroupsStore';
import { m } from '@/lib/i18n';

// ── Direct panel group badge (when group ID is known) ─────────────────────────

interface PanelGroupBadgeByIdProps {
  panelGroupId: string;
  onOpen?: (panelGroupId: string) => void;
}

function PanelGroupBadgeById({
  panelGroupId,
  onOpen,
}: PanelGroupBadgeByIdProps) {
  const { data, isLoading } = usePanelGroup(panelGroupId);

  if (isLoading) return <Skeleton variant="line" />;
  if (!data) return null;

  const { acceptedHead } = data;

  return (
    <button
      type="button"
      className="pv-group-badge"
      aria-label={m.sessions_group_badge_aria({ id: panelGroupId })}
      onClick={() => onOpen?.(panelGroupId)}
      disabled={!onOpen}
    >
      <Pill variant="neutral">
        {m.sessions_group_badge_label({ count: acceptedHead.sessionCount })}
      </Pill>
      {acceptedHead.retired && (
        <Pill variant="warn" aria-label={m.sessions_group_retired_aria()}>
          {m.sessions_group_retired_label()}
        </Pill>
      )}
    </button>
  );
}

// ── Session-scoped badge (resolves group from session ID) ─────────────────────

interface PanelGroupBadgeBySessionProps {
  sessionId: string;
  onOpen?: (panelGroupId: string) => void;
}

function PanelGroupBadgeBySession({
  sessionId,
  onOpen,
}: PanelGroupBadgeBySessionProps) {
  const { data, isLoading } = usePanelGroupList({
    sessionId,
    activeOnly: true,
  });

  if (isLoading) return <Skeleton variant="line" />;

  const head = data?.items[0];
  if (!head) return null;

  return (
    <PanelGroupBadgeById panelGroupId={head.panelGroupId} onOpen={onOpen} />
  );
}

// ── Public component (union of both modes) ────────────────────────────────────

export type SessionGroupBadgeProps =
  | {
      panelGroupId: string;
      sessionId?: never;
      onOpen?: (panelGroupId: string) => void;
    }
  | {
      sessionId: string;
      panelGroupId?: never;
      onOpen?: (panelGroupId: string) => void;
    };

export function SessionGroupBadge(props: SessionGroupBadgeProps) {
  if (props.panelGroupId) {
    return (
      <PanelGroupBadgeById
        panelGroupId={props.panelGroupId}
        onOpen={props.onOpen}
        data-testid="session-group-badge"
      />
    );
  }
  return (
    <PanelGroupBadgeBySession
      sessionId={props.sessionId!}
      onOpen={props.onOpen}
    />
  );
}
