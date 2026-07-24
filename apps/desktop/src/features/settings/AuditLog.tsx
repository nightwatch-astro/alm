// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { useState, useMemo, useId, useCallback, useEffect } from 'react';
import { useDebouncedCallback } from 'use-debounce';
import { useNavigate } from '@tanstack/react-router';
import { Btn, Pill, Table } from '@/ui';
import { auditList, auditExport } from './settingsIpc';
import type {
  AuditEntry,
  AuditFilterDto,
  AuditOutcome,
} from '@/bindings/index';
import { useEntityNames, entityNameKey } from '@/hooks/useEntityNames';
import { errMessage } from '@/lib/errors';
import { formatDateTime, toEpochMs } from '@/lib/datetime';
import { m } from '@/lib/i18n';
import { SettingsSection } from './SettingsKit';
import { selectBase } from '@/styles/select.css';

/** All `AuditOutcome` values, for the structured outcome filter (#749). */
const OUTCOME_VALUES: AuditOutcome[] = [
  'applied',
  'ok',
  'refused',
  'failed',
  'paused',
];

/**
 * Entity types seen across `write_audit` call-sites (domain-core's
 * `EntityType` enum plus `target`/`session`, which are audited via
 * free-form strings rather than that enum). Kept local — `AuditFilterDto`'s
 * `entityType` is a plain string, not a closed contract enum (#749).
 */
const ENTITY_TYPE_VALUES = [
  'project',
  'target',
  'session',
  'plan',
  'filesystem_plan',
  'settings',
  'protection',
  'equipment',
  'framing',
  'library_root',
  'file_record',
  'data_source',
  'prepared_source',
  'processing_artifact',
  'projection',
] as const;

/**
 * Entity types with a dedicated detail page to cross-link to (#831). Other
 * types (settings, protection, equipment, and the rest of
 * `ENTITY_TYPE_VALUES`) have no reachable page yet, so their rows stay
 * unlinked rather than routing to a dead end. `plan` is intentionally
 * excluded — no `/plans/:id` route exists yet (#626, same reasoning as the
 * LogPanel's `buildEntityPath`).
 */
function resolveAuditEntityPath(
  entityType: string,
  entityId: string,
): string | null {
  switch (entityType) {
    case 'project':
      return `/projects/${entityId}`;
    case 'target':
      return `/targets/${entityId}`;
    case 'session':
      return `/sessions/${entityId}`;
    default:
      return null;
  }
}

const ITEMS_PER_PAGE = 8;

/** Debounce for the free-text search box (matches TargetSearch's DEBOUNCE_MS). */
const SEARCH_DEBOUNCE_MS = 300;

function outcomeVariant(
  outcome: AuditOutcome,
): 'ok' | 'danger' | 'warn' | 'neutral' {
  switch (outcome) {
    case 'applied':
    case 'ok':
      return 'ok';
    case 'refused':
    case 'paused':
      return 'warn';
    case 'failed':
      return 'danger';
    default:
      return 'neutral';
  }
}

/** Render-time factory (spec 046 #8b) so outcome labels re-read the active locale. */
function outcomeLabel(outcome: AuditOutcome): string {
  switch (outcome) {
    case 'applied':
      return m.settings_auditlog_outcome_applied();
    case 'ok':
      return m.settings_auditlog_outcome_ok();
    case 'refused':
      return m.settings_auditlog_outcome_refused();
    case 'failed':
      return m.settings_auditlog_outcome_failed();
    case 'paused':
      return m.settings_auditlog_outcome_paused();
  }
}

/**
 * Localize an entry's detail text at DISPLAY time (D23 upgrade, campaign
 * task #45). The backend surfaces a stable `detailCode` + flat string
 * `detailParams` (derived from the durable `audit_log_entry.payload` JSON);
 * this render-time factory (spec 046 #8b) maps them to Paraglide catalog
 * messages so the tooltip re-reads the active locale.
 *
 * A code is only mapped when the params its template needs are present —
 * `transition.refused` in particular also covers free-form refusal reasons
 * that carry no params. Any unmapped row (old rows, unknown codes, missing
 * params) falls back to the stored backend-composed English `detail`.
 */
function detailText(e: AuditEntry): string {
  const p = e.detailParams ?? {};
  switch (e.detailCode) {
    case 'transition.refused':
      if (p.entityType && p.fromState && p.toState) {
        return m.settings_auditlog_detail_transition_refused_edge({
          entityType: p.entityType,
          fromState: p.fromState,
          toState: p.toState,
        });
      }
      break;
    case 'actor.not_authorised':
      return m.settings_auditlog_detail_actor_not_authorised();
    case 'provenance.unreviewed':
      if (p.count && !Number.isNaN(Number(p.count))) {
        return m.settings_auditlog_detail_provenance_unreviewed({
          count: Number(p.count),
        });
      }
      break;
    case 'plan.required':
      if (p.entityType && p.fromState && p.toState) {
        return m.settings_auditlog_detail_plan_required({
          entityType: p.entityType,
          fromState: p.fromState,
          toState: p.toState,
        });
      }
      break;
    case 'entity.not_found':
      return m.settings_auditlog_detail_entity_not_found({
        entityId: p.entityId ?? e.entityId,
      });
    case 'target.resolved':
      if (p.query)
        return m.settings_auditlog_detail_target_resolved({ query: p.query });
      break;
    case 'target.user_override':
      if (p.query)
        return m.settings_auditlog_detail_target_user_override({
          query: p.query,
        });
      break;
    default:
      break;
  }
  return e.detail;
}

/** Build the `audit.list` filter payload from the screen's search/date/structured controls. */
function buildFilters(
  search: string,
  dateFrom: string,
  dateTo: string,
  outcome: AuditOutcome | '',
  entityType: string,
): AuditFilterDto | null {
  const filters: AuditFilterDto = {};
  let hasFilter = false;
  if (search.trim()) {
    filters.search = search.trim();
    hasFilter = true;
  }
  if (dateFrom) {
    filters.from = new Date(toEpochMs(dateFrom)).toISOString();
    hasFilter = true;
  }
  if (dateTo) {
    // Exclusive upper bound: the day after `dateTo`, so the whole day is included.
    filters.to = new Date(toEpochMs(dateTo) + 86400000).toISOString();
    hasFilter = true;
  }
  if (outcome) {
    filters.outcome = outcome;
    hasFilter = true;
  }
  if (entityType) {
    filters.entityType = entityType;
    hasFilter = true;
  }
  return hasFilter ? filters : null;
}

export function AuditLog() {
  const [entries, setEntries] = useState<AuditEntry[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [exportError, setExportError] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);

  // `searchInput` mirrors the text box on every keystroke; `search` (which
  // drives the IPC filter) lags behind it by SEARCH_DEBOUNCE_MS so we don't
  // fire an `audit.list` round-trip per keystroke (same pattern as
  // TargetSearch's debounced typeahead).
  const [searchInput, setSearchInput] = useState('');
  const [search, setSearch] = useState('');
  const [dateFrom, setDateFrom] = useState('');
  const [dateTo, setDateTo] = useState('');
  const [outcomeFilter, setOutcomeFilter] = useState<AuditOutcome | ''>('');
  const [entityTypeFilter, setEntityTypeFilter] = useState('');
  const [page, setPage] = useState(0);
  const dateFromId = useId();
  const dateToId = useId();
  const outcomeFilterId = useId();
  const entityTypeFilterId = useId();
  const navigate = useNavigate();

  const applySearch = useDebouncedCallback((value: string) => {
    setSearch(value);
    setPage(0);
  }, SEARCH_DEBOUNCE_MS);

  // Cancel any pending debounced search on unmount.
  useEffect(() => () => applySearch.cancel(), [applySearch]);

  const filters = useMemo(
    () =>
      buildFilters(search, dateFrom, dateTo, outcomeFilter, entityTypeFilter),
    [search, dateFrom, dateTo, outcomeFilter, entityTypeFilter],
  );

  const entityNames = useEntityNames(entries);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setLoadError(null);
    auditList(filters, { limit: ITEMS_PER_PAGE, offset: page * ITEMS_PER_PAGE })
      .then((res) => {
        if (cancelled) return;
        setEntries(res.entries);
        setTotal(res.total);
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setLoadError(errMessage(err));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [filters, page]);

  const handleExport = useCallback(async () => {
    setExportError(null);
    setExporting(true);
    try {
      let filePath: string | null = null;
      try {
        const { save: showSaveDialog } = await import(
          '@tauri-apps/plugin-dialog'
        );
        filePath = await showSaveDialog({
          // eslint-disable-next-line alm/no-user-string -- native OS file-picker chrome, not rendered in the app
          title: 'Export Audit Log',
          defaultPath: `audit-log-export-${Date.now()}.ndjson`,
          filters: [{ name: 'NDJSON', extensions: ['ndjson', 'json'] }],
        });
      } catch {
        filePath = null;
      }
      if (!filePath) return;
      await auditExport(filePath, filters);
    } catch (err: unknown) {
      setExportError(errMessage(err));
    } finally {
      setExporting(false);
    }
  }, [filters]);

  const totalPages = Math.max(1, Math.ceil(total / ITEMS_PER_PAGE));

  return (
    <SettingsSection
      title={m.settings_auditlog_title()}
      action={
        <Btn
          size="sm"
          variant="ghost"
          onClick={() => void handleExport()}
          disabled={exporting || loading}
          aria-label={m.settings_auditlog_export_aria()}
        >
          {m.settings_auditlog_export()}
        </Btn>
      }
    >
      <div className="pv-audit-log__filters">
        <input
          type="text"
          className="pv-input pv-audit-log__search"
          placeholder={m.settings_auditlog_search_placeholder()}
          value={searchInput}
          onChange={(e) => {
            setSearchInput(e.target.value);
            applySearch(e.target.value);
          }}
          aria-label={m.settings_auditlog_search_aria()}
        />
        <label className="pv-audit-log__date-label" htmlFor={dateFromId}>
          {m.settings_auditlog_date_from()}
          {/* eslint-disable-next-line jsx-a11y/control-has-associated-label -- labelled by the wrapping <label> (htmlFor + id + visible text); rule misses the wrapping-label association */}
          <input
            id={dateFromId}
            type="date"
            className="pv-input pv-audit-log__date-input"
            value={dateFrom}
            onChange={(e) => {
              setDateFrom(e.target.value);
              setPage(0);
            }}
          />
        </label>
        <label className="pv-audit-log__date-label" htmlFor={dateToId}>
          {m.settings_auditlog_date_to()}
          {/* eslint-disable-next-line jsx-a11y/control-has-associated-label -- labelled by the wrapping <label> (htmlFor + id + visible text); rule misses the wrapping-label association */}
          <input
            id={dateToId}
            type="date"
            className="pv-input pv-audit-log__date-input"
            value={dateTo}
            onChange={(e) => {
              setDateTo(e.target.value);
              setPage(0);
            }}
          />
        </label>
        <label className="pv-audit-log__date-label" htmlFor={outcomeFilterId}>
          {m.settings_auditlog_col_outcome()}
          {/* eslint-disable-next-line jsx-a11y/control-has-associated-label -- labelled by the wrapping <label> (htmlFor + id + visible text); rule misses the wrapping-label association */}
          <select
            id={outcomeFilterId}
            className={`${selectBase} pv-audit-log__date-input`}
            value={outcomeFilter}
            onChange={(e) => {
              setOutcomeFilter(e.target.value as AuditOutcome | '');
              setPage(0);
            }}
          >
            <option value="">{m.common_all()}</option>
            {OUTCOME_VALUES.map((o) => (
              <option key={o} value={o}>
                {outcomeLabel(o)}
              </option>
            ))}
          </select>
        </label>
        <label
          className="pv-audit-log__date-label"
          htmlFor={entityTypeFilterId}
        >
          {m.settings_auditlog_col_entity()}
          {/* eslint-disable-next-line jsx-a11y/control-has-associated-label -- labelled by the wrapping <label> (htmlFor + id + visible text); rule misses the wrapping-label association */}
          <select
            id={entityTypeFilterId}
            className={`${selectBase} pv-audit-log__date-input`}
            value={entityTypeFilter}
            onChange={(e) => {
              setEntityTypeFilter(e.target.value);
              setPage(0);
            }}
          >
            <option value="">{m.common_all()}</option>
            {ENTITY_TYPE_VALUES.map((t) => (
              <option key={t} value={t}>
                {t}
              </option>
            ))}
          </select>
        </label>
      </div>

      {exportError && (
        <div className="pv-audit-log__export-error" role="alert">
          {m.settings_auditlog_export_failed({ error: exportError })}
        </div>
      )}

      {loading && (
        <div className="pv-audit-log__status">{m.common_loading()}</div>
      )}

      {loadError && (
        <div className="pv-audit-log__load-error">
          {m.settings_auditlog_load_error({ error: loadError })}
        </div>
      )}

      {!loading && !loadError && (
        <Table
          columns={[
            {
              key: 'timestamp',
              label: m.settings_auditlog_col_timestamp(),
              style: { width: 150 },
            },
            { key: 'event', label: m.settings_auditlog_col_event() },
            { key: 'entity', label: m.settings_auditlog_col_entity() },
            {
              key: 'stateChange',
              label: m.settings_auditlog_col_state_change(),
              style: { width: 140 },
            },
            { key: 'detail', label: m.settings_auditlog_col_detail() },
            {
              key: 'outcome',
              label: m.settings_auditlog_col_outcome(),
              style: { width: 90 },
            },
            {
              key: 'actor',
              label: m.settings_auditlog_col_actor(),
              style: { width: 72 },
            },
          ]}
          rows={entries.map((e) => {
            const path = resolveAuditEntityPath(e.entityType, e.entityId);
            const resolvedName = entityNames.get(entityNameKey(e));
            return {
              _onClick: path
                ? () => void navigate({ to: path as never })
                : undefined,
              timestamp: (
                <code className="pv-mono pv-audit-log__ts">
                  {formatDateTime(e.timestamp)}
                </code>
              ),
              event: <span className="pv-audit-log__event">{e.eventType}</span>,
              entity: (
                <span className="pv-audit-log__entity" title={e.entityId}>
                  {resolvedName ?? `${e.entityType} · ${e.entityId}`}
                </span>
              ),
              stateChange:
                e.fromState || e.toState ? (
                  <span className="pv-audit-log__state-change">
                    {e.fromState ?? '—'} → {e.toState ?? '—'}
                  </span>
                ) : (
                  '—'
                ),
              detail: (
                <span className="pv-audit-log__detail">{detailText(e)}</span>
              ),
              outcome: (
                <Pill variant={outcomeVariant(e.outcome)}>
                  {outcomeLabel(e.outcome)}
                </Pill>
              ),
              actor: <span className="pv-audit-log__actor">{e.actor}</span>,
            };
          })}
        />
      )}

      {!loading && !loadError && entries.length === 0 && (
        <p className="pv-audit-log__empty">{m.settings_auditlog_empty()}</p>
      )}

      {/* Pagination */}
      <div className="pv-audit-log__pagination">
        <span className="pv-audit-log__page-count">
          {m.settings_auditlog_event_count({ count: total })} &middot;{' '}
          {m.settings_auditlog_page_of({
            current: page + 1,
            total: totalPages,
          })}
        </span>
        <div className="pv-audit-log__page-btns">
          <Btn
            size="sm"
            variant="ghost"
            onClick={() => setPage(Math.max(0, page - 1))}
            disabled={page === 0}
          >
            {m.settings_auditlog_previous()}
          </Btn>
          <Btn
            size="sm"
            variant="ghost"
            onClick={() => setPage(Math.min(totalPages - 1, page + 1))}
            disabled={page >= totalPages - 1}
          >
            {m.settings_auditlog_next()}
          </Btn>
        </div>
      </div>
    </SettingsSection>
  );
}
