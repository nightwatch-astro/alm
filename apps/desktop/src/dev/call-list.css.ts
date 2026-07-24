// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// CallList.tsx styles.

import { style, styleVariants } from '@vanilla-extract/css';
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
export const tdId = style({
  padding: 'var(--pv-sp-1) var(--pv-sp-2)',
  fontFamily: vars.fontMono,
  color: vars.textMuted,
  fontSize: 'var(--pv-text-xs)',
});
export const tdContract = style({
  padding: 'var(--pv-sp-1) var(--pv-sp-2)',
  fontFamily: vars.fontMono,
});
export const tdStarted = style({
  padding: 'var(--pv-sp-1) var(--pv-sp-2)',
  color: vars.textMuted,
});
export const tdActions = style({
  padding: 'var(--pv-sp-1) var(--pv-sp-2)',
  display: 'flex',
  gap: 'var(--pv-sp-1)',
});
export const truncated = style({
  marginLeft: 'var(--pv-sp-1)',
  color: vars.warn,
});
export const outcomeVariants = styleVariants({
  error: { color: vars.danger },
  ok: { color: vars.ok },
});
export const replayBtnVariants = styleVariants({
  safe: { opacity: 1, cursor: 'pointer' },
  unsafe: { opacity: 0.4, cursor: 'not-allowed' },
});
