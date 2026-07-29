// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import type { ReactNode } from 'react';
import clsx from 'clsx';
import {
  actionBar,
  actionBarWrap,
  title as actionBarTitle,
  subtitle as actionBarSubtitle,
  spacer as actionBarSpacer,
  spacerWrap as actionBarSpacerWrap,
  actions as actionBarActions,
  actionsWrap as actionBarActionsWrap,
} from './TopActionBar.css';

export interface TopActionBarProps {
  title: string;
  subtitle?: string;
  right?: ReactNode;
  children?: ReactNode;
  /**
   * Lay the action cluster out on its own row instead of one fixed-height row
   * (task #81). Needed where the bar sits above content it would overlap.
   */
  wrap?: boolean;
}

export function TopActionBar({
  title,
  subtitle,
  right,
  children,
  wrap = false,
}: TopActionBarProps) {
  return (
    <div
      className={clsx(actionBar, wrap && actionBarWrap)}
      data-testid="top-action-bar"
    >
      <span className={actionBarTitle}>{title}</span>
      {subtitle && <span className={actionBarSubtitle}>{subtitle}</span>}
      {children}
      <span className={clsx(actionBarSpacer, wrap && actionBarSpacerWrap)} />
      {right && (
        <div className={clsx(actionBarActions, wrap && actionBarActionsWrap)}>
          {right}
        </div>
      )}
    </div>
  );
}
