// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Equipment pane — spec 030 T018 wiring.
 *
 * Manages cameras, telescopes, optical trains, and filters against the real
 * `equipment.{cameras,telescopes,trains,filters}.*` commands (previously a
 * `useState` stub seeded from `@/data/fixtures/settings`).
 *
 * Notes on the real DTOs vs. the retired fixtures:
 *  - `Camera` is `{ id, name, aliases, autoDetected }` — there is no sensor,
 *    pixel size, resolution, cooled, or color field in the backend model.
 *  - `Telescope` is `{ id, name, aliases, focalLengthMm, autoDetected }` —
 *    there is no aperture or f-ratio field.
 *  - `OpticalTrain` is `{ id, name, telescopeId, cameraId, focalLengthMm }` —
 *    `telescopeId`/`cameraId` are real foreign ids (nullable), not free-text
 *    camera/telescope name strings; there is no derived pixel-scale field.
 *  - Deleting a camera or telescope that is referenced by an optical train
 *    fails at the database's foreign-key constraint (mapped to a generic
 *    `internal.database` `ContractError`, not a dedicated "in use" error
 *    code). This pane pre-checks the loaded trains client-side so the user
 *    gets an actionable message instead of a raw database error; the backend
 *    constraint remains the source of truth for correctness.
 */
import { Btn, NumberField, Table } from '@/ui';
import type { TableRow } from '@/ui';
import { Modal } from '@/components';
import { m } from '@/lib/i18n';
import type { SensorType, FilterCategory } from './settingsIpc';
import { SettingsSection, SettingsFormShell } from './SettingsKit';
import {
  autoDetectedBadge,
  filterCategoryLabel,
  formatAliases,
  fovSummary,
  FILTER_CATEGORIES,
  passbandChoiceFrom,
  passbandLabel,
  sensorSummary,
  type PassbandChoice,
} from './equipment-helpers';
import { useEquipment } from './useEquipment';
import { selectBase } from '@/styles/select.css';

interface EquipmentProps {
  save: (scope: string, values: Record<string, unknown>) => void;
}

// The add/edit form shell (field grid + error line + cancel/save actions) now
// lives in `SettingsKit.tsx` as `SettingsFormShell`, shared with other panes'
// CRUD lists (e.g. observing-site management, spec 044 US3) — no per-pane
// clone (shared-component mandate).

export function Equipment({ save: _save }: EquipmentProps) {
  const {
    cameras,
    camerasLoading,
    camerasError,
    cameraForm,
    setCameraForm,
    cameraFormError,
    cameraSaving,
    handleCameraSubmit,
    requestDeleteCamera,

    telescopes,
    telescopesLoading,
    telescopesError,
    telescopeForm,
    setTelescopeForm,
    telescopeFormError,
    telescopeSaving,
    handleTelescopeSubmit,
    requestDeleteTelescope,

    trains,
    trainsLoading,
    trainsError,
    trainForm,
    setTrainForm,
    trainFormError,
    trainSaving,
    handleTrainSubmit,
    requestDeleteTrain,

    filters,
    filtersLoading,
    filtersError,
    filterForm,
    setFilterForm,
    filterFormError,
    filterSaving,
    handleFilterSubmit,
    requestDeleteFilter,

    deleteTarget,
    setDeleteTarget,
    deleteBusy,
    deleteError,
    setDeleteError,
    handleConfirmDelete,

    cameraName,
    telescopeName,
  } = useEquipment();

  // Dismissal is refused while the delete is in flight, so a half-applied
  // removal can't be hidden behind a closed dialog (#1190).
  const closeDeleteConfirm = () => {
    if (deleteBusy) return;
    setDeleteTarget(null);
    setDeleteError(null);
  };

  return (
    <>
      {/* Optical Trains */}
      <SettingsSection
        title={m.settings_equipment_trains_title()}
        action={
          <Btn
            size="sm"
            onClick={() =>
              setTrainForm({
                id: null,
                name: '',
                telescopeId: '',
                cameraId: '',
                focalLengthMmText: '',
              })
            }
          >
            {m.settings_equipment_trains_add()}
          </Btn>
        }
      >
        {trainsError && (
          <p className="pv-equipment__load-error">{trainsError}</p>
        )}
        {trainsLoading && (
          <p className="pv-equipment__empty">{m.common_loading()}</p>
        )}

        {!trainsLoading && (
          <Table
            columns={[
              { key: 'name', label: m.settings_equipment_col_name() },
              { key: 'camera', label: m.settings_equipment_col_camera() },
              { key: 'telescope', label: m.settings_equipment_col_telescope() },
              {
                key: 'focalLength',
                label: m.settings_equipment_col_focal_length(),
              },
              { key: 'fov', label: m.settings_equipment_col_fov() },
              { key: 'actions', label: '', style: { width: 140 } },
            ]}
            rows={trains.map(
              (t): TableRow => ({
                name: t.name,
                camera: cameraName(t.cameraId),
                telescope: telescopeName(t.telescopeId),
                focalLength: (
                  <code className="pv-mono">
                    {m.settings_equipment_focal_length_value({
                      mm: t.focalLengthMm,
                    })}
                  </code>
                ),
                // Backend-derived; absent when the linked camera has no
                // sensor geometry. Rendered as "Not known", never as 0°.
                fov: fovSummary(t.fovDiagonalDeg),
                actions: (
                  <span className="pv-equipment__row-actions">
                    <Btn
                      size="sm"
                      variant="ghost"
                      onClick={() =>
                        setTrainForm({
                          id: t.id,
                          name: t.name,
                          telescopeId: t.telescopeId ?? '',
                          cameraId: t.cameraId ?? '',
                          focalLengthMmText: String(t.focalLengthMm),
                        })
                      }
                    >
                      {m.common_edit()}
                    </Btn>
                    <Btn
                      size="sm"
                      variant="ghost"
                      onClick={() => requestDeleteTrain(t)}
                    >
                      {m.common_remove()}
                    </Btn>
                  </span>
                ),
              }),
            )}
          />
        )}
        {!trainsLoading && trains.length === 0 && !trainsError && (
          <p className="pv-equipment__empty">
            {m.settings_equipment_trains_empty()}
          </p>
        )}

        {trainForm && (
          <SettingsFormShell
            error={trainFormError}
            saving={trainSaving}
            onCancel={() => setTrainForm(null)}
            onSave={handleTrainSubmit}
          >
            <div className="pv-stack-1">
              <label className="pv-field-label" htmlFor="equipment-train-name">
                {m.settings_equipment_col_name()}
              </label>
              <input
                id="equipment-train-name"
                type="text"
                className="pv-input"
                aria-label={m.settings_equipment_col_name()}
                value={trainForm.name}
                onChange={(e) =>
                  setTrainForm({ ...trainForm, name: e.target.value })
                }
              />
            </div>
            <div className="pv-stack-1">
              <label
                className="pv-field-label"
                htmlFor="equipment-train-camera"
              >
                {m.settings_equipment_field_camera()}
              </label>
              <select
                id="equipment-train-camera"
                className={selectBase}
                value={trainForm.cameraId}
                onChange={(e) =>
                  setTrainForm({ ...trainForm, cameraId: e.target.value })
                }
              >
                <option value="">{m.settings_equipment_field_none()}</option>
                {cameras.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </select>
            </div>
            <div className="pv-stack-1">
              <label
                className="pv-field-label"
                htmlFor="equipment-train-telescope"
              >
                {m.settings_equipment_field_telescope()}
              </label>
              <select
                id="equipment-train-telescope"
                className={selectBase}
                value={trainForm.telescopeId}
                onChange={(e) =>
                  setTrainForm({ ...trainForm, telescopeId: e.target.value })
                }
              >
                <option value="">{m.settings_equipment_field_none()}</option>
                {telescopes.map((t) => (
                  <option key={t.id} value={t.id}>
                    {t.name}
                  </option>
                ))}
              </select>
            </div>
            <div className="pv-stack-1">
              <label
                className="pv-field-label"
                htmlFor="equipment-train-focal-length"
              >
                {m.settings_equipment_field_focal_length()}
              </label>
              <input
                id="equipment-train-focal-length"
                type="text"
                inputMode="numeric"
                className="pv-input"
                aria-label={m.settings_equipment_field_focal_length()}
                value={trainForm.focalLengthMmText}
                onChange={(e) =>
                  setTrainForm({
                    ...trainForm,
                    focalLengthMmText: e.target.value,
                  })
                }
              />
            </div>
          </SettingsFormShell>
        )}
      </SettingsSection>

      {/* Cameras */}
      <SettingsSection
        title={m.settings_equipment_cameras_title()}
        action={
          <Btn
            size="sm"
            onClick={() =>
              setCameraForm({
                id: null,
                name: '',
                aliasesText: '',
                sensorType: '',
                passband: 'rgb',
                pixelSizeUmText: '',
                sensorWidthPxText: '',
                sensorHeightPxText: '',
              })
            }
          >
            {m.settings_equipment_cameras_add()}
          </Btn>
        }
      >
        {camerasError && (
          <p className="pv-equipment__load-error">{camerasError}</p>
        )}
        {camerasLoading && (
          <p className="pv-equipment__empty">{m.common_loading()}</p>
        )}

        {!camerasLoading && (
          <Table
            columns={[
              { key: 'name', label: m.settings_equipment_col_name() },
              { key: 'aliases', label: m.common_aliases() },
              { key: 'sensor', label: m.settings_equipment_col_sensor() },
              { key: 'source', label: m.settings_equipment_col_source() },
              { key: 'actions', label: '', style: { width: 140 } },
            ]}
            rows={cameras.map(
              (c): TableRow => ({
                name: c.name,
                aliases: formatAliases(c.aliases),
                sensor: sensorSummary(c),
                source: autoDetectedBadge(c.autoDetected),
                actions: (
                  <span className="pv-equipment__row-actions">
                    <Btn
                      size="sm"
                      variant="ghost"
                      onClick={() =>
                        setCameraForm({
                          id: c.id,
                          name: c.name,
                          aliasesText: c.aliases.join(', '),
                          sensorType: c.sensorType ?? '',
                          passband: passbandChoiceFrom(c.passband),
                          // Absent geometry opens as blank, not as '0'.
                          pixelSizeUmText: c.pixelSizeUm?.toString() ?? '',
                          sensorWidthPxText: c.sensorWidthPx?.toString() ?? '',
                          sensorHeightPxText:
                            c.sensorHeightPx?.toString() ?? '',
                        })
                      }
                    >
                      {m.common_edit()}
                    </Btn>
                    {/* Disabled until trains load: the in-use guard reads the
                        trains list, so a click before it arrives would bypass
                        the check (TOCTOU) and hit the raw FK error instead. */}
                    <Btn
                      size="sm"
                      variant="ghost"
                      disabled={trainsLoading}
                      onClick={() => requestDeleteCamera(c)}
                    >
                      {m.common_remove()}
                    </Btn>
                  </span>
                ),
              }),
            )}
          />
        )}
        {!camerasLoading && cameras.length === 0 && !camerasError && (
          <p className="pv-equipment__empty">
            {m.settings_equipment_cameras_empty()}
          </p>
        )}

        {cameraForm && (
          <SettingsFormShell
            error={cameraFormError}
            saving={cameraSaving}
            onCancel={() => setCameraForm(null)}
            onSave={handleCameraSubmit}
          >
            <div className="pv-stack-1">
              <label className="pv-field-label" htmlFor="equipment-camera-name">
                {m.settings_equipment_col_name()}
              </label>
              <input
                id="equipment-camera-name"
                type="text"
                className="pv-input"
                aria-label={m.settings_equipment_col_name()}
                value={cameraForm.name}
                onChange={(e) =>
                  setCameraForm({ ...cameraForm, name: e.target.value })
                }
              />
            </div>
            <div className="pv-stack-1">
              <label
                className="pv-field-label"
                htmlFor="equipment-camera-aliases"
              >
                {m.common_aliases()}
                <span className="pv-field-hint">
                  {' '}
                  ({m.settings_equipment_field_aliases_hint()})
                </span>
              </label>
              <input
                id="equipment-camera-aliases"
                type="text"
                className="pv-input"
                aria-label={m.common_aliases()}
                value={cameraForm.aliasesText}
                onChange={(e) =>
                  setCameraForm({ ...cameraForm, aliasesText: e.target.value })
                }
              />
            </div>
            {/* FR-035: sensor-type dimension. Unknown stays selectable and
                behaves as mono downstream (FR-038) — additive, never a
                required migration step for existing cameras. */}
            <div className="pv-stack-1">
              <label
                className="pv-field-label"
                htmlFor="equipment-camera-sensor"
              >
                {m.settings_equipment_field_sensor()}
              </label>
              <select
                id="equipment-camera-sensor"
                className="pv-input"
                aria-label={m.settings_equipment_field_sensor()}
                value={cameraForm.sensorType}
                onChange={(e) =>
                  setCameraForm({
                    ...cameraForm,
                    sensorType: e.target.value as '' | SensorType,
                  })
                }
              >
                <option value="">
                  {m.settings_equipment_sensor_unknown_option()}
                </option>
                <option value="mono">
                  {m.settings_equipment_sensor_mono()}
                </option>
                <option value="osc">{m.settings_equipment_sensor_osc()}</option>
              </select>
            </div>
            {cameraForm.sensorType === 'osc' && (
              <div className="pv-stack-1">
                <label
                  className="pv-field-label"
                  htmlFor="equipment-camera-passband"
                >
                  {m.settings_equipment_field_passband()}
                </label>
                <select
                  id="equipment-camera-passband"
                  className="pv-input"
                  aria-label={m.settings_equipment_field_passband()}
                  value={cameraForm.passband}
                  onChange={(e) =>
                    setCameraForm({
                      ...cameraForm,
                      passband: e.target.value as PassbandChoice,
                    })
                  }
                >
                  <option value="rgb">{passbandLabel('rgb')}</option>
                  <option value="ha_oiii">{passbandLabel('ha_oiii')}</option>
                  <option value="ha_sii_oiii">
                    {passbandLabel('ha_sii_oiii')}
                  </option>
                </select>
              </div>
            )}
            {/* Migration 0079: sensor geometry. All three are optional — a
                camera without them simply reports no field of view. */}
            <NumberField
              id="equipment-camera-pixel-size"
              label={m.settings_equipment_field_pixel_size()}
              hint={m.settings_equipment_geometry_hint()}
              min={0}
              step="any"
              value={cameraForm.pixelSizeUmText}
              onChange={(value) =>
                setCameraForm({ ...cameraForm, pixelSizeUmText: value })
              }
            />
            <NumberField
              id="equipment-camera-sensor-width"
              label={m.settings_equipment_field_sensor_width()}
              min={0}
              step={1}
              value={cameraForm.sensorWidthPxText}
              onChange={(value) =>
                setCameraForm({ ...cameraForm, sensorWidthPxText: value })
              }
            />
            <NumberField
              id="equipment-camera-sensor-height"
              label={m.settings_equipment_field_sensor_height()}
              min={0}
              step={1}
              value={cameraForm.sensorHeightPxText}
              onChange={(value) =>
                setCameraForm({ ...cameraForm, sensorHeightPxText: value })
              }
            />
          </SettingsFormShell>
        )}
      </SettingsSection>

      {/* Telescopes */}
      <SettingsSection
        title={m.settings_equipment_telescopes_title()}
        action={
          <Btn
            size="sm"
            onClick={() =>
              setTelescopeForm({
                id: null,
                name: '',
                aliasesText: '',
                focalLengthMmText: '',
              })
            }
          >
            {m.settings_equipment_telescopes_add()}
          </Btn>
        }
      >
        {telescopesError && (
          <p className="pv-equipment__load-error">{telescopesError}</p>
        )}
        {telescopesLoading && (
          <p className="pv-equipment__empty">{m.common_loading()}</p>
        )}

        {!telescopesLoading && (
          <Table
            columns={[
              { key: 'name', label: m.settings_equipment_col_name() },
              { key: 'aliases', label: m.common_aliases() },
              {
                key: 'focalLength',
                label: m.settings_equipment_col_focal_length(),
              },
              { key: 'source', label: m.settings_equipment_col_source() },
              { key: 'actions', label: '', style: { width: 140 } },
            ]}
            rows={telescopes.map(
              (t): TableRow => ({
                name: t.name,
                aliases: formatAliases(t.aliases),
                focalLength: (
                  <code className="pv-mono">
                    {t.focalLengthMm != null
                      ? m.settings_equipment_focal_length_value({
                          mm: t.focalLengthMm,
                        })
                      : '—'}
                  </code>
                ),
                source: autoDetectedBadge(t.autoDetected),
                actions: (
                  <span className="pv-equipment__row-actions">
                    <Btn
                      size="sm"
                      variant="ghost"
                      onClick={() =>
                        setTelescopeForm({
                          id: t.id,
                          name: t.name,
                          aliasesText: t.aliases.join(', '),
                          focalLengthMmText:
                            t.focalLengthMm != null
                              ? String(t.focalLengthMm)
                              : '',
                        })
                      }
                    >
                      {m.common_edit()}
                    </Btn>
                    {/* Disabled until trains load — same TOCTOU guard as cameras. */}
                    <Btn
                      size="sm"
                      variant="ghost"
                      disabled={trainsLoading}
                      onClick={() => requestDeleteTelescope(t)}
                    >
                      {m.common_remove()}
                    </Btn>
                  </span>
                ),
              }),
            )}
          />
        )}
        {!telescopesLoading && telescopes.length === 0 && !telescopesError && (
          <p className="pv-equipment__empty">
            {m.settings_equipment_telescopes_empty()}
          </p>
        )}

        {telescopeForm && (
          <SettingsFormShell
            error={telescopeFormError}
            saving={telescopeSaving}
            onCancel={() => setTelescopeForm(null)}
            onSave={handleTelescopeSubmit}
          >
            <div className="pv-stack-1">
              <label
                className="pv-field-label"
                htmlFor="equipment-telescope-name"
              >
                {m.settings_equipment_col_name()}
              </label>
              <input
                id="equipment-telescope-name"
                type="text"
                className="pv-input"
                aria-label={m.settings_equipment_col_name()}
                value={telescopeForm.name}
                onChange={(e) =>
                  setTelescopeForm({ ...telescopeForm, name: e.target.value })
                }
              />
            </div>
            <div className="pv-stack-1">
              <label
                className="pv-field-label"
                htmlFor="equipment-telescope-aliases"
              >
                {m.common_aliases()}
                <span className="pv-field-hint">
                  {' '}
                  ({m.settings_equipment_field_aliases_hint()})
                </span>
              </label>
              <input
                id="equipment-telescope-aliases"
                type="text"
                className="pv-input"
                aria-label={m.common_aliases()}
                value={telescopeForm.aliasesText}
                onChange={(e) =>
                  setTelescopeForm({
                    ...telescopeForm,
                    aliasesText: e.target.value,
                  })
                }
              />
            </div>
            <div className="pv-stack-1">
              <label
                className="pv-field-label"
                htmlFor="equipment-telescope-focal-length"
              >
                {m.settings_equipment_field_focal_length()}
              </label>
              <input
                id="equipment-telescope-focal-length"
                type="text"
                inputMode="numeric"
                className="pv-input"
                aria-label={m.settings_equipment_field_focal_length()}
                value={telescopeForm.focalLengthMmText}
                onChange={(e) =>
                  setTelescopeForm({
                    ...telescopeForm,
                    focalLengthMmText: e.target.value,
                  })
                }
              />
            </div>
          </SettingsFormShell>
        )}
      </SettingsSection>

      {/* Filters */}
      <SettingsSection
        title={m.common_filters()}
        action={
          <Btn
            size="sm"
            onClick={() =>
              setFilterForm({ id: null, name: '', category: 'narrowband' })
            }
          >
            {m.settings_equipment_filters_add()}
          </Btn>
        }
      >
        {filtersError && (
          <p className="pv-equipment__load-error">{filtersError}</p>
        )}
        {filtersLoading && (
          <p className="pv-equipment__empty">{m.common_loading()}</p>
        )}

        {!filtersLoading && (
          <Table
            columns={[
              { key: 'name', label: m.settings_equipment_col_name() },
              { key: 'category', label: m.settings_equipment_col_category() },
              { key: 'source', label: m.settings_equipment_col_source() },
              { key: 'actions', label: '', style: { width: 140 } },
            ]}
            rows={filters.map(
              (f): TableRow => ({
                name: f.name,
                category: filterCategoryLabel(f.category),
                source: autoDetectedBadge(f.autoDetected),
                actions: (
                  <span className="pv-equipment__row-actions">
                    <Btn
                      size="sm"
                      variant="ghost"
                      onClick={() =>
                        setFilterForm({
                          id: f.id,
                          name: f.name,
                          category: f.category,
                        })
                      }
                    >
                      {m.common_edit()}
                    </Btn>
                    <Btn
                      size="sm"
                      variant="ghost"
                      onClick={() => requestDeleteFilter(f)}
                    >
                      {m.common_remove()}
                    </Btn>
                  </span>
                ),
              }),
            )}
          />
        )}
        {!filtersLoading && filters.length === 0 && !filtersError && (
          <p className="pv-equipment__empty">
            {m.settings_equipment_filters_empty()}
          </p>
        )}

        {filterForm && (
          <SettingsFormShell
            error={filterFormError}
            saving={filterSaving}
            onCancel={() => setFilterForm(null)}
            onSave={handleFilterSubmit}
          >
            <div className="pv-stack-1">
              <label className="pv-field-label" htmlFor="equipment-filter-name">
                {m.settings_equipment_col_name()}
              </label>
              <input
                id="equipment-filter-name"
                type="text"
                className="pv-input"
                aria-label={m.settings_equipment_col_name()}
                value={filterForm.name}
                onChange={(e) =>
                  setFilterForm({ ...filterForm, name: e.target.value })
                }
              />
            </div>
            <div className="pv-stack-1">
              <label
                className="pv-field-label"
                htmlFor="equipment-filter-category"
              >
                {m.settings_equipment_field_category()}
              </label>
              <select
                id="equipment-filter-category"
                className={selectBase}
                value={filterForm.category}
                onChange={(e) =>
                  setFilterForm({
                    ...filterForm,
                    category: e.target.value as FilterCategory,
                  })
                }
              >
                {FILTER_CATEGORIES.map((cat) => (
                  <option key={cat} value={cat}>
                    {filterCategoryLabel(cat)}
                  </option>
                ))}
              </select>
            </div>
          </SettingsFormShell>
        )}
      </SettingsSection>

      <Modal
        open={deleteTarget != null}
        onClose={closeDeleteConfirm}
        title={m.settings_equipment_delete_confirm_title({
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
        <p className="pv-modal__message">
          {m.settings_equipment_delete_confirm_desc()}
        </p>
        {deleteError && <span className="pv-field-error">{deleteError}</span>}
      </Modal>
    </>
  );
}
