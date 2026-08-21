// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vitest unit tests for CalibrationMatching (spec 007 / spec 043 P8).
 *
 * Covers the Offset "match required" toggle persistence path — the gap this
 * package closes (previously tagged STUB-OFFSET-REQUIRED: local-state-only,
 * no backend field). Mocks the generated bindings surface so the real
 * settingsIpc wrappers (calibrationTolerancesGet/Update) run and unwrap the
 * Result envelope, mirroring SourceProtectionOverride.test.tsx.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  render,
  screen,
  fireEvent,
  waitFor,
  within,
} from '@testing-library/react';
import { CalibrationMatching } from './CalibrationMatching';
import type { CalibrationTolerances } from './settingsIpc';

// ── Mocks ─────────────────────────────────────────────────────────────────────

const { mockGet, mockUpdate } = vi.hoisted(() => ({
  mockGet: vi.fn(),
  mockUpdate: vi.fn(),
}));

vi.mock('@/bindings/index', () => ({
  commands: {
    calibrationTolerancesGet: mockGet,
    calibrationTolerancesUpdate: mockUpdate,
  },
}));

// ── Helpers ───────────────────────────────────────────────────────────────────

function makeTolerances(
  overrides: Partial<CalibrationTolerances> = {},
): CalibrationTolerances {
  return {
    temperatureToleranceC: 5,
    exposureToleranceS: 2,
    agingLimitDays: 365,
    requireSameGain: true,
    requireSameBinning: true,
    requireSameOffset: true,
    ...overrides,
  };
}

/** The Offset row's toggle checkbox — table rows have no per-toggle aria-label,
 *  so scope the query to the row containing the "Offset" field label. */
function offsetToggleInput(): HTMLElement {
  const row = screen.getByText('Offset').closest('tr');
  if (!row) throw new Error('Offset row not found');
  return within(row).getByRole('checkbox');
}

/** The Gain row's toggle checkbox — used as a hydration marker: tests mock
 *  `requireSameGain: false` (differing from the in-code default `true`) so
 *  waiting for this toggle to become unchecked proves the fetched tolerances
 *  have been applied. Waiting on the Offset toggle alone is racy, because the
 *  in-code default (`requireSameOffset: true`) already satisfies "checked"
 *  before hydration lands. */
function gainToggleInput(): HTMLElement {
  const row = screen.getByText('Gain').closest('tr');
  if (!row) throw new Error('Gain row not found');
  return within(row).getByRole('checkbox');
}

beforeEach(() => {
  vi.clearAllMocks();
});

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('CalibrationMatching — offset match-required persistence', () => {
  it('loads requireSameOffset from the backend on mount', async () => {
    mockGet.mockResolvedValue({
      status: 'ok',
      data: makeTolerances({ requireSameOffset: false }),
    });
    render(<CalibrationMatching save={vi.fn()} />);

    await waitFor(() => {
      expect(offsetToggleInput()).not.toBeChecked();
    });
  });

  it('persists the offset toggle via calibration.tolerances.update, not just local state', async () => {
    mockGet.mockResolvedValue({
      status: 'ok',
      data: makeTolerances({
        requireSameGain: false,
        requireSameOffset: true,
      }),
    });
    mockUpdate.mockResolvedValue({
      status: 'ok',
      data: makeTolerances({ requireSameOffset: false }),
    });

    render(<CalibrationMatching save={vi.fn()} />);
    // Wait for hydration (gain flips to the fetched non-default value), not
    // just the offset toggle — the in-code default already renders it checked.
    await waitFor(() => expect(gainToggleInput()).not.toBeChecked());
    expect(offsetToggleInput()).toBeChecked();

    fireEvent.click(offsetToggleInput());

    await waitFor(() => {
      expect(mockUpdate).toHaveBeenCalledWith(
        expect.objectContaining({ requireSameOffset: false }),
      );
    });
    expect(offsetToggleInput()).not.toBeChecked();
  });

  it('does not persist sibling fields as a side effect of toggling offset', async () => {
    mockGet.mockResolvedValue({
      status: 'ok',
      data: makeTolerances({
        requireSameGain: false,
        requireSameOffset: true,
      }),
    });
    mockUpdate.mockResolvedValue({ status: 'ok', data: makeTolerances() });

    render(<CalibrationMatching save={vi.fn()} />);
    // Hydration marker: gain arrives as false (non-default). Waiting on the
    // offset toggle alone races the fetch — its in-code default is already
    // checked, so an early click would persist the *default* gain value.
    await waitFor(() => expect(gainToggleInput()).not.toBeChecked());
    expect(offsetToggleInput()).toBeChecked();

    fireEvent.click(offsetToggleInput());

    await waitFor(() => expect(mockUpdate).toHaveBeenCalled());
    // The persisted patch carries the *current* (unrelated) requireSameGain
    // state through unchanged — this pane always sends the full DTO.
    expect(mockUpdate).toHaveBeenCalledWith(
      expect.objectContaining({
        requireSameGain: false,
        requireSameOffset: false,
      }),
    );
  });

  it('restore-defaults resets requireSameOffset to true and persists it', async () => {
    mockGet.mockResolvedValue({
      status: 'ok',
      data: makeTolerances({ requireSameOffset: false }),
    });
    mockUpdate.mockResolvedValue({
      status: 'ok',
      data: makeTolerances({ requireSameOffset: true }),
    });

    render(<CalibrationMatching save={vi.fn()} />);
    await waitFor(() => expect(offsetToggleInput()).not.toBeChecked());

    fireEvent.click(screen.getByRole('button', { name: /restore/i }));

    await waitFor(() => {
      expect(mockUpdate).toHaveBeenCalledWith(
        expect.objectContaining({ requireSameOffset: true }),
      );
    });
  });

  it('falls back to the in-code default (true) when the backend is unavailable', async () => {
    mockGet.mockRejectedValue('network error');
    render(<CalibrationMatching save={vi.fn()} />);

    // No assertion to wait on for a rejected get; flush microtasks instead.
    await waitFor(() => {
      expect(mockGet).toHaveBeenCalled();
    });
    expect(offsetToggleInput()).toBeChecked();
  });

  it('a slow initial fetch does not clobber an edit made before it resolves', async () => {
    // Reproduces a real race, not just CI flakiness: the mount-time
    // calibrationTolerancesGet() fetch can still be in flight when the
    // user's first toggle fires. If the late resolution unconditionally
    // overwrites state, the user's edit reverts (same class of bug as the
    // Ingestion pane's startup-fetch race).
    let resolveGet!: (value: {
      status: 'ok';
      data: CalibrationTolerances;
    }) => void;
    mockGet.mockReturnValueOnce(
      new Promise((resolve) => {
        resolveGet = resolve;
      }),
    );
    mockUpdate.mockResolvedValue({
      status: 'ok',
      data: makeTolerances({ requireSameOffset: false }),
    });

    render(<CalibrationMatching save={vi.fn()} />);

    // Edit fires before the mount fetch has resolved.
    fireEvent.click(offsetToggleInput());
    await waitFor(() => expect(offsetToggleInput()).not.toBeChecked());

    // Now let the slow initial fetch resolve with the stale "true" value.
    resolveGet({
      status: 'ok',
      data: makeTolerances({ requireSameOffset: true }),
    });
    await Promise.resolve();
    await Promise.resolve();

    // The user's edit must survive — the late fetch must not stomp it.
    expect(offsetToggleInput()).not.toBeChecked();
  });
});

describe('CalibrationMatching — exposureToleranceS never sent as null (#639)', () => {
  it('round-trips the fetched exposureToleranceS instead of sending null', async () => {
    // Backend DTO field is a non-optional f64; a null payload fails Tauri arg
    // deserialization and the pane's own catch swallowed every rejection,
    // making all updates in this pane silently no-op.
    mockGet.mockResolvedValue({
      status: 'ok',
      data: makeTolerances({
        exposureToleranceS: 3.5,
        // Hydration marker — see `gainToggleInput`.
        requireSameGain: false,
      }),
    });
    mockUpdate.mockResolvedValue({ status: 'ok', data: makeTolerances() });

    render(<CalibrationMatching save={vi.fn()} />);
    // Gate on the Gain toggle, not the Offset one: `requireSameOffset` is
    // `true` in both the in-code default and the mocked payload, so waiting on
    // it cannot prove the fetch landed. The click would then race hydration and
    // send the default `2` instead of the fetched `3.5` (#1213).
    await waitFor(() => expect(gainToggleInput()).not.toBeChecked());

    fireEvent.click(offsetToggleInput());

    await waitFor(() => expect(mockUpdate).toHaveBeenCalled());
    const call = mockUpdate.mock.calls[0][0] as { exposureToleranceS: number };
    expect(call.exposureToleranceS).toBe(3.5);
  });

  it('restore-defaults sends the migration default (2.0), not null', async () => {
    mockGet.mockResolvedValue({ status: 'ok', data: makeTolerances() });
    mockUpdate.mockResolvedValue({ status: 'ok', data: makeTolerances() });

    render(<CalibrationMatching save={vi.fn()} />);
    await waitFor(() => expect(offsetToggleInput()).toBeChecked());

    fireEvent.click(screen.getByRole('button', { name: /restore/i }));

    await waitFor(() => {
      expect(mockUpdate).toHaveBeenCalledWith(
        expect.objectContaining({ exposureToleranceS: 2.0 }),
      );
    });
  });
});
