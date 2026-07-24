// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import type { ReactNode, Ref } from 'react';
import { m } from '@/lib/i18n';
import {
  listSidebar,
  search as listSidebarSearch,
  controls as listSidebarControls,
  list as listSidebarList,
  footer as listSidebarFooter,
} from './ListSidebar.css';
import { virtualScroll } from '@/ui/page-layout.css';

export interface ListSidebarProps {
  placeholder?: string;
  /** Controlled search value. When provided, the input is controlled. */
  searchValue?: string;
  /** Called when the user types in the search box. */
  onSearchChange?: (value: string) => void;
  controls?: ReactNode;
  footer?: ReactNode;
  children: ReactNode;
  /**
   * Forward a ref to the scrolling list container so callers can virtualize
   * its contents (the container owns `overflow-y: auto`). When set, the
   * container is tagged `data-virtual-scroll` so a virtualizer can target it.
   */
  scrollRef?: Ref<HTMLDivElement>;
  /** Tag the list container as a virtual-scroll viewport (test/measure hook). */
  virtualized?: boolean;
}

export function ListSidebar({
  placeholder,
  searchValue,
  onSearchChange,
  controls,
  footer,
  children,
  scrollRef,
  virtualized = false,
}: ListSidebarProps) {
  return (
    <div className={listSidebar}>
      <div className={listSidebarSearch}>
        <input
          type="text"
          placeholder={placeholder || m.common_search_placeholder()}
          value={searchValue ?? ''}
          onChange={
            onSearchChange ? (e) => onSearchChange(e.target.value) : undefined
          }
          readOnly={!onSearchChange}
          aria-label={placeholder || m.common_search_aria()}
        />
      </div>
      {controls && <div className={listSidebarControls}>{controls}</div>}
      <div
        className={[listSidebarList, virtualized ? virtualScroll : undefined]
          .filter(Boolean)
          .join(' ')}
        ref={scrollRef}
        data-virtual-scroll={virtualized ? 'true' : undefined}
      >
        {children}
      </div>
      {footer && <div className={listSidebarFooter}>{footer}</div>}
    </div>
  );
}
