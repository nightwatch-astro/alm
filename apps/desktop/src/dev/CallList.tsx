// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * CallList — renders the recent-calls ring buffer table (spec 021 US2).
 *
 * Columns: id, contract, version, started_at, duration, outcome, truncation,
 * and actions (view schema, replay when replay-safe).
 */

import type { ContractCall, ContractMeta } from '@/bindings/index';
import {
  callsEmpty,
  callsTable,
  callsTheadRow,
  callsTh,
  callsRow,
  callsTd,
  callsTdId,
  callsTdContract,
  callsTdStarted,
  callsTdActions,
  callsTruncated,
  callsOutcomeVariants,
  callsReplayBtnVariants,
} from './dev.css';

interface CallListProps {
  calls: ContractCall[];
  contracts: ContractMeta[];
  onViewSchema: (call: ContractCall) => void;
  onReplay: (call: ContractCall) => void;
}

function formatDuration(ms: number | null): string {
  if (ms == null) return '—';
  if (ms < 1) return '<1 ms';
  if (ms < 1000) return `${Math.round(ms)} ms`;
  return `${(ms / 1000).toFixed(2)} s`;
}

function formatStarted(iso: string): string {
  try {
    return new Date(iso).toLocaleTimeString();
  } catch {
    return iso;
  }
}

export function CallList({
  calls,
  contracts,
  onViewSchema,
  onReplay,
}: CallListProps) {
  if (calls.length === 0) {
    return (
      <p className={callsEmpty}>
        No calls recorded yet. Make some API calls with devMode on.
      </p>
    );
  }

  return (
    <table className={callsTable} aria-label="Recent contract calls">
      <thead>
        <tr className={callsTheadRow}>
          <th className={callsTh}>ID</th>
          <th className={callsTh}>Contract</th>
          <th className={callsTh}>Version</th>
          <th className={callsTh}>Started</th>
          <th className={callsTh}>Duration</th>
          <th className={callsTh}>Outcome</th>
          <th className={callsTh}>Actions</th>
        </tr>
      </thead>
      <tbody>
        {calls.map((call) => {
          const contractMeta = contracts.find((c) => c.name === call.contract);
          const isReplaySafe = contractMeta?.replaySafe === true;
          const isError = call.error !== undefined;

          return (
            <tr
              key={call.id}
              className={callsRow}
              data-testid={`call-row-${call.id}`}
            >
              <td className={callsTdId}>{call.id}</td>
              <td className={callsTdContract}>
                {call.contract}
                {call.payloadTruncated && (
                  <span
                    title="Payload was truncated (exceeded 64 KB)"
                    className={callsTruncated}
                  >
                    ⚠T
                  </span>
                )}
              </td>
              <td className={callsTd}>{call.contractVersion}</td>
              <td className={callsTdStarted}>
                {formatStarted(call.startedAt)}
              </td>
              <td className={callsTd}>{formatDuration(call.durationMs)}</td>
              <td className={callsTd}>
                {isError ? (
                  <span
                    className={callsOutcomeVariants.error}
                    title={call.error?.message}
                  >
                    error: {call.error?.code}
                  </span>
                ) : (
                  <span className={callsOutcomeVariants.ok}>ok</span>
                )}
              </td>
              <td className={callsTdActions}>
                <button
                  type="button"
                  className="pv-btn pv-btn--xs"
                  onClick={() => onViewSchema(call)}
                  aria-label={`View schema for ${call.contract} v${call.contractVersion}`}
                >
                  Schema
                </button>
                <button
                  type="button"
                  className={`pv-btn pv-btn--xs ${isReplaySafe ? callsReplayBtnVariants.safe : callsReplayBtnVariants.unsafe}`}
                  onClick={() => onReplay(call)}
                  disabled={!isReplaySafe}
                  title={
                    isReplaySafe
                      ? `Replay ${call.contract}`
                      : 'Replay disabled: write contract'
                  }
                  aria-label={
                    isReplaySafe
                      ? `Replay call to ${call.contract}`
                      : `Replay disabled for write contract ${call.contract}`
                  }
                  aria-disabled={!isReplaySafe}
                >
                  Replay
                </button>
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
