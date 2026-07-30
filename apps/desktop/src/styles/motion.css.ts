// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Shared animations. `spin` replaces the `@keyframes pv-spin` that lived in the
 * migrated app-shell.css; the `pv-spin` global alias exists only so the
 * un-migrated `components/wizard-steps.css` rule keeps animating, and is
 * removable once that sheet moves to vanilla-extract.
 */

import { globalKeyframes, keyframes } from '@vanilla-extract/css';

const spinFrames = { to: { transform: 'rotate(360deg)' } };

export const spin = keyframes(spinFrames);

globalKeyframes('pv-spin', spinFrames);
