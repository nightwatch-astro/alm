// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * SessionDetail — clean tabular session detail (spec 043 §4 redesign).
 *
 * The session's attributes render as a flat PropertyTable (Property | Value |
 * Source) spread across two columns inside the canonical DetailPanel. Linked
 * projects sit below with a clickable link per project.
 *
 * The per-frame frames table + review-state pill were removed: a session is a
 * single frame-type group, so the frames table only duplicated the row data;
 * the freed space lets the attribute table use both columns.
 *
 * Spec 041 FR-051 (T076, Phase 13): the Confirm/Re-open/Reject/Ignore review
 * actions were removed — sessions are derived, already-confirmed inventory
 * with no review lifecycle. Session metadata remains editable post-hoc via
 * the inbox per-file metadata/override tables. The Reveal action (FR-007) is
 * unrelated to the review lifecycle and is retained.
 */

import { useState } from 'react';
import type {
  CalibrationType,
  InventorySession,
  InventorySourceState,
  SessionCalibrationMatch,
} from '@/bindings/index';
import {
  DetailPane,
  DetailPanel,
  Modal,
  PropertyTable,
  type PropertyDef,
} from '@/components';
import { EmptyState, Btn, Section, Pill } from '@/ui';
import { TwoColDetailLayout } from '@/components';
import { m } from '@/lib/i18n';
import { revealLabel } from '@/lib/reveal-label';
import { addToast } from '@/shared/toast';
import { useCalibrationUnassign } from '@/features/calibration/useCalibration';
import { SessionFrameInventory } from './SessionFrameInventory';
import { SessionNotesSection } from './SessionNotesSection';
import { RawFrameCleanupSection } from './RawFrameCleanupSection';
import { SessionGroupBadge } from './SessionGroupBadge';
import { sessionDisplayName } from './displayName';
import { integrationSeconds } from './integration';
import { formatIntegration } from '@/lib/format';
import { connectivityLabel, connectivityVariant } from './connectivity';
import {
  linkedList,
  calibRow,
  calibNote,
  actions as actionsCls,
  head,
  link,
  muted as mutedCls,
} from '@/components/two-col-detail-layout.css';

/** `SessionCalibrationMatch.kind` is the wider `CalibrationKind` (adds
 * `dark_flat`/`bad_pixel_map`, neither assignable per FR-001), while
 * `calibration.match.unassign` accepts only the narrower `CalibrationType`
 * (dark/flat/bias) DB assignments are actually constrained to. `null` for
 * the two kinds that can never legitimately appear on a real assignment row. */
function toUnassignType(kind: string): CalibrationType | null {
  return kind === 'dark' || kind === 'flat' || kind === 'bias' ? kind : null;
}

/** Calibration-linkage list (#772) with an explicit un-assign action per row
 * (#875): removes the session's assignment for that calibration type,
 * returning it to "no master assigned" — previously only a same-type
 * *replacement* assignment (S6) could clear a wrong match, never a plain
 * removal. Renders an explicit "no calibration match" state when a light
 * session has no assignment yet (and for calibration sessions, which never
 * carry assignments). */
function CalibrationLinkage({
  sessionId,
  matches,
}: {
  sessionId: string;
  matches: SessionCalibrationMatch[];
}) {
  const { unassigning, unassign } = useCalibrationUnassign();
  const [pendingUnassign, setPendingUnassign] =
    useState<SessionCalibrationMatch | null>(null);

  const handleConfirmUnassign = async () => {
    const match = pendingUnassign;
    setPendingUnassign(null);
    const calType = match ? toUnassignType(match.kind) : null;
    if (!match || !calType) return;
    try {
      const res = await unassign(sessionId, calType, match.masterId);
      addToast({
        message:
          res.status === 'success'
            ? m.sessions_calib_unassign_success()
            : (res.error?.message ?? m.sessions_calib_unassign_failed()),
        variant: res.status === 'success' ? 'info' : 'error',
      });
    } catch {
      addToast({
        message: m.sessions_calib_unassign_failed(),
        variant: 'error',
      });
    }
  };

  if (matches.length === 0) {
    return (
      <EmptyState
        title={m.sessions_calib_none()}
        desc={m.sessions_calib_none_desc()}
        data-testid="session-calib-empty"
      />
    );
  }
  return (
    <>
      <div className={linkedList} data-testid="session-calib-list">
        {matches.map((match) => (
          <div key={`${match.kind}-${match.masterId}`} className={calibRow}>
            <Pill variant="info">{match.kind}</Pill>
            <span className="pv-mono">{match.masterId}</span>
            {match.score != null && (
              <span className={calibNote}>
                {m.sessions_calib_score({
                  pct: Math.round(match.score * 100),
                })}
              </span>
            )}
            {match.softMismatches.length > 0 && (
              <span className={calibNote}>
                {m.sessions_calib_soft_mismatch({
                  dims: match.softMismatches.join(', '),
                })}
              </span>
            )}
            {toUnassignType(match.kind) && (
              <Btn
                size="sm"
                variant="danger"
                onClick={() => setPendingUnassign(match)}
                data-testid={`session-calib-unassign-${match.kind}`}
              >
                {m.sessions_calib_unassign_btn()}
              </Btn>
            )}
          </div>
        ))}
      </div>

      {/* Confirm gate (#875, journey J11 "first-class action"): mirrors the
          calibration Archive in-use-confirm modal (MasterDetail.tsx). */}
      <Modal
        open={pendingUnassign !== null}
        onClose={() => setPendingUnassign(null)}
        title={m.sessions_calib_unassign_confirm_title()}
        size="sm"
        ariaLabel={m.sessions_calib_unassign_confirm_title()}
        footer={
          <>
            <Btn variant="ghost" onClick={() => setPendingUnassign(null)}>
              {m.common_cancel()}
            </Btn>
            <Btn
              variant="destructive"
              disabled={unassigning}
              onClick={() => void handleConfirmUnassign()}
              data-testid="session-calib-unassign-confirm-btn"
            >
              {m.sessions_calib_unassign_btn()}
            </Btn>
          </>
        }
      >
        <p>{m.sessions_calib_unassign_confirm_desc()}</p>
      </Modal>
    </>
  );
}

interface Props {
  session: InventorySession | null;
  /** Open the session's source location in the OS file browser (FR-007). */
  onReveal?: () => void;
  /** Whether the source path is known so Reveal can be offered (FR-007). */
  revealVisible?: boolean;
  /** Open a linked project — wired by the page to navigation. */
  onOpenProject?: (projectId: string) => void;
  /** Open a panel group detail view — spec 062. */
  onOpenGroup?: (panelGroupId: string) => void;
  /** The session's owning source connectivity state (#889); `undefined` when
   * unknown (e.g. loading). A non-`active` state renders a chip explaining
   * why file-touching actions (Reveal) are unavailable. */
  sourceState?: InventorySourceState;
}

/** Equipment context subtitle: camera · gain · sensor temp · binning. */
function equipmentSubtitle(session: InventorySession): string {
  const parts: string[] = [];
  if (session.camera) parts.push(session.camera);
  if (session.gain) parts.push(`g${session.gain}`);
  if (session.setTemp) parts.push(session.setTemp);
  if (session.binning) parts.push(session.binning);
  return parts.join(' · ');
}

// ── Component ─────────────────────────────────────────────────────────────────

export function SessionDetail({
  session,
  onReveal,
  revealVisible = false,
  onOpenProject,
  onOpenGroup,
  sourceState,
}: Props) {
  if (!session) {
    return (
      <DetailPane data-testid="session-detail">
        <EmptyState
          title={m.sessions_select_title()}
          desc={m.sessions_select_desc()}
        />
      </DetailPane>
    );
  }

  const isLinked = (session.linked?.projects?.length ?? 0) > 0;
  const prov = session.provenance;
  // Null (not zero) means no derivable total, which omits the row entirely
  // rather than rendering a dash for it.
  const integrationSec = integrationSeconds(session);
  const connLabel = sourceState ? connectivityLabel(sourceState) : null;

  // Session facts as a clean tabular PropertyTable, spread across two columns.
  const factProps: PropertyDef[] = [
    {
      // #619: `type` was a genuinely unrendered DTO field — the row doesn't
      // show it either, so this is new information, not a restated column.
      key: 'type',
      label: m.inbox_frame_type_label(),
      value: session.type,
    },
    {
      key: 'target',
      label: m.projects_create_target_label(),
      value: session.target ?? null,
      source: prov?.target ? 'inferred' : 'fits',
    },
    {
      key: 'filter',
      label: m.common_filter(),
      value: session.filter ?? null,
      source: prov?.filter ? 'inferred' : 'fits',
    },
    {
      key: 'frames',
      label: m.projects_wizard_col_frames(),
      value: session.frames,
    },
    {
      key: 'exposure',
      label: m.calibration_fp_exposure(),
      value: session.exposure ?? null,
      source: 'fits',
    },
    ...(integrationSec != null
      ? [
          {
            key: 'integration',
            label: m.sessions_col_total_integration(),
            value: formatIntegration(integrationSec),
          } as PropertyDef,
        ]
      : []),
    {
      key: 'night',
      label: m.sessions_col_night(),
      value: session.capturedOn ?? null,
      source: 'fits',
    },
    {
      key: 'camera',
      label: m.settings_calmatch_camera(),
      value: session.camera ?? null,
      source: 'fits',
    },
    {
      key: 'gain',
      label: m.settings_calmatch_gain(),
      value: session.gain ?? null,
      source: 'fits',
    },
    {
      key: 'binning',
      label: m.settings_calmatch_binning(),
      value: session.binning ?? null,
      source: 'fits',
    },
    {
      // Applicable to every light session (data-model.md matrix) — always
      // present so an absent value renders the unresolved chip (spec-030
      // Q16 / FR-135) instead of silently vanishing.
      key: 'temp',
      label: m.settings_calmatch_sensor_temp(),
      value: session.setTemp ?? null,
      source: 'fits',
    },
    ...(prov?.confirmedBy
      ? [
          {
            key: 'confirmedby',
            label: m.sessions_col_confirmed_by(),
            value: prov.confirmedBy,
            source: 'user',
          } as PropertyDef,
        ]
      : []),
  ];

  const mid = Math.ceil(factProps.length / 2);
  const colA = factProps.slice(0, mid);
  const colB = factProps.slice(mid);

  // The Reveal action sits inline with the title (left-grouped) so growing
  // the panel only adds trailing whitespace — it never spreads the title and
  // button apart. Spec 041 FR-051 (T076): the review actions that used to
  // share this row (Confirm/Re-open/Reject/Ignore) are removed.
  const actionButtons = (
    <span className={actionsCls}>
      {/* Backing-source connectivity (#889): a session on a missing/disabled/
          reconnect-required root is not "healthy" — surface the reason
          file-touching actions like Reveal are unavailable. */}
      {connLabel && sourceState && (
        <Pill
          variant={connectivityVariant(sourceState)}
          data-testid="session-detail-connectivity"
        >
          {connLabel}
        </Pill>
      )}
      {/* Platform-native label via the shared revealLabel() helper;
          the title keeps the descriptive what-it-does tooltip. */}
      {revealVisible && (
        <Btn size="sm" onClick={onReveal} title={m.sessions_reveal_title()}>
          {revealLabel()}
        </Btn>
      )}
    </span>
  );

  return (
    <DetailPanel
      variant="sessions"
      title={<strong>{sessionDisplayName(session)}</strong>}
      titleExtra={actionButtons}
      subtitle={equipmentSubtitle(session) || undefined}
    >
      {/* Left-packed columns: [props A] [props B] [linked projects] (#813:
          shared TwoColDetailLayout instead of hand-copied divs). */}
      <TwoColDetailLayout
        colA={<PropertyTable mode="view" showSource properties={colA} />}
        colB={<PropertyTable mode="view" showSource properties={colB} />}
        linked={
          <>
            <div className={head}>{m.sessions_linked_projects_heading()}</div>
            {isLinked ? (
              <div className={linkedList}>
                {session.linked?.projects?.map((p) => (
                  <button
                    key={p.id}
                    type="button"
                    className={link}
                    onClick={() => onOpenProject?.(p.id)}
                  >
                    {p.name}
                  </button>
                ))}
              </div>
            ) : (
              <span className={mutedCls}>{m.common_none()}</span>
            )}
          </>
        }
      />

      {/* Spec 062: panel group membership badge — light sessions only. Shows
          the stable panel group this session belongs to, with a link to
          the group detail. Hidden for calibration sessions (no panel group). */}
      {session.type === 'light' && (
        <Section title={m.sessions_panel_group_heading()} defaultOpen>
          <SessionGroupBadge
            sessionId={session.id}
            onOpen={(id) => onOpenGroup?.(id)}
          />
        </Section>
      )}

      {/* Calibration linkage (#772): the session's assigned calibration
          masters, or an explicit "no calibration match" state. */}
      <Section title={m.sessions_calib_heading()} defaultOpen>
        <CalibrationLinkage
          sessionId={session.id}
          matches={session.calibrationMatches ?? []}
        />
      </Section>

      {/* Post-hoc notes (#773): debounced-autosave free-text editor.
          key: one instance (draft + debouncer) per session — required by
          SessionNotesSection's contract so a pending save can never be
          flushed against a different session (cross-session lost write). */}
      <Section title={m.sessions_notes_heading()} defaultOpen>
        <SessionNotesSection
          key={session.id}
          sessionId={session.id}
          initialContent={session.notes ?? null}
        />
      </Section>

      {/* Spec 048 T014/T025: on-demand per-frame inventory (present count +
          disk total) with a relink action for frames flagged missing. */}
      <SessionFrameInventory sessionId={session.id} />

      {/* Spec 048 US3 T031: raw sub-frame cleanup review, grouped by this
          session — previously impossible (no per-frame inventory to scan). */}
      <RawFrameCleanupSection sessionId={session.id} defaultOpen={false} />
    </DetailPanel>
  );
}
