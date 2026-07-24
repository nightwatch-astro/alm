// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// DetailHeader.tsx styles.

import { style } from '@vanilla-extract/css';
import { vars } from '@/styles/themes.css';

export const header = style({
  display: 'flex',
  alignItems: 'flex-start',
  gap: 'var(--pv-sp-3)',
  marginBottom: 'var(--pv-sp-4)',
  paddingBottom: 'var(--pv-sp-3)',
  borderBottom: `1px solid ${vars.borderSubtle}`,
  flexWrap: 'wrap',
});

export const content = style({ flex: 1, minWidth: '200px' });

export const title = style({
  fontSize: 'var(--pv-text-lg)',
  fontWeight: 'var(--pv-weight-semibold)',
  display: 'flex',
  alignItems: 'center',
  gap: 'var(--pv-sp-2)',
  lineHeight: 'var(--pv-leading-tight)',
  flexWrap: 'wrap',
});

export const subtitle = style({
  fontFamily: vars.fontMono,
  fontSize: 'var(--pv-text-xs)',
  color: vars.textMuted,
  marginTop: 'var(--pv-sp-1)',
  wordBreak: 'break-all',
});

export const actions = style({
  display: 'flex',
  gap: 'var(--pv-sp-2)',
  flexShrink: 0,
  flexWrap: 'wrap',
});

export const stats = style({
  display: 'flex',
  gap: 'var(--pv-sp-4)',
  marginBottom: 'var(--pv-sp-4)',
  flexWrap: 'wrap',
});

export const stat = style({
  display: 'flex',
  flexDirection: 'column',
  gap: '1px',
});

export const statValue = style({
  fontSize: 'var(--pv-text-md)',
  fontWeight: 'var(--pv-weight-semibold)',
  fontVariantNumeric: 'tabular-nums',
});

export const statLabel = style({
  fontSize: 'var(--pv-text-xs)',
  color: vars.textMuted,
  textTransform: 'uppercase',
  letterSpacing: 'var(--pv-tracking-normal)',
  fontWeight: 'var(--pv-weight-medium)',
});
