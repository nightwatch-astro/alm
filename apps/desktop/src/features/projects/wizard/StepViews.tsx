// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * #776: this step used to hardcode `scope: 'all sources (3 lights + 4
 * masters)'` / `items: '126 items'` regardless of the actual selection. The
 * table row below is now derived from the real selected sessions
 * (`sessionsList`, same cache as `SessionSourcePicker`) and the real
 * calibration master count picked in step 3.
 */

import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { m } from '@/lib/i18n';
import { Pill, Box, Btn, Section } from '@/ui';
import { queryKeys } from '@/data/queryKeys';
import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import { formatBytes } from '@/lib/format';

export type SourceViewStrategy = 'symlink' | 'hardlink' | 'copy' | 'junction';
export type ConflictPolicy = 'fail' | 'rename' | 'skip' | 'manual';

export interface StepViewsData {
  strategy: SourceViewStrategy;
  conflictPolicy?: ConflictPolicy;
}

export interface StepViewsProps {
  data: StepViewsData;
  onChange: (data: StepViewsData) => void;
  /** Sessions selected in step 2 (Sources) — drives the real scope/items row. */
  selectedSessionIds: string[];
  /** Calibration masters picked in step 3 (flats + shared dark/bias/dark-flat). */
  selectedMasterCount: number;
}

// ── Component ───────────────────────────────────────────────────────────────

export function StepViews({
  data,
  onChange,
  selectedSessionIds,
  selectedMasterCount,
}: StepViewsProps) {
  const [viewName, setViewName] = useState('wbpp_input');
  const conflictPolicy = data.conflictPolicy ?? 'fail';

  const { data: sessions } = useQuery({
    queryKey: queryKeys.sessions.all(),
    queryFn: async () => unwrap(await commands.sessionsList(null, null)),
  });

  const selected = (sessions ?? []).filter((s) =>
    selectedSessionIds.includes(s.id),
  );
  const frameCount = selected.reduce((acc, s) => acc + s.frameCount, 0);
  const totalBytes = selected.reduce((acc, s) => acc + s.totalSizeBytes, 0);
  const itemCount = frameCount + selectedMasterCount;

  const scope = m.projects_wizard_views_scope({
    lights: selected.length,
    masters: selectedMasterCount,
  });
  const items = m.projects_wizard_views_item_count({ count: itemCount });
  // Link/junction strategies cost negligible metadata on disk; only 'copy'
  // duplicates real bytes, so only that strategy gets a real size estimate.
  const estimatedSize =
    data.strategy === 'copy'
      ? formatBytes(totalBytes)
      : m.projects_wizard_views_metadata_only();

  return (
    <div className="pv-wizard-views">
      {/* Step description */}
      <div className="pv-wizard-views__desc">
        {m.projects_wizard_views_desc()}{' '}
        <button
          type="button"
          onClick={(e) => e.preventDefault()}
          className="pv-wizard-views__link"
        >
          {m.projects_wizard_views_settings_link()}
        </button>{' '}
        {m.projects_wizard_views_override_note()}
      </div>

      {/* ── Strategy (from settings) ── */}
      <Box title={m.projects_wizard_strategy_title()}>
        <div className="pv-wizard-views__box-header">
          <button
            type="button"
            onClick={(e) => e.preventDefault()}
            className="pv-wizard-views__box-link"
          >
            {m.projects_wizard_strategy_override_link()}
          </button>
        </div>
        <div className="pv-wizard-views__strategy-row">
          <Pill variant="ok">{m.projects_wizard_strategy_ntfs()}</Pill>
          <span className="pv-wizard-views__strategy-label">
            {m.projects_wizard_strategy_ntfs_label()}
          </span>
        </div>
        <div className="pv-wizard-views__strategy-note">
          {m.projects_wizard_strategy_fallback_note()}
        </div>
      </Box>

      {/* ── Views to generate ── */}
      <Section title={m.projects_wizard_views_title()}>
        <div className="pv-wizard-views__table-hint">
          {m.projects_wizard_views_hint()}
        </div>
        <table className="pv-simple-table">
          <thead>
            <tr>
              <th>{m.projects_wizard_col_view_name()}</th>
              <th>{m.projects_wizard_col_strategy()}</th>
              <th>{m.projects_wizard_col_scope()}</th>
              <th>{m.projects_wizard_col_items()}</th>
              <th>{m.projects_wizard_col_est_size()}</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td className="pv-mono">
                <input
                  value={viewName}
                  onChange={(e) => setViewName(e.target.value)}
                  className="pv-wizard-views__name-input"
                  aria-label={m.projects_wizard_col_view_name()}
                />
              </td>
              <td>
                <Pill variant="ok">{data.strategy}</Pill>
              </td>
              <td className="pv-wizard-views__td-scope">{scope}</td>
              <td className="pv-mono pv-wizard-views__td-small">{items}</td>
              <td className="pv-mono pv-wizard-views__td-small">
                {estimatedSize}
              </td>
            </tr>
          </tbody>
        </table>
        <Btn size="sm" className="pv-wizard-views__add-btn">
          {m.projects_wizard_add_view_btn()}
        </Btn>
      </Section>

      {/* ── Conflict policy ── */}
      <Box title={m.projects_wizard_conflict_title()}>
        <div className="pv-wizard-views__box-header">
          <button
            type="button"
            onClick={(e) => e.preventDefault()}
            className="pv-wizard-views__box-link"
          >
            {m.projects_wizard_conflict_defaults_link()}
          </button>
        </div>
        <div className="pv-wizard-views__radio-group">
          <label
            className="pv-wizard-views__radio-label"
            htmlFor="conflict-policy-fail"
          >
            <input
              id="conflict-policy-fail"
              type="radio"
              name="conflict-policy"
              aria-label={m.projects_wizard_conflict_fail()}
              checked={conflictPolicy === 'fail'}
              onChange={() => onChange({ ...data, conflictPolicy: 'fail' })}
            />{' '}
            {m.projects_wizard_conflict_fail()}
          </label>
          <label
            className="pv-wizard-views__radio-label"
            htmlFor="conflict-policy-rename"
          >
            <input
              id="conflict-policy-rename"
              type="radio"
              name="conflict-policy"
              aria-label={m.projects_wizard_conflict_rename()}
              checked={conflictPolicy === 'rename'}
              onChange={() => onChange({ ...data, conflictPolicy: 'rename' })}
            />{' '}
            {m.projects_wizard_conflict_rename()}
          </label>
          <label
            className="pv-wizard-views__radio-label"
            htmlFor="conflict-policy-skip"
          >
            <input
              id="conflict-policy-skip"
              type="radio"
              name="conflict-policy"
              aria-label={m.projects_wizard_conflict_skip()}
              checked={conflictPolicy === 'skip'}
              onChange={() => onChange({ ...data, conflictPolicy: 'skip' })}
            />{' '}
            {m.projects_wizard_conflict_skip()}
          </label>
          <label
            className="pv-wizard-views__radio-label"
            htmlFor="conflict-policy-manual"
          >
            <input
              id="conflict-policy-manual"
              type="radio"
              name="conflict-policy"
              aria-label={m.projects_wizard_conflict_manual()}
              checked={conflictPolicy === 'manual'}
              onChange={() => onChange({ ...data, conflictPolicy: 'manual' })}
            />{' '}
            {m.projects_wizard_conflict_manual()}
          </label>
        </div>
      </Box>
    </div>
  );
}
