// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import type { ReactNode } from 'react';
import { detail } from '@/styles/app-shell.css';

export interface DetailPaneProps {
  children: ReactNode;
  /**
   * Dashboard mode (design v4): the pane fills the available height, the header
   * and metric line stay pinned, and the primary column scrolls independently
   * of the rail. Use with DetailGrid. Without it, the pane scrolls as one block.
   */
  fill?: boolean;
}

export function DetailPane({ children, fill }: DetailPaneProps) {
  return (
    <div
      className={`${detail}${fill ? ' pv-detail--fill' : ''}`}
      data-testid="detail"
    >
      {children}
    </div>
  );
}
