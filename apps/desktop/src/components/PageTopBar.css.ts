// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for PageTopBar — replaces .pv-topbar* in tables-lists.css.
 * Single consumer: src/components/PageTopBar.tsx.
 */

import { style } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

export const root = style({
  display: 'flex',
  alignItems: 'center',
  gap: uvars.sp3,
  flexWrap: 'wrap',
  padding: `${uvars.sp2} ${uvars.sp3}`,
  borderBottom: `1px solid ${vars.border}`,
  background: vars.surface,
  // Reserve scrollbar space to prevent layout shift when scrollbar appears.
  scrollbarGutter: 'stable',
});

export const lead = style({
  display: 'flex',
  alignItems: 'baseline',
  gap: uvars.sp2,
  flexShrink: 0,
});

export const title = style({
  display: 'flex',
  alignItems: 'baseline',
});

export const heading = style({
  margin: 0,
  fontSize: uvars.textLg,
  fontWeight: uvars.weightSemibold,
  color: vars.text,
});

export const summary = style({
  fontSize: uvars.textXs,
  color: vars.textMuted,
});

export const filters = style({
  flex: '1 1 auto',
  minWidth: 0,
});

export const actions = style({
  display: 'flex',
  gap: uvars.sp2,
  alignItems: 'center',
  flexShrink: 0,
});
