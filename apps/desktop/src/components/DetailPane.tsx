// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import type { ReactNode } from 'react';
import { detail } from './DetailPane.css';

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
      // `pv-detail` is a structural hook, NOT a style class: the un-migrated
      // list-detail layout (tables-lists.css) keys its scroll contract on
      // `.pv-listpage__detail-body > .pv-detail` and `:has(> .pv-detail)`.
      // Dropping it triggers the no-scroll fallback and clips the pane (#816).
      // Visual padding comes from the vanilla-extract `detail` class.
      className={`${detail} pv-detail${fill ? ' pv-detail--fill' : ''}`}
      data-testid="detail"
    >
      {children}
    </div>
  );
}
