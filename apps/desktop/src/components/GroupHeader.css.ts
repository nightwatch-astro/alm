// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// GroupHeader — virtual-list section divider primitive.

import { style } from '@vanilla-extract/css';
import { vars } from '@/styles/themes.css';

export const groupHeader = style({
  padding: '6px var(--pv-sp-3)',
  fontSize: 'var(--pv-text-xs)',
  fontWeight: 'var(--pv-weight-semibold)',
  color: vars.textMuted,
  textTransform: 'uppercase',
  letterSpacing: 'var(--pv-tracking-normal)',
  background: vars.surface,
  borderBottom: `1px solid ${vars.borderSubtle}`,
});
