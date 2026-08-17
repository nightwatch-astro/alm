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

// `wrap` variant (task #81): relaxes the fixed single-row height so the title
// and the action cluster occupy their own rows and cannot overlap the content
// below. Was `.pv-project-detail__action-bar .pv-action-bar*` in projects.css.
export const actionBarWrap = style({
  height: 'auto',
  minHeight: 'var(--pv-toolbar-height)',
  flexWrap: 'wrap',
  rowGap: 'var(--pv-sp-2)',
  paddingTop: 'var(--pv-sp-2)',
  paddingBottom: 'var(--pv-sp-2)',
});

// `flex: 1 1 100%` already forces this onto its own flex line, so the wrap
// variant renders no spacer: an empty full-width spacer would form a third,
// zero-height line and apply `rowGap` twice.
export const actionsWrap = style({
  flex: '1 1 100%',
  justifyContent: 'flex-end',
  flexWrap: 'wrap',
  rowGap: 'var(--pv-sp-1)',
});
