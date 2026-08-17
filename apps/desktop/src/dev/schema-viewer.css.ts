// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// SchemaViewer.tsx styles.

import { style } from '@vanilla-extract/css';
import { vars } from '@/styles/themes.css';

export const overlay = style({
  position: 'fixed',
  inset: 0,
  background: `color-mix(in srgb, ${vars.ink} 50%, transparent)`,
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  zIndex: 1000,
});
export const panel = style({
  background: vars.surface,
  border: `1px solid ${vars.border}`,
  borderRadius: 'var(--pv-radius-md)',
  width: '80vw',
  maxWidth: '900px',
  maxHeight: '80vh',
  display: 'flex',
  flexDirection: 'column',
  padding: 'var(--pv-sp-4)',
  gap: 'var(--pv-sp-3)',
});
export const header = style({
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
});
export const name = style({
  fontFamily: vars.fontMono,
  fontWeight: 'var(--pv-weight-semibold)',
});
export const ver = style({
  color: vars.textMuted,
  marginLeft: 'var(--pv-sp-2)',
  fontSize: 'var(--pv-text-xs)',
});
export const actions = style({ display: 'flex', gap: 'var(--pv-sp-2)' });
export const body = style({ flex: 1, overflow: 'auto', minHeight: 0 });
export const missing = style({
  color: vars.danger,
  padding: 'var(--pv-sp-4)',
  fontSize: 'var(--pv-text-sm)',
});
export const missingPath = style({ marginTop: 'var(--pv-sp-1)' });
export const missingCode = style({
  fontFamily: vars.fontMono,
  fontSize: '0.8em',
});
export const loading = style({
  color: vars.textMuted,
  padding: 'var(--pv-sp-4)',
});
export const pre = style({
  margin: 0,
  padding: 'var(--pv-sp-2)',
  fontSize: 'var(--pv-text-xs)',
  lineHeight: 'var(--pv-leading-normal)',
  whiteSpace: 'pre-wrap',
  wordBreak: 'break-all',
  background: vars.bg3,
  borderRadius: 'var(--pv-radius-sm)',
});
