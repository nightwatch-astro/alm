// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for ProjectsTable — replaces .pv-projects-table*
 * in projects.css. Single consumer: src/features/projects/ProjectsTable.tsx.
 */

import { globalStyle, style } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

export const table = style({ tableLayout: 'auto' });

globalStyle(`${table} th`, { userSelect: 'none' });

globalStyle(`${table} td, ${table} th`, {
  padding: `${uvars.sp1} ${uvars.sp3}`,
});

export const row = style({ cursor: 'pointer' });

globalStyle(`${row}:hover td`, { background: vars.surface });

export const rowSelected = style({});

globalStyle(`${rowSelected} td`, { background: vars.accentBg });

export const name = style({
  display: 'inline-flex',
  alignItems: 'center',
  gap: uvars.sp1,
  fontWeight: uvars.weightMedium,
  color: vars.text,
});

export const blockedIcon = style({
  color: vars.danger,
  flexShrink: 0,
});

export const driftBadge = style({
  display: 'inline-flex',
  alignItems: 'center',
  gap: uvars.sp0,
  marginLeft: uvars.sp1,
  fontSize: uvars.textXs,
  fontWeight: uvars.weightNormal,
  color: vars.warn,
});

export const cellMuted = style({ color: vars.textMuted });

export const cellNum = style({
  fontVariantNumeric: 'tabular-nums',
  textAlign: 'right',
});

export const cellMono = style({
  fontFamily: vars.fontMono,
  fontSize: uvars.textXs,
  color: vars.textSecondary,
});

export const dash = style({ color: vars.textMuted });

export const empty = style({
  padding: uvars.sp5,
  textAlign: 'center',
  color: vars.textMuted,
  fontSize: uvars.textSm,
});
