// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for Section — replaces .pv-section* in primitives.css.
 * Single consumer: src/ui/Section.tsx.
 */

import { style } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

export const root = style({
  marginBottom: uvars.sp4,
});

export const header = style({
  display: 'flex',
  alignItems: 'center',
  gap: uvars.sp2,
  paddingBottom: uvars.sp2,
  borderBottom: `1px solid ${vars.borderSubtle}`,
  marginBottom: uvars.sp3,
  cursor: 'pointer',
  userSelect: 'none',
});

export const toggle = style({
  fontSize: uvars.textXs,
  color: vars.textFaint,
  width: '14px',
  textAlign: 'center',
  flexShrink: 0,
});

export const title = style({
  fontSize: uvars.textSm,
  fontWeight: uvars.weightSemibold,
  color: vars.text,
});

export const count = style({
  fontSize: uvars.textXs,
  color: vars.textMuted,
});

export const right = style({
  marginLeft: 'auto',
});
