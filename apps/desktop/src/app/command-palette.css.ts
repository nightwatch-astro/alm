// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for CommandPalette — replaces .pv-palette* +
 * .pv-palette-backdrop* in target-search.css. Single consumer: CommandPalette.tsx.
 *
 * Sits above Modal (z-index 500/501) at 600/601 so a command can be summoned
 * from within a modal.
 */

import { globalStyle, style } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

export const backdrop = style({
  position: 'fixed',
  inset: 0,
  zIndex: 600,
  background: `color-mix(in srgb, ${vars.ink} 45%, transparent)`,
  backdropFilter: 'blur(2px)',
  '@supports': {
    'not (backdrop-filter: blur(2px))': {
      background: `color-mix(in srgb, ${vars.ink} 55%, transparent)`,
    },
  },
});

export const palette = style({
  position: 'fixed',
  top: '14vh',
  left: '50%',
  transform: 'translateX(-50%)',
  zIndex: 601,
  display: 'flex',
  flexDirection: 'column',
  width: 'min(94vw, 560px)',
  maxHeight: '70vh',
  background: vars.surfaceRaised,
  border: `1px solid ${vars.border}`,
  borderRadius: uvars.radiusLg,
  boxShadow: vars.shadowSm,
  overflow: 'hidden',
  outline: 'none',
});

export const paletteInput = style({
  flexShrink: 0,
  width: '100%',
  padding: `${uvars.sp3} ${uvars.sp4}`,
  border: 'none',
  borderBottom: `1px solid ${vars.borderSubtle}`,
  borderRadius: 0,
  background: 'transparent',
  color: vars.text,
  fontSize: uvars.textMd,
  outline: 'none',
  selectors: { '&::placeholder': { color: vars.textFaint } },
});

export const paletteList = style({
  flex: 1,
  minHeight: 0,
  overflowY: 'auto',
  padding: uvars.sp2,
});

export const paletteEmpty = style({
  padding: uvars.sp4,
  textAlign: 'center',
  fontSize: uvars.textSm,
  color: vars.textMuted,
});

export const paletteGroup = style({
  paddingBlock: uvars.sp1,
  selectors: {
    '&:not(:last-child)': {
      borderBottom: `1px solid ${vars.borderSubtle}`,
      marginBottom: uvars.sp1,
    },
  },
});

// cmdk wraps heading in an element carrying `cmdk-group-heading` — no dedicated className prop.
globalStyle(`${paletteGroup} [cmdk-group-heading]`, {
  padding: `${uvars.sp2} ${uvars.sp3} ${uvars.sp1}`,
  fontSize: uvars.textXs,
  fontWeight: uvars.weightSemibold,
  textTransform: 'uppercase',
  letterSpacing: uvars.trackingNormal,
  color: vars.textMuted,
});

export const paletteItem = style({
  display: 'flex',
  alignItems: 'center',
  gap: uvars.sp2,
  padding: `${uvars.sp2} ${uvars.sp3}`,
  borderRadius: uvars.radiusSm,
  color: vars.text,
  cursor: 'pointer',
  selectors: { '&[data-selected="true"]': { background: vars.selectedBg } },
});

export const paletteItemKind = style({
  flexShrink: 0,
  fontSize: uvars.textXs,
  fontWeight: uvars.weightSemibold,
  textTransform: 'uppercase',
  letterSpacing: uvars.trackingTight,
  color: vars.textFaint,
  minWidth: '3.5em',
});

export const paletteItemLabel = style({
  flex: 1,
  minWidth: 0,
  fontSize: uvars.textSm,
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
});

export const paletteItemSub = style({
  flexShrink: 0,
  fontSize: uvars.textXs,
  color: vars.textMuted,
});
