// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import type { ReactNode } from 'react';
import {
  actionBar,
  title as actionBarTitle,
  subtitle as actionBarSubtitle,
  spacer as actionBarSpacer,
  actions as actionBarActions,
} from './TopActionBar.css';

export interface TopActionBarProps {
  title: string;
  subtitle?: string;
  right?: ReactNode;
  children?: ReactNode;
}

/**
 * Renders a top action bar with a title, optional subtitle, content, and right-aligned actions.
 *
 * @param title - The action bar title
 * @param subtitle - Optional subtitle displayed beside the title
 * @param right - Optional content displayed in the action area
 * @param children - Additional content rendered within the action bar
 */
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
