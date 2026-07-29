// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { describe, it, expect } from 'vitest';
import { resolveRevealPath, basename, parentSegment } from './path';

describe('resolveRevealPath', () => {
  it('joins a forward-slash relative path onto a unix root', () => {
    expect(resolveRevealPath('/var/lib/pv', 'sessions/2024/IC1396')).toBe(
      '/var/lib/pv/sessions/2024/IC1396',
    );
  });

  it('strips trailing slash on root before joining', () => {
    expect(resolveRevealPath('/var/lib/pv/', 'sessions/2024')).toBe(
      '/var/lib/pv/sessions/2024',
    );
  });

  it('strips leading slash on relativePath before joining', () => {
    expect(resolveRevealPath('/var/lib/pv', '/sessions/2024')).toBe(
      '/var/lib/pv/sessions/2024',
    );
  });

  it('normalizes separators to backslash on a Windows root', () => {
    // Root uses backslash → relative forward slashes become backslashes.
    expect(
      resolveRevealPath('C:\\Users\\sjors\\Pictures', 'sessions/2024/IC1396'),
    ).toBe('C:\\Users\\sjors\\Pictures\\sessions\\2024\\IC1396');
  });

  it('returns rootPath when relativePath is null', () => {
    expect(resolveRevealPath('/var/lib/pv', null)).toBe('/var/lib/pv');
  });

  it('returns rootPath when relativePath is undefined', () => {
    expect(resolveRevealPath('/var/lib/pv', undefined)).toBe('/var/lib/pv');
  });

  it('returns rootPath when relativePath is empty string', () => {
    expect(resolveRevealPath('/var/lib/pv', '')).toBe('/var/lib/pv');
  });
});

describe('basename', () => {
  it('returns the last forward-slash segment', () => {
    expect(basename('sessions/2024/IC1396')).toBe('IC1396');
  });

  it('handles backslash separators', () => {
    expect(basename('C:\\Users\\sjors\\Pictures')).toBe('Pictures');
  });

  it('returns the whole string when there are no separators', () => {
    expect(basename('filename.fits')).toBe('filename.fits');
  });

  it('returns the segment, not an empty string, when path ends with slash', () => {
    // path.split('/').pop() behaviour: empty string becomes fallback
    // this is consistent with the original implementation
    expect(basename('sessions/2024/')).toBe('sessions/2024/');
  });
});

describe('parentSegment', () => {
  it('returns the second-to-last segment', () => {
    expect(parentSegment('sessions/2024/IC1396')).toBe('2024');
  });

  it('returns empty string for a single-segment path', () => {
    expect(parentSegment('filename.fits')).toBe('');
  });

  it('returns empty string for an empty string', () => {
    expect(parentSegment('')).toBe('');
  });

  it('handles backslash separators', () => {
    expect(parentSegment('C:\\Users\\sjors')).toBe('Users');
  });

  it('filters empty segments from trailing slashes', () => {
    expect(parentSegment('sessions/2024/')).toBe('sessions');
  });
});
