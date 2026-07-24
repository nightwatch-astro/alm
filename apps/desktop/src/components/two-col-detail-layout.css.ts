// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for TwoColDetailLayout — migrates pv-session-detail2-*
 * from sessions.css (221L). Also used by SessionDetail, InboxDetail,
 * MasterDetail (all share the same layout structure).
 */

import { style } from '@vanilla-extract/css';
import { vars } from '@/styles/themes.css';

export const wrapper = style({
  display: 'flex',
  flexWrap: 'wrap',
  alignItems: 'flex-start',
  justifyContent: 'flex-start',
  gap: 'var(--pv-sp-6)',
  padding: 'var(--pv-sp-4)',
});

export const col = style({
  flex: '0 1 400px',
  minWidth: '340px',
});

export const linked = style({
  flex: '0 0 auto',
  minWidth: '160px',
});

export const head = style({
  fontSize: 'var(--pv-text-xs)',
  textTransform: 'uppercase',
  letterSpacing: 'var(--pv-tracking-normal)',
  fontWeight: 'var(--pv-weight-semibold)',
  color: vars.textMuted,
  marginBottom: 'var(--pv-sp-2)',
});

export const linkedList = style({
  display: 'flex',
  flexDirection: 'column',
  gap: 'var(--pv-sp-2)',
});

export const muted = style({ color: vars.textMuted });

export const calibRow = style({
  display: 'flex',
  alignItems: 'center',
  gap: 'var(--pv-sp-2)',
});

export const calibNote = style({ color: vars.textFaint });

export const actions = style({
  display: 'inline-flex',
  flexWrap: 'wrap',
  alignItems: 'center',
  gap: 'var(--pv-sp-2)',
  marginLeft: 'var(--pv-sp-3)',
});

export const link = style({
  background: 'none',
  border: 'none',
  padding: 0,
  font: 'inherit',
  textAlign: 'left',
  cursor: 'pointer',
  color: vars.accent,
  selectors: {
    '&:hover': { textDecoration: 'underline' },
  },
});

export const linkedStack = style({
  display: 'flex',
  flexDirection: 'column',
  gap: 'var(--pv-sp-4)',
});

export const match = style({
  padding: '0 var(--pv-sp-4) var(--pv-sp-4)',
});
