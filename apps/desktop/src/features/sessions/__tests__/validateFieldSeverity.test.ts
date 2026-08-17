// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Unit tests for `validateFieldSeverity` — the client-side fast-feedback
 * validator for matching settings fields.
 *
 * Spec 062 FR-030: risky-but-valid = yellow; out-of-bounds = red.
 * FR-027/028: hard ranges and yellow thresholds per field.
 */

import { describe, it, expect } from 'vitest';
import {
  validateFieldSeverity,
  MATCHING_SETTINGS_BOUNDS,
} from '../useGroupsStore';

const ssC = MATCHING_SETTINGS_BOUNDS.sameSession.coverageMinPercent; // 90–99.5, yellow<93
const ssK = MATCHING_SETTINGS_BOUNDS.sameSession.centerSeparationMaxPercent; // 0.5–5, yellow>3
const ssR = MATCHING_SETTINGS_BOUNDS.sameSession.rotationMaxDeg; // 0.25–3, yellow>2

const sibC = MATCHING_SETTINGS_BOUNDS.sibling.coverageMinPercent; // 80–95, yellow<85
const sibR = MATCHING_SETTINGS_BOUNDS.sibling.rotationMaxDeg; // 1–15, yellow>10

const mosMin = MATCHING_SETTINGS_BOUNDS.mosaic.overlapMinPercent; // 1–20, yellow<3
const mosMax = MATCHING_SETTINGS_BOUNDS.mosaic.overlapMaxPercent; // 20–60, yellow>50

const dtMod = MATCHING_SETTINGS_BOUNDS.darkThermal.moderateDeg; // 0.1–2, yellow>1
const dtSev = MATCHING_SETTINGS_BOUNDS.darkThermal.severeDeg; // 0.5–5, yellow>3

const foN = MATCHING_SETTINGS_BOUNDS.flatOrientation.normalThroughDeg; // 0.5–5, yellow>3

const fa = MATCHING_SETTINGS_BOUNDS.flatAge.redAfterNights; // 7–365, yellow>90

describe('validateFieldSeverity — same-session coverage (90–99.5, yellow<93)', () => {
  it('returns ok at the default (95)', () => {
    expect(validateFieldSeverity(95, ssC)).toBe('ok');
  });

  it('returns yellow at the hard min (90) — below yellowBelow: 93', () => {
    // 90 is within [90, 99.5] so not red, but below yellowBelow: 93 → yellow
    expect(validateFieldSeverity(90, ssC)).toBe('yellow');
  });

  it('returns ok at exactly the yellow-below boundary (93) — at the threshold, not below it', () => {
    // yellowBelow means "below 93" → 93 itself is NOT yellow
    expect(validateFieldSeverity(93, ssC)).toBe('ok');
  });

  it('returns yellow one step below the yellow boundary (92.9)', () => {
    expect(validateFieldSeverity(92.9, ssC)).toBe('yellow');
  });

  it('returns yellow at 91 — in bounds but below yellowBelow threshold', () => {
    expect(validateFieldSeverity(91, ssC)).toBe('yellow');
  });

  it('returns red below the hard min (89.9)', () => {
    expect(validateFieldSeverity(89.9, ssC)).toBe('red');
  });

  it('returns red above the hard max (99.6)', () => {
    expect(validateFieldSeverity(99.6, ssC)).toBe('red');
  });

  it('returns ok at the hard max (99.5)', () => {
    expect(validateFieldSeverity(99.5, ssC)).toBe('ok');
  });
});

describe('validateFieldSeverity — same-session centre separation (0.5–5, yellow>3)', () => {
  it('returns ok at default (2)', () => {
    expect(validateFieldSeverity(2, ssK)).toBe('ok');
  });

  it('returns ok exactly at yellow-above boundary (3)', () => {
    expect(validateFieldSeverity(3, ssK)).toBe('ok');
  });

  it('returns yellow above yellow-above boundary (3.1)', () => {
    expect(validateFieldSeverity(3.1, ssK)).toBe('yellow');
  });

  it('returns yellow at the hard max (5)', () => {
    expect(validateFieldSeverity(5, ssK)).toBe('yellow');
  });

  it('returns red above hard max (5.1)', () => {
    expect(validateFieldSeverity(5.1, ssK)).toBe('red');
  });

  it('returns red below hard min (0.4)', () => {
    expect(validateFieldSeverity(0.4, ssK)).toBe('red');
  });
});

describe('validateFieldSeverity — sibling coverage (80–95, yellow<85)', () => {
  it('returns yellow at exactly yellow-below boundary (85)', () => {
    // yellowBelow: below 85 = yellow, so exactly 85 is NOT yellow
    expect(validateFieldSeverity(85, sibC)).toBe('ok');
  });

  it('returns yellow one step below (84.9)', () => {
    expect(validateFieldSeverity(84.9, sibC)).toBe('yellow');
  });

  it('returns red below hard min (79.9)', () => {
    expect(validateFieldSeverity(79.9, sibC)).toBe('red');
  });
});

describe('validateFieldSeverity — sibling rotation (1–15, yellow>10)', () => {
  it('returns ok at default (5)', () => {
    expect(validateFieldSeverity(5, sibR)).toBe('ok');
  });

  it('returns ok at yellow-above boundary (10)', () => {
    expect(validateFieldSeverity(10, sibR)).toBe('ok');
  });

  it('returns yellow above yellow-above (10.1)', () => {
    expect(validateFieldSeverity(10.1, sibR)).toBe('yellow');
  });

  it('returns red at hard min boundary minus one (0.9)', () => {
    expect(validateFieldSeverity(0.9, sibR)).toBe('red');
  });
});

describe('validateFieldSeverity — mosaic overlap', () => {
  it('mosaic min yellow below 3 (2.9 → yellow)', () => {
    expect(validateFieldSeverity(2.9, mosMin)).toBe('yellow');
  });

  it('mosaic min ok at 3', () => {
    expect(validateFieldSeverity(3, mosMin)).toBe('ok');
  });

  it('mosaic max yellow above 50 (50.1 → yellow)', () => {
    expect(validateFieldSeverity(50.1, mosMax)).toBe('yellow');
  });

  it('mosaic max ok at 50', () => {
    expect(validateFieldSeverity(50, mosMax)).toBe('ok');
  });
});

describe('validateFieldSeverity — dark thermal', () => {
  it('moderate yellow above 1 (1.1 → yellow)', () => {
    expect(validateFieldSeverity(1.1, dtMod)).toBe('yellow');
  });

  it('severe yellow above 3 (3.1 → yellow)', () => {
    expect(validateFieldSeverity(3.1, dtSev)).toBe('yellow');
  });

  it('moderate red above hard max (2.1)', () => {
    expect(validateFieldSeverity(2.1, dtMod)).toBe('red');
  });
});

describe('validateFieldSeverity — flat orientation', () => {
  it('normal yellow above 3 (3.1 → yellow)', () => {
    expect(validateFieldSeverity(3.1, foN)).toBe('yellow');
  });

  it('red above hard max 5 (5.1 → red)', () => {
    expect(validateFieldSeverity(5.1, foN)).toBe('red');
  });
});

describe('validateFieldSeverity — flat age', () => {
  it('ok at default (7)', () => {
    expect(validateFieldSeverity(7, fa)).toBe('ok');
  });

  it('yellow above 90 (91 → yellow)', () => {
    expect(validateFieldSeverity(91, fa)).toBe('yellow');
  });

  it('ok at 90', () => {
    expect(validateFieldSeverity(90, fa)).toBe('ok');
  });

  it('red above 365 (366 → red)', () => {
    expect(validateFieldSeverity(366, fa)).toBe('red');
  });

  it('red below 7 (6 → red)', () => {
    expect(validateFieldSeverity(6, fa)).toBe('red');
  });
});

// ── Boundary proof for same-session rotation (0.25–3, yellow>2) ───────────────
describe('validateFieldSeverity — same-session rotation (0.25–3, yellow>2)', () => {
  it('ok at default (1)', () => {
    expect(validateFieldSeverity(1, ssR)).toBe('ok');
  });

  it('ok at yellow-above boundary (2)', () => {
    expect(validateFieldSeverity(2, ssR)).toBe('ok');
  });

  it('yellow above boundary (2.01)', () => {
    expect(validateFieldSeverity(2.01, ssR)).toBe('yellow');
  });

  it('red at 0.24 (below hard min 0.25)', () => {
    expect(validateFieldSeverity(0.24, ssR)).toBe('red');
  });

  it('ok at hard min (0.25)', () => {
    expect(validateFieldSeverity(0.25, ssR)).toBe('ok');
  });
});
