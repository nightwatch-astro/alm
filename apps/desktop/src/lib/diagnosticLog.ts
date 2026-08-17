// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Forward frontend errors to the Rust rotating diagnostics log (spec 051 US7).
 *
 * @tauri-apps/plugin-log routes through the ambient tracing subscriber
 * (tracing-appender) since the backend installs the plugin with skip_logger().
 * No-op in non-Tauri environments (vitest, browser dev, mock mode).
 *
 * isTauri() is evaluated once at module load so subsequent calls are
 * synchronous guards — consistent with window.ts T008 pattern.
 */

import { isTauri } from '@tauri-apps/api/core';

const IN_TAURI = isTauri();

/** Forward an error-level message to the rotating diagnostics log. */
export function logError(message: string): void {
  if (!IN_TAURI) return;
  void import('@tauri-apps/plugin-log')
    .then(({ error }) => error(message))
    .catch(() => {});
}

/** Forward a warn-level message to the rotating diagnostics log. */
export function logWarn(message: string): void {
  if (!IN_TAURI) return;
  void import('@tauri-apps/plugin-log')
    .then(({ warn }) => warn(message))
    .catch(() => {});
}
