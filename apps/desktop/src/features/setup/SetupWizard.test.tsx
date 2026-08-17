// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * SetupWizard gating and persisted-state migration tests for the eight-step
 * Language → Theme → Source Folders flow.
 *
 * Validates that Step 3 (Source Folders) blocks advancement when required
 * folder types (light_frames, project) are missing, and that Steps 4–5
 * advance freely. Step 6 (Observing Site) is covered separately below (it's
 * optional and never blocks advancement). The Scan step (step 8) has its own
 * StepScan.test.tsx. The Language step (step 1) has its own
 * `steps/StepLanguage.test.tsx`; the tests below cover its wizard-level
 * integration (T019 immediate re-render, T020 back-navigation recovery).
 */
import {
  render,
  screen,
  fireEvent,
  waitFor,
  act,
} from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { QueryClientProvider } from '@tanstack/react-query';
import type { ReactNode } from 'react';
import { queryClient } from '@/data/queryClient';

function wrapper({ children }: { children: ReactNode }) {
  return (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
}

// ---------------------------------------------------------------------------
// Hoisted mock state — vi.hoisted runs BEFORE vi.mock factories, so
// the `mockPickDirectory` fn is accessible from within the hoisted mock factory.
// ---------------------------------------------------------------------------
const { mockPickDirectory } = vi.hoisted(() => {
  const mockPickDirectory =
    vi.fn<() => Promise<{ path: string | null; cancelled: boolean }>>();
  return { mockPickDirectory };
});

// Mock @tauri-apps/plugin-dialog so any leftover dynamic imports resolve.
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn().mockResolvedValue(null),
}));

// Mock the new native picker module so AddFolderButton uses our controllable
// mock instead of attempting a real Tauri invoke.
vi.mock('@/shared/native/picker', () => ({
  pickDirectory: mockPickDirectory,
  useDirectoryPicker: () => ({
    pick: mockPickDirectory,
    loading: false,
    error: null,
    clearError: vi.fn(),
  }),
}));

// Mock toast module to prevent side-effect issues in tests.
vi.mock('@/shared/toast', () => ({
  addToast: vi.fn(),
  dismissToast: vi.fn(),
  useToasts: () => ({ toasts: [], dismiss: vi.fn(), add: vi.fn() }),
}));

// Mock @tanstack/react-router so useNavigate returns a no-op.
vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => vi.fn(),
}));

// Mock Tauri backend commands so they never reach the native bridge.
// Catalog manifest fetch resolves to 'failed' so StepCatalogs renders its
// graceful unavailable state in these gating tests (download flow is covered
// in StepCatalogs.test.tsx).
//
// StepCatalogs' ResolverSettingsControl reads resolver settings from the
// settings feature's settingsIpc glue module (spec 037); mock those two there.
vi.mock('@/features/settings/settingsIpc', async (importOriginal) => {
  const actual =
    await importOriginal<typeof import('@/features/settings/settingsIpc')>();
  return {
    ...actual,
    getResolverSettings: vi.fn().mockResolvedValue({
      contractVersion: '1.0',
      requestId: 'r',
      settings: {
        onlineEnabled: true,
        simbadEndpoint: 'https://simbad.example/tap',
        debounceMs: 300,
        requestTimeoutSecs: 10,
      },
    }),
    updateResolverSettings: vi
      .fn()
      .mockImplementation((settings) =>
        Promise.resolve({ contractVersion: '1.0', requestId: 'r', settings }),
      ),
  };
});

// spec 037: the wizard, sources-store, StepCatalogs, and StepScan now call
// generated commands.* bindings + unwrap directly. Mock the bindings surface
// (Result-shaped responses) and let unwrap really unwrap them.
// vi.hoisted: vi.mock factories below are hoisted above these declarations.
const {
  mockToolsUpdate,
  mockFirstrunComplete,
  mockRootsRegisterBatch,
  mockSettingsGet,
  mockSettingsUpdate,
  mockInboxScanFolder,
  mockInboxClassify,
} = vi.hoisted(() => ({
  mockToolsUpdate: vi.fn().mockResolvedValue({
    status: 'ok',
    data: {
      id: 'pixinsight',
      name: 'PixInsight',
      enabled: false,
      configured: false,
      available: false,
      supportsOpenFolder: false,
      autoDetected: false,
      executablePath: null,
    },
  }),
  mockFirstrunComplete: vi
    .fn()
    .mockResolvedValue({ status: 'ok', data: { success: true } }),
  mockRootsRegisterBatch: vi.fn().mockResolvedValue({
    status: 'ok',
    data: { status: 'success', items: [] },
  }),
  mockSettingsGet: vi.fn().mockResolvedValue({
    status: 'ok',
    data: { values: { defaultProtection: 'protected' } },
  }),
  mockSettingsUpdate: vi.fn().mockResolvedValue({ status: 'ok', data: null }),
  // StepScan calls these — stub with empty responses so render doesn't throw
  // if the Scan step is reached during tests that navigate that far.
  mockInboxScanFolder: vi.fn().mockResolvedValue({
    status: 'ok',
    data: { rootId: 'root-mock', items: [] },
  }),
  // Never actually invoked in these gating tests (scan responses carry no
  // items), but the module surface must still expose it.
  mockInboxClassify: vi.fn(),
}));

vi.mock('@/bindings/index', () => ({
  commands: {
    toolsUpdate: mockToolsUpdate,
    firstrunComplete: mockFirstrunComplete,
    rootsRegisterBatch: mockRootsRegisterBatch,
    settingsGet: mockSettingsGet,
    settingsUpdate: mockSettingsUpdate,
    inboxScanFolder: mockInboxScanFolder,
    inboxClassify: mockInboxClassify,
  },
}));

// Mock @tauri-apps/api/core to prevent any accidental live invoke.
// isTauri must be mocked too (not just invoke): LocaleProvider now wraps this
// component (spec 061 US1) and calls it during mount hydration — the real
// implementation only degrades to `false` inside an actual Tauri webview,
// which jsdom is not, but leaving it unmocked destructures to `undefined` on
// this mocked module and throws when called.
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockRejectedValue(new Error('no tauri')),
  isTauri: () => false,
}));

// Ensure mock mode is OFF so the gating logic actually fires.
// (When VITE_USE_MOCKS === 'true', canProceed always returns true.)
vi.stubEnv('VITE_USE_MOCKS', 'false');

// ---------------------------------------------------------------------------
// Component under test
// ---------------------------------------------------------------------------
import { SetupWizard } from './SetupWizard';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const WIZARD_STORAGE_KEY = 'alm-setup-wizard-state';
const WIZARD_STATE_VERSION = 2;

/** Render the wizard and return the result. */
function renderWizard() {
  return render(<SetupWizard />, { wrapper });
}

/**
 * Render the wizard at Source Folders. Most of this suite exercises
 * Source-Folders-and-later gating and has no interest in the first two steps.
 */
function renderWizardAtSources() {
  window.localStorage.setItem(
    WIZARD_STORAGE_KEY,
    JSON.stringify({ version: WIZARD_STATE_VERSION, currentStep: 2 }),
  );
  return render(<SetupWizard />, { wrapper });
}

/**
 * Click the primary "Continue" button.
 * Throws if the button is not found.
 */
function _clickContinue() {
  const btn = getContinueButton();
  fireEvent.click(btn);
}

/** Return the primary continue button. */
function getContinueButton(): HTMLElement {
  const allButtons = screen.getAllByRole('button');
  // Matches both "Continue to <step>" and the empty-site-step variant
  // "Continue without a site".
  const match = allButtons.find((b) => b.textContent?.includes('Continue'));
  if (!match) throw new Error('Continue button not found');
  return match;
}

/**
 * Map a SourceKind to the display label used in each group's add-button
 * aria-label ("Add <Label> folder").
 */
const KIND_LABEL: Record<string, string> = {
  light_frames: 'Light frames',
  calibration: 'Calibration frames',
  project: 'Projects',
  inbox: 'Inbox',
};

/**
 * Simulate adding a folder to a specific group by configuring the mocked
 * pickDirectory() and clicking that group's own "+ Add folder" button (located
 * via its per-kind aria-label). Defaults to the light_frames group.
 */
async function addFolder(path: string, kind = 'light_frames') {
  mockPickDirectory.mockResolvedValueOnce({ path, cancelled: false });

  const label = KIND_LABEL[kind] ?? kind;
  const addBtn = screen.getByRole('button', {
    name: new RegExp(`add ${label} folder`, 'i'),
  });

  await act(async () => {
    fireEvent.click(addBtn);
    // handleChoose is async: it awaits pickDirectory(). Flush the microtask
    // queue so React processes the state update.
    await new Promise((r) => setTimeout(r, 0));
  });
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  // Clear all wizard and preference state between tests.
  window.localStorage.clear();
  mockPickDirectory.mockReset();
  queryClient.clear();
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('SetupWizard persisted-state migration', () => {
  function persist(state: Record<string, unknown>) {
    window.localStorage.setItem(WIZARD_STORAGE_KEY, JSON.stringify(state));
  }

  it('starts at Language when the stored state is unparseable JSON', () => {
    window.localStorage.setItem(WIZARD_STORAGE_KEY, '{not json');
    renderWizard();

    expect(screen.getByText(/choose your language/i)).toBeInTheDocument();
    expect(screen.getByText(/step 1 of 8/i)).toBeInTheDocument();
  });

  it('drops a stale sub-state shape without losing the step position', () => {
    // A `tools` shape from an older build must not reset the whole wizard.
    persist({
      version: WIZARD_STATE_VERSION,
      currentStep: 1,
      tools: { pixinsight: 'enabled' },
    });
    renderWizard();

    expect(
      screen.getByRole('heading', { name: /choose how platevault looks/i }),
    ).toBeInTheDocument();
    expect(screen.getByText(/step 2 of 8/i)).toBeInTheDocument();
  });

  it('shifts an unversioned Source Folders step past the inserted Theme step', () => {
    persist({ currentStep: 1 });
    renderWizard();

    expect(screen.getByText(/where does your data live/i)).toBeInTheDocument();
    expect(screen.getByText(/step 3 of 8/i)).toBeInTheDocument();
  });

  it('shifts a v1 Observing Site step past the inserted Theme step', () => {
    persist({ version: 1, currentStep: 4 });
    renderWizard();

    expect(
      screen.getByRole('heading', { name: /where do you observe from/i }),
    ).toBeInTheDocument();
    expect(screen.getByText(/step 6 of 8/i)).toBeInTheDocument();
  });

  it('preserves a current v2 Theme step', () => {
    persist({ version: WIZARD_STATE_VERSION, currentStep: 1 });
    renderWizard();

    expect(
      screen.getByRole('heading', { name: /choose how platevault looks/i }),
    ).toBeInTheDocument();
    expect(screen.getByText(/step 2 of 8/i)).toBeInTheDocument();
    expect(
      JSON.parse(localStorage.getItem(WIZARD_STORAGE_KEY) ?? '{}'),
    ).toMatchObject({ version: WIZARD_STATE_VERSION, currentStep: 1 });
  });

  it.each([
    ['invalid', '2'],
    ['negative', -1],
    ['non-integer', 1.5],
  ])('starts at Language for a %s persisted step', (_case, currentStep) => {
    persist({ version: WIZARD_STATE_VERSION, currentStep });
    renderWizard();

    expect(screen.getByText(/choose your language/i)).toBeInTheDocument();
    expect(screen.getByText(/step 1 of 8/i)).toBeInTheDocument();
  });

  it('clamps an out-of-range current step and reopens on Confirm', () => {
    persist({ version: WIZARD_STATE_VERSION, currentStep: 99 });
    renderWizard();

    expect(screen.getByText(/ready to go/i)).toBeInTheDocument();
    expect(screen.getByText(/step 7 of 8/i)).toBeInTheDocument();
  });

  it.each([undefined, 1])(
    'reopens a legacy Scan step on Confirm for version %s',
    (version) => {
      persist({ version, currentStep: 6 });
      renderWizard();

      expect(screen.getByText(/ready to go/i)).toBeInTheDocument();
      expect(screen.getByText(/step 7 of 8/i)).toBeInTheDocument();
      expect(screen.queryByTestId('step-scan')).not.toBeInTheDocument();
    },
  );
});

describe('SetupWizard eight-step flow', () => {
  it('starts on the Language step and shows the heading', () => {
    renderWizard();
    expect(screen.getByText(/choose your language/i)).toBeInTheDocument();
    expect(screen.getByText(/step 1 of 8/i)).toBeInTheDocument();
  });

  it('advances from Language through Theme to Source Folders', () => {
    renderWizard();
    fireEvent.click(getContinueButton());
    expect(
      screen.getByRole('heading', { name: /choose how platevault looks/i }),
    ).toBeInTheDocument();
    expect(screen.getByText(/step 2 of 8/i)).toBeInTheDocument();

    fireEvent.click(getContinueButton());
    expect(screen.getByText(/where does your data live/i)).toBeInTheDocument();
    expect(screen.getByText(/step 3 of 8/i)).toBeInTheDocument();
  });

  it('blocks Continue on the Source Folders step when no paths are added', () => {
    renderWizardAtSources();
    const continueBtn = getContinueButton();
    expect(continueBtn).toBeDisabled();
  });

  // spec 061 US1 T019/T020: selecting a language re-renders the wizard
  // immediately (no reload) and does not discard whatever progress the user
  // already made — go back to Language mid-flow, switch, and confirm the
  // folder added before Back is still there afterwards.
  it('T019: selecting a language on Back-navigation preserves wizard progress', () => {
    const seeded = {
      version: WIZARD_STATE_VERSION,
      currentStep: 2,
      sources: [{ path: '/astro/lights', kind: 'light_frames' }],
    };
    window.localStorage.setItem(WIZARD_STORAGE_KEY, JSON.stringify(seeded));
    renderWizard();
    expect(screen.getByText('/astro/lights')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /back/i }));
    expect(
      screen.getByRole('heading', { name: /choose how platevault looks/i }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /back/i }));
    expect(screen.getByText(/choose your language/i)).toBeInTheDocument();
    expect(
      screen.getByText('Pick your preferred language'),
    ).toBeInTheDocument();

    const portuguese = screen.getByRole('button', {
      name: 'Português (Brasil)',
    });
    fireEvent.click(portuguese);

    // Applies immediately, in place — no navigation/reload occurred.
    expect(portuguese).toHaveAttribute('aria-pressed', 'true');
    expect(
      screen.getByText('Escolha seu idioma preferido'),
    ).toBeInTheDocument();
    expect(localStorage.getItem('pv.locale')).toBe('pt-BR');

    // Progress from before Back survives the language change.
    fireEvent.click(screen.getByRole('button', { name: /continuar/i }));
    fireEvent.click(screen.getByRole('button', { name: /continuar/i }));
    expect(screen.getByText('/astro/lights')).toBeInTheDocument();
  });

  it('blocks Continue on Step 3 when only light_frames is added (project missing)', async () => {
    renderWizardAtSources();

    await addFolder('/astro/lights');
    await waitFor(() => {
      expect(screen.getByText('/astro/lights')).toBeInTheDocument();
    });

    // Only light_frames added, project is still missing
    const continueBtn = getContinueButton();
    expect(continueBtn).toBeDisabled();
  });

  it('enables Continue on Step 3 after adding all required folder types', async () => {
    // spec 039: inbox is now optional — only light_frames + project are required.
    renderWizardAtSources();

    // Add a folder to each required group via its own per-group add button.
    await addFolder('/astro/lights', 'light_frames');
    await waitFor(() => {
      expect(screen.getByText('/astro/lights')).toBeInTheDocument();
    });

    await addFolder('/astro/projects', 'project');
    await waitFor(() => {
      expect(screen.getByText('/astro/projects')).toBeInTheDocument();
    });

    // Should now be enabled — inbox NOT required (spec 039 FR-004).
    await waitFor(() => {
      expect(getContinueButton()).not.toBeDisabled();
    });
  });

  it('allows Step 3 to advance without an inbox folder (spec 039 FR-004)', async () => {
    // Completing setup without an inbox folder must be allowed.
    renderWizardAtSources();

    await addFolder('/astro/lights', 'light_frames');
    await addFolder('/astro/projects', 'project');

    // Continue must become enabled with only the two required kinds.
    await waitFor(() => {
      expect(getContinueButton()).not.toBeDisabled();
    });

    // The inbox group card is still present (it's a supported optional kind).
    expect(screen.getByTestId('source-group-inbox')).toBeInTheDocument();
    // But it must NOT carry a data-requirement-met attribute (it's optional).
    expect(screen.getByTestId('source-group-inbox')).not.toHaveAttribute(
      'data-requirement-met',
    );
  });

  it('renders one persistent group card per source kind, even when empty', () => {
    renderWizardAtSources();

    for (const kind of ['light_frames', 'calibration', 'project', 'inbox']) {
      expect(screen.getByTestId(`source-group-${kind}`)).toBeInTheDocument();
    }
  });

  it('highlights required group cards with met/unmet status', async () => {
    renderWizardAtSources();

    // Required groups start unmet; optional groups carry no requirement flag.
    expect(screen.getByTestId('source-group-light_frames')).toHaveAttribute(
      'data-requirement-met',
      'false',
    );
    expect(screen.getByTestId('source-group-project')).toHaveAttribute(
      'data-requirement-met',
      'false',
    );
    // calibration and inbox are optional (spec 039: inbox removed from required kinds).
    expect(screen.getByTestId('source-group-calibration')).not.toHaveAttribute(
      'data-requirement-met',
    );
    expect(screen.getByTestId('source-group-inbox')).not.toHaveAttribute(
      'data-requirement-met',
    );

    // Adding to the light_frames group flips its card to met.
    await addFolder('/astro/lights', 'light_frames');
    await waitFor(() => {
      expect(screen.getByTestId('source-group-light_frames')).toHaveAttribute(
        'data-requirement-met',
        'true',
      );
    });
    // Project still unmet.
    expect(screen.getByTestId('source-group-project')).toHaveAttribute(
      'data-requirement-met',
      'false',
    );
  });

  it('lists added folders inside their own kind group card', async () => {
    renderWizardAtSources();

    await addFolder('/astro/lights', 'light_frames');
    await waitFor(() =>
      expect(screen.getByText('/astro/lights')).toBeInTheDocument(),
    );

    await addFolder('/astro/cals', 'calibration');
    await waitFor(() =>
      expect(screen.getByText('/astro/cals')).toBeInTheDocument(),
    );

    const lightGroup = screen.getByTestId('source-group-light_frames');
    const calGroup = screen.getByTestId('source-group-calibration');
    expect(lightGroup).toContainElement(screen.getByText('/astro/lights'));
    expect(calGroup).toContainElement(screen.getByText('/astro/cals'));
  });

  it('allows Step 4 (Processing Tools) to advance without changes', async () => {
    const seeded = {
      version: WIZARD_STATE_VERSION,
      currentStep: 3,
      sources: [
        { path: '/astro/lights', kind: 'light_frames' },
        { path: '/astro/projects', kind: 'project' },
      ],
      catalogSettings: {
        selectedCatalogIds: ['common', 'openngc'],
      },
      tools: {
        pixinsight: { enabled: false, path: null },
        siril: { enabled: false, path: null },
      },
    };
    window.localStorage.setItem(WIZARD_STORAGE_KEY, JSON.stringify(seeded));

    renderWizard();

    // We should be on the Processing Tools step (heading)
    expect(
      screen.getByRole('heading', { name: /processing tools/i }),
    ).toBeInTheDocument();
    expect(screen.getByText(/step 4 of 8/i)).toBeInTheDocument();

    // Continue should be enabled (tools step has no requirements)
    const continueBtn = getContinueButton();
    expect(continueBtn).not.toBeDisabled();

    // Click Continue — should advance to Catalogs step
    fireEvent.click(continueBtn);
    expect(
      screen.getByRole('heading', { name: /configuration/i }),
    ).toBeInTheDocument();
  });

  it('allows Step 5 (Catalogs) to advance without changes', async () => {
    const seeded = {
      version: WIZARD_STATE_VERSION,
      currentStep: 4,
      sources: [
        { path: '/astro/lights', kind: 'light_frames' },
        { path: '/astro/projects', kind: 'project' },
      ],
      catalogSettings: {
        selectedCatalogIds: ['common', 'openngc'],
      },
      tools: {
        pixinsight: { enabled: false, path: null },
        siril: { enabled: false, path: null },
      },
    };
    window.localStorage.setItem(WIZARD_STORAGE_KEY, JSON.stringify(seeded));

    renderWizard();

    // We should be on the Catalogs step (heading)
    expect(
      screen.getByRole('heading', { name: /configuration/i }),
    ).toBeInTheDocument();
    expect(screen.getByText(/step 5 of 8/i)).toBeInTheDocument();

    // Continue should be enabled
    const continueBtn = getContinueButton();
    expect(continueBtn).not.toBeDisabled();
  });

  it('allows Step 6 (Observing Site) to advance while completely empty (spec 044 T016, optional)', () => {
    const seeded = {
      version: WIZARD_STATE_VERSION,
      currentStep: 5,
      sources: [
        { path: '/astro/lights', kind: 'light_frames' },
        { path: '/astro/projects', kind: 'project' },
      ],
      catalogSettings: { selectedCatalogIds: ['common', 'openngc'] },
      tools: {
        pixinsight: { enabled: false, path: null },
        siril: { enabled: false, path: null },
      },
      site: {
        name: '',
        latitudeDegText: '',
        longitudeDegText: '',
        elevationMText: '',
        timezone: 'UTC',
      },
    };
    window.localStorage.setItem(WIZARD_STORAGE_KEY, JSON.stringify(seeded));

    renderWizard();

    expect(
      screen.getByRole('heading', { name: /where do you observe from/i }),
    ).toBeInTheDocument();
    expect(screen.getByText(/step 6 of 8/i)).toBeInTheDocument();

    // Never required — Continue stays enabled with every field blank (spec 044
    // T016). Skipping is acknowledged, not blocked (#1050): the first click
    // surfaces what is lost, the second proceeds.
    const continueBtn = getContinueButton();
    expect(continueBtn).not.toBeDisabled();

    fireEvent.click(continueBtn);

    // Still on the site step, now with the consequence spelled out.
    expect(screen.getByTestId('setup-site-skip-warning')).toBeInTheDocument();
    expect(
      screen.getByRole('heading', { name: /where do you observe from/i }),
    ).toBeInTheDocument();

    fireEvent.click(getContinueButton());
    expect(screen.getByText(/ready to go/i)).toBeInTheDocument();
  });

  it('re-arms the observing-site skip warning if the user edits then clears the step', () => {
    const seeded = {
      version: WIZARD_STATE_VERSION,
      currentStep: 5,
      sources: [
        { path: '/astro/lights', kind: 'light_frames' },
        { path: '/astro/projects', kind: 'project' },
      ],
      catalogSettings: { selectedCatalogIds: ['common', 'openngc'] },
      tools: {
        pixinsight: { enabled: false, path: null },
        siril: { enabled: false, path: null },
      },
      site: {
        name: '',
        latitudeDegText: '',
        longitudeDegText: '',
        elevationMText: '',
        timezone: 'UTC',
      },
    };
    window.localStorage.setItem(WIZARD_STORAGE_KEY, JSON.stringify(seeded));

    renderWizard();

    // Acknowledge the skip once.
    fireEvent.click(getContinueButton());
    expect(screen.getByTestId('setup-site-skip-warning')).toBeInTheDocument();

    // Touching the step withdraws that acknowledgement: typing a name hides the
    // warning, and clearing it again must not inherit the earlier consent.
    const nameField = screen.getByLabelText(/name/i);
    fireEvent.change(nameField, { target: { value: 'Backyard' } });
    expect(
      screen.queryByTestId('setup-site-skip-warning'),
    ).not.toBeInTheDocument();

    fireEvent.change(nameField, { target: { value: '' } });
    fireEvent.click(getContinueButton());

    // Warned again rather than advancing straight to Confirm.
    expect(screen.getByTestId('setup-site-skip-warning')).toBeInTheDocument();
    expect(screen.queryByText(/ready to go/i)).not.toBeInTheDocument();
  });

  it('blocks Continue on the Observing Site step when latitude is out of range', () => {
    const seeded = {
      version: WIZARD_STATE_VERSION,
      currentStep: 5,
      sources: [
        { path: '/astro/lights', kind: 'light_frames' },
        { path: '/astro/projects', kind: 'project' },
      ],
      catalogSettings: { selectedCatalogIds: ['common', 'openngc'] },
      tools: {
        pixinsight: { enabled: false, path: null },
        siril: { enabled: false, path: null },
      },
      site: {
        name: 'Backyard',
        latitudeDegText: '120',
        longitudeDegText: '4.9',
        elevationMText: '',
        timezone: 'UTC',
      },
    };
    window.localStorage.setItem(WIZARD_STORAGE_KEY, JSON.stringify(seeded));

    renderWizard();

    const continueBtn = getContinueButton();
    expect(continueBtn).toBeDisabled();
  });

  it('persists the site from the Observing Site step as default+active when finishing', async () => {
    mockSettingsUpdate.mockClear();

    // Seed at the Scan step's predecessor state directly at Confirm with a
    // filled-in site, then drive to Scan -> Finish the same way the
    // tool-persistence test does.
    const seeded = {
      version: WIZARD_STATE_VERSION,
      currentStep: 6,
      sources: [
        { path: '/astro/lights', kind: 'light_frames' },
        { path: '/astro/projects', kind: 'project' },
      ],
      catalogSettings: { selectedCatalogIds: [] },
      tools: {
        pixinsight: { enabled: false, path: null },
        siril: { enabled: false, path: null },
      },
      site: {
        name: 'Backyard',
        latitudeDegText: '52.37',
        longitudeDegText: '4.9',
        elevationMText: '2',
        timezone: 'Europe/Amsterdam',
      },
    };
    window.localStorage.setItem(WIZARD_STORAGE_KEY, JSON.stringify(seeded));

    renderWizard();
    expect(screen.getByText(/ready to go/i)).toBeInTheDocument();

    const startScanBtn = screen.getByRole('button', { name: /start scan/i });
    await act(async () => {
      fireEvent.click(startScanBtn);
      await new Promise((r) => setTimeout(r, 0));
    });

    const finishBtn = await screen.findByTestId('finish-button');
    await waitFor(() => expect(finishBtn).not.toBeDisabled());

    await act(async () => {
      fireEvent.click(finishBtn);
      await new Promise((r) => setTimeout(r, 0));
    });

    await waitFor(() =>
      expect(mockSettingsUpdate).toHaveBeenCalledWith(
        'observing',
        expect.objectContaining({
          observingSites: [
            expect.objectContaining({
              name: 'Backyard',
              latitudeDeg: 52.37,
              longitudeDeg: 4.9,
              elevationM: 2,
            }),
          ],
        }),
      ),
    );
    const call = mockSettingsUpdate.mock.calls.find(
      ([scope]) => scope === 'observing',
    );
    const values = call?.[1] as {
      observingSites: Array<{ id: string }>;
      observingDefaultSiteId: string;
      observingActiveSiteId: string;
    };
    const siteId = values.observingSites[0].id;
    expect(values.observingDefaultSiteId).toBe(siteId);
    expect(values.observingActiveSiteId).toBe(siteId);
  });

  it('does not call settingsUpdate for observing sites when the site step was left empty', async () => {
    mockSettingsUpdate.mockClear();
    mockFirstrunComplete.mockClear();

    const seeded = {
      version: WIZARD_STATE_VERSION,
      currentStep: 6,
      sources: [
        { path: '/astro/lights', kind: 'light_frames' },
        { path: '/astro/projects', kind: 'project' },
      ],
      catalogSettings: { selectedCatalogIds: [] },
      tools: {
        pixinsight: { enabled: false, path: null },
        siril: { enabled: false, path: null },
      },
      site: {
        name: '',
        latitudeDegText: '',
        longitudeDegText: '',
        elevationMText: '',
        timezone: 'UTC',
      },
    };
    window.localStorage.setItem(WIZARD_STORAGE_KEY, JSON.stringify(seeded));

    renderWizard();

    const startScanBtn = screen.getByRole('button', { name: /start scan/i });
    await act(async () => {
      fireEvent.click(startScanBtn);
      await new Promise((r) => setTimeout(r, 0));
    });

    const finishBtn = await screen.findByTestId('finish-button');
    await waitFor(() => expect(finishBtn).not.toBeDisabled());

    await act(async () => {
      fireEvent.click(finishBtn);
      await new Promise((r) => setTimeout(r, 0));
    });

    await waitFor(() => expect(mockFirstrunComplete).toHaveBeenCalledTimes(1));
    expect(mockSettingsUpdate).not.toHaveBeenCalledWith(
      'observing',
      expect.anything(),
    );
  });

  it('shows Confirm step (Step 7) with Start scan button', async () => {
    const seeded = {
      version: WIZARD_STATE_VERSION,
      currentStep: 6,
      sources: [
        { path: '/astro/lights', kind: 'light_frames' },
        { path: '/astro/projects', kind: 'project' },
        { path: '/astro/inbox', kind: 'inbox' },
      ],
      catalogSettings: {
        selectedCatalogIds: ['common', 'openngc'],
      },
      tools: {
        pixinsight: { enabled: false, path: null },
        siril: { enabled: false, path: null },
      },
    };
    window.localStorage.setItem(WIZARD_STORAGE_KEY, JSON.stringify(seeded));

    renderWizard();

    // Verify we are on the Confirm step
    expect(screen.getByText(/ready to go/i)).toBeInTheDocument();
    expect(screen.getByText(/step 7 of 8/i)).toBeInTheDocument();

    // "Start scan" button should be present and enabled (not "Complete setup")
    const startScanBtn = screen.getByRole('button', { name: /start scan/i });
    expect(startScanBtn).not.toBeDisabled();
    expect(
      screen.queryByRole('button', { name: /complete setup/i }),
    ).toBeNull();
  });

  it('blocks Start scan on Confirm step when required folders are missing', async () => {
    const seeded = {
      version: WIZARD_STATE_VERSION,
      currentStep: 6,
      sources: [{ path: '/astro/lights', kind: 'light_frames' }],
      catalogSettings: {
        selectedCatalogIds: ['common', 'openngc'],
      },
      tools: {
        pixinsight: { enabled: false, path: null },
        siril: { enabled: false, path: null },
      },
    };
    window.localStorage.setItem(WIZARD_STORAGE_KEY, JSON.stringify(seeded));

    renderWizard();

    expect(screen.getByText(/ready to go/i)).toBeInTheDocument();

    // Start scan should be disabled
    const startScanBtn = screen.getByRole('button', { name: /start scan/i });
    expect(startScanBtn).toBeDisabled();

    // Should show the blocked message
    expect(screen.getByText(/cannot complete setup/i)).toBeInTheDocument();
  });

  it('rejects a duplicate path within the same kind', async () => {
    renderWizardAtSources();

    // Add a folder path
    await addFolder('/home/user/astrophoto/raw');
    await waitFor(() => {
      expect(screen.getByText('/home/user/astrophoto/raw')).toBeInTheDocument();
    });

    // Confirm the footer shows 1 folder
    expect(screen.getByText(/1 folder selected/i)).toBeInTheDocument();

    // Add the same path again — the dedup check should reject it
    await addFolder('/home/user/astrophoto/raw');

    // After attempting to add a duplicate, the path should still appear only
    // once and the folder count should remain at 1.
    await waitFor(() => {
      const pathElements = screen.getAllByText('/home/user/astrophoto/raw');
      expect(pathElements).toHaveLength(1);
    });
    expect(screen.getByText(/1 folder selected/i)).toBeInTheDocument();
  });

  it('persists wizard processing-tool config to the backend when finishing', async () => {
    mockToolsUpdate.mockClear();
    mockFirstrunComplete.mockClear();

    const seeded = {
      version: WIZARD_STATE_VERSION,
      currentStep: 6,
      sources: [
        { path: '/astro/lights', kind: 'light_frames' },
        { path: '/astro/projects', kind: 'project' },
      ],
      catalogSettings: { selectedCatalogIds: [] },
      tools: {
        pixinsight: {
          enabled: true,
          path: '/Applications/PixInsight/PixInsight.app',
        },
        siril: { enabled: false, path: null },
      },
    };
    window.localStorage.setItem(WIZARD_STORAGE_KEY, JSON.stringify(seeded));

    renderWizard();

    // Confirm step renders.
    expect(screen.getByText(/ready to go/i)).toBeInTheDocument();

    // Advance to the Scan step by clicking "Start scan →".
    const startScanBtn = screen.getByRole('button', { name: /start scan/i });
    await act(async () => {
      fireEvent.click(startScanBtn);
      // Give flushToDB (mocked registerRootBatch) time to resolve.
      await new Promise((r) => setTimeout(r, 0));
    });

    // StepScan mounts and immediately calls inboxScanFolder for each source.
    // The mock resolves synchronously, so after a tick the scan is 'done'
    // and the Finish button becomes enabled.
    const finishBtn = await screen.findByTestId('finish-button');
    await waitFor(() => expect(finishBtn).not.toBeDisabled());

    await act(async () => {
      fireEvent.click(finishBtn);
      await new Promise((r) => setTimeout(r, 0));
    });

    // handleFinish (non-mock path: VITE_USE_MOCKS='false') must call toolsUpdate
    // once per tool before firstrunComplete.
    await waitFor(() => expect(mockToolsUpdate).toHaveBeenCalledTimes(2));

    expect(mockToolsUpdate).toHaveBeenCalledWith({
      id: 'pixinsight',
      enabled: true,
      path: '/Applications/PixInsight/PixInsight.app',
    });
    expect(mockToolsUpdate).toHaveBeenCalledWith({
      id: 'siril',
      enabled: false,
      path: null,
    });

    // firstrunComplete must be called exactly once, after the tool updates.
    // waitFor, not a sync assertion: handleFinish AWAITS each toolsUpdate
    // before reaching firstrunComplete (SetupWizard.tsx:384-415), so the
    // waitFor above — which only proves both toolsUpdate calls were dispatched
    // — still leaves firstrunComplete pending. Asserting it synchronously
    // fails with "expected 1 times, but got 0 times" (#1083).
    await waitFor(() => expect(mockFirstrunComplete).toHaveBeenCalledTimes(1));
  });
});

// ---------------------------------------------------------------------------
// Issue #704 — restart flow: already-registered batch items are benign no-ops
// ---------------------------------------------------------------------------

describe('SetupWizard restart re-confirm (issue #704)', () => {
  /** Seed the wizard at the Confirm step with the two required sources. */
  function seedConfirmStep() {
    window.localStorage.setItem(
      WIZARD_STORAGE_KEY,
      JSON.stringify({
        version: WIZARD_STATE_VERSION,
        currentStep: 6,
        sources: [
          {
            path: '/astro/lights',
            kind: 'light_frames',
            organizationState: 'organized',
          },
          {
            path: '/astro/projects',
            kind: 'project',
            organizationState: 'organized',
          },
        ],
      }),
    );
  }

  async function clickStartScan() {
    const startScanBtn = screen.getByRole('button', { name: /start scan/i });
    await act(async () => {
      fireEvent.click(startScanBtn);
      await new Promise((r) => setTimeout(r, 0));
    });
  }

  it('advances to Scan when every item is already registered (unchanged restart buffer)', async () => {
    seedConfirmStep();
    mockRootsRegisterBatch.mockResolvedValueOnce({
      status: 'ok',
      data: {
        status: 'failure',
        items: [
          {
            index: 0,
            status: 'failure',
            sourceId: null,
            error: 'path.already_registered',
            errorDetail: null,
          },
          {
            index: 1,
            status: 'failure',
            sourceId: null,
            error: 'path.already_registered',
            errorDetail: null,
          },
        ],
      },
    });

    renderWizard();
    await clickStartScan();

    // No misleading "batch failed" banner, and the wizard is on the Scan step.
    expect(screen.queryByTestId('setup-submit-error')).toBeNull();
    expect(screen.getByTestId('step-scan')).toBeInTheDocument();
  });

  it('advances to Scan and scans only the new folder in a mixed batch', async () => {
    seedConfirmStep();
    mockRootsRegisterBatch.mockResolvedValueOnce({
      status: 'ok',
      data: {
        status: 'partial',
        items: [
          {
            index: 0,
            status: 'failure',
            sourceId: null,
            error: 'path.already_registered',
            errorDetail: null,
          },
          {
            index: 1,
            status: 'success',
            sourceId: 'root-new-1',
            error: null,
            errorDetail: null,
          },
        ],
      },
    });

    renderWizard();
    await clickStartScan();

    expect(screen.queryByTestId('setup-submit-error')).toBeNull();
    expect(screen.getByTestId('step-scan')).toBeInTheDocument();

    // Only the newly registered folder is scanned — never the
    // already-registered one (its content is already ingested; scanning by
    // path-as-rootId would orphan inbox items).
    await waitFor(() => expect(mockInboxScanFolder).toHaveBeenCalledTimes(1));
    expect(mockInboxScanFolder).toHaveBeenCalledWith(
      expect.objectContaining({ rootId: 'root-new-1' }),
    );
  });

  it('still blocks on Confirm for genuine registration failures', async () => {
    seedConfirmStep();
    mockInboxScanFolder.mockClear();
    mockRootsRegisterBatch.mockResolvedValueOnce({
      status: 'ok',
      data: {
        status: 'partial',
        items: [
          {
            index: 0,
            status: 'failure',
            sourceId: null,
            error: 'path.not_exists',
            errorDetail: null,
          },
          {
            index: 1,
            status: 'success',
            sourceId: 'root-new-2',
            error: null,
            errorDetail: null,
          },
        ],
      },
    });

    renderWizard();
    await clickStartScan();

    expect(screen.getByTestId('setup-submit-error')).toBeInTheDocument();
    expect(screen.queryByTestId('step-scan')).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Issue #512 — step tabs are clickable with validation-gated forward jumps
// ---------------------------------------------------------------------------

describe('SetupWizard step-tab navigation (issue #512)', () => {
  it('focuses the new step heading after validated Continue and Back transitions', async () => {
    renderWizard();

    fireEvent.click(getContinueButton());
    const themeHeading = screen.getByRole('heading', {
      name: /choose how platevault looks/i,
    });
    await waitFor(() => expect(themeHeading).toHaveFocus());

    fireEvent.click(getContinueButton());
    const sourcesHeading = screen.getByRole('heading', {
      name: /where does your data live/i,
    });
    await waitFor(() => expect(sourcesHeading).toHaveFocus());

    fireEvent.click(screen.getByRole('button', { name: /back/i }));
    const restoredThemeHeading = screen.getByRole('heading', {
      name: /choose how platevault looks/i,
    });
    await waitFor(() => expect(restoredThemeHeading).toHaveFocus());

    fireEvent.click(screen.getByRole('button', { name: /back/i }));
    const languageHeading = screen.getByRole('heading', {
      name: /choose your language/i,
    });
    await waitFor(() => expect(languageHeading).toHaveFocus());
  });

  it('renders step tabs as buttons and marks the current step', () => {
    renderWizard();
    const languageTab = screen.getByRole('button', { name: /1\. Language/i });
    expect(languageTab).toHaveAttribute('aria-current', 'step');
  });

  it('disables forward tabs while step validation fails, enables them once it passes', async () => {
    renderWizardAtSources();

    // No folders yet: the Source Folders step fails validation, so every
    // forward tab is disabled.
    expect(
      screen.getByRole('button', { name: /4\. Processing Tools/i }),
    ).toBeDisabled();
    expect(screen.getByRole('button', { name: /7\. Confirm/i })).toBeDisabled();

    await addFolder('/astro/lights', 'light_frames');
    await addFolder('/astro/projects', 'project');

    // Required kinds present: intermediate steps and Confirm become reachable…
    await waitFor(() =>
      expect(
        screen.getByRole('button', { name: /4\. Processing Tools/i }),
      ).not.toBeDisabled(),
    );
    expect(
      screen.getByRole('button', { name: /7\. Confirm/i }),
    ).not.toBeDisabled();
    // …but Scan is never a plain jump target (it runs registration on entry).
    expect(screen.getByRole('button', { name: /8\. Scan/i })).toBeDisabled();
  });

  it('jumps forward and freely back via the tabs', async () => {
    renderWizardAtSources();
    await addFolder('/astro/lights', 'light_frames');
    await addFolder('/astro/projects', 'project');

    // Forward jump straight to Confirm (all gates pass).
    await waitFor(() =>
      expect(
        screen.getByRole('button', { name: /7\. Confirm/i }),
      ).not.toBeDisabled(),
    );
    fireEvent.click(screen.getByRole('button', { name: /7\. Confirm/i }));
    expect(screen.getByText(/ready to go/i)).toBeInTheDocument();

    // Free backward jump to any visited/earlier step.
    fireEvent.click(
      screen.getByRole('button', { name: /3\. Source Folders/i }),
    );
    expect(screen.getByText(/where does your data live/i)).toBeInTheDocument();
  });

  // spec 061 US1 T020: a language chosen by mistake must be recoverable
  // without completing setup first — Back must always reach the Language
  // step, and the tab bar must expose it as a free backward jump too.
  it('T020: Back navigation from Source Folders returns through Theme to Language', () => {
    renderWizardAtSources();
    expect(screen.getByText(/where does your data live/i)).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /back/i }));
    expect(
      screen.getByRole('heading', { name: /choose how platevault looks/i }),
    ).toBeInTheDocument();
    expect(screen.getByText(/step 2 of 8/i)).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /back/i }));
    expect(screen.getByText(/choose your language/i)).toBeInTheDocument();
    expect(screen.getByText(/step 1 of 8/i)).toBeInTheDocument();
  });

  it('T020: the Language tab is always a free backward jump once visited', () => {
    renderWizardAtSources();
    fireEvent.click(screen.getByRole('button', { name: /1\. Language/i }));
    expect(screen.getByText(/choose your language/i)).toBeInTheDocument();
  });
});
