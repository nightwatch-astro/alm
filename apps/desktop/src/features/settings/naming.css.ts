// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for naming/pattern editors — replaces .pv-naming*
 * in detail-panes.css. Consumers: PatternChipsEditor, NamingStructure,
 * PerTypePatternChipsEditor, PerTypeDestinationPatterns (all in settings).
 */

import { style } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

export const chipRow = style({
  display: 'flex',
  flexWrap: 'wrap',
  gap: uvars.sp1,
  alignItems: 'center',
  minHeight: '32px',
});

export const menuAnchor = style({
  position: 'relative',
  display: 'inline-block',
});

export const dropdown = style({
  position: 'absolute',
  top: '100%',
  left: 0,
  zIndex: 10,
  background: vars.surfaceRaised,
  border: `1px solid ${vars.border}`,
  borderRadius: uvars.radiusMd,
  padding: uvars.sp1,
  boxShadow: vars.shadowSm,
});

export const dropdownToken = style({ minWidth: '160px' });
export const dropdownSep = style({ minWidth: '100px' });
export const dropdownLiteral = style({
  minWidth: '160px',
  display: 'flex',
  gap: uvars.sp1,
});

export const menuItem = style({
  display: 'block',
  width: '100%',
  textAlign: 'left',
  padding: `${uvars.sp1} ${uvars.sp2}`,
  background: vars.surfaceRaised,
  color: vars.text,
  border: 'none',
  cursor: 'pointer',
  fontSize: uvars.textXs,
  selectors: {
    '&:hover, &:focus': { outline: 'none', background: vars.hoverBg },
  },
});

export const literalInput = style({
  fontSize: uvars.textXs,
  width: '100px',
  padding: `${uvars.sp0} 6px`,
  background: vars.surfaceRaised,
  color: vars.text,
  border: `1px solid ${vars.border}`,
  borderRadius: uvars.radiusMd,
  selectors: {
    '&::placeholder': { color: vars.textFaint },
    '&:focus': { outline: 'none', borderColor: vars.accent },
  },
});

export const literalAddBtn = style({
  padding: `${uvars.sp0} ${uvars.sp2}`,
  background: vars.accent,
  color: vars.onAccent,
  border: 'none',
  borderRadius: uvars.radiusMd,
  cursor: 'pointer',
  fontSize: uvars.textXs,
  selectors: {
    '&:hover': { background: vars.accentHover },
    '&:focus': { outline: 'none', border: `1px solid ${vars.accentDeep}` },
  },
});

export const chipPlaceholder = style({
  fontSize: uvars.textXs,
  color: vars.textMuted,
  fontStyle: 'italic',
});

export const error = style({
  marginTop: uvars.sp1,
  fontSize: uvars.textXs,
  color: vars.danger,
});

export const warning = style({
  marginTop: uvars.sp1,
  fontSize: uvars.textXs,
  color: vars.textMuted,
});
