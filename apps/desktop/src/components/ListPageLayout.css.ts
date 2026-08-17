// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for ListPageLayout — replaces .pv-listpage* +
 * .pv-resize-handle* + .pv-dock-placement-control* + .pv-listtable* +
 * .pv-densetable* + .pv-sessions-table* + shared cell helpers in tables-lists.css.
 *
 * Consumers: ListPageLayout.tsx, DetailPanel.tsx, DetailDockPlacementControl.tsx,
 * and feature tables (Sessions, Archive, etc.).
 */

import { globalStyle, style } from '@vanilla-extract/css';
import { uvars, vars } from '@/styles/themes.css';

// ── List-page body ────────────────────────────────────────────────────────────

export const body = style({
  display: 'flex',
  flexDirection: 'column',
  flex: 1,
  minHeight: 0,
  overflow: 'hidden',
});

export const main = style({
  flex: '1 1 auto',
  minWidth: 0,
  minHeight: 0,
  overflow: 'auto',
});

export const detail = style({
  flex: '0 0 auto',
  height: 'fit-content',
  maxHeight: 'clamp(220px, 40vh, 52vh)',
  display: 'flex',
  flexDirection: 'column',
  minHeight: 0,
  overflow: 'hidden',
  borderTop: `1px solid ${vars.border}`,
  background: vars.surfaceRaised,
});

export const detailSide = style({
  position: 'relative',
});

export const detailBar = style({
  flexShrink: 0,
  display: 'flex',
  alignItems: 'center',
  gap: uvars.sp1,
  justifyContent: 'flex-end',
  padding: `${uvars.sp1} ${uvars.sp2}`,
  borderBottom: `1px solid ${vars.borderSubtle}`,
});

export const detailClose = style({
  background: 'none',
  border: 'none',
  cursor: 'pointer',
  color: vars.textMuted,
  fontSize: uvars.textSm,
  lineHeight: 1,
  padding: uvars.sp1,
  borderRadius: uvars.radiusSm,
  selectors: { '&:hover': { color: vars.text } },
});

export const detailBody = style({
  flex: 1,
  minHeight: 0,
  overflow: 'hidden',
});

// Side-variant panels (CSS custom properties drive width).
export const bodySide = style({
  vars: { '--pv-side-detail-w': '420px' },
  flexDirection: 'row',
});

export const detailSidePanel = style({
  flex: '0 0 var(--pv-side-detail-w, 420px)',
  width: 'var(--pv-side-detail-w, 420px)',
  height: 'auto',
  maxHeight: 'none',
  alignSelf: 'stretch',
  borderTop: 'none',
  borderLeft: `1px solid ${vars.border}`,
});

// Dual side+bottom panels.
export const bodyDual = style({ flexDirection: 'row' });

export const mainCol = style({
  flex: '1 1 0',
  minWidth: 0,
  display: 'flex',
  flexDirection: 'column',
  minHeight: 0,
  overflow: 'hidden',
});

export const sidePanel = style({
  flex: '0 0 var(--pv-side-detail-w, 420px)',
  width: 'var(--pv-side-detail-w, 420px)',
  height: 'auto',
  maxHeight: 'none',
  alignSelf: 'stretch',
  borderLeft: `1px solid ${vars.border}`,
  background: vars.surfaceRaised,
  display: 'flex',
  flexDirection: 'column',
  overflow: 'hidden',
});

export const bottomPanel = style({
  flex: '0 0 auto',
  height: 'fit-content',
  maxHeight: '40vh',
  display: 'flex',
  flexDirection: 'column',
  minHeight: 0,
  overflow: 'hidden',
  borderTop: `1px solid ${vars.border}`,
  background: vars.surfaceRaised,
});

export const panelBar = style({
  flexShrink: 0,
  display: 'flex',
  justifyContent: 'flex-end',
  padding: `${uvars.sp1} ${uvars.sp2}`,
  borderBottom: `1px solid ${vars.borderSubtle}`,
});

export const panelClose = style({
  background: 'none',
  border: 'none',
  cursor: 'pointer',
  color: vars.textMuted,
  fontSize: uvars.textSm,
  lineHeight: 1,
  padding: uvars.sp1,
  borderRadius: uvars.radiusSm,
  selectors: { '&:hover': { color: vars.text } },
});

export const panelBody = style({
  flex: 1,
  minHeight: 0,
  overflowY: 'auto',
});

// Detail-body flex chain corrections (#1107).
globalStyle(`${detailBody} > .pv-detail`, {
  display: 'flex',
  flexDirection: 'column',
  height: '100%',
  minHeight: 0,
});

globalStyle(`${detailBody} .pv-detail__header`, { flexShrink: 0 });

// ── Resize handle ─────────────────────────────────────────────────────────────

export const resizeHandle = style({
  position: 'absolute',
  top: 0,
  left: '-3px',
  width: '6px',
  height: '100%',
  cursor: 'col-resize',
  zIndex: 1,
  touchAction: 'none',
  selectors: {
    '&:hover, &:active': { background: vars.accent, opacity: 0.4 },
  },
});

// ── Dock placement control ────────────────────────────────────────────────────
// Icon-only 28px buttons for the Auto/Bottom/Right placement SegControl.

export const dockPlacementControl = style({});

globalStyle(`${dockPlacementControl} .pv-seg__btn`, {
  width: '28px',
  height: '28px',
  padding: 0,
  display: 'inline-flex',
  alignItems: 'center',
  justifyContent: 'center',
});

globalStyle(`${dockPlacementControl}.pv-seg`, { height: '28px' });

// ── Dense sortable table (sessions + generic densetable) ─────────────────────

export const denseTable = style({ tableLayout: 'auto' });

globalStyle(`${denseTable} th`, { userSelect: 'none' });

globalStyle(`${denseTable} td, ${denseTable} th`, {
  padding: `${uvars.sp1} ${uvars.sp3}`,
});

export const denseTableRowBase = style({ cursor: 'pointer' });

globalStyle(`${denseTable} ${denseTableRowBase}:hover td`, {
  background: vars.surface,
});

export const denseTableRowSelected = style({});

globalStyle(`${denseTable} ${denseTableRowSelected} td`, {
  background: vars.accentBg,
});

// ── Shared table-cell helpers ─────────────────────────────────────────────────

export const cellNum = style({
  fontVariantNumeric: 'tabular-nums',
  textAlign: 'right',
});

export const cellMono = style({
  fontFamily: vars.fontMono,
  fontSize: uvars.textXs,
  color: vars.textSecondary,
});

export const cellMuted = style({ color: vars.textMuted });

export const sessionsTargetCell = style({
  fontWeight: uvars.weightMedium,
  color: vars.text,
});

export const sessionsProjectsCell = style({
  display: 'flex',
  flexWrap: 'wrap',
  gap: uvars.sp1,
});

export const sessionsWarnIcon = style({
  color: vars.textMuted,
  marginRight: uvars.sp1,
  verticalAlign: 'middle',
});

// ── Shared list-table viewport ────────────────────────────────────────────────

export const listTable = style({
  display: 'flex',
  flexDirection: 'column',
  flex: 1,
  minHeight: 0,
  height: '100%',
});

export const listTableScroll = style({ flex: 1 });

export const listTableFoot = style({
  flexShrink: 0,
  padding: `${uvars.sp1} ${uvars.sp3}`,
  borderTop: `1px solid ${vars.border}`,
  fontSize: uvars.textXs,
  color: vars.textMuted,
});

export const listTableEmpty = style({
  padding: uvars.sp4,
  color: vars.textMuted,
  fontSize: uvars.textSm,
});
