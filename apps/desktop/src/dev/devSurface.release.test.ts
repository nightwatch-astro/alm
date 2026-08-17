// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * T072 — Release build has NO reachable developer route or commands (FR-031, SC-009).
 *
 * Verifies that when VITE_DEV_TOOLS is not "true":
 * 1. The DEV_TOOLS_ENABLED flag is false.
 * 2. devContractsRoute is null (never added to the route tree).
 * 3. The recorder's setInvokeOverride is never called (no proxy installed).
 *
 * This is a static/unit check — the build-time constant is what matters for
 * tree-shaking. We verify the runtime guard here.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// This file's dynamic imports (./recorder, @/bindings, @/app/router) are the
// first touch of those module graphs in an isolated worker, and cold
// transform of that graph alone measures ~4s locally — right at vitest's 5s
// default testTimeout. Under CI's concurrent worker load that tips over into
// a false failure (issue #737; same timeout-flake class as #1082's Windows-CI
// fix). Raise it file-wide rather than per-test so no sibling is left behind.
// Bumped 15_000→30_000: the heavier spec-062 message bundle adds import
// time; measured ~12s locally in the full suite under concurrent load.
vi.setConfig({ testTimeout: 30_000 });

describe('T072: release build dev surface gate', () => {
  let originalEnv: string;

  beforeEach(() => {
    const env = import.meta.env as unknown as
      | Record<string, string>
      | undefined;
    originalEnv = env?.VITE_DEV_TOOLS ?? 'false';
  });

  afterEach(() => {
    (import.meta.env as unknown as Record<string, string>).VITE_DEV_TOOLS =
      originalEnv;
  });

  it('DEV_TOOLS_ENABLED is false when VITE_DEV_TOOLS is not "true"', () => {
    // The vite.config.ts sets VITE_DEV_TOOLS="false" by default.
    // In the test environment it is also "false" (no override provided).
    const devToolsEnv = import.meta.env.VITE_DEV_TOOLS;
    const devToolsEnabled = devToolsEnv === 'true';
    expect(devToolsEnabled).toBe(false);
  });

  it('recorder wrap returns the original dispatch unchanged when devMode=false', async () => {
    const { wrap } = await import('./recorder');
    const original = vi.fn().mockResolvedValue({ status: 'ok' });
    const wrapped = wrap(original, false);
    // When devMode=false, wrap returns the original function reference
    expect(wrapped).toBe(original);
  });

  it('setInvokeOverride(null) stops routing generated commands through the override', async () => {
    const { setInvokeOverride } = await import('@/api/ipc');
    const { commands } = await import('@/bindings');
    const seen: string[] = [];
    const overrideCall = vi.fn((cmd: string) => {
      seen.push(cmd);
      return Promise.resolve([]);
    });

    // While an override is installed, a generated command dispatches through it
    // — the bindings route via the IPC switcher, not @tauri-apps/api/core.
    setInvokeOverride(overrideCall);
    await commands.sessionsList(null, null);
    expect(seen).toContain('sessions_list');

    // Clearing it (release-build state) must stop dispatching through the
    // override. There's no real Tauri runtime in jsdom, so the real invoke
    // rejects here — the assertion is that it did NOT go through the override.
    setInvokeOverride(null);
    overrideCall.mockClear();
    await commands.sessionsList(null, null).catch(() => {});
    expect(overrideCall).not.toHaveBeenCalled();
  });

  it('dev route is not registered in the real router when DEV_TOOLS_ENABLED=false', async () => {
    // VITE_DEV_TOOLS is "false" in tests (default vite.config.ts), so
    // router.tsx's DEV_TOOLS_ENABLED gate evaluates false and devContractsRoute
    // is never added to routeTree — verified here against the actual built
    // `router` export, not just the env var the gate reads.
    expect(import.meta.env.VITE_DEV_TOOLS).not.toBe('true');
    const { router } = await import('@/app/router');
    expect(Object.keys(router.routesById)).not.toContain('/dev/contracts');
    expect(Object.keys(router.routesById)).not.toContain('/dev/settings');
  });
});
