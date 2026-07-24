// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import type { ReactNode } from 'react';
import {
  pageBar,
  threePane,
  threePaneContent,
  threePaneSidebar,
  twoPane,
  twoPaneDetail,
} from '@/styles/app-shell.css';

export interface ListDetailLayoutProps {
  topBar?: ReactNode;
  list: ReactNode;
  detail: ReactNode;
  sidebar?: ReactNode;
}

export function ListDetailLayout({
  topBar,
  list,
  detail,
  sidebar,
}: ListDetailLayoutProps) {
  if (sidebar) {
    return (
      <>
        {topBar && (
          <div className={pageBar} data-testid="page-bar">
            {topBar}
          </div>
        )}
        <div className={threePane}>
          {list}
          <div className={threePaneContent}>{detail}</div>
          <div className={threePaneSidebar}>{sidebar}</div>
        </div>
      </>
    );
  }
  return (
    <>
      {topBar && (
        <div className={pageBar} data-testid="page-bar">
          {topBar}
        </div>
      )}
      <div className={twoPane}>
        {list}
        <div className={twoPaneDetail} data-testid="two-pane-detail">
          {detail}
        </div>
      </div>
    </>
  );
}
