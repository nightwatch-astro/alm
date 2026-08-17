// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// ContractList.tsx styles.

import { style } from '@vanilla-extract/css';
import { vars } from '@/styles/themes.css';

export const empty = style({
  fontSize: 'var(--pv-text-sm)',
  color: vars.textMuted,
});
export const table = style({
  width: '100%',
  borderCollapse: 'collapse',
  fontSize: 'var(--pv-text-xs)',
});
export const theadRow = style({
  borderBottom: `1px solid ${vars.border}`,
  textAlign: 'left',
});
export const th = style({ padding: 'var(--pv-sp-1) var(--pv-sp-2)' });
export const row = style({ borderBottom: `1px solid ${vars.borderSubtle}` });
export const td = style({ padding: 'var(--pv-sp-1) var(--pv-sp-2)' });
export const tdName = style({
  padding: 'var(--pv-sp-1) var(--pv-sp-2)',
  fontFamily: vars.fontMono,
});
export const tdMuted = style({
  padding: 'var(--pv-sp-1) var(--pv-sp-2)',
  color: vars.textMuted,
});
export const tdCenter = style({
  padding: 'var(--pv-sp-1) var(--pv-sp-2)',
  textAlign: 'center',
});
export const tdSchema = style({
  padding: 'var(--pv-sp-1) var(--pv-sp-2)',
  fontFamily: vars.fontMono,
  maxWidth: '280px',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
  color: vars.textMuted,
});
export const mismatchIcon = style({
  color: vars.warn,
  marginRight: 'var(--pv-sp-1)',
});
export const replayOk = style({ color: vars.ok });
export const replayNa = style({ color: vars.textMuted });
