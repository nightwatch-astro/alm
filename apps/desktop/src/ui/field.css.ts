// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for shared form field primitives —
 * replaces .pv-field-* and .pv-input in target-search.css.
 * Multi-consumer: various feature and component files reference these.
 */

import { style } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

export const label = style({
  display: 'block',
  fontSize: uvars.textXs,
  fontWeight: uvars.weightMedium,
  color: vars.textSecondary,
  marginBottom: uvars.sp1,
});

export const hint = style({
  color: vars.textFaint,
  fontWeight: 400,
});

export const error = style({
  display: 'block',
  fontSize: uvars.textXs,
  color: vars.danger,
  marginTop: uvars.sp1,
});

export const input = style({
  width: '100%',
  height: uvars.controlH,
  padding: `0 ${uvars.sp3}`,
  border: `1px solid ${vars.controlBorder}`,
  borderRadius: uvars.radiusSm,
  fontSize: uvars.textSm,
  background: vars.bg,
  color: vars.text,
  outline: 'none',
  transition: `border-color ${uvars.transitionFast}`,
  selectors: {
    '&:focus': { borderColor: vars.accent, boxShadow: vars.focusRing },
    '&::placeholder': { color: vars.textFaint },
  },
});
