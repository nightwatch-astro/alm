// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for ThemePicker — replaces .pv-theme-swatch* in
 * target-search.css. Consumers: ThemePicker.tsx, StepLanguage.tsx.
 */

import { style } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

export const picker = style({
  display: 'flex',
  flexDirection: 'column',
  gap: uvars.sp3,
});

export const pickerGroup = style({
  display: 'flex',
  flexDirection: 'column',
  gap: uvars.sp2,
});

export const swatches = style({
  display: 'grid',
  gridTemplateColumns: `repeat(auto-fit, minmax(min(100%, calc(${uvars.controlH} * 4)), 1fr))`,
  gap: uvars.sp2,
});

export const swatch = style({
  position: 'relative',
  border: `1px solid ${vars.border}`,
  borderRadius: uvars.radiusLg,
  background: vars.surfaceRaised,
  padding: uvars.sp2,
  color: vars.text,
  cursor: 'pointer',
  minWidth: 0,
  textAlign: 'left',
  display: 'flex',
  flexDirection: 'column',
  gap: uvars.sp1,
  transition: `border-color ${uvars.transitionFast}, box-shadow ${uvars.transitionFast}`,
  selectors: {
    '&:hover': { borderColor: vars.accent },
    '&:focus-visible': { outline: 'none', boxShadow: vars.focusRing },
  },
});

export const swatchActive = style({
  borderColor: vars.accent,
  boxShadow: `0 0 0 1px ${vars.accent}`,
});

export const swatchSelected = style({
  position: 'absolute',
  insetBlockStart: uvars.sp2,
  insetInlineEnd: uvars.sp2,
  display: 'grid',
  placeItems: 'center',
  inlineSize: `calc(${uvars.controlH} / 2)`,
  blockSize: `calc(${uvars.controlH} / 2)`,
  borderRadius: uvars.radiusLg,
  background: vars.accent,
  color: vars.onAccent,
  fontSize: uvars.textXs,
  fontWeight: uvars.weightSemibold,
});

export const swatchPrev = style({
  display: 'flex',
  height: `calc(${uvars.controlH} + ${uvars.sp3})`,
  borderRadius: uvars.radiusMd,
  overflow: 'hidden',
  border: `1px solid ${vars.borderSubtle}`,
});

// color swatch bars — apply background inline per theme
export const swatchName = style({
  fontSize: uvars.textSm,
  fontWeight: uvars.weightSemibold,
  color: vars.text,
});

export const swatchMode = style({
  fontSize: uvars.textXs,
  color: vars.textMuted,
  textTransform: 'capitalize',
});
