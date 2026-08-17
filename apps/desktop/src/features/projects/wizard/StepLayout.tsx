// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { useMemo } from 'react';
import { m } from '@/lib/i18n';
import type { StepNameData } from './StepName';
import type { SourceViewStrategy } from './StepViews';

export interface StepLayoutData {
  namingPattern: string;
}

export interface StepLayoutProps {
  data: StepLayoutData;
  nameData: StepNameData;
  strategy: SourceViewStrategy;
  onChange: (data: StepLayoutData) => void;
}

// Default matches the v1 token registry — no {sequence} which is not a registered token.
const DEFAULT_PATTERN = '{target}_{filter}_{exposure}s';

const AVAILABLE_TOKENS = [
  { token: '{target}', example: 'NGC7000' },
  { token: '{filter}', example: 'Ha' },
  { token: '{exposure}', example: '600' },
  { token: '{date}', example: '2026-04-15' },
  { token: '{gain}', example: '100' },
  { token: '{binning}', example: '1x1' },
];

export function StepLayout({
  data,
  nameData,
  strategy,
  onChange,
}: StepLayoutProps) {
  const pattern = data.namingPattern || DEFAULT_PATTERN;

  // Generate example paths based on current wizard state
  const examplePaths = useMemo(() => {
    const projectSlug = nameData.name
      ? nameData.name.replace(/[^a-zA-Z0-9]+/g, '_').replace(/^_|_$/g, '')
      : 'Project_Name';

    const base = `/media/Astrophoto/Projects/${projectSlug}`;
    const ext = nameData.workflowProfile === 'planetary' ? '.ser' : '.fit';

    const exampleFilename = pattern
      .replace('{target}', 'NGC7000')
      .replace('{filter}', 'Ha')
      .replace('{exposure}', '600')
      .replace('{date}', '2026-04-15')
      .replace('{gain}', '100')
      .replace('{binning}', '1x1')
      .replace('{set_temp}', '-10C')
      .replace('{frame_type}', 'light');

    return {
      root: base,
      structure: [
        `${base}/`,
        `${base}/lights/Ha/`,
        `${base}/lights/Ha/${exampleFilename}${ext}`,
        `${base}/lights/OIII/`,
        `${base}/calibration/darks/`,
        `${base}/calibration/flats/Ha/`,
        `${base}/source-view/ (${strategy})`,
        `${base}/processing/`,
        `${base}/outputs/`,
      ],
    };
  }, [nameData, pattern, strategy]);

  return (
    <div className="pv-wizard-layout">
      {/* Naming pattern */}
      <div className="pv-wizard-layout__section">
        {}
        <label htmlFor="naming-pattern" className="pv-wizard-layout__label">
          {m.projects_wizard_naming_pattern_label()}
        </label>
        <input
          id="naming-pattern"
          type="text"
          aria-label={m.projects_wizard_naming_pattern_label()}
          value={pattern}
          onChange={(e) => onChange({ namingPattern: e.target.value })}
          placeholder={DEFAULT_PATTERN}
          className="pv-wizard-layout__input"
        />
        <div className="pv-wizard-layout__token-row">
          {AVAILABLE_TOKENS.map((t) => (
            <span
              key={t.token}
              title={m.projects_wizard_token_example_title({
                example: t.example,
              })}
              className="pv-wizard-layout__token-chip"
            >
              {t.token}
            </span>
          ))}
        </div>
      </div>

      {/* Directory structure preview */}
      <div className="pv-wizard-layout__section">
        <h3 className="pv-wizard-layout__label">
          {m.projects_wizard_dir_preview_title()}
        </h3>
        <div className="pv-wizard-layout__preview">
          {examplePaths.structure.map((path, i) => {
            const depth =
              path.split('/').length - examplePaths.root.split('/').length;
            const isDir = path.endsWith('/') || path.includes('(');
            return (
              <div
                key={i}
                className="pv-wizard-layout__path-row"
                data-dir={isDir ? 'true' : undefined}
                // eslint-disable-next-line no-restricted-syntax -- dynamic: depth-based indent padding for layout tree rows
                style={{ paddingLeft: `${Math.max(0, depth) * 12}px` }}
              >
                {isDir ? '\u{1F4C1} ' : '\u{1F4C4} '}
                {path.replace(examplePaths.root, '.')}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
