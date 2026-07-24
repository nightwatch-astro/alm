// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Shell -- main application shell with sidebar, main content area, status bar.
 * Route hybrid layout: Inbox and Projects get their own right sidebar
 * handling (built into their pages). Other routes use full-width main.
 */

import { lazy, Suspense, useEffect } from 'react';
import { Outlet, useNavigate } from '@tanstack/react-router';
import { usePreferences } from '@/data/preferences';
import { stepZoomIn, stepZoomOut, resetZoom } from '@/data/theme';
import { useHotkeys } from '@/lib/useHotkeys';
import { Sidebar } from './Sidebar';
import { StatusBar } from './StatusBar';
import { LogPanel } from './LogPanel';
import { CommandPalette } from './CommandPalette';
import { LogPanelProvider, useLogPanel } from './LogPanelContext';
import { OperationStatusProvider } from './OperationStatusContext';
import { PageStatusProvider } from './PageStatusContext';
import { ToastContainer } from '@/ui/ToastContainer';
import { useOnboardingState, useWalkActive } from '@/features/onboarding/store';
import { loadObservingState } from '@/features/targets/observing-sites/site-store';
import {
  startUpdateSubscription,
  stopUpdateSubscription,
} from '@/data/updateSubscription';
import { frame, frameBody, frameMain } from './shell.css';

// react-joyride is large (~100 kB gz). The lazy chunk loads only when
// walkReady is true — i.e. never for users who finished the walk and haven't
// requested a replay. walkActive holds the gate open for the full walk
// lifetime so consuming the replay request flag can't collapse it mid-mount.
const OrientationWalk = lazy(() =>
  import('@/features/onboarding/OrientationWalk').then((m) => ({
    default: m.OrientationWalk,
  })),
);

function ShellInner() {
  const prefs = usePreferences();
  const onboarding = useOnboardingState();
  const walkActive = useWalkActive();
  const { expanded } = useLogPanel();
  const navigate = useNavigate();

  // Gate: mount (and load) OrientationWalk when setup is done AND either:
  //   • orientationDone is false — first-run auto-run path, OR
  //   • walkActive is true — a replay was requested or the walk is running.
  // Users with orientationDone=true and no pending/active walk never load the
  // joyride chunk. walkActive is set by requestOrientationReplay BEFORE mount
  // so the gate stays open through the entire walk, surviving consume().
  const walkReady =
    prefs.setupCompleted &&
    (onboarding === null || !onboarding.flags.orientationDone || walkActive);

  // Redirect to /setup if first-run setup is not completed
  useEffect(() => {
    if (!prefs.setupCompleted) {
      void navigate({ to: '/setup' });
    }
  }, [prefs.setupCompleted, navigate]);

  // Spec 044 Track B: hydrate the observing-site live cache once per app
  // session so `useActiveSite()`/`useObservingState()` reflect the persisted
  // sites/threshold immediately (without this, every planner consumer would
  // be stuck on the no-site EMPTY_STATE forever, since nothing else in the
  // app currently calls `loadObservingState()`).
  useEffect(() => {
    if (!prefs.setupCompleted) return;
    void loadObservingState();
  }, [prefs.setupCompleted]);

  // Spec 055 T030: app-owned whole-app zoom shortcuts (VS Code-style,
  // stacks with the font-size dial). Window-local tinykeys bindings, NOT
  // Tauri global shortcuts — WebView2 exposes no zoom-change event, so the
  // app must own every write path; there is no listener to keep in sync.
  useHotkeys(
    {
      '$mod+Equal': (e) => {
        e.preventDefault();
        stepZoomIn();
      },
      '$mod+Shift+Equal': (e) => {
        e.preventDefault();
        stepZoomIn();
      },
      '$mod+NumpadAdd': (e) => {
        e.preventDefault();
        stepZoomIn();
      },
      '$mod+Minus': (e) => {
        e.preventDefault();
        stepZoomOut();
      },
      '$mod+NumpadSubtract': (e) => {
        e.preventDefault();
        stepZoomOut();
      },
      '$mod+Digit0': (e) => {
        e.preventDefault();
        resetZoom();
      },
      '$mod+Numpad0': (e) => {
        e.preventDefault();
        resetZoom();
      },
    },
    [],
    { ignoreFormFields: false },
  );

  // Spec 051 US10 (T058, #888 staged flow): run the frontend-driven update
  // check for the whole app session, so it's captured even if the user
  // hasn't opened Settings > Advanced yet.
  useEffect(() => {
    void startUpdateSubscription();
    return () => stopUpdateSubscription();
  }, []);

  // Spec 051 US5 (T033): native "Settings…" application-menu item — no
  // native dialog of its own, so the backend emits an event and this
  // listener navigates to the existing Settings route. In mock/test mode
  // (VITE_USE_MOCKS=true) there's no native menu to listen for, matching the
  // convention already used by logSubscription.ts/updateSubscription.ts.
  useEffect(() => {
    if (import.meta.env.VITE_USE_MOCKS === 'true') return undefined;

    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void (async () => {
      try {
        const { listen } = await import('@tauri-apps/api/event');
        const fn = await listen('menu:open-settings', () => {
          void navigate({ to: '/settings' });
        });
        if (cancelled) {
          fn();
        } else {
          unlisten = fn;
        }
      } catch (err) {
        console.warn('[Shell] menu:open-settings listen failed:', err);
      }
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [navigate]);

  return (
    <div className={`${frame} density-${prefs.density}`} data-testid="frame">
      <div className={frameBody}>
        <Sidebar />
        <main className={frameMain} data-testid="frame-main">
          <Outlet />
        </main>
      </div>
      {expanded && <LogPanel />}
      <StatusBar />
      <CommandPalette />
      <ToastContainer />
      {walkReady && (
        <Suspense fallback={null}>
          <OrientationWalk />
        </Suspense>
      )}
    </div>
  );
}

export function Shell() {
  return (
    <OperationStatusProvider>
      <PageStatusProvider>
        <LogPanelProvider>
          <ShellInner />
        </LogPanelProvider>
      </PageStatusProvider>
    </OperationStatusProvider>
  );
}
