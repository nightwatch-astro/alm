// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for ProjectLifecycleStepper — replaces .pv-stepper*
 * in projects.css. Single consumer: src/features/projects/ProjectLifecycleStepper.tsx.
 */

import { style, styleVariants } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

export const root = style({
  display: 'flex',
  flexDirection: 'column',
  gap: uvars.sp2,
  padding: uvars.sp3,
  borderBottom: `1px solid ${vars.borderSubtle}`,
});

export const track = style({
  display: 'flex',
  flexWrap: 'wrap',
  alignItems: 'center',
  gap: uvars.sp1,
  margin: 0,
  padding: 0,
  listStyle: 'none',
});

const chipBase = style({
  display: 'inline-flex',
  alignItems: 'center',
  padding: `${uvars.sp0} ${uvars.sp2}`,
  fontSize: uvars.textXs,
  textTransform: 'capitalize',
  color: vars.textMuted,
  background: vars.surface,
  border: `1px solid ${vars.borderSubtle}`,
  borderRadius: uvars.radiusLg,
  whiteSpace: 'nowrap',
});

export const chipVariants = styleVariants({
  default: [chipBase],
  done: [chipBase, { color: vars.textSecondary, borderColor: vars.border }],
  active: [
    chipBase,
    {
      color: vars.accentText,
      background: vars.accentBg,
      borderColor: vars.accent,
      fontWeight: uvars.weightSemibold,
    },
  ],
  blocked: [
    chipBase,
    {
      color: vars.danger,
      background: vars.dangerBg,
      borderColor: vars.danger,
      fontWeight: uvars.weightSemibold,
    },
  ],
});

export const next = style({
  margin: 0,
  fontSize: uvars.textXs,
  color: vars.textMuted,
});

export const history = style({
  display: 'flex',
  flexDirection: 'column',
  gap: uvars.sp0,
});

export const historyRow = style({
  fontSize: uvars.textXs,
  color: vars.textMuted,
});
