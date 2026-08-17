// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// First-run wizard: "Scan" step (spec 038).
//
// Registers sources (via flushToDB), then runs inbox_scan_folder +
// inbox_classify per source, showing per-source progress and a detection
// summary.  Approval stays in the Inbox — no inbox_confirm called here.

import { useEffect, useMemo } from 'react';
import { useQueries, useQuery } from '@tanstack/react-query';
import { m } from '@/lib/i18n';
import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import type { SourceEntry } from '../sources-store';
import type { FlushResult } from '../sources-store';
import { errMessage } from '@/lib/errors';
import {
  isScanTarget,
  queryPhase,
  scanAndClassify,
  type RenderTarget,
} from './step-scan-model';
import { SourceSummary, type SourceScanState } from './SourceSummary';

// ── Props ─────────────────────────────────────────────────────────────────────

export interface StepScanProps {
  /** All registered sources from the wizard state. */
  sources: SourceEntry[];
  /** Result of flushToDB; rootId per path lets us pass the right id to scan. */
  flushResult: FlushResult;
  /**
   * Called whenever the "all scans done" state changes so the parent can
   * enable/disable the Finish button that now lives in the wizard footer.
   */
  onAllDoneChange: (done: boolean) => void;
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/** Derive a stable rootId string from the flushResult for a given path.
 *  Falls back to the path itself when no root was returned (e.g. registration
 *  failed — we still try to scan using the path directly). */
function getRootId(flushResult: FlushResult, path: string): string {
  const row = flushResult.results.find((r) => r.path === path);
  // Successful rows carry the assigned rootId; fall back to the path if absent.
  return row?.rootId ?? path;
}

// ── StepScan ─────────────────────────────────────────────────────────────────

/**
 * Step 5 — Scan.
 *
 * Runs inbox_scan_folder + inbox_classify per registered source and shows a
 * per-source detection summary.  Ingestion-group approval stays in the Inbox;
 * Finish navigates there without calling inbox_confirm.
 *
 * Back and Finish buttons are rendered by the shared WizardShell footer in
 * SetupWizard.  StepScan notifies the parent of scan completion via
 * `onAllDoneChange` so the footer can enable/disable the Finish button.
 *
 * TanStack Query (#615/#630) replaces the former manual `cancelled`-flag
 * orchestration. Query-cache identity (keyed per source) is what now protects
 * against React StrictMode's mount→cleanup→remount double-invoke: the
 * remounted observer subscribes to the SAME in-flight/cached query instead of
 * starting a second scan, which is what a re-entry ref-guard was trying (and
 * failing) to achieve by hand — see git history for that bug.
 */
export function StepScan({
  sources,
  flushResult,
  onAllDoneChange,
}: StepScanProps) {
  // Issue #916: a wizard retry after a partial batch-registration failure
  // resubmits *every* source, not just the ones that failed. A source that
  // registered successfully on an earlier attempt in this same session
  // therefore comes back from flushToDB as `alreadyRegistered` (duplicate) on
  // the retry — the exact same shape a genuinely-already-scanned
  // restart-flow source has (issue #704). flushResult alone can't tell the
  // two apart: one has already been scanned and is safe to skip, the other
  // has never been scanned and must not be dropped. Resolve the real root via
  // roots.list and use its `lastScanned` (only set once inbox.scan_folder has
  // actually run for that root — see roots.rs) as the ground truth.
  const needsResolution = useMemo(
    () =>
      sources.filter((s) => {
        if (!s.path) return false;
        const row = flushResult.results.find((r) => r.path === s.path);
        return row?.alreadyRegistered ?? false;
      }),
    [sources, flushResult],
  );

  const rootsResolveQuery = useQuery({
    queryKey: ['setup', 'rootsResolve'] as const,
    queryFn: async () => unwrap(await commands.rootsList()),
    enabled: needsResolution.length > 0,
    retry: false,
  });

  const resolvedRoots = useMemo(() => {
    const map = new Map<string, { id: string; lastScanned: string | null }>();
    for (const root of rootsResolveQuery.data ?? []) {
      map.set(root.path, {
        id: root.id,
        lastScanned: root.lastScanned ?? null,
      });
    }
    return map;
  }, [rootsResolveQuery.data]);

  // Mirrors the former `resolved` flag: true once the alreadyRegistered
  // resolution has settled (success or failure) or was never needed.
  const resolutionReady =
    needsResolution.length === 0 || rootsResolveQuery.isFetched;

  // Issue #704 / #916: only scan folders this flush actually registered, or
  // that a prior same-session attempt registered but never scanned. A root
  // whose registration failed for a genuine reason (no rootId, not
  // `alreadyRegistered`) is skipped — scanning it with the path-as-rootId
  // fallback fails the registered_sources JOIN and would insert orphaned
  // inbox_items. A source genuinely already scanned in a prior session is
  // skipped entirely (not rendered at all), matching the prior behaviour.
  const renderTargets = useMemo((): RenderTarget[] | undefined => {
    if (!resolutionReady) return undefined;
    const out: RenderTarget[] = [];
    for (const s of sources) {
      if (!s.path) continue;
      const row = flushResult.results.find((r) => r.path === s.path);
      if (row?.success) {
        out.push({
          source: s,
          rootId: getRootId(flushResult, s.path),
          kind: 'scan',
        });
        continue;
      }
      if (!row?.alreadyRegistered) continue;

      const resolvedRoot = resolvedRoots.get(s.path);
      if (resolvedRoot != null && resolvedRoot.lastScanned != null) {
        continue; // genuinely already scanned — safe to skip
      }
      if (resolvedRoot != null) {
        out.push({ source: s, rootId: resolvedRoot.id, kind: 'scan' });
        continue;
      }
      // Review round 2 #1: roots.list failed or returned no match for this
      // path — we can't tell whether this source was already scanned.
      // Silently excluding it here would resurrect the exact #916 bug via a
      // resolution failure instead of a misclassification, so surface it as
      // a visible error state instead of dropping it.
      out.push({ source: s, rootId: s.path, kind: 'resolution-error' });
    }
    return out;
  }, [sources, flushResult, resolvedRoots, resolutionReady]);

  const scanTargets = useMemo(
    () => (renderTargets ?? []).filter(isScanTarget),
    [renderTargets],
  );

  // One query per source (StrictMode-safe dedup via the shared query cache —
  // see the StepScan doc comment above). `retry`/`refetchOnWindowFocus` are
  // disabled: a scan is a one-shot wizard action, not a resource to silently
  // re-fetch or retry behind the user's back.
  const scanQueries = useQueries({
    queries: scanTargets.map((t) => ({
      queryKey: ['setup', 'scan', t.rootId, t.source.path] as const,
      queryFn: () => scanAndClassify(t.rootId, t.source.path),
      retry: false,
      refetchOnWindowFocus: false,
    })),
  });

  const sourceStates: SourceScanState[] = useMemo(() => {
    const scanResultByPath = new Map(
      scanTargets.map((t, i) => [t.source.path, scanQueries[i]]),
    );
    return (renderTargets ?? []).map((t): SourceScanState => {
      if (t.kind === 'resolution-error') {
        return {
          source: t.source,
          rootId: t.rootId,
          phase: 'error',
          items: [],
          classifications: new Map(),
          classifyFailures: new Set(),
          error: m.setup_scan_resolution_failed(),
        };
      }
      const q = scanResultByPath.get(t.source.path);
      return {
        source: t.source,
        rootId: t.rootId,
        phase: q ? queryPhase(q) : 'pending',
        items: q?.data?.items ?? [],
        classifications: q?.data?.classifications ?? new Map(),
        classifyFailures: q?.data?.classifyFailures ?? new Set(),
        error: q?.error ? errMessage(q.error) : undefined,
      };
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [renderTargets, scanTargets, scanQueries]);

  const resolved = resolutionReady;
  const allDone =
    resolved &&
    sourceStates.every((s) => s.phase === 'done' || s.phase === 'error');
  const totalDetected = sourceStates.reduce(
    (acc, s) => acc + s.items.length,
    0,
  );

  // Notify parent whenever the allDone state changes so the footer can
  // enable/disable the Finish button.
  useEffect(() => {
    onAllDoneChange(allDone);
  }, [allDone, onAllDoneChange]);

  return (
    <div className="pv-setup-scan" data-testid="step-scan">
      {!resolved ? null : sourceStates.length === 0 ? (
        // Review round 2 #2: gated on `resolved` (not sourceStates.length
        // alone) so this genuinely-empty message can't flash for real
        // sources while the alreadyRegistered resolution above is pending.
        <p className="pv-setup-scan__empty-msg">{m.setup_scan_no_sources()}</p>
      ) : (
        <>
          {/* Per-source results — scrollable region; expanding accordions scroll here,
              not the whole wizard page. */}
          <div className="pv-setup-scan__scroll">
            {sourceStates.map((s) => (
              <SourceSummary key={s.source.path} state={s} />
            ))}
          </div>

          {/* Summary line (shown when all sources are complete) */}
          {allDone && (
            <p
              data-testid="scan-summary"
              className="pv-setup-scan__summary"
              role="status"
              aria-live="polite"
              aria-atomic="true"
            >
              {totalDetected > 0
                ? m.setup_scan_summary_found({ count: totalDetected })
                : m.setup_scan_summary_empty()}
            </p>
          )}
        </>
      )}
    </div>
  );
}
