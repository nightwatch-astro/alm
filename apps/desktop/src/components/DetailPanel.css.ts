// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for DetailPanel — replaces .pv-detail-panel* +
 * .pv-detailpanel* in detail-panes.css. Consumers: DetailPanel.tsx.
 */

import { style } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

export const root = style({
  display: 'flex',
  flexDirection: 'column',
  minHeight: 0,
});

export const header = style({
  display: 'flex',
  alignItems: 'flex-start',
  gap: uvars.sp3,
  padding: `${uvars.sp3} ${uvars.sp4}`,
  borderBottom: `1px solid ${vars.borderSubtle}`,
  flexWrap: 'wrap',
});

export const titleBlock = style({
  flex: 1,
  minWidth: 0,
  display: 'flex',
  flexDirection: 'column',
  gap: uvars.sp0,
});

export const title = style({
  display: 'flex',
  alignItems: 'center',
  gap: uvars.sp2,
  fontSize: uvars.textMd,
  fontWeight: uvars.weightSemibold,
  lineHeight: uvars.leadingTight,
  color: vars.text,
  minWidth: 0,
});

export const titleExtra = style({
  display: 'flex',
  alignItems: 'center',
  gap: uvars.sp1,
  flexWrap: 'wrap',
  marginTop: uvars.sp0,
});

export const subtitle = style({
  fontSize: uvars.textSm,
  color: vars.textMuted,
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
  marginTop: uvars.sp0,
});

export const actions = style({
  display: 'flex',
  alignItems: 'center',
  gap: uvars.sp2,
  flexShrink: 0,
  flexWrap: 'wrap',
});

export const body = style({
  flex: 1,
  minHeight: 0,
  overflowY: 'auto',
  padding: `${uvars.sp3} ${uvars.sp4}`,
});

// Sessions density tightening.
export const sessionsDensityHeader = style({
  paddingTop: uvars.sp2,
  paddingBottom: uvars.sp2,
});

export const sessionsDensityBody = style({ paddingTop: uvars.sp2 });

// Canonical body scroll region.
export const content = style({
  minWidth: 0,
  flex: '1 1 auto',
  minHeight: 0,
  overflowY: 'auto',
  scrollbarGutter: 'stable',
});
