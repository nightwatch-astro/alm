// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import type { ReactNode } from 'react';
import {
  header as detailHeader,
  content as detailHeaderContent,
  title as detailTitle,
  actions as detailActions,
} from './DetailHeader.css';

export interface DetailHeaderProps {
  title: ReactNode;
  titleExtra?: ReactNode;
  subtitle?: string;
  actions?: ReactNode;
  children?: ReactNode;
}

export function DetailHeader({
  title,
  titleExtra,
  subtitle,
  actions,
  children,
}: DetailHeaderProps) {
  return (
    <div className={detailHeader}>
      <div className={detailHeaderContent}>
        <div className={detailTitle}>
          {title}
          {titleExtra}
        </div>
        {subtitle && <div className="pv-detail__subtitle">{subtitle}</div>}
        {children}
      </div>
      {actions && <div className={detailActions}>{actions}</div>}
    </div>
  );
}
