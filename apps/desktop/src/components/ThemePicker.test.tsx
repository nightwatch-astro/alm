// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />

import { act, cleanup, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  isTauri: () => false,
}));

import { getThemeChoice } from '@/data/theme';
import { ThemePicker } from './ThemePicker';

function stubOsTheme(prefersDark: boolean): void {
  vi.stubGlobal(
    'matchMedia',
    vi.fn().mockImplementation((query: string) => ({
      matches: query === '(prefers-color-scheme: dark)' && prefersDark,
      media: query,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })),
  );
}

beforeEach(() => {
  localStorage.clear();
  document.documentElement.removeAttribute('data-theme');
  stubOsTheme(false);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe('ThemePicker', () => {
  const SYSTEM_CASES = [
    {
      prefersDark: false,
      resolved: 'warm-slate',
      accessibleName: 'System · auto · light',
    },
    {
      prefersDark: true,
      resolved: 'observatory-cool',
      accessibleName: 'System · auto · dark',
    },
  ] as const;

  for (const { prefersDark, resolved, accessibleName } of SYSTEM_CASES) {
    it(`defaults to System and previews the resolved ${resolved} OS theme`, () => {
      stubOsTheme(prefersDark);
      render(<ThemePicker />);

      const system = screen.getByRole('button', { name: accessibleName });
      expect(system).toHaveAttribute('aria-pressed', 'true');
      expect(
        system.querySelector('[data-testid="theme-swatch-preview"]'),
      ).toHaveAttribute('data-theme', resolved);
      expect(getThemeChoice()).toBe('system');
      expect(localStorage.getItem('pv.theme')).toBeNull();
    });
  }

  it('applies a selection immediately, persists it, and updates pressed state', () => {
    const { unmount } = render(<ThemePicker />);
    const system = screen.getByRole('button', {
      name: 'System · auto · light',
    });
    const dark = screen.getByRole('button', {
      name: 'Observatory Cool · dark',
    });

    dark.focus();
    expect(dark).toHaveFocus();
    act(() => dark.click());

    expect(document.documentElement).toHaveAttribute(
      'data-theme',
      'observatory-cool',
    );
    expect(getThemeChoice()).toBe('observatory-cool');
    expect(localStorage.getItem('pv.theme')).toBe('observatory-cool');
    expect(dark).toHaveAttribute('aria-pressed', 'true');
    expect(system).toHaveAttribute('aria-pressed', 'false');

    unmount();
    render(<ThemePicker />);
    expect(
      screen.getByRole('button', { name: 'Observatory Cool · dark' }),
    ).toHaveAttribute('aria-pressed', 'true');
  });

  it('uses focusable native buttons for every choice', () => {
    render(<ThemePicker includeVariants />);

    const picker = screen.getByRole('group', { name: 'Theme' });
    for (const choice of within(picker).getAllByRole('button')) {
      expect(choice.tagName).toBe('BUTTON');
      expect(choice).toHaveAttribute('type', 'button');
      expect(choice.tabIndex).toBe(0);
    }
  });
});
