// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import type { ReactNode } from 'react';
import {
  actionBar,
  actionBarActions,
  actionBarSpacer,
  actionBarSubtitle,
  actionBarTitle,
} from '@/styles/app-shell.css';

export interface TopActionBarProps {
  title: string;
  subtitle?: string;
  right?: ReactNode;
  children?: ReactNode;
}

export function TopActionBar({
  title,
  subtitle,
  right,
  children,
}: TopActionBarProps) {
  return (
    <div className={actionBar}>
      <span className={actionBarTitle}>{title}</span>
      {subtitle && <span className={actionBarSubtitle}>{subtitle}</span>}
      {children}
      <span className={actionBarSpacer} />
      {right && <div className={actionBarActions}>{right}</div>}
    </div>
  );
}
