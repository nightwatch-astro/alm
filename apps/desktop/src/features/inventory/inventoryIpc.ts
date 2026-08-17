// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Per-frame inventory IPC helpers (spec 048 T014/T022/T025/T034 frontend).
 *
 * Thin `unwrap()` wrappers over the generated bindings, following the
 * `settingsIpc.ts` / `cleanupStore.ts` convention used elsewhere in the app.
 * `inventory.frame.list`, `inventory.reconcile.run`, `inventory.frame.relink`,
 * and `inventory.root_config.{get,set}` had real backend implementations
 * (spec 048 T006) but zero frontend callers before this module — see the
 * documented gap in `crates/e2e-tests/tests/inventory_journeys.rs`.
 */

import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
import type {
  InventoryFrameListRequest,
  InventoryFrameListResponse,
  InventoryFrameRelinkRequest,
  InventoryFrameRelinkResponse,
  InventoryReconcileRunRequest,
  InventoryReconcileRunResponse,
  RootConfigGetRequest,
  RootConfigSetRequest,
  RootInventoryConfig,
  RootWatcherRequest,
  RawFrameCleanupScanRequest,
  RawFrameCleanupScanResponse,
  RawFrameCleanupGenerateRequest,
  GenerateCleanupPlanResult,
} from '@/bindings/index';

export async function inventoryFrameList(
  req: InventoryFrameListRequest,
): Promise<InventoryFrameListResponse> {
  return unwrap(
    await commands.inventoryFrameList(
      req as Parameters<typeof commands.inventoryFrameList>[0],
    ),
  );
}

export async function inventoryReconcileRun(
  req: InventoryReconcileRunRequest,
): Promise<InventoryReconcileRunResponse> {
  return unwrap(await commands.inventoryReconcileRun(req));
}

export async function inventoryFrameRelink(
  req: InventoryFrameRelinkRequest,
): Promise<InventoryFrameRelinkResponse> {
  return unwrap(await commands.inventoryFrameRelink(req));
}

export async function inventoryRootConfigGet(
  req: RootConfigGetRequest,
): Promise<RootInventoryConfig> {
  return unwrap(await commands.inventoryRootConfigGet(req));
}

export async function inventoryRootConfigSet(
  req: RootConfigSetRequest,
): Promise<RootInventoryConfig> {
  return unwrap(
    await commands.inventoryRootConfigSet(
      req as Parameters<typeof commands.inventoryRootConfigSet>[0],
    ),
  );
}

export async function inventoryWatcherAttach(
  req: RootWatcherRequest,
): Promise<void> {
  unwrap(await commands.inventoryWatcherAttach(req));
}

export async function inventoryWatcherDetach(
  req: RootWatcherRequest,
): Promise<void> {
  unwrap(await commands.inventoryWatcherDetach(req));
}

export async function cleanupRawFramesScan(
  request: RawFrameCleanupScanRequest,
): Promise<RawFrameCleanupScanResponse> {
  return unwrap(
    await commands.cleanupRawFramesScan(
      request as Parameters<typeof commands.cleanupRawFramesScan>[0],
    ),
  );
}

export async function cleanupRawFramesGenerate(
  request: RawFrameCleanupGenerateRequest,
): Promise<GenerateCleanupPlanResult> {
  return unwrap(await commands.cleanupRawFramesGenerate(request));
}
