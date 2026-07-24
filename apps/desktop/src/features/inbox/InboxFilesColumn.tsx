// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * The Inbox detail pane's "Files" column (#994, extracted from
 * InboxDetail.tsx): the FR-011 mixed-composition summary, the per-file
 * metadata popover (FR-010 table + FileInspector), and the FR-032/US9
 * missing-required-attribute banner.
 *
 * Owns the popover's inspected-row state, which nothing outside this column
 * reads.
 */

import { Popover } from '@base-ui-components/react/popover';
import { Fragment, useState } from 'react';
import type { InboxFileMetadata_Serialize as InboxFileMetadata } from '@/bindings/index';
import { renderValue } from '@/components';
import { fieldApplicability } from '@/lib/field-applicability';
import { m } from '@/paraglide/messages';
import { Banner, Table } from '@/ui';
import { FileInspector } from './FileInspector';
import { basename, formatExposureSeconds } from './inboxDetailHelpers';
import {
  head as detailHead,
  muted as detailMuted,
} from '@/components/two-col-detail-layout.css';

export interface InboxFilesColumnProps {
  fileMetadata: InboxFileMetadata[] | null | undefined;
  /** FR-011 summary node, or null when the item is not a mixed folder. */
}

export function InboxFilesColumn({ fileMetadata }: InboxFilesColumnProps) {
  // Files popover: which row is "inspected" inside the popover.
  const [inspectedIdx, setInspectedIdx] = useState<number | null>(null);

  const metadataColumns = [
    { key: 'file', label: m.inbox_col_file(), style: { minWidth: 160 } },
    { key: 'type', label: m.inbox_col_type(), style: { width: 80 } },
    { key: 'filter', label: m.common_filter(), style: { width: 70 } },
    { key: 'exposure', label: m.inbox_col_exposure(), style: { width: 80 } },
    { key: 'binning', label: m.inbox_col_binning(), style: { width: 70 } },
    { key: 'gain', label: m.inbox_col_gain(), style: { width: 60 } },
    { key: 'temp', label: m.inbox_col_temp(), style: { width: 70 } },
    { key: 'object', label: m.inbox_col_object(), style: { width: 100 } },
    { key: 'date', label: m.archive_prop_date(), style: { width: 110 } },
  ];

  // FR-032 (US9): files missing a path-load-bearing attribute.
  const filesMissingAttrs = (fileMetadata ?? []).filter(
    (f) => (f.missingPathAttributes?.length ?? 0) > 0,
  );

  // TODO(astro-plan-kyo7.37): virtualize or lazy-load — a large session folder
  // renders one row per file; deferred to the memoization bead (popover rows
  // built lazily on open).
  const metadataRows = (fileMetadata ?? []).map((f, rowIdx) => {
    const missingAttrs = f.missingPathAttributes ?? [];
    const fileName = basename(f.relativeFilePath);
    const needsAttention = f.overrideStale || missingAttrs.length > 0;
    const isInspected = inspectedIdx === rowIdx;
    return {
      file: (
        <span title={f.relativeFilePath} className="pv-inbox-detail__file-cell">
          {f.relativeFilePath}
          {missingAttrs.length > 0 && (
            <span
              data-testid={`inbox-missing-attr-${fileName}`}
              title={m.inbox_missing_attrs_title({
                attrs: missingAttrs.join(', '),
              })}
              className="pv-inbox-detail__missing-attr-badge"
            >
              {m.inbox_needs_attrs({ attrs: missingAttrs.join(', ') })}
            </span>
          )}
        </span>
      ),
      // Per-row applicability (spec-030 Q16 / FR-135, FR-137): each file in a
      // mixed folder can have its own effective frame type, so a field that
      // doesn't apply to THIS row's type renders blank while a genuinely
      // missing-but-applicable value renders the unresolved chip — never the
      // same dash for both.
      type: renderValue(f.frameTypeEffective ?? null, {
        applicability: 'applicable',
      }),
      filter: renderValue(f.filter ?? null, {
        applicability: fieldApplicability(f.frameTypeEffective, 'filter'),
      }),
      exposure: renderValue(
        f.exposureS ?? null,
        { applicability: fieldApplicability(f.frameTypeEffective, 'exposure') },
        (v) => formatExposureSeconds(Number(v)),
      ),
      binning: renderValue(
        f.binningX != null || f.binningY != null
          ? `${f.binningX ?? '?'}x${f.binningY ?? '?'}`
          : null,
        { applicability: 'applicable' },
      ),
      gain: renderValue(f.gain ?? null, { applicability: 'applicable' }),
      temp: renderValue(
        f.temperatureC ?? null,
        { applicability: fieldApplicability(f.frameTypeEffective, 'setTemp') },
        (v) => `${v} °C`,
      ),
      object: renderValue(f.object ?? null, {
        applicability: fieldApplicability(f.frameTypeEffective, 'target'),
      }),
      date: renderValue(f.dateObs ?? null, { applicability: 'applicable' }),
      _rowClassName: [
        needsAttention ? 'pv-inbox-meta-row--warn' : '',
        isInspected ? 'pv-inbox-meta-row--inspected' : '',
        'pv-inbox-meta-row',
      ]
        .filter(Boolean)
        .join(' '),
      _onClick: () => setInspectedIdx(isInspected ? null : rowIdx),
    };
  });

  const hasMetadata = metadataRows.length > 0;

  return (
    <Fragment>
      <div className={detailHead}>{m.inbox_col_files()}</div>

      {/* Files popover — trigger + portaled popup with metadata table + inspector */}
      {hasMetadata ? (
        <Popover.Root
          onOpenChange={() => {
            // Reset inspector selection whenever the popover is closed.
            setInspectedIdx(null);
          }}
        >
          <Popover.Trigger
            className="pv-inbox-detail__files-trigger"
            aria-label={m.inbox_file_metadata_count({
              count: metadataRows.length,
            })}
            data-testid="inbox-files-popover-trigger"
          >
            {m.inbox_file_metadata_count({
              count: metadataRows.length,
            })}{' '}
            ▾
          </Popover.Trigger>
          <Popover.Portal>
            <Popover.Positioner side="bottom" align="start" sideOffset={4}>
              <Popover.Popup
                className="pv-inbox-detail__files-popup"
                data-testid="inbox-files-popup"
                aria-label={m.inbox_file_metadata_aria()}
              >
                {/* Scrollable metadata table */}
                <div className="pv-inbox-detail__files-popup-table">
                  <Table columns={metadataColumns} rows={metadataRows} />
                </div>
                {/* Inspector — updates on row click */}
                {inspectedIdx != null && (
                  <div className="pv-inbox-detail__files-popup-inspector">
                    <FileInspector
                      file={fileMetadata?.[inspectedIdx] ?? null}
                    />
                  </div>
                )}
              </Popover.Popup>
            </Popover.Positioner>
          </Popover.Portal>
        </Popover.Root>
      ) : (
        <span className={detailMuted}>
          {m.inbox_no_file_metadata()}
          {/* #551: no per-file metadata means the required-destination-
            attribute gate has no data to evaluate here — say so
            explicitly instead of silently reading as "nothing to
            worry about" (confirm can still be rejected server-side
            for these files; see inbox.missing_path_attributes). */}
          {' — '}
          {m.inbox_no_file_metadata_caveat()}
        </span>
      )}

      {/* FR-032 (US9) / #554: missing-required-attribute warning lives
      INLINE in the Files column (the field it explains) rather than
      as its own full-width alert column competing with the property
      tables (#554 — "stands out horribly"). */}
      {filesMissingAttrs.length > 0 && (
        <Banner
          variant="danger"
          className="pv-inbox-detail__banner-mt2 pv-inbox-alert"
          data-testid="inbox-missing-attr-banner"
        >
          <div className="pv-inbox-alert__msg">
            <span className="pv-inbox-alert__title">
              {m.inbox_required_metadata_missing_title()}
            </span>
            <span className="pv-inbox-alert__body">
              {m.inbox_required_metadata_body({
                count: filesMissingAttrs.length,
              })}
            </span>
          </div>
        </Banner>
      )}
    </Fragment>
  );
}
