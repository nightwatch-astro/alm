// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for Modal — replaces modals.css (115L).
 */

import { style, styleVariants } from '@vanilla-extract/css';
import { vars } from '@/styles/themes.css';

export const backdrop = style({
  position: 'fixed',
  inset: 0,
  zIndex: 500,
  background: `color-mix(in srgb, ${vars.ink} 45%, transparent)`,
  backdropFilter: 'blur(2px)',
  '@supports': {
    'not (backdrop-filter: blur(2px))': {
      background: `color-mix(in srgb, ${vars.ink} 55%, transparent)`,
    },
  },
});

export const popup = style({
  position: 'fixed',
  top: '50%',
  left: '50%',
  transform: 'translate(-50%, -50%)',
  zIndex: 501,
  display: 'flex',
  flexDirection: 'column',
  width: 'min(94vw, var(--pv-modal-w, 640px))',
  maxHeight: '88vh',
  background: vars.surfaceRaised,
  border: `1px solid ${vars.border}`,
  borderRadius: 'var(--pv-radius-lg)',
  boxShadow: `0 8px 32px color-mix(in srgb, ${vars.ink} 16%, transparent)`,
  overflow: 'hidden',
});

/** Width presets — keyed on ModalSize union. */
export const sizeVariants = styleVariants({
  sm: { '--pv-modal-w': '420px' } as Record<string, string>,
  md: { '--pv-modal-w': '640px' } as Record<string, string>,
  lg: { '--pv-modal-w': '920px' } as Record<string, string>,
  xl: { '--pv-modal-w': '1200px' } as Record<string, string>,
  auto: { width: 'auto', maxWidth: '94vw' },
});

export const header = style({
  flexShrink: 0,
  display: 'flex',
  alignItems: 'center',
  gap: 'var(--pv-sp-3)',
  padding: 'var(--pv-sp-3) var(--pv-sp-4)',
  borderBottom: `1px solid ${vars.borderSubtle}`,
});

export const title = style({
  fontSize: 'var(--pv-text-md)',
  fontWeight: 'var(--pv-weight-semibold)',
  color: vars.text,
  margin: 0,
});

export const titleSpacer = style({ flex: 1 });

export const subtitle = style({
  fontSize: 'var(--pv-text-sm)',
  color: vars.textMuted,
});

export const closeBtn = style({
  marginLeft: 'auto',
  appearance: 'none',
  background: 'none',
  border: 'none',
  cursor: 'pointer',
  color: vars.textMuted,
  fontSize: 'var(--pv-text-sm)',
  lineHeight: 1,
  padding: 'var(--pv-sp-1) var(--pv-sp-2)',
  borderRadius: 'var(--pv-radius-sm)',
  selectors: {
    '&:hover': {
      color: vars.text,
      background: vars.hoverBg,
    },
  },
});

export const body = style({
  flex: 1,
  minHeight: 0,
  overflowY: 'auto',
  padding: 'var(--pv-sp-4)',
});

/** Body variant for content managing its own internal scroll region. */
export const bodyFill = style({
  display: 'flex',
  flexDirection: 'column',
  overflow: 'hidden',
});

export const message = style({
  margin: 0,
  fontSize: 'var(--pv-text-sm)',
  color: vars.textSecondary,
});

export const footer = style({
  flexShrink: 0,
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'flex-end',
  gap: 'var(--pv-sp-2)',
  padding: 'var(--pv-sp-3) var(--pv-sp-4)',
  borderTop: `1px solid ${vars.borderSubtle}`,
});
