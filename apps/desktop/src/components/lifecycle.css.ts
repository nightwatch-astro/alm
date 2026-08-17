// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for the Lifecycle flowchart component.
 * Replaces the BEM classes from wizard-base.css (.pv-lifecycle*).
 */

import { style, styleVariants } from '@vanilla-extract/css';
import { vars } from '@/styles/themes.css';

export const root = style({
  display: 'flex',
  flexDirection: 'column',
  gap: 0,
});

export const step = style({
  display: 'flex',
  alignItems: 'center',
  gap: 'var(--pv-sp-2)',
  position: 'relative',
});

export const connector = style({
  display: 'flex',
  flexDirection: 'column',
  alignItems: 'center',
  width: '20px',
  flexShrink: 0,
});

// Base dot — shared geometry for all state variants.
const dotBase = {
  width: '12px',
  height: '12px',
  borderRadius: '50%',
  flexShrink: 0,
  zIndex: 1,
} as const;

export const dotVariants = styleVariants({
  default: {
    ...dotBase,
    border: `2px solid ${vars.rule}`,
    background: vars.bg,
  },
  done: {
    ...dotBase,
    background: vars.ok,
    border: `2px solid ${vars.ok}`,
  },
  active: {
    ...dotBase,
    background: vars.accent,
    border: `2px solid ${vars.accent}`,
    boxShadow: `0 0 0 3px ${vars.accentBg}`,
  },
  blocked: {
    ...dotBase,
    background: vars.danger,
    border: `2px solid ${vars.danger}`,
  },
});

// Base line — shared geometry.
const lineBase = {
  width: '2px',
  height: '20px',
} as const;

export const lineVariants = styleVariants({
  default: { ...lineBase, background: vars.rule2 },
  done: { ...lineBase, background: vars.ok },
});

// Base label — shared typography.
const labelBase = {
  fontSize: 'var(--pv-text-xs)',
  textTransform: 'capitalize' as const,
  padding: 'var(--pv-sp-1) 0',
} as const;

export const labelVariants = styleVariants({
  default: { ...labelBase, color: vars.textMuted },
  active: {
    ...labelBase,
    color: vars.accentText,
    fontWeight: 'var(--pv-weight-semibold)',
  },
  done: { ...labelBase, color: vars.ok },
  danger: { ...labelBase, color: vars.danger },
});
