// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// StatusBar.tsx styles.

import { style } from '@vanilla-extract/css';
import { vars } from '@/styles/themes.css';

export const statusBar = style({
  height: 'var(--pv-statusbar-height)',
  background: vars.surface,
  borderTop: `1px solid ${vars.border}`,
  display: 'flex',
  alignItems: 'center',
  padding: '0 var(--pv-sp-3)',
  fontSize: 'var(--pv-text-xs)',
  color: vars.textMuted,
  gap: 'var(--pv-sp-4)',
  flexShrink: 0,
});

export const sep = style({ color: vars.rule });
export const right = style({
  marginLeft: 'auto',
  display: 'flex',
  gap: 'var(--pv-sp-3)',
  alignItems: 'center',
});
export const op = style({
  display: 'flex',
  alignItems: 'center',
  gap: 'var(--pv-sp-2)',
});
export const idle = style({ color: vars.textFaint });

export const spinner = style({
  width: '9px',
  height: '9px',
  borderRadius: '50%',
  border: `2px solid ${vars.accent}`,
  borderTopColor: 'transparent',
  animation: 'pv-spin 1s linear infinite',
});

export const vol = style({
  display: 'flex',
  alignItems: 'center',
  gap: 'var(--pv-sp-1)',
});
export const volWarn = style({ color: vars.warn });

export const meter = style({
  width: '40px',
  height: '5px',
  borderRadius: '3px',
  background: vars.bg3,
  overflow: 'hidden',
});

export const logToggle = style({
  border: `1px solid ${vars.border}`,
  borderRadius: 'var(--pv-radius-sm)',
  padding: '0 var(--pv-sp-2)',
  height: '16px',
  background: vars.bg,
  cursor: 'pointer',
  color: vars.textSecondary,
  fontSize: 'var(--pv-text-xs)',
});
