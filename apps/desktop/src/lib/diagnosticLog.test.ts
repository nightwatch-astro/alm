// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * diagnosticLog tests.
 *
 * isTauri() returns false in vitest (no window.__TAURI_INTERNALS__ set),
 * so logError/logWarn must be no-ops — the plugin must never be invoked.
 */

import { describe, it, expect, vi } from 'vitest';

vi.mock('@tauri-apps/plugin-log', () => ({
  error: vi.fn(),
  warn: vi.fn(),
}));

import { logError, logWarn } from './diagnosticLog';

describe('diagnosticLog', () => {
  it('logError is a no-op outside Tauri (isTauri()=false in vitest)', async () => {
    const { error } = await import('@tauri-apps/plugin-log');
    logError('test error');
    await Promise.resolve();
    expect(error).not.toHaveBeenCalled();
  });

  it('logWarn is a no-op outside Tauri', async () => {
    const { warn } = await import('@tauri-apps/plugin-log');
    logWarn('test warn');
    await Promise.resolve();
    expect(warn).not.toHaveBeenCalled();
  });
});
