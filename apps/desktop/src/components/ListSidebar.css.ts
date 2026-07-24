// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// ListSidebar.tsx styles.

import { style } from '@vanilla-extract/css';
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

export const list = style({ flex: 1, overflowY: 'auto', position: 'relative' });

export const footer = style({
  padding: 'var(--pv-sp-1) var(--pv-sp-3)',
  borderTop: `1px solid ${vars.borderSubtle}`,
  fontSize: 'var(--pv-text-xs)',
  color: vars.textFaint,
});
