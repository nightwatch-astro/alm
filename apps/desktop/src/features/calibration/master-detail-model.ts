// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * master-detail-model.ts — pure builders for MasterDetail (spec 007, spec 043).
 *
 * Extracted from MasterDetail.tsx (refactor sweep kyo7.104) so the title
 * string and fingerprint property list can be derived and reasoned about
 * independently of rendering. Uses formatBytes from @/lib/format (kyo7.89
 * canon swap — note: rounding differs from the former local fmtBytes:
 * formatBytes always uses 1 decimal for KB/MB/GB; local fmtBytes used 0 for
 * MB and KB. Coordinate with kyo7.89).
 */

import type { CalibrationMaster_Serialize as CalibrationMaster } from '@/bindings/index';
import type { PropertyDef } from '@/components';
import { m } from '@/lib/i18n';
import {
  formatExposureSeconds,
  formatTempC,
  formatGain,
  formatBinning,
  formatBytes,
} from '@/lib/format';
import { masterFieldApplicability } from './master-applicability';

/**
 * Derive the human-readable title for a master frame (e.g. "Dark 30s",
 * "Flat Ha", "Bias").
 */
export function buildMasterTitle(master: CalibrationMaster): string {
  const kindStr = master.kind.toString().toLowerCase().replace('_', ' ');
  const fp = master.fingerprint;
  const kindCap = kindStr.charAt(0).toUpperCase() + kindStr.slice(1);
  const masterDisc =
    kindStr === 'dark'
      ? fp.exposureS != null
        ? `${fp.exposureS}s`
        : ''
      : kindStr === 'flat'
        ? (fp.filter ?? '')
        : '';
  return masterDisc
    ? m.calibration_master_title_disc({ kind: kindCap, disc: masterDisc })
    : m.calibration_master_title({ kind: kindCap });
}

/**
 * Build the flat fingerprint PropertyDef rows for a master, split across two
 * columns like SessionDetail's factProps. Rows are always present (never
 * omitted for a missing value — spec-030 Q16 / FR-135).
 */
export function buildFingerprintProps(
  master: CalibrationMaster,
): PropertyDef[] {
  const kindStr = master.kind.toString().toLowerCase().replace('_', ' ');
  const fp = master.fingerprint;
  return [
    { key: 'kind', label: m.calibration_fp_kind(), value: kindStr },
    {
      key: 'camera',
      label: m.settings_calmatch_camera(),
      value: fp.camera ?? null,
    },
    {
      key: 'gain',
      label: m.settings_calmatch_gain(),
      value: fp.gain != null ? formatGain(fp.gain) : null,
    },
    {
      key: 'exposure',
      label: m.calibration_fp_exposure(),
      // #811: shared formatter (consistent rounding/spacing with Calibration's
      // MastersTable and Inbox), instead of the local `${v}s` ad hoc version.
      value: fp.exposureS != null ? formatExposureSeconds(fp.exposureS) : null,
      applicability: masterFieldApplicability(master.kind, 'exposure'),
    },
    {
      key: 'temp',
      label: m.calibration_fp_temperature(),
      value: fp.tempC != null ? formatTempC(fp.tempC) : null,
      applicability: masterFieldApplicability(master.kind, 'setTemp'),
    },
    {
      key: 'filter',
      label: m.common_filter(),
      value: fp.filter ?? null,
      applicability: masterFieldApplicability(master.kind, 'filter'),
    },
    {
      key: 'sensorMode',
      label: m.calibration_fp_sensor_mode(),
      value: fp.sensorMode ?? null,
    },
    {
      key: 'binning',
      label: m.settings_calmatch_binning(),
      // #811: was raw `fp.binning`, unlike MastersTable's binningCell which
      // already normalises the "x" separator to "×" — now consistent.
      value: fp.binning != null ? formatBinning(fp.binning) : null,
    },
    {
      key: 'size',
      label: m.settings_advanced_db_size(),
      value: master.sizeBytes != null ? formatBytes(master.sizeBytes) : null,
    },
    {
      // #619: the row only shows ageDays conditionally, as a warning pill
      // when the master is aging past the threshold — the actual age is
      // otherwise invisible. The detail panel shows it unconditionally.
      key: 'age',
      label: m.calibration_fp_age(),
      value: m.calibration_fp_age_value({ days: master.ageDays }),
    },
  ];
}
