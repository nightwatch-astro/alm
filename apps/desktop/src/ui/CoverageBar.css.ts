// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for CoverageBar — replaces .pv-coverage* in primitives.css.
 * Single consumer: src/ui/CoverageBar.tsx.
 */

import { style, styleVariants } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

export const root = style({
  display: 'flex',
  alignItems: 'center',
  gap: uvars.sp2,
});

export const label = style({
  width: '32px',
  fontSize: uvars.textXs,
  fontWeight: uvars.weightMedium,
  textAlign: 'right',
  flexShrink: 0,
});

export const bar = style({
  flex: 1,
  height: '6px',
  background: vars.rule2,
  borderRadius: '3px',
  overflow: 'hidden',
});

const fillBase = style({
  height: '100%',
  borderRadius: '3px',
  background: vars.accent,
  transition: `width ${uvars.transitionSlow}`,
});

export const fillVariants = styleVariants({
  default: [fillBase],
  low: [fillBase, { background: vars.warn }],
  ok: [fillBase, { background: vars.ok }],
});

export const value = style({
  width: '36px',
  fontSize: uvars.textXs,
  color: vars.textMuted,
  textAlign: 'right',
  flexShrink: 0,
  fontVariantNumeric: 'tabular-nums',
});
