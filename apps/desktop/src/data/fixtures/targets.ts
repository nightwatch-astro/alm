// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// Static mock fixture data — gen-2 shapes retained for reference only.
// These fixtures are not imported anywhere; type annotations removed to avoid
// gen-2 vs gen-3 shape mismatch errors (Target is now TargetListItem_Serialize).

// ─── Design V3 flat fixture shape ───────────────────────────────────────────
//
// `uuid` maps each fixture entry to the stable UUIDv5 used by the spec-013/023
// backend.  When a row is selected in the list, its `uuid` is put into the URL
// search param and passed to `TargetDetail` so the detail loads from the real
// `target.get` backend command.

export interface TargetFixture {
  id: number;
  /** Stable UUIDv5 — matches the backend `targets` table row. */
  uuid: string;
  name: string;
  common: string;
  kind: string;
  sessions: number;
  hours: number;
  projects: number;
  warn?: boolean;
}

export const TARGETS_DATA: TargetFixture[] = [
  {
    id: 1,
    uuid: '550e8400-e29b-41d4-a716-446655440206',
    name: '(unresolved)',
    common: '',
    kind: 'unknown',
    sessions: 3,
    hours: 4.2,
    projects: 0,
    warn: true,
  },
  {
    id: 2,
    uuid: '550e8400-e29b-41d4-a716-446655440203',
    name: 'IC 1396',
    common: "Elephant's Trunk",
    kind: 'deep sky',
    sessions: 4,
    hours: 9.3,
    projects: 1,
  },
  {
    id: 3,
    uuid: '550e8400-e29b-41d4-a716-446655440204',
    name: 'Jupiter',
    common: '',
    kind: 'planetary',
    sessions: 6,
    hours: 2.5,
    projects: 1,
  },
  {
    id: 4,
    uuid: '550e8400-e29b-41d4-a716-446655440202',
    name: 'M31',
    common: 'Andromeda Galaxy',
    kind: 'deep sky',
    sessions: 8,
    hours: 11.8,
    projects: 1,
    warn: true,
  },
  {
    id: 5,
    uuid: '550e8400-e29b-41d4-a716-446655440205',
    name: 'M42',
    common: 'Orion Nebula',
    kind: 'deep sky',
    sessions: 5,
    hours: 3.4,
    projects: 0,
  },
  {
    id: 6,
    uuid: '550e8400-e29b-41d4-a716-446655440201',
    name: 'NGC 7000',
    common: 'North America Nebula',
    kind: 'deep sky',
    sessions: 12,
    hours: 14.2,
    projects: 2,
  },
];

// ---------------------------------------------------------------------------
// Rich list items — shown in the left pane (existing shape retained)
// ---------------------------------------------------------------------------

export const targets = [
  {
    id: '550e8400-e29b-41d4-a716-446655440201',
    name: 'NGC 7000',
    aliases: ['North America Nebula', 'Caldwell 20'],
    catalogIds: { ngc: '7000' },
    kind: 'deep_sky',
    coordinates: { ra: 20.983, dec: 44.517 },
    sessionCount: 12,
    projectCount: 2,
    totalIntegrationHours: 14.2,
    coverage: { Ha: 6.3, OIII: 4.8, SII: 1.8, L: 0 },
    recommendedHours: { Ha: 6.0, OIII: 5.0, SII: 3.0, L: 0 },
  },
  {
    id: '550e8400-e29b-41d4-a716-446655440202',
    name: 'M31',
    aliases: ['Andromeda Galaxy'],
    catalogIds: { messier: '31', ngc: '224' },
    kind: 'deep_sky',
    coordinates: { ra: 0.712, dec: 41.27 },
    sessionCount: 8,
    projectCount: 1,
    totalIntegrationHours: 11.8,
    coverage: { L: 5.2, R: 3.1, G: 1.8, B: 1.7 },
    recommendedHours: { L: 6.0, R: 3.0, G: 3.0, B: 3.0 },
  },
  {
    id: '550e8400-e29b-41d4-a716-446655440203',
    name: 'IC 1396',
    aliases: ["Elephant's Trunk"],
    catalogIds: { ic: '1396' },
    kind: 'deep_sky',
    coordinates: { ra: 21.62, dec: 57.5 },
    sessionCount: 4,
    projectCount: 1,
    totalIntegrationHours: 9.3,
    coverage: { Ha: 5.1, OIII: 4.2 },
    recommendedHours: { Ha: 6.0, OIII: 6.0 },
  },
  {
    id: '550e8400-e29b-41d4-a716-446655440204',
    name: 'Jupiter',
    aliases: [],
    catalogIds: {},
    kind: 'planetary',
    coordinates: {},
    sessionCount: 6,
    projectCount: 1,
    totalIntegrationHours: 2.5,
    coverage: {},
    recommendedHours: {},
  },
  {
    id: '550e8400-e29b-41d4-a716-446655440205',
    name: 'M42',
    aliases: ['Orion Nebula'],
    catalogIds: { messier: '42', ngc: '1976' },
    kind: 'deep_sky',
    coordinates: { ra: 5.588, dec: -5.39 },
    sessionCount: 5,
    projectCount: 0,
    totalIntegrationHours: 3.4,
    coverage: { Ha: 3.4 },
    recommendedHours: { Ha: 4.0 },
  },
  {
    id: '550e8400-e29b-41d4-a716-446655440206',
    name: '(unresolved)',
    aliases: [],
    catalogIds: {},
    kind: 'deep_sky',
    coordinates: {},
    sessionCount: 3,
    projectCount: 0,
    totalIntegrationHours: 4.2,
    coverage: { Ha: 4.2 },
    recommendedHours: { Ha: 4.0 },
  },
];

// ---------------------------------------------------------------------------
// Detail — the full NGC 7000 detail matching the wireframe
// ---------------------------------------------------------------------------

export const targetDetail = {
  ...targets[0],
  sessions: [
    {
      id: 's-001',
      sessionKey: {
        target: 'NGC 7000',
        filter: 'Ha',
        binning: '1x1',
        gain: '100',
        night: '2024-11-30',
      },
      confidence: 'confirmed',
      opticalTrainId: 'ot-2600mm',
      frameCount: 54,
      totalIntegrationSeconds: 16200,
      totalSizeBytes: 0,
      metadata: {},
      targetIds: ['550e8400-e29b-41d4-a716-446655440201'],
      projectIds: ['proj-hoo'],
      warnings: [],
    },
    {
      id: 's-002',
      sessionKey: {
        target: 'NGC 7000',
        filter: 'OIII',
        binning: '1x1',
        gain: '100',
        night: '2024-11-30',
      },
      confidence: 'confirmed',
      opticalTrainId: 'ot-2600mm',
      frameCount: 38,
      totalIntegrationSeconds: 11520,
      totalSizeBytes: 0,
      metadata: {},
      targetIds: ['550e8400-e29b-41d4-a716-446655440201'],
      projectIds: ['proj-hoo'],
      warnings: [],
    },
    {
      id: 's-003',
      sessionKey: {
        target: 'NGC 7000',
        filter: 'SII',
        binning: '1x1',
        gain: '100',
        night: '2024-12-01',
      },
      confidence: 'high',
      opticalTrainId: 'ot-2600mm',
      frameCount: 22,
      totalIntegrationSeconds: 6480,
      totalSizeBytes: 0,
      metadata: {},
      targetIds: ['550e8400-e29b-41d4-a716-446655440201'],
      projectIds: [],
      warnings: [],
    },
    {
      id: 's-004',
      sessionKey: {
        target: 'NGC 7000',
        filter: 'Ha',
        binning: '1x1',
        gain: '100',
        night: '2024-12-15',
      },
      confidence: 'confirmed',
      opticalTrainId: 'ot-2600mm',
      frameCount: 30,
      totalIntegrationSeconds: 9000,
      totalSizeBytes: 0,
      metadata: {},
      targetIds: ['550e8400-e29b-41d4-a716-446655440201'],
      projectIds: ['proj-sho'],
      warnings: [],
    },
  ],
  projects: [
    { id: 'proj-hoo', name: 'NGC 7000 · HOO', state: 'processing' },
    { id: 'proj-sho', name: 'NGC 7000 · SHO mosaic', state: 'ready' },
  ],
};
