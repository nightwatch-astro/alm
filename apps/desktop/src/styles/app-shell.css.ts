// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for the app frame and page layout primitives.
 * Replaces app-shell.css (353L).
 *
 * Layout token glossary:
 *   frame      — outermost flex column (100% viewport)
 *   page       — fills the bounded content area (flex column, height:100%)
 *   pane       — two/three-column layouts, pane content regions
 */

import { style, styleVariants } from '@vanilla-extract/css';
import { vars } from '@/styles/themes.css';

// ── APP FRAME ────────────────────────────────────────────────────────────────

export const frame = style({
  display: 'flex',
  flexDirection: 'column',
  height: '100%',
});
export const frameBody = style({ display: 'flex', flex: 1, minHeight: 0 });
export const frameMain = style({
  display: 'flex',
  flexDirection: 'column',
  flex: 1,
  minWidth: 0,
  minHeight: 0,
});

// ── PAGE LAYOUT PRIMITIVES ───────────────────────────────────────────────────

export const page = style({
  display: 'flex',
  flexDirection: 'column',
  flex: 1,
  minHeight: 0,
  height: '100%',
});
export const pageBar = style({ flexShrink: 0 });
export const pageScroll = style({ flex: 1, minHeight: 0, overflowY: 'auto' });

// ── STATUS BAR ───────────────────────────────────────────────────────────────

export const statusBar = style({
  height: 'var(--pv-statusbar-height)',
  background: vars.surface,
  borderTop: `1px solid ${vars.border}`,
  display: 'flex',
  alignItems: 'center',
  padding: '0 var(--pv-sp-3)',
  fontSize: 'var(--pv-text-xs)',
  color: vars.textMuted,
  gap: 'var(--pv-sp-4)',
  flexShrink: 0,
});

export const statusBarSep = style({ color: vars.rule });
export const statusBarRight = style({
  marginLeft: 'auto',
  display: 'flex',
  gap: 'var(--pv-sp-3)',
  alignItems: 'center',
});
export const statusBarOp = style({
  display: 'flex',
  alignItems: 'center',
  gap: 'var(--pv-sp-2)',
});
export const statusBarIdle = style({ color: vars.textFaint });

export const statusBarSpinner = style({
  width: '9px',
  height: '9px',
  borderRadius: '50%',
  border: `2px solid ${vars.accent}`,
  borderTopColor: 'transparent',
  animation: 'pv-spin 1s linear infinite',
});

export const statusBarVol = style({
  display: 'flex',
  alignItems: 'center',
  gap: 'var(--pv-sp-1)',
});
export const statusBarVolWarn = style({ color: vars.warn });

export const statusBarMeter = style({
  width: '40px',
  height: '5px',
  borderRadius: '3px',
  background: vars.bg3,
  overflow: 'hidden',
});

export const statusBarMeterFill = style({
  display: 'block',
  height: '100%',
  background: vars.ok,
});
export const statusBarMeterFillWarn = style({
  display: 'block',
  height: '100%',
  background: vars.warn,
});

export const statusBarLogToggle = style({
  border: `1px solid ${vars.border}`,
  borderRadius: 'var(--pv-radius-sm)',
  padding: '0 var(--pv-sp-2)',
  height: '16px',
  background: vars.bg,
  cursor: 'pointer',
  color: vars.textSecondary,
  fontSize: 'var(--pv-text-xs)',
});

// ── SIDEBAR ──────────────────────────────────────────────────────────────────

export const sidebar = style({
  width: 'var(--pv-sidebar-width)',
  background: vars.surface,
  borderRight: `1px solid ${vars.border}`,
  display: 'flex',
  flexDirection: 'column',
  flexShrink: 0,
  transition: 'width var(--pv-transition-slow)',
  overflow: 'hidden',
});

export const sidebarCollapsed = style({ width: 'var(--pv-sidebar-collapsed)' });

export const sidebarHeader = style({
  padding: 'var(--pv-sp-3)',
  display: 'flex',
  alignItems: 'center',
  gap: 'var(--pv-sp-2)',
  minHeight: '44px',
});

export const sidebarBrand = style({
  display: 'flex',
  flexDirection: 'column',
  minWidth: 0,
  flex: 1,
});

export const sidebarBrandLabel = style({
  fontSize: 'var(--pv-text-xs)',
  color: vars.textMuted,
  letterSpacing: 'var(--pv-tracking-wide)',
  textTransform: 'uppercase',
  fontWeight: 'var(--pv-weight-medium)',
  whiteSpace: 'nowrap',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
});

export const sidebarBrandName = style({
  fontSize: 'var(--pv-text-md)',
  fontWeight: 'var(--pv-weight-semibold)',
  whiteSpace: 'nowrap',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  flex: 1,
  flexShrink: 0,
});

export const sidebarCollapse = style({
  width: '24px',
  height: '24px',
  border: 'none',
  background: 'none',
  cursor: 'pointer',
  color: vars.textMuted,
  borderRadius: 'var(--pv-radius-sm)',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  flexShrink: 0,
  marginLeft: 'auto',
  selectors: { '&:hover': { background: vars.hoverBg } },
});

export const sidebarNav = style({
  flex: 1,
  padding: 'var(--pv-sp-1) 0',
  overflowY: 'auto',
});

export const sidebarItem = style({
  display: 'flex',
  alignItems: 'center',
  gap: 'var(--pv-sp-2)',
  padding: '6px var(--pv-sp-3)',
  margin: '1px var(--pv-sp-1)',
  cursor: 'pointer',
  color: vars.textSecondary,
  borderRadius: 'var(--pv-radius-sm)',
  fontSize: 'var(--pv-text-sm)',
  fontWeight: 'var(--pv-weight-medium)',
  transition:
    'background var(--pv-transition-fast), color var(--pv-transition-fast)',
  whiteSpace: 'nowrap',
  overflow: 'hidden',
  border: 'none',
  background: 'none',
  width: 'calc(100% - var(--pv-sp-1) * 2)',
  textAlign: 'left',
  selectors: {
    '&:hover': { background: vars.hoverBg, color: vars.text },
    '&:focus-visible': { outline: 'none', boxShadow: vars.focusRing },
  },
});

export const sidebarItemActive = style({
  background: vars.selectedBg,
  color: vars.accentText,
  fontWeight: 'var(--pv-weight-semibold)',
});

export const sidebarItemIcon = style({
  width: '18px',
  height: '18px',
  flexShrink: 0,
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
});

export const sidebarItemLabel = style({
  flex: 1,
  overflow: 'hidden',
  textOverflow: 'ellipsis',
});

export const sidebarItemBadge = style({
  fontSize: 'var(--pv-text-xs)',
  color: vars.textMuted,
  fontVariantNumeric: 'tabular-nums',
  minWidth: '20px',
  textAlign: 'right',
});

export const sidebarItemBadgeAlert = style({
  color: vars.onAccent,
  background: vars.accent,
  borderRadius: '8px',
  padding: '0 5px',
  minWidth: '18px',
  textAlign: 'center',
});

export const sidebarFooter = style({
  padding: 'var(--pv-sp-2) var(--pv-sp-3)',
  borderTop: `1px solid ${vars.borderSubtle}`,
  fontSize: 'var(--pv-text-xs)',
  color: vars.textFaint,
  whiteSpace: 'nowrap',
  overflow: 'hidden',
});

export const sidebarMark = style({
  width: '22px',
  height: '22px',
  borderRadius: 'var(--pv-radius-sm)',
  background: `linear-gradient(135deg, ${vars.accent}, ${vars.accentDeep})`,
  color: vars.onAccent,
  fontSize: 'var(--pv-text-xs)',
  fontWeight: 'var(--pv-weight-semibold)',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  flexShrink: 0,
});

export const sidebarVersion = style({
  fontSize: 'var(--pv-text-xs)',
  color: vars.textFaint,
});
export const sidebarGroup = style({ paddingBottom: 'var(--pv-sp-1)' });

export const sidebarGroupLabel = style({
  fontSize: 'var(--pv-text-xs)',
  fontWeight: 'var(--pv-weight-semibold)',
  letterSpacing: 'var(--pv-tracking-wide)',
  textTransform: 'uppercase',
  color: vars.textFaint,
  padding: 'var(--pv-sp-3) var(--pv-sp-3) var(--pv-sp-1)',
});

export const sidebarSettings = style({
  borderTop: `1px solid ${vars.borderSubtle}`,
  padding: 'var(--pv-sp-1) 0',
});

// ── TOP ACTION BAR ───────────────────────────────────────────────────────────

export const actionBar = style({
  height: 'var(--pv-toolbar-height)',
  borderBottom: `1px solid ${vars.border}`,
  display: 'flex',
  alignItems: 'center',
  padding: '0 var(--pv-sp-4)',
  gap: 'var(--pv-sp-3)',
  flexShrink: 0,
  background: vars.bg,
});

export const actionBarTitle = style({
  fontSize: 'var(--pv-text-md)',
  fontWeight: 'var(--pv-weight-semibold)',
});
export const actionBarSubtitle = style({
  fontSize: 'var(--pv-text-xs)',
  color: vars.textMuted,
});
export const actionBarSpacer = style({ flex: 1 });
export const actionBarActions = style({
  display: 'flex',
  gap: 'var(--pv-sp-2)',
  alignItems: 'center',
});

// ── LIST SIDEBAR ─────────────────────────────────────────────────────────────

export const listSidebar = style({
  width: 'var(--pv-list-width)',
  flexShrink: 0,
  display: 'flex',
  flexDirection: 'column',
  borderRight: `1px solid ${vars.border}`,
  background: vars.bg,
  minWidth: '220px',
});

export const listSidebarSearch = style({
  padding: 'var(--pv-sp-2) var(--pv-sp-3)',
  borderBottom: `1px solid ${vars.borderSubtle}`,
});

export const listSidebarControls = style({
  padding: 'var(--pv-sp-2) var(--pv-sp-3)',
  display: 'flex',
  flexDirection: 'column',
  gap: 'var(--pv-sp-1)',
  borderBottom: `1px solid ${vars.borderSubtle}`,
});

export const listSidebarList = style({
  flex: 1,
  overflowY: 'auto',
  position: 'relative',
});
export const listSidebarListVirtual = style({ flex: 1, position: 'relative' });

export const listSidebarFooter = style({
  padding: 'var(--pv-sp-1) var(--pv-sp-3)',
  borderTop: `1px solid ${vars.borderSubtle}`,
  fontSize: 'var(--pv-text-xs)',
  color: vars.textFaint,
});

// ── VIRTUALIZED LISTS ────────────────────────────────────────────────────────

export const virtualScroll = style({ position: 'relative' });
export const virtualInner = style({ width: '100%' });

// ── LIST ITEM ────────────────────────────────────────────────────────────────

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

export const listItemTitle = style({
  display: 'flex',
  alignItems: 'center',
  gap: 'var(--pv-sp-1)',
  fontSize: 'var(--pv-text-sm)',
  fontWeight: 'var(--pv-weight-medium)',
  color: vars.text,
});

export const listItemMeta = style({
  display: 'flex',
  alignItems: 'center',
  gap: 'var(--pv-sp-1)',
  fontSize: 'var(--pv-text-xs)',
  color: vars.textMuted,
  fontVariantNumeric: 'tabular-nums',
});

export const listItemMetaSep = style({ color: vars.rule });

// ── GROUP HEADER ─────────────────────────────────────────────────────────────

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

// ── TWO-PANE LAYOUT ──────────────────────────────────────────────────────────

export const twoPane = style({
  display: 'flex',
  flex: 1,
  minHeight: 0,
  overflow: 'hidden',
});
export const twoPaneDetail = style({ flex: 1, overflowY: 'auto', minWidth: 0 });

export const inboxCenter = style({
  display: 'flex',
  flexDirection: 'column',
  height: '100%',
  flex: 1,
  minHeight: 0,
  overflow: 'hidden',
});
export const inboxCenterDetail = style({
  flex: '1 1 auto',
  minHeight: 0,
  overflow: 'auto',
});
export const inboxCenterPlans = style({
  flexShrink: 0,
  borderTop: `1px solid ${vars.border}`,
  paddingTop: 'var(--pv-sp-2)',
  maxHeight: '40vh',
  overflow: 'auto',
});

// ── THREE-PANE LAYOUT ────────────────────────────────────────────────────────

export const threePane = style({
  display: 'flex',
  flex: 1,
  minHeight: 0,
  overflow: 'hidden',
});
export const threePaneContent = style({
  flex: 1,
  overflowY: 'auto',
  minWidth: 0,
});
export const threePaneSidebar = style({
  width: 'var(--pv-action-sidebar-width)',
  flexShrink: 0,
  overflowY: 'auto',
  borderLeft: `1px solid ${vars.border}`,
  background: vars.surface,
  padding: 'var(--pv-sp-4)',
});

// ── DETAIL PANE ──────────────────────────────────────────────────────────────

export const detail = style({ padding: 'var(--pv-sp-4)' });

export const detailHeader = style({
  display: 'flex',
  alignItems: 'flex-start',
  gap: 'var(--pv-sp-3)',
  marginBottom: 'var(--pv-sp-4)',
  paddingBottom: 'var(--pv-sp-3)',
  borderBottom: `1px solid ${vars.borderSubtle}`,
  flexWrap: 'wrap',
});

export const detailHeaderContent = style({ flex: 1, minWidth: '200px' });

export const detailTitle = style({
  fontSize: 'var(--pv-text-lg)',
  fontWeight: 'var(--pv-weight-semibold)',
  display: 'flex',
  alignItems: 'center',
  gap: 'var(--pv-sp-2)',
  lineHeight: 'var(--pv-leading-tight)',
  flexWrap: 'wrap',
});

export const detailSubtitle = style({
  fontFamily: vars.fontMono,
  fontSize: 'var(--pv-text-xs)',
  color: vars.textMuted,
  marginTop: 'var(--pv-sp-1)',
  wordBreak: 'break-all',
});

export const detailActions = style({
  display: 'flex',
  gap: 'var(--pv-sp-2)',
  flexShrink: 0,
  flexWrap: 'wrap',
});

export const detailStats = style({
  display: 'flex',
  gap: 'var(--pv-sp-4)',
  marginBottom: 'var(--pv-sp-4)',
  flexWrap: 'wrap',
});

export const detailStat = style({
  display: 'flex',
  flexDirection: 'column',
  gap: '1px',
});

export const detailStatValue = style({
  fontSize: 'var(--pv-text-md)',
  fontWeight: 'var(--pv-weight-semibold)',
  fontVariantNumeric: 'tabular-nums',
});

export const detailStatLabel = style({
  fontSize: 'var(--pv-text-xs)',
  color: vars.textMuted,
  textTransform: 'uppercase',
  letterSpacing: 'var(--pv-tracking-normal)',
  fontWeight: 'var(--pv-weight-medium)',
});

// ── EMPTY STATE ──────────────────────────────────────────────────────────────

export const empty = style({
  display: 'flex',
  flexDirection: 'column',
  alignItems: 'center',
  justifyContent: 'center',
  height: '100%',
  color: vars.textMuted,
  gap: 'var(--pv-sp-2)',
  padding: 'var(--pv-sp-7)',
});

export const emptyTitle = style({
  fontSize: 'var(--pv-text-md)',
  fontWeight: 'var(--pv-weight-medium)',
  color: vars.textSecondary,
});

export const emptyDesc = style({
  fontSize: 'var(--pv-text-sm)',
  textAlign: 'center',
  maxWidth: '300px',
});
