// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for Tooltip — replaces .pv-tooltip in primitives.css.
 * Single consumer: src/ui/Tooltip.tsx.
 */

import { globalStyle } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

// Tooltip renders in a portal, so it must carry its own surface.
globalStyle('.pv-tooltip', {
  maxWidth: '280px',
  whiteSpace: 'normal',
  overflowWrap: 'anywhere',
  background: vars.surfaceRaised,
  color: vars.text,
  border: `1px solid ${vars.rule}`,
  borderRadius: uvars.radiusMd,
  padding: `${uvars.sp2} ${uvars.sp3}`,
  fontSize: uvars.textXs,
  lineHeight: uvars.leadingNormal,
  boxShadow: vars.shadowSm,
  zIndex: 60,
});
