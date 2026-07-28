// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Type-safe localStorage read helper (C-24).
 *
 * All four localStorage JSON-parsing sites (wizard draft, setup wizard state,
 * sources store, preferences) followed the same try/catch + JSON.parse +
 * fallback pattern but without schema validation. This helper consolidates
 * that pattern and adds Zod parse so corrupted / stale storage values fall
 * back to the default instead of propagating as silently-wrong shapes.
 */

import type { ZodType } from 'zod';

/**
 * Read and parse a JSON value from localStorage, validating with a Zod schema.
 *
 * Returns `fallback` when:
 * - the key is absent
 * - JSON.parse throws (corrupted value)
 * - Zod validation fails (stale shape after a schema change)
 * - localStorage is unavailable (SSR / unit-test environments that mock it)
 *
 * @param key      localStorage key
 * @param schema   Zod schema; `.safeParse()` is called so errors never throw
 * @param fallback value returned on miss or validation failure
 */
export function readLocalStorage<T>(
  key: string,
  schema: ZodType<T>,
  fallback: T,
): T {
  try {
    const raw = localStorage.getItem(key);
    if (raw === null) return fallback;
    const parsed: unknown = JSON.parse(raw);
    const result = schema.safeParse(parsed);
    return result.success ? result.data : fallback;
  } catch {
    return fallback;
  }
}

/**
 * Write a JSON value to localStorage. Silently no-ops when storage is full or
 * unavailable (matching the convention across all existing write sites).
 *
 * A value JSON cannot represent (`undefined`, a function, a symbol) makes
 * `JSON.stringify` return `undefined`, which `setItem` would coerce to the
 * literal string `"undefined"` — a value `readLocalStorage` then treats as
 * corrupt rather than absent. Remove the key instead, so a subsequent read
 * reports a clean miss and returns its fallback.
 */
export function writeLocalStorage<T>(key: string, value: T): void {
  try {
    const serialized = JSON.stringify(value);
    if (serialized === undefined) {
      localStorage.removeItem(key);
      return;
    }
    localStorage.setItem(key, serialized);
  } catch {
    // Storage full or unavailable — non-fatal.
  }
}
