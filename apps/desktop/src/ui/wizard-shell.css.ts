// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for WizardShell — migrates the pv-wizard root
 * from wizard-base.css. The wizard step/layout classes live in wizard-steps.css
 * and will migrate with their own wave (kyo7.103).
 */

import { style } from '@vanilla-extract/css';

/** Root wrapper: flex column filling available height. */
export const wizardRoot = style({
  display: 'flex',
  flexDirection: 'column',
  flex: 1,
  minHeight: 0,
});
