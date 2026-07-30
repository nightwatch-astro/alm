// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// ListDetailLayout.tsx — two-pane and three-pane layout containers.

import { style } from '@vanilla-extract/css';
import { vars } from '@/styles/themes.css';

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
