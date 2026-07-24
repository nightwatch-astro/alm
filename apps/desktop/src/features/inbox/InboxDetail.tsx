// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * InboxDetail — bottom pane for the Inbox classify/confirm workflow.
 *
 * Follows the SessionDetail shape (spec 043 §4 redesign):
 *
 *   HEADER — item path (title, bold) + titleExtra: classification pill +
 *             inline action buttons (left-packed, pv-session-detail2__actions).
 *             Confirm lives HERE; disabled for "mixed" rows (spec 041 FR-050 —
 *             the backend "split" action is removed, T071/T072).
 *   BODY   — scrollable (#553) left-packed .pv-session-detail2 flex row:
 *     col A (PropertyTable) — classification + file-count + FITS metadata
 *     mixed-summary line   — compact "N light · M dark" muted text (mixed only)
 *     "Files (N) ▾" popover trigger — opens a portaled popup containing:
 *       • scrollable per-file metadata table
 *       • FileInspector detail for the clicked row
 *       • FR-032/#554 missing-required-attribute banner, inline right below
 *         the trigger it explains (not a separate full-width alert column)
 *     "Needs review" col  — unclassified-file type-override editing (if any)
 *
 * No facts/aux props on DetailPanel — body is fully self-contained.
 * No breakdown-filter interaction — the breakdown table is gone from the detail.
 */

import { useCallback, useMemo } from 'react';
import type {
  InboxFileMetadata_Serialize as InboxFileMetadata,
  InboxItemSummary,
  InboxReclassifyV2Response_Serialize as InboxReclassifyV2Response,
} from '@/bindings/index';
import { DetailPanel, PropertyTable, TwoColDetailLayout } from '@/components';
import { m } from '@/lib/i18n';
import { revealLabel } from '@/lib/reveal-label';
import { copyToClipboard, revealInOs } from '@/shared/native/reveal';
import { addToast } from '@/shared/toast';
import { Banner, Btn, Pill } from '@/ui';
import type { InboxClassifyResponse } from './store';
import { ConeSearchSuggestions } from './ConeSearchSuggestions';
import {
  buildDetectionProps,
  splitDetectionColumns,
} from './inboxDetectionProps';
import { InboxNeedsReview } from './InboxNeedsReview';
import { InboxFilesColumn } from './InboxFilesColumn';
import { useInboxReclassifyState } from './useInboxReclassifyState';
import {
  applicableRootCategory,
  basename,
  buildRootLabels,
  classificationVariant,
  FRAME_TYPE_OPTIONS,
  humanizeKey,
  resolveInboxRevealPath,
} from './inboxDetailHelpers';
import { selectBase } from '@/styles/select.css';

// ── Props ─────────────────────────────────────────────────────────────────────

export interface InboxDetailProps {
  // #768: `organizationState` isn't on the base `InboxItemSummary` (it's an
  // `InboxListItem` extension) — widened here so the picker-gate below can
  // read it without every other InboxDetail caller needing a full InboxListItem.
  item: InboxItemSummary & { organizationState?: string };
  rootAbsolutePath: string;
  classification: InboxClassifyResponse | null;
  /**
   * Per-file metadata from `inbox.item.metadata` (FR-010).
   * Optional — rendered when provided and non-empty.
   */
  fileMetadata?: InboxFileMetadata[];
  /**
   * Primary confirm action for this detection — runs the confirm/split flow.
   * For a mixed folder the page labels this "Generate split plan"; otherwise
   * "Confirm to inventory". Rendered inline-left in the detail header (the
   * canonical Sessions placement). Omit to hide the button.
   */
  onConfirm?: () => void;
  /** Header confirm label, e.g. "Confirm to inventory" / "Generate split plan". */
  confirmLabel?: string;
  /** Disable the confirm action (gated: unclassified / missing attrs / open plan). */
  confirmDisabled?: boolean;
  /** True while a confirm/split is in flight — shows "Working…" + disables. */
  confirmBusy?: boolean;
  /**
   * Candidate destination library roots (non-inbox). When more than one
   * exists, a "Source" picker is shown next to Confirm so the user chooses
   * WHERE files are placed instead of relying on auto-selection.
   */
  destinationRoots?: Array<{ id: string; path: string; category: string }>;
  /** Currently chosen destination root id; "" / null means auto-select. */
  selectedRootId?: string | null;
  /** Called when the user picks a destination root ("" = auto). */
  onSelectRoot?: (rootId: string) => void;
  /**
   * Called with the raw `inbox.reclassify_v2` response after EITHER the
   * per-file or bulk override flow succeeds. `reclassify_v2` operates at
   * source-group scope and re-splits the group into new single-type
   * sub-items (R-14, issue #755) — the CURRENTLY selected `item.inboxItemId`
   * may cease to exist. The caller (InboxPage) owns selection and is
   * responsible for moving it to the correct post-split item once the
   * refetched list contains it; this component does not track selection.
   */
  onReclassified?: (response: InboxReclassifyV2Response) => void;
  /**
   * Stable `inbox_source_groups` id for this item's folder, when known.
   * Preferred over `item.inboxItemId` for `inbox.reclassify_v2` because
   * sub-item ids are purged/recreated across re-splits (see
   * `useInboxReclassifyV2`); `null`/absent falls back to the item id.
   */
  sourceGroupId?: string | null;
}

// ── Component ─────────────────────────────────────────────────────────────────

export function InboxDetail({
  item,
  rootAbsolutePath,
  classification,
  fileMetadata,
  onConfirm,
  confirmLabel = m.inbox_confirm_to_inventory(),
  confirmDisabled = false,
  confirmBusy = false,
  destinationRoots,
  selectedRootId,
  onSelectRoot,
  onReclassified,
  sourceGroupId,
}: InboxDetailProps) {
  const reclassify = useInboxReclassifyState({
    inboxItemId: item.inboxItemId,
    rootAbsolutePath,
    sourceGroupId,
    classification,
    fileMetadata,
    onReclassified,
  });
  // Everything else this component still renders itself; the bulk-override
  // surface reads the rest straight off `reclassify` (see InboxNeedsReview).
  // `filesNeedingReview` is #1114's union gate, which the hook now owns.
  const {
    filesNeedingReview,
    pendingOverrides,
    handleOverrideChange,
    selectedFiles,
    handleToggleFile,
  } = reclassify;

  const title = item.relativePath || m.inbox_list_root_label();
  const classType = classification?.type ?? 'pending';

  // #1114: the mandatory attributes still blocking promotion, frame type
  // excluded. `frameType` is dropped because it has its own dedicated banner
  // copy and its own dropdown affordance — once the user has supplied it, the
  // remaining keys are what the banner must name. Empty (either nothing is
  // missing, or only the frame type is) falls back to the frame-type banner.
  const blockingAttrsExcludingFrameType = useMemo(() => {
    const keys = new Set<string>();
    for (const f of fileMetadata ?? []) {
      for (const k of f.missingMandatory ?? []) {
        if (k !== 'frameType') keys.add(k);
      }
    }
    return Array.from(keys);
  }, [fileMetadata]);
  const blockingAttrsLabel = blockingAttrsExcludingFrameType
    .map(humanizeKey)
    .join(', ');

  // Reveal this item's location in the OS file browser (#715, spec 004
  // FR-005/SC-002). No connectivity gate like Sessions (#889): the Inbox
  // root is the currently-scanned root, always reachable while this pane is
  // shown. On failure, the toast carries a "Copy path" action (#717 FR-010:
  // `copyToClipboard` was exported with zero call sites anywhere in the app).
  const handleReveal = useCallback(async () => {
    const revealPath = resolveInboxRevealPath(
      rootAbsolutePath,
      item.relativePath,
    );
    try {
      await revealInOs(revealPath, {
        entityKind: 'inbox_item',
        entityId: item.inboxItemId,
      });
    } catch {
      addToast({
        message: m.inbox_toast_reveal_error(),
        variant: 'error',
        action: {
          label: m.common_copy_path(),
          onClick: () => {
            void copyToClipboard(revealPath).then((ok) => {
              if (ok) {
                addToast({ message: m.common_path_copied(), variant: 'info' });
              }
            });
          },
        },
      });
    }
  }, [rootAbsolutePath, item.relativePath, item.inboxItemId]);

  // Library picker (point 1): narrow the destination roots to the category that
  // can receive this item's frame type. Only meaningful for a single-type item
  // (or a master); mixed/unknown → show all roots. The picker is shown only when
  // MORE THAN ONE applicable root exists (otherwise there's nothing to choose).
  //
  // #768: an item sourced from an ORGANIZED root is always catalogued in
  // place — confirm ignores any chosen destination and reuses the source
  // root server-side — so there is never a real destination choice to make.
  // No applicable roots means no picker, regardless of how many other
  // library roots are registered.
  const itemFrameType =
    classType === 'single_type'
      ? (classification?.frameType ?? null)
      : item.isMaster
        ? (item.masterFrameType ?? null)
        : null;
  const applicableCategory = applicableRootCategory(itemFrameType);
  const applicableRoots =
    item.organizationState === 'organized'
      ? []
      : applicableCategory && destinationRoots
        ? destinationRoots.filter((r) => r.category === applicableCategory)
        : (destinationRoots ?? []);
  const rootLabels = buildRootLabels(applicableRoots);

  const unclassifiedColumns = [
    { key: 'select', label: '', style: { width: 36 } },
    { key: 'file', label: m.inbox_col_file(), style: { width: 160 } },
    { key: 'override', label: m.inbox_col_assign_frame_type() },
  ];

  const unclassifiedRows = filesNeedingReview.map((filePath, idx) => ({
    select: (
      <input
        type="checkbox"
        checked={selectedFiles.has(filePath)}
        onChange={() => handleToggleFile(filePath)}
        aria-label={m.inbox_select_file_aria({ file: filePath })}
        data-testid={`reclassify-select-${idx}`}
      />
    ),
    file: (
      <span title={filePath} className="pv-inbox-detail__file-cell">
        {filePath}
      </span>
    ),
    override: (
      <select
        value={pendingOverrides[filePath] ?? ''}
        onChange={(e) => handleOverrideChange(filePath, e.target.value)}
        aria-label={m.inbox_override_frame_type_aria({ file: filePath })}
        data-testid={`override-select-${filePath}`}
        className={selectBase}
      >
        <option value="">{m.inbox_pick_type_placeholder()}</option>
        {FRAME_TYPE_OPTIONS.map((t) => (
          <option key={t} value={t}>
            {t}
          </option>
        ))}
      </select>
    ),
  }));

  // ── Detection property table (col A) ─────────────────────────────────────
  const repFile = fileMetadata?.[0] ?? null;
  const { colA: detColA, colB: detColB } = splitDetectionColumns(
    buildDetectionProps({
      item,
      classification,
      classType,
      repFile,
      itemFrameType,
    }),
  );

  // ── Inline header actions ─────────────────────────────────────────────────

  const titleActions = (
    <span className="pv-session-detail2__actions">
      <Pill variant={classificationVariant(classType)}>
        {classType === 'single_type'
          ? (classification?.frameType ?? m.inbox_detail_single_fallback())
          : classType}
      </Pill>
      {item.lane === 'video' && (
        <Pill variant="ghost">{m.inbox_lane_video()}</Pill>
      )}
      {onConfirm && (
        <Btn
          size="sm"
          variant="primary"
          onClick={onConfirm}
          disabled={confirmDisabled || confirmBusy}
          aria-label={confirmLabel}
          // Issue #747: lets assistive tech announce the C binding on the
          // control it activates, not only in the list's visual hint.
          aria-keyshortcuts="C"
          data-testid="inbox-confirm-btn"
        >
          {confirmBusy ? m.common_working() : confirmLabel}
        </Btn>
      )}
      {/* Destination library picker — to the RIGHT of Confirm. Lets the user
			    choose which library the files are placed in (Auto = backend pick). */}
      {onConfirm && applicableRoots.length > 1 && (
        <label className="pv-inbox-detail__dest-root">
          <span className="pv-inbox-detail__dest-root-label">
            {m.inbox_dest_root_label()}
          </span>
          <select
            className={selectBase}
            value={selectedRootId ?? ''}
            onChange={(e) => onSelectRoot?.(e.target.value)}
            aria-label={m.inbox_dest_root_aria()}
            data-testid="inbox-dest-root-select"
          >
            <option value="">{m.projects_edit_channels_auto_tag()}</option>
            {applicableRoots.map((r) => (
              <option key={r.id} value={r.id}>
                {rootLabels.get(r.id) ?? `${basename(r.path)} · ${r.category}`}
              </option>
            ))}
          </select>
        </label>
      )}
      {/* Reveal — platform-native label (shared revealLabel() helper, #715). */}
      <Btn
        size="sm"
        onClick={() => void handleReveal()}
        title={m.inbox_reveal_title()}
        data-testid="inbox-reveal-btn"
      >
        {revealLabel()}
      </Btn>
    </span>
  );

  // ── Render ────────────────────────────────────────────────────────────────

  return (
    <DetailPanel
      variant="inbox"
      title={<strong>{title}</strong>}
      titleExtra={titleActions}
    >
      {/* #553: the body (this whole subtree) is the sole scroll region — the
          header above stays pinned via the `:has()` rule in detail-panes.css.
          DetailPanel's "content-only" mode (no facts/aux slots) renders
          children with no scroll wrapper of its own, so a tall FILES/Needs-
          review body previously overflowed the docked panel's max-height and
          was clipped by `.pv-listpage__detail-body`'s `overflow: hidden`
          (unreachable, not just unscrolled). */}
      <div className="pv-inbox-detail__scroll">
        {/* Unclassified: blocking banner. #1114 — when the frame type is
            already supplied and only OTHER mandatory attributes are absent,
            name those instead of asking again for the frame type. */}
        {classType === 'unclassified' && (
          <Banner
            variant="danger"
            className="pv-inbox-detail__banner-mt3 pv-inbox-alert"
            data-testid="inbox-unclassified-alert"
          >
            <div className="pv-inbox-alert__msg">
              <span className="pv-inbox-alert__title">
                {blockingAttrsExcludingFrameType.length > 0
                  ? m.inbox_mandatory_attrs_required_title({
                      attrs: blockingAttrsLabel,
                    })
                  : m.inbox_frame_types_required_title()}
              </span>
              <span className="pv-inbox-alert__body">
                {blockingAttrsExcludingFrameType.length > 0
                  ? m.inbox_mandatory_attrs_required_body({
                      attrs: blockingAttrsLabel,
                    })
                  : m.inbox_frame_types_required_body()}
              </span>
            </div>
          </Banner>
        )}

        {!classification && (
          <div className="pv-inbox-detail__empty">
            {m.inbox_select_item_prompt()}
          </div>
        )}

        {/* Left-packed .pv-session-detail2 row, via the shared component
            (#813) instead of a hand-copied div structure. Files and Needs-
            review ride in `extraCols`, NOT `linked`: both are table-shaped
            and need `__col` sizing (flex 0 1 400px / min-width 340px) —
            `__linked` is flex 0 0 auto / min-width 160px and would squeeze
            them. */}
        <TwoColDetailLayout
          colA={<PropertyTable mode="view" showSource properties={detColA} />}
          colB={
            // Only when the fact list was long enough to spread across two.
            detColB.length > 0 ? (
              <PropertyTable mode="view" showSource properties={detColB} />
            ) : null
          }
          extraCols={[
            /* Files — mixed-composition summary + the metadata popover */
            <InboxFilesColumn key="files" fileMetadata={fileMetadata} />,
            /* Needs review — rendered when unclassified files exist */
            /* Needs review — null (not an empty component) when there is
               nothing to review: TwoColDetailLayout allocates a `__col` per
               non-null entry, so an element that merely renders null would
               still claim a column (#813 layout test). */
            unclassifiedRows.length > 0 ? (
              <InboxNeedsReview
                key="needs-review"
                reclassify={reclassify}
                itemFrameType={itemFrameType}
                columns={unclassifiedColumns}
                rows={unclassifiedRows}
              />
            ) : null,
          ]}
        />

        {/* Cone-search target suggestion (spec 052 P3) — light framesets only. */}
        {itemFrameType === 'light' && (
          <ConeSearchSuggestions framesetId={item.inboxItemId} />
        )}
      </div>
    </DetailPanel>
  );
}
