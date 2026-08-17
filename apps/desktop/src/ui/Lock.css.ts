// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for Lock — replaces .pv-lock in primitives.css.
 * Single consumer: src/ui/Lock.tsx.
 *
 * INVARIANT: hover and focus-visible states MUST use disjoint declarations
 * (same constraint as InfoTip — the padlock emoji cannot carry accent recolour,
 * so only the focus ring distinguishes keyboard state).
 */

import { style } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

export const lock = style({
  display: 'inline-flex',
  alignItems: 'center',
  justifyContent: 'center',
  lineHeight: 1,
  flex: 'none',
  position: 'relative',
  cursor: 'help',
  userSelect: 'none',
  borderRadius: uvars.radiusSm,
  fontSize: uvars.textXs,
  selectors: {
    // focus-visible: ring only — disjoint from any hover state.
    '&:focus-visible': { outline: 'none', boxShadow: vars.focusRing },
  },
});
