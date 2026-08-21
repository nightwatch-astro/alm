// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import {
  createHashHistory,
  createRouter,
  createRootRoute,
  createRoute,
  lazyRouteComponent,
  redirect,
  Outlet,
} from '@tanstack/react-router';
import { Shell } from './Shell';
import { RouteError } from './RouteError';
import { RouteNotFound } from './RouteNotFound';
import { checkFirstRunComplete } from './first-run';
import {
  makeValidateSearch,
  parseString,
  parseEnum,
  parseCsvEnum,
  FRAME_TYPES,
  INBOX_GROUPS,
  PROJECT_STATES,
  INVENTORY_FRAME_FILTERS,
} from '@/lib/route-contract';
import { DEV_TOOLS_ENABLED } from '@/dev/devToolsEnabled';

/** String `selected` redirect for routes where IDs are UUIDs (e.g. sessions). */
function selectedSearchString(rawId: string): { selected?: string } {
  return rawId && rawId.trim() !== '' ? { selected: rawId.trim() } : {};
}

// Root route — bare Outlet so setup can render without the shell
const rootRoute = createRootRoute({
  component: () => <Outlet />,
});

// Shell layout — wraps all app pages (not setup)
const shellRoute = createRoute({
  getParentRoute: () => rootRoute,
  id: 'shell',
  component: Shell,
});

// --- Sessions (default landing) ---

const sessionsRoute = createRoute({
  getParentRoute: () => shellRoute,
  path: '/sessions',
  validateSearch: makeValidateSearch({
    // Spec 006: session IDs are UUIDs (switched from legacy numeric fixtures).
    selected: parseString,
    // Spec 006 inventory filters — applied server-side by inventory.list.
    sourceFilter: parseEnum(['all'] as const),
    frameFilter: parseEnum(INVENTORY_FRAME_FILTERS),
  }),
  component: lazyRouteComponent(
    () => import('@/features/sessions/SessionsPage'),
    'SessionsPage',
  ),
});

const sessionDetailRoute = createRoute({
  getParentRoute: () => shellRoute,
  path: '/sessions/$id',
  beforeLoad: ({ params }) => {
    throw redirect({
      to: '/sessions',
      search: selectedSearchString(params.id),
    });
  },
});

// --- Inbox (was Review) ---

const inboxRoute = createRoute({
  getParentRoute: () => shellRoute,
  path: '/inbox',
  validateSearch: makeValidateSearch({
    // Issue #644: `selected` is the item's own id, not a list-position index —
    // an index silently points at whatever item now occupies that slot once
    // search/filters change the array shape (Sessions/Calibration already use
    // this id-based pattern).
    selected: parseString,
    type: parseEnum(FRAME_TYPES),
    group: parseEnum(INBOX_GROUPS),
  }),
  component: lazyRouteComponent(
    () => import('@/features/inbox/InboxPage'),
    'InboxPage',
  ),
});

// --- Calibration ---

// spec 007: calibration IDs are UUIDs from the real backend (switched from
// legacy numeric fixtures). Route search uses parseString.
const calibrationRoute = createRoute({
  getParentRoute: () => shellRoute,
  path: '/calibration',
  validateSearch: makeValidateSearch({ selected: parseString }),
  component: lazyRouteComponent(
    () => import('@/features/calibration/CalibrationPage'),
    'CalibrationPage',
  ),
});

const calibrationDetailRoute = createRoute({
  getParentRoute: () => shellRoute,
  path: '/calibration/$id',
  beforeLoad: ({ params }) => {
    throw redirect({
      to: '/calibration',
      search: selectedSearchString(params.id),
    });
  },
});

// --- Targets ---
//
// spec 023: target IDs are UUIDs (strings), not legacy numeric fixture IDs.
// `selected` uses parseString so Cmd+K navigation (which uses UUID routes like
// /targets/550e8400-...) redirects cleanly to /targets?selected=<uuid>.

const targetsRoute = createRoute({
  getParentRoute: () => shellRoute,
  path: '/targets',
  validateSearch: makeValidateSearch({ selected: parseString }),
  component: lazyRouteComponent(
    () => import('@/features/targets/TargetsPage'),
    'TargetsPage',
  ),
});

const targetDetailRoute = createRoute({
  getParentRoute: () => shellRoute,
  path: '/targets/$id',
  beforeLoad: ({ params }) => {
    // Redirect deep-link /targets/<uuid> → /targets?selected=<uuid>
    // so Cmd+K target results land on the list+detail view with the right
    // target pre-selected (spec 023 T008).
    throw redirect({ to: '/targets', search: selectedSearchString(params.id) });
  },
});

// --- Projects ---

const projectsRoute = createRoute({
  getParentRoute: () => shellRoute,
  path: '/projects',
  validateSearch: makeValidateSearch({
    // spec 023: project IDs are UUIDs (strings). Switched from legacy numeric
    // index so linked-project deep-links from TargetDetail work correctly.
    selected: parseString,
    lifecycle: parseCsvEnum(PROJECT_STATES),
  }),
  component: lazyRouteComponent(
    () => import('@/features/projects/ProjectsPage'),
    'ProjectsPage',
  ),
});

const projectDetailRoute = createRoute({
  getParentRoute: () => shellRoute,
  path: '/projects/$id',
  beforeLoad: ({ params }) => {
    throw redirect({
      to: '/projects',
      search: selectedSearchString(params.id),
    });
  },
});

const projectNewRoute = createRoute({
  getParentRoute: () => shellRoute,
  path: '/projects/new',
  // #612: carries the originating target's id from "+ New project here" so
  // the wizard can prefill a real target reference instead of fabricating a
  // "From target context" label from typed text.
  validateSearch: makeValidateSearch({ targetId: parseString }),
  component: lazyRouteComponent(
    () => import('@/features/projects/wizard/WizardPage'),
    'WizardPage',
  ),
});

// --- Archive ---

const archiveRoute = createRoute({
  getParentRoute: () => shellRoute,
  path: '/archive',
  // spec 017 WP-B: archive ids are project UUID strings, not the legacy
  // numeric fixture index.
  validateSearch: makeValidateSearch({ selected: parseString }),
  component: lazyRouteComponent(
    () => import('@/features/archive/ArchivePage'),
    'ArchivePage',
  ),
});

// --- Settings ---

const settingsRoute = createRoute({
  getParentRoute: () => shellRoute,
  path: '/settings',
  component: lazyRouteComponent(
    () => import('@/features/settings/SettingsPage'),
    'SettingsPage',
  ),
});

const settingsPaneRoute = createRoute({
  getParentRoute: () => shellRoute,
  path: '/settings/$pane',
  component: lazyRouteComponent(
    () => import('@/features/settings/SettingsPage'),
    'SettingsPage',
  ),
});

// --- Developer Contract Diagnostics (spec 021, T075) ---
// Compile-time gate: only registered when VITE_DEV_TOOLS="true".
// Release builds set VITE_DEV_TOOLS="false" (default in vite.config.ts),
// so this import and the entire @/dev/ContractsPage chunk are tree-shaken
// out of the production bundle (T072 / FR-031 / SC-009).

const devContractsRoute = DEV_TOOLS_ENABLED
  ? createRoute({
      getParentRoute: () => shellRoute,
      path: '/dev/contracts',
      component: lazyRouteComponent(
        () => import('@/dev/ContractsPage'),
        'ContractsPage',
      ),
    })
  : null;

// Hidden devMode toggle (spec 021 T032). Deliberately NOT added to the
// command palette's DEV_PAGES and NOT linked from Settings — reachable only
// by typing `/dev/settings` directly. Same compile-time gate as
// `devContractsRoute` above.
const devSettingsRoute = DEV_TOOLS_ENABLED
  ? createRoute({
      getParentRoute: () => shellRoute,
      path: '/dev/settings',
      component: lazyRouteComponent(
        () => import('@/dev/DevSettingsPage'),
        'DevSettingsPage',
      ),
    })
  : null;

// --- Setup (standalone, no shell) ---

const setupRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/setup',
  component: lazyRouteComponent(
    () => import('@/features/setup/SetupPage'),
    'SetupPage',
  ),
});

// --- Index redirect (first-run gate) ---

const indexRoute = createRoute({
  getParentRoute: () => shellRoute,
  path: '/',
  beforeLoad: async () => {
    const complete = await checkFirstRunComplete();
    if (!complete) {
      throw redirect({ to: '/setup' });
    }
    // Setup complete: land on the Sessions ledger. We MUST redirect rather than
    // render SessionsPage here — SessionsPage calls useSearch({from:'/shell/sessions'}),
    // which throws an invariant when the active match is the index route ('/shell/').
    throw redirect({ to: '/sessions' });
  },
});

// --- Route tree ---

const routeTree = rootRoute.addChildren([
  setupRoute,
  shellRoute.addChildren([
    indexRoute,
    inboxRoute,
    sessionsRoute,
    sessionDetailRoute,
    calibrationRoute,
    calibrationDetailRoute,
    targetsRoute,
    targetDetailRoute,
    projectNewRoute,
    projectsRoute,
    projectDetailRoute,
    archiveRoute,
    settingsRoute,
    settingsPaneRoute,
    // Developer Contract Diagnostics (spec 021 / T075): only present in dev-tools builds.
    ...(devContractsRoute ? [devContractsRoute] : []),
    // Hidden devMode toggle (spec 021 / T032): only present in dev-tools builds.
    ...(devSettingsRoute ? [devSettingsRoute] : []),
  ]),
]);

// --- Router instance ---

const hashHistory = createHashHistory();

export const router = createRouter({
  routeTree,
  history: hashHistory,
  defaultPreload: 'intent',
  // Scoped error panel instead of a blank app shell when any route throws.
  defaultErrorComponent: RouteError,
  // Unknown routes show a not-found page instead of silently redirecting to
  // home; the index route's beforeLoad still handles first-run gating (T031).
  defaultNotFoundComponent: RouteNotFound,
});

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router;
  }
}
