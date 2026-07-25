// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * ArchivePage — spec 017 WP-B (real archive backend) on the spec 043
 * SINGLE-COLUMN list-page layout.
 *
 * Alignment pass over the #401-shipped page (which used the deprecated
 * two-pane `ListDetailLayout` + `ListSidebar`): the page now follows the
 * shared list-page system every other list page uses —
 *
 *   - pinned `PageTopBar`: search (wired) + the selected entry's actions on
 *     the right. NO title / NO summary / NO duplicated counts (top-bar
 *     convention, task #80 — the sidebar footer count + subtitle count of the
 *     old layout are gone).
 *   - full-width sortable `ArchiveTable` (shared Table + SortHeader).
 *   - single-column `ArchiveDetail` docked BELOW on selection (spec 043 §4
 *     Archive: no rail; the panel mounts only when an entry is selected — no
 *     empty centered dashboard).
 *
 * Management actions stay in the top bar per spec 043 §4 Archive (Send to
 * trash / Delete permanently / Reveal). Delete permanently keeps the typed
 * "DELETE" confirmation modal gating `archive.permanently_delete`.
 */

import { useRef, useState } from 'react';
import { useNavigate, useSearch } from '@tanstack/react-router';
import {
  ListPageLayout,
  PageTopBar,
  FilterToolbar,
  Modal,
  TableStateGate,
} from '@/components';
import { Btn, EmptyState } from '@/ui';
import { m } from '@/lib/i18n';
import { useStaleSelectionCleanup } from '@/lib/use-stale-selection';
import { revealLabel } from '@/lib/reveal-label';
import { revealInOs } from '@/shared/native/reveal';
import { addToast } from '@/shared/toast';
import { queryClient as sharedQueryClient } from '@/data/queryClient';
import { queryKeys } from '@/data/queryKeys';
import { PlanReviewOverlay } from '@/features/plans/PlanReviewOverlay';
import { ArchiveDetail } from './ArchiveDetail';
import { ArchiveTable, DEFAULT_ARCHIVE_SORT } from './ArchiveTable';
import type { ArchiveSort, ArchiveSortCol } from './ArchiveTable';
import {
  useArchiveList,
  useSendToTrash,
  usePermanentlyDelete,
  useGenerateRestorePlan,
} from './store';
import type { ArchiveEntry } from '@/bindings/index';

// Backend safety gate for `archive.permanently_delete` (spec 017 WP-B):
// the literal confirmation string the user must type before the action
// unlocks.
const DELETE_CONFIRM_TEXT = 'DELETE';

/** Client-side text search across name / reason / original path. */
function filterEntries(entries: ArchiveEntry[], query: string): ArchiveEntry[] {
  const q = query.trim().toLowerCase();
  if (!q) return entries;
  return entries.filter(
    (a) =>
      a.name.toLowerCase().includes(q) ||
      (a.reason?.toLowerCase().includes(q) ?? false) ||
      a.originalPath.toLowerCase().includes(q),
  );
}

export function ArchivePage() {
  const { selected } = useSearch({ from: '/shell/archive' });
  const navigate = useNavigate({ from: '/archive' });
  const { data: entries = [], loading, error } = useArchiveList();

  const [search, setSearch] = useState('');
  const [sort, setSort] = useState<ArchiveSort>(DEFAULT_ARCHIVE_SORT);
  const filtered = filterEntries(entries, search);

  const item: ArchiveEntry | null =
    entries.find((a) => a.id === selected) ?? null;

  const sendToTrash = useSendToTrash();
  const permanentlyDelete = usePermanentlyDelete();
  const generateRestorePlan = useGenerateRestorePlan();
  const [restoreReviewPlanId, setRestoreReviewPlanId] = useState<string | null>(
    null,
  );
  const [deleteModalOpen, setDeleteModalOpen] = useState(false);
  const [confirmInput, setConfirmInput] = useState('');
  // #841: point the destructive confirm modal's initial focus directly at the
  // confirm input instead of racing Base UI's own default (first tabbable =
  // the ✕ close button) with a bare `autoFocus`.
  const confirmInputRef = useRef<HTMLInputElement>(null);

  // #735: gated on `loading` so a cold reload's empty cache isn't mistaken for
  // a stale id — see ProjectsPage for the full rationale.
  useStaleSelectionCleanup(selected, loading || item !== null, () => {
    void navigate({
      search: (prev) => ({ ...prev, selected: undefined }),
      replace: true,
    });
  });

  const onSelect = (id: string) =>
    navigate({ search: (prev) => ({ ...prev, selected: id }) });
  const clearSelection = () =>
    navigate({
      search: (prev) => ({ ...prev, selected: undefined }),
      replace: true,
    });

  const handleSort = (col: ArchiveSortCol) =>
    setSort((prev) =>
      prev.col === col
        ? { col, dir: prev.dir === 'asc' ? 'desc' : 'asc' }
        : { col, dir: 'asc' },
    );

  const closeDeleteModal = () => {
    setDeleteModalOpen(false);
    setConfirmInput('');
  };

  const handleSendToTrash = () => {
    if (!item?.archivedViaPlanId) return;
    sendToTrash.mutate(item.archivedViaPlanId);
  };

  /**
   * Generate a reviewable restore (un-archive) plan (#885, decision D15) and
   * open the shared {@link PlanReviewOverlay} for review + apply — mirrors
   * ProjectDetail's `handleGenerateArchivePlan`, the plan-gated entry point
   * for the reverse edge.
   */
  const handleGenerateRestorePlan = () => {
    if (!item?.archivedViaPlanId) return;
    generateRestorePlan.mutate(
      {
        archivedViaPlanId: item.archivedViaPlanId,
        entityType: item.entityType,
      },
      {
        onSuccess: (res) => {
          addToast({
            message: m.archive_restore_plan_created_toast({
              count: res.itemCount,
            }),
            variant: 'info',
          });
          setRestoreReviewPlanId(res.planId);
        },
        onError: () => {
          addToast({
            message: m.archive_restore_generate_failed(),
            variant: 'error',
          });
        },
      },
    );
  };

  /** After the restore plan applies, the project leaves `archived` (backend
   * `finalize_restore_lifecycle`) — refresh the list so the row drops. */
  const handleRestorePlanApplied = () => {
    void sharedQueryClient.invalidateQueries({
      queryKey: queryKeys.archive.list(),
    });
    void clearSelection();
  };

  /** Reveal the app-managed archive folder in the OS file manager (#874). */
  const handleReveal = async () => {
    if (!item?.archiveFolderPath) return;
    try {
      await revealInOs(item.archiveFolderPath, {
        entityKind: 'other',
        entityId: item.id,
      });
    } catch (err: unknown) {
      const msg =
        typeof err === 'string'
          ? err
          : ((err as Error)?.message ?? m.archive_load_error());
      addToast({ message: msg, variant: 'error' });
    }
  };

  const handleConfirmDelete = () => {
    if (!item?.archivedViaPlanId || confirmInput !== DELETE_CONFIRM_TEXT)
      return;
    permanentlyDelete.mutate(item.archivedViaPlanId, {
      onSuccess: closeDeleteModal,
    });
  };

  // Top bar: search + the selected entry's management actions (spec 043 §4
  // Archive keeps these in the TOP BAR, unlike Sessions' detail-header set).
  const topBar = (
    <PageTopBar
      filters={
        <FilterToolbar
          search={{
            value: search,
            onChange: setSearch,
            placeholder: m.archive_search_placeholder(),
            ariaLabel: m.archive_search_placeholder(),
          }}
        />
      }
      actions={
        item && (
          <>
            {/* Restore (#756/#885, spec 043 §4 order): generates a
                reviewable un-archive plan and opens the same review/apply
                overlay the Archive-generation edge uses. Label picks the
                entity-specific wording (#886 added the master row kind). */}
            <Btn
              size="sm"
              disabled={
                !item.archivedViaPlanId || generateRestorePlan.isPending
              }
              onClick={handleGenerateRestorePlan}
              data-testid="archive-restore-btn"
            >
              {item.entityType === 'master'
                ? m.archive_restore_master_btn()
                : m.archive_restore_project_btn()}
            </Btn>
            <Btn
              size="sm"
              variant="destructive"
              disabled={!item.archivedViaPlanId || sendToTrash.isPending}
              onClick={handleSendToTrash}
            >
              {m.archive_send_to_trash_btn()}
            </Btn>
            <Btn
              size="sm"
              variant="destructive"
              disabled={!item.archivedViaPlanId}
              onClick={() => setDeleteModalOpen(true)}
            >
              {m.archive_delete_permanently_btn()}
            </Btn>
            {/* Reveal (#874): enabled once the backend resolves the
                app-managed archive folder (`.astro-plan-archive/<planId>/`)
                for this entry; stays disabled — same STUB tooltip — when the
                owning plan has no items to derive a folder from. Label is
                the shared platform-native revealLabel() (File Explorer /
                Finder / file manager). */}
            <Btn
              size="sm"
              disabled={!item.archiveFolderPath}
              title={
                item.archiveFolderPath
                  ? undefined
                  : m.archive_reveal_unavailable_title()
              }
              onClick={() => void handleReveal()}
              data-testid="archive-reveal-btn"
            >
              {revealLabel()}
            </Btn>
          </>
        )
      }
    />
  );

  return (
    <>
      <ListPageLayout
        topBar={topBar}
        dockId="archive"
        detailLabel={m.common_details()}
        detail={item != null ? <ArchiveDetail item={item} /> : undefined}
        onCloseDetail={item != null ? () => void clearSelection() : undefined}
      >
        <TableStateGate
          loading={loading}
          error={error ? error.message : null}
          isEmpty={entries.length === 0}
          isFilteredEmpty={filtered.length === 0}
          skeletonLabel={m.common_loading()}
          errorEmpty={<EmptyState title={m.archive_load_error()} />}
          empty={
            <EmptyState
              title={m.archive_empty_title()}
              desc={m.archive_empty_desc()}
            />
          }
          filteredEmpty={<EmptyState title={m.archive_no_match()} />}
        >
          <ArchiveTable
            entries={filtered}
            selected={selected ?? null}
            onSelect={onSelect}
            sort={sort}
            onSort={handleSort}
          />
        </TableStateGate>
      </ListPageLayout>

      {item && (
        <Modal
          open={deleteModalOpen}
          onClose={closeDeleteModal}
          title={m.archive_delete_permanently_confirm_title()}
          size="sm"
          ariaLabel={m.archive_delete_permanently_confirm_title()}
          initialFocus={confirmInputRef}
          footer={
            <>
              <Btn
                variant="ghost"
                onClick={closeDeleteModal}
                disabled={permanentlyDelete.isPending}
              >
                {m.common_cancel()}
              </Btn>
              <Btn
                variant="destructive"
                disabled={
                  confirmInput !== DELETE_CONFIRM_TEXT ||
                  permanentlyDelete.isPending
                }
                onClick={handleConfirmDelete}
              >
                {m.archive_delete_permanently_btn()}
              </Btn>
            </>
          }
        >
          <p>
            {m.archive_delete_permanently_confirm_desc({ name: item.name })}
          </p>
          <input
            ref={confirmInputRef}
            className="pv-input"
            type="text"
            value={confirmInput}
            onChange={(e) => setConfirmInput(e.target.value)}
            placeholder={DELETE_CONFIRM_TEXT}
            aria-label={m.archive_delete_permanently_confirm_aria()}
          />
        </Modal>
      )}

      {/* Restore plan review overlay (#885): shares the same review → approve
          → apply kit every other plan-gated flow uses. */}
      <PlanReviewOverlay
        planId={restoreReviewPlanId}
        open={restoreReviewPlanId !== null}
        onClose={() => setRestoreReviewPlanId(null)}
        title={m.archive_restore_review_title()}
        onApplied={handleRestorePlanApplied}
        onRetryCreated={setRestoreReviewPlanId}
      />
    </>
  );
}
