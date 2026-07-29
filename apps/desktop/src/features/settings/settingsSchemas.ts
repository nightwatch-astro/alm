// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Per-scope Zod schemas for the Settings IPC surface (C-5).
 *
 * Each schema covers the keys a settings pane reads via `getSettings`. Using
 * `.partial()` ensures fields are optional — the DB may return a subset,
 * e.g. on first run before any value has been persisted. Unknown keys are
 * stripped (`.strip()` is Zod's default).
 *
 * Callers use `getSettingsTyped(scope, schema)` from `./settingsIpc` to get
 * a typed result instead of `Record<string, unknown>` with manual guards.
 */

import { z } from 'zod';

// ── 'advanced' scope ──────────────────────────────────────────────────────────

export const LOG_LEVELS = ['error', 'warn', 'info', 'debug'] as const;

export const AdvancedSettingsSchema = z
  .object({
    logLevel: z.enum(LOG_LEVELS),
    rememberFollowLogs: z.boolean(),
    devMode: z.boolean(),
  })
  .partial();

export type AdvancedSettings = z.infer<typeof AdvancedSettingsSchema>;

// ── 'cleanup' scope ───────────────────────────────────────────────────────────

export const DEFAULT_PROTECTION_LEVELS = [
  'protected',
  'standard',
  'unprotected',
] as const;

export const CleanupSettingsSchema = z
  .object({
    blockPermanentDelete: z.boolean(),
    defaultProtection: z.enum(DEFAULT_PROTECTION_LEVELS),
  })
  .partial();

export type CleanupSettings = z.infer<typeof CleanupSettingsSchema>;

// ── 'framing' scope ───────────────────────────────────────────────────────────

export const FramingSettingsSchema = z
  .object({
    framingPointingFractionOfFov: z.number(),
    framingPointingFallbackDeg: z.number(),
    framingRotationToleranceDeg: z.number(),
    framingMosaicEnvelopeFractionOfFov: z.number(),
  })
  .partial();

export type FramingSettings = z.infer<typeof FramingSettingsSchema>;

// ── 'sourceViews' scope ───────────────────────────────────────────────────────

export const LINK_KINDS = ['symlink', 'hardlink', 'none'] as const;

export const SourceViewsSettingsSchema = z
  .object({
    sourceViewLinkKindIntraDrive: z.enum(LINK_KINDS),
    sourceViewLinkKindCrossDrive: z.enum(LINK_KINDS),
  })
  .partial();

export type SourceViewsSettings = z.infer<typeof SourceViewsSettingsSchema>;

// ── 'naming' scope ────────────────────────────────────────────────────────────

export const NamingSettingsSchema = z
  .object({
    pattern: z.array(z.unknown()),
    autoApplyPattern: z.boolean(),
  })
  .partial();

export type NamingSettings = z.infer<typeof NamingSettingsSchema>;
