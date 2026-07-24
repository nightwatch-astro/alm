// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for PropertyTable — replaces .pv-property-table*
 * in target-search.css. Consumers: PropertyTable.tsx, RenderValue.tsx.
 */

import { style } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

const gridRow = {
  display: 'grid',
  gridTemplateColumns: 'minmax(130px, 190px) 1fr auto auto',
  gap: uvars.sp3,
  alignItems: 'baseline',
  padding: '5px 0',
  borderBottom: `1px solid ${vars.rule2}`,
} as const;

export const root = style({ display: 'flex', flexDirection: 'column' });

export const header = style({
  ...gridRow,
  borderBottom: `1px solid ${vars.rule}`,
  paddingBottom: '6px',
  marginBottom: uvars.sp0,
});

export const row = style({
  ...gridRow,
  selectors: { '&:last-child': { borderBottom: 'none' } },
});

export const cellLabel = style({
  color: vars.textMuted,
  fontSize: uvars.textSm,
});

export const cellValue = style({
  color: vars.text,
  fontSize: uvars.textSm,
  fontVariantNumeric: 'tabular-nums',
  minWidth: 0,
  overflowWrap: 'anywhere',
  // Multi-line values keep their \n breaks.
  whiteSpace: 'pre-line',
});

export const cellSource = style({ justifySelf: 'start' });
export const cellConfirm = style({ justifySelf: 'start' });

export const headerCell = style({
  fontSize: uvars.textXs,
  fontWeight: uvars.weightSemibold,
  letterSpacing: uvars.trackingNormal,
  textTransform: 'uppercase',
  color: vars.textMuted,
});

// Source badges
const badgeBase = style({
  fontSize: uvars.textXs,
  fontWeight: uvars.weightSemibold,
  letterSpacing: uvars.trackingTight,
  textTransform: 'uppercase',
  padding: '1px 7px',
  borderRadius: '8px',
  background: vars.bg3,
  color: vars.textMuted,
  border: '1px solid transparent',
});

export const sourceBadge = style([badgeBase]);
export const sourceBadgeFits = style([badgeBase, { background: vars.infoBg, color: vars.info, borderColor: vars.infoBorder }]);
export const sourceBadgeUser = style([badgeBase, { background: vars.okBg, color: vars.ok, borderColor: vars.okBorder }]);
export const sourceBadgeInferred = style([badgeBase, { background: vars.warnBg, color: vars.warn, borderColor: vars.warnBorder }]);
export const sourceBadgeDefault = style([badgeBase, { background: vars.bg3, color: vars.textMuted }]);
