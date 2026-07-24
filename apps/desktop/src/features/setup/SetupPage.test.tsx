// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * SetupPage tests — #505: the setup wizard renders outside Shell.tsx, so it
 * carries its own `density-*` class (mirroring `.pv-frame density-${density}`
 * in app/Shell.tsx) for a live density preview during setup.
 */

import { render, screen, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';

const mockNavigate = vi.fn();

vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => mockNavigate,
}));

vi.mock('@/app/first-run', () => ({
  checkFirstRunComplete: vi.fn().mockResolvedValue(false),
}));

// SetupWizard pulls in the full wizard step tree (folder pickers, IPC, etc.)
// — out of scope here, stub it so this test stays focused on SetupPage's own
// container markup.
vi.mock('./SetupWizard', () => ({
  SetupWizard: () => <div data-testid="setup-wizard-stub" />,
}));

import { setPreference, resetPreferences } from '@/data/preferences';
import { SetupPage } from './SetupPage';

beforeEach(() => {
  mockNavigate.mockReset();
  resetPreferences();
});

describe('SetupPage — live density preview (#505)', () => {
  it('carries the density class matching the current preference', async () => {
    setPreference('density', 'spacious');
    render(<SetupPage />);
    await waitFor(() =>
      expect(screen.getByTestId('setup-wizard-stub')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('setup-page')).toHaveClass('density-spacious');
  });

  it('defaults to the comfortable density class', async () => {
    render(<SetupPage />);
    await waitFor(() =>
      expect(screen.getByTestId('setup-wizard-stub')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('setup-page')).toHaveClass('density-comfortable');
  });
});
