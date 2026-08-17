// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// Shell.tsx — app frame wrapper.

import { style } from '@vanilla-extract/css';

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
