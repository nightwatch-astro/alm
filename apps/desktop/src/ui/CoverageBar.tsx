// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { forwardRef } from 'react';
import type { HTMLAttributes } from 'react';
import * as cb from './CoverageBar.css';

export interface CoverageBarProps extends HTMLAttributes<HTMLDivElement> {
  label: string;
  value: number;
  max: number;
  /**
   * Unit suffix appended to the value label. Defaults to `'h'` (hours) for
   * backward compatibility. Ignored when `formatLabel` is supplied.
   */
  unit?: string;
  /** Full control over the value label; overrides `unit`. */
  formatLabel?: (value: number, max: number) => string;
}

export const CoverageBar = forwardRef<HTMLDivElement, CoverageBarProps>(
  function CoverageBar(
    { label, value, max, unit = 'h', formatLabel, className, ...rest },
    ref,
  ) {
    const pct = Math.min(100, (value / max) * 100);
    const fillKey = pct < 40 ? 'low' : pct >= 80 ? 'ok' : 'default';
    const rootCls = [cb.root, className].filter(Boolean).join(' ');
    const valueLabel = formatLabel
      ? formatLabel(value, max)
      : `${value}${unit}`;
    return (
      <div ref={ref} className={rootCls} {...rest}>
        <span className={cb.label}>{label}</span>
        <div className={cb.bar}>
          <div
            className={cb.fillVariants[fillKey]}
            // eslint-disable-next-line no-restricted-syntax -- dynamic: coverage bar fill width %
            style={{ width: `${pct}%` }}
          />
        </div>
        <span className={cb.value}>{valueLabel}</span>
      </div>
    );
  },
);
CoverageBar.displayName = 'CoverageBar';
