// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * StatusTag — shared status indicator: a small colored dot + plain-text label.
 *
 * The Tier-2 canonical affordance for "state at a glance" (project lifecycle,
 * and any future dot+label status). Color is a 6px accent dot only, never a
 * filled badge background, so it stays quiet in dense tables. The `variant`
 * maps to the same {@link PillVariant} tone semantics used elsewhere, so
 * callers need no extra mapping.
 *
 * Not for filled badges (use `Pill`) or multi-step flowcharts (use the
 * lifecycle stepper). CSS: `.pv-status-tag` in styles/components.
 */

import type { PillVariant } from '@/ui';
import { variantStyles, dotVariants } from './StatusTag.css';

export interface StatusTagProps {
  variant: PillVariant;
  children: string;
}

export function StatusTag({ variant, children }: StatusTagProps) {
  return (
    <span className={variantStyles[variant]}>
      <span className={dotVariants[variant]} aria-hidden="true" />
      {children}
    </span>
  );
}
