// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for Advanced settings — migrates pv-adv-settings-*
 * from dev.css. These layout helpers are not dev-tools-gated; they live in
 * Advanced settings (confirm / restore controls).
 */

import { style } from '@vanilla-extract/css';

/** Flex column for a description + action pair in a settings row. */
export const controlCol = style({
  display: 'flex',
  flexDirection: 'column',
  gap: 'var(--pv-sp-2)',
  alignItems: 'flex-start',
});

/** Flex row for a confirm/cancel action pair in a settings row. */
export const controlRow = style({
  display: 'flex',
  flexDirection: 'row',
  gap: 'var(--pv-sp-2)',
  alignItems: 'center',
});
