// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// Shared page layout primitives — consumed by PageShell, ListPageLayout,
// ListDetailLayout, PageTopBar, SetupPage, SetupWizard, WizardPage,
// ContractsPage, DevSettingsPage.
// These are primitives (layout scaffolding used by every page) per
// the three-layer architecture: token → primitive → component.

import { style } from '@vanilla-extract/css';

export const page = style({
  display: 'flex',
  flexDirection: 'column',
  flex: 1,
  minHeight: 0,
  height: '100%',
});
export const pageBar = style({ flexShrink: 0 });
export const pageScroll = style({ flex: 1, minHeight: 0, overflowY: 'auto' });
export const virtualScroll = style({ position: 'relative' });
export const virtualInner = style({ width: '100%' });
