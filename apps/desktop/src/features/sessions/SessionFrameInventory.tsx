// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Per-session frame inventory panel (spec 048 T014/T025 frontend).
 *
 * An on-demand `inventory.frame.list` scan surfaces the session's real
 * present-frame count and disk total (SC-002) — the command had a real,
 * tested backend but zero frontend callers before this component (see
 * `crates/e2e-tests/tests/inventory_journeys.rs`). Frames the backend has
 * flagged `missing` get an inline relink action (`inventory.frame.relink`,
 * sha256-confirmed re-home per FR-012a); a `hash.mismatch` renders as an
 * inline error — never silently treated as success.
 */

import { useState } from 'react';
import { Btn, Banner, Table, Pill, Section } from '@/ui';
import type { TableColumn } from '@/ui';
import { m } from '@/lib/i18n';
import { formatBytes } from '@/lib/format';
import { errMessage } from '@/lib/errors';
import { addToast } from '@/shared/toast';
import {
  useFrameListScan,
  useRelinkFrame,
  useRootFrameWatcher,
} from '@/features/inventory/store';
import type { InventoryFrame } from '@/bindings/index';

export interface SessionFrameInventoryProps {
  sessionId: string;
  /** The session's owning library root, whose live detection triggers are
   * attached for as long as this panel is mounted (spec 048 T023/T026). */
  rootId: string;
}

function columns(): TableColumn[] {
  return [
    { key: 'path', label: m.sessions_frame_inventory_col_path() },
    { key: 'type', label: m.sessions_frame_inventory_col_type() },
    { key: 'size', label: m.sessions_frame_inventory_col_size() },
    { key: 'state', label: '' },
    { key: 'relink', label: '' },
  ];
}

function RelinkControl({
  frame,
  onRelinked,
}: {
  frame: InventoryFrame;
  onRelinked: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [candidate, setCandidate] = useState('');
  const relink = useRelinkFrame();

  if (!open) {
    return (
      <Btn
        size="sm"
        variant="ghost"
        onClick={() => setOpen(true)}
        data-testid={`relink-open-${frame.frameId}`}
      >
        {m.sessions_frame_inventory_relink_btn()}
      </Btn>
    );
  }

  return (
    <span className="pv-settings__row-content">
      <input
        className="pv-input"
        value={candidate}
        onChange={(e) => setCandidate(e.target.value)}
        placeholder={m.sessions_frame_inventory_relink_path_placeholder()}
        aria-label={m.sessions_frame_inventory_relink_path_label()}
        data-testid={`relink-input-${frame.frameId}`}
      />
      <Btn
        size="sm"
        disabled={relink.isPending || !candidate.trim()}
        onClick={() =>
          relink.mutate(
            { frameId: frame.frameId, candidateRelativePath: candidate.trim() },
            {
              onSuccess: () => {
                addToast({
                  message: m.sessions_frame_inventory_relink_success_toast(),
                  variant: 'info',
                });
                setOpen(false);
                setCandidate('');
                onRelinked();
              },
            },
          )
        }
        data-testid={`relink-confirm-${frame.frameId}`}
      >
        {relink.isPending
          ? m.common_saving()
          : m.sessions_frame_inventory_relink_confirm_btn()}
      </Btn>
      <Btn
        size="sm"
        variant="ghost"
        onClick={() => {
          setOpen(false);
          setCandidate('');
          relink.reset();
        }}
      >
        {m.common_cancel()}
      </Btn>
      {relink.isError && (
        <span
          className="pv-field-error"
          data-testid={`relink-error-${frame.frameId}`}
        >
          {errMessage(relink.error)}
        </span>
      )}
    </span>
  );
}

export function SessionFrameInventory({
  sessionId,
  rootId,
}: SessionFrameInventoryProps) {
  useRootFrameWatcher(rootId);
  const scan = useFrameListScan();
  const result = scan.data;

  const runScan = () => scan.mutate({ sessionId, rootId: null });

  const rows = (result?.frames ?? []).map((f) => ({
    _testid: `frame-inventory-row-${f.frameId}`,
    path: (
      <span className="pv-mono" title={f.relativePath}>
        {f.relativePath}
      </span>
    ),
    type: f.frameType,
    size: formatBytes(f.sizeBytes),
    state:
      f.state === 'missing' ? (
        <Pill variant="warn">{m.sessions_frame_inventory_state_missing()}</Pill>
      ) : null,
    relink:
      f.state === 'missing' ? (
        <RelinkControl frame={f} onRelinked={runScan} />
      ) : null,
  }));

  return (
    <Section
      title={m.sessions_frame_inventory_title()}
      data-testid="session-frame-inventory"
    >
      <div className="pv-cleanup-scan__controls">
        <Btn
          size="sm"
          onClick={runScan}
          disabled={scan.isPending}
          data-testid="frame-inventory-scan-btn"
        >
          {scan.isPending
            ? m.common_loading()
            : m.sessions_frame_inventory_scan_btn()}
        </Btn>
        {result && (
          <span
            className="pv-cleanup-scan__reclaimable"
            data-testid="frame-inventory-summary"
          >
            {m.sessions_frame_inventory_summary({
              count: result.presentCount,
              size: formatBytes(result.presentSizeBytes),
            })}
          </span>
        )}
      </div>
      {scan.isError && (
        <Banner variant="danger">{errMessage(scan.error)}</Banner>
      )}
      {result && result.frames.length > 0 && (
        <Table columns={columns()} rows={rows} />
      )}
    </Section>
  );
}
