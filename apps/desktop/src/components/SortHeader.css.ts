// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for SortHeader — replaces .pv-sorth* in tables-lists.css.
 * Single consumer: src/components/SortHeader.tsx.
 */

import { style } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

export const root = style({
  display: 'inline-flex',
  alignItems: 'center',
  gap: uvars.sp1,
  background: 'none',
  border: 'none',
  padding: 0,
  margin: 0,
  font: 'inherit',
  color: 'inherit',
  textTransform: 'inherit',
  letterSpacing: 'inherit',
  cursor: 'pointer',
  selectors: { '&:hover': { color: vars.text } },
});

export const active = style({
  color: vars.text,
});

export const arrow = style({
  fontSize: uvars.textXs,
  color: vars.textMuted,
});
