// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// Sidebar.tsx styles.

import { style } from '@vanilla-extract/css';
import { vars } from '@/styles/themes.css';

export const sidebar = style({
  width: 'var(--pv-sidebar-width)',
  background: vars.surface,
  borderRight: `1px solid ${vars.border}`,
  display: 'flex',
  flexDirection: 'column',
  flexShrink: 0,
  transition: 'width var(--pv-transition-slow)',
  overflow: 'hidden',
});

export const sidebarCollapsed = style({ width: 'var(--pv-sidebar-collapsed)' });

export const header = style({
  padding: 'var(--pv-sp-3)',
  display: 'flex',
  alignItems: 'center',
  gap: 'var(--pv-sp-2)',
  minHeight: '44px',
});

export const brand = style({
  display: 'flex',
  flexDirection: 'column',
  minWidth: 0,
  flex: 1,
});

export const brandLabel = style({
  fontSize: 'var(--pv-text-xs)',
  color: vars.textMuted,
  letterSpacing: 'var(--pv-tracking-wide)',
  textTransform: 'uppercase',
  fontWeight: 'var(--pv-weight-medium)',
  whiteSpace: 'nowrap',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
});

export const brandName = style({
  fontSize: 'var(--pv-text-md)',
  fontWeight: 'var(--pv-weight-semibold)',
  whiteSpace: 'nowrap',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  flex: 1,
  flexShrink: 0,
});

export const collapseBtn = style({
  width: '24px',
  height: '24px',
  border: 'none',
  background: 'none',
  cursor: 'pointer',
  color: vars.textMuted,
  borderRadius: 'var(--pv-radius-sm)',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  flexShrink: 0,
  marginLeft: 'auto',
  selectors: { '&:hover': { background: vars.hoverBg } },
});

export const nav = style({
  flex: 1,
  padding: 'var(--pv-sp-1) 0',
  overflowY: 'auto',
});

export const item = style({
  display: 'flex',
  alignItems: 'center',
  gap: 'var(--pv-sp-2)',
  padding: '6px var(--pv-sp-3)',
  margin: '1px var(--pv-sp-1)',
  cursor: 'pointer',
  color: vars.textSecondary,
  borderRadius: 'var(--pv-radius-sm)',
  fontSize: 'var(--pv-text-sm)',
  fontWeight: 'var(--pv-weight-medium)',
  transition:
    'background var(--pv-transition-fast), color var(--pv-transition-fast)',
  whiteSpace: 'nowrap',
  overflow: 'hidden',
  border: 'none',
  background: 'none',
  width: 'calc(100% - var(--pv-sp-1) * 2)',
  textAlign: 'left',
  selectors: {
    '&:hover': { background: vars.hoverBg, color: vars.text },
    '&:focus-visible': { outline: 'none', boxShadow: vars.focusRing },
  },
});

export const itemActive = style({
  background: vars.selectedBg,
  color: vars.accentText,
  fontWeight: 'var(--pv-weight-semibold)',
});

export const itemIcon = style({
  width: '18px',
  height: '18px',
  flexShrink: 0,
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
});

export const itemLabel = style({
  flex: 1,
  overflow: 'hidden',
  textOverflow: 'ellipsis',
});

export const itemBadge = style({
  fontSize: 'var(--pv-text-xs)',
  color: vars.textMuted,
  fontVariantNumeric: 'tabular-nums',
  minWidth: '20px',
  textAlign: 'right',
});

export const itemBadgeAlert = style({
  color: vars.onAccent,
  background: vars.accent,
  borderRadius: '8px',
  padding: '0 5px',
  minWidth: '18px',
  textAlign: 'center',
});

export const footer = style({
  padding: 'var(--pv-sp-2) var(--pv-sp-3)',
  borderTop: `1px solid ${vars.borderSubtle}`,
  fontSize: 'var(--pv-text-xs)',
  color: vars.textFaint,
  whiteSpace: 'nowrap',
  overflow: 'hidden',
});

export const mark = style({
  width: '22px',
  height: '22px',
  borderRadius: 'var(--pv-radius-sm)',
  background: `linear-gradient(135deg, ${vars.accent}, ${vars.accentDeep})`,
  color: vars.onAccent,
  fontSize: 'var(--pv-text-xs)',
  fontWeight: 'var(--pv-weight-semibold)',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  flexShrink: 0,
});

export const version = style({
  fontSize: 'var(--pv-text-xs)',
  color: vars.textFaint,
});
export const group = style({ paddingBottom: 'var(--pv-sp-1)' });

export const groupLabel = style({
  fontSize: 'var(--pv-text-xs)',
  fontWeight: 'var(--pv-weight-semibold)',
  letterSpacing: 'var(--pv-tracking-wide)',
  textTransform: 'uppercase',
  color: vars.textFaint,
  padding: 'var(--pv-sp-3) var(--pv-sp-3) var(--pv-sp-1)',
});

export const settings = style({
  borderTop: `1px solid ${vars.borderSubtle}`,
  padding: 'var(--pv-sp-1) 0',
});
