// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * vanilla-extract styles for the LogPanel component.
 * Replaces the BEM classes from logpanel.css.
 */

import { style, styleVariants } from '@vanilla-extract/css';
import { vars } from '@/styles/themes.css';

// ── Layout ────────────────────────────────────────────────────────────────────

export const logpanel = style({
  display: 'flex',
  flexDirection: 'column',
  flex: '0 0 40%',
  minHeight: '180px',
  borderTop: `1px solid ${vars.border}`,
  background: vars.surface,
});

export const header = style({
  display: 'flex',
  flexWrap: 'wrap',
  alignItems: 'center',
  gap: 'var(--pv-sp-2)',
  padding: 'var(--pv-sp-2) var(--pv-sp-3)',
  borderBottom: `1px solid ${vars.borderSubtle}`,
  flexShrink: 0,
});

export const title = style({
  fontSize: 'var(--pv-text-xs)',
  fontWeight: 'var(--pv-weight-semibold)',
  textTransform: 'uppercase',
  letterSpacing: 'var(--pv-tracking-normal)',
  color: vars.text,
});

export const filters = style({
  display: 'flex',
  flexWrap: 'wrap',
  alignItems: 'center',
  gap: 'var(--pv-sp-1)',
  minWidth: 0,
});

export const filtersSources = style({
  display: 'flex',
  flexWrap: 'wrap',
  alignItems: 'center',
  gap: 'var(--pv-sp-1)',
  minWidth: 0,
  flex: '1 0 100%',
});

export const actions = style({
  display: 'flex',
  alignItems: 'center',
  gap: 'var(--pv-sp-1)',
  marginLeft: 'auto',
});

// ── Chip variants ─────────────────────────────────────────────────────────────

export const chipActive = style({
  background: vars.accentBg,
  color: vars.accentText,
  borderColor: vars.accent,
});

// ── Export error ──────────────────────────────────────────────────────────────

export const exportError = style({
  padding: 'var(--pv-sp-2) var(--pv-sp-3)',
  fontSize: 'var(--pv-text-xs)',
  color: vars.danger,
  background: vars.dangerBg,
  borderBottom: `1px solid ${vars.dangerBorder}`,
  flexShrink: 0,
});

// ── Body + events ─────────────────────────────────────────────────────────────

export const body = style({
  display: 'flex',
  flexDirection: 'column',
  flex: 1,
  minHeight: 0,
});

export const truncationMarker = style({
  padding: 'var(--pv-sp-1) var(--pv-sp-3)',
  fontSize: 'var(--pv-text-xs)',
  color: vars.warn,
  background: vars.warnBg,
  borderBottom: `1px solid ${vars.warnBorder}`,
  flexShrink: 0,
});

export const events = style({
  listStyle: 'none',
  padding: 0,
  margin: 0,
  flex: 1,
  minHeight: 0,
  overflowY: 'auto',
});

export const empty = style({
  display: 'flex',
  height: '100%',
  minHeight: 0,
});

// ── Event row ─────────────────────────────────────────────────────────────────

export const event = style({
  display: 'flex',
  alignItems: 'center',
  gap: 'var(--pv-sp-2)',
  padding: 'var(--pv-sp-1) var(--pv-sp-3)',
  fontSize: 'var(--pv-text-xs)',
  borderBottom: `1px solid ${vars.borderSubtle}`,
  selectors: {
    '&:last-child': { borderBottom: 'none' },
  },
});

export const eventLink = style({
  cursor: 'pointer',
  selectors: {
    '&:hover': { background: vars.hoverBg },
    '&:focus-visible': {
      outline: 'none',
      boxShadow: vars.focusRing,
    },
  },
});

export const eventTime = style({
  color: vars.textMuted,
  minWidth: '64px',
  flexShrink: 0,
});

// Level severity colors via styleVariants.
const levelBase = {
  fontSize: 'var(--pv-text-xs)',
  fontWeight: 'var(--pv-weight-medium)',
  textTransform: 'uppercase' as const,
  minWidth: '40px',
  flexShrink: 0,
};

export const levelVariants = styleVariants({
  info: { ...levelBase, color: vars.info },
  warn: { ...levelBase, color: vars.warn },
  error: { ...levelBase, color: vars.danger },
  debug: { ...levelBase, color: vars.textMuted },
});

export const eventSource = style({
  color: vars.textMuted,
  background: vars.chip,
  borderRadius: 'var(--pv-radius-sm)',
  padding: '0 var(--pv-sp-1)',
  minWidth: '64px',
  flexShrink: 0,
  textAlign: 'center',
});

export const eventContext = style({
  color: vars.textFaint,
  maxWidth: '200px',
  flexShrink: 0,
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
});

export const eventMsg = style({
  color: vars.text,
  minWidth: 0,
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
});

export const eventLinkIndicator = style({
  marginLeft: 'auto',
  color: vars.textFaint,
  flexShrink: 0,
  selectors: {
    [`${eventLink}:hover &`]: { color: vars.accent },
  },
});
