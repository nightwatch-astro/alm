// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * SessionsPage — spec 006 wired; spec 043 §4 redesign (task #36) + shared
 * layout-system adoption (tasks #62/#63/#73).
 *
 * The Sessions page is the inventory ledger and the REFERENCE adoption of the
 * shared list-page system: a pinned `PageTopBar` (title + `FilterToolbar`)
 * over a `ListPageLayout` body — a dense full-width sortable table
 * (SessionsTable) on the left and the existing SessionDetail in a bottom
 * detail pane that mounts on selection.
 *
 * Toolbar (spec 043 §4): search + Group-by control (Target / Filter / Night /
 * Camera / Month). Consistent with every list page, the table is FLAT by
 * default (a single sorted list) and grouping is opt-in.
 *
 * #652: the ledger also carries calibration (dark/flat/bias) frame groups
 * alongside light (acquisition) sessions, but the sidebar/status-bar "N
 * sessions" chrome counts acquisition-only (`d.library.sessions`,
 * `count_acquisition_sessions`). The Type field defaults to Light so the
 * table's row count matches that chrome by default; switching it surfaces
 * calibration sessions too, with the mismatch then self-explained by the
 * selected Type.
 *
 * Spec 041 FR-051 (T076, Phase 13): sessions are derived, already-confirmed
 * inventory. The review-state filter (`reviewFilter`) and the contextual
 * Confirm / Re-open / Reject / Ignore actions in the SessionDetail header were
 * removed along with the review-state machine. The Reveal action (FR-007) is
 * unrelated to the review lifecycle and is retained.
 *
 * URL state (extends spec 020):
 *   selected     — string session UUID
 *   sourceFilter — optional LibraryRoot UUID or 'all'
 */

import { useNavigate, useSearch } from '@tanstack/react-router';
import { useCallback, useMemo, useState } from 'react';
import { PageTopBar, FilterToolbar, ListPageLayout } from '@/components';
import type { FilterOption } from '@/components';
import { useStaleSelectionCleanup } from '@/lib/use-stale-selection';
import { useGrouping } from '@/lib/use-grouping';
import {
  SessionsTable,
  DEFAULT_SESSION_SORT,
  SESSION_DIM_LABELS,
} from './SessionsTable';
import type { SessionSort, SessionSortCol } from './SessionsTable';
import { SessionDetail } from './SessionDetail';
import { useInventorySources, type InventoryFilters } from './store';
import { addToast } from '@/shared/toast';
import { m } from '@/lib/i18n';
import { revealInventoryPath, resolveRevealPath } from './revealInventory';
import { isSourceActionable } from './connectivity';
import { RelationProposalList } from './RelationProposalList';
import { RelationProposalDetail } from './RelationProposalDetail';
import { ManualRelationDialog } from './ManualRelationDialog';
import { PanelGroupView } from './PanelGroupView';
import { MatchingSettingsPanel } from './MatchingSettingsPanel';
import { Btn } from '@/ui';
import type { InventoryFrameType, InventorySource } from '@/bindings/index';

/**
 * Client-side text search + field filters across the visible session fields
 * (Inbox-parity: the top toolbar carries select filters next to search; all
 * are applied here in one pass). Exported for tests.
 */
export function filterSources(
  sources: InventorySource[],
  query: string,
  filterName: string,
  camera: string,
): InventorySource[] {
  const q = query.trim().toLowerCase();
  if (!q && !filterName && !camera) return sources;
  const matches = (v: string | null | undefined) =>
    (v ?? '').toLowerCase().includes(q);
  return sources
    .map((src) => ({
      ...src,
      sessions: src.sessions.filter(
        (s) =>
          (!q ||
            matches(s.target) ||
            matches(s.name) ||
            matches(s.filter) ||
            matches(s.camera)) &&
          (!filterName || s.filter === filterName) &&
          (!camera || s.camera === camera),
      ),
    }))
    .filter((src) => src.sessions.length > 0) as InventorySource[];
}

/** Unique, sorted non-empty values of one session field → select options. Exported for tests. */
export function fieldOptions(
  sources: InventorySource[],
  pick: (s: InventorySource['sessions'][number]) => string | null | undefined,
): FilterOption[] {
  const seen = new Set<string>();
  for (const src of sources) {
    for (const s of src.sessions) {
      const v = pick(s);
      if (v) seen.add(v);
    }
  }
  return [...seen]
    .sort((a, b) => a.localeCompare(b))
    .map((v) => ({ value: v, label: v }));
}

export function SessionsPage() {
  const { selected, sourceFilter } = useSearch({
    from: '/shell/sessions',
  });
  const navigate = useNavigate({ from: '/sessions' });

  const [search, setSearch] = useState('');
  const [sort, setSort] = useState<SessionSort>(DEFAULT_SESSION_SORT);
  // Inbox-parity field filters ('' = all): optical filter + camera.
  const [filterName, setFilterName] = useState('');
  const [cameraFilter, setCameraFilter] = useState('');
  // Type field ('' = all); defaults to 'light' (acquisition) so the table's
  // row count matches the acquisition-only sidebar/status-bar count (#652).
  const [kindFilter, setKindFilter] = useState<InventoryFrameType | ''>(
    'light',
  );

  const { dims, setSlot } = useGrouping({
    storageKey: 'sessions.grouping.dims.v1',
    validIds: ['target', 'filter', 'night', 'camera', 'month'],
    defaultDims: [],
  });

  // Spec 062: proposal list/detail and panel group/mosaic side panels.
  const [selectedProposalId, setSelectedProposalId] = useState<
    string | undefined
  >(undefined);
  const [selectedPanelGroupId, setSelectedPanelGroupId] = useState<
    string | undefined
  >(undefined);
  const [showManualRelationDialog, setShowManualRelationDialog] =
    useState(false);

  // Group-by options share their labels with the table's grouping-hint footer.
  const SESSION_DIMENSIONS: FilterOption[] = Object.entries(
    SESSION_DIM_LABELS,
  ).map(([value, label]) => ({ value, label: label() }));

  // Build filters from URL params and pass directly to useInventorySources.
  const filters: InventoryFilters = {};
  if (sourceFilter && sourceFilter !== 'all')
    filters.sourceFilter = sourceFilter;
  if (kindFilter) filters.frameFilter = kindFilter;

  const { data: response, loading, error } = useInventorySources(filters);

  const sources = useMemo(
    () =>
      filterSources(response?.sources ?? [], search, filterName, cameraFilter),
    [response?.sources, search, filterName, cameraFilter],
  );

  // Filter options come from the UNFILTERED response so a pick never removes
  // the other options.
  const filterOptions = useMemo(
    () => fieldOptions(response?.sources ?? [], (s) => s.filter),
    [response?.sources],
  );
  const cameraOptions = useMemo(
    () => fieldOptions(response?.sources ?? [], (s) => s.camera),
    [response?.sources],
  );

  // (task #87) The per-page status-bar summary (session/confirmed/needs-review
  // counts) was removed: the status bar now shows GLOBAL library totals via
  // useStatusSummary, not per-route counts.

  // Flatten all sessions across sources to find the selected one.
  const allSessions = response?.sources.flatMap((src) => src.sessions) ?? [];
  const selectedSession =
    selected != null ? allSessions.find((s) => s.id === selected) : undefined;

  // Resolve the selected session's owning source for the Reveal action
  // (FR-007) and connectivity chip (#889) — sessions carry only `sourceId`;
  // the root path + connectivity state live on the source. The reveal target
  // then joins the root with the session's own frame folder (`relativePath`,
  // #567) so it opens that session's folder rather than the shared library
  // root; it falls back to the root when relativePath is null.
  const selectedSource = useMemo(
    () =>
      selectedSession != null
        ? response?.sources.find((src) => src.id === selectedSession.sourceId)
        : undefined,
    [selectedSession, response?.sources],
  );
  const revealTarget = useMemo(
    () =>
      selectedSession != null && selectedSource != null
        ? resolveRevealPath(selectedSource.path, selectedSession.relativePath)
        : undefined,
    [selectedSession, selectedSource],
  );

  // Clear stale selection when the session disappears after a filter change.
  const clearSelection = useCallback(
    () =>
      navigate({
        search: (prev) => ({ ...prev, selected: undefined }),
        replace: true,
      }),
    [navigate],
  );
  // #735: gated on `loading` so a cold reload's empty cache isn't mistaken for
  // a stale id — see ProjectsPage for the full rationale.
  useStaleSelectionCleanup(
    selected,
    loading || selectedSession !== undefined,
    clearSelection,
  );

  const onSelect = (id: string) =>
    navigate({ search: (prev) => ({ ...prev, selected: id }) });

  const handleSort = useCallback((col: SessionSortCol) => {
    setSort((prev) =>
      prev.col === col
        ? { col, dir: prev.dir === 'asc' ? 'desc' : 'asc' }
        : { col, dir: 'asc' },
    );
  }, []);

  // Reveal the session's frame folder in the OS file browser (FR-007, #567).
  const handleReveal = useCallback(async () => {
    if (!selected || !revealTarget) return;
    try {
      await revealInventoryPath({
        path: revealTarget,
        sessionId: selected,
      });
    } catch {
      addToast({ message: m.common_reveal_error(), variant: 'error' });
    }
  }, [selected, revealTarget]);

  // #889: file-touching actions are gated on the backing source being
  // actively connected — a session on a missing/disabled/reconnect-required
  // root no longer offers Reveal (the connectivity chip in the header
  // explains why).
  const revealVisible =
    revealTarget != null &&
    (selectedSource == null || isSourceActionable(selectedSource.state));

  // Top-bar convention (task #80): NO title + NO summary (the left nav names
  // the page; the count/metadata lives in the bottom status bar) and NO sort
  // control (sorting is driven by the clickable SessionsTable column headers).
  // The bar carries search + Filter/Camera field filters + Group-by —
  // Inbox-parity toolbar (spec 043 §4). (Spec 041 FR-051: the review-state
  // filter was removed along with the review lifecycle.)
  const topBar = (
    <PageTopBar
      filters={
        <FilterToolbar
          search={{
            value: search,
            onChange: setSearch,
            placeholder: m.sessions_search_placeholder(),
            ariaLabel: m.sessions_search_aria(),
          }}
          fields={[
            {
              key: 'kind',
              label: m.sessions_kind_filter_label(),
              value: kindFilter,
              options: [
                { value: 'light', label: m.sessions_kind_light_label() },
                { value: 'dark', label: m.common_dark() },
                { value: 'flat', label: m.inbox_kind_flat() },
                { value: 'bias', label: m.common_bias() },
              ],
              allLabel: m.common_all(),
              onChange: (v) => setKindFilter(v as InventoryFrameType | ''),
            },
            {
              key: 'filter',
              label: m.common_filter(),
              value: filterName,
              options: filterOptions,
              allLabel: m.common_all(),
              onChange: setFilterName,
            },
            {
              key: 'camera',
              label: m.settings_calmatch_camera(),
              value: cameraFilter,
              options: cameraOptions,
              allLabel: m.common_all(),
              onChange: setCameraFilter,
            },
          ]}
          grouping={{
            dimensions: SESSION_DIMENSIONS,
            dims,
            setSlot,
          }}
        />
      }
    />
  );

  // Spec 062: priority order for the detail pane:
  //   1. panel group / mosaic view (triggered from SessionGroupBadge)
  //   2. proposal detail (triggered from RelationProposalList)
  //   3. session detail (default)
  const panelGroupDetail =
    selectedPanelGroupId != null ? (
      <PanelGroupView panelGroupId={selectedPanelGroupId} />
    ) : undefined;

  const proposalDetail =
    selectedProposalId != null ? (
      <RelationProposalDetail proposalId={selectedProposalId} />
    ) : undefined;

  const activeDetail =
    panelGroupDetail ??
    proposalDetail ??
    (selectedSession != null ? (
      <SessionDetail
        session={selectedSession}
        onReveal={() => void handleReveal()}
        revealVisible={revealVisible}
        sourceState={selectedSource?.state}
        onOpenProject={(id) =>
          navigate({ to: '/projects', search: { selected: id } })
        }
        onOpenGroup={(id) => {
          setSelectedPanelGroupId(id);
          setSelectedProposalId(undefined);
        }}
      />
    ) : undefined);

  const handleCloseDetail = (() => {
    if (selectedPanelGroupId != null)
      return () => setSelectedPanelGroupId(undefined);
    if (selectedProposalId != null)
      return () => setSelectedProposalId(undefined);
    if (selectedSession != null) return clearSelection;
    return undefined;
  })();

  const detailAriaLabel = (() => {
    if (selectedPanelGroupId != null)
      return m.cmp_listpage_close_proposal_details_aria();
    if (selectedProposalId != null)
      return m.cmp_listpage_close_proposal_details_aria();
    return m.cmp_listpage_close_session_details_aria();
  })();

  return (
    <ListPageLayout
      topBar={topBar}
      dockId="sessions"
      data-testid="sessions-page"
      detail={activeDetail}
      onCloseDetail={handleCloseDetail}
      detailLabel={detailAriaLabel}
    >
      {error != null ? (
        <div className="pv-listtable__empty">{m.sessions_load_error()}</div>
      ) : (
        <SessionsTable
          sources={sources}
          selected={selected ?? null}
          onSelect={(id) => {
            setSelectedProposalId(undefined);
            setSelectedPanelGroupId(undefined);
            void onSelect(id);
          }}
          loading={loading}
          sort={sort}
          onSort={handleSort}
          dims={dims}
        />
      )}

      {/* Spec 062: relation proposal list — shown beneath the session table
          when no session is selected in the detail pane, or always visible
          as a secondary disclosure zone. Keyboard-operable; selecting a
          proposal opens RelationProposalDetail in the right panel. */}
      <RelationProposalList
        selectedId={selectedProposalId}
        onSelect={(id) => {
          setSelectedProposalId(id);
          // Clear session selection so the detail panel shows the proposal.
          void navigate({
            search: (prev) => ({ ...prev, selected: undefined }),
            replace: true,
          });
        }}
        headerAction={
          <Btn
            variant="ghost"
            size="sm"
            onClick={() => setShowManualRelationDialog(true)}
            aria-label={m.manual_relation_open_btn_aria()}
            data-testid="open-manual-relation-btn"
          >
            {m.manual_relation_open_btn()}
          </Btn>
        }
      />

      {/* Spec 062: manual relation creation dialog. */}
      <ManualRelationDialog
        open={showManualRelationDialog}
        onClose={() => setShowManualRelationDialog(false)}
        defaultSubjectIds={selected != null ? [selected] : undefined}
      />

      {/* Spec 062: matching geometry and calibration settings.
          TODO(ic9h.20): move this pane into SettingsPage as a new 'matching'
          pane alongside 'cal' once the Tauri commands are wired — the seam is
          SettingsPage.renderPane() in src/features/settings/SettingsPage.tsx.
          Kept here so the surface is reachable and reviewable before .20. */}
      <MatchingSettingsPanel />
    </ListPageLayout>
  );
}
