// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for TargetSearch — replaces .pv-target-search*
 * in target-search.css. Single consumer: TargetSearch.tsx.
 */

import { style } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

export const root = style({ position: 'relative' });

export const labelSr = style({
  position: 'absolute',
  width: '1px',
  height: '1px',
  padding: 0,
  margin: '-1px',
  overflow: 'hidden',
  clip: 'rect(0, 0, 0, 0)',
  whiteSpace: 'nowrap',
  border: 0,
});

// Positioner: rendered by base-ui Combobox.Positioner in a portal on <body>.
// Must out-rank Modal's z-index (501) — sits at 510.
export const positioner = style({
  zIndex: 510,
  width: 'var(--anchor-width, auto)',
});

export const popup = style({
  background: vars.surfaceRaised,
  border: `1px solid ${vars.border}`,
  borderRadius: uvars.radiusMd,
  boxShadow: vars.shadowSm,
  outline: 'none',
});

export const list = style({
  listStyle: 'none',
  margin: 0,
  padding: uvars.sp1,
  maxHeight: '320px',
  overflowY: 'auto',
});

export const status = style({
  padding: `${uvars.sp2} ${uvars.sp3}`,
  fontSize: uvars.textXs,
  color: vars.textMuted,
});

export const statusResolving = style({
  display: 'flex',
  alignItems: 'center',
  gap: uvars.sp2,
  color: vars.textFaint,
  borderTop: `1px solid ${vars.borderSubtle}`,
});

export const noMatch = style({
  display: 'flex',
  alignItems: 'center',
  gap: uvars.sp2,
});

export const option = style({
  display: 'flex',
  alignItems: 'center',
  gap: uvars.sp2,
  padding: `${uvars.sp2} ${uvars.sp3}`,
  borderRadius: uvars.radiusSm,
  cursor: 'pointer',
  selectors: { '&[data-highlighted]': { background: vars.selectedBg } },
});

export const primary = style({
  fontSize: uvars.textSm,
  fontWeight: uvars.weightSemibold,
  color: vars.text,
});

export const secondary = style({
  fontSize: uvars.textXs,
  color: vars.textMuted,
});

export const badges = style({
  marginLeft: 'auto',
  display: 'flex',
  alignItems: 'center',
  gap: uvars.sp1,
  flexShrink: 0,
});

export const filters = style({
  display: 'flex',
  gap: uvars.sp3,
  marginTop: uvars.sp2,
  flexWrap: 'wrap',
});

export const filterLabel = style({
  display: 'flex',
  flexDirection: 'column',
  gap: uvars.sp1,
  fontSize: uvars.textXs,
  color: vars.textMuted,
});

export const filterSelect = style({ minWidth: '160px' });

export const override = style({
  border: `1px solid ${vars.border}`,
  background: vars.surface,
  color: vars.textSecondary,
  fontSize: uvars.textXs,
  padding: `0 ${uvars.sp1}`,
  borderRadius: uvars.radiusSm,
  cursor: 'pointer',
  selectors: {
    '&:hover:not(:disabled)': { background: vars.hoverBg, color: vars.text },
    '&:disabled': { opacity: 0.5, cursor: 'default' },
  },
});
