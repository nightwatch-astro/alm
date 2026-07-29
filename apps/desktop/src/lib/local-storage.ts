// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Type-safe localStorage read/write helpers (C-24).
 *
 * Used by every localStorage JSON site in the app: the project wizard draft
 * (`features/projects/wizard/WizardPage.tsx`), the setup wizard state
 * (`features/setup/SetupWizard.tsx`), the setup sources store
 * (`features/setup/sources-store.ts`), and app preferences
 * (`data/preferences.ts`). Each passes a Zod schema, so a corrupted or stale
 * stored value falls back to the default instead of propagating as a
 * silently-wrong shape.
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
