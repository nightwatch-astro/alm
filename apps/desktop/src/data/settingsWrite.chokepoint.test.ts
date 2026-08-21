// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * settingsWrite.chokepoint.test.ts — astro-plan-cia8.
 *
 * `settingsQueryOptions` caches every scope read with `staleTime: Infinity`, so
 * a write that does not invalidate `queryKeys.settings.scope(scope)` leaves the
 * reader on a value the user has already changed. Nine call sites called
 * `commands.settingsUpdate` directly; two of them wrote scopes a `useQuery`
 * caller already reads.
 *
 * The rule lived in the `settingsQueries.ts` docstring, which is why nine
 * writers accumulated under it. This turns it into a gate: `data/settingsWrite.ts`
 * is the only module allowed to name the raw command, and every scope with a
 * reader must be written through it.
 *
 * It reads the sources rather than the module graph because the offending call
 * is what matters, not whether a given test happened to exercise it. vitest runs
 * with cwd = apps/desktop.
 */

/// <reference types="node" />
import { readdirSync, readFileSync } from 'node:fs';
import { join, relative } from 'node:path';
import { describe, expect, it } from 'vitest';

const srcDir = join(process.cwd(), 'src');

/** The chokepoint itself, and the generated binding that declares the command. */
const ALLOWED = new Set(['data/settingsWrite.ts', 'bindings/index.ts']);

function sourceFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      out.push(...sourceFiles(full));
    } else if (/\.tsx?$/.test(entry.name)) {
      out.push(full);
    }
  }
  return out;
}

/** Source with comments stripped, so a docstring naming the command is not a
 *  call. Test files are excluded: a mock or an assertion legitimately names it. */
function callSites(): string[] {
  const offenders: string[] = [];
  for (const file of sourceFiles(srcDir)) {
    const rel = relative(srcDir, file).split(/[\\/]/).join('/');
    if (ALLOWED.has(rel)) continue;
    if (/\.(test|spec)\.tsx?$/.test(rel)) continue;

    const code = readFileSync(file, 'utf-8')
      .replace(/\/\*[\s\S]*?\*\//g, '')
      .replace(/^\s*\/\/.*$/gm, '');
    // `.settingsUpdate(` covers both `commands.settingsUpdate(...)` and the
    // `void commands\n  .settingsUpdate(...)` chained form.
    if (/\.settingsUpdate\s*\(/.test(code)) offenders.push(rel);
  }
  return offenders;
}

describe('settings.update has one write path', () => {
  it('no module outside data/settingsWrite.ts calls commands.settingsUpdate', () => {
    expect(
      callSites(),
      'these modules write a settings scope without invalidating its cached read — call updateSettings from @/data/settingsWrite instead',
    ).toEqual([]);
  });

  it('the chokepoint invalidates the scope it just wrote', () => {
    const source = readFileSync(join(srcDir, 'data/settingsWrite.ts'), 'utf-8');

    // Without this the guard above would pass while every write went stale:
    // one unchecked write path is not better than nine.
    expect(source).toMatch(/invalidateQueries/);
    expect(source).toMatch(/queryKeys\.settings\.scope\(args\.scope\)/);
  });
});
