// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for Table — replaces .pv-table* in primitives.css.
 * Single consumer: src/ui/Table.tsx.
 */

import { globalStyle, style } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

export const root = style({
  width: '100%',
  borderCollapse: 'collapse',
});

// th and td rules reference the class — use globalStyle for descendant selectors.
globalStyle(`${root} th`, {
  textAlign: 'left',
  fontSize: uvars.textXs,
  fontWeight: uvars.weightMedium,
  color: vars.textMuted,
  textTransform: 'uppercase',
  letterSpacing: uvars.trackingNormal,
  padding: `${uvars.sp2} ${uvars.sp3}`,
  borderBottom: `1px solid ${vars.border}`,
  background: vars.surface,
  whiteSpace: 'nowrap',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
});

globalStyle(`${root} td`, {
  padding: `${uvars.sp2} ${uvars.sp3}`,
  borderBottom: `1px solid ${vars.borderSubtle}`,
  fontSize: uvars.textSm,
  verticalAlign: 'middle',
  whiteSpace: 'nowrap',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
});

globalStyle(`${root} td.mono`, {
  fontFamily: vars.fontMono,
  fontSize: uvars.textXs,
});

globalStyle(`${root} td.num`, {
  fontVariantNumeric: 'tabular-nums',
  textAlign: 'right',
});

globalStyle(`${root} td.muted`, { color: vars.textMuted });

// Group-header rows: exempt first cell from clip so label spans empty columns.
globalStyle(`${root} tr[class*="__group"] > td`, { overflow: 'visible' });

// Group-header cell button: reset native chrome.
globalStyle(`${root} [class*="__group-cell"]`, {
  display: 'inline-flex',
  alignItems: 'center',
  gap: uvars.sp2,
  background: 'none',
  border: 'none',
  padding: 0,
  cursor: 'pointer',
  textAlign: 'left',
  font: 'inherit',
  fontSize: uvars.textXs,
  textTransform: 'uppercase',
  letterSpacing: uvars.trackingNormal,
  fontWeight: uvars.weightSemibold,
  color: vars.textMuted,
});

export const rowClickable = style({
  cursor: 'pointer',
  selectors: {
    '&:focus-visible': {
      outline: `2px solid ${vars.accent}`,
      outlineOffset: '-2px',
    },
  },
});

// Per-row indent via CSS custom property set on <tr>.
export const rowIndented = style({});
globalStyle(`${rowIndented} > td:first-child > :first-child`, {
  paddingLeft: 'var(--pv-row-indent, 0)',
});

// Cell helpers.
export const cellInline = style({
  display: 'inline-flex',
  alignItems: 'center',
  gap: uvars.sp2,
  flexWrap: 'wrap',
});

// Virtualized scroll container.
export const scroll = style({
  minHeight: 0,
  overflow: 'auto',
});

globalStyle(`${scroll} thead th`, {
  position: 'sticky',
  top: 0,
  zIndex: 1,
});

export const spacerRow = style({});
globalStyle(`${spacerRow} td`, { padding: 0, border: 0 });
