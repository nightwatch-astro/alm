// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for dev-tools surfaces (feature-gated, VITE_DEV_TOOLS=true).
 * Replaces the dev.css sections for pv-dev-calls-*, pv-dev-contracts-list-*,
 * pv-dev-schema-*, and pv-dev-contracts-page-*.
 * pv-adv-settings-* migrates to src/features/settings/advanced.css.ts.
 */

import { style, styleVariants } from '@vanilla-extract/css';
import { vars } from '@/styles/themes.css';

// ── DEV CALL LIST ───────────────────────────────────────────────────────────

export const callsEmpty = style({
  fontSize: 'var(--pv-text-sm)',
  color: vars.textMuted,
});

export const callsTable = style({
  width: '100%',
  borderCollapse: 'collapse',
  fontSize: 'var(--pv-text-xs)',
});

export const callsTheadRow = style({
  borderBottom: `1px solid ${vars.border}`,
  textAlign: 'left',
});

export const callsTh = style({
  padding: 'var(--pv-sp-1) var(--pv-sp-2)',
});

export const callsRow = style({
  borderBottom: `1px solid ${vars.borderSubtle}`,
});

export const callsTd = style({
  padding: 'var(--pv-sp-1) var(--pv-sp-2)',
});

export const callsTdId = style({
  padding: 'var(--pv-sp-1) var(--pv-sp-2)',
  fontFamily: vars.fontMono,
  color: vars.textMuted,
  fontSize: 'var(--pv-text-xs)',
});

export const callsTdContract = style({
  padding: 'var(--pv-sp-1) var(--pv-sp-2)',
  fontFamily: vars.fontMono,
});

export const callsTdStarted = style({
  padding: 'var(--pv-sp-1) var(--pv-sp-2)',
  color: vars.textMuted,
});

export const callsTdActions = style({
  padding: 'var(--pv-sp-1) var(--pv-sp-2)',
  display: 'flex',
  gap: 'var(--pv-sp-1)',
});

export const callsTruncated = style({
  marginLeft: 'var(--pv-sp-1)',
  color: vars.warn,
});

export const callsOutcomeVariants = styleVariants({
  error: { color: vars.danger },
  ok: { color: vars.ok },
});

export const callsReplayBtnVariants = styleVariants({
  safe: { opacity: 1, cursor: 'pointer' },
  unsafe: { opacity: 0.4, cursor: 'not-allowed' },
});

// ── DEV CONTRACT LIST ───────────────────────────────────────────────────────

export const contractsListEmpty = style({
  fontSize: 'var(--pv-text-sm)',
  color: vars.textMuted,
});

export const contractsListTable = style({
  width: '100%',
  borderCollapse: 'collapse',
  fontSize: 'var(--pv-text-xs)',
});

export const contractsListTheadRow = style({
  borderBottom: `1px solid ${vars.border}`,
  textAlign: 'left',
});

export const contractsListTh = style({
  padding: 'var(--pv-sp-1) var(--pv-sp-2)',
});

export const contractsListRow = style({
  borderBottom: `1px solid ${vars.borderSubtle}`,
});

export const contractsListTd = style({
  padding: 'var(--pv-sp-1) var(--pv-sp-2)',
});

export const contractsListTdName = style({
  padding: 'var(--pv-sp-1) var(--pv-sp-2)',
  fontFamily: vars.fontMono,
});

export const contractsListTdMuted = style({
  padding: 'var(--pv-sp-1) var(--pv-sp-2)',
  color: vars.textMuted,
});

export const contractsListTdCenter = style({
  padding: 'var(--pv-sp-1) var(--pv-sp-2)',
  textAlign: 'center',
});

export const contractsListTdSchema = style({
  padding: 'var(--pv-sp-1) var(--pv-sp-2)',
  fontFamily: vars.fontMono,
  maxWidth: '280px',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
  color: vars.textMuted,
});

export const contractsListMismatchIcon = style({
  color: vars.warn,
  marginRight: 'var(--pv-sp-1)',
});

export const contractsListReplayOk = style({ color: vars.ok });

export const contractsListReplayNa = style({ color: vars.textMuted });

// ── DEV SCHEMA VIEWER ───────────────────────────────────────────────────────

export const schemaOverlay = style({
  position: 'fixed',
  inset: 0,
  background: `color-mix(in srgb, ${vars.ink} 50%, transparent)`,
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  zIndex: 1000,
});

export const schemaPanel = style({
  background: vars.surface,
  border: `1px solid ${vars.border}`,
  borderRadius: 'var(--pv-radius-md)',
  width: '80vw',
  maxWidth: '900px',
  maxHeight: '80vh',
  display: 'flex',
  flexDirection: 'column',
  padding: 'var(--pv-sp-4)',
  gap: 'var(--pv-sp-3)',
});

export const schemaHeader = style({
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
});

export const schemaName = style({
  fontFamily: vars.fontMono,
  fontWeight: 'var(--pv-weight-semibold)',
});

export const schemaVersion = style({
  color: vars.textMuted,
  marginLeft: 'var(--pv-sp-2)',
  fontSize: 'var(--pv-text-xs)',
});

export const schemaActions = style({
  display: 'flex',
  gap: 'var(--pv-sp-2)',
});

export const schemaBody = style({
  flex: 1,
  overflow: 'auto',
  minHeight: 0,
});

export const schemaMissing = style({
  color: vars.danger,
  padding: 'var(--pv-sp-4)',
  fontSize: 'var(--pv-text-sm)',
});

export const schemaMissingPath = style({ marginTop: 'var(--pv-sp-1)' });

export const schemaMissingCode = style({
  fontFamily: vars.fontMono,
  fontSize: '0.8em',
});

export const schemaLoading = style({
  color: vars.textMuted,
  padding: 'var(--pv-sp-4)',
});

export const schemaPre = style({
  margin: 0,
  padding: 'var(--pv-sp-2)',
  fontSize: 'var(--pv-text-xs)',
  lineHeight: 'var(--pv-leading-normal)',
  whiteSpace: 'pre-wrap',
  wordBreak: 'break-all',
  background: vars.bg3,
  borderRadius: 'var(--pv-radius-sm)',
});

// ── DEV CONTRACTS PAGE ──────────────────────────────────────────────────────

export const pageStubBody = style({
  padding: 'var(--pv-sp-7)',
  textAlign: 'center',
  color: vars.textMuted,
});

export const pageStubHeading = style({
  fontSize: 'var(--pv-text-lg)',
  marginBottom: 'var(--pv-sp-2)',
});

export const pageStubText = style({ fontSize: 'var(--pv-text-sm)' });

export const pageLoading = style({
  padding: 'var(--pv-sp-7)',
  color: vars.textMuted,
});

export const pageBody = style({
  padding: 'var(--pv-sp-4)',
  display: 'flex',
  flexDirection: 'column',
  gap: 'var(--pv-sp-4)',
});

export const pageHeader = style({
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  borderBottom: `1px solid ${vars.border}`,
  paddingBottom: 'var(--pv-sp-3)',
});

export const pageTitle = style({
  fontSize: 'var(--pv-text-lg)',
  fontWeight: 'var(--pv-weight-semibold)',
});

export const pageActions = style({
  display: 'flex',
  gap: 'var(--pv-sp-2)',
});

export const pageError = style({
  color: vars.danger,
  fontSize: 'var(--pv-text-sm)',
});

export const pageExportResult = style({
  fontSize: 'var(--pv-text-sm)',
  color: vars.textMuted,
});

export const pageSectionHeading = style({
  fontSize: 'var(--pv-text-md)',
  fontWeight: 'var(--pv-weight-semibold)',
  marginBottom: 'var(--pv-sp-2)',
});
