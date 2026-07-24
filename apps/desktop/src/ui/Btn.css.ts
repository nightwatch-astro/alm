// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for Btn — replaces .pv-btn* rules in primitives.css.
 * Single consumer: src/ui/Btn.tsx.
 */

import { style, styleVariants } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

const base = style({
  display: 'inline-flex',
  alignItems: 'center',
  justifyContent: 'center',
  gap: uvars.sp1,
  height: uvars.controlH,
  padding: `0 ${uvars.sp3}`,
  fontSize: uvars.textXs,
  fontWeight: uvars.weightMedium,
  border: `1px solid ${vars.border}`,
  borderRadius: uvars.radiusSm,
  background: vars.bg,
  color: vars.textSecondary,
  cursor: 'pointer',
  whiteSpace: 'nowrap',
  transition: `all ${uvars.transitionFast}`,
  selectors: {
    '&:hover': { borderColor: vars.ink3, color: vars.text },
    // Disabled: visually inert, no accent or danger fill.
    '&:disabled': { opacity: 0.35, cursor: 'not-allowed' },
    '&:disabled:hover': { borderColor: vars.border, color: vars.textSecondary },
  },
});

/** Size modifiers */
export const sizeVariants = styleVariants({
  default: [base],
  sm: [
    base,
    {
      height: uvars.controlHSm,
      padding: `0 ${uvars.sp2}`,
    },
  ],
  xs: [
    base,
    {
      height: uvars.controlHXs,
      padding: `0 ${uvars.sp2}`,
    },
  ],
});

// Destructive glyph: alert triangle mask so it tracks currentColor in all themes.
const TRIANGLE_SVG = encodeURIComponent(
  `<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 16 16'><path d='M8 1L15 14H1L8 1Z'/><rect x='7.25' y='6' width='1.5' height='4' rx='0.5'/><rect x='7.25' y='11' width='1.5' height='1.5' rx='0.5'/></svg>`,
);

/** Variant styles keyed on BtnVariant union */
export const variantStyles = styleVariants({
  default: {},
  primary: {
    background: vars.accent,
    color: vars.onAccent,
    borderColor: vars.accent,
    selectors: {
      '&:hover': {
        background: vars.accentHover,
        borderColor: vars.accentHover,
      },
      '&:disabled': {
        background: vars.bg,
        color: vars.textSecondary,
        borderColor: vars.border,
      },
    },
  },
  danger: {
    background: vars.dangerBg,
    color: vars.danger,
    borderColor: vars.dangerBorder,
    selectors: {
      '&:hover': { background: vars.dangerBgHover },
      '&:disabled': {
        background: vars.bg,
        color: vars.textSecondary,
        borderColor: vars.border,
      },
    },
  },
  destructive: {
    color: vars.destructive,
    background: vars.destructiveBg,
    borderColor: vars.destructive,
    borderWidth: '1.5px',
    boxShadow: `0 2px 10px -3px color-mix(in oklab, ${vars.destructive} 42%, transparent)`,
    selectors: {
      '&::before': {
        content: '""',
        flex: 'none',
        width: '13px',
        height: '13px',
        backgroundColor: 'currentColor',
        maskImage: `url("data:image/svg+xml,${TRIANGLE_SVG}")`,
        maskRepeat: 'no-repeat',
        maskSize: 'contain',
        WebkitMaskImage: `url("data:image/svg+xml,${TRIANGLE_SVG}")`,
        WebkitMaskRepeat: 'no-repeat',
        WebkitMaskSize: 'contain',
      },
      '&:hover': {
        background: vars.destructiveBgHover,
        borderColor: vars.destructive,
        color: vars.destructive,
        boxShadow: `0 3px 13px -3px color-mix(in oklab, ${vars.destructive} 55%, transparent)`,
      },
      '&:active': {
        background: vars.destructiveBgHover,
        boxShadow: 'inset 0 2px 5px rgba(0,0,0,0.22)',
      },
      '&:disabled': {
        background: vars.destructiveBg,
        color: vars.destructive,
        borderColor: vars.destructive,
        opacity: 0.4,
        boxShadow: 'none',
      },
    },
  },
  ghost: {
    background: 'transparent',
    borderColor: 'transparent',
    color: vars.textMuted,
    selectors: {
      '&:hover': {
        background: vars.hoverBg,
        borderColor: 'transparent',
        color: vars.text,
      },
    },
  },
});
