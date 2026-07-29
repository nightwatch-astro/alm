// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import type {
  CalendarData,
  MasterDetail,
  MatchCandidate,
} from '@/bindings/types';
import type { CalibrationMatchDto_Serialize } from '@/bindings/index';

export const mockCalendarData: CalendarData = {
  months: [
    {
      year: 2026,
      month: 5,
      days: [
        { day: 18, sessions: [{ id: 'ses-001', target: 'M31', filter: 'L' }] },
        {
          day: 19,
          sessions: [
            { id: 'ses-003', target: 'M31', filter: 'R' },
            { id: 'ses-004', target: 'M31', filter: 'G' },
          ],
        },
        {
          day: 20,
          sessions: [{ id: 'ses-005', target: 'NGC 7000', filter: 'Ha' }],
        },
      ],
    },
  ],
};

export const mockMasterDetail: MasterDetail = {
  id: 'master-001',
  kind: 'dark',
  fingerprint: {
    camera: 'ASI2600MM',
    sensorMode: 'normal',
    exposureS: 300,
    tempC: -10,
    gain: 100,
    binning: '1x1',
  },
  sourceSessionId: 'cal-ses-001',
  createdAt: '2026-05-15T20:00:00Z',
  ageDays: 9,
  sizeBytes: 52_428_800,
  usedBySessionIds: ['ses-001', 'ses-003'],
  usedByProjectIds: ['proj-001'],
  compatibleSessions: [
    { sessionId: 'ses-001', score: 0.97, softMismatches: [] },
  ],
  usageStats: { sessionCount: 2, projectCount: 1 },
};

export const mockMatchCandidates: MatchCandidate[] = [
  { masterId: 'master-001', kind: 'dark', score: 0.97, softMismatches: [] },
  {
    masterId: 'master-002',
    kind: 'flat',
    score: 0.92,
    filter: 'L',
    softMismatches: ['age > 60 days'],
  },
  { masterId: 'master-003', kind: 'bias', score: 0.99, softMismatches: [] },
];

/**
 * `calibration.match.suggest` / `.suggest.batch` fixtures (spec P9).
 *
 * The second candidate deliberately omits every session-context field to
 * exercise the real-app "—" fallback (no canonical target link / no
 * fingerprint row) alongside the first candidate's fully-resolved context.
 */
export function mockCalibrationMatches(
  sessionId: string,
): CalibrationMatchDto_Serialize[] {
  return [
    {
      sessionId,
      masterId: 'master-001',
      calibrationType: 'dark',
      confidence: 0.97,
      dimensionsMatched: [
        {
          dimension: 'gain',
          observed: { value: 100 },
          reference: { value: 100 },
        },
        {
          dimension: 'offset',
          observed: { value: 10 },
          reference: { value: 10 },
        },
      ],
      dimensionsMismatched: [],
      selectionReason: 'same_night',
      targetName: 'M 31',
      filter: 'Ha',
      acquisitionNight: '2026-05-18',
      frameCount: 42,
    },
    {
      sessionId,
      masterId: 'master-002',
      calibrationType: 'dark',
      confidence: 0.81,
      dimensionsMatched: [
        {
          dimension: 'gain',
          observed: { value: 100 },
          reference: { value: 100 },
        },
      ],
      dimensionsMismatched: [
        { dimension: 'temperature', reason: 'out_of_tolerance', delta: 3.5 },
      ],
      selectionReason: 'compatible_fallback',
      // Unresolved session context — every P9 field stays absent.
    },
  ];
}
