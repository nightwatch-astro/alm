// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// ListSidebar.tsx styles.

import { globalStyle, style } from '@vanilla-extract/css';
import { vars } from '@/styles/themes.css';

export const listSidebar = style({
  width: 'var(--pv-list-width)',
  flexShrink: 0,
  display: 'flex',
  flexDirection: 'column',
  borderRight: `1px solid ${vars.border}`,
  background: vars.bg,
  minWidth: '220px',
});

export const search = style({
  padding: 'var(--pv-sp-2) var(--pv-sp-3)',
  borderBottom: `1px solid ${vars.borderSubtle}`,
});

export const controls = style({
  padding: 'var(--pv-sp-2) var(--pv-sp-3)',
  display: 'flex',
  flexDirection: 'column',
  gap: 'var(--pv-sp-1)',
  borderBottom: `1px solid ${vars.borderSubtle}`,
});

// The search input and the caller-supplied controls are unclassed elements
// (ListSidebar.tsx, TargetList.tsx), so both are styled by descendant selector.
globalStyle(`${search} input`, {
  width: '100%',
  height: '28px',
  padding: '0 var(--pv-sp-2)',
  border: `1px solid ${vars.border}`,
  borderRadius: 'var(--pv-radius-sm)',
  fontSize: 'var(--pv-text-xs)',
  background: vars.bg,
  color: vars.text,
  outline: 'none',
  transition: 'border-color var(--pv-transition-fast)',
});

globalStyle(`${search} input:focus`, { borderColor: vars.accent });
globalStyle(`${search} input::placeholder`, { color: vars.textFaint });

globalStyle(`${controls} select`, {
  width: '100%',
  height: 'var(--pv-control-h)',
  fontSize: 'var(--pv-text-xs)',
  border: `1px solid ${vars.border}`,
  borderRadius: 'var(--pv-radius-sm)',
  background: vars.bg,
  color: vars.textSecondary,
  padding: '0 var(--pv-sp-2)',
  cursor: 'pointer',
});

export const list = style({ flex: 1, overflowY: 'auto', position: 'relative' });

export const footer = style({
  padding: 'var(--pv-sp-1) var(--pv-sp-3)',
  borderTop: `1px solid ${vars.borderSubtle}`,
  fontSize: 'var(--pv-text-xs)',
  color: vars.textFaint,
});
