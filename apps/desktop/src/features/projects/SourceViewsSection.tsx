// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * SourceViewsSection — spec 026 UI surface in ProjectDetail.
 *
 * Shows generated source views for a project with:
 * - State badge (current / stale / missing / removed / failed / kind_diverged)
 * - Remove action → routes to plan review (spec 017)
 * - Regenerate action → routes to plan review for removed/stale views
 * - Stale indicator badge
 * - kind_diverged affordance with manual-resolution message
 *
 * Per FR-003 / R-026-Pipeline: both actions produce a plan_id that is routed
 * through the spec 017/025 pipeline. The UI directs the user to the Plans page
 * with a toast on success.
 *
 * Spec 030 note: layout must follow Spec 030 design-v4 patterns when that
 * spec lands. For now this follows the existing ProjectDetail Section pattern.
 */

import { useState, useEffect } from 'react';
import { Pill, Btn, Section, Banner } from '@/ui';
import { m } from '@/lib/i18n';
import { addToast } from '@/shared/toast';
import {
  listPreparedViews,
  removePreparedView,
  regeneratePreparedView,
  verifySourceView,
  viewStateLabel,
  viewStateVariant,
  canRemoveView,
  canRegenerateView,
  canVerifyView,
  brokenItemStateLabel,
  observedStateLabel,
} from './source-views';
import type {
  PreparedViewSummary,
  PreparedViewItemDetail,
  SourceViewVerifyResponse,
} from './source-views';
import { errMessage } from '@/lib/errors';
import { GenerateSourceViewDialog } from './GenerateSourceViewDialog';
import { PlanReviewOverlay } from '@/features/plans/PlanReviewOverlay';
import { ViewAuditHistory } from './ViewAuditHistory';

// ── Props ─────────────────────────────────────────────────────────────────────

export interface SourceViewsSectionProps {
  projectId: string;
  /** Whether the collapsible section starts open. Default true. */
  defaultOpen?: boolean;
}

// ── Component ─────────────────────────────────────────────────────────────────

export function SourceViewsSection({
  projectId,
  defaultOpen = true,
}: SourceViewsSectionProps) {
  const [views, setViews] = useState<PreparedViewSummary[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busyViewId, setBusyViewId] = useState<string | null>(null);
  const [generateOpen, setGenerateOpen] = useState(false);
  const [verifyResults, setVerifyResults] = useState<
    Record<string, SourceViewVerifyResponse>
  >({});
  // Every action here produces a plan that only takes effect once it is
  // reviewed, approved and applied; without this overlay the plan is created
  // and then orphaned, so no links are ever materialised.
  const [reviewPlanId, setReviewPlanId] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);

  const generateButton = (
    <Btn
      size="sm"
      variant="primary"
      onClick={() => setGenerateOpen(true)}
      data-testid="generate-source-view-btn"
    >
      {m.projects_source_views_generate_btn()}
    </Btn>
  );

  // Load views on mount or when projectId changes.
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    listPreparedViews(projectId)
      .then((resp) => {
        if (!cancelled) {
          setViews(resp.views);
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setError(errMessage(err));
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [projectId, reloadKey]);

  // ── Handlers ────────────────────────────────────────────────────────────────

  async function handleRemove(viewId: string) {
    setBusyViewId(viewId);
    try {
      const resp = await removePreparedView(viewId);
      const planId = resp.planId;
      addToast({
        variant: 'info',
        message: m.projects_source_views_removal_toast(),
        action: {
          label: m.projects_source_views_view_plan_btn(),
          onClick: () => setReviewPlanId(planId),
        },
      });
      setReviewPlanId(planId);
    } catch (err: unknown) {
      addToast({
        variant: 'warn',
        message: m.projects_source_views_removal_failed({
          message: errMessage(err),
        }),
      });
    } finally {
      setBusyViewId(null);
    }
  }

  async function handleRegenerate(viewId: string) {
    setBusyViewId(viewId);
    try {
      const resp = await regeneratePreparedView(viewId);
      const planId = resp.planId;
      const warning =
        resp.unresolvedItemCount > 0
          ? m.projects_source_views_regen_unresolved({
              count: String(resp.unresolvedItemCount),
            })
          : '';
      addToast({
        variant: 'info',
        message: m.projects_source_views_regen_toast({ warning }),
        action: {
          label: m.projects_source_views_view_plan_btn(),
          onClick: () => setReviewPlanId(planId),
        },
      });
      setReviewPlanId(planId);
    } catch (err: unknown) {
      addToast({
        variant: 'warn',
        message: m.projects_source_views_regen_failed({
          message: errMessage(err),
        }),
      });
    } finally {
      setBusyViewId(null);
    }
  }

  async function handleVerify(viewId: string) {
    setBusyViewId(viewId);
    try {
      const resp = await verifySourceView(viewId);
      setVerifyResults((prev) => ({ ...prev, [viewId]: resp }));
    } catch (err: unknown) {
      addToast({
        variant: 'warn',
        message: m.projects_source_views_verify_failed({
          message: errMessage(err),
        }),
      });
    } finally {
      setBusyViewId(null);
    }
  }

  // ── Render ───────────────────────────────────────────────────────────────────

  const dialog = (
    <GenerateSourceViewDialog
      projectId={projectId}
      open={generateOpen}
      onClose={() => setGenerateOpen(false)}
      onPlanCreated={setReviewPlanId}
    />
  );

  // Mounted only while a plan is under review — the overlay reads a query
  // client at render time even when closed.
  const overlay = reviewPlanId !== null && (
    <PlanReviewOverlay
      planId={reviewPlanId}
      open={reviewPlanId !== null}
      onClose={() => setReviewPlanId(null)}
      title={m.projects_source_views_review_title()}
      onApplied={() => setReloadKey((key) => key + 1)}
      onRetryCreated={setReviewPlanId}
    />
  );

  if (loading) {
    return (
      <Section
        title={m.projects_source_views_title()}
        defaultOpen={defaultOpen}
        right={generateButton}
      >
        <p className="pv-text-sm pv-text-muted">{m.common_loading()}</p>
        {dialog}
        {overlay}
      </Section>
    );
  }

  if (error) {
    return (
      <Section
        title={m.projects_source_views_title()}
        defaultOpen={defaultOpen}
        right={generateButton}
      >
        <Banner variant="danger">
          {m.projects_source_views_load_error({ error })}
        </Banner>
        {dialog}
        {overlay}
      </Section>
    );
  }

  if (views.length === 0) {
    return (
      <Section
        title={m.projects_source_views_title()}
        defaultOpen={defaultOpen}
        right={generateButton}
      >
        <p className="pv-text-sm pv-text-muted">
          {m.projects_source_views_empty()}
        </p>
        {dialog}
        {overlay}
      </Section>
    );
  }

  return (
    <Section
      title={m.projects_source_views_title()}
      defaultOpen={defaultOpen}
      right={generateButton}
    >
      {dialog}
      {overlay}
      <ul className="pv-source-views__list">
        {views.map((view) => (
          <li
            key={view.id}
            className="pv-source-views__row"
            data-testid={`source-view-row-${view.id}`}
          >
            <div className="pv-stack-1 pv-rail">
              <div className="pv-source-views__row-head">
                <span className="pv-source-views__id" title={view.id}>
                  {view.id.slice(0, 8)}…
                </span>
                <Pill variant={viewStateVariant(view.state)}>
                  {viewStateLabel(view.state)}
                </Pill>
                <span className="pv-text-xs pv-text-muted">{view.kind}</span>
                <span className="pv-text-xs pv-text-muted">
                  {view.itemCount} {m.projects_source_views_items_unit()}
                </span>
              </div>

              {/* FR-033 / T078: per-item inventory refs. T016: each item shows
                  its `lastObservedState` (T014/T015 sweep, refreshed on every
                  list load) when it isn't `present` — this is the persisted
                  broken-reference detail, distinct from the on-demand Verify
                  report below. */}
              {view.items.length > 0 && (
                <details className="pv-source-views__refs-details pv-text-xs pv-text-muted">
                  <summary className="pv-source-views__refs-summary">
                    {m.projects_source_views_inventory_ref_count({
                      count: view.items.length,
                    })}
                  </summary>
                  <ul
                    className="pv-source-views__refs-list"
                    data-testid={`source-view-items-${view.id}`}
                  >
                    {view.items.map((item: PreparedViewItemDetail) => (
                      <li
                        key={item.id}
                        title={m.projects_source_view_item_title({
                          id: item.inventoryItemId,
                        })}
                        className="pv-source-views__refs-item"
                      >
                        {item.viewRelativePath}
                        {item.lastObservedState !== 'present' && (
                          <>
                            {' — '}
                            <span
                              data-testid={`source-view-item-observed-${item.id}`}
                            >
                              {observedStateLabel(item.lastObservedState)}
                            </span>
                          </>
                        )}
                      </li>
                    ))}
                  </ul>
                </details>
              )}

              {/* T016: persisted stale-item summary from the T014 sweep — no
                  click required, unlike the on-demand Verify report below. */}
              {(view.state === 'stale' || view.state === 'missing') && (
                <Banner variant="warn" data-testid={`stale-summary-${view.id}`}>
                  {m.projects_source_views_stale_items_summary({
                    count: String(
                      view.items.filter(
                        (item) => item.lastObservedState !== 'present',
                      ).length,
                    ),
                  })}
                </Banner>
              )}

              {/* kind_diverged resolution affordance (D-026-H2) */}
              {view.state === 'kind_diverged' && (
                <Banner variant="warn">
                  {m.projects_source_views_kind_mismatch()}
                </Banner>
              )}

              {/* Spec 049 US4: read-only verify-before-processing report — no
                  mutation affordance; repair is via Regenerate above. */}
              {verifyResults[view.id] && (
                <Banner
                  variant={verifyResults[view.id].clean ? 'info' : 'warn'}
                  data-testid={`verify-view-result-${view.id}`}
                >
                  {verifyResults[view.id].clean ? (
                    m.projects_source_views_verify_clean()
                  ) : (
                    <div className="flex flex-col gap-1">
                      <span>
                        {m.projects_source_views_verify_broken_summary({
                          count: String(
                            verifyResults[view.id].brokenItems?.length ?? 0,
                          ),
                        })}
                      </span>
                      <ul className="pv-source-views__refs-list">
                        {(verifyResults[view.id].brokenItems ?? []).map(
                          (item) => (
                            <li
                              key={item.inventoryItemId}
                              className="pv-source-views__refs-item"
                            >
                              {item.viewRelativePath} —{' '}
                              {brokenItemStateLabel(item.state)}
                            </li>
                          ),
                        )}
                      </ul>
                    </div>
                  )}
                </Banner>
              )}

              {/* T019: audit-history surface — this view's removal/regeneration
                  plans, reusing the shared plan review overlay for full detail. */}
              <ViewAuditHistory viewId={view.id} onViewPlan={setReviewPlanId} />
            </div>

            <div className="pv-source-views__actions">
              {canVerifyView(view.state) && (
                <Btn
                  size="sm"
                  variant="ghost"
                  disabled={busyViewId !== null}
                  onClick={() => handleVerify(view.id)}
                  data-testid={`verify-view-${view.id}`}
                >
                  {busyViewId === view.id
                    ? m.common_working()
                    : m.projects_source_views_verify_btn()}
                </Btn>
              )}

              {canRemoveView(view.state) && (
                <Btn
                  size="sm"
                  variant="destructive"
                  disabled={busyViewId !== null}
                  onClick={() => handleRemove(view.id)}
                  data-testid={`remove-view-${view.id}`}
                >
                  {busyViewId === view.id
                    ? m.common_working()
                    : m.common_remove()}
                </Btn>
              )}

              {canRegenerateView(view.state) && (
                <Btn
                  size="sm"
                  variant="primary"
                  disabled={busyViewId !== null}
                  onClick={() => handleRegenerate(view.id)}
                  data-testid={`regenerate-view-${view.id}`}
                >
                  {busyViewId === view.id
                    ? m.common_working()
                    : m.common_regenerate()}
                </Btn>
              )}
            </div>
          </li>
        ))}
      </ul>
    </Section>
  );
}
