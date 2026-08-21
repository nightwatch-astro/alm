// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * The single compile-time developer-tools gate (spec 021 FR-031, SC-009).
 *
 * `vite.config.ts` replaces `import.meta.env.VITE_DEV_TOOLS` with a string
 * literal at transform time, so this comparison folds to a constant and every
 * `DEV_TOOLS_ENABLED ? … : …` branch below it is dropped from a release bundle
 * rather than shipped and skipped at runtime.
 *
 * Every developer-surface gate reads this constant. A second copy would let one
 * gate flip while another stayed open, which is how the command palette shipped
 * a `/dev/contracts` entry into release bundles whose route the router had
 * already compiled out.
 */
export const DEV_TOOLS_ENABLED = import.meta.env.VITE_DEV_TOOLS === 'true';
