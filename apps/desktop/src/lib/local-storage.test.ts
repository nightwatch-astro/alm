// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { z } from 'zod';
import { readLocalStorage, writeLocalStorage } from './local-storage';

const KEY = 'test.readLocalStorage';

beforeEach(() => {
  localStorage.removeItem(KEY);
});

describe('readLocalStorage', () => {
  it('returns fallback when key is absent', () => {
    const result = readLocalStorage(KEY, z.string(), 'default');
    expect(result).toBe('default');
  });

  it('returns the parsed value when key is present and valid', () => {
    localStorage.setItem(KEY, JSON.stringify('hello'));
    const result = readLocalStorage(KEY, z.string(), 'default');
    expect(result).toBe('hello');
  });

  it('returns fallback when JSON is corrupted', () => {
    localStorage.setItem(KEY, 'not-valid-json{');
    const result = readLocalStorage(KEY, z.string(), 'default');
    expect(result).toBe('default');
  });

  it('returns fallback when the stored shape fails schema validation', () => {
    // Stale storage: stored a number, schema expects string.
    localStorage.setItem(KEY, JSON.stringify(42));
    const result = readLocalStorage(KEY, z.string(), 'default');
    expect(result).toBe('default');
  });

  it('works with object schemas', () => {
    const schema = z.object({ count: z.number() });
    localStorage.setItem(KEY, JSON.stringify({ count: 7 }));
    const result = readLocalStorage(KEY, schema, { count: 0 });
    expect(result).toEqual({ count: 7 });
  });

  it('returns fallback for partial object when schema requires all fields', () => {
    const schema = z.object({ a: z.string(), b: z.number() });
    localStorage.setItem(KEY, JSON.stringify({ a: 'x' })); // missing b
    const result = readLocalStorage(KEY, schema, { a: '', b: 0 });
    expect(result).toEqual({ a: '', b: 0 });
  });
});

describe('writeLocalStorage', () => {
  it('writes a JSON value that readLocalStorage can read back', () => {
    writeLocalStorage(KEY, { x: 1 });
    const result = readLocalStorage(KEY, z.object({ x: z.number() }), { x: 0 });
    expect(result).toEqual({ x: 1 });
  });

  // These assert on the storage CALLS rather than on a subsequent read: some
  // storage backends (Node's built-in localStorage) already reject a
  // non-string, so a read-back assertion passes either way and would not
  // catch a regression. What must hold is that `setItem` is never handed a
  // non-JSON value, because a backend that coerces it stores the literal
  // string "undefined", which `readLocalStorage` then reports as corrupt
  // rather than absent.
  describe('values JSON cannot represent', () => {
    // Install a recording stub rather than spying on the ambient
    // `localStorage`: which object that is varies by environment (jsdom's
    // Storage, Node's experimental global, or vitest.setup.ts's Map-backed
    // replacement), and a spy attached to the wrong one records nothing —
    // which made an earlier version of these tests pass locally and fail in
    // CI. Replacing the global makes the assertion environment-independent.
    const setItem = vi.fn<(key: string, value: string) => void>();
    const removeItem = vi.fn<(key: string) => void>();
    let original: Storage;

    beforeEach(() => {
      setItem.mockClear();
      removeItem.mockClear();
      original = globalThis.localStorage;
      Object.defineProperty(globalThis, 'localStorage', {
        value: { ...original, setItem, removeItem },
        writable: true,
        configurable: true,
      });
    });

    afterEach(() => {
      Object.defineProperty(globalThis, 'localStorage', {
        value: original,
        writable: true,
        configurable: true,
      });
    });

    it('removes the key for undefined instead of calling setItem', () => {
      writeLocalStorage(KEY, undefined);
      expect(setItem).not.toHaveBeenCalled();
      expect(removeItem).toHaveBeenCalledWith(KEY);
    });

    it('removes the key for a function instead of calling setItem', () => {
      writeLocalStorage(KEY, () => 'not serializable');
      expect(setItem).not.toHaveBeenCalled();
      expect(removeItem).toHaveBeenCalledWith(KEY);
    });

    it('still persists falsy values that JSON can represent', () => {
      writeLocalStorage(KEY, null);
      writeLocalStorage(KEY, 0);
      writeLocalStorage(KEY, false);
      expect(setItem.mock.calls).toEqual([
        [KEY, 'null'],
        [KEY, '0'],
        [KEY, 'false'],
      ]);
      expect(removeItem).not.toHaveBeenCalled();
    });
  });
});
