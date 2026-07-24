// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for SessionListPopover — migrates pv-session-popover-*
 * from sessions.css.
 */

import { style } from '@vanilla-extract/css';
import { vars } from '@/styles/themes.css';

export const trigger = style({
  display: 'inline-flex',
  alignItems: 'center',
  gap: 'var(--pv-sp-1)',
  padding: '1px var(--pv-sp-2)',
  fontSize: 'var(--pv-text-sm)',
  color: vars.text,
  background: 'none',
  border: `1px solid ${vars.border}`,
  borderRadius: 'var(--pv-radius-sm)',
  cursor: 'pointer',
  lineHeight: 1.5,
  selectors: {
    '&:hover': { background: vars.hoverBg },
    '&:focus-visible': { outline: 'none', boxShadow: vars.focusRing },
  },
});

export const popup = style({
  minWidth: '260px',
  maxWidth: '360px',
  background: vars.surfaceRaised,
  border: `1px solid ${vars.border}`,
  borderRadius: 'var(--pv-radius-md)',
  boxShadow: vars.shadowSm,
  outline: 'none',
  zIndex: 200,
  display: 'flex',
  flexDirection: 'column',
  gap: 0,
});

export const search = style({
  display: 'block',
  width: '100%',
  boxSizing: 'border-box',
  padding: 'var(--pv-sp-2) var(--pv-sp-3)',
  fontSize: 'var(--pv-text-sm)',
  color: vars.text,
  background: 'transparent',
  border: 'none',
  borderBottom: `1px solid ${vars.borderSubtle}`,
  outline: 'none',
  selectors: {
    '&:focus': { borderBottomColor: vars.accent },
  },
});

export const list = style({
  overflowY: 'auto',
  maxHeight: '240px',
  padding: 'var(--pv-sp-1)',
  margin: 0,
  listStyle: 'none',
});

export const item = style({
  padding: '3px var(--pv-sp-2)',
  fontSize: 'var(--pv-text-sm)',
  borderRadius: 'var(--pv-radius-sm)',
  color: vars.text,
  whiteSpace: 'nowrap',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
});

export const empty = style({
  padding: 'var(--pv-sp-2)',
  fontSize: 'var(--pv-text-sm)',
  color: vars.textMuted,
});
