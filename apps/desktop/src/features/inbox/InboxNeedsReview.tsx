// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * The Inbox detail pane's "Needs review" column (#994, extracted from
 * InboxDetail.tsx): the unclassified-file table, its select-all row, the
 * bulk-override fieldset (FR-044/US11, #755), the #611 heterogeneous-
 * selection gate and undo banner, and the per-file apply row.
 *
 * Takes the whole `useInboxReclassifyState` result rather than 20 individual
 * props — every value below comes from that one hook, so passing it intact
 * keeps this boundary stable as the hook grows.
 */

import { m } from '@/paraglide/messages';
import { Banner, Section, Table } from '@/ui';
import { FRAME_TYPE_OPTIONS, humanizeKey } from './inboxDetailHelpers';
import type { UseInboxReclassifyState } from './useInboxReclassifyState';
import { selectBase } from '@/styles/select.css';

export interface InboxNeedsReviewProps {
  reclassify: UseInboxReclassifyState;
  /** Frame type of the item as a whole; null when unknown or mixed. */
  itemFrameType: string | null;
  columns: { key: string; label: string; style?: React.CSSProperties }[];
  rows: Record<string, React.ReactNode>[];
}

export function InboxNeedsReview({
  reclassify,
  itemFrameType,
  columns: unclassifiedColumns,
  rows: unclassifiedRows,
}: InboxNeedsReviewProps) {
  const {
    reclassifyLoading,
    filesNeedingReview,
    propertyRegistry,
    pendingOverrides,
    applyError,
    handleApplyOverrides,
    selectedFiles,
    bulkFrameType,
    setBulkFrameType,
    bulkPropValues,
    bulkError,
    handleSelectAll,
    handleBulkPropChange,
    handleBulkApply,
    isHeterogeneousFrameTypeBulk,
    heterogeneousSignature,
    heterogeneousAcked,
    setHeterogeneousAckKey,
    lastFrameTypeUndo,
    undoLoading,
    undoError,
    handleUndoBulkFrameType,
  } = reclassify;

  const allSelected =
    filesNeedingReview.length > 0 &&
    selectedFiles.size === filesNeedingReview.length;
  const someSelected = selectedFiles.size > 0 && !allSelected;

  // Generic bulk-editable properties (FR-044/US11, issue #755): every
  // overridable registry entry other than frameType (which keeps its own
  // enum select above), narrowed to the ones applicable to this item's frame
  // type. Unknown/mixed frame type (itemFrameType == null) shows all of them.
  const genericBulkFields = (propertyRegistry ?? []).filter((e) => {
    if (e.key === 'frameType' || !e.overridable) return false;
    if (!itemFrameType) return true;
    return e.appliesTo.includes(itemFrameType);
  });

  // Reuse existing translated labels/placeholders for the well-known keys;
  // any other registry key falls back to `humanizeKey` in the render loop.
  const KNOWN_BULK_FIELD_LABELS: Record<
    string,
    { label: string; placeholder?: string; testid: string }
  > = {
    filter: {
      label: m.common_filter(),
      placeholder: m.inbox_filter_placeholder(),
      testid: 'bulk-filter',
    },
    exposureS: {
      label: m.inbox_exposure_label(),
      placeholder: m.inbox_exposure_placeholder(),
      testid: 'bulk-exposure-s',
    },
    binning: {
      label: m.settings_calmatch_binning(),
      placeholder: m.inbox_binning_placeholder(),
      testid: 'bulk-binning',
    },
    gain: { label: m.inbox_col_gain(), testid: 'bulk-gain' },
    temperatureC: {
      label: m.settings_calmatch_sensor_temp(),
      testid: 'bulk-temperature-c',
    },
  };

  return (
    <Section
      key="needs-review"
      title={m.inbox_needs_review_title({
        count: unclassifiedRows.length,
      })}
    >
      <div className="pv-inbox-detail__select-all-row">
        <input
          type="checkbox"
          checked={allSelected}
          ref={(el) => {
            if (el) el.indeterminate = someSelected;
          }}
          onChange={handleSelectAll}
          aria-label={m.inbox_select_all_unclassified_aria()}
          data-testid="reclassify-select-all"
        />
        <span className="pv-inbox-detail__select-all-label">
          {selectedFiles.size === 0
            ? m.common_select_all()
            : m.inbox_n_selected({ count: selectedFiles.size })}
        </span>
      </div>
      <Table columns={unclassifiedColumns} rows={unclassifiedRows} />

      {selectedFiles.size > 0 && (
        <fieldset className="pv-inbox-detail__bulk-controls">
          <legend className="pv-visually-hidden">
            {m.inbox_bulk_override_controls_aria()}
          </legend>
          <div className="pv-inbox-detail__bulk-field">
            {}
            <label
              htmlFor="bulk-frame-type"
              className="pv-inbox-detail__bulk-label"
            >
              {m.inbox_frame_type_label()}
            </label>
            <select
              id="bulk-frame-type"
              value={bulkFrameType}
              onChange={(e) => setBulkFrameType(e.target.value)}
              aria-label={m.inbox_bulk_frame_type_aria()}
              data-testid="bulk-frame-type"
              className={selectBase}
            >
              <option value="">{m.inbox_unchanged_placeholder()}</option>
              {FRAME_TYPE_OPTIONS.map((t) => (
                <option key={t} value={t}>
                  {t}
                </option>
              ))}
            </select>
          </div>

          {/* Field-agnostic bulk properties (FR-044/US11, issue #755):
                  every OTHER overridable registry property applicable to
                  this item's frame type — filter/exposureS/binning/gain/
                  temperatureC/etc, whichever the registry returns. Known
                  keys reuse their existing translated label; unrecognised
                  future registry keys fall back to a humanized label so
                  new properties are reachable without a UI change. */}
          {genericBulkFields.map((field) => {
            const known = KNOWN_BULK_FIELD_LABELS[field.key];
            const label =
              known?.label ??
              humanizeKey(field.key) + (field.unit ? ` (${field.unit})` : '');
            const testid = known?.testid ?? `bulk-prop-${field.key}`;
            const inputType =
              field.kind === 'number' || field.kind === 'integer'
                ? 'number'
                : 'text';
            return (
              <div className="pv-inbox-detail__bulk-field" key={field.key}>
                {}
                <label
                  htmlFor={testid}
                  className="pv-inbox-detail__bulk-label"
                  title={field.validation ?? undefined}
                >
                  {label}
                </label>
                <input
                  id={testid}
                  type={inputType}
                  value={bulkPropValues[field.key] ?? ''}
                  onChange={(e) =>
                    handleBulkPropChange(field.key, e.target.value)
                  }
                  placeholder={
                    known?.placeholder ?? m.inbox_unchanged_placeholder()
                  }
                  aria-label={label}
                  data-testid={testid}
                  className="pv-input pv-input--sm pv-inbox-detail__bulk-input-w80"
                />
              </div>
            );
          })}

          {/* #611: the selection spans more than one currently-
                  detected frame type — warn before the override is
                  silently applied to files it may not actually belong
                  to (e.g. calibration files stranded under a light's
                  target/filter/date destination). Require an explicit
                  acknowledgement before Apply proceeds. */}
          {isHeterogeneousFrameTypeBulk && (
            <Banner
              variant="warn"
              className="pv-inbox-detail__banner-mt2"
              data-testid="bulk-heterogeneous-warning"
            >
              <div className="pv-inbox-alert__msg">
                <span className="pv-inbox-alert__title">
                  {m.inbox_bulk_heterogeneous_title()}
                </span>
                <span className="pv-inbox-alert__body">
                  {m.inbox_bulk_heterogeneous_body({
                    type: bulkFrameType,
                  })}
                </span>
              </div>
              <label className="pv-inbox-detail__select-all-row">
                <input
                  type="checkbox"
                  checked={heterogeneousAcked}
                  onChange={(e) =>
                    setHeterogeneousAckKey(
                      e.target.checked ? heterogeneousSignature : null,
                    )
                  }
                  aria-label={m.inbox_bulk_heterogeneous_ack_label({
                    type: bulkFrameType,
                  })}
                  data-testid="bulk-heterogeneous-ack"
                />
                {m.inbox_bulk_heterogeneous_ack_label({
                  type: bulkFrameType,
                })}
              </label>
            </Banner>
          )}

          <button
            type="button"
            className="pv-btn pv-btn--sm pv-btn--primary"
            onClick={handleBulkApply}
            disabled={reclassifyLoading || !heterogeneousAcked}
            aria-label={m.inbox_bulk_override_apply_aria({
              count: selectedFiles.size,
            })}
            data-testid="bulk-apply-btn"
          >
            {reclassifyLoading
              ? m.common_applying()
              : isHeterogeneousFrameTypeBulk
                ? m.inbox_bulk_apply_anyway({
                    count: selectedFiles.size,
                  })
                : m.inbox_apply_to_selected({
                    count: selectedFiles.size,
                  })}
          </button>
        </fieldset>
      )}

      {bulkError && (
        <Banner
          variant="danger"
          className="pv-inbox-detail__banner-mt2"
          data-testid="inbox-detail-banner-mt2"
        >
          {bulkError}
        </Banner>
      )}

      {/* #611: recoverable bulk frame-type override — restores each
              file's pre-override detected type via a per-file
              `overrides` call. */}
      {lastFrameTypeUndo && (
        <Banner
          variant="info"
          className="pv-inbox-detail__banner-mt2"
          data-testid="bulk-undo-banner"
        >
          <div className="pv-inbox-alert__msg">
            <span className="pv-inbox-alert__body">
              {m.inbox_bulk_undo_message({
                count: lastFrameTypeUndo.count,
              })}
            </span>
          </div>
          <button
            type="button"
            className="pv-btn pv-btn--sm"
            onClick={() => void handleUndoBulkFrameType()}
            disabled={undoLoading}
            aria-label={m.inbox_bulk_undo_aria()}
            data-testid="bulk-undo-btn"
          >
            {undoLoading ? m.common_applying() : m.inbox_bulk_undo_button()}
          </button>
        </Banner>
      )}

      {undoError && (
        <Banner
          variant="danger"
          className="pv-inbox-detail__banner-mt2"
          data-testid="inbox-detail-banner-mt2"
        >
          {undoError}
        </Banner>
      )}

      {Object.keys(pendingOverrides).length > 0 && (
        <div className="pv-inbox-detail__apply-row">
          <button
            type="button"
            className="pv-btn pv-btn--sm pv-btn--primary"
            onClick={handleApplyOverrides}
            disabled={
              Object.keys(pendingOverrides).length === 0 || reclassifyLoading
            }
            aria-label={m.inbox_apply_manual_overrides_aria()}
          >
            {reclassifyLoading
              ? m.common_applying()
              : m.inbox_apply_n_overrides({
                  count: Object.keys(pendingOverrides).length,
                })}
          </button>
        </div>
      )}

      {applyError && (
        <Banner
          variant="danger"
          className="pv-inbox-detail__banner-mt2"
          data-testid="inbox-detail-banner-mt2"
        >
          {applyError}
        </Banner>
      )}
    </Section>
  );
}
