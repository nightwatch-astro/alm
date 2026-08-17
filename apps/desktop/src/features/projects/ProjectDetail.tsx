// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * ProjectDetail — spec 008 wired, redesigned per PlateVault mock (2026-06-22).
 *
 * Layout (spec 043 task #74 — compact stepper replaces the vertical rail):
 *   DetailHeader            (title + state tag + Edit action)
 *   TopActionBar            (tool · path breadcrumb · Reveal · Open in {tool} · lifecycle actions)
 *   MetricLine              (integration · sources · channels · tool)
 *   ProjectLifecycleStepper (horizontal stage chips + next-action + History collapsible)
 *   Target block            (canonical target, when resolved)
 *   Sections (side column): Sources table · Channels palette
 *
 * Secondary/operational sections (Notes · Manifests · Calibration · Source
 * views · Outputs · Cleanup) live in ProjectBottomDetail, which renders in
 * the full-width bottom panel of the dual side+bottom layout (task #104).
 * They benefit from the horizontal room the bottom strip provides and were
 * collapsed by default in the narrow 420px side column.
 *
 * Per-project actions (Reveal · Open in {tool} · lifecycle
 * transitions) live ONLY in the detail action bar (data-testid="lifecycle-actions").
 * The transition buttons carry the data-testid="transition-btn-*" hooks. The
 * previous duplicate bottom footer was removed to de-duplicate these actions.
 *
 * Channels palette: `subFrames`/`totalIntegrationS` are aggregated server-side
 * (P7 — `ProjectChannelDto`, grouped by `filter_snapshot` over the project's
 * linked sources). `deriveChannels()` only adds the presentational `inSync`
 * flag (whether the channel is currently backed by a linked source with a
 * matching filter), which the API does not model.
 */

import { useState } from 'react';
import { Link } from '@tanstack/react-router';
import { m } from '@/lib/i18n';
import { revealLabel } from '@/lib/reveal-label';
import {
  DetailPane,
  DetailPanel,
  MetricLine,
  Modal,
  StatusTag,
  TopActionBar,
} from '@/components';
import { ProjectLifecycleStepper } from './ProjectLifecycleStepper';
import { Btn, Banner } from '@/ui';
import { deriveChannels } from './projectDetailHelpers';
import { formatIntegration } from '@/lib/format';
import { ProjectChannelsSection } from './ProjectChannelsSection';
import { ProjectSourcesSection } from './ProjectSourcesSection';
import { projectStateLabel, projectStateVariant } from '@/lib/lifecycle';
import { useProjectDetail, useSessionNames } from './store';
import type { ProjectLifecycleState } from './store';
import { useProjectDetailActions } from './useProjectDetailActions';
import { EditProjectPane } from './edit/EditProjectPane';
import { BlockedBanner, deriveBlockedReason } from './BlockedBanner';
import type { BlockedReason } from './BlockedBanner';
import { lifecycleFooterActions } from './lifecycle-actions';
import { PlanReviewOverlay } from '@/features/plans/PlanReviewOverlay';
// spec 011: tool launch CTA
import {
  toolIdFromProjectTool,
  toolLaunchDisabledReason,
  toolLaunchDisabledTooltip,
  useToolProfiles,
  useToolLaunch,
} from './tool-launch';
// spec 012 T008: filesystem artifact watcher, attached/detached with this
// drawer's own mount lifecycle.
import { useProjectArtifactWatcher } from './artifacts';
import type {
  ProjectChannelDto_Deserialize,
  ProjectSourceDto_Deserialize,
} from '@/bindings/index';
// Secondary sections (Notes, Manifests, Calibration, Source views, Outputs,
// Cleanup) have moved to ProjectBottomDetail (task #104 — bottom panel).

// ── Props ────────────────────────────────────────────────────────────────────

export interface ProjectDetailContentProps {
  projectId: string;
}

/** Alias export for backward compatibility (smoke tests, index.ts). */
export { ProjectDetailContent as ProjectDetail };

// ── Component ────────────────────────────────────────────────────────────────

export function ProjectDetailContent({ projectId }: ProjectDetailContentProps) {
  const { data: project, loading, error } = useProjectDetail(projectId);
  // #663: resolve raw session UUIDs to the same human names Sessions shows.
  const sessionNames = useSessionNames();
  const [editOpen, setEditOpen] = useState(false);

  // Every mutating interaction this pane offers, plus the busy flags that
  // gate their buttons. Called unconditionally (before the early returns
  // below), so its handlers guard on `project` themselves.
  const {
    lifecycle,
    channelWorking,
    transitionWorking,
    archiveReviewPlanId,
    setArchiveReviewPlanId,
    archiveEmptyReason,
    setArchiveEmptyReason,
    handleReinfer,
    handleDismissDrift,
    handleTransition,
    handleArchivePlanApplied,
    handleResolveBlocked,
    handleReveal,
  } = useProjectDetailActions(projectId, project);

  // spec 012 T008: attach the project's filesystem artifact watcher for as
  // long as this drawer is open; detaches on close/project switch.
  useProjectArtifactWatcher(projectId);

  // spec 011: tool launch (hooks must be called unconditionally)
  const projectToolStr = typeof project?.tool === 'string' ? project.tool : '';
  const toolId = projectToolStr ? toolIdFromProjectTool(projectToolStr) : '';
  const { profiles } = useToolProfiles();
  const toolProfile = profiles.find((p) => p.id === toolId);
  const {
    state: launchState,
    launch: launchTool,
    dismissPriorWarning,
  } = useToolLaunch(
    projectId,
    toolId,
    projectToolStr || 'tool',
    toolProfile?.supportsOpenFolder,
  );
  const launchDisabledReason = toolLaunchDisabledReason(toolProfile);

  if (loading && !project) {
    return (
      <DetailPane fill>
        <div className="pv-project-detail__loading">
          {m.projects_detail_loading()}
        </div>
      </DetailPane>
    );
  }

  if (error || !project) {
    return (
      <DetailPane fill>
        <Banner variant="danger">{m.projects_detail_load_error()}</Banner>
      </DetailPane>
    );
  }

  const toolLabel =
    typeof project.tool === 'string' ? project.tool : m.projects_tool_unknown();
  // Derive contextual footer actions for the current lifecycle state.
  const footerActions = lifecycleFooterActions(
    lifecycle as ProjectLifecycleState,
  );

  // Derive typed blocked reason from project DTO (FR-020 / spec 033 US5 T053).
  const blockedReason: BlockedReason | undefined =
    lifecycle === 'blocked'
      ? deriveBlockedReason(
          project.blockedReasonKind,
          project.blockedReasonNote,
        )
      : undefined;

  // ── Derived channel palette data (server totals + client `inSync`) ────────
  const derivedChannels = deriveChannels(
    (project.channels ?? []) as ProjectChannelDto_Deserialize[],
    project.sources as ProjectSourceDto_Deserialize[],
  );
  return (
    <DetailPanel
      fill
      title={project.name}
      titleExtra={
        <StatusTag variant={projectStateVariant(lifecycle)}>
          {projectStateLabel(lifecycle)}
        </StatusTag>
      }
      subtitle={undefined}
      actions={
        lifecycle !== 'archived' && (
          <Btn
            size="sm"
            variant="ghost"
            onClick={() => setEditOpen(true)}
            data-testid="edit-project-btn"
          >
            {m.common_edit()}
          </Btn>
        )
      }
    >
      {/* ── Top action bar: tool · path · Reveal · Open in tool · CTA ───
          `wrap` puts the breadcrumb (tool + path) and the action cluster on
          their OWN rows so neither overlaps the MetricLine below (task #81). */}
      <TopActionBar
        title=""
        wrap
        right={
          /* Per-project actions live ONLY here (the detail action bar):
             Reveal · Open in {tool} · lifecycle transitions.
             The transition buttons carry the data-testid="transition-btn-*"
             hooks (previously on a separate bottom footer that has been
             removed to de-duplicate the per-project actions). */
          <div
            className="pv-project-detail__bar-actions"
            data-testid="lifecycle-actions"
          >
            {/* Reveal — platform-native label (shared revealLabel()) */}
            <Btn
              size="sm"
              variant="ghost"
              data-testid="action-reveal"
              disabled={!project.path}
              onClick={() => void handleReveal()}
            >
              {revealLabel()}
            </Btn>

            {/* Open in processing tool */}
            {toolId && (
              <Btn
                size="sm"
                variant="ghost"
                disabled={launchDisabledReason !== null || launchState.working}
                title={
                  launchDisabledReason
                    ? toolLaunchDisabledTooltip(launchDisabledReason)
                    : m.projects_open_in_tool_title({ tool: projectToolStr })
                }
                onClick={() => void launchTool()}
                data-testid="tool-launch-btn"
                data-guide-anchor="project.open-in-tool"
              >
                {launchState.working
                  ? m.projects_launching()
                  : m.projects_open_in({ tool: projectToolStr })}
              </Btn>
            )}

            {/* Lifecycle transitions — single source of truth for these actions. */}
            {footerActions.map((action) => (
              <Btn
                key={action.nextState}
                size="sm"
                variant={action.variant}
                disabled={transitionWorking}
                onClick={() =>
                  void handleTransition(action.nextState, action.label)
                }
                data-testid={`transition-btn-${action.nextState}`}
              >
                {action.label}
              </Btn>
            ))}
          </div>
        }
      >
        <span className="pv-project-detail__bar-tool">{toolLabel}</span>
        {project.path && (
          <span className="pv-project-detail__bar-path">{project.path}</span>
        )}
      </TopActionBar>

      {/* ── Blocked banner (spec 009 US4-2) — above all content ──────────── */}
      {lifecycle === 'blocked' && blockedReason && (
        <BlockedBanner
          reason={blockedReason}
          onResolve={handleResolveBlocked}
          disabled={transitionWorking}
        />
      )}

      {/* ── Channel drift banner (US1c / US4) ────────────────────────────── */}
      {project.channelDrift?.hasNewSources && (
        <Banner variant="warn" role="status" aria-live="polite">
          <span>{m.projects_detail_channel_drift()}</span>
          <div className="pv-project-detail__drift-actions">
            <Btn
              size="sm"
              variant="primary"
              onClick={handleReinfer}
              disabled={channelWorking}
            >
              {m.projects_detail_reinfer_btn()}
            </Btn>
            <Btn
              size="sm"
              variant="ghost"
              onClick={handleDismissDrift}
              disabled={channelWorking}
            >
              {m.projects_detail_dismiss_btn()}
            </Btn>
          </div>
        </Banner>
      )}

      {/* ── Metric line: consolidated single-row summary ──────────────────── */}
      <MetricLine
        metrics={[
          {
            value: formatIntegration(
              derivedChannels.reduce((s, c) => s + c.totalIntegS, 0),
            ),
            label: m.projects_metric_integration(),
          },
          {
            value: project.sources.length,
            label: m.projects_metric_sources({ count: project.sources.length }),
          },
          {
            value: project.channels?.length ?? 0,
            label: m.projects_metric_channels({
              count: project.channels?.length ?? 0,
            }),
          },
          { value: toolLabel, label: m.projects_metric_tool() },
        ]}
      />

      {/* ── Compact lifecycle stepper (task #74) — replaces the vertical rail.
          Horizontal stage chips + next-action line + History collapsible. ── */}
      <ProjectLifecycleStepper
        state={lifecycle}
        createdAt={project.createdAt}
        updatedAt={project.updatedAt}
      />

      {/* spec 035 US1 #2: associated canonical target (resolved on read path).
          No longer a rail card — a compact inline block under the stepper.
          #738: links through to the Targets page with the target
          pre-selected, matching the Cmd+K / TargetsTable entry point. */}
      {project.canonicalTarget && (
        <Link
          to="/targets"
          search={{ selected: project.canonicalTarget.id }}
          className="pv-project-detail__target-info"
          data-testid="project-canonical-target"
        >
          <span className="pv-project-detail__target-label">
            {m.projects_create_target_label()}
          </span>
          <span className="pv-project-detail__target-name">
            {project.canonicalTarget.primaryDesignation}
          </span>
          {project.canonicalTarget.commonName && (
            <span className="pv-project-detail__target-common">
              {project.canonicalTarget.commonName}
            </span>
          )}
        </Link>
      )}

      <div className="pv-project-detail__sections">
        <ProjectSourcesSection
          sources={project.sources as ProjectSourceDto_Deserialize[]}
          sessionNames={sessionNames}
        />

        {/* Channels palette (task #10) — renders nothing when empty. */}
        <ProjectChannelsSection channels={derivedChannels} />

        {/* Secondary sections (Notes · Manifests · Calibration · Source views ·
            Outputs · Cleanup) have moved to the bottom panel (ProjectBottomDetail,
            task #104). They benefit from the full-width horizontal room there and
            were collapsed by default in this narrow 420px column anyway. */}
      </div>

      {/* Lifecycle transition buttons now live in the detail action bar above
          (single source of truth) — the duplicate bottom footer was removed. */}

      {/* ── Tool-launch not-configured hint ─────────────────────────────── */}
      {launchDisabledReason === 'not_configured' && (
        <div
          className="pv-project-detail__footer pv-project-detail__footer--tool"
          data-testid="tool-launch-footer"
        >
          <span className="pv-project-detail__tool-hint">
            {m.projects_tool_not_configured()}{' '}
            <Link
              to="/settings/$pane"
              params={{ pane: 'tools' }}
              className="pv-project-detail__tool-link"
            >
              {m.projects_tool_configure_link()}
            </Link>
          </span>
        </div>
      )}

      {/* ── spec 011: Re-launch confirmation modal ───────────────────────── */}
      {launchState.priorInstanceAlive && (
        <div
          role="dialog"
          aria-modal="true"
          aria-label={m.projects_tool_already_running_aria({
            tool: projectToolStr,
          })}
          className="pv-project-detail__modal-overlay"
          data-testid="relaunch-modal"
        >
          <div className="pv-project-detail__modal-card">
            <p className="pv-project-detail__modal-body">
              {m.projects_relaunch_body({ tool: projectToolStr })}
            </p>
            <div className="pv-project-detail__modal-actions">
              <Btn
                size="sm"
                variant="ghost"
                onClick={dismissPriorWarning}
                data-testid="relaunch-cancel"
              >
                {m.common_cancel()}
              </Btn>
              <Btn
                size="sm"
                variant="primary"
                onClick={() => void launchTool(true)}
                data-testid="relaunch-confirm"
              >
                {m.projects_relaunch_confirm_btn()}
              </Btn>
            </div>
          </div>
        </div>
      )}

      {/* ── Edit pane overlay (#660) ────────────────────────────────────────
          Uses the shared Modal rather than a bare positioned div: the old
          `absolute; inset:0` overlay had no positioned ancestor, so it sized
          against the viewport and hid the whole app shell. Modal also supplies
          the dialog semantics Journey 16 requires (role=dialog, Escape,
          focus trap). */}
      <Modal
        open={editOpen}
        onClose={() => setEditOpen(false)}
        title={m.projects_edit_pane_aria()}
        size="lg"
        data-testid="project-edit-modal"
      >
        <EditProjectPane project={project} onClose={() => setEditOpen(false)} />
      </Modal>

      {/* ── Archive plan review overlay (spec 017 US2/WP-B) ──────────────────
          Opens automatically when the plan-gated Archive transition refuses
          with plan.required; shares the same review → approve → apply kit as
          the cleanup flow. */}
      <PlanReviewOverlay
        planId={archiveReviewPlanId}
        open={archiveReviewPlanId !== null}
        onClose={() => {
          setArchiveReviewPlanId(null);
          setArchiveEmptyReason(null);
        }}
        title={m.archive_generate_review_title()}
        emptyReason={archiveEmptyReason}
        onApplied={handleArchivePlanApplied}
        onRetryCreated={setArchiveReviewPlanId}
      />
    </DetailPanel>
  );
}
