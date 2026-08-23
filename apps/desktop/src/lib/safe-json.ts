// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * `JSON.stringify` throws on circular structures and on `BigInt`, and returns
 * `undefined` for `undefined`, functions, and symbols. Diagnostic and
 * error-reporting paths must not fail while describing a value, so they use
 * this instead of calling `JSON.stringify` directly.
 *
 * Returns `null` when the value cannot be serialised to a string.
 */
export function safeStringify(value: unknown): string | null {
  try {
    return JSON.stringify(value) ?? null;
  } catch {
    return null;
  }
}
