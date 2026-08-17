// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// ── pattern.path_preview token bridge (spec 041 P11) ──────────────────────────
//
// Maps a v1 registry `{token}` name (snake_case, as it appears in a per-type
// destination pattern string) to the camelCase `MetadataBundleDto` field name
// carried in `sampleMetadata`. Fallbacks mirror `crates/patterns/src/registry.rs`
// (data-model.md §Errors) so the mock preview matches the real resolver's
// "missing token" substitution.

export const PATH_PREVIEW_TOKEN_FIELDS: Record<string, string> = {
  target: 'target',
  filter: 'filter',
  date: 'date',
  frame_type: 'frameType',
  camera: 'camera',
  exposure: 'exposure',
  gain: 'gain',
  binning: 'binning',
  set_temp: 'setTemp',
};

export const PATH_PREVIEW_TOKEN_FALLBACKS: Record<string, string> = {
  target: 'unclassified',
  filter: 'nofilter',
  date: 'undated',
  frame_type: 'unknown',
  camera: 'unknown-camera',
  exposure: 'unknown-exposure',
  gain: 'unknown-gain',
  binning: '1x1',
  set_temp: 'untempered',
};
