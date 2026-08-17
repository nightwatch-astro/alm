// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for shared group-header rows — replaces .pv-listgroup*
 * in tables-lists.css. Consumers: MastersTable, ProjectsTable, SessionsTable, TargetsTable.
 */

import { globalStyle, style } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

export const row = style({});

globalStyle(`${row} td`, {
  background: vars.surface,
  fontWeight: uvars.weightSemibold,
  fontSize: uvars.textXs,
  textTransform: 'uppercase',
  letterSpacing: uvars.trackingNormal,
  color: vars.textMuted,
  borderBottom: `1px solid ${vars.borderSubtle}`,
  padding: `${uvars.sp2} ${uvars.sp3}`,
});

export const cell = style({
  display: 'inline-flex',
  alignItems: 'center',
  gap: uvars.sp2,
  width: '100%',
  background: 'none',
  border: 'none',
  padding: 0,
  textAlign: 'left',
  font: 'inherit',
  color: 'inherit',
  textTransform: 'inherit',
  letterSpacing: 'inherit',
});

export const caret = style({
  width: '0.7em',
  display: 'inline-block',
  flexShrink: 0,
  color: vars.textMuted,
});

export const label = style({
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
});

export const count = style({
  marginLeft: uvars.sp2,
  fontWeight: uvars.weightNormal,
  textTransform: 'none',
  letterSpacing: 'normal',
  color: vars.textMuted,
  fontSize: uvars.textXs,
});
