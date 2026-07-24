// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { forwardRef } from 'react';
import type { ReactNode, HTMLAttributes } from 'react';
import { variantStyles } from './Pill.css';

export type PillVariant =
  | 'neutral'
  | 'ghost'
  | 'ok'
  | 'warn'
  | 'danger'
  | 'info'
  | 'accent';
export interface PillProps extends HTMLAttributes<HTMLSpanElement> {
  variant?: PillVariant;
  children: ReactNode;
}

export const Pill = forwardRef<HTMLSpanElement, PillProps>(function Pill(
  { variant = 'neutral', className, children, ...rest },
  ref,
) {
  const cls = [variantStyles[variant], className].filter(Boolean).join(' ');
  return (
    <span ref={ref} className={cls} data-testid="pill" {...rest}>
      {children}
    </span>
  );
});
Pill.displayName = 'Pill';
