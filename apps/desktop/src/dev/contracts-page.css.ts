// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// ContractsPage.tsx styles.

import { style } from '@vanilla-extract/css';
import { vars } from '@/styles/themes.css';

export const stubBody = style({
  padding: 'var(--pv-sp-7)',
  textAlign: 'center',
  color: vars.textMuted,
});
export const stubHeading = style({
  fontSize: 'var(--pv-text-lg)',
  marginBottom: 'var(--pv-sp-2)',
});
export const stubText = style({ fontSize: 'var(--pv-text-sm)' });
export const loading = style({
  padding: 'var(--pv-sp-7)',
  color: vars.textMuted,
});
export const pageBody = style({
  padding: 'var(--pv-sp-4)',
  display: 'flex',
  flexDirection: 'column',
  gap: 'var(--pv-sp-4)',
});
export const pageHeader = style({
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  borderBottom: `1px solid ${vars.border}`,
  paddingBottom: 'var(--pv-sp-3)',
});
export const pageTitle = style({
  fontSize: 'var(--pv-text-lg)',
  fontWeight: 'var(--pv-weight-semibold)',
});
export const pageActions = style({ display: 'flex', gap: 'var(--pv-sp-2)' });
export const error = style({
  color: vars.danger,
  fontSize: 'var(--pv-text-sm)',
});
export const exportResult = style({
  fontSize: 'var(--pv-text-sm)',
  color: vars.textMuted,
});
export const sectionHeading = style({
  fontSize: 'var(--pv-text-md)',
  fontWeight: 'var(--pv-weight-semibold)',
  marginBottom: 'var(--pv-sp-2)',
});
