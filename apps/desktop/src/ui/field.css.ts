// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for shared form field primitives —
 * replaces .pv-field-* and .pv-input in target-search.css.
 * Multi-consumer: various feature and component files reference these.
 */

import { style, globalStyle } from '@vanilla-extract/css';
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

// Backward-compat global alias: StepSourceFolders, NumberField, and similar
// pre-migration callers still apply className="pv-input" directly. The rule
// was in target-search.css (dissolved in wave 2); re-declare it here so those
// consumers keep their theming until they are migrated to the `input` style.
globalStyle('.pv-input', {
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
});
globalStyle('.pv-input:focus', {
  borderColor: vars.accent,
  boxShadow: vars.focusRing,
});
globalStyle('.pv-input::placeholder', { color: vars.textFaint });

// Same backward-compat alias for the hint/error text: ConfirmModal, SettingsKit,
// NumberField and the project/target dialogs still apply these class names
// directly, and their rules also left with target-search.css.
globalStyle('.pv-field-hint', {
  color: vars.textFaint,
  fontWeight: 400,
});

globalStyle('.pv-field-error', {
  display: 'block',
  fontSize: uvars.textXs,
  color: vars.danger,
  marginTop: uvars.sp1,
});
