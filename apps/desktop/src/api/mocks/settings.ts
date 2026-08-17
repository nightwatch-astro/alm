// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import type { AppPreferences, SettingsData } from '@/bindings/types';

export const mockSettingsData: SettingsData = {
  scope: 'general',
  values: {
    naming_pattern:
      '{target}/{date}/{filter}/{target}_{filter}_{sequence}.fits',
    default_source_view_strategy: 'symlink',
    calibration_age_warning_days: 90,
  },
};

export const mockPreferences: AppPreferences = {
  sidebarCollapsed: false,
  density: 'comfortable',
  projectViewModes: {},
  defaultProjectView: 'combined',
  sessionsGroupBy: 'none',
  sessionsView: 'list',
  setupCompleted: false,
  detailDock: {},
};
