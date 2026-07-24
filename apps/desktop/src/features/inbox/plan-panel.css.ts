// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for the Plan Panel feature.
 * Consumers: PlanPanel.tsx, PlanGroupRow.tsx, PlanDestructiveControl.tsx,
 * PlanRootPicker.tsx — all in src/features/inbox/.
 */

import { style } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

// ── Root container + column template ────────────────────────────────────────

export const root = style({
  vars: {
    '--pv-plan-cols':
      '60px minmax(120px, 1.4fr) minmax(150px, 1.6fr) minmax(120px, 1.3fr) 72px 92px',
  },
});

// ── Grid rows that share the column template ─────────────────────────────────

const gridRow = style({
  display: 'grid',
  gridTemplateColumns: 'var(--pv-plan-cols)',
  alignItems: 'center',
  columnGap: uvars.sp3,
});

export const listHead = style([
  gridRow,
  {
    padding: `0 0 ${uvars.sp1}`,
    marginBottom: uvars.sp1,
    borderBottom: `1px solid ${vars.border}`,
    fontSize: uvars.textXs,
    textTransform: 'uppercase',
    letterSpacing: uvars.trackingNormal,
    color: vars.textMuted,
  },
]);

export const groupHeader = style([
  gridRow,
  {
    paddingBottom: uvars.sp1,
    borderBottom: `1px solid ${vars.border}`,
  },
]);

export const fileRow = style([
  gridRow,
  {
    padding: '1px 0',
    fontSize: uvars.textXs,
  },
]);

// ── Root container scrollable area ───────────────────────────────────────────

export const scroll = style({
  // Scrollable area — flex:1 handled by parent flex chain.
  flex: 1,
  minHeight: 0,
  overflowY: 'auto',
});

// ── Pinned bar (top of the panel) ────────────────────────────────────────────

export const bar = style({
  display: 'flex',
  alignItems: 'center',
  gap: uvars.sp3,
  padding: `${uvars.sp2} ${uvars.sp3}`,
  borderBottom: `1px solid ${vars.border}`,
  flexShrink: 0,
});

export const barLeft = style({
  display: 'flex',
  alignItems: 'center',
  gap: uvars.sp3,
  flex: 1,
  minWidth: 0,
});

export const selectAll = style({
  display: 'flex',
  alignItems: 'center',
  gap: uvars.sp2,
  cursor: 'pointer',
});

export const selectAllLabel = style({
  fontSize: uvars.textSm,
  color: vars.textSecondary,
});

export const countSummary = style({
  color: vars.textMuted,
  fontSize: uvars.textXs,
});

export const barActions = style({
  display: 'flex',
  gap: uvars.sp2,
  alignItems: 'center',
});

// ── Group ────────────────────────────────────────────────────────────────────

export const group = style({
  marginBottom: uvars.sp3,
});

export const groupLead = style({
  display: 'inline-flex',
  alignItems: 'center',
  gap: uvars.sp1,
});

export const groupName = style({
  fontWeight: '600',
  fontSize: uvars.textSm,
  color: vars.text,
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
});

export const groupBreakdown = style({
  fontSize: uvars.textXs,
  minWidth: 0,
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
});

export const groupDest = style({
  display: 'inline-flex',
  alignItems: 'baseline',
  gap: uvars.sp1,
  minWidth: 0,
});

export const groupCount = style({
  fontSize: uvars.textXs,
  color: vars.textMuted,
  whiteSpace: 'nowrap',
  justifySelf: 'end',
});

export const groupActions = style({
  display: 'inline-flex',
  alignItems: 'center',
  gap: uvars.sp2,
  justifySelf: 'end',
});

// ── Expand/collapse ──────────────────────────────────────────────────────────

export const expand = style({
  flexShrink: 0,
  paddingLeft: uvars.sp1,
  paddingRight: uvars.sp1,
});

export const chevron = style({
  display: 'inline-block',
  fontSize: uvars.textXs,
  color: vars.textMuted,
  transition: `transform ${uvars.transitionFast}`,
});

export const chevronOpen = style({
  transform: 'rotate(90deg)',
});

// ── Collapsed summary ─────────────────────────────────────────────────────────

export const summary = style({
  listStyle: 'none',
  margin: `${uvars.sp1} 0 0`,
  padding: `0 0 0 ${uvars.sp4}`,
  display: 'flex',
  flexDirection: 'column',
  gap: uvars.sp1,
});

export const summaryLine = style({
  display: 'flex',
  alignItems: 'baseline',
  gap: uvars.sp2,
  fontSize: uvars.textXs,
  minWidth: 0,
});

export const summaryType = style({
  display: 'inline-flex',
  gap: uvars.sp1,
  alignItems: 'baseline',
});

export const summarySep = style({
  color: vars.textFaint,
});

export const summaryTypeName = style({
  color: vars.textSecondary,
});

export const summaryTypeCount = style({
  color: vars.textMuted,
  fontVariantNumeric: 'tabular-nums',
});

export const summaryCount = style({
  color: vars.text,
  fontWeight: '600',
  flexShrink: 0,
});

export const summaryArrow = style({
  color: vars.textMuted,
  flexShrink: 0,
});

export const summaryDest = style({
  color: vars.textSecondary,
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
  minWidth: 0,
});

// ── File rows ─────────────────────────────────────────────────────────────────

export const fileRows = style({
  padding: `${uvars.sp1} 0 ${uvars.sp2}`,
});

export const fileName = style({
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
  color: vars.textSecondary,
});

export const fileAction = style({
  display: 'inline-flex',
  alignItems: 'center',
  gap: uvars.sp1,
  color: vars.textMuted,
});

export const fileDest = style({
  display: 'inline-flex',
  alignItems: 'baseline',
  gap: uvars.sp0,
  minWidth: 0,
  // Claims trailing columns (file count + discard are group-level only).
  gridColumn: '4 / -1',
});

export const inplace = style({
  color: vars.textMuted,
  fontStyle: 'italic',
});

export const fileFlag = style({
  fontSize: uvars.textXs,
  textTransform: 'uppercase',
  letterSpacing: uvars.trackingTight,
  color: vars.danger,
  border: `1px solid currentColor`,
  borderRadius: uvars.radiusSm,
  padding: `0 ${uvars.sp1}`,
});

// ── Shared path cell ─────────────────────────────────────────────────────────

export const path = style({
  color: vars.textSecondary,
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
  // rtl keeps the meaningful tail visible when a long absolute path is ellipsised.
  direction: 'rtl',
  minWidth: 0,
});

// ── Action rows ───────────────────────────────────────────────────────────────

export const row = style({
  display: 'grid',
  gridTemplateColumns: 'auto 1fr 1fr',
  gap: uvars.sp2,
  padding: `${uvars.sp1} 0`,
  borderBottom: `1px solid ${vars.border}`,
  fontSize: uvars.textXs,
  alignItems: 'baseline',
});

export const kind = style({
  color: vars.textSecondary,
  textTransform: 'uppercase',
  letterSpacing: uvars.trackingNormal,
  fontWeight: '600',
});

export const planFilename = style({
  color: vars.text,
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
});

// ── Stale badge / banner ──────────────────────────────────────────────────────

export const staleBadge = style({
  fontSize: uvars.textXs,
  fontWeight: '600',
  color: vars.danger,
  border: '1px solid currentColor',
  borderRadius: uvars.radiusMd,
  padding: `0 ${uvars.sp1}`,
});

export const staleBanner = style({
  marginTop: uvars.sp1,
});

// ── Progress line ─────────────────────────────────────────────────────────────

export const progress = style({
  fontSize: uvars.textXs,
  color: vars.textSecondary,
  marginTop: uvars.sp1,
});

// ── Destructive destination control ──────────────────────────────────────────

export const destructive = style({
  marginTop: uvars.sp3,
  padding: uvars.sp3,
  background: vars.surfaceRaised,
  borderRadius: uvars.radiusMd,
});

export const destructiveTitle = style({
  fontSize: uvars.textXs,
  fontWeight: '600',
  marginBottom: uvars.sp2,
  color: vars.textSecondary,
});

export const destOptions = style({
  display: 'flex',
  flexDirection: 'column',
  gap: uvars.sp2,
});

export const destLabel = style({
  display: 'flex',
  alignItems: 'center',
  gap: uvars.sp2,
  cursor: 'pointer',
});

export const destLabelHint = style({
  display: 'block',
  fontSize: uvars.textXs,
  color: vars.textMuted,
});

// ── Root picker ───────────────────────────────────────────────────────────────

export const rootPicker = style({
  marginBottom: uvars.sp3,
  padding: uvars.sp3,
  border: `1px solid ${vars.warn}`,
  borderRadius: uvars.radiusMd,
  background: vars.surfaceRaised,
});

export const rootPickerTitle = style({
  fontSize: uvars.textSm,
  fontWeight: '600',
  marginBottom: uvars.sp1,
});

export const rootPickerDesc = style({
  fontSize: uvars.textXs,
  color: vars.textMuted,
  marginBottom: uvars.sp2,
});

export const rootPickerOptions = style({
  display: 'flex',
  flexDirection: 'column',
  gap: uvars.sp2,
});

export const rootOption = style({
  justifyContent: 'flex-start',
  textAlign: 'left',
});

export const rootOptionInner = style({
  display: 'flex',
  flexDirection: 'column',
  alignItems: 'flex-start',
});

export const rootOptionPath = style({
  fontSize: uvars.textXs,
});

export const rootOptionKind = style({
  fontSize: uvars.textXs,
  color: vars.textMuted,
});
