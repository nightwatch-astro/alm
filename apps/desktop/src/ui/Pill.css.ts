// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for Pill — replaces .pv-pill* in primitives.css.
 * Single consumer: src/ui/Pill.tsx.
 */

import { style, styleVariants } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

// Pills are labels, never data values — stay sans even inside a mono cell.
const base = style({
  display: 'inline-flex',
  alignItems: 'center',
  padding: `1px ${uvars.sp2}`,
  borderRadius: '99px',
  fontSize: uvars.textXs,
  fontWeight: uvars.weightMedium,
  lineHeight: uvars.leadingRelaxed,
  whiteSpace: 'nowrap',
  border: '1px solid transparent',
  fontFamily: uvars.fontSans,
});

export const variantStyles = styleVariants({
  neutral: [
    base,
    {
      background: vars.chip,
      color: vars.textSecondary,
      borderColor: vars.rule,
    },
  ],
  ghost: [
    base,
    {
      background: 'transparent',
      color: vars.textMuted,
      borderColor: vars.rule,
    },
  ],
  ok: [
    base,
    { background: vars.okBg, color: vars.ok, borderColor: vars.okBorder },
  ],
  warn: [
    base,
    { background: vars.warnBg, color: vars.warn, borderColor: vars.warnBorder },
  ],
  danger: [
    base,
    {
      background: vars.dangerBg,
      color: vars.danger,
      borderColor: vars.dangerBorder,
    },
  ],
  info: [
    base,
    { background: vars.infoBg, color: vars.info, borderColor: vars.infoBorder },
  ],
  accent: [
    base,
    {
      background: vars.accentBg,
      color: vars.accentText,
      borderColor: vars.infoBorder,
    },
  ],
});
