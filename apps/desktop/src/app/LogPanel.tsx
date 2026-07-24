// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Bottom log panel (spec 019).
 *
 * Migrated to vanilla-extract (pilot/css-vanilla-extract branch).
 */
import {
  useEffect,
  useRef,
  useCallback,
  useSyncExternalStore,
  useState,
} from 'react';
import { m } from '@/lib/i18n';
import { useVirtualizer } from '@tanstack/react-virtual';
import { Collapsible } from '@base-ui-components/react/collapsible';
import { useNavigate } from '@tanstack/react-router';
import { useLogPanel } from './LogPanelContext';
import { subscribeLog, getLogSnapshot } from '@/data/logStore';
import { startLogSubscription } from '@/data/logSubscription';
import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import { errMessage } from '@/lib/errors';
import { useHotkeys } from '@/lib/useHotkeys';
import { EmptyState } from '@/ui/EmptyState';
import { observeElementOffsetWithCleanup } from '@/lib/observe-element-offset';
import {
  LEVEL_CHIPS,
  ALL_LOG_SOURCES,
  passesLevelFilter,
  passesSourceFilter,
  activeFilterLabel,
  buildEntityPath,
  type EntityNavigateFn,
  type AuditNavigateFn,
} from './log-panel-model';
import { useFollowTail } from './useFollowTail';
import { LogEntryRow } from './LogEntryRow';
import {
  logpanel,
  header,
  title,
  filters,
  filtersSources,
  actions,
  chipActive,
  exportError as exportErrorStyle,
  body,
  truncationMarker,
  events,
  empty,
} from './logpanel.css';
import { virtualInner, virtualScroll } from '@/styles/app-shell.css';

// ── LogPanel component ────────────────────────────────────────────────────────

export function LogPanel() {
  const {
    expanded,
    toggle,
    logLevel,
    followLogs,
    setFollowLogs,
    levelFilter,
    setLevelFilter,
    sourceFilter,
    setSourceFilter,
  } = useLogPanel();

  const navigate = useNavigate();
  const listRef = useRef<HTMLUListElement>(null);
  const [showDiagnostics, setShowDiagnostics] = useState(false);
  const [exportError, setExportError] = useState<string | null>(null);

  const prefersReducedMotion =
    typeof window !== 'undefined' &&
    window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;

  const { entries, truncated, truncatedCount } = useSyncExternalStore(
    subscribeLog,
    getLogSnapshot,
  );

  const visibleEntries = entries.filter((entry) => {
    if (!passesLevelFilter(entry.level, levelFilter)) return false;
    if (!passesSourceFilter(entry.source, sourceFilter)) return false;
    if (entry.source === 'diagnostic' && !showDiagnostics) return false;
    if (entry.source === 'diagnostic' && logLevel !== 'debug') return false;
    return true;
  });

  const filterLabel = activeFilterLabel(levelFilter, sourceFilter);

  const virtualizer = useVirtualizer({
    count: visibleEntries.length,
    getScrollElement: () => listRef.current,
    estimateSize: () => 28,
    overscan: 12,
    observeElementOffset: observeElementOffsetWithCleanup,
  });

  useEffect(() => {
    void startLogSubscription();
  }, []);

  useHotkeys(
    {
      Escape: (e) => {
        if (!expanded) return;
        e.preventDefault();
        toggle();
      },
    },
    [expanded, toggle],
    { ignoreFormFields: false },
  );

  const { scrollPaused, handleScroll, toggleFollow } = useFollowTail({
    expanded,
    followLogs,
    setFollowLogs,
    entryCount: visibleEntries.length,
    virtualizer,
    listRef,
    prefersReducedMotion,
  });

  const navigateToEntity: EntityNavigateFn = useCallback(
    (entityType, entityId) => {
      const path = buildEntityPath(entityType, entityId);
      if (path == null) return;
      void navigate({ to: path as never });
    },
    [navigate],
  );

  const navigateToAudit: AuditNavigateFn = useCallback(
    (requestId) => {
      void navigate({ to: `/settings/audit?requestId=${requestId}` as never });
    },
    [navigate],
  );

  const handleExport = useCallback(async () => {
    setExportError(null);
    try {
      const requestId = crypto.randomUUID?.() ?? `req-${Date.now()}`;
      let filePath: string | null = null;
      try {
        const { save: showSaveDialog } = await import(
          '@tauri-apps/plugin-dialog'
        );
        filePath = await showSaveDialog({
          title: m.logpanel_save_dialog_title(),
          defaultPath: `astro-log-export-${Date.now()}.json`,
          filters: [{ name: 'JSON', extensions: ['json'] }],
        });
      } catch {
        filePath = null;
      }

      if (!filePath) return;

      unwrap(
        await commands.logExport(
          requestId,
          filePath,
          'json',
          null,
          null,
          null,
          showDiagnostics,
        ),
      );
    } catch (err) {
      setExportError(errMessage(err));
    }
  }, [showDiagnostics]);

  return (
    <Collapsible.Root
      open={expanded}
      onOpenChange={toggle}
      className={logpanel}
      data-testid="log-panel"
      role="log"
      aria-label={m.logpanel_aria_label()}
    >
      <div className={header}>
        <span className={title}>{m.logpanel_title()}</span>

        {expanded && (
          <div
            className={filters}
            role="group"
            aria-label={m.logpanel_level_filter_aria()}
          >
            {LEVEL_CHIPS.map((chip) => (
              <button
                key={chip.value}
                type="button"
                className={`pv-btn pv-btn--ghost pv-btn--xs${
                  levelFilter === chip.value ? ` ${chipActive}` : ''
                }`}
                onClick={() => setLevelFilter(chip.value)}
                aria-pressed={levelFilter === chip.value}
                aria-label={
                  chip.value === 'all' ? m.logpanel_level_all_aria() : undefined
                }
              >
                {chip.label()}
              </button>
            ))}

            {logLevel === 'debug' && (
              <button
                type="button"
                className={`pv-btn pv-btn--ghost pv-btn--xs${
                  showDiagnostics ? ` ${chipActive}` : ''
                }`}
                onClick={() => setShowDiagnostics((v) => !v)}
                aria-pressed={showDiagnostics}
              >
                {m.logpanel_diagnostics()}
              </button>
            )}
          </div>
        )}

        {expanded && (
          <div
            className={filtersSources}
            role="group"
            aria-label={m.logpanel_source_filter_aria()}
          >
            <button
              type="button"
              className={`pv-btn pv-btn--ghost pv-btn--xs${
                sourceFilter.length === 0 ? ` ${chipActive}` : ''
              }`}
              onClick={() => setSourceFilter([])}
              aria-pressed={sourceFilter.length === 0}
              aria-label={m.logpanel_source_all_aria()}
            >
              {m.common_all()}
            </button>
            {ALL_LOG_SOURCES.map((source) => (
              <button
                key={source}
                type="button"
                className={`pv-btn pv-btn--ghost pv-btn--xs${
                  sourceFilter.length === 0 || sourceFilter.includes(source)
                    ? ` ${chipActive}`
                    : ''
                }`}
                onClick={() =>
                  setSourceFilter(
                    sourceFilter.length === 0
                      ? [source]
                      : sourceFilter.includes(source)
                        ? sourceFilter.filter((s) => s !== source)
                        : [...sourceFilter, source],
                  )
                }
                aria-pressed={
                  sourceFilter.length === 0 || sourceFilter.includes(source)
                }
              >
                {source}
              </button>
            ))}
          </div>
        )}

        <div className={actions}>
          {expanded && (
            <button
              type="button"
              className={`pv-btn pv-btn--ghost pv-btn--xs${followLogs ? ` ${chipActive}` : ''}`}
              onClick={toggleFollow}
              aria-pressed={followLogs}
              aria-label={
                followLogs
                  ? m.log_follow_tail_on_aria()
                  : m.log_follow_tail_off_aria()
              }
              title={
                scrollPaused && followLogs
                  ? m.log_follow_tail_paused_title()
                  : undefined
              }
            >
              {followLogs
                ? scrollPaused
                  ? m.logpanel_follow_paused()
                  : m.logpanel_follow_active()
                : m.logpanel_follow_off()}
            </button>
          )}

          {expanded && (
            <button
              type="button"
              className="pv-btn pv-btn--ghost pv-btn--xs"
              onClick={() => void handleExport()}
              aria-label={m.logpanel_export_aria()}
            >
              {m.logpanel_export()}
            </button>
          )}

          <Collapsible.Trigger
            className="pv-btn pv-btn--ghost pv-btn--sm"
            aria-label={
              expanded ? m.log_collapse_panel_aria() : m.log_expand_panel_aria()
            }
          >
            {expanded ? '▾' : '▸'}
          </Collapsible.Trigger>
        </div>
      </div>

      {exportError && (
        <div className={exportErrorStyle} role="alert">
          {m.logpanel_export_failed({ error: exportError ?? '' })}
        </div>
      )}

      <Collapsible.Panel className={body}>
        {truncated && (
          <div
            className={truncationMarker}
            data-testid="logpanel-truncation-marker"
            role="note"
          >
            {truncatedCount != null
              ? m.logpanel_history_gap_count({ count: String(truncatedCount) })
              : m.logpanel_history_gap()}
          </div>
        )}

        <ul
          className={`${events} ${virtualScroll}`}
          ref={listRef}
          onScroll={handleScroll}
          data-virtual-scroll="true"
          data-testid="logpanel-events"
        >
          {visibleEntries.length === 0 ? (
            <li className={empty}>
              <EmptyState
                title={
                  entries.length === 0
                    ? m.logpanel_empty()
                    : filterLabel != null
                      ? m.logpanel_empty_filtered_named({ filter: filterLabel })
                      : m.logpanel_empty_filtered()
                }
              />
            </li>
          ) : (
            <div
              className={virtualInner}
              // eslint-disable-next-line no-restricted-syntax
              style={{
                height: `${virtualizer.getTotalSize()}px`,
                position: 'relative',
              }}
            >
              {virtualizer.getVirtualItems().map((virtualRow) => {
                const entry = visibleEntries[virtualRow.index];
                return (
                  <LogEntryRow
                    key={entry.id}
                    entry={entry}
                    index={virtualRow.index}
                    measureRef={virtualizer.measureElement}
                    // eslint-disable-next-line no-restricted-syntax
                    style={{
                      position: 'absolute',
                      top: 0,
                      left: 0,
                      right: 0,
                      transform: `translateY(${virtualRow.start}px)`,
                    }}
                    onNavigateEntity={navigateToEntity}
                    onNavigateAudit={navigateToAudit}
                  />
                );
              })}
            </div>
          )}
        </ul>
      </Collapsible.Panel>
    </Collapsible.Root>
  );
}
