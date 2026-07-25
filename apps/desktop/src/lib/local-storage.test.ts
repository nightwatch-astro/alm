// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { describe, it, expect, beforeEach } from 'vitest';
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
});
