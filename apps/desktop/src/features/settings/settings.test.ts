// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * spec 018 T009 — desktop layer settings tests.
 *
 * Tests the real layer that exists: useAutoSave debounce + updateSettings, and
 * the two new command wrappers settingsRestoreDefaults / settingsSourceOverrideSet.
 *
 * Mock pattern mirrors ResolverSettingsControl.test.tsx: vi.hoisted() + vi.mock().
 * Mocks the generated bindings surface (spec 037) so the real `settingsIpc`
 * wrappers run and their arg-shaping is exercised.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';

// ── Mock the generated bindings surface before any module imports it ─────────

const {
  mockUpdateSettings,
  mockSettingsRestoreDefaults,
  mockSettingsSourceOverrideSet,
  mockSettingsGet,
} = vi.hoisted(() => ({
  mockUpdateSettings: vi.fn(),
  mockSettingsRestoreDefaults: vi.fn(),
  mockSettingsSourceOverrideSet: vi.fn(),
  mockSettingsGet: vi.fn(),
}));

vi.mock('@/bindings/index', () => ({
  commands: {
    settingsUpdate: mockUpdateSettings,
    settingsRestoreDefaults: mockSettingsRestoreDefaults,
    settingsSourceOverrideSet: mockSettingsSourceOverrideSet,
    settingsGet: mockSettingsGet,
  },
}));

import { useAutoSave } from './useAutoSave';
import {
  getSettingsTyped,
  settingsRestoreDefaults,
  settingsSourceOverrideSet,
} from './settingsIpc';
import {
  AdvancedSettingsSchema,
  CleanupSettingsSchema,
  FramingSettingsSchema,
  SourceViewsSettingsSchema,
  NamingSettingsSchema,
} from './settingsSchemas';

// ── useAutoSave ───────────────────────────────────────────────────────────────

describe('useAutoSave', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mockUpdateSettings.mockResolvedValue({ status: 'ok', data: null });
  });

  afterEach(() => {
    vi.useRealTimers();
    mockUpdateSettings.mockReset();
  });

  it('debounces rapid save calls and calls updateSettings only once per burst', async () => {
    const { result } = renderHook(() => useAutoSave());

    act(() => {
      result.current.save('advanced', { logLevel: 'debug' });
      result.current.save('advanced', { logLevel: 'info' });
      result.current.save('advanced', { logLevel: 'warn' });
    });

    // Before debounce fires: no call yet.
    expect(mockUpdateSettings).not.toHaveBeenCalled();

    // Advance past 300ms debounce.
    await act(async () => {
      vi.advanceTimersByTime(350);
      // Let the async updateSettings promise resolve.
      await Promise.resolve();
    });

    // Only one call — the last value in the burst.
    expect(mockUpdateSettings).toHaveBeenCalledTimes(1);
    expect(mockUpdateSettings).toHaveBeenCalledWith('advanced', {
      logLevel: 'warn',
    });
  });

  it('does not fire updateSettings before the 300ms window elapses', () => {
    const { result } = renderHook(() => useAutoSave());

    act(() => {
      result.current.save('cleanup', { blockPermanentDelete: true });
    });

    act(() => {
      vi.advanceTimersByTime(200);
    });

    expect(mockUpdateSettings).not.toHaveBeenCalled();
  });

  it('resets saved flag to false after 1.5s feedback window', async () => {
    const { result } = renderHook(() => useAutoSave());

    act(() => {
      result.current.save('advanced', { logLevel: 'debug' });
    });

    await act(async () => {
      vi.advanceTimersByTime(350);
      await Promise.resolve();
    });

    expect(result.current.saved).toBe(true);

    await act(async () => {
      vi.advanceTimersByTime(1600);
    });

    expect(result.current.saved).toBe(false);
  });

  it('passes scope and values to updateSettings with correct shape', async () => {
    const { result } = renderHook(() => useAutoSave());
    const values = {
      blockPermanentDelete: false,
      defaultProtection: 'unprotected',
    };

    act(() => {
      result.current.save('cleanup', values);
    });

    await act(async () => {
      vi.advanceTimersByTime(350);
      await Promise.resolve();
    });

    expect(mockUpdateSettings).toHaveBeenCalledWith('cleanup', values);
  });

  it('DS-7: flushes ALL pending scopes when two different scopes are queued within 300ms', async () => {
    const { result } = renderHook(() => useAutoSave());

    act(() => {
      result.current.save('advanced', { logLevel: 'debug' });
      // Second call within the window targets a DIFFERENT scope.
      result.current.save('cleanup', { blockPermanentDelete: true });
    });

    // Neither scope should fire yet.
    expect(mockUpdateSettings).not.toHaveBeenCalled();

    await act(async () => {
      vi.advanceTimersByTime(350);
      await Promise.resolve();
      await Promise.resolve();
    });

    // Both scopes must be flushed.
    expect(mockUpdateSettings).toHaveBeenCalledTimes(2);
    expect(mockUpdateSettings).toHaveBeenCalledWith('advanced', {
      logLevel: 'debug',
    });
    expect(mockUpdateSettings).toHaveBeenCalledWith('cleanup', {
      blockPermanentDelete: true,
    });
  });

  it('DS-7: unmount flushes pending writes without waiting for the debounce', async () => {
    const { result, unmount } = renderHook(() => useAutoSave());

    act(() => {
      result.current.save('advanced', { logLevel: 'warn' });
    });

    // Does NOT advance timers — unmount should flush immediately.
    await act(async () => {
      unmount();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(mockUpdateSettings).toHaveBeenCalledTimes(1);
    expect(mockUpdateSettings).toHaveBeenCalledWith('advanced', {
      logLevel: 'warn',
    });
  });
});

// ── settingsRestoreDefaults wrapper ───────────────────────────────────────────

describe('settingsRestoreDefaults', () => {
  beforeEach(() => {
    mockSettingsRestoreDefaults.mockResolvedValue({
      status: 'ok',
      data: {
        status: 'success',
        restored: ['logLevel'],
        alreadyAtDefault: [],
      },
    });
  });

  afterEach(() => {
    mockSettingsRestoreDefaults.mockReset();
  });

  it('calls the generated command with a { keys } object', async () => {
    await settingsRestoreDefaults(['logLevel', 'rememberFollowLogs']);
    expect(mockSettingsRestoreDefaults).toHaveBeenCalledWith({
      keys: ['logLevel', 'rememberFollowLogs'],
    });
  });

  it('passes an empty array to restore all keys', async () => {
    await settingsRestoreDefaults([]);
    expect(mockSettingsRestoreDefaults).toHaveBeenCalledWith({ keys: [] });
  });

  it('returns the RestoreDefaultsResponse from the backend', async () => {
    const result = await settingsRestoreDefaults(['logLevel']);
    expect(result).toEqual({
      status: 'success',
      restored: ['logLevel'],
      alreadyAtDefault: [],
    });
  });
});

// ── settingsSourceOverrideSet wrapper ─────────────────────────────────────────

describe('settingsSourceOverrideSet', () => {
  beforeEach(() => {
    mockSettingsSourceOverrideSet.mockResolvedValue({
      status: 'ok',
      data: {
        sourceId: 'root-uuid-1',
        key: 'hashOnScan',
      },
    });
  });

  afterEach(() => {
    mockSettingsSourceOverrideSet.mockReset();
  });

  it('calls the generated command with camelCase sourceId, key, value', async () => {
    await settingsSourceOverrideSet({
      sourceId: 'root-uuid-1',
      key: 'hashOnScan',
      value: true,
    });
    expect(mockSettingsSourceOverrideSet).toHaveBeenCalledWith({
      sourceId: 'root-uuid-1',
      key: 'hashOnScan',
      value: true,
    });
  });

  it('returns the SetSourceOverrideResponse from the backend', async () => {
    const result = await settingsSourceOverrideSet({
      sourceId: 'root-uuid-1',
      key: 'hashOnScan',
      value: false,
    });
    expect(result).toEqual({ sourceId: 'root-uuid-1', key: 'hashOnScan' });
  });
});

// ── getSettingsTyped ──────────────────────────────────────────────────────────

describe('getSettingsTyped', () => {
  beforeEach(() => {
    mockSettingsGet.mockReset();
  });

  const respond = (values: Record<string, unknown>) => {
    mockSettingsGet.mockResolvedValue({ status: 'ok', data: { values } });
  };

  it('returns every field when the whole object validates', async () => {
    respond({ logLevel: 'debug', rememberFollowLogs: true, devMode: false });
    await expect(
      getSettingsTyped('advanced', AdvancedSettingsSchema),
    ).resolves.toEqual({
      logLevel: 'debug',
      rememberFollowLogs: true,
      devMode: false,
    });
  });

  it('keeps the valid fields when one field is invalid', async () => {
    // A stale enum member left by an older build must not discard its
    // neighbours: whole-object safeParse would reject all three.
    respond({
      logLevel: 'verbose',
      rememberFollowLogs: true,
      devMode: false,
    });
    await expect(
      getSettingsTyped('advanced', AdvancedSettingsSchema),
    ).resolves.toEqual({ rememberFollowLogs: true, devMode: false });
  });

  it('drops unknown keys', async () => {
    respond({ logLevel: 'info', somethingRetired: 'x' });
    await expect(
      getSettingsTyped('advanced', AdvancedSettingsSchema),
    ).resolves.toEqual({ logLevel: 'info' });
  });

  it('returns an empty object when every field is invalid', async () => {
    respond({ logLevel: 42, rememberFollowLogs: 'yes' });
    await expect(
      getSettingsTyped('advanced', AdvancedSettingsSchema),
    ).resolves.toEqual({});
  });

  it('returns an empty object when the scope has no values', async () => {
    respond({});
    await expect(
      getSettingsTyped('advanced', AdvancedSettingsSchema),
    ).resolves.toEqual({});
  });

  // Each pane's mount read drops a stale field and keeps its neighbour, so the
  // pane falls back to its in-code default for the bad key alone.
  it('cleanup: drops a retired protection level, keeps the sibling', async () => {
    // 'standard' is the third level issue #506 retired.
    respond({ defaultProtection: 'standard', blockPermanentDelete: false });
    await expect(
      getSettingsTyped('cleanup', CleanupSettingsSchema),
    ).resolves.toEqual({ blockPermanentDelete: false });
  });

  it('framing: drops a non-numeric tolerance, keeps the sibling', async () => {
    respond({
      framingRotationToleranceDeg: '3.0',
      framingPointingFallbackDeg: 0.5,
    });
    await expect(
      getSettingsTyped('framing', FramingSettingsSchema),
    ).resolves.toEqual({ framingPointingFallbackDeg: 0.5 });
  });

  it('sourceViews: drops a cross-drive hardlink, keeps the intra-drive kind', async () => {
    // FR-004a: a hardlink cannot cross a volume, so this value is invalid.
    respond({
      sourceViewLinkKindCrossDrive: 'hardlink',
      sourceViewLinkKindIntraDrive: 'junction',
    });
    await expect(
      getSettingsTyped('sourceViews', SourceViewsSettingsSchema),
    ).resolves.toEqual({ sourceViewLinkKindIntraDrive: 'junction' });
  });

  it('naming: drops a malformed pattern element list, keeps the sibling', async () => {
    respond({ pattern: ['target', 'filter'], autoApplyPattern: false });
    await expect(
      getSettingsTyped('naming', NamingSettingsSchema),
    ).resolves.toEqual({ autoApplyPattern: false });
  });

  it('naming: accepts a well-formed pattern', async () => {
    const pattern = [{ id: 'a', kind: 'token', value: 'target' }];
    respond({ pattern });
    await expect(
      getSettingsTyped('naming', NamingSettingsSchema),
    ).resolves.toEqual({ pattern });
  });
});
