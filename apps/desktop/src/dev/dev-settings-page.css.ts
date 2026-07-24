// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// DevSettingsPage.tsx styles — reuses ContractsPage layout conventions.

import { style } from '@vanilla-extract/css';
import { vars } from '@/styles/themes.css';

export const body = style({
  padding: 'var(--pv-sp-4)',
  display: 'flex',
  flexDirection: 'column',
  gap: 'var(--pv-sp-4)',
});
export const title = style({
  fontSize: 'var(--pv-text-lg)',
  fontWeight: 'var(--pv-weight-semibold)',
});
export const exportResult = style({
  fontSize: 'var(--pv-text-sm)',
  color: vars.textMuted,
});
export const error = style({
  color: vars.danger,
  fontSize: 'var(--pv-text-sm)',
});
export const loading = style({ color: vars.textMuted });
