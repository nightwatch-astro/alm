// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Shared-component guard — spec 054 T012a, issue #1067 (owner mandate: "the
 * panels should be completely shared components that the individual pages can
 * fill out with data").
 *
 * A static source check (in the `scripts/css-dup-sniff.mjs` spirit — cheaper
 * and just as effective as mounting all six real detail components with their
 * full data/IPC mocking, which would duplicate each feature's own test setup
 * for no extra regression coverage): every list-page detail component must
 * render through the shared `DetailPanel` (source contains a `<DetailPanel`
 * JSX usage) and must NOT render `<DetailHeader` directly — that is
 * `DetailPanel`'s own internal implementation detail, so a consumer using it
 * means it has bypassed the shared container.
 *
 * Note this deliberately does NOT forbid `DetailPane` (no "l"): the bare
 * primitive is still the correct choice for a loading/error early return,
 * which has no title to give `DetailPanel`'s required `title` prop. Several
 * of the files below use it for exactly that.
 *
 * Runtime confirmation that `DetailPanel`'s root actually carries its shared
 * structure lives in `DetailPanel.test.tsx`, which renders real instances.
 */

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const MIGRATED_DETAIL_FILES = [
  'src/features/sessions/SessionDetail.tsx',
  'src/features/calibration/MasterDetail.tsx',
  'src/features/inbox/InboxDetail.tsx',
  'src/features/archive/ArchiveDetail.tsx',
  'src/features/targets/TargetDetail.tsx',
  'src/features/projects/ProjectDetail.tsx',
];

describe('shared DetailPanel guard (spec 054 T012a)', () => {
  it.each(MIGRATED_DETAIL_FILES)(
    '%s renders through DetailPanel, not a raw DetailHeader',
    (relPath) => {
      const source = readFileSync(resolve(process.cwd(), relPath), 'utf8');
      expect(source).toMatch(/<DetailPanel\b/);
      expect(source).not.toMatch(/<DetailHeader\b/);
    },
  );
});
