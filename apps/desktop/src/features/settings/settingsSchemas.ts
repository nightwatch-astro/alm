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

/**
 * 2-level model (issue #506): the third `standard`/`normal` level is retired.
 * Matches `crates/domain/core/src/settings.rs` and `Cleanup.tsx`'s
 * `DefaultProtection` — a persisted third-level value is stale and must be
 * dropped rather than applied.
 */
export const DEFAULT_PROTECTION_LEVELS = ['protected', 'unprotected'] as const;

export type DefaultProtectionLevel = (typeof DEFAULT_PROTECTION_LEVELS)[number];

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

/**
 * Per-field link-kind domains from `crates/domain/core/src/settings.rs`
 * (spec 049 FR-004a): cross-drive excludes `hardlink` because a hardlink
 * cannot cross a volume. A persisted `hardlink` on the cross-drive key is
 * therefore invalid, not merely unusual, and must be dropped.
 */
export const INTRA_DRIVE_LINK_KINDS = [
  'hardlink',
  'symlink',
  'junction',
] as const;
export const CROSS_DRIVE_LINK_KINDS = ['symlink', 'junction'] as const;

export const SourceViewsSettingsSchema = z
  .object({
    sourceViewLinkKindIntraDrive: z.enum(INTRA_DRIVE_LINK_KINDS),
    sourceViewLinkKindCrossDrive: z.enum(CROSS_DRIVE_LINK_KINDS),
  })
  .partial();

export type SourceViewsSettings = z.infer<typeof SourceViewsSettingsSchema>;

// ── 'naming' scope ────────────────────────────────────────────────────────────

/**
 * Mirrors `PatternPartDto` in `@/bindings/index` (itself a re-export of
 * `patterns::PatternPart`). `kind` stays a plain string rather than an enum:
 * the backend types it as `String`, and rejecting an unknown kind here would
 * discard a user's saved pattern on any backend token addition.
 */
export const PatternPartSchema = z.object({
  id: z.string(),
  kind: z.string(),
  value: z.string(),
});

export const NamingSettingsSchema = z
  .object({
    pattern: z.array(PatternPartSchema),
    autoApplyPattern: z.boolean(),
  })
  .partial();

export type NamingSettings = z.infer<typeof NamingSettingsSchema>;
