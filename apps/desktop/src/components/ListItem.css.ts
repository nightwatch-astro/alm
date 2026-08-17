// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// ListItem.tsx styles. Also consumed by TargetList (one structural copy of
// the same primitive — TargetList imports from here rather than duplicating).

import { style } from '@vanilla-extract/css';
import { vars } from '@/styles/themes.css';

export const listItem = style({
  display: 'flex',
  flexDirection: 'column',
  gap: 'var(--pv-sp-0)',
  padding: 'var(--pv-sp-2) var(--pv-sp-3)',
  cursor: 'pointer',
  borderBottom: `1px solid ${vars.borderSubtle}`,
  transition: 'background var(--pv-transition-fast)',
  selectors: { '&:hover': { background: vars.hoverBg } },
});

export const listItemSelected = style({ background: vars.selectedBg });
export const listItemMuted = style({ opacity: 0.55 });

export const title = style({
  display: 'flex',
  alignItems: 'center',
  gap: 'var(--pv-sp-1)',
  fontSize: 'var(--pv-text-sm)',
  fontWeight: 'var(--pv-weight-medium)',
  color: vars.text,
});

export const meta = style({
  display: 'flex',
  alignItems: 'center',
  gap: 'var(--pv-sp-1)',
  fontSize: 'var(--pv-text-xs)',
  color: vars.textMuted,
  fontVariantNumeric: 'tabular-nums',
});

export const metaSep = style({ color: vars.rule });
