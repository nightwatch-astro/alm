// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Shared `pv-select` primitive — applies to native `<select>` elements
 * and base-ui `<Select.Trigger>` wrappers across the app.
 * Consumers import `selectBase` and compose it with any co-located classes.
 */

import { style } from '@vanilla-extract/css';
import { vars } from '@/styles/themes.css';

export const selectBase = style({
  height: 'var(--pv-control-h)',
  padding: '0 var(--pv-sp-2)',
  border: `1px solid ${vars.controlBorder}`,
  borderRadius: 'var(--pv-radius-sm)',
  fontSize: 'var(--pv-text-sm)',
  background: vars.bg,
  color: vars.text,
  cursor: 'pointer',
  selectors: {
    '&:focus': {
      borderColor: vars.accent,
      outline: 'none',
    },
  },
});
