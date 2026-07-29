// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import type {
  AuditEntry,
  SearchResult,
  LibraryRoot,
  Equipment,
  SettingsData,
  AppPreferences,
  CalendarData,
  MasterDetail,
  MatchCandidate,
} from '@/bindings/types';
import type {
  CalibrationTolerances,
  CleanupPolicy,
  UpdateCleanupPolicy,
  InboxListResponse_Serialize,
  InboxClassifySourceGroupResponse,
  InboxScanFolderResponse_Serialize,
  InboxClassifyResponse_Serialize,
  IngestionAttributionCandidateDto_Serialize,
  InboxConfirmResponse_Serialize,
  InboxConfirmActionsSummary,
  InboxConfirmDestination,
  InboxOpenPlansResponse,
  InboxOpenPlan,
  InboxPlanAction,
  InboxPlanView,
  InboxApplyAllResponse,
  InboxPlanCancelResponse,
  InboxStatsResponse,
  InboxItemMetadataResponse_Serialize,
  InboxFileMetadata_Serialize,
  PropertyRegistryEntry_Serialize,
  GenerateArchivePlanResult,
  TransitionError_Serialize,
  InboxReclassifyResponse_Serialize,
  InboxReclassifyV2Response_Serialize,
  TargetDetailV3_Serialize,
  TargetSearchResponse_Serialize,
  TargetResolveSimbadResponse_Serialize,
  TargetMoonOppositionBatchRequest,
  TargetMoonOppositionBatchResponse,
  ProjectCreateResult_Serialize,
  ProjectUpdateResult,
  ProjectSourceAddResult_Serialize,
  ProjectSourceRemoveResult_Serialize,
  ProjectChannelsReinferResult_Serialize,
  ProjectChannelsDismissDriftResult,
  TransitionResponse_Serialize,
  LogRecentResponse_Serialize,
  LogExportResponse_Serialize,
  FirstRunRestartResponse,
  FirstRunStateResponse_Serialize,
  FirstRunCompleteResponse,
  RegisterSourceBatchResponse_Serialize,
  RemapVerification,
  ToolPathValidation,
  IpcOperationHandle,
  OperationEvent,
  PlanApplyResponse,
  AuditListResponse_Serialize,
  ArchiveListResponse,
  ArchiveSendToTrashResponse,
  ArchivePermanentlyDeleteResponse,
  Camera,
  CreateCamera,
  UpdateCamera,
  Telescope,
  CreateTelescope,
  UpdateTelescope,
  OpticalTrain,
  CreateOpticalTrain,
  UpdateOpticalTrain,
  Filter,
  CreateFilter,
  UpdateFilter,
  CalibrationMatchSuggestResponse,
  CalibrationMatchBatchResponse,
  CalibrationMatchDto_Serialize,
  IngestionSettings,
  UpdateIngestionSettings,
  AuditFilterDto,
  AuditPaginationDto,
  PathPatternPreviewResponse,
  ProjectNoteGetResult,
  ProjectNoteUpdateResult,
  ManifestListResponse_Serialize,
  PlanSummary_Serialize,
  OnboardingStateDto,
  OnboardingItemDto,
  OnboardingFlagsDto,
  // Value imported in type position only, so this stays erased at runtime and
  // does not create an import cycle with `ipc.ts` (which the bindings import).
  commands,
} from '@/bindings/index';
import {
  PATH_PREVIEW_TOKEN_FIELDS,
  PATH_PREVIEW_TOKEN_FALLBACKS,
} from './mocks/path-preview';
import { filterMockAuditEntries } from './mocks/audit';
import { mockSettingsData, mockPreferences } from './mocks/settings';
import { mockSearchResults } from './mocks/targets';
import {
  mockCalendarData,
  mockMasterDetail,
  mockMatchCandidates,
  mockCalibrationMatches,
} from './mocks/calibration';
import {
  mockMoveAction,
  mockCatalogueAction,
  seedInboxOpenPlans,
  MOCK_ORGANIZED_ITEM_IDS,
  MOCK_ATTRIBUTION_CANDIDATES,
  MOCK_PLAN_REQUIRED_EDGES,
} from './mocks/inbox';
import {
  E2E_ONBOARDING_UNMET_STORE_ID,
  isE2EFlagSet,
  E2E_EMPTY_INVENTORY_STORE_ID,
  unmetPrerequisiteIds,
  freshMockOnboardingItems,
  E2E_ONBOARDING_STORE_ID,
  type OnboardingSeed,
} from './mocks/onboarding';

const delay = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

// ── `observing`-scope settings (spec 044 Track B + spec 047) — scope-aware ─────
//
// The planner's observing-site gate (`features/targets/site-gate.ts` →
// `activeSite() !== null`) hydrates the site store from
// `settings_get('observing')` (`observing-sites/site-store.ts`), and the usable
// altitude threshold (`altitude-settings.ts`) reads the same scope. To let
// mock-mode exercise BOTH planner states — the no-site "set up your observing
// site" prompt (spec 047 D7 / edge case) AND the with-site astronomy render —
// this scope reflects a per-session values bag seeded from the
// `pv-e2e-observing` localStorage key (set by a test before navigation):
//
//   - key ABSENT  → empty observing values → no active site → planner GATED.
//   - key PRESENT → seeded sites + active pointer → `activeSite() !== null` →
//                   planner renders real 044 (altitude/imaging-time) + 047
//                   (moon phase / lunar separation / filter guidance /
//                   opposition) values against that site.
//
// `settings_update('observing', …)` merges into the same bag so a UI-driven
// site creation (Settings → Observing Sites) round-trips like the real backend.
// Non-observing scopes are untouched and still resolve to `mockSettingsData`.
const E2E_OBSERVING_SEED_STORE_ID = 'pv-e2e-observing';
let mockObservingValues: Record<string, unknown> | null = null;

function observingValues(): Record<string, unknown> {
  if (mockObservingValues === null) {
    let seeded: Record<string, unknown> = {};
    try {
      const raw =
        typeof localStorage !== 'undefined'
          ? localStorage.getItem(E2E_OBSERVING_SEED_STORE_ID)
          : null;
      if (raw) seeded = JSON.parse(raw) as Record<string, unknown>;
    } catch {
      seeded = {};
    }
    mockObservingValues = seeded;
  }
  return mockObservingValues;
}

// Mutable so `cleanup_policy_update` round-trips through `cleanup_policy_get`
// in mock mode (issue #804). Seeded at `default_cleanup_policy()` — all-Keep,
// no auto-run — so mock mode reproduces the real fresh-install state, where a
// scan legitimately finds nothing until the user opts a data type in.
let mockCleanupPolicy: CleanupPolicy = {
  entries: [
    { dataType: 'intermediate', action: 'keep' },
    { dataType: 'master', action: 'keep' },
    { dataType: 'final', action: 'keep' },
  ],
  autoOnCompletion: false,
};

// Mutable so `settings_update('framing', …)` round-trips through
// `settings_get('framing')` in mock mode (spec 008 Q27 F-Framing-11) —
// the Settings → Framing pane's four tunables need a real persisted bag to
// prove auto-save actually sticks. Seeded at the R11a shipped defaults
// (mirrors `domain_core::settings::SettingsState::default()`).
let mockFramingSettings: Record<string, unknown> = {
  framingPointingFractionOfFov: 0.1,
  framingPointingFallbackDeg: 0.2,
  framingRotationToleranceDeg: 3.0,
  framingMosaicEnvelopeFractionOfFov: 1.0,
};

// Mutable so `ingestion_settings_update` round-trips through `_get` in mock
// mode (spec 030, package P12) — mirrors real persistence closely enough for
// the Ingestion settings pane's load/save flow to be exercised without a
// backend.
let mockIngestionSettings: IngestionSettings = {
  watcherEnabled: true,
  scanOnStartup: true,
  followSymlinks: false,
  followJunctions: false,
  hashingMode: 'lazy',
  metadataExtraction: true,
  exposureGroupingToleranceS: 2,
  temperatureGroupingToleranceC: 5,
  defaultFilter: null,
};

// Spec 007 / spec 043 P8 — mirrors the persisted `calibration_tolerances`
// singleton row's real defaults (migration 0008 + 0051), including
// `requireSameOffset` (STUB-OFFSET-REQUIRED closed: this is now a real,
// persisted field, not a local-only stub).
//
// Mutable so `calibration_tolerances_update` round-trips through `_get` in mock
// mode (mirrors `persistence_db::repositories::calibration_tolerances`'s
// upsert-then-return): the Calibration-matching settings pane's edited
// tolerance survives a component remount, exactly like the real singleton row.
// (Module state resets per page context, so tests stay isolated.)
let mockCalibrationTolerances: CalibrationTolerances = {
  temperatureToleranceC: 5.0,
  exposureToleranceS: 2.0,
  agingLimitDays: 365,
  requireSameCamera: true,
  requireSameGain: true,
  requireSameBinning: true,
  requireSameOffset: true,
};

// ── Onboarding (spec 056) ─────────────────────────────────────────────────
//
// Mutable in-memory state for onboarding items and flags. Seed data and pure
// helpers live in ./mocks/onboarding.ts; the mutable variables and functions
// that mutate them stay here so handler closures can reassign them directly.

let mockOnboardingItems: OnboardingItemDto[] = freshMockOnboardingItems();
let mockOnboardingFlags: OnboardingFlagsDto = {
  orientationDone: false,
  sectionHidden: false,
  sidebarCollapsed: false,
};

let onboardingHydrated = false;

function hydrateOnboarding(): void {
  if (onboardingHydrated) return;
  onboardingHydrated = true;
  try {
    const raw =
      typeof localStorage !== 'undefined'
        ? localStorage.getItem(E2E_ONBOARDING_STORE_ID)
        : null;
    if (!raw) return;
    const seed = JSON.parse(raw) as OnboardingSeed;
    if (seed.flags) {
      mockOnboardingFlags = { ...mockOnboardingFlags, ...seed.flags };
    }
    if (seed.items) {
      mockOnboardingItems = mockOnboardingItems.map((i) => {
        const s = seed.items?.[i.itemId];
        return s ? { ...i, state: s.state, source: s.source ?? i.source } : i;
      });
    }
  } catch {
    // ignore malformed seed — fall back to the fresh defaults
  }
}

function persistOnboarding(): void {
  try {
    if (typeof localStorage === 'undefined') return;
    const items: NonNullable<OnboardingSeed['items']> = {};
    for (const i of mockOnboardingItems) {
      if (i.state !== 'unchecked') {
        items[i.itemId] = { state: i.state, source: i.source };
      }
    }
    localStorage.setItem(
      E2E_ONBOARDING_STORE_ID,
      JSON.stringify({
        flags: mockOnboardingFlags,
        items,
      } satisfies OnboardingSeed),
    );
  } catch {
    // best-effort persistence; never throw from a mock handler
  }
}

function mockOnboardingStateDto(): OnboardingStateDto {
  hydrateOnboarding();
  const perPageMap = new Map<
    OnboardingItemDto['page'],
    { done: number; total: number }
  >();
  for (const item of mockOnboardingItems) {
    const entry = perPageMap.get(item.page) ?? { done: 0, total: 0 };
    entry.total += 1;
    if (item.state !== 'unchecked') entry.done += 1;
    perPageMap.set(item.page, entry);
  }
  const pageOrder: OnboardingItemDto['page'][] = [
    'inbox',
    'sessions',
    'calibration',
    'targets',
    'projects',
  ];
  const perPage = pageOrder.flatMap((page) => {
    const entry = perPageMap.get(page);
    return entry ? [{ page, ...entry }] : [];
  });
  const done = perPage.reduce((sum, p) => sum + p.done, 0);
  const total = perPage.reduce((sum, p) => sum + p.total, 0);
  return {
    items: mockOnboardingItems.map((i) => ({ ...i })),
    flags: { ...mockOnboardingFlags },
    progress: { done, total, perPage },
  };
}

let mockRoots: LibraryRoot[] = [
  {
    id: 'root-001',
    path: '/astro/raw',
    category: 'raw',
    online: true,
    fileCount: 1247,
    lastScanned: '2026-05-19T23:30:00Z',
    active: true,
  },
  {
    id: 'root-002',
    path: '/astro/calibration',
    category: 'calibration',
    online: true,
    fileCount: 342,
    lastScanned: '2026-05-19T23:30:00Z',
    active: true,
  },
  {
    id: 'root-003',
    path: '/astro/projects',
    category: 'project',
    online: true,
    fileCount: 856,
    lastScanned: '2026-05-18T20:00:00Z',
    active: true,
  },
];

const mockEquipment: Equipment[] = [
  {
    id: 'eq-001',
    name: 'ASI2600MM Pro',
    kind: 'camera',
    aliases: ['ZWO ASI2600MM'],
  },
  {
    id: 'eq-002',
    name: 'Esprit 100ED',
    kind: 'telescope',
    aliases: ['SW Esprit 100ED'],
  },
  { id: 'eq-003', name: 'EQ6-R Pro', kind: 'mount', aliases: ['EQ6R'] },
];

// ── Equipment CRUD (spec 030) ────────────────────────────────────────────────
//
// Mutable in-memory stores so mock mode's add/edit/delete flows behave like
// the real backend across a session (previously `@/data/fixtures/settings`,
// which the Equipment pane held in local `useState` and never persisted
// through an IPC round-trip). Seed data replaces those retired fixtures.

let mockCameras: Camera[] = [
  {
    id: 'cam-001',
    name: 'ASI2600MM Pro',
    aliases: ['ZWO ASI2600MM'],
    autoDetected: false,
    sensorType: 'mono',
    passband: null,
    pixelSizeUm: 3.76,
    sensorWidthPx: 6248,
    sensorHeightPx: 4176,
  },
  {
    // No sensor geometry: exercises the absent-FOV branch in mock mode, which
    // is the common state for a camera registered before migration 0079.
    id: 'cam-002',
    name: 'ASI533MC Pro',
    aliases: ['ZWO ASI533MC'],
    autoDetected: false,
    sensorType: 'osc',
    passband: null,
    pixelSizeUm: null,
    sensorWidthPx: null,
    sensorHeightPx: null,
  },
];

// Target favourites (spec 051 US2): id → favourited-at timestamp, mirroring
// the real `target_favourite` table's shape (row presence = favourited).
const mockFavourites = new Map<string, string>();

let mockTelescopes: Telescope[] = [
  {
    id: 'tel-001',
    name: 'Takahashi FSQ-106EDX4',
    aliases: [],
    focalLengthMm: 530,
    autoDetected: false,
  },
  {
    id: 'tel-002',
    name: 'William Optics GT81',
    aliases: [],
    focalLengthMm: 478,
    autoDetected: false,
  },
];

let mockOpticalTrains: OpticalTrain[] = [
  {
    id: 'train-001',
    name: 'FSQ-106 + ASI2600MM',
    telescopeId: 'tel-001',
    cameraId: 'cam-001',
    focalLengthMm: 530,
    // Fixture value for cam-001's geometry at 530 mm. Held as a constant
    // rather than recomputed here: the FOV formula lives in the backend
    // (`sessions::fov_diagonal_deg`) and must not be reimplemented in TS.
    fovDiagonalDeg: 3.05,
  },
  {
    // Linked to the geometry-less cam-002, so mock mode renders the
    // "not known" branch alongside a real value.
    id: 'train-002',
    name: 'GT81 + ASI533MC',
    telescopeId: 'tel-002',
    cameraId: 'cam-002',
    focalLengthMm: 478,
    fovDiagonalDeg: null,
  },
];

let mockFilters: Filter[] = [
  { id: 'filt-001', name: 'Ha', category: 'narrowband', autoDetected: false },
  { id: 'filt-002', name: 'OIII', category: 'narrowband', autoDetected: false },
  { id: 'filt-003', name: 'SII', category: 'narrowband', autoDetected: false },
  { id: 'filt-004', name: 'L', category: 'broadband', autoDetected: false },
  { id: 'filt-005', name: 'R', category: 'broadband', autoDetected: false },
  { id: 'filt-006', name: 'G', category: 'broadband', autoDetected: false },
  { id: 'filt-007', name: 'B', category: 'broadband', autoDetected: false },
];

/** Mirrors the shape `unwrap()` expects on the error branch of a `Result`. */
function mockContractError(code: string, message: string): never {
  throw { code, message, severity: 'blocking', retryable: false };
}

// Review items are loaded from the wireframe-aligned fixture file (review.queue case below).

// ── Inbox plan surface (spec 041) — stateful mock ─────────────────────────────
//
// Apply/cancel MUTATE this array (removing the applied/cancelled plan) so the
// aggregate surface refresh + auto-close behaviour round-trips like the backend.
// Seed data helpers (mockMoveAction, mockCatalogueAction, seedInboxOpenPlans)
// live in ./mocks/inbox.ts and are imported above.

let mockInboxOpenPlans: InboxOpenPlan[] = seedInboxOpenPlans();

/**
/**
 * Inbox items the mock has classified as LIGHT frames.
 *
 * Attribution is light-frame-only: `attribution::suggest_candidates` returns an
 * empty list for anything else, gated on `confirm::evidence_is_light`. The mock
 * must reproduce that gate, not just the happy path — returning candidates for
 * every item made the picker intercept the confirm of a DARK item, so the
 * reviewable-plan toast never fired (`inbox_ingest_confirm.spec.ts`).
 *
 * `inbox_classify` reports `dark` for every fixture item, so this stays empty
 * until `inbox_reclassify` (which reports `frameType: 'light'`) puts an item in
 * it — which is what makes the picker reachable in mock mode.
 */
const mockLightInboxItemIds = new Set<string>();

/**
 * snake_case Tauri wire name for a camelCase generated-binding key.
 *
 * Tauri registers each command under its Rust fn name; tauri-specta exposes it
 * camelCased. Verified exact across all 197 generated commands. A future
 * command that breaks the convention fails to compile here rather than
 * silently missing its mock at runtime.
 */
type WireName<S extends string> = S extends `${infer H}${infer R}`
  ? H extends Lowercase<H>
    ? `${H}${WireName<R>}`
    : `_${Lowercase<H>}${WireName<R>}`
  : S;

/** The value a command resolves to, unwrapped from its `Result` envelope. */
type CommandPayload<F> = F extends (...args: never[]) => Promise<infer R>
  ? R extends { status: 'ok'; data: infer D }
    ? D
    : never
  : never;

/**
 * Mock fixtures keyed by Tauri wire command name.
 *
 * Every handler's return value is checked against the payload type of the
 * matching generated binding, so a contract change the mock fails to mirror is
 * a `tsc --noEmit` error. Keys are checked too: a mock for a command that no
 * longer exists does not compile.
 *
 * Partial by design — unmocked commands throw loudly at dispatch rather than
 * returning a fabricated payload. `scripts/check-mock-baseline.mjs` tracks how
 * many stay unmocked so newly generated commands surface.
 *
 * LIMIT: this catches SHAPE drift only. A fixture that is well-typed but
 * semantically wrong (plausible values the backend would never produce for
 * that input) stays invisible here. That is an argument for real-UI coverage,
 * not a reason to trust mock-mode specs on their own.
 */
type MockRegistry = {
  [K in keyof typeof commands as WireName<K & string>]?: (
    args?: Record<string, unknown>,
  ) => Promise<CommandPayload<(typeof commands)[K]>>;
};

const mockHandlers = {
  // ---------- Query Commands ----------

  sessions_list: async () => {
    const { sessions } = await import('@/data/fixtures/sessions');
    return sessions;
  },
  sessions_get: async () => {
    const { sessionDetail } = await import('@/data/fixtures/sessions');
    return sessionDetail;
  },
  sessions_calendar: async () => {
    return mockCalendarData;
  },
  calibration_masters_list: async () => {
    const { masters } = await import('@/data/fixtures/calibration');
    return masters;
  },
  calibration_masters_get: async () => {
    return mockMasterDetail;
  },
  calibration_matches: async () => {
    return mockMatchCandidates;
  },
  calibration_match_suggest: async (_args) => {
    const req = (
      _args as { req?: { requestId?: string; sessionId?: string } } | undefined
    )?.req;
    const sessionId = req?.sessionId ?? 'ses-001';
    return {
      status: 'success',
      contractVersion: '2.0.0',
      requestId: req?.requestId ?? crypto.randomUUID(),
      suggestStatus: 'match',
      matches: mockCalibrationMatches(sessionId),
    } satisfies CalibrationMatchSuggestResponse;
  },
  calibration_match_suggest_batch: async (_args) => {
    const req = (
      _args as
        | { req?: { requestId?: string; sessionIds?: string[] } }
        | undefined
    )?.req;
    const sessionIds = req?.sessionIds ?? ['ses-001'];
    return {
      status: 'success',
      contractVersion: '1.0',
      requestId: req?.requestId ?? crypto.randomUUID(),
      results: sessionIds.map((sessionId) => ({
        sessionId,
        calibrationType: 'dark' as const,
        status: 'match',
        candidates: mockCalibrationMatches(sessionId),
      })),
    } satisfies CalibrationMatchBatchResponse;
  },
  calibration_tolerances_get: async () => {
    return mockCalibrationTolerances;
  },
  // ── gen-3 target commands (spec 036) ──────────────────────────────────────
  target_list: async (_args) => {
    const search = (_args as { search?: string | null } | undefined)?.search;
    const items = [
      {
        id: 'tgt-m31',
        effectiveLabel: 'M 31',
        primaryDesignation: 'M 31',
        objectType: 'galaxy',
        raDeg: 10.6847,
        decDeg: 41.269,
        constellation: 'Andromeda',
        magnitude: 3.44,
        sessionCount: 3,
      },
      {
        id: 'tgt-ngc7000',
        effectiveLabel: 'NGC 7000',
        primaryDesignation: 'NGC 7000',
        objectType: 'emission_nebula',
        raDeg: 314.75,
        decDeg: 44.52,
        constellation: 'Cygnus',
        magnitude: 4.0,
        sessionCount: 5,
      },
    ];
    if (!search) return items;
    const q = search.toLowerCase();
    return items.filter(
      (t) =>
        t.primaryDesignation.toLowerCase().includes(q) ||
        t.effectiveLabel.toLowerCase().includes(q),
    );
  },
  target_get: async (_args) => {
    // tauri-specta wraps every named parameter as a top-level object key, so
    // `target_get(req: TargetGetRequest)` arrives as `{ req: { targetId } }`.
    const req = (_args as { req?: { targetId?: string } } | undefined)?.req;
    return {
      id: req?.targetId ?? 'tgt-m31',
      primaryDesignation: 'M 31',
      effectiveLabel: 'M 31',
      displayAlias: null,
      objectType: 'galaxy',
      raDeg: 10.68,
      decDeg: 41.27,
      simbadOid: 1_234_567,
      source: 'resolved',
      aliases: [],
    } satisfies TargetDetailV3_Serialize;
  },
  target_favourites_list: async () => {
    return { targetIds: [...mockFavourites.keys()] };
  },
  target_favourites_add: async (_args) => {
    const req = (_args as { req?: { targetId?: string } } | undefined)?.req;
    const targetId = req?.targetId ?? '';
    const favouritedAt =
      mockFavourites.get(targetId) ?? new Date().toISOString();
    mockFavourites.set(targetId, favouritedAt);
    return { targetId, favouritedAt };
  },
  target_favourites_remove: async (_args) => {
    const req = (_args as { req?: { targetId?: string } } | undefined)?.req;
    const targetId = req?.targetId ?? '';
    mockFavourites.delete(targetId);
    return { targetId };
  },
  target_moon_opposition_batch: async (_args) => {
    // #634: mock mode reuses the SAME TS ephemeris (`astro/moon-state.ts` +
    // `astro/lunar-separation.ts` + `astro/opposition.ts`) the detail panel
    // (`TargetDetail`'s Best-date tooltip, still TS per ADR-0001's
    // interactive-ephemeris boundary) computes independently — a
    // hand-rolled placeholder here would diverge from the detail panel's
    // value and break the "list Opposition == detail Best date" mock-mode
    // regression guard (e2e 9.5c). This mirrors what the real Rust command
    // approximates (both derive the same physical quantity); it is NOT the
    // real backend, only its mock-mode stand-in.
    const [{ moonStateAt }, { lunarSeparationDeg }, { nextOpposition }] =
      await Promise.all([
        import('@/features/targets/astro/moon-state'),
        import('@/features/targets/astro/lunar-separation'),
        import('@/features/targets/astro/opposition'),
      ]);
    const req = (
      _args as { req?: TargetMoonOppositionBatchRequest } | undefined
    )?.req;
    const at = req?.at ? new Date(req.at) : new Date();
    const targets = req?.targets ?? [];
    const { moonVec } = moonStateAt(at);
    return {
      results: targets.map((t) => {
        if (
          t.raDeg == null ||
          t.decDeg == null ||
          !Number.isFinite(t.raDeg) ||
          !Number.isFinite(t.decDeg)
        ) {
          return { id: t.id, moonSeparationDeg: null, opposition: null };
        }
        const moonSeparationDeg = lunarSeparationDeg(
          t.raDeg,
          t.decDeg,
          moonVec,
        );
        const opposition = nextOpposition(t.raDeg, at);
        return {
          id: t.id,
          moonSeparationDeg,
          opposition: opposition
            ? {
                date: opposition.date.toISOString(),
                daysUntil: opposition.daysUntil,
              }
            : null,
        };
      }),
    } satisfies TargetMoonOppositionBatchResponse;
  },
  target_search: async (_args) => {
    const req = (_args as { req?: { query?: string } } | undefined)?.req;
    const q = (req?.query ?? '').toLowerCase();
    const allSuggestions = [
      {
        targetId: 'tgt-m31',
        primaryDesignation: 'M 31',
        commonName: 'Andromeda Galaxy',
        objectType: 'galaxy',
        matchedAlias: 'Andromeda',
        source: 'seed',
      },
      {
        targetId: 'tgt-ngc7000',
        primaryDesignation: 'NGC 7000',
        commonName: null,
        objectType: 'emission_nebula',
        matchedAlias: 'North America Nebula',
        source: 'seed',
      },
    ] satisfies TargetSearchResponse_Serialize['suggestions'];
    const suggestions = q
      ? allSuggestions.filter(
          (s) =>
            s.primaryDesignation.toLowerCase().includes(q) ||
            (s.commonName?.toLowerCase().includes(q) ?? false) ||
            (s.matchedAlias?.toLowerCase().includes(q) ?? false),
        )
      : allSuggestions;
    return {
      contractVersion: '1.0',
      requestId: crypto.randomUUID(),
      suggestions,
      // Mock mode never warms a real resolve cache — always settled.
      cacheWarming: false,
    } satisfies TargetSearchResponse_Serialize;
  },
  target_resolve: async (_args) => {
    const req = (_args as { req?: { query?: string } } | undefined)?.req;
    const query = req?.query ?? 'M 31';
    return {
      contractVersion: '1.0',
      requestId: crypto.randomUUID(),
      status: 'resolved',
      target: {
        targetId: `tgt-resolved-${Date.now()}`,
        primaryDesignation: query,
        commonName: null,
        objectType: 'other',
        source: 'resolved',
        raDeg: 0,
        decDeg: 0,
        simbadOid: null,
        aliases: [],
      },
      unresolvedReason: null,
      error: null,
    } satisfies TargetResolveSimbadResponse_Serialize;
  },

  projects_list: async () => {
    // spec 008 real shape: ProjectSummaryDto[]
    const { mockProjectSummaries } = await import('@/data/fixtures/projects');
    return mockProjectSummaries;
  },
  projects_get: async (_args) => {
    // spec 008 real shape: ProjectDetailDto. Arg-sensitive so the detail's
    // lifecycle matches the requested project (mirrors the real backend,
    // which returns each project's actual lifecycle) — this is what lets the
    // full lifecycle state machine (ready/prepared/completed/archived/blocked)
    // be exercised through the UI, not just proj-001's `processing`.
    const { mockProjectDetailFor } = await import('@/data/fixtures/projects');
    const id = (_args as { id?: string } | undefined)?.id ?? 'proj-001';
    return mockProjectDetailFor(id);
  },
  projects_create: async () => {
    // Return a minimal success result; tests override this via vi.mock.
    return {
      projectId: 'mock-project-id',
      lifecycle: 'setup_incomplete',
      planId: 'mock-plan-id',
      channels: [],
      auditId: 'mock-audit-id',
      createdAt: new Date().toISOString(),
      // mkdir-only scaffolding auto-applies (user decision 2026-07-04).
      scaffoldApplied: true,
    } satisfies ProjectCreateResult_Serialize;
  },
  projects_update: async (_args) => {
    const req = (_args as { req?: { projectId?: string } } | undefined)?.req;
    return {
      projectId: req?.projectId ?? 'mock-id',
      fieldsUpdated: [],
      auditId: 'mock-audit-id',
      updatedAt: new Date().toISOString(),
    } satisfies ProjectUpdateResult;
  },
  projects_source_add: async (_args) => {
    const req = (_args as { req?: { projectId?: string } } | undefined)?.req;
    return {
      projectId: req?.projectId ?? 'mock-id',
      sourceAdded: {
        inventoryId: 'mock-inv',
        name: '',
        frames: 0,
        filter: '',
        exposure: '',
        linkedAt: new Date().toISOString(),
      },
      channels: [],
      auditId: 'mock-audit-id',
      linkedAt: new Date().toISOString(),
    } satisfies ProjectSourceAddResult_Serialize;
  },
  projects_source_remove: async (_args) => {
    const req = (
      _args as
        | {
            req?: {
              projectId?: string;
              projectSourceId?: string;
              confirmLastSource?: boolean;
            };
          }
        | undefined
    )?.req;
    // FR-011 last-confirmed-source guard: the backend refuses removing the
    // final confirmed source unless the caller re-confirms
    // (`confirm_last_source`). The mock surfaces the exact error contract by
    // requiring the flag on every removal, so the inline confirm guard flow
    // in EditProjectPane is exercisable in mock mode. Real per-source-count
    // discrimination is a Layer-2 concern (needs stateful backend fixtures).
    if (!req?.confirmLastSource) {
      return mockContractError(
        'lifecycle.last_confirmed_source',
        'Cannot remove the last confirmed source without confirmation.',
      );
    }
    return {
      projectId: req?.projectId ?? 'mock-id',
      removedSourceId: req?.projectSourceId ?? 'mock-src',
      auditId: 'mock-audit-id',
    } satisfies ProjectSourceRemoveResult_Serialize;
  },
  note_get: async (_args) => {
    // spec 024 `project.note.get` — persisted free-text notes body.
    const projectId =
      (_args as { req?: { projectId?: string } } | undefined)?.req?.projectId ??
      'proj-001';
    return {
      projectId,
      content:
        'SHO palette — Ha dominant. Review OIII stretch before integration.',
    } satisfies ProjectNoteGetResult;
  },
  note_update: async (_args) => {
    // spec 024 `project.note.update` — echoes an updated timestamp on success.
    const projectId =
      (_args as { req?: { projectId?: string } } | undefined)?.req?.projectId ??
      'proj-001';
    return {
      projectId,
      updatedAt: new Date().toISOString(),
    } satisfies ProjectNoteUpdateResult;
  },
  manifest_list: async () => {
    // spec 024 `project.manifest.list` — snapshot history. hasBody:false so
    // the accordion renders rows without an expandable body fetch.
    return {
      manifests: [
        {
          id: 'man-001',
          reason: 'created',
          timestamp: '2026-05-01T10:00:00Z',
          path: 'notes/manifest-2026-05-01.md',
          hasBody: false,
        },
        {
          id: 'man-002',
          reason: 'lifecycle_transition',
          timestamp: '2026-05-20T22:15:00Z',
          path: 'notes/manifest-2026-05-20.md',
          hasBody: false,
        },
      ],
      nextCursor: null,
    } satisfies ManifestListResponse_Serialize;
  },
  manifest_reveal_in_os: async () => {
    return null;
  },
  projects_channels_reinfer: async (_args) => {
    const req = (_args as { req?: { projectId?: string } } | undefined)?.req;
    return {
      projectId: req?.projectId ?? 'mock-id',
      channels: [],
      auditId: 'mock-audit-id',
      updatedAt: new Date().toISOString(),
    } satisfies ProjectChannelsReinferResult_Serialize;
  },
  projects_channels_dismiss_drift: async (_args) => {
    const req = (_args as { req?: { projectId?: string } } | undefined)?.req;
    return {
      projectId: req?.projectId ?? 'mock-id',
      auditId: 'mock-audit-id',
      dismissedAt: new Date().toISOString(),
    } satisfies ProjectChannelsDismissDriftResult;
  },
  plans_list: async (_args) => {
    // Spec 026 T019 mock coverage: `ViewAuditHistory` is `plans.list`'s
    // first real mock-mode consumer, and it always requests these two
    // origins — synthesize the view-scoped removal/regeneration history
    // for the T014/T016 mock stale view rather than growing the shared
    // static fixture below (which several other Playwright specs already
    // depend on for its exact plan count/rows).
    const listArgs = _args as { originFilter?: string[] | null } | undefined;
    if (
      listArgs?.originFilter?.includes('prepared_view_removal') ||
      listArgs?.originFilter?.includes('prepared_view_regeneration')
    ) {
      const historyPlans: PlanSummary_Serialize[] = [
        {
          id: 'mock-sv-plan-removal-1',
          number: 901,
          title: 'Remove source view mock-sv-view-stale',
          origin: 'prepared_view_removal',
          originPath: 'mock-sv-view-stale',
          state: 'applied',
          planType: 'source_view_removal',
          destructiveDestination: 'archive',
          itemsTotal: 1,
          itemsApplied: 1,
          itemsFailed: 0,
          itemsSkipped: 0,
          itemsCancelled: 0,
          itemsPending: 0,
          totalBytesRequired: 0,
          createdAt: '2026-05-20T09:00:00Z',
        },
        {
          id: 'mock-sv-plan-regen-1',
          number: 902,
          title: 'Regenerate source view mock-sv-view-stale',
          origin: 'prepared_view_regeneration',
          originPath: 'mock-sv-view-stale',
          state: 'partially_applied',
          planType: 'source_view_regeneration',
          destructiveDestination: 'archive',
          itemsTotal: 2,
          itemsApplied: 1,
          itemsFailed: 1,
          itemsSkipped: 0,
          itemsCancelled: 0,
          itemsPending: 0,
          totalBytesRequired: 0,
          createdAt: '2026-05-20T10:00:00Z',
        },
      ];
      return { plans: historyPlans };
    }
    const { plans } = await import('@/data/fixtures/plans');
    return { plans };
  },
  plans_get: async () => {
    const { planDetail } = await import('@/data/fixtures/plans');
    return planDetail;
  },
  plans_free_space_estimate: async () => {
    // Issue #876: advisory destination free-space estimate. Mock mode has
    // no real filesystem to probe — report comfortably more free space
    // than the fixture plan requires so the mock UI shows the healthy
    // (non-warning) state by default.
    const { planDetail } = await import('@/data/fixtures/plans');
    return {
      requiredBytes: planDetail.totalBytesRequired,
      availableBytes: planDetail.totalBytesRequired + 10_000_000_000,
    };
  },
  audit_list: async (_args) => {
    const args = _args as
      | {
          filters?: AuditFilterDto | null;
          pagination?: AuditPaginationDto | null;
        }
      | undefined;
    const filtered = filterMockAuditEntries(args?.filters);
    const offset = args?.pagination?.offset ?? 0;
    const limit = args?.pagination?.limit ?? filtered.length;
    const page = filtered.slice(offset, offset + limit);
    return {
      entries: page,
      total: filtered.length,
    } satisfies AuditListResponse_Serialize;
  },
  audit_export: async (_args) => {
    const args = _args as
      | { filePath: string; filters?: AuditFilterDto | null }
      | undefined;
    const filtered = filterMockAuditEntries(args?.filters);
    return {
      filePath: args?.filePath ?? '/tmp/mock-audit-export.ndjson',
      count: filtered.length,
      bytes: 0,
    };
  },

  entity_names: async () => ({
    // Batch display-name lookup (GF-7 / DS-14). Returns empty map in stub.
    names: {} as Record<string, string>,
  }),

  // ── Archive commands (spec 017 WP-B) ──────────────────────────────────────
  archive_list: async () => {
    return {
      entries: [
        {
          id: 'arch-proj-001',
          name: 'NGC 7000 · HOO (v1)',
          entityType: 'project',
          archivedAt: '2026-05-12T21:40:00Z',
          reason: 'Superseded by reprocess',
          originalPath: 'Projects/NGC7000_HOO_v1',
          sizeBytes: 12_400_000_000,
          archivedViaPlanId: 'plan-archive-001',
        },
        {
          id: 'arch-proj-002',
          name: 'M31 · LRGB (2025)',
          entityType: 'project',
          archivedAt: '2026-03-02T19:05:00Z',
          reason: 'Completed and delivered',
          originalPath: 'Projects/M31_LRGB_2025',
          sizeBytes: 8_100_000_000,
          archivedViaPlanId: 'plan-archive-002',
        },
      ],
    } satisfies ArchiveListResponse;
  },
  archive_send_to_trash: async (_args) => {
    const args = _args as { planId?: string } | undefined;
    return {
      planId: args?.planId ?? 'plan-archive-001',
      itemsMoved: 3,
      auditId: 'audit-archive-trash-001',
    } satisfies ArchiveSendToTrashResponse;
  },
  archive_permanently_delete: async (_args) => {
    const args = _args as { planId?: string } | undefined;
    return {
      planId: args?.planId ?? 'plan-archive-001',
      itemsDeleted: 3,
      auditId: 'audit-archive-delete-001',
    } satisfies ArchivePermanentlyDeleteResponse;
  },
  log_recent: async () => {
    const { MOCK_LOG_ENTRIES } = await import('@/data/mockLogEntries');
    return {
      contractVersion: '1',
      entries: MOCK_LOG_ENTRIES,
      truncated: false,
    } satisfies LogRecentResponse_Serialize;
  },
  log_export: async (_args) => {
    const args = _args as { requestId?: string; filePath?: string } | undefined;
    return {
      contractVersion: '1',
      requestId: args?.requestId ?? 'mock-req',
      status: 'success',
      filePath: args?.filePath ?? '/tmp/log-export.json',
      count: 8,
      bytes: 1024,
    } satisfies LogExportResponse_Serialize;
  },
  settings_get: async (_args) => {
    // Scope-aware: the `observing` scope reflects the seedable per-session
    // values bag (planner site gate + usable-altitude threshold). Every other
    // scope keeps the legacy general fixture (unchanged behaviour).
    const scope = (_args as { scope?: string } | undefined)?.scope;
    if (scope === 'observing') {
      return {
        scope: 'observing',
        values: observingValues(),
      } satisfies SettingsData;
    }
    if (scope === 'framing') {
      return {
        scope: 'framing',
        values: mockFramingSettings,
      } satisfies SettingsData;
    }
    return mockSettingsData;
  },
  ingestion_settings_get: async () => {
    return mockIngestionSettings;
  },
  roots_list: async () => {
    return mockRoots;
  },
  equipment_list: async () => {
    return mockEquipment;
  },
  equipment_cameras_list: async () => {
    return mockCameras;
  },
  equipment_telescopes_list: async () => {
    return mockTelescopes;
  },
  equipment_trains_list: async () => {
    return mockOpticalTrains;
  },
  equipment_filters_list: async () => {
    return mockFilters;
  },
  review_queue: async () => {
    const { reviewItems } = await import('@/data/fixtures/review');
    return reviewItems;
  },
  preferences_get: async () => {
    return mockPreferences;
  },
  search_global: async () => {
    return mockSearchResults;
  },

  // ---------- Mutation Commands ----------

  lifecycle_transition_apply: async (_args) => {
    // Arg-sensitive: a plan-required edge (e.g. completed → archived) returns
    // `status: 'error'` with `error.code: 'plan.required'`, mirroring the
    // backend — the UI then routes to the plan-create flow (and, for
    // → archived, calls `archive_plan_generate`). Non-plan edges (e.g.
    // processing → completed) succeed immediately. The request is the
    // canonical FLAT discriminated envelope (issue #423): `nextState` sits
    // beside the `entityType` tag — no `{ project: {...} }` wrapper.
    const req = (
      _args as
        | {
            request?: {
              nextState?: string;
              currentState?: string;
              entityId?: string;
            };
          }
        | undefined
    )?.request;
    const prior = req?.currentState ?? 'processing';
    const next = req?.nextState ?? 'completed';
    if (MOCK_PLAN_REQUIRED_EDGES.has(`${prior}→${next}`)) {
      return {
        status: 'error',
        contractVersion: '2.0.0',
        requestId: crypto.randomUUID(),
        error: {
          code: 'plan.required',
          message: `Transition ${prior} → ${next} requires an approved plan.`,
          details: null,
        } satisfies TransitionError_Serialize,
      } satisfies TransitionResponse_Serialize;
    }
    return {
      status: 'success',
      contractVersion: '2.0.0',
      requestId: crypto.randomUUID(),
      appliedAt: new Date().toISOString(),
      priorState: prior,
      newState: next,
      auditId: 'mock-audit-transition',
    } satisfies TransitionResponse_Serialize;
  },

  archive_plan_generate: async () => {
    // `archive.plan.generate` — whole-project reviewable archive plan (spec
    // 017 / constitution II: created in `ready_for_review`, never
    // auto-applied). Previously ABSENT from the mock switch, so the
    // completed → archived archive flow dead-ended after the plan.required
    // toast. One protected item so the spec-016 acknowledge gate is exercised.
    return {
      planId: 'plan-archive-mock',
      itemCount: 4,
      protectedItemCount: 1,
    } satisfies GenerateArchivePlanResult;
  },

  sessions_split: async () => {
    const { sessions } = await import('@/data/fixtures/sessions');
    return { original: sessions[0], new: sessions[1] };
  },
  sessions_merge: async () => {
    const { sessions } = await import('@/data/fixtures/sessions');
    return sessions[0];
  },
  projects_create_plan: async () => {
    // Answers with a full PlanDetail (summary + items), not a PlanSummary.
    const { planDetail } = await import('@/data/fixtures/plans');
    return planDetail;
  },
  plans_approve: async (_args) => {
    // Real contract shape (PlanApproveResponse): the overlay consumes
    // `approvalToken` and hands it to `plans_apply_real`.
    const planId = (_args as { id?: string } | undefined)?.id ?? 'mock-plan';
    return {
      planId,
      newState: 'approved',
      approvalToken: `tok-${planId}-mock`,
      approvedAt: new Date().toISOString(),
    };
  },
  plan_protection_check_cmd: async (_args) => {
    // One protected item so the spec-016 gate is exercised in mock mode.
    const planId =
      (_args as { planId?: string } | undefined)?.planId ?? 'mock-plan';
    return {
      planId,
      hasProtectedItems: true,
      protectedItems: [
        {
          itemId: `${planId}-item-0`,
          sourceId: 'mock-project-1',
          level: 'protected',
          reason:
            'Default protection level is protected; no per-source override.',
          matchedCategories: ['masters'],
          originalAction: 'archive',
          rewrittenAction: null,
          requiresAcknowledgement: true,
        },
      ],
      nonBlockingSummary: { normalCount: 2, unprotectedCount: 0 },
    };
  },
  protection_plan_acknowledged: async () => {
    return 'mock-audit-ack';
  },
  plans_confirm_destructive: async (_args) => {
    // FR-003/D9/issue #741: persists `destructive_confirmed` so the
    // review overlay's destructive-confirm gate unlocks Approve & apply.
    const planId =
      (_args as { planId?: string } | undefined)?.planId ?? 'mock-plan';
    return { planId, itemsConfirmed: 1 };
  },
  recovery_status: async () => {
    // Mock mode never has a crashed prior process.
    return { uncleanShutdown: false, interruptedPlanIds: [] };
  },
  cleanup_policy_get: async () => {
    return mockCleanupPolicy;
  },
  cleanup_policy_update: async (_args) => {
    const req = (_args as { request?: UpdateCleanupPolicy } | undefined)
      ?.request;
    if (req) mockCleanupPolicy = { ...req };
    return mockCleanupPolicy;
  },
  cleanup_scan: async (_args) => {
    // D11 step 1: pure read-only preview. Reason strings follow the backend
    // generator format (see cleanup_generator.rs::scan_with_policy).
    const projectId =
      (_args as { projectId?: string } | undefined)?.projectId ??
      'mock-project-1';
    return {
      projectId,
      candidates: [
        {
          filePath: 'processing/calibrated/Ha_300s_c_0001.xisf',
          dataType: 'intermediate',
          sizeBytes: 268_435_456,
          reason:
            'intermediate artifact (classified by rule, 90% confidence); protection: normal; policy: archive',
        },
        {
          filePath: 'processing/calibrated/Ha_300s_c_0002.xisf',
          dataType: 'intermediate',
          sizeBytes: 268_435_456,
          reason:
            'intermediate artifact (classified by rule, 90% confidence); protection: normal; policy: archive',
        },
        {
          filePath: 'masters/master_dark_300s.xisf',
          dataType: 'master',
          sizeBytes: 536_870_912,
          reason:
            'master artifact (classified by rule, 95% confidence); protection: protected; policy: archive',
        },
      ],
      totalReclaimableBytes: 1_073_741_824,
    };
  },
  cleanup_plan_generate: async () => {
    return {
      planId: 'plan-cleanup-mock',
      itemCount: 3,
      protectedItemCount: 1,
    };
  },
  plans_apply_real: async (_args) => {
    // Spec 042 US16 (T240): drive the live long-op channel if a subscriber
    // passed one. `onEvent` is a real `Channel<OperationEvent>` in mock mode;
    // pushing through `onmessage` mirrors the backend's Started → per-item →
    // Completed lifecycle so UI/tests can exercise streaming without a backend.
    const channel = (
      _args as { onEvent?: { onmessage?: (e: OperationEvent) => void } }
    )?.onEvent;
    if (channel?.onmessage) {
      const opId = 'op-mock-001';
      const push = channel.onmessage;
      const mk = (
        sequence: number,
        eventType: OperationEvent['eventType'],
        payload: unknown,
      ): OperationEvent => ({
        contractVersion: '1.0.0',
        operationId: opId,
        eventType,
        sequence,
        payload,
      });
      // Emit asynchronously so callers can attach listeners first.
      void Promise.resolve().then(() => {
        push(
          mk(0, 'item_started', {
            runId: opId,
            itemsTotal: 1,
            at: '1970-01-01T00:00:00Z',
          }),
        );
        push(
          mk(1, 'item_applied', {
            runId: opId,
            itemId: 'item-0',
            newState: 'succeeded',
          }),
        );
        push(
          mk(2, 'completed', {
            runId: opId,
            terminalState: 'completed',
            itemsApplied: 1,
            itemsFailed: 0,
          }),
        );
      });
    }
    // Resolve with the real `PlanApplyResponse` contract ({ planId, runId,
    // newState }) — the live per-item progress is the channel stream above
    // (spec 042 T270).
    const applyPlanId =
      (_args as { planId?: string } | undefined)?.planId ?? 'mock-plan';
    return {
      planId: applyPlanId,
      runId: 'op-mock-001',
      newState: 'applied',
    } satisfies PlanApplyResponse;
  },
  plans_discard: async (_args) => {
    return {
      planId: (_args?.id as string) ?? 'plan-001',
      discardedAt: new Date().toISOString(),
    };
  },
  settings_update: async (_args) => {
    // The `observing` scope round-trips into the seedable values bag so a
    // UI-driven site creation / active-site switch persists across the
    // session (site store + altitude threshold both read this scope back).
    const scope = (_args as { scope?: string } | undefined)?.scope;
    if (scope === 'observing') {
      const values = (_args as { values?: Record<string, unknown> } | undefined)
        ?.values;
      if (values) mockObservingValues = { ...observingValues(), ...values };
    }
    if (scope === 'framing') {
      const values = (_args as { values?: Record<string, unknown> } | undefined)
        ?.values;
      if (values) mockFramingSettings = { ...mockFramingSettings, ...values };
    }
    return null;
  },
  ingestion_settings_update: async (_args) => {
    const req = (_args as { request?: UpdateIngestionSettings } | undefined)
      ?.request;
    if (req) {
      mockIngestionSettings = { ...req };
    }
    return mockIngestionSettings;
  },
  calibration_tolerances_update: async (_args) => {
    // Persist then return, mirroring the real `calibration.tolerances.update`
    // command's upsert-then-return behaviour (persistence_db::repositories::
    // calibration_tolerances::update). Storing back into the singleton makes a
    // later `calibration_tolerances_get` observe the edit (round-trip seam).
    const req = (_args as { request?: CalibrationTolerances } | undefined)
      ?.request;
    mockCalibrationTolerances = { ...mockCalibrationTolerances, ...req };
    return mockCalibrationTolerances satisfies CalibrationTolerances;
  },
  roots_register: async (_args) => {
    // `rootsRegister(path, category, scanSettings)` answers with the source
    // envelope, not the LibraryRoot row the Settings list renders.
    return {
      sourceId: `src-${crypto.randomUUID()}`,
      kind: 'light_frames',
      path: (_args?.path as string) ?? '/astro/raw',
      createdAt: new Date().toISOString(),
      organizationState: 'unorganized',
    };
  },
  roots_remap: async (_args) => {
    // Generated `RemapVerification` is camelCase; mirror the real contract so
    // the verification UI (which reads `samples`/`allVerified`) works in mock
    // mode exactly as it does against the backend. The generated `rootsRemap`
    // binding invokes with `{ rootId, newPath }` (camelCase) — NOT
    // `root_id`/`new_path` — so read those keys here.
    const rootId = (_args?.rootId as string) ?? 'root-001';
    const newPath = (_args?.newPath as string) ?? '/new/path';
    const originalPath =
      mockRoots.find((r) => r.id === rootId)?.path ?? '/old/path';
    return {
      rootId,
      originalPath,
      newPath,
      samples: [
        { relativePath: 'M31/light_001.fits', found: true },
        { relativePath: 'M31/light_002.fits', found: true },
      ],
      allVerified: true,
    } satisfies RemapVerification;
  },
  roots_remap_apply: async () => {
    return null;
  },
  sources_set_active: async (_args) => {
    // Generated `sourcesSetActive` binding invokes with `{ rootId, active }`
    // (camelCase) — mirror the real backend's `registered_sources.active`
    // toggle so mock mode's Disable/Enable buttons behave persistently.
    const rootId = (_args?.rootId as string) ?? '';
    const active = (_args?.active as boolean) ?? true;
    mockRoots = mockRoots.map((r) => (r.id === rootId ? { ...r, active } : r));
    return null;
  },
  roots_delete: async (_args) => {
    // Mirrors the real backend's decision D8 block: `root-001` carries mock
    // "dependents" (it has file_count/lastScanned in the demo fixture) so
    // mock mode can also exercise the has_dependents error path — every
    // other seed root deletes cleanly.
    const rootId = (_args?.rootId as string) ?? '';
    if (rootId === 'root-001') {
      return mockContractError(
        'root.has_dependents',
        `root ${rootId} has dependent records and cannot be deleted`,
      );
    }
    mockRoots = mockRoots.filter((r) => r.id !== rootId);
    return null;
  },
  scan_start: async () => {
    return {
      operationId: 'op-scan-001',
      kind: 'scan',
    } satisfies IpcOperationHandle;
  },
  preferences_set: async () => {
    return null;
  },
  // ---------- Onboarding (spec 056) ----------
  // Manual actions round-trip through the in-memory cache; the auto-tick
  // event path is a documented no-op in mock mode (VC-002 limit).
  onboarding_state_get: async () => {
    return { state: mockOnboardingStateDto() };
  },
  onboarding_item_set_state: async (_args) => {
    hydrateOnboarding();
    // The generated binding invokes `{ request: { itemId, state } }`; the
    // handler must read the request-wrapped args (mirrors
    // `ingestion_settings_update` / `calibration_tolerances_update`).
    const req = (
      _args as
        | {
            request?: { itemId?: string; state?: OnboardingItemDto['state'] };
          }
        | undefined
    )?.request;
    const itemId = req?.itemId as string;
    const nextState = req?.state as OnboardingItemDto['state'];
    const item = mockOnboardingItems.find((i) => i.itemId === itemId);
    if (!item) {
      return mockContractError(
        'onboarding.item.unknown',
        `unknown onboarding item id: ${itemId}`,
      );
    }
    // An explicit un-check is the one transition allowed to clear a settled
    // row (it mirrors `force_unchecked` on the backend, which bypasses the
    // terminality rule). It never settles anything, so it cannot trigger the
    // FR-031 auto-hide below — it moves an item away from settled.
    if (nextState === 'unchecked') {
      item.state = 'unchecked';
      item.source = 'user';
      item.at = new Date().toISOString();
      persistOnboarding();
    } else if (item.state === 'unchecked') {
      // Otherwise settled states stay terminal (FR-017).
      item.state = nextState;
      item.source = 'user';
      item.at = new Date().toISOString();
      // FR-031 completion auto-hide: once the last open item settles, the
      // backend flips `sectionHidden`; mirror that so the whole-section
      // auto-hide is exercisable in mock mode.
      if (mockOnboardingItems.every((i) => i.state !== 'unchecked')) {
        mockOnboardingFlags = { ...mockOnboardingFlags, sectionHidden: true };
      }
      persistOnboarding();
    }
    return { item: { ...item } };
  },
  onboarding_orientation_complete: async () => {
    hydrateOnboarding();
    // No request field is read here (the `outcome` in `{ request: { outcome
    // } }` does not change the mock's done-forever effect), but the flip must
    // persist so "no auto-run after restart" (FR-004) holds across a reload.
    if (!mockOnboardingFlags.orientationDone) {
      mockOnboardingFlags = {
        ...mockOnboardingFlags,
        orientationDone: true,
      };
      persistOnboarding();
    }
    return { orientationDoneAt: new Date().toISOString() };
  },
  onboarding_section_set: async (_args) => {
    hydrateOnboarding();
    // Request-wrapped args (see `onboarding_item_set_state` above).
    const sectionReq = (
      _args as
        | {
            request?: {
              hidden?: boolean | null;
              sidebarCollapsed?: boolean | null;
            };
          }
        | undefined
    )?.request;
    const hidden = sectionReq?.hidden;
    const sidebarCollapsed = sectionReq?.sidebarCollapsed;
    if (hidden == null && sidebarCollapsed == null) {
      return mockContractError(
        'onboarding.invalid_state',
        'onboarding.section.set request must set hidden or sidebarCollapsed',
      );
    }
    if (hidden === false) {
      return mockContractError(
        'onboarding.invalid_state',
        'hidden may only be true; unhide via onboarding.restore',
      );
    }
    mockOnboardingFlags = {
      ...mockOnboardingFlags,
      sectionHidden: hidden === true ? true : mockOnboardingFlags.sectionHidden,
      sidebarCollapsed:
        sidebarCollapsed == null
          ? mockOnboardingFlags.sidebarCollapsed
          : sidebarCollapsed,
    };
    persistOnboarding();
    return { flags: { ...mockOnboardingFlags } };
  },
  onboarding_restore: async () => {
    hydrateOnboarding();
    // Re-derive AUTOMATIC items only (mock: reset to unchecked); manual
    // states survive. Clears the hidden flag (FR-014).
    mockOnboardingItems = mockOnboardingItems.map((i) =>
      i.hasAutoTick && (i.state === 'unchecked' || i.state === 'auto_checked')
        ? { ...i, state: 'unchecked', source: 'seed' }
        : i,
    );
    mockOnboardingFlags = { ...mockOnboardingFlags, sectionHidden: false };
    persistOnboarding();
    return { state: mockOnboardingStateDto() };
  },

  // ---------- First-Run / Batch Commands ----------

  roots_register_batch: async (_args) => {
    // Generated `RegisterSourceBatchResponse` shape is `{ status, items: [{
    // index, status, sourceId, error }] }`.  An earlier mock invented a
    // `{ results: [{ root }] }` shape that `registerRootBatch` could not read,
    // so mock mode produced zero registered roots.  Mirror the real contract.
    const req =
      (_args?.request as {
        sources?: Array<{ kind: string; path: string }>;
      }) ?? _args;
    const sources =
      (req?.sources as Array<{ kind: string; path: string }>) ?? [];
    return {
      status: 'success',
      items: sources.map((_s, i) => ({
        index: i,
        status: 'success',
        sourceId: `src-${i}`,
        error: null,
      })),
    } satisfies RegisterSourceBatchResponse_Serialize;
  },
  tools_validate_path: async (_args) => {
    // Setup wizard manual path entry (#662) add-time existence check —
    // mock mode has no real filesystem, so treat every non-empty path as
    // valid (mirrors the native-picker flow, which only ever adds paths
    // that already exist).
    const path = (_args?.path as string) ?? '';
    const valid = path.trim().length > 0;
    return {
      path,
      valid,
      reason: valid ? null : 'Path does not exist',
      // Mock mode has no real filesystem; treat every valid mock path as a
      // directory (mirrors the native-picker flow, which only ever adds
      // existing directories).
      isDir: valid ? true : null,
    } satisfies ToolPathValidation;
  },
  firstrun_complete: async () => {
    return {
      completedAt: new Date().toISOString(),
      registeredSourceCount: 0,
    } satisfies FirstRunCompleteResponse;
  },
  firstrun_restart: async () => {
    // Generated `FirstRunRestartResponse` is `{ restartedAt, prefilledSources:
    // RegisterSourceResponse[] }` — mirror it instead of the legacy
    // `{ success, prefilled_sources }` shape.
    return {
      restartedAt: new Date().toISOString(),
      prefilledSources: mockRoots.map((r, i) => ({
        sourceId: `src-${i}`,
        kind: 'light_frames',
        path: r.path,
        createdAt: new Date().toISOString(),
        organizationState: 'unorganized',
      })),
    } satisfies FirstRunRestartResponse;
  },
  firstrun_state: async () => {
    // Generated `FirstRunStateResponse` is `{ completedAt?, lastStep }`.
    return {
      completedAt: null,
      lastStep: 'welcome',
    } satisfies FirstRunStateResponse_Serialize;
  },

  // ── Inbox commands (spec 005 + 039) ───────────────────────────────────────
  inbox_list: async () => {
    // Mock: two roots each with unacknowledged items (SC-001 cross-root).
    // Spec 040 P2a: includes individual master items + real format field.
    return {
      items: [
        {
          inboxItemId: 'item-001',
          needsReview: false,
          groupId: 'item-001',
          groupKey: '',
          rootId: 'root-lights-001',
          rootAbsolutePath: '/astro/raw',
          relativePath: '2025-10-10/NGC7000',
          fileCount: 18,
          lane: 'fits',
          format: 'fits',
          state: 'classified',
          contentSignature: 'sig-abc',
          organizationState: 'unorganized',
          isMaster: false,
        },
        {
          inboxItemId: 'item-002',
          needsReview: false,
          groupId: 'item-002',
          groupKey: '',
          rootId: 'root-lights-001',
          rootAbsolutePath: '/astro/raw',
          relativePath: '2025-10-10/darks',
          fileCount: 46,
          lane: 'fits',
          format: 'fits',
          state: 'pending_classification',
          contentSignature: 'sig-def',
          organizationState: 'unorganized',
          isMaster: false,
        },
        {
          // Individual master item — spec 040 FR-005
          inboxItemId: 'item-master-dark',
          needsReview: false,
          groupId: 'item-master-dark',
          groupKey: '',
          rootId: 'root-lights-001',
          rootAbsolutePath: '/astro/raw',
          relativePath: '2025-10-10/darks/masterDark_Ha_300s.xisf',
          fileCount: 1,
          lane: 'fits',
          format: 'xisf',
          state: 'pending_classification',
          contentSignature: '',
          organizationState: 'unorganized',
          isMaster: true,
          masterFrameType: 'dark',
          masterFilter: 'Ha',
          masterExposureS: 300,
        },
        {
          inboxItemId: 'item-003',
          needsReview: false,
          groupId: 'item-003',
          groupKey: '',
          rootId: 'root-inbox-001',
          rootAbsolutePath: '/astro/inbox',
          relativePath: '2025-11-01/Jupiter',
          fileCount: 3,
          lane: 'video',
          format: 'video',
          state: 'pending_classification',
          contentSignature: 'sig-ghi',
          organizationState: 'unorganized',
          isMaster: false,
        },
      ],
      // Spec 058 FR-016: mock roots are all already classified, so no
      // folder is awaiting its item rows.
      sourceGroups: [],
      capped: false,
      limit: 500,
    } satisfies InboxListResponse_Serialize;
  },
  inbox_scan_folder: async () => {
    return {
      rootId: 'root-inbox-001',
      items: [
        {
          inboxItemId: 'item-001',
          relativePath: '2025-10-10/NGC7000',
          fileCount: 18,
          lane: 'fits',
          format: 'fits',
          state: 'classified',
          contentSignature: 'sig-abc',
          isMaster: false,
        },
        {
          inboxItemId: 'item-002',
          relativePath: '2025-10-10/darks',
          fileCount: 46,
          lane: 'fits',
          format: 'fits',
          state: 'pending_classification',
          contentSignature: 'sig-def',
          isMaster: false,
        },
        {
          // Individual master item detected during scan — spec 040 FR-005
          inboxItemId: 'item-master-dark',
          relativePath: '2025-10-10/darks/masterDark_Ha_300s.xisf',
          fileCount: 1,
          lane: 'fits',
          format: 'xisf',
          state: 'pending_classification',
          contentSignature: '',
          isMaster: true,
          masterFrameType: 'dark',
          masterFilter: 'Ha',
          masterExposureS: 300,
        },
      ],
      scanWarnings: [],
    } satisfies InboxScanFolderResponse_Serialize;
  },
  /**
   * Spec 058 FR-017. Group-scoped classification, the action a source-group
   * row offers.
   *
   * The real operation materialises item rows and returns only a count, so the
   * mock returns a plausible count and nothing else — there is no
   * classification payload to invent. Mock mode's `inbox_list` returns
   * `sourceGroups: []` (every mock root is already classified), so no row
   * actually invokes this today; it exists so the command is exercisable and
   * so `tsc` checks this payload against the generated binding.
   */
  inbox_classify_source_group: async (_args) => {
    const args = _args as { req: { sourceGroupId: string } } | undefined;
    return {
      sourceGroupId: args?.req?.sourceGroupId ?? 'sg-001',
      materializedSubItemCount: 2,
    } satisfies InboxClassifySourceGroupResponse;
  },
  inbox_classify: async (_args) => {
    const args = _args as { req: { inboxItemId: string } } | undefined;
    const id = args?.req?.inboxItemId ?? 'item-001';
    // spec 058 T035 retired `mixed`: a folder spanning several frame types
    // now reports `unclassified` (both producers do), and the breakdown
    // carries the composition FR-011 asks for. Keeping `mixed` here let the
    // mock-mode Playwright specs pass against a shape the backend no longer
    // emits.
    const isMixed = id === 'item-001';
    return {
      inboxItemId: id,
      type: isMixed ? 'unclassified' : 'single_type',
      frameType: isMixed ? undefined : 'dark',
      contentSignature: `sig-${id}`,
      breakdown: isMixed
        ? [
            {
              kind: 'light',
              count: 16,
              destinationPreview: 'NGC7000/Ha/2025-10-10/light/',
              sampleFiles: ['NGC7000_Ha_001.fits', 'NGC7000_Ha_002.fits'],
            },
            {
              kind: 'dark',
              count: 2,
              destinationPreview: 'unclassified/2025-10-10/dark/',
              sampleFiles: ['dark_001.fits'],
            },
          ]
        : [
            {
              kind: 'dark',
              count: 50,
              destinationPreview: 'darks/2025-10-10/dark/',
              sampleFiles: ['dark_001.fits'],
            },
          ],
      unclassifiedFiles: isMixed ? ['NGC7000_Ha_mixed.fits'] : [],
      sampleFiles: ['NGC7000_Ha_001.fits'],
      computedAt: new Date().toISOString(),
    } satisfies InboxClassifyResponse_Serialize;
  },
  inbox_confirm: async (_args) => {
    // Arg-sensitive: an ORGANIZED source catalogues in place (zero moves —
    // spec 041 US4/FR-018), an unorganized source produces a move plan
    // (FR-017). The observable move-vs-catalogue distinction surfaces on the
    // aggregate plan surface (`inbox_plan_list_open`); the confirm response
    // itself carries the branch via `actionsSummary`/`organizationState`.
    const req = (
      _args as
        | { req?: { inboxItemId?: string; rootAbsolutePath?: string } }
        | undefined
    )?.req;
    const inboxItemId = req?.inboxItemId ?? '';
    const rootAbsolutePath = req?.rootAbsolutePath ?? '/astro/raw';
    const organized = MOCK_ORGANIZED_ITEM_IDS.has(inboxItemId);
    // `itemsTotal` kept at 18 for the existing move-path e2e assertion.
    const itemsTotal = 18;
    const actionsSummary: InboxConfirmActionsSummary = organized
      ? { moveCount: 0, catalogueCount: itemsTotal }
      : { moveCount: itemsTotal, catalogueCount: 0 };
    const fromPath = `${rootAbsolutePath}/light_001.fits`;
    const destinations: InboxConfirmDestination[] = [
      organized
        ? {
            fromPath,
            toRelativePath: 'light_001.fits',
            toAbsolutePath: fromPath, // catalogue = no move
            toRootId: 'root-lights-001',
            action: 'catalogue',
          }
        : {
            fromPath,
            toRelativePath: 'darks/2025-10-10/light_001.fits',
            toAbsolutePath: '/astro/library/darks/2025-10-10/light_001.fits',
            toRootId: 'root-lights-001',
            action: 'move',
          },
    ];
    return {
      planId: `plan-${Date.now()}`,
      planState: 'ready_for_review',
      itemsTotal,
      registeredAsMaster: false,
      actionsSummary,
      organizationState: organized ? 'organized' : 'unorganized',
      destinations,
      // spec 008 US7/FR-022 (#943): the candidate list also ships on the
      // confirm response. Kept in sync with the `inbox_attribution_suggest`
      // fixture below — a mock that omitted it hid the missing UI caller.
      attributionCandidates: mockLightInboxItemIds.has(inboxItemId)
        ? MOCK_ATTRIBUTION_CANDIDATES
        : [],
      attributionApplied: null,
    } satisfies InboxConfirmResponse_Serialize;
  },
  inbox_attribution_suggest: async (_args) => {
    // spec 008 US7/FR-019 (#943): ranked, suggest-only, light-frames-only —
    // mirroring `attribution::suggest_candidates`, which returns [] for any
    // item that fails `confirm::evidence_is_light`.
    const args = _args as { req?: { inboxItemId?: string } } | undefined;
    return mockLightInboxItemIds.has(args?.req?.inboxItemId ?? '')
      ? MOCK_ATTRIBUTION_CANDIDATES
      : [];
  },
  inbox_reclassify: async (_args) => {
    const args = _args as { req: { inboxItemId: string } } | undefined;
    const reclassifiedId = args?.req?.inboxItemId ?? 'item-001';
    // Reports `frameType: 'light'` below, so the item now passes the
    // attribution light gate.
    mockLightInboxItemIds.add(reclassifiedId);
    return {
      inboxItemId: reclassifiedId,
      updatedType: 'single_type',
      frameType: 'light',
      remainingUnclassified: 0,
      appliedCount: 1,
      breakdown: [],
    } satisfies InboxReclassifyResponse_Serialize;
  },

  inbox_reclassify_v2: async (_args) => {
    // Field-agnostic + bulk reclassify (spec 041 R-13/T068, issue #755).
    // No sub-item re-split fixture is modeled here — InboxDetail only reads
    // `needsReviewCount` and relies on cache invalidation to re-fetch the
    // list/classify queries (which their own mock cases already serve).
    const args = _args as { req?: { inboxItemId?: string } } | undefined;
    return {
      sourceGroupId: args?.req?.inboxItemId ?? 'item-001',
      subItems: [],
      needsReviewCount: 0,
    } satisfies InboxReclassifyV2Response_Serialize;
  },

  // ── Inbox plan surface (spec 041 US2) ─────────────────────────────────────
  //
  // These cases were previously ABSENT: every one fell through to `default:
  // throw new Error('Unknown mock command')`, so the "Review plans" overlay,
  // per-file metadata popover, and aggregate stats were unreachable in mock
  // mode. Shapes are pinned to the generated bindings.

  inbox_plan_list_open: async () => {
    const totalActions = mockInboxOpenPlans.reduce(
      (sum, p) => sum + p.actions.length,
      0,
    );
    return {
      plans: mockInboxOpenPlans,
      totalActions,
    } satisfies InboxOpenPlansResponse;
  },
  inbox_plan: async (_args) => {
    const inboxItemId = (_args?.inboxItemId as string) ?? '';
    const plan = mockInboxOpenPlans.find((p) => p.inboxItemId === inboxItemId);
    if (!plan) {
      // Mirrors the backend's `inbox.item.no_plan` error branch, which the
      // hook swallows into an empty state (store.ts `useInboxPlan`).
      throw 'inbox.item.no_plan';
    }
    return {
      planId: plan.planId,
      state: plan.state,
      stale: plan.stale,
      actions: plan.actions,
    } satisfies InboxPlanView;
  },
  inbox_plan_apply: async (_args) => {
    // The InboxPage apply-one path streams live progress through
    // `plans_apply_real` (Channel); this direct binding is retained for
    // completeness. Removes the applied plan from the aggregate surface.
    const inboxItemId = (_args?.inboxItemId as string) ?? '';
    const plan = mockInboxOpenPlans.find((p) => p.inboxItemId === inboxItemId);
    mockInboxOpenPlans = mockInboxOpenPlans.filter(
      (p) => p.inboxItemId !== inboxItemId,
    );
    return {
      planId: plan?.planId ?? 'plan-unknown',
      runId: 'op-inbox-apply-001',
      newState: 'applied',
    } satisfies PlanApplyResponse;
  },
  inbox_plan_apply_all: async () => {
    const results = mockInboxOpenPlans.map((p) => ({
      inboxItemId: p.inboxItemId,
      planId: p.planId,
      state: 'applied',
      error: null,
    }));
    mockInboxOpenPlans = [];
    return { results } satisfies InboxApplyAllResponse;
  },
  inbox_plan_apply_selected: async (_args) => {
    const ids =
      (_args as { request?: { inboxItemIds?: string[] } } | undefined)?.request
        ?.inboxItemIds ?? [];
    const selected = mockInboxOpenPlans.filter((p) =>
      ids.includes(p.inboxItemId),
    );
    const results = selected.map((p) => ({
      inboxItemId: p.inboxItemId,
      planId: p.planId,
      state: 'applied',
      error: null,
    }));
    mockInboxOpenPlans = mockInboxOpenPlans.filter(
      (p) => !ids.includes(p.inboxItemId),
    );
    return { results } satisfies InboxApplyAllResponse;
  },
  inbox_plan_cancel: async (_args) => {
    const inboxItemId = (_args?.inboxItemId as string) ?? '';
    const plan = mockInboxOpenPlans.find((p) => p.inboxItemId === inboxItemId);
    mockInboxOpenPlans = mockInboxOpenPlans.filter(
      (p) => p.inboxItemId !== inboxItemId,
    );
    return {
      inboxItemId,
      planId: plan?.planId ?? 'plan-unknown',
      state: 'classified',
    } satisfies InboxPlanCancelResponse;
  },
  inbox_stats: async () => {
    // Faithful aggregate: 3 folders (item-001/002/003) + 1 master
    // (item-master-dark), mirroring the `inbox_list` fixture. No component
    // currently invokes this (the UI derives stats client-side via
    // `deriveInboxStats`), but future batches can exercise it directly.
    return {
      perType: [
        { frameType: 'dark', folderCount: 0, masterCount: 1, imageCount: 1 },
        {
          frameType: 'mixed',
          folderCount: 3,
          masterCount: 0,
          imageCount: 67,
        },
      ],
      totals: { folders: 3, masters: 1, images: 68 },
    } satisfies InboxStatsResponse;
  },
  inbox_item_metadata: async (_args) => {
    // Per-file extracted metadata for the selected inbox item (spec 041
    // US2/FR-010). Deliberately carries NO `missingPathAttributes` /
    // `missingMandatory` so confirm stays enabled in the move-path e2e.
    const inboxItemId =
      (_args as { req?: { inboxItemId?: string } } | undefined)?.req
        ?.inboxItemId ?? 'item-001';
    const files: InboxFileMetadata_Serialize[] = [
      {
        relativeFilePath: 'NGC7000_Ha_001.fits',
        frameTypeEffective: 'light',
        imageTyp: 'LIGHT',
        filter: 'Ha',
        exposureS: 300,
        gain: '100',
        binningX: 1,
        binningY: 1,
        temperatureC: -10,
        object: 'NGC 7000',
        dateObs: '2025-10-10T22:14:00',
        instrume: 'ASI2600MM Pro',
        telescop: 'Esprit 100ED',
        naxis1: 6248,
        naxis2: 4176,
        stackCount: null,
        isMaster: false,
        overrideStale: false,
        missingPathAttributes: [],
        missingMandatory: [],
        offset: 10,
        setTempC: -10,
        ccdTempC: -10,
      },
    ];
    return {
      inboxItemId,
      files,
    } satisfies InboxItemMetadataResponse_Serialize;
  },
  inbox_property_registry: async () => {
    // Typed property registry (spec 041 R-13/FR-044). No component invokes it
    // yet; a faithful subset keeps the field-agnostic reclassify editor
    // mockable for future batches.
    return [
      {
        key: 'frameType',
        kind: 'enum',
        unit: null,
        sourceHeaders: ['IMAGETYP'],
        overridable: true,
        appliesTo: ['light', 'dark', 'flat', 'bias'],
        validation: 'one of: light, dark, flat, bias',
      },
      {
        key: 'exposureS',
        kind: 'number',
        unit: 's',
        sourceHeaders: ['EXPTIME', 'EXPOSURE'],
        overridable: true,
        appliesTo: ['light', 'dark', 'flat'],
        validation: '> 0',
      },
      {
        key: 'filter',
        kind: 'string',
        unit: null,
        sourceHeaders: ['FILTER'],
        overridable: true,
        appliesTo: ['light', 'flat'],
        validation: null,
      },
    ] satisfies PropertyRegistryEntry_Serialize[];
  },

  // ── Inventory commands (spec 006) ─────────────────────────────────────────
  //
  // Spec 041 FR-051 (T076, Phase 13): sessions are derived, already-confirmed
  // inventory. `inventory_session_review` (the mock for the removed
  // `inventory.session.review` command) and the `reviewFilter`/`ignored`
  // session filtering were removed along with the review-state machine.

  inventory_list: async (_args) => {
    const { INVENTORY_LIST_RESPONSE } = await import(
      '@/data/fixtures/inventory'
    );
    // Empty-library toggle: lets a spec exercise the "no sessions exist yet"
    // branches (e.g. the onboarding find spotlight's note-field deep link,
    // which has no session to link to) without a second fixture.
    if (isE2EFlagSet(E2E_EMPTY_INVENTORY_STORE_ID)) {
      return { ...INVENTORY_LIST_RESPONSE, sources: [] };
    }
    const req = (
      _args as
        | {
            req?: {
              filters?: {
                sourceFilter?: string;
                frameFilter?: 'light' | 'dark' | 'flat' | 'bias';
              };
            };
          }
        | undefined
    )?.req;
    const filters = req?.filters;
    // #652: mirror the real backend's `filters_to_db` scoping (source_id +
    // frame_type) so mock-mode e2e coverage of the Sessions Type filter
    // (defaults to Light) matches production behavior instead of always
    // returning the unfiltered fixture.
    const sources = filters
      ? INVENTORY_LIST_RESPONSE.sources
          .filter(
            (src) => !filters.sourceFilter || src.id === filters.sourceFilter,
          )
          .map((src) => ({
            ...src,
            sessions: filters.frameFilter
              ? src.sessions.filter((s) => s.type === filters.frameFilter)
              : src.sessions,
          }))
          .filter((src) => src.sessions.length > 0)
      : INVENTORY_LIST_RESPONSE.sources;
    return { ...INVENTORY_LIST_RESPONSE, sources };
  },

  // ── Developer diagnostics (spec 021) ─────────────────────────────────────
  //
  // The `dev_*` commands are compile-time gated behind the Rust `dev-tools`
  // feature, so they are absent from the generated `@/bindings` surface in
  // release builds.  These fixtures therefore have no generated type to pin
  // against; the dev UI defines its own local response interfaces.

  dev_contracts_list: async () => {
    return {
      contracts: [
        {
          name: 'sessions_list',
          version: '1.0.0',
          schemaPath: '',
          direction: 'ui-to-core',
          replaySafe: true,
          sensitiveFields: [],
        },
        {
          name: 'settings_update',
          version: '1.0.0',
          schemaPath: '',
          direction: 'ui-to-core',
          replaySafe: false,
          sensitiveFields: [],
        },
      ],
    };
  },

  dev_calls_list: async () => {
    return { calls: [] };
  },

  dev_export: async () => {
    return {
      writtenPath: '/tmp/dev-export.json',
      callCount: 0,
      contractCount: 2,
    };
  },

  dev_schema_get: async (_args) => {
    const req = (_args as { request?: { schemaPath?: string } } | undefined)
      ?.request;
    const path = req?.schemaPath ?? '';
    if (!path) {
      return { found: false };
    }
    // Return a minimal stub schema for any non-empty path.
    return {
      found: true,
      content: JSON.stringify(
        {
          $schema: 'https://json-schema.org/draft/2020-12/schema',
          title: 'mock-schema',
          description: `Mock schema for ${path}`,
        },
        null,
        2,
      ),
    };
  },

  preparedview_list: async (_args) => {
    // Spec 049 US4 mock coverage: proj-002 carries two already-materialized
    // views so the Playwright mock suite can exercise "Verify before
    // processing" (clean + broken paths) without also mocking
    // generate/apply. Every other project keeps the pre-existing empty
    // list.
    const projectId = (_args as { projectId?: string } | undefined)?.projectId;
    if (projectId === 'proj-002') {
      return {
        views: [
          {
            id: 'mock-sv-view-clean',
            projectId: 'proj-002',
            kind: 'symlink',
            state: 'current',
            createdAt: '2026-05-19T20:00:00Z',
            removedAt: null,
            itemCount: 1,
            items: [
              {
                id: 'mock-sv-item-clean-1',
                inventoryItemId: 'mock-sv-inv-clean-1',
                viewRelativePath: '/mock/source-views/clean/light_001.fits',
                materialization: 'symlink',
                lastObservedState: 'present',
              },
            ],
          },
          {
            id: 'mock-sv-view-broken',
            projectId: 'proj-002',
            kind: 'symlink',
            state: 'current',
            createdAt: '2026-05-19T20:00:00Z',
            removedAt: null,
            itemCount: 1,
            items: [
              {
                id: 'mock-sv-item-broken-1',
                inventoryItemId: 'mock-sv-inv-broken-1',
                viewRelativePath: '/mock/source-views/broken/light_002.fits',
                materialization: 'symlink',
                lastObservedState: 'present',
              },
            ],
          },
          // Spec 026 T014/T015/T016 mock coverage: a view already flagged
          // `stale` by a prior sweep, with one broken item pre-recorded —
          // the badge + broken-reference detail must render straight from
          // this list response, no Verify click required.
          {
            id: 'mock-sv-view-stale',
            projectId: 'proj-002',
            kind: 'symlink',
            state: 'stale',
            createdAt: '2026-05-19T20:00:00Z',
            removedAt: null,
            itemCount: 2,
            items: [
              {
                id: 'mock-sv-item-stale-ok',
                inventoryItemId: 'mock-sv-inv-stale-ok',
                viewRelativePath: '/mock/source-views/stale/light_003.fits',
                materialization: 'symlink',
                lastObservedState: 'present',
              },
              {
                id: 'mock-sv-item-stale-broken',
                inventoryItemId: 'mock-sv-inv-stale-broken',
                viewRelativePath: '/mock/source-views/stale/light_004.fits',
                materialization: 'symlink',
                lastObservedState: 'missing',
              },
            ],
          },
        ],
      };
    }
    return { views: [] };
  },

  preparedview_remove: async () => {
    return { planId: 'mock-plan-remove-001' };
  },

  preparedview_regenerate: async () => {
    return { planId: 'mock-plan-regen-001', unresolvedItemCount: 0 };
  },

  sourceview_verify: async (_args) => {
    // FR-014/FR-015: read-only, no mutation, no auto-repair — the mock
    // simply reports canned clean/broken results keyed by view id.
    const viewId = (_args as { viewId?: string } | undefined)?.viewId;
    if (viewId === 'mock-sv-view-broken') {
      return {
        clean: false,
        brokenItems: [
          {
            inventoryItemId: 'mock-sv-inv-broken-1',
            viewRelativePath: '/mock/source-views/broken/light_002.fits',
            state: 'moved',
          },
        ],
      };
    }
    return { clean: true, brokenItems: [] };
  },

  // spec 012 T008: watcher attach/detach — no real filesystem watching in
  // mock mode; the project drawer's mount/unmount effect still calls these,
  // so they must resolve rather than throw "unknown mock command".
  artifact_watcher_attach: async () => {
    return null;
  },
  artifact_watcher_detach: async () => {
    return null;
  },

  artifact_watcher_refresh: async () => {
    return [] as string[];
  },

  // ── Equipment CRUD (spec 030) ───────────────────────────────────────────

  equipment_cameras_create: async (_args) => {
    const req = (_args as { request?: CreateCamera } | undefined)?.request;
    const camera: Camera = {
      id: `cam-${crypto.randomUUID()}`,
      name: req?.name ?? '',
      aliases: req?.aliases ?? [],
      autoDetected: false,
      sensorType: req?.sensorType ?? null,
      passband: req?.passband ?? null,
      pixelSizeUm: req?.pixelSizeUm ?? null,
      sensorWidthPx: req?.sensorWidthPx ?? null,
      sensorHeightPx: req?.sensorHeightPx ?? null,
    };
    mockCameras = [...mockCameras, camera];
    return camera;
  },
  equipment_cameras_update: async (_args) => {
    const req = (_args as { request?: UpdateCamera } | undefined)?.request;
    if (!req)
      return mockContractError('equipment.not_found', 'camera not found');
    const existing = mockCameras.find((c) => c.id === req.id);
    if (!existing)
      return mockContractError(
        'equipment.not_found',
        `camera ${req.id} not found`,
      );
    const updated: Camera = {
      ...existing,
      name: req.name,
      aliases: req.aliases,
      sensorType: req.sensorType ?? null,
      passband: req.passband ?? null,
      pixelSizeUm: req.pixelSizeUm ?? null,
      sensorWidthPx: req.sensorWidthPx ?? null,
      sensorHeightPx: req.sensorHeightPx ?? null,
    };
    mockCameras = mockCameras.map((c) => (c.id === req.id ? updated : c));
    return updated;
  },
  equipment_cameras_delete: async (_args) => {
    const id = (_args as { id?: string } | undefined)?.id;
    if (!id || !mockCameras.some((c) => c.id === id)) {
      return mockContractError(
        'equipment.not_found',
        `camera ${id ?? ''} not found`,
      );
    }
    if (mockOpticalTrains.some((t) => t.cameraId === id)) {
      return mockContractError(
        'internal.database',
        'FOREIGN KEY constraint failed',
      );
    }
    mockCameras = mockCameras.filter((c) => c.id !== id);
    return null;
  },

  equipment_telescopes_create: async (_args) => {
    const req = (_args as { request?: CreateTelescope } | undefined)?.request;
    const telescope: Telescope = {
      id: `tel-${crypto.randomUUID()}`,
      name: req?.name ?? '',
      aliases: req?.aliases ?? [],
      focalLengthMm: req?.focalLengthMm ?? null,
      autoDetected: false,
    };
    mockTelescopes = [...mockTelescopes, telescope];
    return telescope;
  },
  equipment_telescopes_update: async (_args) => {
    const req = (_args as { request?: UpdateTelescope } | undefined)?.request;
    if (!req)
      return mockContractError('equipment.not_found', 'telescope not found');
    const existing = mockTelescopes.find((t) => t.id === req.id);
    if (!existing)
      return mockContractError(
        'equipment.not_found',
        `telescope ${req.id} not found`,
      );
    const updated: Telescope = {
      ...existing,
      name: req.name,
      aliases: req.aliases,
      focalLengthMm: req.focalLengthMm,
    };
    mockTelescopes = mockTelescopes.map((t) => (t.id === req.id ? updated : t));
    return updated;
  },
  equipment_telescopes_delete: async (_args) => {
    const id = (_args as { id?: string } | undefined)?.id;
    if (!id || !mockTelescopes.some((t) => t.id === id)) {
      return mockContractError(
        'equipment.not_found',
        `telescope ${id ?? ''} not found`,
      );
    }
    if (mockOpticalTrains.some((t) => t.telescopeId === id)) {
      return mockContractError(
        'internal.database',
        'FOREIGN KEY constraint failed',
      );
    }
    mockTelescopes = mockTelescopes.filter((t) => t.id !== id);
    return null;
  },

  equipment_trains_create: async (_args) => {
    const req = (_args as { request?: CreateOpticalTrain } | undefined)
      ?.request;
    const train: OpticalTrain = {
      id: `train-${crypto.randomUUID()}`,
      name: req?.name ?? '',
      telescopeId: req?.telescopeId ?? null,
      cameraId: req?.cameraId ?? null,
      focalLengthMm: req?.focalLengthMm ?? 0,
      // The FOV is backend-derived. Mock mode does not reimplement the
      // formula, so a mock-created train honestly reports no FOV.
      fovDiagonalDeg: null,
    };
    mockOpticalTrains = [...mockOpticalTrains, train];
    return train;
  },
  equipment_trains_update: async (_args) => {
    const req = (_args as { request?: UpdateOpticalTrain } | undefined)
      ?.request;
    if (!req)
      return mockContractError(
        'equipment.not_found',
        'optical train not found',
      );
    const existing = mockOpticalTrains.find((t) => t.id === req.id);
    if (!existing) {
      return mockContractError(
        'equipment.not_found',
        `optical train ${req.id} not found`,
      );
    }
    const updated: OpticalTrain = {
      ...existing,
      name: req.name,
      telescopeId: req.telescopeId,
      cameraId: req.cameraId,
      focalLengthMm: req.focalLengthMm,
    };
    mockOpticalTrains = mockOpticalTrains.map((t) =>
      t.id === req.id ? updated : t,
    );
    return updated;
  },
  equipment_trains_delete: async (_args) => {
    const id = (_args as { id?: string } | undefined)?.id;
    if (!id || !mockOpticalTrains.some((t) => t.id === id)) {
      return mockContractError(
        'equipment.not_found',
        `optical train ${id ?? ''} not found`,
      );
    }
    mockOpticalTrains = mockOpticalTrains.filter((t) => t.id !== id);
    return null;
  },

  equipment_filters_create: async (_args) => {
    const req = (_args as { request?: CreateFilter } | undefined)?.request;
    const filter: Filter = {
      id: `filt-${crypto.randomUUID()}`,
      name: req?.name ?? '',
      category: req?.category ?? 'custom',
      autoDetected: false,
    };
    mockFilters = [...mockFilters, filter];
    return filter;
  },
  equipment_filters_update: async (_args) => {
    const req = (_args as { request?: UpdateFilter } | undefined)?.request;
    if (!req)
      return mockContractError('equipment.not_found', 'filter not found');
    const existing = mockFilters.find((f) => f.id === req.id);
    if (!existing)
      return mockContractError(
        'equipment.not_found',
        `filter ${req.id} not found`,
      );
    const updated: Filter = {
      ...existing,
      name: req.name,
      category: req.category,
    };
    mockFilters = mockFilters.map((f) => (f.id === req.id ? updated : f));
    return updated;
  },
  equipment_filters_delete: async (_args) => {
    const id = (_args as { id?: string } | undefined)?.id;
    if (!id || !mockFilters.some((f) => f.id === id)) {
      return mockContractError(
        'equipment.not_found',
        `filter ${id ?? ''} not found`,
      );
    }
    mockFilters = mockFilters.filter((f) => f.id !== id);
    return null;
  },

  // ── pattern.path_preview (spec 041 per-type destination patterns, P11) ──
  //
  // Mirrors the real resolver's token-substitution + missing-token report
  // for the mock/dev environment. `{token}` names are the v1 registry
  // token names (snake_case); `sampleMetadata` carries the camelCase DTO
  // field names, so PATH_PREVIEW_TOKEN_FIELDS bridges the two. Errors are
  // thrown as full ContractError envelopes (code + message + severity +
  // retryable), matching the real backend, so mock mode exercises the same
  // `errMessage()` catalog-resolution path as production.
  pattern_path_preview: async (_args) => {
    const req = (
      _args as
        | {
            request?: {
              pattern?: string;
              sampleMetadata?: Record<string, string | null | undefined>;
            };
          }
        | undefined
    )?.request;
    const pattern = req?.pattern ?? '';
    const sample = req?.sampleMetadata ?? {};
    if (pattern.trim() === '') {
      throw {
        code: 'pattern.empty',
        message: 'Pattern is empty.',
        severity: 'blocking',
        retryable: false,
        details: null,
      };
    }
    const missingTokens: string[] = [];
    const segments = pattern
      .split('/')
      .filter((seg) => seg !== '')
      .map((seg) =>
        seg.replace(/\{([^}]*)\}/g, (_match, token: string) => {
          const field = PATH_PREVIEW_TOKEN_FIELDS[token];
          if (!field) {
            // The real resolver rejects unregistered tokens outright.
            throw {
              code: 'token.unknown',
              message: `Unknown token: ${token}`,
              severity: 'blocking',
              retryable: false,
              details: { token },
            };
          }
          const value = sample[field];
          if (value == null || value === '') {
            missingTokens.push(token);
            return PATH_PREVIEW_TOKEN_FALLBACKS[token] ?? token;
          }
          return value;
        }),
      );
    return {
      resolvedPath: segments.join('/'),
      missingTokens,
      warnings: [],
    } satisfies PathPatternPreviewResponse;
  },
} satisfies MockRegistry;

/**
 * Dispatch a mock IPC response for `cmd`.
 *
 * Returns `Promise<unknown>` because `cmd` is a runtime string; the payload
 * types are enforced at the `mockHandlers` literal above, not here.
 */
export async function mockInvoke(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<unknown> {
  // Simulate realistic network/IPC latency
  await delay(50 + Math.random() * 100);

  const handler = mockHandlers[cmd as keyof typeof mockHandlers];
  if (!handler) {
    throw new Error(`Unknown mock command: ${cmd}`);
  }
  return handler(args);
}
