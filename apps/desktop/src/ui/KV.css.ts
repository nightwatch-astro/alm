// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for KV row — replaces .pv-kv* in primitives.css.
 * Single consumer: src/ui/KV.tsx.
 */

import { style } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

export const row = style({
  display: 'flex',
  padding: `${uvars.sp1} 0`,
  gap: uvars.sp3,
});

export const label = style({
  width: '120px',
  flexShrink: 0,
  fontSize: uvars.textXs,
  color: vars.textMuted,
  textTransform: 'uppercase',
  letterSpacing: uvars.trackingTight,
  fontWeight: uvars.weightMedium,
});

export const value = style({
  fontSize: uvars.textSm,
});

export const provenance = style({
  fontSize: uvars.textXs,
  color: vars.textFaint,
  marginLeft: uvars.sp1,
});
