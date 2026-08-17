// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import './styles/reset.css';
import './styles/tokens.css';
import './styles/components.css';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { RouterProvider } from '@tanstack/react-router';
import { QueryClientProvider } from '@tanstack/react-query';
import { router } from './app/router';
import { AppErrorBoundary } from './app/AppErrorBoundary';
import { queryClient } from './data/queryClient';
import { initAppearance, hydrateThemeFromSettings } from './data/theme';
import { hydrateScope } from './data/persisted-state';
import { registerLocaleStrategy, LocaleProvider } from './data/locale';
import { logError } from './lib/diagnosticLog';

// Register the `custom-almSettings` strategy before the first render, so the
// very first `getLocale()` already consults the stored choice. Without this
// the chain falls through to `preferredLanguage`/`baseLocale` and a saved
// language is ignored on a cold start. Mirrors `initAppearance()` below;
// `LocaleProvider` (wrapped around the router) then reconciles against the
// settings DB, which is the durable source of truth.
registerLocaleStrategy();

// Apply the persisted theme + density to <html> before first paint, and wire
// OS light/dark changes for the `system` choice. Synchronous and driven off
// the localStorage boot cache so there is no flash of the wrong theme.
initAppearance();

// Reconcile that boot cache against the settings DB (spec 018, source of
// truth — theme-settings-db) once IPC is available. Deliberately NOT awaited
// before the initial render: the DB is only consulted after the fast,
// synchronous cache has already painted, and this at most swaps the theme
// once if the two disagree (e.g. localStorage lost to a WebView2 force-kill).
void hydrateThemeFromSettings();

// Reconcile ui_state scope keys (log-panel, grouping dims, picker paths) from
// SQLite. Same fire-and-forget contract as `hydrateThemeFromSettings`: DB is
// the durable source of truth; localStorage is authoritative until this call
// resolves. Handles that register AFTER this call (lazy picker/grouping state
// created on first user interaction) reconcile immediately from the cached DB
// map inside `createPersistedState` — no second IPC round-trip.
void hydrateScope('ui_state');

// T075 / SC-002: Install the recording proxy at boot in dev-tools builds.
// VITE_DEV_TOOLS is statically "false" in release builds, so this branch and
// the bootRecorder/recorder chunks are tree-shaken by the bundler (FR-031).
if (import.meta.env.VITE_DEV_TOOLS === 'true') {
  void import('./dev/bootRecorder').then(({ installRecorder }) =>
    import('./dev/recorder').then(({ wrap }) => installRecorder(wrap)),
  );
}

// Feature 037 US3: expose a real IPC invoke bridge for the WebdriverIO E2E
// journeys. `withGlobalTauri` is off, so `window.__TAURI__` is unavailable in the
// webview; this gives the journeys a way to assert UI -> real-backend round-trips
// (e.g. `roots_list`) over the *real* IPC path. VITE_E2E is statically falsy in
// production builds, so this branch and the ipc chunk reference are tree-shaken
// out (mirrors the VITE_DEV_TOOLS gate above and the VITE_E2E path override).
//
// Fix-lane round 5 (PR #477, `inbox_ui_mixed_folder_splits_into_single_type_items`
// Windows-only failure): the same VITE_E2E gate also buffers uncaught
// errors/rejections into `window.__e2eErrors` and exposes the shared
// `queryClient` + a build-time marker on `window.__PV_E2E__`, so a failing
// real-UI journey can dump (a) whether the UI's own IPC channel ever errored
// (vs. the diagnostic invoke, which bypasses the app's normal query path) and
// (c) whether the served dist is the commit under test. Installed
// synchronously (not inside the dynamic `import()` below) so it captures
// errors from the very first tick, before the ipc chunk resolves.
if (import.meta.env.VITE_E2E) {
  const e2eErrors: string[] = [];
  (window as Window & { __e2eErrors?: string[] }).__e2eErrors = e2eErrors;
  window.addEventListener('error', (event) => {
    e2eErrors.push(`error: ${event.message}`);
  });
  window.addEventListener('unhandledrejection', (event) => {
    e2eErrors.push(`unhandledrejection: ${String(event.reason)}`);
  });

  void import('./api/ipc').then(({ invoke }) => {
    (
      window as Window & {
        __PV_E2E__?: {
          invoke: typeof invoke;
          queryClient: typeof queryClient;
          buildTime: string;
        };
      }
    ).__PV_E2E__ = {
      invoke,
      queryClient,
      buildTime: String(import.meta.env.VITE_BUILD_TIME ?? 'unknown'),
    };
  });
}

// Handoff 07: tell the splash window it can close. `onRendered` fires once
// the first route has actually rendered — for the common path that's after
// `checkFirstRunComplete()`'s real IPC round-trip in the index route's
// `beforeLoad` (router.tsx), so this is closer to "the app responded" than a
// bare paint signal. It is still a boot-paint proxy, not a true
// migration-complete signal — no such IPC surface exists yet (open item,
// see handoff 07 report). No-ops outside a Tauri window (browser dev
// preview, vitest): `emit` rejects there and the splash window doesn't
// exist to listen anyway.
let bootReadySent = false;
const unsubscribeBootReady = router.subscribe('onRendered', () => {
  if (bootReadySent) return;
  bootReadySent = true;
  unsubscribeBootReady();
  void import('@tauri-apps/api/event').then(({ emit }) =>
    emit('app:boot-ready').catch(() => {}),
  );
});

// Forward uncaught errors to the rotating diagnostics log for crash forensics.
// logError() is a no-op outside a Tauri host — safe in all non-production envs.
// Parallel to (not replacing) the VITE_E2E buffer above.
window.addEventListener('error', (event) => {
  logError(
    `[window.onerror] ${event.message} at ${event.filename}:${event.lineno}`,
  );
});
window.addEventListener('unhandledrejection', (event) => {
  logError(`[unhandledrejection] ${String(event.reason)}`);
});

const root = document.getElementById('root');
if (!root) {
  throw new Error('Root element #root is missing from index.html');
}

createRoot(root).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <AppErrorBoundary>
        <LocaleProvider>
          <RouterProvider router={router} />
        </LocaleProvider>
      </AppErrorBoundary>
    </QueryClientProvider>
  </StrictMode>,
);
