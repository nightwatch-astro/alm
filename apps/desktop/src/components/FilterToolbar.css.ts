// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for FilterToolbar — replaces .pv-filterbar* in tables-lists.css.
 * Single consumer: src/components/FilterToolbar.tsx.
 */

import { style } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

export const root = style({
  display: 'flex',
  alignItems: 'center',
  gap: uvars.sp2,
  flexWrap: 'wrap',
});

export const search = style({
  flex: '1 1 200px',
  minWidth: '160px',
  maxWidth: '320px',
  padding: `${uvars.sp1} ${uvars.sp2}`,
  fontSize: uvars.textSm,
  color: vars.text,
  background: vars.surfaceRaised,
  border: `1px solid ${vars.border}`,
  borderRadius: uvars.radiusSm,
  selectors: {
    '&:focus': { outline: 'none', borderColor: vars.accent },
  },
});

export const field = style({
  display: 'flex',
  alignItems: 'center',
  gap: uvars.sp1,
  fontSize: uvars.textXs,
  color: vars.textMuted,
});

export const fieldLabel = style({ whiteSpace: 'nowrap' });

export const select = style({
  height: uvars.controlH,
  padding: `0 ${uvars.sp2}`,
  fontSize: uvars.textSm,
  color: vars.text,
  background: vars.surfaceRaised,
  border: `1px solid ${vars.border}`,
  borderRadius: uvars.radiusSm,
});

export const group = style({
  display: 'flex',
  alignItems: 'center',
  gap: uvars.sp1,
});

export const sort = style({
  display: 'flex',
  alignItems: 'center',
  gap: uvars.sp1,
});

export const sortDir = style({ lineHeight: 1 });

export const actions = style({
  display: 'flex',
  gap: uvars.sp2,
  alignItems: 'center',
});

// Multi-select popover field (native <details> disclosure with a checkbox menu).
export const multi = style({
  position: 'relative',
  display: 'inline-block',
});

export const multiSummary = style({
  display: 'inline-flex',
  alignItems: 'center',
  gap: uvars.sp1,
  padding: `${uvars.sp1} ${uvars.sp2}`,
  fontSize: uvars.textSm,
  color: vars.text,
  background: vars.surfaceRaised,
  border: `1px solid ${vars.border}`,
  borderRadius: uvars.radiusSm,
  cursor: 'pointer',
  listStyle: 'none',
  userSelect: 'none',
  selectors: {
    '&::-webkit-details-marker': { display: 'none' },
    '&::marker': { content: '""' },
    // Chevron indicator.
    '&::after': {
      content: '"\\25BE"',
      fontSize: uvars.textXs,
      color: vars.textMuted,
    },
  },
});

// When the <details> is open, highlight the summary border.
export const multiOpen = style({});

export const multiMenu = style({
  position: 'absolute',
  zIndex: 50,
  top: 'calc(100% + var(--pv-sp-1, 4px))',
  left: 0,
  minWidth: '11rem',
  display: 'flex',
  flexDirection: 'column',
  gap: uvars.sp1,
  padding: uvars.sp2,
  background: vars.surfaceRaised,
  border: `1px solid ${vars.border}`,
  borderRadius: uvars.radiusSm,
  boxShadow: vars.shadowSm,
});

export const multiOption = style({
  display: 'flex',
  alignItems: 'center',
  gap: uvars.sp2,
  fontSize: uvars.textSm,
  color: vars.text,
  cursor: 'pointer',
  whiteSpace: 'nowrap',
});

export const multiCheck = style({
  accentColor: vars.accent,
  cursor: 'pointer',
});
