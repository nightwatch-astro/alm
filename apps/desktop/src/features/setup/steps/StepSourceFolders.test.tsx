// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * StepSourceFolders — required-first ordering, help tooltips, add-time
 * validation, and scan-depth removal (issues #496, #497/#714, #502, #509).
 */

import {
  render,
  screen,
  fireEvent,
  waitFor,
  within,
} from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';

import { m } from '@/lib/i18n';

const mockPick = vi.fn();

vi.mock('@/shared/native', () => ({
  useDirectoryPicker: () => ({
    pick: mockPick,
    loading: false,
    error: null,
    clearError: vi.fn(),
  }),
  pickDirectory: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn().mockResolvedValue(null),
}));

// Manual-entry add-time existence check (#662) round-trips through
// `tools.validate_path` (commands.toolsValidatePath) — default to "exists" so
// the picker-flow tests above (which never mock this) keep passing; override
// per-test for the nonexistent-path coverage below.
const mockToolsValidatePath = vi.fn().mockResolvedValue({
  status: 'ok',
  data: { path: '', valid: true, reason: null },
});

vi.mock('@/bindings/index', () => ({
  commands: {
    toolsValidatePath: (path: string) => mockToolsValidatePath(path),
  },
}));

import { StepSourceFolders } from './StepSourceFolders';
import type { SourceEntry } from '../sources-store';

function makeEntry(path: string, kind: SourceEntry['kind']): SourceEntry {
  return { path, kind, organizationState: 'organized' };
}

const noop = vi.fn();

function renderStep(entries: SourceEntry[], onAdd = vi.fn()) {
  return render(
    <StepSourceFolders
      entries={entries}
      onAdd={onAdd}
      onRemove={noop}
      onKindChange={noop}
      onOrganizationStateChange={noop}
      errors={{}}
    />,
  );
}

// ── Required-first ordering (#496) ──────────────────────────────────────

describe('StepSourceFolders — required-first ordering', () => {
  it('renders both required groups before both optional groups, under Required/Optional headings', () => {
    renderStep([]);

    const groups = screen
      .getAllByTestId(/^source-group-/)
      .map((el) => el.dataset.testid);
    expect(groups).toEqual([
      'source-group-light_frames',
      'source-group-project',
      'source-group-calibration',
      'source-group-inbox',
    ]);

    const headings = screen
      .getAllByText(/^(Required|Optional)$/)
      .map((el) => el.textContent);
    expect(headings).toEqual(['Required', 'Optional']);
  });

  it('groups required and optional source kinds under a valid heading hierarchy', () => {
    renderStep([]);

    const requiredHeading = screen.getByRole('heading', {
      level: 2,
      name: 'Required',
    });
    const optionalHeading = screen.getByRole('heading', {
      level: 2,
      name: 'Optional',
    });
    const requiredSection = requiredHeading.closest('section');
    const optionalSection = optionalHeading.closest('section');

    expect(requiredSection).toHaveAccessibleName('Required');
    expect(optionalSection).toHaveAccessibleName('Optional');
    expect(
      within(requiredSection as HTMLElement)
        .getAllByRole('heading', { level: 3 })
        .map((heading) => heading.textContent),
    ).toEqual(['Light frames', 'Projects']);
    expect(
      within(optionalSection as HTMLElement)
        .getAllByRole('heading', { level: 3 })
        .map((heading) => heading.textContent),
    ).toEqual(['Calibration frames', 'Inbox']);
  });

  it('uses shared semantic pills for met, unmet, and optional statuses', () => {
    renderStep([makeEntry('/astro/lights', 'light_frames')]);

    const met = screen.getByTestId('requirement-status-light_frames');
    const unmet = screen.getByTestId('requirement-status-project');
    expect(met).toHaveClass('pv-pill', 'pv-pill--ok');
    expect(met).toHaveTextContent('required ✓');
    expect(unmet).toHaveClass('pv-pill', 'pv-pill--warn');
    expect(unmet).toHaveTextContent(/^required$/);
    for (const kind of ['calibration', 'inbox']) {
      const optional = screen.getByTestId(`requirement-status-${kind}`);
      expect(optional).toHaveClass('pv-pill', 'pv-pill--ghost');
      expect(optional).toHaveTextContent(/^optional$/);
    }
  });

  it('uses shared select and input primitives without changing control order', () => {
    renderStep([makeEntry('/astro/lights', 'light_frames')]);

    const group = screen.getByTestId('source-group-light_frames');
    const info = within(group).getByLabelText(/More information: Raw light/i);
    const choose = within(group).getByRole('button', {
      name: /add light frames folder/i,
    });
    const input = within(group).getByRole('textbox', {
      name: /light frames folder path/i,
    });
    const addByPath = within(group).getByTestId(
      'manual-add-path-btn-light_frames',
    );
    const organization = within(group).getByTestId('org-select-light_frames');

    expect(organization.tagName.toLowerCase()).toBe('select');
    expect(input).toHaveClass('pv-input', 'pv-step-sources__manual-input');
    expect(info.compareDocumentPosition(choose)).toBe(
      Node.DOCUMENT_POSITION_FOLLOWING,
    );
    expect(choose.compareDocumentPosition(input)).toBe(
      Node.DOCUMENT_POSITION_FOLLOWING,
    );
    expect(input.compareDocumentPosition(addByPath)).toBe(
      Node.DOCUMENT_POSITION_FOLLOWING,
    );
  });
});

// ── Contextual help tooltips (#497, #714) ───────────────────────────────

describe('StepSourceFolders — category help tooltips', () => {
  it('exposes an accessible, keyboard-focusable help affordance for every category', () => {
    renderStep([]);

    // aria-label mirrors the tip text, so it is available without hover
    // (screen-reader accessible per #497) — this also proves each category
    // has distinct explanatory copy (#714).
    expect(
      screen.getByLabelText(/More information: Raw light frames/i),
    ).toBeInTheDocument();
    expect(
      screen.getByLabelText(/More information: Dark, flat, and bias frames/i),
    ).toBeInTheDocument();
    expect(
      screen.getByLabelText(
        /More information: Folders where your PixInsight\/WBPP projects/i,
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByLabelText(/More information: An optional drop folder/i),
    ).toBeInTheDocument();
  });

  it('help affordances are focusable (tabIndex 0), not hover-only', () => {
    renderStep([]);
    const tip = screen.getByLabelText(/More information: Raw light frames/i);
    expect(tip).toHaveAttribute('tabIndex', '0');
  });
});

// ── Scan-depth control removed (#509) ───────────────────────────────────

describe('StepSourceFolders — scan-depth control removal', () => {
  it('no longer renders a scan-depth selector for any source row', () => {
    renderStep([makeEntry('/astro/lights', 'light_frames')]);
    expect(
      screen.queryByRole('combobox', { name: /scan depth/i }),
    ).not.toBeInTheDocument();
  });
});

// ── Add-time path validation (#502) ─────────────────────────────────────

describe('StepSourceFolders — add-time validation', () => {
  it('rejects a duplicate path within the same kind with an inline, accessible error and does not call onAdd', async () => {
    const onAdd = vi.fn();
    mockPick.mockResolvedValue({ path: '/astro/lights' });
    renderStep([makeEntry('/astro/lights', 'light_frames')], onAdd);

    fireEvent.click(
      screen.getByRole('button', { name: /add light frames folder/i }),
    );

    expect(await screen.findByRole('alert')).toHaveTextContent(
      /already added/i,
    );
    expect(onAdd).not.toHaveBeenCalled();
  });

  it('rejects a path already registered under a different kind, naming the other category', async () => {
    const onAdd = vi.fn();
    mockPick.mockResolvedValue({ path: '/astro/lights' });
    renderStep([makeEntry('/astro/lights', 'light_frames')], onAdd);

    fireEvent.click(
      screen.getByRole('button', { name: /add calibration frames folder/i }),
    );

    expect(await screen.findByRole('alert')).toHaveTextContent(
      /registered under Light frames/i,
    );
    expect(onAdd).not.toHaveBeenCalled();
  });

  it('rejects a folder nested inside an already added folder (no-overlap rule)', async () => {
    const onAdd = vi.fn();
    mockPick.mockResolvedValue({ path: '/astro/lights/sub' });
    renderStep([makeEntry('/astro/lights', 'light_frames')], onAdd);

    fireEvent.click(
      screen.getByRole('button', { name: /add light frames folder/i }),
    );

    expect(await screen.findByRole('alert')).toHaveTextContent(/overlaps/i);
    expect(onAdd).not.toHaveBeenCalled();
  });

  it('rejects a folder that is an ancestor of an already added folder', async () => {
    const onAdd = vi.fn();
    mockPick.mockResolvedValue({ path: '/astro' });
    renderStep([makeEntry('/astro/lights', 'light_frames')], onAdd);

    fireEvent.click(
      screen.getByRole('button', { name: /add light frames folder/i }),
    );

    expect(await screen.findByRole('alert')).toHaveTextContent(/overlaps/i);
    expect(onAdd).not.toHaveBeenCalled();
  });

  it('calls onAdd for a clear, non-overlapping path', async () => {
    const onAdd = vi.fn();
    mockPick.mockResolvedValue({ path: '/astro/lights2' });
    renderStep([makeEntry('/astro/lights', 'light_frames')], onAdd);

    fireEvent.click(
      screen.getByRole('button', { name: /add light frames folder/i }),
    );

    await waitFor(() =>
      expect(onAdd).toHaveBeenCalledWith('/astro/lights2', 'light_frames'),
    );
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });
});

// ── Manual path entry — existence validation (#662) ─────────────────────

describe('StepSourceFolders — manual path entry existence validation', () => {
  it('rejects a nonexistent path with an inline error and does not call onAdd, keeping the typed path', async () => {
    mockToolsValidatePath.mockResolvedValueOnce({
      status: 'ok',
      data: { path: '/astro/ghost', valid: false, reason: 'missing' },
    });
    const onAdd = vi.fn();
    renderStep([], onAdd);

    const input = screen.getByLabelText(/Light frames folder path/i);
    fireEvent.change(input, { target: { value: '/astro/ghost' } });
    fireEvent.click(screen.getByTestId('manual-add-path-btn-light_frames'));

    expect(await screen.findByRole('alert')).toHaveTextContent(
      /does not exist/i,
    );
    expect(onAdd).not.toHaveBeenCalled();
    // The rejected path stays in the input so the user can see/fix it.
    expect(input).toHaveValue('/astro/ghost');
  });

  it('adds a manually typed path that exists, clearing the input', async () => {
    mockToolsValidatePath.mockResolvedValueOnce({
      status: 'ok',
      data: { path: '/astro/lights', valid: true, reason: null, isDir: true },
    });
    const onAdd = vi.fn();
    renderStep([], onAdd);

    const input = screen.getByLabelText(/Light frames folder path/i);
    fireEvent.change(input, { target: { value: '/astro/lights' } });
    fireEvent.click(screen.getByTestId('manual-add-path-btn-light_frames'));

    await waitFor(() =>
      expect(onAdd).toHaveBeenCalledWith('/astro/lights', 'light_frames'),
    );
    expect(input).toHaveValue('');
  });

  // Issue #1056: a path that exists but points at a file (not a directory)
  // must be rejected inline instead of deferring to Confirm-step flush.
  it('rejects a manually typed path that points at a file, not a directory', async () => {
    mockToolsValidatePath.mockResolvedValueOnce({
      status: 'ok',
      data: {
        path: '/astro/lights/frame.fits',
        valid: true,
        reason: null,
        isDir: false,
      },
    });
    const onAdd = vi.fn();
    renderStep([], onAdd);

    const input = screen.getByLabelText(/Light frames folder path/i);
    fireEvent.change(input, { target: { value: '/astro/lights/frame.fits' } });
    fireEvent.click(screen.getByTestId('manual-add-path-btn-light_frames'));

    expect(await screen.findByRole('alert')).toHaveTextContent(
      m.err_path_not_directory(),
    );
    expect(onAdd).not.toHaveBeenCalled();
    expect(input).toHaveValue('/astro/lights/frame.fits');
  });
});
