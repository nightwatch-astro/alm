// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * Regression test for #612: "+ New project here" (TargetDetail) navigates
 * to the wizard with a real `?targetId=` search param. The wizard must
 * resolve it via `target.get`, prefill the name step with a real target
 * reference (not a guessed string), show the "From target" subbar chip, and
 * carry `canonicalTargetId` through to `projects.create` — instead of the
 * prior behaviour of dropping the association at the entry point (see
 * issue #612 re-confirmation 2026-07-12).
 *
 * Uses the REAL `StepName` (unlike WizardPage.test.tsx, which stubs it) so
 * the `target` prop actually renders the "From target" summary and the
 * resolved target flows into `wizardData.name.target`.
 */

import {
  render,
  screen,
  fireEvent,
  waitFor,
  act,
} from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

const { mockUseSearch } = vi.hoisted(() => ({
  mockUseSearch: vi.fn(),
}));
vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => vi.fn().mockReturnValue(undefined),
  useSearch: mockUseSearch,
}));

vi.mock('@/features/projects/store', () => ({
  callCreateProject: vi.fn(),
}));

vi.mock('@/shared/toast', () => ({
  addToast: vi.fn(),
}));

const { mockListProjects, mockSessionsList, mockTargetGet } = vi.hoisted(
  () => ({
    mockListProjects: vi.fn(),
    mockSessionsList: vi.fn(),
    mockTargetGet: vi.fn(),
  }),
);
vi.mock('@/bindings/index', async (importOriginal) => {
  const original = await importOriginal<typeof import('@/bindings/index')>();
  return {
    ...original,
    commands: {
      ...original.commands,
      projectsList: mockListProjects,
      sessionsList: mockSessionsList,
      targetGet: mockTargetGet,
    },
  };
});

// StepName's target picker renders `TargetSearch`, which is irrelevant here
// (the target arrives via `?targetId=`, never a manual pick) — stub it so
// this test doesn't need to satisfy its own IPC surface.
vi.mock('@/components', async (importOriginal) => {
  const original = await importOriginal<typeof import('@/components')>();
  return {
    ...original,
    TargetSearch: () => <div data-testid="target-search" />,
  };
});

vi.mock('./StepSources', () => ({
  StepSources: ({
    onChange,
  }: {
    onChange: (d: { selectedSessionIds: string[] }) => void;
  }) => (
    <div data-testid="step-sources">
      <button onClick={() => onChange({ selectedSessionIds: [] })}>
        Select sessions
      </button>
    </div>
  ),
}));

vi.mock('./StepCalibration', () => ({
  StepCalibration: () => <div data-testid="step-calibration" />,
}));

vi.mock('./StepViews', () => ({
  StepViews: () => <div data-testid="step-views" />,
}));

vi.mock('./StepLayout', () => ({
  StepLayout: () => <div data-testid="step-layout" />,
}));

vi.mock('./StepReview', () => ({
  StepReview: () => <div data-testid="step-review">Review your plan</div>,
}));

vi.mock('@/ui', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/ui')>();
  return {
    ...actual,
    WizardShell: ({
      children,
      summary,
    }: {
      children: React.ReactNode;
      summary?: React.ReactNode;
    }) => (
      <div data-testid="wizard-shell">
        <div data-testid="wizard-summary">{summary}</div>
        <div data-testid="wizard-content">{children}</div>
      </div>
    ),
    Btn: ({
      children,
      onClick,
      disabled,
      'data-testid': testid,
    }: React.ButtonHTMLAttributes<HTMLButtonElement> & {
      'data-testid'?: string;
    }) => (
      <button onClick={onClick} disabled={disabled} data-testid={testid}>
        {children}
      </button>
    ),
  };
});

import type React from 'react';
import { WizardPage } from './WizardPage';
import { callCreateProject } from '@/features/projects/store';

const mockCallCreateProject = vi.mocked(callCreateProject);

const TARGET_DETAIL = {
  id: 'target-001',
  primaryDesignation: 'NGC 7000',
  displayAlias: 'North America Nebula',
  effectiveLabel: 'North America Nebula',
  objectType: 'emission_nebula',
  raDeg: 314.5,
  decDeg: 44.3,
  simbadOid: null,
  source: 'seed',
  aliases: [],
};

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.removeItem('alm-project-wizard-draft');
  mockUseSearch.mockReturnValue({ targetId: 'target-001' });
  mockListProjects.mockResolvedValue({ status: 'ok', data: [] });
  mockSessionsList.mockResolvedValue({ status: 'ok', data: [] });
  mockTargetGet.mockResolvedValue({ status: 'ok', data: TARGET_DETAIL });
});

function renderWizard() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <WizardPage />
    </QueryClientProvider>,
  );
}

describe('WizardPage consumes ?targetId= (#612)', () => {
  it('resolves the target and prefills the name step + subbar chip', async () => {
    renderWizard();

    await waitFor(() => {
      expect(mockTargetGet).toHaveBeenCalledWith({ targetId: 'target-001' });
    });

    // Name prefilled from the resolved target's primary designation.
    await waitFor(() => {
      expect(screen.getByLabelText('Project name')).toHaveValue('NGC 7000');
    });

    // Subbar shows a real "From target" chip (not a guessed string). Two
    // "North America Nebula" nodes render (subbar chip + StepName's own
    // target summary), so assert on the subbar's "From target context:" text.
    await waitFor(() => {
      expect(
        screen.getByText(/From target context:/i).parentElement,
      ).toHaveTextContent('North America Nebula');
    });
  });

  it('carries canonicalTargetId through to projects.create', async () => {
    mockCallCreateProject.mockResolvedValue({
      projectId: 'proj-new-005',
      lifecycle: 'setup_incomplete',
      planId: null,
      channels: [],
      auditId: 'audit-005',
      createdAt: '2026-07-18T00:00:00Z',
    });

    renderWizard();

    await waitFor(() => {
      expect(screen.getByLabelText('Project name')).toHaveValue('NGC 7000');
    });

    fireEvent.click(screen.getByText(/Next: sources/i));
    await waitFor(() =>
      expect(screen.getByTestId('step-sources')).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByText('Select sessions'));

    fireEvent.click(screen.getByText(/Next: calibration/i));
    await waitFor(() =>
      expect(screen.getByTestId('step-calibration')).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByText(/Next: source views/i));
    await waitFor(() =>
      expect(screen.getByTestId('step-views')).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByText(/Next: naming/i));
    await waitFor(() =>
      expect(screen.getByTestId('step-layout')).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByText(/Next: review/i));
    await waitFor(() =>
      expect(screen.getByTestId('step-review')).toBeInTheDocument(),
    );

    await act(async () => {
      fireEvent.click(screen.getByTestId('wizard-create-btn'));
    });

    await waitFor(() => {
      expect(mockCallCreateProject).toHaveBeenCalledWith(
        expect.objectContaining({
          name: 'NGC 7000',
          canonicalTargetId: 'target-001',
        }),
      );
    });
  });
});
