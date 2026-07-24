// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// TopActionBar.tsx styles.

import { style } from '@vanilla-extract/css';
import { vars } from '@/styles/themes.css';

export const actionBar = style({
  height: 'var(--pv-toolbar-height)',
  borderBottom: `1px solid ${vars.border}`,
  display: 'flex',
  alignItems: 'center',
  padding: '0 var(--pv-sp-4)',
  gap: 'var(--pv-sp-3)',
  flexShrink: 0,
  background: vars.bg,
});

export const title = style({
  fontSize: 'var(--pv-text-md)',
  fontWeight: 'var(--pv-weight-semibold)',
});
export const subtitle = style({
  fontSize: 'var(--pv-text-xs)',
  color: vars.textMuted,
});
export const spacer = style({ flex: 1 });
export const actions = style({
  display: 'flex',
  gap: 'var(--pv-sp-2)',
  alignItems: 'center',
});
