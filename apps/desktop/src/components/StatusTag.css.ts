// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for StatusTag — replaces .pv-status-tag* in projects.css.
 * Single consumer: src/components/StatusTag.tsx.
 */

import { style, styleVariants } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

const base = style({
  display: 'inline-flex',
  alignItems: 'center',
  gap: uvars.sp1,
  fontSize: uvars.textSm,
  color: vars.textSecondary,
  whiteSpace: 'nowrap',
});

export const dot = style({
  display: 'inline-block',
  width: '6px',
  height: '6px',
  borderRadius: '50%',
  flexShrink: 0,
  background: vars.textFaint,
});

export const variantStyles = styleVariants({
  ok: [base, { selectors: { '& > .dot': { background: vars.ok } } }],
  warn: [base, { selectors: { '& > .dot': { background: vars.warn } } }],
  danger: [
    base,
    {
      color: vars.danger,
      selectors: { '& > .dot': { background: vars.danger } },
    },
  ],
  info: [base, { selectors: { '& > .dot': { background: vars.info } } }],
  accent: [base, { selectors: { '& > .dot': { background: vars.accent } } }],
  neutral: [
    base,
    { selectors: { '& > .dot': { background: vars.textMuted } } },
  ],
  ghost: [base],
});

export const dotVariants = styleVariants({
  ok: [dot, { background: vars.ok }],
  warn: [dot, { background: vars.warn }],
  danger: [dot, { background: vars.danger }],
  info: [dot, { background: vars.info }],
  accent: [dot, { background: vars.accent }],
  neutral: [dot, { background: vars.textMuted }],
  ghost: [dot],
});
