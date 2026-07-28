// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Settings feature IPC helpers (spec 037 caller migration).
 *
 * Moves the Settings pane glue off the hand-written `@/api/commands` wrappers
 * onto the generated `commands.*` bindings (FR-004: behaviour is moved, not
 * dropped). `unwrap()` turns each generated `Result` into the throw-on-error
 * contract the panes already rely on. Every settings pane (Advanced, Cleanup,
 * DataSources, NamingStructure, ProcessingTools, CalibrationMatching,
 * SourceProtectionOverride, ResolverSettingsControl, useAutoSave, SettingsKit)
 * imports from this one module.
 */

import type { ZodType } from 'zod';
import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import { ipcArgs } from '@/lib/ipc-args';
// `LibraryRoot` is a plain `_Serialize` re-export in `@/bindings/types` (the
// facade every settings pane already imports it through), NOT the wider
// `LibraryRoot_Serialize | LibraryRoot_Deserialize` union `@/bindings/index`
// exports under the same name — importing the union here would widen
// `listRoots()`'s return type against DataSources.tsx's `LibraryRoot` state.
import type { LibraryRoot } from '@/bindings/types';
import type {
  SettingsData,
  RestoreDefaultsResponse,
  SetSourceOverrideResponse,
  CalibrationTolerances,
  UpdateCalibrationTolerances,
  CleanupPolicy,
  CleanupPolicyEntry,
  CleanupAction,
  UpdateCleanupPolicy,
  IngestionSettings,
  UpdateIngestionSettings,
  ProtectionLevel,
  SourceProtectionGetResponse,
  SourceProtectionSetRequest,
  SourceProtectionSetResponse,
  ToolProfileListResponse,
  ToolProfileSummary,
  ToolDiscoverRequest,
  ToolDiscoverResponse,
  UpdateProcessingTool,
  ToolPathValidation,
  PatternPartDto,
  MetadataBundleDto_Serialize as MetadataBundleDto,
  PatternValidateResponse_Serialize as PatternValidateResponse,
  PatternPreviewResponse,
  PathPatternPreviewResponse,
  ResolverSettings,
  ResolverSettingsResponse,
  TargetCacheClearResponse,
  FirstRunRestartResponse,
  Camera,
  SensorType,
  Telescope,
  OpticalTrain,
  Filter,
  FilterCategory,
  CreateCamera,
  UpdateCamera,
  CreateTelescope,
  UpdateTelescope,
  CreateOpticalTrain,
  UpdateOpticalTrain,
  CreateFilter,
  UpdateFilter,
  RemapVerification,
  AuditExportResponse,
  AuditListResponse,
  AuditFilterDto,
  AuditPaginationDto,
  InventoryReconcileRunResponse,
} from '@/bindings/index';

export type {
  ProtectionLevel,
  SourceProtectionGetResponse,
  SourceProtectionSetRequest,
  SourceProtectionSetResponse,
  ToolProfileSummary,
  ResolverSettings,
  TargetCacheClearResponse,
  FirstRunRestartResponse,
  Camera,
  SensorType,
  Telescope,
  OpticalTrain,
  Filter,
  FilterCategory,
  CreateCamera,
  UpdateCamera,
  CreateTelescope,
  UpdateTelescope,
  CreateOpticalTrain,
  UpdateOpticalTrain,
  CreateFilter,
  UpdateFilter,
  RemapVerification,
  InventoryReconcileRunResponse,
};
export type { PatternPartDto as PatternPart };
export type {
  PatternValidateResponse,
  PatternPreviewResponse,
  PathPatternPreviewResponse,
};
export type { CalibrationTolerances, UpdateCalibrationTolerances };
export type { CleanupPolicy, CleanupPolicyEntry, CleanupAction };
export type { IngestionSettings, UpdateIngestionSettings };

// ── Settings scope read/write (spec 018) ──────────────────────────────────────

export async function getSettings(args: {
  scope: string;
}): Promise<SettingsData> {
  return unwrap(await commands.settingsGet(args.scope));
}

/**
 * Type-safe variant of `getSettings` (C-5). Calls `settingsGet` and passes
 * `data.values` through a Zod `.partial()` schema so callers receive a typed
 * object rather than `Record<string, unknown>`. Unknown/extra keys are dropped.
 * Never throws — settings reads are always best-effort.
 *
 * Validation is per field, not whole-object. `safeParse` on the whole object
 * rejects it entirely when a single field is bad, which would discard every
 * other valid persisted setting in the scope; a stale enum member left by an
 * older build would silently reset the whole pane. Each key is validated
 * against its own `schema.shape` entry instead, so a bad field is dropped and
 * its neighbours survive.
 *
 * Note there is no "return the schema defaults" behaviour to fall back on: the
 * per-scope schemas are `.partial()` with no `.default()`, so an absent field
 * is simply absent and callers handle `undefined`.
 *
 * Usage:
 *   const vals = await getSettingsTyped('advanced', AdvancedSettingsSchema);
 *   if (vals.logLevel) setLogLevel(vals.logLevel);
 */
export async function getSettingsTyped<T extends Record<string, unknown>>(
  scope: string,
  schema: ZodType<T>,
): Promise<T> {
  const data = await getSettings({ scope });
  // Narrow to a plain object: a primitive or array here cannot carry named
  // fields, and `in` is not valid against a primitive.
  const raw: unknown = data.values;
  const values: Record<string, unknown> =
    typeof raw === 'object' && raw !== null && !Array.isArray(raw)
      ? (raw as Record<string, unknown>)
      : {};

  const result = schema.safeParse(values);
  if (result.success) return result.data;

  // Fall back to per-field salvage. `shape` exists on object schemas; if this
  // is not one, there is nothing to iterate and the empty object is correct.
  const shape = (schema as unknown as { shape?: Record<string, ZodType> })
    .shape;
  if (!shape) return {} as T;

  const salvaged: Record<string, unknown> = {};
  for (const [key, fieldSchema] of Object.entries(shape)) {
    if (!(key in values)) continue;
    const field = fieldSchema.safeParse(values[key]);
    if (field.success) salvaged[key] = field.data;
  }
  return salvaged as T;
}

export async function updateSettings(args: {
  scope: string;
  values: Record<string, unknown>;
}): Promise<void> {
  unwrap(await commands.settingsUpdate(args.scope, args.values));
}

/**
 * `settings.restore-defaults` — restore named keys (or all keys when `keys`
 * is empty) to their default values (spec 018 T028).
 */
export async function settingsRestoreDefaults(
  keys: string[],
): Promise<RestoreDefaultsResponse> {
  return unwrap(await commands.settingsRestoreDefaults({ keys }));
}

/**
 * `settings.overridable-keys` — return the authoritative list of stable settings
 * keys that can be overridden per source root (spec 018 T025).
 *
 * Falls back to a hardcoded pair when the command fails (forward-compat).
 */
export async function settingsOverridableKeys(): Promise<string[]> {
  try {
    return unwrap(await commands.settingsOverridableKeys());
  } catch {
    // Fallback for older backends or failed calls — matches the formerly hardcoded list.
    return ['hashOnScan', 'followSymlinks'];
  }
}

/**
 * `settings.source-override.set` — set a per-source settings override
 * (spec 018 T025).
 */
export async function settingsSourceOverrideSet(args: {
  sourceId: string;
  key: string;
  value: unknown;
}): Promise<SetSourceOverrideResponse> {
  return unwrap(
    await commands.settingsSourceOverrideSet({
      sourceId: args.sourceId,
      key: args.key,
      value: args.value,
    }),
  );
}

// ── Data sources / roots (spec 003) ───────────────────────────────────────────

export async function listRoots(): Promise<LibraryRoot[]> {
  return unwrap(await commands.rootsList());
}

/**
 * `firstrun.restart` — reopen the first-run source setup wizard (spec 003
 * US3). This clears the
 * `first_run_state.completed_at` flag and returns the currently registered
 * sources so the wizard's working buffer can be prefilled for editing (A7).
 * `confirm: true` is required by the backend to guard against accidental
 * restarts (R-E5).
 */
export async function restartFirstRun(): Promise<FirstRunRestartResponse> {
  return unwrap(await commands.firstrunRestart({ confirm: true }));
}

export async function registerRoot(args: {
  path: string;
  category: string;
  scanSettings: Record<string, unknown>;
}): Promise<void> {
  unwrap(
    await commands.rootsRegister(args.path, args.category, args.scanSettings),
  );
}

/**
 * `inbox.scan_folder` — rescan one registered root (P6a).
 *
 * Replaces the former `startScan`/`scan.start` wrapper: `scan.start` is a
 * dead stub that never touched the database (silent no-op), so the Settings
 * "Rescan" button now calls the same real scan command the setup wizard and
 * the Inbox page's "Rescan all" use. Persists/refreshes the root's
 * `inbox_source_groups` rows (which `roots.list`'s `lastScanned` is derived
 * from) — no classification is run, matching `useInboxRescan`'s per-root
 * scope (classification stays a separate, explicit Inbox-page step).
 */
export async function rescanRoot(args: {
  rootId: string;
  rootAbsolutePath: string;
}): Promise<void> {
  unwrap(
    await commands.inboxScanFolder({
      rootId: args.rootId,
      rootAbsolutePath: args.rootAbsolutePath,
    }),
  );
}

/**
 * `inventory.reconcile.run` — run an on-demand per-frame reconciliation pass
 * over a root (spec 048 T022). Read-only walk: reports `missing`/`recovered`/
 * `size_backfilled` counts, never mutates a file.
 */
export async function reconcileRoot(args: {
  rootId: string;
}): Promise<InventoryReconcileRunResponse> {
  return unwrap(
    await commands.inventoryReconcileRun({
      rootId: args.rootId,
      reason: 'on_demand',
    }),
  );
}

/**
 * `roots.remap` — preview a root path remap. Verifies whether a set of
 * sample relative paths from the current root can be found under `newPath`
 * (P6a). Does NOT mutate anything; call `applyRootRemap` after review.
 */
export async function remapRoot(args: {
  rootId: string;
  newPath: string;
}): Promise<RemapVerification> {
  return unwrap(await commands.rootsRemap(args.rootId, args.newPath));
}

/**
 * `roots.remap.apply` — apply a previously previewed root remap (P6a).
 * The backend has no server-side memory of a pending preview, so `newPath`
 * must be resent (and is re-validated) alongside the `verified` flag, which
 * should be the `allVerified` value from the matching `remapRoot` preview.
 */
export async function applyRootRemap(args: {
  rootId: string;
  newPath: string;
  verified: boolean;
}): Promise<void> {
  unwrap(
    await commands.rootsRemapApply(args.rootId, args.newPath, args.verified),
  );
}

/**
 * `sources.set_active` — enable or disable a registered source (P6b).
 * Disabled roots are excluded from scan/ingest surfaces; their history
 * (sessions, plan items, file records) is retained untouched.
 */
export async function setRootActive(args: {
  rootId: string;
  active: boolean;
}): Promise<void> {
  unwrap(await commands.sourcesSetActive(args.rootId, args.active));
}

/**
 * `roots.delete` — permanently remove a root's registration (P6b, decision D8).
 * Blocks with `root.has_dependents` when dependent records (inbox items, plan
 * items, file records, sessions) still reference the root — the caller must
 * surface that block reason to the user. Files on disk are never touched.
 */
export async function deleteRoot(args: { rootId: string }): Promise<void> {
  unwrap(await commands.rootsDelete(args.rootId));
}

// ── Calibration tolerances (spec 007) ─────────────────────────────────────────

export async function calibrationTolerancesGet(): Promise<CalibrationTolerances> {
  return unwrap(await commands.calibrationTolerancesGet());
}

export async function calibrationTolerancesUpdate(
  request: UpdateCalibrationTolerances,
): Promise<CalibrationTolerances> {
  return unwrap(await commands.calibrationTolerancesUpdate(request));
}

// ── Cleanup policy (issue #804) ───────────────────────────────────────────────
//
// The policy `cleanup_scan`/`cleanup_plan_generate` actually read: a per-data-
// type (`intermediate`/`master`/`final`) action plus an auto-on-completion
// flag, persisted in the protection-defaults store. Deliberately NOT the
// `cleanup` settings scope (block-permanent-delete, protected categories) —
// the scan path never consults that scope.

export async function cleanupPolicyGet(): Promise<CleanupPolicy> {
  return unwrap(await commands.cleanupPolicyGet());
}

export async function cleanupPolicyUpdate(
  request: UpdateCleanupPolicy,
): Promise<CleanupPolicy> {
  return unwrap(await commands.cleanupPolicyUpdate(request));
}

// ── Ingestion settings (spec 030, package P12) ────────────────────────────────

export async function ingestionSettingsGet(): Promise<IngestionSettings> {
  return unwrap(await commands.ingestionSettingsGet());
}

export async function ingestionSettingsUpdate(
  request: UpdateIngestionSettings,
): Promise<IngestionSettings> {
  return unwrap(await commands.ingestionSettingsUpdate(request));
}

// ── Source protection (spec 016 US2) ──────────────────────────────────────────

/**
 * `source.protection.get` — resolve effective protection for a source.
 * Pass `sourceId: null` to retrieve global defaults.
 */
export async function sourceProtectionGet(
  sourceId: string | null,
): Promise<SourceProtectionGetResponse> {
  return unwrap(await commands.sourceProtectionGet(sourceId));
}

/**
 * `source.protection.set` — set or replace the protection override for a source
 * (spec 016 US2, T013). Emits a `protection.source.set` audit event.
 */
export async function sourceProtectionSet(
  request: SourceProtectionSetRequest,
): Promise<SourceProtectionSetResponse> {
  return unwrap(
    await commands.sourceProtectionSet(
      ipcArgs<typeof commands.sourceProtectionSet>(request),
    ),
  );
}

// ── Processing tools (spec 011) ───────────────────────────────────────────────

export async function toolProfileList(): Promise<ToolProfileListResponse> {
  return unwrap(await commands.toolsList());
}

export async function toolUpdate(
  request: UpdateProcessingTool,
): Promise<ToolProfileSummary> {
  return unwrap(await commands.toolsUpdate(request));
}

export async function toolValidatePath(
  path: string,
): Promise<ToolPathValidation> {
  return unwrap(await commands.toolsValidatePath(path));
}

export async function toolDiscover(
  request: ToolDiscoverRequest,
): Promise<ToolDiscoverResponse> {
  return unwrap(await commands.toolsDiscover(request));
}

// ── Naming pattern (spec 015) ─────────────────────────────────────────────────

/**
 * Validate a pattern structurally (no metadata required).
 * Never rejects — all error states are in the response body.
 */
export async function patternValidate(
  pattern: PatternPartDto[],
): Promise<PatternValidateResponse> {
  return unwrap(await commands.patternValidate({ pattern }));
}

/**
 * Preview a pattern against sample metadata for the Settings UI live preview.
 * Applies the same validation and sanitization pipeline as pattern.resolve.
 */
export async function patternPreview(
  pattern: PatternPartDto[],
  sampleMetadata: MetadataBundleDto,
): Promise<PatternPreviewResponse> {
  return unwrap(
    await commands.patternPreview(
      ipcArgs<typeof commands.patternPreview>({ pattern, sampleMetadata }),
    ),
  );
}

/**
 * Preview a per-type destination **path-string** pattern (e.g.
 * `masters/flats/{filter}/`) against sample metadata, for the per-frame-type
 * destination pattern editor's live preview (spec 041, package P11).
 *
 * Unlike `patternPreview` (which resolves the `PatternPart[]` token/separator
 * model), `pattern` here is a raw path string that may interleave `{token}`
 * placeholders with literal directory segments.
 */
export async function patternPathPreview(
  pattern: string,
  sampleMetadata: MetadataBundleDto,
): Promise<PathPatternPreviewResponse> {
  return unwrap(
    await commands.patternPathPreview(
      ipcArgs<typeof commands.patternPathPreview>({ pattern, sampleMetadata }),
    ),
  );
}

// ── SIMBAD resolver settings (spec 035, FR-015) ───────────────────────────────

/** Contract version for the spec-035 `target.resolution.settings` commands. */
const TARGET_SEARCH_CONTRACT_VERSION = '1.0';

/**
 * `target.resolution.settings` — read the SIMBAD resolver settings
 * (online toggle, endpoint, debounce, request timeout) (spec 035, FR-015).
 */
export async function getResolverSettings(): Promise<ResolverSettingsResponse> {
  return unwrap(
    await commands.targetResolutionSettings({
      contractVersion: TARGET_SEARCH_CONTRACT_VERSION,
      requestId: crypto.randomUUID(),
      op: 'get',
    }),
  );
}

/**
 * `target.resolution.settings.update` — persist new resolver settings
 * (spec 035, FR-015). Returns the saved settings.
 */
export async function updateResolverSettings(
  settings: ResolverSettings,
): Promise<ResolverSettingsResponse> {
  return unwrap(
    await commands.targetResolutionSettingsUpdate({
      contractVersion: TARGET_SEARCH_CONTRACT_VERSION,
      requestId: crypto.randomUUID(),
      op: 'update',
      settings,
    }),
  );
}

/**
 * `target.cache.clear` — wipe the shared SIMBAD redb resolve cache and
 * re-warm it from the bundled seed + existing durable targets (spec 052 P1
 * FR-002). Never touches durable target records.
 */
export async function clearResolveCache(): Promise<TargetCacheClearResponse> {
  return unwrap(await commands.targetCacheClear());
}

// ── Equipment (spec 030) ───────────────────────────────────────────────────────
//
// Cameras, telescopes, optical trains, and filters. CRUD orchestration lives in
// `app_core::equipment` (re-exported from `app_core_calibration`); commands are
// registered in `apps/desktop/src-tauri/src/commands/equipment.rs`.

export async function equipmentCamerasList(): Promise<Camera[]> {
  return unwrap(await commands.equipmentCamerasList());
}

export async function equipmentCameraCreate(
  request: CreateCamera,
): Promise<Camera> {
  return unwrap(await commands.equipmentCamerasCreate(request));
}

export async function equipmentCameraUpdate(
  request: UpdateCamera,
): Promise<Camera> {
  return unwrap(await commands.equipmentCamerasUpdate(request));
}

export async function equipmentCameraDelete(id: string): Promise<void> {
  unwrap(await commands.equipmentCamerasDelete(id));
}

export async function equipmentTelescopesList(): Promise<Telescope[]> {
  return unwrap(await commands.equipmentTelescopesList());
}

export async function equipmentTelescopeCreate(
  request: CreateTelescope,
): Promise<Telescope> {
  return unwrap(await commands.equipmentTelescopesCreate(request));
}

export async function equipmentTelescopeUpdate(
  request: UpdateTelescope,
): Promise<Telescope> {
  return unwrap(await commands.equipmentTelescopesUpdate(request));
}

export async function equipmentTelescopeDelete(id: string): Promise<void> {
  unwrap(await commands.equipmentTelescopesDelete(id));
}

export async function equipmentTrainsList(): Promise<OpticalTrain[]> {
  return unwrap(await commands.equipmentTrainsList());
}

export async function equipmentTrainCreate(
  request: CreateOpticalTrain,
): Promise<OpticalTrain> {
  return unwrap(await commands.equipmentTrainsCreate(request));
}

export async function equipmentTrainUpdate(
  request: UpdateOpticalTrain,
): Promise<OpticalTrain> {
  return unwrap(await commands.equipmentTrainsUpdate(request));
}

export async function equipmentTrainDelete(id: string): Promise<void> {
  unwrap(await commands.equipmentTrainsDelete(id));
}

export async function equipmentFiltersList(): Promise<Filter[]> {
  return unwrap(await commands.equipmentFiltersList());
}

export async function equipmentFilterCreate(
  request: CreateFilter,
): Promise<Filter> {
  return unwrap(await commands.equipmentFiltersCreate(request));
}

export async function equipmentFilterUpdate(
  request: UpdateFilter,
): Promise<Filter> {
  return unwrap(await commands.equipmentFiltersUpdate(request));
}

export async function equipmentFilterDelete(id: string): Promise<void> {
  unwrap(await commands.equipmentFiltersDelete(id));
}

// ── Audit log (spec 029, real backend) ────────────────────────────────────────
//
// `audit.list` / `audit.export` were spec-029 stubs returning a hardcoded
// fixture and ignoring `filters` / `pagination`. They now read the durable
// `audit_log_entry` table (migration `0002_lifecycle.sql`) via
// `persistence_db::repositories::audit`, with real server-side filtering
// (`entityType`, `entityId`, `outcome`, `severity`, `search`, `from`/`to`) and
// `limit`/`offset` pagination — see `apps/desktop/src-tauri/src/commands/audit.rs`.

export async function auditList(
  filters: AuditFilterDto | null,
  pagination: AuditPaginationDto | null,
): Promise<AuditListResponse> {
  return unwrap(await commands.auditList(filters, pagination));
}

/** `audit.export` — export filtered audit entries to a file (NDJSON). */
export async function auditExport(
  filePath: string,
  filters: AuditFilterDto | null,
): Promise<AuditExportResponse> {
  return unwrap(await commands.auditExport(filePath, filters));
}
