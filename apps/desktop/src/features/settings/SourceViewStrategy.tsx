// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { useState } from 'react';
import { Select } from '@base-ui-components/react/select';
import { m } from '@/lib/i18n';
import { selectBase } from '@/styles/select.css';

interface SourceViewStrategyProps {
  save: (scope: string, values: Record<string, unknown>) => void;
}

interface StrategyOption {
  id: string;
  /** Render-time thunks so the strings re-read the active locale (spec 046 #8). */
  label: () => string;
  description: () => string;
}

const STRATEGIES: StrategyOption[] = [
  {
    id: 'junctions',
    label: () => m.settings_sourceview_label_junctions(),
    description: () => m.settings_sourceview_desc_junctions(),
  },
  {
    id: 'symlinks',
    label: () => m.settings_sourceview_label_symlinks(),
    description: () => m.settings_sourceview_desc_symlinks(),
  },
  {
    id: 'hardlinks',
    label: () => m.settings_sourceview_label_hardlinks(),
    description: () => m.settings_sourceview_desc_hardlinks(),
  },
  {
    id: 'copy',
    label: () => m.settings_sourceview_label_copy(),
    description: () => m.settings_sourceview_desc_copy(),
  },
];

export function SourceViewStrategy({ save }: SourceViewStrategyProps) {
  const [selected, setSelected] = useState('junctions');

  const handleChange = (value: string | null) => {
    if (value === null) return;
    setSelected(value);
    save('source_view', { strategy: value });
  };

  const current = STRATEGIES.find((s) => s.id === selected);

  return (
    <div className="pv-svs">
      <div className="pv-svs__field">
        {}
        <label className="pv-svs__label">
          {m.settings_sourceview_default_strategy()}
        </label>
        <Select.Root value={selected} onValueChange={handleChange}>
          <Select.Trigger
            className={selectBase}
            aria-label={m.settings_sourceview_strategy_aria()}
          >
            <Select.Value />
            <Select.Icon className="pv-select__icon" />
          </Select.Trigger>
          <Select.Portal>
            <Select.Positioner>
              <Select.Popup className="pv-select__popup">
                {STRATEGIES.map((s) => (
                  <Select.Item
                    key={s.id}
                    value={s.id}
                    className="pv-select__item"
                  >
                    <Select.ItemText>{s.label()}</Select.ItemText>
                  </Select.Item>
                ))}
              </Select.Popup>
            </Select.Positioner>
          </Select.Portal>
        </Select.Root>
      </div>

      {current && (
        <p className="pv-svs__description">{current.description()}</p>
      )}

      <div className="pv-svs__all-options">
        {STRATEGIES.map((s) => (
          <div key={s.id} className="pv-svs__option-card">
            <strong className="pv-svs__option-name">{s.label()}</strong>
            <p className="pv-svs__option-desc">{s.description()}</p>
          </div>
        ))}
      </div>
    </div>
  );
}
