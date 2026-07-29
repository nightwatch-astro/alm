// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * T078c — New-project wizard: session+calibration selection, create succeeds
 * end-to-end (FR-043).
 *
 * Tests:
 * 1. WizardPage renders step 1 (Name & profile) by default.
 * 2. Step 2 (Sources / lights) is reachable and shows session selection.
 * 3. Step 3 (Calibration) is reachable and shows calibration mapping.
 * 4. Advancing to step 6 (Review) shows the Create project button.
 * 5. Clicking Create project calls callCreateProject with the wizard data.
 * 6. On success, a toast is shown and navigate('/projects') is called.
 * 7. Target detail "new project" button navigates to /projects/new.
 *
 * WP-008-B: per-field projects.create error handling, ported from
 * CreateProjectDialog (spec 008 US1) —
 * 8. A live duplicate-name pre-check blocks creation (no backend call) and
 *    surfaces the error on the name step.
 * 9. A `name.*`/`tool.*` backend error code routes back to the name step.
 * 10. A `path.*`/other error code surfaces inline on the review step (no
 *     dedicated path field exists in the wizard).
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

// ── Mocks ─────────────────────────────────────────────────────────────────────

const { mockNavigate } = vi.hoisted(() => ({
  mockNavigate: vi.fn().mockReturnValue(undefined),
}));
vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => mockNavigate,
  useSearch: () => ({}),
}));

vi.mock('@/features/projects/store', () => ({
  callCreateProject: vi.fn(),
}));

vi.mock('@/shared/toast', () => ({
  addToast: vi.fn(),
}));

// The live duplicate-name pre-check (WP-008-B, ported from CreateProjectDialog)
// calls commands.projectsList directly; #776/#599 (real StepViews/StepReview
// data) call commands.sessionsList directly.
const { mockListProjects, mockSessionsList } = vi.hoisted(() => ({
  mockListProjects: vi.fn(),
  mockSessionsList: vi.fn(),
}));
vi.mock('@/bindings/index', async (importOriginal) => {
  const original = await importOriginal<typeof import('@/bindings/index')>();
  return {
    ...original,
    commands: {
      ...original.commands,
      projectsList: mockListProjects,
      sessionsList: mockSessionsList,
    },
  };
});

// Stub child wizard steps so we only test the WizardPage orchestration.
// The real StepName also renders a WP-008-B `serverError` prop (name/tool
// create errors); the stub surfaces it the same way so orchestration tests
// can assert it lands on this step without pulling in RHF/zod.
vi.mock('./StepName', () => ({
  StepName: ({
    onChange,
    serverError,
  }: {
    onChange: (d: { name: string; workflowProfile: string }) => void;
    serverError?: { field: 'name' | 'tool'; message: string } | null;
  }) => (
    <div data-testid="step-name">
      <input
        aria-label="Project name"
        onChange={(e) =>
          onChange({ name: e.target.value, workflowProfile: 'pixinsight' })
        }
      />
      {serverError && <span role="alert">{serverError.message}</span>}
    </div>
  ),
}));

vi.mock('./StepSources', () => ({
  StepSources: ({
    onChange,
  }: {
    onChange: (d: { selectedSessionIds: string[] }) => void;
  }) => (
    <div data-testid="step-sources">
      <button
        onClick={() =>
          onChange({ selectedSessionIds: ['sess-001', 'sess-002'] })
        }
      >
        Select sessions
      </button>
    </div>
  ),
}));

vi.mock('./StepCalibration', () => ({
  StepCalibration: ({ onChange }: { onChange: (d: unknown) => void }) => (
    <div data-testid="step-calibration">
      <button
        onClick={() =>
          onChange({
            flatMappings: {},
            sharedDarkId: 'dark-001',
            sharedBiasId: '',
            sharedDarkFlatId: '',
          })
        }
      >
        Select dark master
      </button>
    </div>
  ),
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

// ── Tests ─────────────────────────────────────────────────────────────────────

import { WizardPage } from './WizardPage';
import { callCreateProject } from '@/features/projects/store';
import { addToast } from '@/shared/toast';

const mockCallCreateProject = vi.mocked(callCreateProject);
const mockAddToast = vi.mocked(addToast);

beforeEach(() => {
  vi.clearAllMocks();
  // Clear the draft from localStorage before each test
  localStorage.removeItem('alm-project-wizard-draft');
  // Default: no existing projects, so the live duplicate-name pre-check never
  // blocks a test that doesn't care about it.
  mockListProjects.mockResolvedValue({ status: 'ok', data: [] });
  mockSessionsList.mockResolvedValue({ status: 'ok', data: [] });
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

describe('persisted draft validation', () => {
  const DRAFT_KEY = 'alm-project-wizard-draft';

  function summaryName(container: HTMLElement): string {
    return (
      container.querySelector('.pv-wizard-page__summary-project-name')
        ?.textContent ?? ''
    );
  }

  it('restores a valid draft', () => {
    localStorage.setItem(
      DRAFT_KEY,
      JSON.stringify({
        name: { name: 'Draft target', workflowProfile: 'siril', target: null },
      }),
    );
    const { container } = renderWizard();
    expect(summaryName(container)).toContain('Draft target');
  });

  it('falls back to the initial draft for unparseable JSON', () => {
    localStorage.setItem(DRAFT_KEY, '{not json');
    const { container } = renderWizard();
    expect(summaryName(container)).not.toContain('Draft target');
    expect(screen.getByTestId('step-name')).toBeInTheDocument();
  });

  it('falls back to the initial draft for a valid-JSON wrong shape', () => {
    // `name` must be an object; a bare string is a shape from no known version.
    localStorage.setItem(DRAFT_KEY, JSON.stringify({ name: 'Draft target' }));
    const { container } = renderWizard();
    expect(summaryName(container)).not.toContain('Draft target');
    expect(screen.getByTestId('step-name')).toBeInTheDocument();
  });
});

describe('T078c: WizardPage renders inside main window with correct layout', () => {
  it('renders step 1 (Name & profile) by default', () => {
    renderWizard();
    expect(screen.getByTestId('step-name')).toBeInTheDocument();
  });

  it('shows "New project" in the toolbar heading span', () => {
    renderWizard();
    // The toolbar span contains "New project —" + projectLabel.
    // Use getAllByText since the summary rail also mentions project name.
    const matches = screen.getAllByText(/New project/i);
    expect(matches.length).toBeGreaterThanOrEqual(1);
  });

  it('has the layout fix: outer div carries the pv-wizard-page class (min-height:0 via CSS)', () => {
    const { container } = renderWizard();
    const outer = container.firstChild as HTMLElement;
    // The outer div must carry pv-wizard-page which sets min-height:0 to prevent flex overflow
    expect(outer.classList.contains('pv-wizard-page')).toBe(true);
  });
});

describe('T078c: wizard step navigation with session+calibration selection', () => {
  it('navigates from step 1 to step 2 (Sources)', async () => {
    renderWizard();

    // Fill in project name to enable next
    const nameInput = screen.getByLabelText('Project name');
    fireEvent.change(nameInput, { target: { value: 'NGC 7000 HOO' } });

    // Click Next: sources
    const nextBtn = screen.getByText(/Next: sources/i);
    fireEvent.click(nextBtn);

    await waitFor(() => {
      expect(screen.getByTestId('step-sources')).toBeInTheDocument();
    });
  });

  it('session selection is possible at step 2', async () => {
    renderWizard();

    // Step 0: fill name so canAdvance() returns true
    const nameInput = screen.getByLabelText('Project name');
    fireEvent.change(nameInput, { target: { value: 'Test Project' } });

    // Navigate to step 2 (sources)
    fireEvent.click(screen.getByText(/Next: sources/i));
    await waitFor(() =>
      expect(screen.getByTestId('step-sources')).toBeInTheDocument(),
    );

    // The step has a "Select sessions" button
    expect(screen.getByText('Select sessions')).toBeInTheDocument();

    // Click it to select sessions
    fireEvent.click(screen.getByText('Select sessions'));

    // Summary rail should update to show 2 sessions
    await waitFor(() => {
      expect(screen.getByText('2 sess')).toBeInTheDocument();
    });
  });

  it('calibration selection is possible at step 3', async () => {
    renderWizard();

    // Step 0: fill name
    const nameInput = screen.getByLabelText('Project name');
    fireEvent.change(nameInput, { target: { value: 'Test Project' } });

    // Step 0 → 1
    fireEvent.click(screen.getByText(/Next: sources/i));
    await waitFor(() =>
      expect(screen.getByTestId('step-sources')).toBeInTheDocument(),
    );

    // Select sessions so canAdvance() at step 1 returns true
    fireEvent.click(screen.getByText('Select sessions'));
    await waitFor(() => expect(screen.getByText('2 sess')).toBeInTheDocument());

    // Step 1 → 2
    fireEvent.click(screen.getByText(/Next: calibration/i));
    await waitFor(() =>
      expect(screen.getByTestId('step-calibration')).toBeInTheDocument(),
    );

    // The step has a "Select dark master" button
    expect(screen.getByText('Select dark master')).toBeInTheDocument();
    fireEvent.click(screen.getByText('Select dark master'));

    // Summary rail should show 1 dark master
    await waitFor(() => {
      expect(screen.getByText('1 master')).toBeInTheDocument();
    });
  });
});

describe('T078c: create project end-to-end', () => {
  /** Helper: advance WizardPage from step 0 through to review (step 5). */
  async function advanceToReview(nameValue = 'NGC 7000 HOO') {
    const nameInput = screen.getByLabelText('Project name');
    fireEvent.change(nameInput, { target: { value: nameValue } });

    fireEvent.click(screen.getByText(/Next: sources/i));
    await waitFor(() =>
      expect(screen.getByTestId('step-sources')).toBeInTheDocument(),
    );

    // Select sessions so step 1 canAdvance = true
    fireEvent.click(screen.getByText('Select sessions'));
    await waitFor(() => expect(screen.getByText('2 sess')).toBeInTheDocument());

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
  }

  it('shows "Create project" button at step 6 (Review)', async () => {
    renderWizard();
    await advanceToReview();

    // Create project button appears at step 6
    expect(screen.getByTestId('wizard-create-btn')).toBeInTheDocument();
    expect(screen.getByTestId('wizard-create-btn')).toHaveTextContent(
      'Create project',
    );
  });

  it('calls callCreateProject on clicking Create project', async () => {
    mockCallCreateProject.mockResolvedValue({
      projectId: 'proj-new-001',
      lifecycle: 'setup_incomplete',
      planId: null,
      channels: [],
      auditId: 'audit-001',
      createdAt: '2026-06-17T00:00:00Z',
    });

    renderWizard();
    await advanceToReview('NGC 7000 HOO');

    await act(async () => {
      fireEvent.click(screen.getByTestId('wizard-create-btn'));
    });

    await waitFor(() => {
      expect(mockCallCreateProject).toHaveBeenCalledWith(
        expect.objectContaining({
          name: 'NGC 7000 HOO',
          tool: 'PixInsight',
          path: expect.stringContaining('ngc-7000-hoo'),
          initialSources: ['sess-001', 'sess-002'],
        }),
      );
    });
  });

  it('shows success toast and navigates to /projects after successful create', async () => {
    mockCallCreateProject.mockResolvedValue({
      projectId: 'proj-new-002',
      lifecycle: 'setup_incomplete',
      planId: null,
      channels: [],
      auditId: 'audit-002',
      createdAt: '2026-06-17T00:00:00Z',
    });

    renderWizard();
    await advanceToReview('M31 LRGB');

    await act(async () => {
      fireEvent.click(screen.getByTestId('wizard-create-btn'));
    });

    await waitFor(() => {
      expect(mockAddToast).toHaveBeenCalledWith(
        expect.objectContaining({ variant: 'success' }),
      );
    });
  });

  it('#604: success toast carries a "View project" navigation action', async () => {
    mockCallCreateProject.mockResolvedValue({
      projectId: 'proj-new-002b',
      lifecycle: 'setup_incomplete',
      planId: null,
      channels: [],
      auditId: 'audit-002b',
      createdAt: '2026-06-17T00:00:00Z',
    });

    renderWizard();
    await advanceToReview('M31 LRGB');

    await act(async () => {
      fireEvent.click(screen.getByTestId('wizard-create-btn'));
    });

    await waitFor(() => {
      expect(mockAddToast).toHaveBeenCalledWith(
        expect.objectContaining({
          action: expect.objectContaining({ label: 'View project' }),
        }),
      );
    });

    const call = mockAddToast.mock.calls.find(
      ([opts]) => opts.action?.label === 'View project',
    );
    call?.[0].action?.onClick();
    expect(mockNavigate).toHaveBeenCalledWith(
      expect.objectContaining({
        to: '/projects',
        search: { selected: 'proj-new-002b' },
      }),
    );
  });

  it('confirms folder creation when the scaffolding plan auto-applied (scaffoldApplied: true)', async () => {
    mockCallCreateProject.mockResolvedValue({
      projectId: 'proj-new-003',
      lifecycle: 'setup_incomplete',
      planId: 'plan-003',
      channels: [],
      auditId: 'audit-003',
      createdAt: '2026-07-04T00:00:00Z',
      scaffoldApplied: true,
    });

    renderWizard();
    await advanceToReview('NGC 891');

    await act(async () => {
      fireEvent.click(screen.getByTestId('wizard-create-btn'));
    });

    await waitFor(() => {
      expect(mockAddToast).toHaveBeenCalledWith(
        expect.objectContaining({
          variant: 'success',
          message: expect.stringContaining('project folders created'),
        }),
      );
    });
  });

  it('shows the failure toast when scaffolding auto-apply failed (scaffoldApplied: false)', async () => {
    mockCallCreateProject.mockResolvedValue({
      projectId: 'proj-new-004',
      lifecycle: 'setup_incomplete',
      planId: 'plan-004',
      channels: [],
      auditId: 'audit-004',
      createdAt: '2026-07-04T00:00:00Z',
      scaffoldApplied: false,
    });

    renderWizard();
    await advanceToReview('IC 1396');

    await act(async () => {
      fireEvent.click(screen.getByTestId('wizard-create-btn'));
    });

    await waitFor(() => {
      expect(mockAddToast).toHaveBeenCalledWith(
        expect.objectContaining({
          variant: 'error',
          message: expect.stringContaining('folder creation failed'),
        }),
      );
    });
  });

  it('routes a name.* backend error back to the name step instead of a toast', async () => {
    mockCallCreateProject.mockRejectedValue(new Error('name.duplicate'));

    renderWizard();
    await advanceToReview('Duplicate Project');

    await act(async () => {
      fireEvent.click(screen.getByTestId('wizard-create-btn'));
    });

    // Routed back to step 0 (name) with the mapped message inline, not a toast.
    await waitFor(() => {
      expect(screen.getByTestId('step-name')).toBeInTheDocument();
      expect(screen.getByRole('alert')).toHaveTextContent(/already exists/i);
    });
    expect(mockAddToast).not.toHaveBeenCalled();
  });

  it('routes a tool.* backend error back to the name step (workflow profile lives there)', async () => {
    mockCallCreateProject.mockRejectedValue(new Error('tool.unknown'));

    renderWizard();
    await advanceToReview('Some Project');

    await act(async () => {
      fireEvent.click(screen.getByTestId('wizard-create-btn'));
    });

    await waitFor(() => {
      expect(screen.getByTestId('step-name')).toBeInTheDocument();
      expect(screen.getByRole('alert')).toHaveTextContent(
        /unknown processing tool/i,
      );
    });
  });

  it('surfaces a path.* backend error inline on the review step (no dedicated path field)', async () => {
    mockCallCreateProject.mockRejectedValue(new Error('path.collision'));

    renderWizard();
    await advanceToReview('Some Project');

    await act(async () => {
      fireEvent.click(screen.getByTestId('wizard-create-btn'));
    });

    // Stays on the review step (path has no dedicated step/field to return to).
    await waitFor(() => {
      expect(screen.getByTestId('step-review')).toBeInTheDocument();
      expect(screen.getByRole('alert')).toHaveTextContent(
        /already uses this folder path/i,
      );
    });
    expect(mockAddToast).not.toHaveBeenCalled();
  });

  it('blocks creation via the live duplicate-name pre-check without calling the backend', async () => {
    mockListProjects.mockResolvedValueOnce({
      status: 'ok',
      data: [
        {
          id: 'proj-existing',
          name: 'Existing Project',
          tool: 'PixInsight',
          lifecycle: 'active',
          path: 'projects/existing',
          notes: null,
          channelDrift: false,
          sourceCount: 0,
          createdAt: '2026-01-01T00:00:00Z',
          updatedAt: '2026-01-01T00:00:00Z',
          blockedReasonKind: null,
          blockedReasonNote: null,
        },
      ],
    });

    renderWizard();
    // Same name, different case — the pre-check is case-insensitive.
    await advanceToReview('existing project');

    await act(async () => {
      fireEvent.click(screen.getByTestId('wizard-create-btn'));
    });

    await waitFor(() => {
      expect(screen.getByTestId('step-name')).toBeInTheDocument();
      expect(screen.getByRole('alert')).toHaveTextContent(/already exists/i);
    });
    expect(mockCallCreateProject).not.toHaveBeenCalled();
  });
});
