// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * ObservingSites pane tests (spec 044 Track B, US3, T023).
 *
 * Covers, against the settings-backed `site-store.ts` (mocked at the
 * `@/bindings/index` boundary so the real `saveSites`/`unwrap()` path runs):
 *   1. Empty state renders the "no sites yet" message.
 *   2. Adding the first site persists it as both default AND active
 *      (US6 continuity — no separate "make active" step to leave no-site).
 *   3. Editing a site persists the updated fields under the same id.
 *   4. Setting a different site active/default persists via `settingsUpdate`
 *      without touching the other pointer or the site list.
 *   5. Deleting the active site reselects the default (T020); deleting the
 *      last remaining site clears both pointers to the no-site state.
 *   6. A `settingsUpdate` failure surfaces a save error instead of silently
 *      losing the edit.
 */

import {
  render,
  screen,
  fireEvent,
  waitFor,
  within,
  act,
} from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';

const { mockSettingsUpdate } = vi.hoisted(() => ({
  mockSettingsUpdate: vi.fn(),
}));

vi.mock('@/bindings/index', () => ({
  commands: {
    settingsGet: vi.fn(),
    settingsUpdate: mockSettingsUpdate,
  },
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockRejectedValue(new Error('no tauri in tests')),
}));

import { ObservingSites } from './ObservingSites';
import { __setObservingStateForTest } from './site-store';
import type { ObserverSite } from './observer-site';
import { m } from '@/lib/i18n';

/** Wrap a value in the generated `{ status: 'ok' }` Result envelope. */
const ok = <T,>(data: T) => ({ status: 'ok' as const, data });
/** Wrap a ContractError in the generated `{ status: 'error' }` Result envelope. */
const err = (error: unknown) => ({ status: 'error' as const, error });

const HOME: ObserverSite = {
  id: 'site-home',
  name: 'Backyard',
  latitudeDeg: 52.37,
  longitudeDeg: 4.9,
  elevationM: 2,
  timezone: 'Europe/Amsterdam',
  twilight: 'astronomical',
  minHorizonAltDeg: 10,
};

const DARK_SKY: ObserverSite = {
  id: 'site-dark',
  name: 'Dark-sky site',
  latitudeDeg: 51.0,
  longitudeDeg: 5.5,
  elevationM: null,
  timezone: 'Europe/Amsterdam',
  twilight: 'nautical',
  minHorizonAltDeg: 0,
};

const THIRD_SITE: ObserverSite = {
  id: 'site-third',
  name: 'Remote site',
  latitudeDeg: 51.5,
  longitudeDeg: -0.13,
  elevationM: null,
  timezone: 'Europe/London',
  twilight: 'astronomical',
  minHorizonAltDeg: 0,
};

beforeEach(() => {
  vi.clearAllMocks();
  mockSettingsUpdate.mockResolvedValue(ok(null));
  __setObservingStateForTest({});
});

describe('ObservingSites', () => {
  it('shows the empty state with no sites', () => {
    render(<ObservingSites />);
    expect(
      screen.getByText(m.settings_observing_sites_empty()),
    ).toBeInTheDocument();
  });

  it('adding the first site persists it as default AND active', async () => {
    render(<ObservingSites />);

    fireEvent.click(screen.getByText(m.settings_observing_sites_add()));
    fireEvent.change(
      screen.getByLabelText(m.settings_observing_sites_field_name()),
      {
        target: { value: 'Backyard' },
      },
    );
    fireEvent.change(
      screen.getByLabelText(m.settings_observing_sites_field_latitude()),
      {
        target: { value: '52.37' },
      },
    );
    fireEvent.change(
      screen.getByLabelText(m.settings_observing_sites_field_longitude()),
      {
        target: { value: '4.9' },
      },
    );

    await act(async () => {
      fireEvent.click(screen.getByText(m.common_save()));
      await Promise.resolve();
    });

    expect(mockSettingsUpdate).toHaveBeenCalledTimes(1);
    const [scope, values] = mockSettingsUpdate.mock.calls[0] as [
      string,
      Record<string, unknown>,
    ];
    expect(scope).toBe('observing');
    const sites = values['observingSites'] as ObserverSite[];
    expect(sites).toHaveLength(1);
    expect(sites[0].name).toBe('Backyard');
    expect(values['observingDefaultSiteId']).toBe(sites[0].id);
    expect(values['observingActiveSiteId']).toBe(sites[0].id);

    await waitFor(() =>
      expect(screen.getByText('Backyard')).toBeInTheDocument(),
    );
    expect(
      screen.getByText(m.settings_observing_sites_default_badge()),
    ).toBeInTheDocument();
    expect(
      screen.getByText(m.settings_observing_sites_active_badge()),
    ).toBeInTheDocument();
  });

  it('rejects an out-of-range latitude', () => {
    render(<ObservingSites />);

    fireEvent.click(screen.getByText(m.settings_observing_sites_add()));
    fireEvent.change(
      screen.getByLabelText(m.settings_observing_sites_field_name()),
      {
        target: { value: 'Bad site' },
      },
    );
    fireEvent.change(
      screen.getByLabelText(m.settings_observing_sites_field_latitude()),
      {
        target: { value: '120' },
      },
    );
    fireEvent.change(
      screen.getByLabelText(m.settings_observing_sites_field_longitude()),
      {
        target: { value: '0' },
      },
    );

    fireEvent.click(screen.getByText(m.common_save()));

    expect(
      screen.getByText(m.settings_observing_sites_error_latitude()),
    ).toBeInTheDocument();
    expect(mockSettingsUpdate).not.toHaveBeenCalled();
  });

  it('edits an existing site in place', async () => {
    __setObservingStateForTest({
      sites: [HOME],
      defaultSiteId: HOME.id,
      activeSiteId: HOME.id,
    });

    render(<ObservingSites />);
    fireEvent.click(screen.getByText(m.common_edit()));

    const nameInput = screen.getByLabelText(
      m.settings_observing_sites_field_name(),
    );
    fireEvent.change(nameInput, { target: { value: 'Backyard (renamed)' } });

    await act(async () => {
      fireEvent.click(screen.getByText(m.common_save()));
      await Promise.resolve();
    });

    const [, values] = mockSettingsUpdate.mock.calls[0] as [
      string,
      Record<string, unknown>,
    ];
    const sites = values['observingSites'] as ObserverSite[];
    expect(sites).toHaveLength(1);
    expect(sites[0].id).toBe(HOME.id);
    expect(sites[0].name).toBe('Backyard (renamed)');
    // Default/active pointers are unchanged by an edit.
    expect(values['observingDefaultSiteId']).toBe(HOME.id);
    expect(values['observingActiveSiteId']).toBe(HOME.id);
  });

  it('switches the active site without touching the default', async () => {
    __setObservingStateForTest({
      sites: [HOME, DARK_SKY],
      defaultSiteId: HOME.id,
      activeSiteId: HOME.id,
    });

    render(<ObservingSites />);

    const darkSkyRow = screen
      .getByText(DARK_SKY.name)
      .closest('tr') as HTMLElement;
    fireEvent.click(
      within(darkSkyRow).getByText(m.settings_observing_sites_set_active()),
    );

    await waitFor(() => expect(mockSettingsUpdate).toHaveBeenCalledTimes(1));
    const [, values] = mockSettingsUpdate.mock.calls[0] as [
      string,
      Record<string, unknown>,
    ];
    expect(values['observingActiveSiteId']).toBe(DARK_SKY.id);
    expect(values['observingDefaultSiteId']).toBe(HOME.id);
  });

  it('deleting the active site reselects the default site (T020)', async () => {
    __setObservingStateForTest({
      sites: [HOME, DARK_SKY],
      defaultSiteId: HOME.id,
      activeSiteId: DARK_SKY.id,
    });

    render(<ObservingSites />);

    const darkSkyRow = screen
      .getByText(DARK_SKY.name)
      .closest('tr') as HTMLElement;
    fireEvent.click(within(darkSkyRow).getByText(m.common_remove()));

    const dialog = await screen.findByRole('dialog');
    await act(async () => {
      fireEvent.click(within(dialog).getByText(m.common_remove()));
      await Promise.resolve();
    });

    const [, values] = mockSettingsUpdate.mock.calls[0] as [
      string,
      Record<string, unknown>,
    ];
    const sites = values['observingSites'] as ObserverSite[];
    expect(sites).toHaveLength(1);
    expect(sites[0].id).toBe(HOME.id);
    expect(values['observingActiveSiteId']).toBe(HOME.id);
    expect(values['observingDefaultSiteId']).toBe(HOME.id);
  });

  it('deleting the last site clears default/active to the no-site state', async () => {
    __setObservingStateForTest({
      sites: [HOME],
      defaultSiteId: HOME.id,
      activeSiteId: HOME.id,
    });

    render(<ObservingSites />);
    fireEvent.click(screen.getByText(m.common_remove()));

    const dialog = await screen.findByRole('dialog');
    await act(async () => {
      fireEvent.click(within(dialog).getByText(m.common_remove()));
      await Promise.resolve();
    });

    const [, values] = mockSettingsUpdate.mock.calls[0] as [
      string,
      Record<string, unknown>,
    ];
    expect(values['observingSites']).toEqual([]);
    expect(values['observingActiveSiteId']).toBeNull();
    expect(values['observingDefaultSiteId']).toBeNull();

    await waitFor(() =>
      expect(
        screen.getByText(m.settings_observing_sites_empty()),
      ).toBeInTheDocument(),
    );
  });

  // #840: with 2+ non-target candidates remaining, removing the active site
  // must force an explicit fallback choice instead of auto-selecting one.
  it('requires an explicit fallback choice when removing the active site with 2+ candidates remaining', async () => {
    __setObservingStateForTest({
      sites: [HOME, DARK_SKY, THIRD_SITE],
      defaultSiteId: HOME.id,
      activeSiteId: DARK_SKY.id,
    });

    render(<ObservingSites />);

    const darkSkyRow = screen
      .getByText(DARK_SKY.name)
      .closest('tr') as HTMLElement;
    fireEvent.click(within(darkSkyRow).getByText(m.common_remove()));

    const dialog = await screen.findByRole('dialog');
    // Confirm without choosing a fallback: blocked, no update fires.
    await act(async () => {
      fireEvent.click(within(dialog).getByText(m.common_remove()));
      await Promise.resolve();
    });
    expect(mockSettingsUpdate).not.toHaveBeenCalled();
    expect(
      within(dialog).getByText(m.settings_observing_sites_fallback_required()),
    ).toBeInTheDocument();

    fireEvent.change(
      within(dialog).getByLabelText(
        m.settings_observing_sites_fallback_label(),
      ),
      { target: { value: THIRD_SITE.id } },
    );
    await act(async () => {
      fireEvent.click(within(dialog).getByText(m.common_remove()));
      await Promise.resolve();
    });

    expect(mockSettingsUpdate).toHaveBeenCalledTimes(1);
    const [, values] = mockSettingsUpdate.mock.calls[0] as [
      string,
      Record<string, unknown>,
    ];
    expect(values['observingActiveSiteId']).toBe(THIRD_SITE.id);
    // Default was untouched (HOME) since only the active pointer targeted
    // the removed site.
    expect(values['observingDefaultSiteId']).toBe(HOME.id);
  });

  it('shows a save error when the backend rejects the update', async () => {
    mockSettingsUpdate.mockResolvedValue(
      err({
        code: 'internal.database',
        message: 'db down',
        severity: 'blocking',
        retryable: true,
      }),
    );

    render(<ObservingSites />);
    fireEvent.click(screen.getByText(m.settings_observing_sites_add()));
    fireEvent.change(
      screen.getByLabelText(m.settings_observing_sites_field_name()),
      {
        target: { value: 'Backyard' },
      },
    );
    fireEvent.change(
      screen.getByLabelText(m.settings_observing_sites_field_latitude()),
      {
        target: { value: '52.37' },
      },
    );
    fireEvent.change(
      screen.getByLabelText(m.settings_observing_sites_field_longitude()),
      {
        target: { value: '4.9' },
      },
    );

    await act(async () => {
      fireEvent.click(screen.getByText(m.common_save()));
      await Promise.resolve();
    });

    expect(
      screen.getByText(
        m.settings_observing_sites_save_error({
          error: m.err_internal_database(),
        }),
      ),
    ).toBeInTheDocument();
  });
});
