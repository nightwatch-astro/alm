// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// EmptyState.tsx styles.

import { style } from '@vanilla-extract/css';
import { vars } from '@/styles/themes.css';

export const empty = style({
  display: 'flex',
  flexDirection: 'column',
  alignItems: 'center',
  justifyContent: 'center',
  height: '100%',
  color: vars.textMuted,
  gap: 'var(--pv-sp-2)',
  padding: 'var(--pv-sp-7)',
});

export const title = style({
  fontSize: 'var(--pv-text-md)',
  fontWeight: 'var(--pv-weight-medium)',
  color: vars.textSecondary,
});

export const desc = style({
  fontSize: 'var(--pv-text-sm)',
  textAlign: 'center',
  maxWidth: '300px',
});
