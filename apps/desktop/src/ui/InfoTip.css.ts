// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for InfoTip — replaces .pv-info-tip in primitives.css.
 * Single consumer: src/ui/InfoTip.tsx.
 *
 * INVARIANT: hover and focus-visible states MUST use disjoint declarations.
 * Conveying focus with the accent recolour alone — as the original CSS did
 * while it shared one block with :hover — leaves keyboard focus visually
 * identical to pointer hover (primitives.focus-vs-hover.test.ts asserts this).
 */

import { style } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

const triggerBase = style({
  display: 'inline-flex',
  alignItems: 'center',
  justifyContent: 'center',
  lineHeight: 1,
  flex: 'none',
  position: 'relative',
  cursor: 'help',
  userSelect: 'none',
});

export const infoTip = style([
  triggerBase,
  {
    width: '14px',
    height: '14px',
    borderRadius: '50%',
    border: `1px solid ${vars.textFaint}`,
    color: vars.textMuted,
    fontSize: uvars.textXs,
    fontWeight: uvars.weightSemibold,
    fontStyle: 'normal',
    selectors: {
      // hover: accent recolour only — no focus ring here.
      '&:hover': { borderColor: vars.accent, color: vars.accent },
      // focus-visible: ring only — disjoint from hover state.
      '&:focus-visible': { outline: 'none', boxShadow: vars.focusRing },
    },
  },
]);
