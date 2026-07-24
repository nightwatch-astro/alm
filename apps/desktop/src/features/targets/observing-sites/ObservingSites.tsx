// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * ObservingSites — Settings → Target Planner → observing-site management
 * (spec 044 Track B, US3, T019-T023).
 *
 * Site CRUD (name, lat/lon, elevation, IANA timezone, twilight, min-horizon)
 * plus default/active selection, all persisted through `site-store.ts`
 * (settings-backed, fully offline — FR-011/FR-012). Default/active pointers
 * are kept valid across edits/deletes here (FR-013, T020): deleting the
 * active site reselects the default (or clears to no-site); deleting the
 * default site falls back to another remaining site (or clears). Switching
 * the active site recomputes observability immediately because every planner
 * consumer subscribes to `useActiveSite()`/`useObservingState()` directly
 * (T022); the settings write makes the choice durable across relaunch
 * (SC-005/SC-006).
 *
 * Reuses the shared `SettingsFormShell`/`.pv-equipment__form*` frame — one
 * parameterised add/edit form, not a per-pane clone.
 */

import { lazy, Suspense, useState } from 'react';
import { Btn, Table, Pill } from '@/ui';
import type { TableRow } from '@/ui';
import { Modal } from '@/components';
import { m } from '@/lib/i18n';

// Leaflet is ~240 KB raw. Lazy-load so it splits to its own chunk and only
// loads when the add/edit form mounts for the first time.
const SiteLocationPicker = lazy(() =>
  import('./SiteLocationPicker').then((mod) => ({
    default: mod.SiteLocationPicker,
  })),
);

/** Parsed coordinate for the map pin, or `null` when blank/invalid mid-edit. */
function parsedCoord(text: string): number | null {
  const trimmed = text.trim();
  if (trimmed === '') return null;
  const n = Number(trimmed);
  return Number.isFinite(n) ? n : null;
}
import { errMessage } from '@/lib/errors';
import {
  SettingsSection,
  SettingsFormShell,
} from '@/features/settings/SettingsKit';
import { useObservingState, saveSites } from './site-store';
import type { ObserverSite, Twilight } from './observer-site';
import { ianaTimezones, localTimezone } from './iana-timezones';
import { selectBase } from '@/styles/select.css';
import { message as modalMessage } from '@/components/modal.css';

interface SiteForm {
  id: string | null;
  name: string;
  latitudeDegText: string;
  longitudeDegText: string;
  elevationMText: string;
  timezone: string;
  twilight: Twilight;
  minHorizonAltDegText: string;
}

function emptyForm(): SiteForm {
  return {
    id: null,
    name: '',
    latitudeDegText: '',
    longitudeDegText: '',
    elevationMText: '',
    timezone: localTimezone(),
    twilight: 'astronomical',
    minHorizonAltDegText: '0',
  };
}

function formFromSite(site: ObserverSite): SiteForm {
  return {
    id: site.id,
    name: site.name,
    latitudeDegText: String(site.latitudeDeg),
    longitudeDegText: String(site.longitudeDeg),
    elevationMText: site.elevationM === null ? '' : String(site.elevationM),
    timezone: site.timezone,
    twilight: site.twilight,
    minHorizonAltDegText: String(site.minHorizonAltDeg),
  };
}

/** Parses a bounded numeric field; out-of-range or non-numeric -> null. */
function parseRange(text: string, lo: number, hi: number): number | null {
  const n = Number(text.trim());
  if (!Number.isFinite(n) || n < lo || n > hi) return null;
  return n;
}

function newSiteId(): string {
  if (
    typeof crypto !== 'undefined' &&
    typeof crypto.randomUUID === 'function'
  ) {
    return crypto.randomUUID();
  }
  return `site-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

export function ObservingSites() {
  const { sites, defaultSiteId, activeSiteId } = useObservingState();
  const [form, setForm] = useState<SiteForm | null>(null);
  const [formError, setFormError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [listError, setListError] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<ObserverSite | null>(null);
  const [deleteBusy, setDeleteBusy] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  // #840: chosen fallback when removing a site that holds the default/active
  // pointer and more than one other candidate remains — forces an explicit
  // choice instead of silently picking the first remaining site.
  const [fallbackSiteId, setFallbackSiteId] = useState<string | null>(null);
  const timezones = ianaTimezones();

  const deleteRemaining = deleteTarget
    ? sites.filter((s) => s.id !== deleteTarget.id)
    : [];
  const deleteNeedsFallbackChoice =
    deleteTarget != null &&
    deleteRemaining.length > 1 &&
    (deleteTarget.id === activeSiteId || deleteTarget.id === defaultSiteId);

  const startAdd = () => {
    setForm(emptyForm());
    setFormError(null);
  };

  const startEdit = (site: ObserverSite) => {
    setForm(formFromSite(site));
    setFormError(null);
  };

  const handleSubmit = async () => {
    if (!form) return;
    const name = form.name.trim();
    if (!name) {
      setFormError(m.settings_observing_sites_error_name());
      return;
    }
    const latitudeDeg = parseRange(form.latitudeDegText, -90, 90);
    if (latitudeDeg === null) {
      setFormError(m.settings_observing_sites_error_latitude());
      return;
    }
    const longitudeDeg = parseRange(form.longitudeDegText, -180, 180);
    if (longitudeDeg === null) {
      setFormError(m.settings_observing_sites_error_longitude());
      return;
    }
    const elevationRaw = form.elevationMText.trim();
    let elevationM: number | null = null;
    if (elevationRaw !== '') {
      const n = Number(elevationRaw);
      if (!Number.isFinite(n)) {
        setFormError(m.settings_observing_sites_error_elevation());
        return;
      }
      elevationM = n;
    }
    const minHorizonAltDeg = parseRange(form.minHorizonAltDegText, 0, 90);
    if (minHorizonAltDeg === null) {
      setFormError(m.settings_observing_sites_error_horizon());
      return;
    }

    const site: ObserverSite = {
      id: form.id ?? newSiteId(),
      name,
      latitudeDeg,
      longitudeDeg,
      elevationM,
      timezone: form.timezone,
      twilight: form.twilight,
      minHorizonAltDeg,
    };

    const nextSites = form.id
      ? sites.map((s) => (s.id === site.id ? site : s))
      : [...sites, site];
    // The first site a user ever adds becomes default+active automatically
    // (US6 continuity: no separate "make this active" step needed to leave
    // the no-site state).
    const isFirstSite = form.id === null && sites.length === 0;
    const nextDefaultSiteId = isFirstSite ? site.id : defaultSiteId;
    const nextActiveSiteId = isFirstSite ? site.id : activeSiteId;

    setSaving(true);
    setFormError(null);
    try {
      await saveSites(nextSites, nextDefaultSiteId, nextActiveSiteId);
      setForm(null);
    } catch (err: unknown) {
      setFormError(
        m.settings_observing_sites_save_error({ error: errMessage(err) }),
      );
    } finally {
      setSaving(false);
    }
  };

  const handleSetDefault = async (id: string) => {
    setListError(null);
    try {
      await saveSites(sites, id, activeSiteId);
    } catch (err: unknown) {
      setListError(
        m.settings_observing_sites_save_error({ error: errMessage(err) }),
      );
    }
  };

  const handleSetActive = async (id: string) => {
    setListError(null);
    try {
      await saveSites(sites, defaultSiteId, id);
    } catch (err: unknown) {
      setListError(
        m.settings_observing_sites_save_error({ error: errMessage(err) }),
      );
    }
  };

  const requestDelete = (site: ObserverSite) => {
    setDeleteError(null);
    setFallbackSiteId(null);
    setDeleteTarget(site);
  };

  const closeDeleteConfirm = () => {
    if (deleteBusy) return;
    setDeleteTarget(null);
    setDeleteError(null);
    setFallbackSiteId(null);
  };

  const handleConfirmDelete = async () => {
    if (!deleteTarget) return;
    const remaining = sites.filter((s) => s.id !== deleteTarget.id);
    // #840: when removing the default/active site with 2+ remaining
    // candidates, the choice is ambiguous — require the user to pick rather
    // than silently taking the first remaining site.
    const needsFallbackChoice =
      remaining.length > 1 &&
      (deleteTarget.id === activeSiteId || deleteTarget.id === defaultSiteId);
    if (needsFallbackChoice && !fallbackSiteId) {
      setDeleteError(m.settings_observing_sites_fallback_required());
      return;
    }
    setDeleteBusy(true);
    setDeleteError(null);
    try {
      // T020: keep default/active valid — reassign to the chosen fallback (or
      // the sole remaining site when unambiguous) when the deleted site held
      // either pointer, or clear to the no-site state when nothing is left.
      const fallbackId = needsFallbackChoice
        ? fallbackSiteId
        : (remaining[0]?.id ?? null);
      const nextDefaultSiteId =
        defaultSiteId === deleteTarget.id ? fallbackId : defaultSiteId;
      const nextActiveSiteId =
        activeSiteId === deleteTarget.id ? fallbackId : activeSiteId;
      await saveSites(remaining, nextDefaultSiteId, nextActiveSiteId);
      setDeleteTarget(null);
      setFallbackSiteId(null);
    } catch (err: unknown) {
      setDeleteError(
        m.settings_observing_sites_delete_error({ error: errMessage(err) }),
      );
    } finally {
      setDeleteBusy(false);
    }
  };

  return (
    <SettingsSection
      title={m.settings_observing_sites_title()}
      action={
        <Btn size="sm" onClick={startAdd}>
          {m.settings_observing_sites_add()}
        </Btn>
      }
    >
      {listError && <p className="pv-equipment__load-error">{listError}</p>}

      {sites.length === 0 ? (
        <p className="pv-equipment__empty">
          {m.settings_observing_sites_empty()}
        </p>
      ) : (
        <Table
          columns={[
            { key: 'name', label: m.settings_observing_sites_col_name() },
            { key: 'coords', label: m.settings_observing_sites_col_coords() },
            {
              key: 'timezone',
              label: m.settings_observing_sites_col_timezone(),
            },
            { key: 'status', label: m.settings_observing_sites_col_status() },
            {
              key: 'actions',
              label: '',
              style: { width: 260 },
              // #838: bound the cell width too (Table only applies `style` to
              // the header) so the 4-action row-actions flex group actually
              // wraps instead of forcing the column past the viewport edge.
              cellStyle: { width: 260 },
            },
          ]}
          rows={sites.map(
            (site): TableRow => ({
              name: site.name,
              coords: (
                <code className="pv-mono">
                  {m.settings_observing_sites_coords_value({
                    lat: site.latitudeDeg.toFixed(4),
                    lon: site.longitudeDeg.toFixed(4),
                  })}
                </code>
              ),
              timezone: site.timezone,
              status: (
                <span className="pv-equipment__badges">
                  {site.id === defaultSiteId && (
                    <Pill variant="info">
                      {m.settings_observing_sites_default_badge()}
                    </Pill>
                  )}
                  {site.id === activeSiteId && (
                    <Pill variant="neutral">
                      {m.settings_observing_sites_active_badge()}
                    </Pill>
                  )}
                </span>
              ),
              actions: (
                <span className="pv-equipment__row-actions">
                  {site.id !== activeSiteId && (
                    <Btn
                      size="sm"
                      variant="ghost"
                      onClick={() => void handleSetActive(site.id)}
                    >
                      {m.settings_observing_sites_set_active()}
                    </Btn>
                  )}
                  {site.id !== defaultSiteId && (
                    <Btn
                      size="sm"
                      variant="ghost"
                      onClick={() => void handleSetDefault(site.id)}
                    >
                      {m.settings_observing_sites_set_default()}
                    </Btn>
                  )}
                  <Btn
                    size="sm"
                    variant="ghost"
                    onClick={() => startEdit(site)}
                  >
                    {m.common_edit()}
                  </Btn>
                  <Btn
                    size="sm"
                    variant="ghost"
                    onClick={() => requestDelete(site)}
                  >
                    {m.common_remove()}
                  </Btn>
                </span>
              ),
            }),
          )}
        />
      )}

      {form && (
        <SettingsFormShell
          error={formError}
          saving={saving}
          onCancel={() => setForm(null)}
          onSave={handleSubmit}
        >
          <div className="pv-step-site__map-section">
            <span className="pv-field-label">{m.setup_site_map_label()}</span>
            <Suspense
              fallback={
                <div
                  className="pv-step-site__map"
                  role="status"
                  aria-label={m.setup_site_map_label()}
                />
              }
            >
              <SiteLocationPicker
                latitudeDeg={parsedCoord(form.latitudeDegText)}
                longitudeDeg={parsedCoord(form.longitudeDegText)}
                onPick={(lat, lon) =>
                  setForm({
                    ...form,
                    latitudeDegText: lat.toFixed(5),
                    longitudeDegText: lon.toFixed(5),
                  })
                }
              />
            </Suspense>
          </div>
          <div className="pv-stack-1">
            <label className="pv-field-label" htmlFor="observing-site-name">
              {m.settings_observing_sites_field_name()}
            </label>
            <input
              id="observing-site-name"
              type="text"
              className="pv-input"
              aria-label={m.settings_observing_sites_field_name()}
              value={form.name}
              onChange={(e) => setForm({ ...form, name: e.target.value })}
            />
          </div>
          <div className="pv-stack-1">
            <label className="pv-field-label" htmlFor="observing-site-lat">
              {m.settings_observing_sites_field_latitude()}
            </label>
            <input
              id="observing-site-lat"
              type="text"
              inputMode="decimal"
              className="pv-input"
              aria-label={m.settings_observing_sites_field_latitude()}
              value={form.latitudeDegText}
              onChange={(e) =>
                setForm({ ...form, latitudeDegText: e.target.value })
              }
            />
          </div>
          <div className="pv-stack-1">
            <label className="pv-field-label" htmlFor="observing-site-lon">
              {m.settings_observing_sites_field_longitude()}
            </label>
            <input
              id="observing-site-lon"
              type="text"
              inputMode="decimal"
              className="pv-input"
              aria-label={m.settings_observing_sites_field_longitude()}
              value={form.longitudeDegText}
              onChange={(e) =>
                setForm({ ...form, longitudeDegText: e.target.value })
              }
            />
          </div>
          <div className="pv-stack-1">
            <label
              className="pv-field-label"
              htmlFor="observing-site-elevation"
            >
              {m.settings_observing_sites_field_elevation()}
            </label>
            <input
              id="observing-site-elevation"
              type="text"
              inputMode="decimal"
              className="pv-input"
              aria-label={m.settings_observing_sites_field_elevation()}
              value={form.elevationMText}
              onChange={(e) =>
                setForm({ ...form, elevationMText: e.target.value })
              }
            />
          </div>
          <div className="pv-stack-1">
            <label className="pv-field-label" htmlFor="observing-site-tz">
              {m.settings_observing_sites_field_timezone()}
            </label>
            <select
              id="observing-site-tz"
              className={selectBase}
              aria-label={m.settings_observing_sites_field_timezone()}
              value={form.timezone}
              onChange={(e) => setForm({ ...form, timezone: e.target.value })}
            >
              {timezones.map((tz) => (
                <option key={tz} value={tz}>
                  {tz}
                </option>
              ))}
            </select>
          </div>
          <div className="pv-stack-1">
            <label className="pv-field-label" htmlFor="observing-site-twilight">
              {m.settings_observing_sites_field_twilight()}
            </label>
            <select
              id="observing-site-twilight"
              className={selectBase}
              aria-label={m.settings_observing_sites_field_twilight()}
              value={form.twilight}
              onChange={(e) =>
                setForm({
                  ...form,
                  twilight:
                    e.target.value === 'nautical' ? 'nautical' : 'astronomical',
                })
              }
            >
              <option value="astronomical">
                {m.settings_observing_sites_twilight_astronomical()}
              </option>
              <option value="nautical">
                {m.settings_observing_sites_twilight_nautical()}
              </option>
            </select>
          </div>
          <div className="pv-stack-1">
            <label className="pv-field-label" htmlFor="observing-site-horizon">
              {m.settings_observing_sites_field_horizon()}
            </label>
            <input
              id="observing-site-horizon"
              type="text"
              inputMode="decimal"
              className="pv-input"
              aria-label={m.settings_observing_sites_field_horizon()}
              value={form.minHorizonAltDegText}
              onChange={(e) =>
                setForm({ ...form, minHorizonAltDegText: e.target.value })
              }
            />
          </div>
        </SettingsFormShell>
      )}

      <Modal
        open={deleteTarget != null}
        onClose={closeDeleteConfirm}
        title={m.settings_observing_sites_delete_confirm_title({
          name: deleteTarget?.name ?? '',
        })}
        size="sm"
        hideClose
        footer={
          <>
            <Btn variant="ghost" onClick={closeDeleteConfirm}>
              {m.common_cancel()}
            </Btn>
            <Btn
              variant="destructive"
              onClick={() => void handleConfirmDelete()}
            >
              {deleteBusy ? m.common_removing() : m.common_remove()}
            </Btn>
          </>
        }
      >
        <p className={modalMessage}>
          {m.settings_observing_sites_delete_confirm_desc()}
        </p>
        {deleteNeedsFallbackChoice && (
          <div className="pv-stack-1">
            <label className="pv-field-label" htmlFor="observing-site-fallback">
              {m.settings_observing_sites_fallback_label()}
            </label>
            <select
              id="observing-site-fallback"
              className={selectBase}
              aria-label={m.settings_observing_sites_fallback_label()}
              value={fallbackSiteId ?? ''}
              onChange={(e) => setFallbackSiteId(e.target.value || null)}
            >
              <option value="">
                {m.settings_observing_sites_fallback_placeholder()}
              </option>
              {deleteRemaining.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
            </select>
          </div>
        )}
        {deleteError && <span className="pv-field-error">{deleteError}</span>}
      </Modal>
    </SettingsSection>
  );
}
