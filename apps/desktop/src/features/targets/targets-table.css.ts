// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for TargetsTable — sole owner of the former
 * .pv-targets-table*, .pv-targets-col--*, .pv-targets-cell*, .pv-targets-star*,
 * .pv-filter-badge* and .pv-imgtime-glyph--* rules. The .pv-guidance-*,
 * .pv-moon-* and .pv-planner-* families stay in targets.css.
 *
 * Consumers: TargetsTable.tsx, TargetsTableColumns.tsx, TargetsPage.tsx,
 * FilterBadges.tsx.
 */

import { globalStyle, style, styleVariants } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

// ── Table ─────────────────────────────────────────────────────────────────────

export const table = style({
  tableLayout: 'fixed',
  width: '100%',
  minWidth: '1000px',
  vars: { '--pv-targets-star-w': '50px' },
});

globalStyle(`${table} th`, { userSelect: 'none' });

globalStyle(`${table} td, ${table} th`, {
  padding: `${uvars.sp1} ${uvars.sp3}`,
});

globalStyle(`${table} tbody td`, { height: uvars.rowHeight });

export const row = style({ cursor: 'pointer' });

globalStyle(`${row}:hover td`, { background: vars.surface });

export const rowSelected = style({});

globalStyle(`${rowSelected} td`, { background: vars.accentBg });

export const tableBody = style({ position: 'relative' });

export const spacer = style({});

globalStyle(`${spacer} td`, { padding: 0, border: 'none' });

export const empty = style({
  padding: uvars.sp5,
  textAlign: 'center',
  color: vars.textMuted,
  fontSize: uvars.textSm,
});

export const footer = style({
  padding: `${uvars.sp2} ${uvars.sp3}`,
  fontSize: uvars.textXs,
  color: vars.textMuted,
  borderTop: `1px solid ${vars.borderSubtle}`,
});

// ── Virtualized scroll container ──────────────────────────────────────────────

export const wrap = style({
  display: 'flex',
  flexDirection: 'column',
  minHeight: 0,
  height: '100%',
});

export const scroll = style({
  flex: '1 1 auto',
  minHeight: 0,
  overflowY: 'auto',
  overflowX: 'auto',
});

globalStyle(`${scroll} ${table} thead th`, {
  position: 'sticky',
  top: 0,
  zIndex: 1,
});

// ── Pinned columns (star + designation) ───────────────────────────────────────

globalStyle(`${scroll} ${table} thead th:nth-child(-n + 2)`, {
  position: 'sticky',
});

globalStyle(`${scroll} ${table} thead th:nth-child(1)`, { left: 0 });

globalStyle(`${scroll} ${table} thead th:nth-child(2)`, {
  left: 'var(--pv-targets-star-w)',
  boxShadow: `1px 0 0 ${vars.borderSubtle}`,
});

globalStyle(`${scroll} ${table} ${row} > td:nth-child(-n + 2)`, {
  position: 'sticky',
  zIndex: 2,
  background: vars.bg,
});

globalStyle(`${scroll} ${table} thead th:nth-child(-n + 2)`, { zIndex: 3 });

globalStyle(`${scroll} ${table} ${row}:hover > td:nth-child(-n + 2)`, {
  background: vars.surface,
});

globalStyle(`${scroll} ${table} ${rowSelected} > td:nth-child(-n + 2)`, {
  background: vars.accentBg,
});

// ── Page wrapper ──────────────────────────────────────────────────────────────

export const page = style({ display: 'contents' });

// Keyed on the test id, not a class: ListPageLayout's main column is a
// vanilla-extract style whose hashed class name is unaddressable from here.
// This rule establishes the containing block for the absolutely-positioned
// `wrap` below — without it the table escapes to the viewport and overlays the
// page top bar.
globalStyle(`${page} [data-testid='listpage-main']`, {
  overflow: 'hidden',
  position: 'relative',
});

globalStyle(`${page} ${wrap}`, { position: 'absolute', inset: 0 });

// ── Column widths ─────────────────────────────────────────────────────────────

export const colStar = style({ width: 'var(--pv-targets-star-w)' });
export const colDesignation = style({ width: '20%' });
export const colType = style({ width: '10.5%' });
export const colMaxalt = style({ width: '7%' });
export const colOpposition = style({ width: '15%' });
export const colSessions = style({ width: '8%' });
export const colLunardist = style({ width: '6.5%' });
export const colFilters = style({ width: '18%' });
export const colImagingtime = style({ width: '10%' });

// ── Designation cell ──────────────────────────────────────────────────────────

export const desigCell = style({
  display: 'inline-flex',
  alignItems: 'baseline',
  gap: uvars.sp2,
  minWidth: 0,
});

export const desigLabel = style({
  fontWeight: uvars.weightMedium,
  color: vars.text,
});

export const desigAlt = style({
  fontSize: uvars.textXs,
  color: vars.textMuted,
});

// ── Cell helpers ──────────────────────────────────────────────────────────────

export const cellNum = style({
  fontVariantNumeric: 'tabular-nums',
  textAlign: 'right',
});
export const cellMuted = style({ color: vars.textMuted });
export const cellCenter = style({ textAlign: 'center' });
export const cellOpposition = style({
  fontVariantNumeric: 'tabular-nums',
  textAlign: 'right',
  color: vars.textMuted,
});
export const cellLunardist = style({
  fontVariantNumeric: 'tabular-nums',
  color: vars.textMuted,
});
export const cellFilters = style({ whiteSpace: 'normal' });

// ── Star toggle ───────────────────────────────────────────────────────────────

export const star = style({
  display: 'inline-flex',
  alignItems: 'center',
  justifyContent: 'center',
  width: '20px',
  height: '20px',
  padding: 0,
  background: 'none',
  border: 'none',
  borderRadius: uvars.radiusSm,
  cursor: 'pointer',
  fontSize: uvars.textBase,
  lineHeight: 1,
  color: vars.textFaint,
  transition: `color ${uvars.transitionFast}`,
  selectors: {
    '&:hover, &:focus-visible': {
      color: vars.accent,
      outline: 'none',
      background: `color-mix(in srgb, ${vars.accent} 12%, transparent)`,
    },
  },
});

export const starActive = style({ color: vars.accent });

// ── Filter badges ─────────────────────────────────────────────────────────────

export const filterBadges = style({
  display: 'inline-flex',
  flexWrap: 'wrap',
  gap: uvars.sp2,
  alignItems: 'center',
});

export const filterBadge = style({
  display: 'inline-block',
  padding: `0 ${uvars.sp1}`,
  borderRadius: uvars.radiusSm,
  fontSize: uvars.textXs,
  fontWeight: uvars.weightMedium,
  lineHeight: uvars.leadingTight,
  letterSpacing: uvars.trackingTight,
  border: '1px solid transparent',
  whiteSpace: 'nowrap',
});

/**
 * Broadband/narrowband tier tint. Composed with `filterBadge` plus one of
 * `filterBadgeViable`/`filterBadgeNotViable`, so these cannot be a
 * `styleVariants` set covering all five former BEM modifiers.
 */
export const filterBadgeTier = styleVariants({
  broadband: {
    background: `color-mix(in srgb, ${vars.accent} 12%, transparent)`,
    color: vars.accent,
    borderColor: `color-mix(in srgb, ${vars.accent} 30%, transparent)`,
  },
  narrowband: {
    background: `color-mix(in srgb, ${vars.textMuted} 12%, transparent)`,
    color: vars.textSecondary,
    borderColor: `color-mix(in srgb, ${vars.textMuted} 30%, transparent)`,
  },
});

export const filterBadgeNotViable = style({
  background: 'transparent',
  color: vars.textMuted,
  borderColor: vars.rule,
  opacity: 0.55,
});

export const filterBadgeViable = style({
  boxShadow: `inset 0 0 0 1px color-mix(in srgb, ${vars.ok} 25%, transparent)`,
});

export const filterBadgeUnknown = style({
  background: vars.chip,
  color: vars.textSecondary,
  borderColor: vars.rule,
  padding: `0 ${uvars.sp2}`,
});

// ── Planner widgets ───────────────────────────────────────────────────────────

export const imgtimeGlyphWarn = style({ color: vars.warn });
export const imgtimeGlyphMuted = style({ color: vars.textFaint });
