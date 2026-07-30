// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/** All stateful logic for the Equipment pane (cameras/telescopes/trains/filters). */
import { useCallback, useEffect, useState } from 'react';
import { useMountedRef } from '@/hooks/useMountedRef';
import { m } from '@/lib/i18n';
import { errMessage } from '@/lib/errors';
import {
  equipmentCamerasList,
  equipmentCameraCreate,
  equipmentCameraUpdate,
  equipmentCameraDelete,
  equipmentTelescopesList,
  equipmentTelescopeCreate,
  equipmentTelescopeUpdate,
  equipmentTelescopeDelete,
  equipmentTrainsList,
  equipmentTrainCreate,
  equipmentTrainUpdate,
  equipmentTrainDelete,
  equipmentFiltersList,
  equipmentFilterCreate,
  equipmentFilterUpdate,
  equipmentFilterDelete,
  type Camera,
  type SensorType,
  type Telescope,
  type OpticalTrain,
  type Filter,
  type FilterCategory,
} from './settingsIpc';
import {
  parseAliases,
  isDuplicateName,
  hasDuplicateAlias,
  parseFocalLength,
  parseGeometry,
  passbandBandsFrom,
  type DeleteTarget,
  type PassbandChoice,
} from './equipment-helpers';

export function useEquipment() {
  // ── Cameras ────────────────────────────────────────────────────────────────
  const [cameras, setCameras] = useState<Camera[]>([]);
  const [camerasLoading, setCamerasLoading] = useState(true);
  const [camerasError, setCamerasError] = useState<string | null>(null);
  const [cameraForm, setCameraForm] = useState<{
    id: string | null;
    name: string;
    aliasesText: string;
    /** FR-035: '' = unknown (behaves as mono, FR-038). */
    sensorType: '' | SensorType;
    passband: PassbandChoice;
    /** Sensor geometry, held as raw text; '' = not provided (persists null). */
    pixelSizeUmText: string;
    sensorWidthPxText: string;
    sensorHeightPxText: string;
  } | null>(null);
  const [cameraFormError, setCameraFormError] = useState<string | null>(null);
  const [cameraSaving, setCameraSaving] = useState(false);

  // ── Telescopes ───────────────────────────────────────────────────────────────
  const [telescopes, setTelescopes] = useState<Telescope[]>([]);
  const [telescopesLoading, setTelescopesLoading] = useState(true);
  const [telescopesError, setTelescopesError] = useState<string | null>(null);
  const [telescopeForm, setTelescopeForm] = useState<{
    id: string | null;
    name: string;
    aliasesText: string;
    focalLengthMmText: string;
  } | null>(null);
  const [telescopeFormError, setTelescopeFormError] = useState<string | null>(
    null,
  );
  const [telescopeSaving, setTelescopeSaving] = useState(false);

  // ── Optical trains ───────────────────────────────────────────────────────────
  const [trains, setTrains] = useState<OpticalTrain[]>([]);
  const [trainsLoading, setTrainsLoading] = useState(true);
  const [trainsError, setTrainsError] = useState<string | null>(null);
  const [trainForm, setTrainForm] = useState<{
    id: string | null;
    name: string;
    telescopeId: string;
    cameraId: string;
    focalLengthMmText: string;
  } | null>(null);
  const [trainFormError, setTrainFormError] = useState<string | null>(null);
  const [trainSaving, setTrainSaving] = useState(false);

  // ── Filters ──────────────────────────────────────────────────────────────────
  const [filters, setFilters] = useState<Filter[]>([]);
  const [filtersLoading, setFiltersLoading] = useState(true);
  const [filtersError, setFiltersError] = useState<string | null>(null);
  const [filterForm, setFilterForm] = useState<{
    id: string | null;
    name: string;
    category: FilterCategory;
  } | null>(null);
  const [filterFormError, setFilterFormError] = useState<string | null>(null);
  const [filterSaving, setFilterSaving] = useState(false);

  // ── Shared delete confirmation ───────────────────────────────────────────────
  const [deleteTarget, setDeleteTarget] = useState<DeleteTarget | null>(null);
  const [deleteBusy, setDeleteBusy] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);

  const mountedRef = useMountedRef();

  // ── Loaders ──────────────────────────────────────────────────────────────────

  const loadCameras = useCallback(() => {
    setCamerasLoading(true);
    setCamerasError(null);
    equipmentCamerasList()
      .then((data) => {
        if (mountedRef.current) setCameras(data);
      })
      .catch((err: unknown) => {
        if (mountedRef.current)
          setCamerasError(
            m.settings_equipment_load_error({ error: errMessage(err) }),
          );
      })
      .finally(() => {
        if (mountedRef.current) setCamerasLoading(false);
      });
  }, [mountedRef]);

  const loadTelescopes = useCallback(() => {
    setTelescopesLoading(true);
    setTelescopesError(null);
    equipmentTelescopesList()
      .then((data) => {
        if (mountedRef.current) setTelescopes(data);
      })
      .catch((err: unknown) => {
        if (mountedRef.current)
          setTelescopesError(
            m.settings_equipment_load_error({ error: errMessage(err) }),
          );
      })
      .finally(() => {
        if (mountedRef.current) setTelescopesLoading(false);
      });
  }, [mountedRef]);

  const loadTrains = useCallback(() => {
    setTrainsLoading(true);
    setTrainsError(null);
    equipmentTrainsList()
      .then((data) => {
        if (mountedRef.current) setTrains(data);
      })
      .catch((err: unknown) => {
        if (mountedRef.current)
          setTrainsError(
            m.settings_equipment_load_error({ error: errMessage(err) }),
          );
      })
      .finally(() => {
        if (mountedRef.current) setTrainsLoading(false);
      });
  }, [mountedRef]);

  const loadFilters = useCallback(() => {
    setFiltersLoading(true);
    setFiltersError(null);
    equipmentFiltersList()
      .then((data) => {
        if (mountedRef.current) setFilters(data);
      })
      .catch((err: unknown) => {
        if (mountedRef.current)
          setFiltersError(
            m.settings_equipment_load_error({ error: errMessage(err) }),
          );
      })
      .finally(() => {
        if (mountedRef.current) setFiltersLoading(false);
      });
  }, [mountedRef]);

  useEffect(() => {
    loadCameras();
    loadTelescopes();
    loadTrains();
    loadFilters();
  }, [loadCameras, loadTelescopes, loadTrains, loadFilters]);

  // ── Camera handlers ──────────────────────────────────────────────────────────

  // biome-ignore lint/complexity/noExcessiveCognitiveComplexity: camera submit guard chain then create-or-update dispatch
  const handleCameraSubmit = async () => {
    if (!cameraForm) return;
    const name = cameraForm.name.trim();
    if (!name) {
      setCameraFormError(m.settings_equipment_name_required());
      return;
    }
    if (isDuplicateName(cameras, name, cameraForm.id)) {
      setCameraFormError(m.settings_equipment_name_duplicate());
      return;
    }
    const aliases = parseAliases(cameraForm.aliasesText);
    if (hasDuplicateAlias(cameras, aliases, cameraForm.id)) {
      setCameraFormError(m.settings_equipment_alias_duplicate());
      return;
    }
    // Migration 0079: a blank geometry field persists as null (no FOV); a
    // non-positive or non-numeric one is rejected here so it never reaches
    // the backend's FOV computation.
    const pixelSize = parseGeometry(cameraForm.pixelSizeUmText, false);
    const sensorWidth = parseGeometry(cameraForm.sensorWidthPxText, true);
    const sensorHeight = parseGeometry(cameraForm.sensorHeightPxText, true);
    if (
      pixelSize.kind === 'invalid' ||
      sensorWidth.kind === 'invalid' ||
      sensorHeight.kind === 'invalid'
    ) {
      setCameraFormError(m.settings_equipment_geometry_invalid());
      return;
    }

    setCameraSaving(true);
    setCameraFormError(null);
    try {
      // FR-035: '' (unknown) persists as null; passband only matters for OSC.
      const sensorType =
        cameraForm.sensorType === '' ? null : cameraForm.sensorType;
      const passband =
        sensorType === 'osc' ? passbandBandsFrom(cameraForm.passband) : null;
      const geometry = {
        pixelSizeUm: pixelSize.kind === 'valid' ? pixelSize.value : null,
        sensorWidthPx: sensorWidth.kind === 'valid' ? sensorWidth.value : null,
        sensorHeightPx:
          sensorHeight.kind === 'valid' ? sensorHeight.value : null,
      };
      if (cameraForm.id) {
        await equipmentCameraUpdate({
          id: cameraForm.id,
          name,
          aliases,
          sensorType,
          passband,
          ...geometry,
        });
      } else {
        await equipmentCameraCreate({
          name,
          aliases,
          sensorType,
          passband,
          ...geometry,
        });
      }
      setCameraForm(null);
      loadCameras();
    } catch (err: unknown) {
      setCameraFormError(
        m.settings_equipment_save_error({ error: errMessage(err) }),
      );
    } finally {
      setCameraSaving(false);
    }
  };

  const requestDeleteCamera = (camera: Camera) => {
    const inUse = trains.some((t) => t.cameraId === camera.id);
    if (inUse) {
      setCamerasError(m.settings_equipment_delete_in_use());
      return;
    }
    setDeleteError(null);
    setDeleteTarget({ kind: 'camera', id: camera.id, name: camera.name });
  };

  // ── Telescope handlers ───────────────────────────────────────────────────────

  const handleTelescopeSubmit = async () => {
    if (!telescopeForm) return;
    const name = telescopeForm.name.trim();
    if (!name) {
      setTelescopeFormError(m.settings_equipment_name_required());
      return;
    }
    if (isDuplicateName(telescopes, name, telescopeForm.id)) {
      setTelescopeFormError(m.settings_equipment_name_duplicate());
      return;
    }
    const aliases = parseAliases(telescopeForm.aliasesText);
    if (hasDuplicateAlias(telescopes, aliases, telescopeForm.id)) {
      setTelescopeFormError(m.settings_equipment_alias_duplicate());
      return;
    }
    setTelescopeSaving(true);
    setTelescopeFormError(null);
    try {
      const focalLengthMm = parseFocalLength(telescopeForm.focalLengthMmText);
      if (telescopeForm.id) {
        await equipmentTelescopeUpdate({
          id: telescopeForm.id,
          name,
          aliases,
          focalLengthMm,
        });
      } else {
        await equipmentTelescopeCreate({ name, aliases, focalLengthMm });
      }
      setTelescopeForm(null);
      loadTelescopes();
    } catch (err: unknown) {
      setTelescopeFormError(
        m.settings_equipment_save_error({ error: errMessage(err) }),
      );
    } finally {
      setTelescopeSaving(false);
    }
  };

  const requestDeleteTelescope = (telescope: Telescope) => {
    const inUse = trains.some((t) => t.telescopeId === telescope.id);
    if (inUse) {
      setTelescopesError(m.settings_equipment_delete_in_use());
      return;
    }
    setDeleteError(null);
    setDeleteTarget({
      kind: 'telescope',
      id: telescope.id,
      name: telescope.name,
    });
  };

  // ── Optical train handlers ───────────────────────────────────────────────────

  const handleTrainSubmit = async () => {
    if (!trainForm) return;
    const name = trainForm.name.trim();
    if (!name) {
      setTrainFormError(m.settings_equipment_name_required());
      return;
    }
    if (isDuplicateName(trains, name, trainForm.id)) {
      setTrainFormError(m.settings_equipment_name_duplicate());
      return;
    }
    // #835: a train without both parts selected has nothing to resolve
    // camera/telescope-specific FITS metadata against.
    if (!trainForm.cameraId || !trainForm.telescopeId) {
      setTrainFormError(m.settings_equipment_train_parts_required());
      return;
    }
    const focalLengthMm = parseFocalLength(trainForm.focalLengthMmText);
    if (focalLengthMm == null) {
      setTrainFormError(m.settings_equipment_focal_length_required());
      return;
    }
    setTrainSaving(true);
    setTrainFormError(null);
    try {
      const telescopeId = trainForm.telescopeId || null;
      const cameraId = trainForm.cameraId || null;
      if (trainForm.id) {
        await equipmentTrainUpdate({
          id: trainForm.id,
          name,
          telescopeId,
          cameraId,
          focalLengthMm,
        });
      } else {
        await equipmentTrainCreate({
          name,
          telescopeId,
          cameraId,
          focalLengthMm,
        });
      }
      setTrainForm(null);
      loadTrains();
    } catch (err: unknown) {
      setTrainFormError(
        m.settings_equipment_save_error({ error: errMessage(err) }),
      );
    } finally {
      setTrainSaving(false);
    }
  };

  const requestDeleteTrain = (train: OpticalTrain) => {
    setDeleteError(null);
    setDeleteTarget({ kind: 'train', id: train.id, name: train.name });
  };

  // ── Filter handlers ──────────────────────────────────────────────────────────

  const handleFilterSubmit = async () => {
    if (!filterForm) return;
    const name = filterForm.name.trim();
    if (!name) {
      setFilterFormError(m.settings_equipment_name_required());
      return;
    }
    if (isDuplicateName(filters, name, filterForm.id)) {
      setFilterFormError(m.settings_equipment_name_duplicate());
      return;
    }
    setFilterSaving(true);
    setFilterFormError(null);
    try {
      if (filterForm.id) {
        await equipmentFilterUpdate({
          id: filterForm.id,
          name,
          category: filterForm.category,
        });
      } else {
        await equipmentFilterCreate({ name, category: filterForm.category });
      }
      setFilterForm(null);
      loadFilters();
    } catch (err: unknown) {
      setFilterFormError(
        m.settings_equipment_save_error({ error: errMessage(err) }),
      );
    } finally {
      setFilterSaving(false);
    }
  };

  const requestDeleteFilter = (filter: Filter) => {
    setDeleteError(null);
    setDeleteTarget({ kind: 'filter', id: filter.id, name: filter.name });
  };

  // ── Shared delete confirm ────────────────────────────────────────────────────

  const handleConfirmDelete = async () => {
    if (!deleteTarget) return;
    setDeleteBusy(true);
    setDeleteError(null);
    try {
      switch (deleteTarget.kind) {
        case 'camera':
          await equipmentCameraDelete(deleteTarget.id);
          loadCameras();
          break;
        case 'telescope':
          await equipmentTelescopeDelete(deleteTarget.id);
          loadTelescopes();
          break;
        case 'train':
          await equipmentTrainDelete(deleteTarget.id);
          loadTrains();
          break;
        case 'filter':
          await equipmentFilterDelete(deleteTarget.id);
          loadFilters();
          break;
      }
      setDeleteTarget(null);
    } catch (err: unknown) {
      setDeleteError(
        m.settings_equipment_delete_error({ error: errMessage(err) }),
      );
    } finally {
      setDeleteBusy(false);
    }
  };

  // ── Row lookups for the optical trains table ─────────────────────────────────

  const cameraName = (id: string | null) =>
    id
      ? (cameras.find((c) => c.id === id)?.name ?? id)
      : m.settings_equipment_field_none();
  const telescopeName = (id: string | null) =>
    id
      ? (telescopes.find((t) => t.id === id)?.name ?? id)
      : m.settings_equipment_field_none();

  return {
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
  };
}
