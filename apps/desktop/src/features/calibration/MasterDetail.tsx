// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * MasterDetail — spec 007 wired · spec 043 §4 (calibration detail redesign).
 *
 * Left-packed flat tabular layout matching SessionDetail exactly:
 *   [props A] [props B] [sessions column: "Used by" + "Compatible" stacked]
 *
 * Actions (Use in project / Replace master / Reveal) are inline-left
 * in the title via titleExtra, wrapped in pv-session-detail2__actions — same
 * pattern as SessionDetail's actionButtons. No `actions` prop passed to
 * DetailPanel. No subtitle (kind is already in the title, size is redundant).
 *
 * #642: "Use in project" and "Replace master" stay disabled with an
 * explanatory `title` — no project-picker or replace-master use case exists
 * yet. Reveal is now wired: the contract carries `rootId`/`relativePath`
 * (masters_list/masters_get, `crates/app/calibration/src/matching.rs`),
 * resolved to an absolute path the same way SessionsPage does (root path
 * from `useInventorySources()` + `resolveRevealPath`), reused rather than
 * duplicated. Falls back to disabled+explanatory-title when the master's
 * frame was never resolved to a `file_record` (legacy masters) or its
 * owning root isn't currently actionable.
 *
 * Data wiring:
 *   - master.usedBySessionIds from the list endpoint is always empty.
 *   - We fetch getCalibrationMaster(master.id) → MasterDetail_Serialize whose
 *     usedBySessionIds and compatibleSessions are populated, then cross-reference
 *     listSessions() to build "{target} · {filter} · {night}" labels for both.
 *
 * Matching hero (spec 043 §4 "Detail hero = compatible-sessions match table"):
 *   `MatchCandidatesPanel` is mounted below the fingerprint/linked-sessions row
 *   using the master's first `usedBySessionIds` entry as the matching context
 *   — `calibration.match.suggest` is anchored on a light SESSION (it returns
 *   ranked candidate masters for that one session), so a session id is
 *   required to drive it. `compatibleSessions` cannot supply that id: the
 *   backend leaves it an empty stub today (`masters_get` in
 *   crates/app/calibration/src/matching.rs hardcodes `compatible_sessions:
 *   vec![]`), so it is not used here. When the master has no used session yet
 *   the panel shows its existing "no selection" empty state — real once the
 *   master gets its first assignment instead of faked from stub data.
 *
 * State/logic lives in `useMasterDetail`; pure builders in
 * `master-detail-model.ts`; the archive modal pair in `MasterArchiveFlow`.
 */

import {
  DetailPane,
  DetailPanel,
  PropertyTable,
  TwoColDetailLayout,
} from '@/components';
import { Btn, EmptyState, Pill } from '@/ui';
import type { CalibrationMaster_Serialize as CalibrationMaster } from '@/bindings/index';
import { m } from '@/lib/i18n';
import { revealLabel } from '@/lib/reveal-label';
import { SessionListPopover } from './SessionListPopover';
import { MatchCandidatesPanel } from './MatchCandidatesPanel';
import { MasterArchiveFlow } from './MasterArchiveFlow';
import { buildMasterTitle, buildFingerprintProps } from './master-detail-model';
import { useMasterDetail } from './useMasterDetail';
import {
  actions as mdActionsCls,
  linkedStack,
  match,
} from '@/components/two-col-detail-layout.css';

// ── Props ─────────────────────────────────────────────────────────────────────

interface Props {
  master: CalibrationMaster | null;
  prefillSuggestion: boolean;
  /** Days threshold for aging warnings. Comes from persisted settings (FR-023). */
  agingThresholdDays: number;
}

// ── Component ─────────────────────────────────────────────────────────────────

export function MasterDetail({
  master,
  prefillSuggestion,
  agingThresholdDays,
}: Props) {
  const {
    detail,
    matchSessionId,
    suggestResponse,
    suggestLoading,
    suggestError,
    assigning,
    archiveReviewPlanId,
    setArchiveReviewPlanId,
    inUseConfirmOpen,
    setInUseConfirmOpen,
    archivePending,
    revealTarget,
    revealActionable,
    handleAssign,
    handleReveal,
    handleArchive,
    handleConfirmArchiveInUse,
    handleArchivePlanApplied,
  } = useMasterDetail(master);

  if (!master) {
    return (
      // eslint-disable-next-line alm/require-root-testid -- DetailPane renders data-testid="detail" internally; testid is present in the DOM, not on this JSX node
      <DetailPane>
        <EmptyState
          title={m.calibration_select_master_title()}
          desc={m.calibration_select_master_desc()}
        />
      </DetailPane>
    );
  }

  const isAging1Year = master.ageDays >= 365;
  const isAgingWarn = master.ageDays > agingThresholdDays && !isAging1Year;
  const masterTitle = buildMasterTitle(master);
  const fingerprintProps = buildFingerprintProps(master);
  const mid = Math.ceil(fingerprintProps.length / 2);
  const colA = fingerprintProps.slice(0, mid);
  const colB = fingerprintProps.slice(mid);

  // Actions inline-left in the title, same pattern as SessionDetail's actionButtons.
  const actionButtons = (
    <span className={mdActionsCls}>
      {/* spec 048 US5 (FR-024/025): distinct wording per trigger path; the
				    match itself is never auto-invalidated or removed, so this is a
				    warning badge, not a blocking state. */}
      {detail.missingFlag && (
        <Pill variant="danger" data-testid="calibration-missing-flag">
          {detail.missingFlagLabel}
        </Pill>
      )}
      {/* #642: "Use in project" and "Replace master" had no onClick and no
          data path to drive one — there is no project-picker or
          replace-master flow wired anywhere in the app yet. Per the codebase
          convention for not-yet-backed actions (see ArchivePage's disabled
          Reveal), disable with an explanatory title rather than ship a dead
          "live" button. */}
      <Btn
        size="sm"
        variant="primary"
        disabled
        title={m.calibration_action_use_in_project_unavailable_title()}
      >
        {m.calibration_action_use_in_project()}
      </Btn>
      {(isAging1Year || isAgingWarn) && (
        <Btn
          size="sm"
          variant="danger"
          disabled
          title={m.calibration_action_replace_master_unavailable_title()}
        >
          {m.calibration_action_replace_master()}
        </Btn>
      )}
      {/* #886: builds a reviewable single-master archive plan and opens the
          shared PlanReviewOverlay (same review→approve→apply kit the
          Archive page's Restore action uses). Disabled once the master has
          no tracked file to archive (same untracked case that disables
          Reveal) — a plan with zero resolvable items has nothing to
          review. */}
      <Btn
        size="sm"
        variant="danger"
        disabled={!revealTarget || archivePending}
        title={
          revealTarget ? undefined : m.calibration_reveal_unavailable_title()
        }
        onClick={handleArchive}
        data-testid="calibration-archive-btn"
      >
        {m.calibration_action_archive()}
      </Btn>
      {/* Platform-native label via the shared revealLabel() helper.
          #642: wired once the master's frame path resolves to an
          actionable source; falls back to disabled+explanatory title
          (legacy master with no tracked file, or its root unavailable). */}
      {revealActionable ? (
        <Btn
          size="sm"
          onClick={() => void handleReveal()}
          title={m.calibration_reveal_title()}
          data-testid="calibration-reveal-btn"
        >
          {revealLabel()}
        </Btn>
      ) : (
        <Btn
          size="sm"
          disabled
          title={m.calibration_reveal_unavailable_title()}
          data-testid="calibration-reveal-btn"
        >
          {revealLabel()}
        </Btn>
      )}
    </span>
  );

  return (
    <>
      <DetailPanel
        variant="calibration"
        title={<strong>{masterTitle}</strong>}
        titleExtra={actionButtons}
      >
        {/* Left-packed columns: [props A] [props B] [sessions: Used by + Compatible
            stacked] (#813: shared TwoColDetailLayout instead of hand-copied divs). */}
        <TwoColDetailLayout
          colA={<PropertyTable mode="view" properties={colA} />}
          colB={<PropertyTable mode="view" properties={colB} />}
          linkedClassName={linkedStack}
          linked={
            <>
              <SessionListPopover
                label={m.calibration_used_by_label()}
                names={detail.loading ? [] : detail.confirmedNames}
              />
              <SessionListPopover
                label={m.calibration_compatible_label()}
                names={detail.loading ? [] : detail.compatibleNames}
              />
            </>
          }
        />

        {/* Detail hero (spec 043 §4): ranked candidate-masters match table for
				    the master's matching-context session, with assign/cancel. */}
        <div className={match}>
          <MatchCandidatesPanel
            sessionId={matchSessionId ?? ''}
            response={suggestResponse}
            loading={suggestLoading}
            error={suggestError}
            onAssign={handleAssign}
            assigning={assigning}
            prefillSuggestion={prefillSuggestion}
          />
        </div>
      </DetailPanel>

      <MasterArchiveFlow
        inUseConfirmOpen={inUseConfirmOpen}
        onCloseConfirm={() => setInUseConfirmOpen(false)}
        onConfirmArchiveInUse={handleConfirmArchiveInUse}
        archivePending={archivePending}
        archiveReviewPlanId={archiveReviewPlanId}
        onCloseReview={() => setArchiveReviewPlanId(null)}
        onArchivePlanApplied={handleArchivePlanApplied}
        onRetryCreated={setArchiveReviewPlanId}
      />
    </>
  );
}
