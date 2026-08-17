// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Detection-facts property rows for the Inbox detail pane (#994, extracted
 * from InboxDetail.tsx), plus the two-column spread the pane renders them in.
 */

import type {
  InboxFileMetadata_Serialize as InboxFileMetadata,
  InboxItemSummary,
} from '@/bindings/index';
import type { PropertyDef } from '@/components';
import { fieldApplicability } from '@/lib/field-applicability';
import { m } from '@/lib/i18n';
import { formatExposureSeconds } from './inboxDetailHelpers';
import type { InboxClassifyResponse } from './store';

export interface BuildDetectionPropsArgs {
  item: InboxItemSummary;
  classification: InboxClassifyResponse | null;
  classType: string;
  /** Representative file for FITS metadata display (best-effort). */
  repFile: InboxFileMetadata | null;
  itemFrameType: string | null;
}

// biome-ignore lint/complexity/noExcessiveCognitiveComplexity: flat mapping from classification evidence to detection display props
export function buildDetectionProps({
  item,
  classification,
  classType,
  repFile,
  itemFrameType,
}: BuildDetectionPropsArgs): PropertyDef[] {
  return [
    {
      key: 'classification',
      label: m.inbox_prop_classification(),
      value:
        classType === 'single_type'
          ? (classification?.frameType ?? 'single_type')
          : classType,
    },
    {
      key: 'files',
      label: m.inbox_col_files(),
      // #653: the breakdown only tallies CLASSIFIED files — it excludes
      // `unclassifiedFiles`, so a needs-review item (the one a user is most
      // likely scrutinizing) undercounted here vs the list row's total
      // `fileCount`. Add the unclassified count back in before falling back.
      value: classification
        ? String(
            (classification.breakdown?.reduce((s, e) => s + e.count, 0) ?? 0) +
              (classification.unclassifiedFiles?.length ?? 0) || item.fileCount,
          )
        : String(item.fileCount),
    },
    // Rows below are always present (never conditionally omitted for a
    // missing value — that collapsed "missing" into "not-applicable", spec-030
    // Q16 / FR-135); applicability per frame type comes from the shared
    // `fieldApplicability` matrix (data-model.md), so an applicable-but-absent
    // field renders the unresolved chip instead of silently vanishing.
    {
      key: 'target',
      label: m.inbox_dim_target(),
      value: repFile?.object ?? null,
      source: 'fits',
      applicability: fieldApplicability(itemFrameType, 'target'),
    },
    {
      key: 'filter',
      label: m.common_filter(),
      value: repFile?.filter ?? null,
      source: 'fits',
      applicability: fieldApplicability(itemFrameType, 'filter'),
    },
    {
      key: 'exposure',
      label: m.inbox_col_exposure(),
      value:
        repFile?.exposureS != null
          ? formatExposureSeconds(repFile.exposureS)
          : null,
      source: 'fits',
      applicability: fieldApplicability(itemFrameType, 'exposure'),
    },
    {
      key: 'binning',
      label: m.settings_calmatch_binning(),
      value:
        repFile?.binningX != null || repFile?.binningY != null
          ? `${repFile?.binningX ?? '?'}x${repFile?.binningY ?? '?'}`
          : null,
      source: 'fits',
    },
    {
      key: 'gain',
      label: m.inbox_col_gain(),
      value: repFile?.gain ?? null,
      source: 'fits',
    },
    {
      key: 'temp',
      label: m.settings_calmatch_sensor_temp(),
      value:
        repFile?.temperatureC != null ? `${repFile.temperatureC} °C` : null,
      source: 'fits',
      applicability: fieldApplicability(itemFrameType, 'setTemp'),
    },
    {
      key: 'instrume',
      label: m.inbox_field_instrument(),
      value: repFile?.instrume ?? null,
      source: 'fits',
    },
    {
      key: 'dims',
      label: m.inbox_field_dimensions(),
      value:
        repFile != null && (repFile.naxis1 != null || repFile.naxis2 != null)
          ? `${repFile.naxis1 ?? '?'}×${repFile.naxis2 ?? '?'}`
          : null,
      source: 'fits',
    },
    {
      key: 'date',
      label: m.sessions_col_night(),
      value: repFile?.dateObs ?? null,
      source: 'fits',
      applicability: fieldApplicability(itemFrameType, 'date'),
    },
  ];
}

/**
 * Spread the detection facts across two left-packed columns (the canonical
 * SessionDetail shape) so the panel reads as multi-column, not one cramped
 * stack.
 */
export function splitDetectionColumns(props: PropertyDef[]): {
  colA: PropertyDef[];
  colB: PropertyDef[];
} {
  const mid = Math.ceil(props.length / 2);
  return { colA: props.slice(0, mid), colB: props.slice(mid) };
}
