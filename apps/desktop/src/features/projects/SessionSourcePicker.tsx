// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * SessionSourcePicker — shared session-list-with-selection pattern (WP-008-C).
 *
 * Extracted from the project-creation wizard's `StepSources` so the same
 * filterable, checkbox-driven session list can be embedded both in the
 * wizard (US2) and in the post-creation "add sources" flow on
 * `EditProjectPane` (US3/US4) — one component, one CSS class, per the
 * shared-UI-component mandate. `StepSources` is now a thin adapter over this
 * component; its own props/behaviour are unchanged.
 *
 * Spec 041 FR-051: sessions are derived, already-confirmed inventory — no
 * review-state filter remains; every fetched session is eligible for
 * selection unless the caller excludes it via `excludeSessionIds` (e.g. a
 * session already linked to the project being edited).
 */

import { useState, useMemo } from 'react';
import { m } from '@/lib/i18n';
import { Checkbox } from '@base-ui-components/react/checkbox';
import { useQuery } from '@tanstack/react-query';
import { queryKeys } from '@/data/queryKeys';
import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';

import { formatIntegration } from '@/lib/format';

export interface SessionSourcePickerProps {
  /** Currently-selected session ids (checked rows). */
  selectedSessionIds: string[];
  /** Called with the full next selection whenever it changes. */
  onChange: (selectedSessionIds: string[]) => void;
  /**
   * Session ids to omit from the pickable list entirely — e.g. sessions
   * already linked to the project being edited. Optional; when absent every
   * fetched session is eligible (the wizard's original behaviour).
   */
  excludeSessionIds?: string[];
  /** Message shown when the (post-exclusion) filtered list is empty. */
  emptyMessage?: string;
}

export function SessionSourcePicker({
  selectedSessionIds,
  onChange,
  excludeSessionIds,
  emptyMessage,
}: SessionSourcePickerProps) {
  const { data: allSessions, isFetching: loading } = useQuery({
    queryKey: queryKeys.sessions.all(),
    queryFn: async () => unwrap(await commands.sessionsList(null, null)),
  });
  const [filterTarget, setFilterTarget] = useState('');
  const [filterFilter, setFilterFilter] = useState('');

  const excludeSet = useMemo(
    () => new Set(excludeSessionIds ?? []),
    [excludeSessionIds],
  );

  const sessions = useMemo(() => {
    if (!allSessions) return [];
    if (excludeSet.size === 0) return allSessions;
    return allSessions.filter((s) => !excludeSet.has(s.id));
  }, [allSessions, excludeSet]);

  const filtered = useMemo(() => {
    return sessions.filter((s) => {
      // sessionKey.target can be undefined for a session whose target never
      // resolved (contract says `string`, but that's a best-effort backend
      // guarantee — defend the UI so an unresolved session doesn't crash the
      // whole picker, it just never matches a target filter).
      if (
        filterTarget &&
        !(s.sessionKey.target ?? '')
          .toLowerCase()
          .includes(filterTarget.toLowerCase())
      )
        return false;
      if (
        filterFilter &&
        !s.sessionKey.filter.toLowerCase().includes(filterFilter.toLowerCase())
      )
        return false;
      return true;
    });
  }, [sessions, filterTarget, filterFilter]);

  const totalIntegration = useMemo(() => {
    return sessions
      .filter((s) => selectedSessionIds.includes(s.id))
      .reduce((acc, s) => acc + (s.totalIntegrationSeconds ?? 0), 0);
  }, [sessions, selectedSessionIds]);

  function toggleSession(id: string) {
    const selected = new Set(selectedSessionIds);
    if (selected.has(id)) {
      selected.delete(id);
    } else {
      selected.add(id);
    }
    onChange(Array.from(selected));
  }

  function toggleAll() {
    if (filtered.length === selectedSessionIds.length) {
      onChange([]);
    } else {
      onChange(filtered.map((s) => s.id));
    }
  }

  if (loading) {
    return (
      <div className="pv-source-picker__loading">
        {m.projects_wizard_sources_loading()}
      </div>
    );
  }

  return (
    <div className="pv-source-picker" data-testid="source-picker">
      {/* Filter row */}
      <div className="pv-source-picker__filter-row">
        <input
          type="text"
          placeholder={m.projects_wizard_filter_target_placeholder()}
          aria-label={m.projects_wizard_filter_target_placeholder()}
          value={filterTarget}
          onChange={(e) => setFilterTarget(e.target.value)}
          // These are free-text filters, not a submittable field — guard
          // against an ambient <form> ancestor treating Enter as submit if
          // this component is ever embedded inside one (WP-008-C).
          onKeyDown={(e) => {
            if (e.key === 'Enter') e.preventDefault();
          }}
          className="pv-source-picker__filter-input"
        />
        <input
          type="text"
          placeholder={m.projects_wizard_filter_filter_placeholder()}
          aria-label={m.projects_wizard_filter_filter_placeholder()}
          value={filterFilter}
          onChange={(e) => setFilterFilter(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') e.preventDefault();
          }}
          className="pv-source-picker__filter-input"
        />
      </div>

      {/* Summary */}
      <div className="pv-source-picker__summary">
        <span>
          <strong>{selectedSessionIds.length}</strong>{' '}
          {m.projects_wizard_sessions_selected()}
        </span>
        <span>
          {m.projects_wizard_total_integration()}{' '}
          <strong>{formatIntegration(totalIntegration)}</strong>
        </span>
      </div>

      {/* Session list */}
      <div className="pv-source-picker__list">
        {/* Header */}
        <div className="pv-source-picker__list-header">
          <Checkbox.Root
            className="pv-checkbox"
            checked={
              filtered.length > 0 &&
              selectedSessionIds.length === filtered.length
            }
            onCheckedChange={toggleAll}
            aria-label={m.projects_wizard_select_all_aria()}
          >
            <Checkbox.Indicator className="pv-checkbox__indicator">
              &#x2713;
            </Checkbox.Indicator>
          </Checkbox.Root>
          <span>{m.projects_wizard_col_target_filter_night()}</span>
          <span>{m.projects_wizard_col_frames()}</span>
          <span>{m.projects_wizard_col_integration()}</span>
          <span>{m.projects_wizard_col_train()}</span>
        </div>

        {/* Rows */}
        {filtered.map((session) => {
          // Defensive fallback: a session whose target never resolved can
          // reach the UI with `sessionKey.target` undefined even though the
          // contract types it as `string` — render a disclosed placeholder
          // instead of crashing (`undefined.toLowerCase()` et al) or silently
          // showing a blank cell.
          const targetLabel =
            session.sessionKey.target || m.projects_wizard_target_unresolved();
          return (
            <label
              key={session.id}
              className={
                'pv-source-picker__row' +
                (selectedSessionIds.includes(session.id)
                  ? ' pv-source-picker__row--selected'
                  : '')
              }
              data-testid={`session-picker-row-${session.id}`}
            >
              <Checkbox.Root
                className="pv-checkbox"
                checked={selectedSessionIds.includes(session.id)}
                onCheckedChange={() => toggleSession(session.id)}
                aria-label={m.projects_wizard_select_session_aria({
                  target: targetLabel,
                })}
              >
                <Checkbox.Indicator className="pv-checkbox__indicator">
                  &#x2713;
                </Checkbox.Indicator>
              </Checkbox.Root>
              <span>
                {targetLabel} / {session.sessionKey.filter} /{' '}
                {session.sessionKey.night}
              </span>
              <span data-testid={`session-picker-frames-${session.id}`}>
                {session.frameCount}
              </span>
              <span>
                {formatIntegration(session.totalIntegrationSeconds ?? 0)}
              </span>
              <span className="pv-source-picker__train-id">
                {session.opticalTrainId.slice(0, 8)}
              </span>
            </label>
          );
        })}

        {filtered.length === 0 && (
          <div className="pv-source-picker__empty">
            {emptyMessage ?? m.projects_wizard_sessions_empty()}
          </div>
        )}
      </div>
    </div>
  );
}
