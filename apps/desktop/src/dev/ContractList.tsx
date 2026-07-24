// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * ContractList — renders the registered contract registry table (spec 021 US1).
 *
 * Columns: name, version, direction, replay-safe, mismatch warning, schema path,
 * and a "View schema" action.
 */

import type { ContractMeta } from '@/bindings/index';
import {
  empty as contractsListEmpty,
  table as contractsListTable,
  theadRow as contractsListTheadRow,
  th as contractsListTh,
  row as contractsListRow,
  td as contractsListTd,
  tdName as contractsListTdName,
  tdMuted as contractsListTdMuted,
  tdCenter as contractsListTdCenter,
  tdSchema as contractsListTdSchema,
  mismatchIcon as contractsListMismatchIcon,
  replayOk as contractsListReplayOk,
  replayNa as contractsListReplayNa,
} from './contract-list.css';

interface ContractListProps {
  contracts: ContractMeta[];
  onViewSchema: (contract: ContractMeta) => void;
}

export function ContractList({ contracts, onViewSchema }: ContractListProps) {
  if (contracts.length === 0) {
    return <p className={contractsListEmpty}>No contracts loaded.</p>;
  }

  return (
    <table className={contractsListTable} aria-label="Contract registry">
      <thead>
        <tr className={contractsListTheadRow}>
          <th className={contractsListTh}>Name</th>
          <th className={contractsListTh}>Version</th>
          <th className={contractsListTh}>Direction</th>
          <th className={contractsListTh}>Replay</th>
          <th className={contractsListTh}>Schema path</th>
          <th className={contractsListTh}>Actions</th>
        </tr>
      </thead>
      <tbody>
        {contracts.map((c) => (
          <tr
            key={c.name}
            className={contractsListRow}
            data-testid={`contract-row-${c.name}`}
          >
            <td className={contractsListTdName}>
              {c.mismatch === true && (
                <span
                  title="Version mismatch between TypeScript and Rust registries"
                  aria-label="Version mismatch warning"
                  className={contractsListMismatchIcon}
                >
                  ⚠
                </span>
              )}
              {c.name}
            </td>
            <td className={contractsListTd}>{c.version}</td>
            <td className={contractsListTdMuted}>{c.direction}</td>
            <td className={contractsListTdCenter}>
              {c.replaySafe ? (
                <span
                  title="Replay-safe: read-only contract, replay allowed"
                  className={contractsListReplayOk}
                >
                  ✓
                </span>
              ) : (
                <span
                  title="Not replay-safe: write contract, replay disabled"
                  className={contractsListReplayNa}
                >
                  —
                </span>
              )}
            </td>
            <td
              className={contractsListTdSchema}
              title={c.schemaPath || 'Schema path not available'}
            >
              {c.schemaPath || <em>N/A</em>}
            </td>
            <td className={contractsListTd}>
              <button
                type="button"
                className="pv-btn pv-btn--xs"
                onClick={() => onViewSchema(c)}
                aria-label={`View schema for ${c.name}`}
              >
                View schema
              </button>
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
