// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * LogPanel context (spec 019).
 *
 * Provides:
 * - `expanded` / `toggle` — panel open/close state.
 * - `logLevel` — current `logLevel` settings key (gates diagnostic visibility).
 * - `followLogs` / `setFollowLogs` — persisted follow-tail preference (via
 *   `rememberFollowLogs` settings key from spec 018).
 * - `levelFilter` / `setLevelFilter` — session-only UI level filter.
 * - `sourceFilter` / `setSourceFilter` — session-only source filter (empty = all).
 */
import {
  createContext,
  useContext,
  useState,
  useEffect,
  useCallback,
  useRef,
  type ReactNode,
} from 'react';
import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import { updateSettings } from '@/data/settingsWrite';
import type { LogLevel, LogEntrySource } from '@/data/logStore';
import { startLogSubscription } from '@/data/logSubscription';
import { createPersistedState } from '@/data/persisted-state';

export type LevelFilter = 'all' | LogLevel;

// Durable via SQLite (ui_state scope); localStorage (`pv.ps.*` key) kept as
// synchronous boot cache so the panel opens in the right state before the
// first IPC round-trip.
const logPanelExpandedState = createPersistedState(
  'ui_state',
  'uiState.logPanelExpanded',
  { default: false },
);

/** Test-only: boot-cache localStorage key for the expanded state. */
export const LOG_PANEL_EXPANDED_LS_KEY =
  'pv.ps.uiState.logPanelExpanded' as const;

/**
 * Test-only: the persisted-state instance for expanded state.
 * Lets tests call `.set(true)` to bootstrap state before rendering,
 * equivalent to seeding the old `alm-log-panel-expanded` localStorage key.
 */
export { logPanelExpandedState as _logPanelExpandedStateForTest };

interface LogPanelState {
  expanded: boolean;
  toggle: () => void;
  /** Global log level from settings (gates diagnostic visibility). */
  logLevel: LogLevel;
  /** Follow-tail preference (persisted). */
  followLogs: boolean;
  setFollowLogs: (v: boolean) => void;
  /** Session-only level filter chip. Resets to 'all' on each panel open. */
  levelFilter: LevelFilter;
  setLevelFilter: (v: LevelFilter) => void;
  /** Session-only source filter. Empty array = all sources. */
  sourceFilter: LogEntrySource[];
  setSourceFilter: (v: LogEntrySource[]) => void;
}

const LogPanelContext = createContext<LogPanelState>({
  expanded: false,
  toggle: () => {},
  logLevel: 'info',
  followLogs: false,
  setFollowLogs: () => {},
  levelFilter: 'all',
  setLevelFilter: () => {},
  sourceFilter: [],
  setSourceFilter: () => {},
});

export function LogPanelProvider({ children }: { children: ReactNode }) {
  const [expanded, setExpanded] = useState(() => logPanelExpandedState.get());

  // Subscribe from the provider, which mounts with the Shell, not from LogPanel,
  // which mounts only while `expanded`. The backend forwarder advances its cursor
  // as it emits, so a row emitted while no listener is attached is unreachable to
  // every later drain — delivery cannot depend on the user opening the drawer.
  useEffect(() => {
    void startLogSubscription();
  }, []);

  // Cancel the debounced SQLite write on unmount to prevent timer leaks.
  useEffect(() => () => logPanelExpandedState.cancelPendingWrite(), []);
  const [logLevel, setLogLevel] = useState<LogLevel>('info');
  const [followLogs, setFollowLogsState] = useState(false);
  const [levelFilter, setLevelFilter] = useState<LevelFilter>('all');
  const [sourceFilter, setSourceFilter] = useState<LogEntrySource[]>([]);

  // Set once the user explicitly toggles follow-tail, so the async load below
  // can never overwrite a deliberate choice.
  const followTouchedRef = useRef(false);

  // Load persisted settings on mount (T012, T032).
  //
  // This resolves asynchronously, so it can land AFTER the user has already
  // interacted: open the panel on a slow machine, hit Follow, and the in-flight
  // read arrives a moment later and flips it straight back. The click is the
  // more recent intent and must win — a late read of the very setting the user
  // just changed is stale by definition.
  //
  // Surfaced as a Windows-only CI failure in LogPanel.followScroll.test.tsx
  // ("expected '↓ Follow' to be '— Follow'"): the loaded `true` clobbered the
  // test's toggle-off whenever the loaded runner resolved the promise late
  // enough. The flakiness was the symptom; this race is the defect.
  useEffect(() => {
    let cancelled = false;
    commands
      .settingsGet('advanced')
      .then(unwrap)
      .then((data) => {
        if (cancelled) return;
        const vals = data.values as Record<string, unknown>;
        if (vals?.logLevel && typeof vals.logLevel === 'string') {
          setLogLevel(vals.logLevel as LogLevel);
        }
        if (
          typeof vals?.rememberFollowLogs === 'boolean' &&
          !followTouchedRef.current
        ) {
          setFollowLogsState(vals.rememberFollowLogs);
        }
      })
      .catch(() => {
        // Non-fatal; fall back to defaults.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const toggle = useCallback(() => {
    setExpanded((v) => {
      const next = !v;
      // Reset level filter to 'all' on each open (per spec research R7).
      if (next) {
        setLevelFilter('all');
      }
      logPanelExpandedState.set(next);
      return next;
    });
  }, []);

  const setFollowLogs = useCallback((v: boolean) => {
    // Claim the setting before the mount read can answer (see the effect
    // above) — from here on, the user owns it for this session.
    followTouchedRef.current = true;
    setFollowLogsState(v);
    // Persist via settings.update (spec 018). The Advanced pane reads the
    // `advanced` scope with `staleTime: Infinity`, so this must invalidate.
    void updateSettings({
      scope: 'advanced',
      values: { rememberFollowLogs: v },
    });
  }, []);

  return (
    // eslint-disable-next-line alm/require-root-testid -- a context Provider renders no DOM node, so it cannot carry a data-testid; the panel itself carries one where it mounts
    <LogPanelContext.Provider
      value={{
        expanded,
        toggle,
        logLevel,
        followLogs,
        setFollowLogs,
        levelFilter,
        setLevelFilter,
        sourceFilter,
        setSourceFilter,
      }}
    >
      {children}
    </LogPanelContext.Provider>
  );
}

export function useLogPanel() {
  return useContext(LogPanelContext);
}
