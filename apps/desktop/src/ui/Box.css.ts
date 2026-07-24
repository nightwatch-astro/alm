// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for Box — replaces .pv-box* in primitives.css.
 * Single consumer: src/ui/Box.tsx.
 */

import { style } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

export const root = style({
  border: `1px solid ${vars.border}`,
  borderRadius: uvars.radiusMd,
  background: vars.bg,
  overflow: 'hidden',
});

export const header = style({
  padding: `${uvars.sp2} ${uvars.sp3}`,
  background: vars.surface,
  borderBottom: `1px solid ${vars.borderSubtle}`,
  fontSize: uvars.textXs,
  fontWeight: uvars.weightSemibold,
  textTransform: 'uppercase',
  letterSpacing: uvars.trackingNormal,
  color: vars.textMuted,
});

export const body = style({
  padding: uvars.sp3,
});
