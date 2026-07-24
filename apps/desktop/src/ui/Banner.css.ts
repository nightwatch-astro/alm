// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for Banner — replaces .pv-banner* in primitives.css.
 * Single consumer: src/ui/Banner.tsx.
 */

import { style, styleVariants } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

const base = style({
  padding: `${uvars.sp2} ${uvars.sp3}`,
  borderRadius: uvars.radiusSm,
  fontSize: uvars.textXs,
  display: 'flex',
  alignItems: 'center',
  gap: uvars.sp2,
});

export const variantStyles = styleVariants({
  warn: [
    base,
    {
      background: vars.warnBg,
      color: vars.warn,
      border: `1px solid ${vars.warnBorder}`,
    },
  ],
  danger: [
    base,
    {
      background: vars.dangerBg,
      color: vars.danger,
      border: `1px solid ${vars.dangerBorder}`,
    },
  ],
  info: [
    base,
    {
      background: vars.infoBg,
      color: vars.info,
      border: `1px solid ${vars.infoBorder}`,
    },
  ],
});

export const actionLink = style({
  color: 'inherit',
  fontWeight: uvars.weightMedium,
  textDecoration: 'underline',
  whiteSpace: 'nowrap',
});
