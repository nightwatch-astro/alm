// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import type { ReactNode } from 'react';
import {
  listItem,
  listItemSelected,
  title as listItemTitle,
  meta as listItemMeta,
} from './ListItem.css';

export interface ListItemProps {
  selected?: boolean;
  onClick?: () => void;
  title: ReactNode;
  pills?: ReactNode;
  meta?: ReactNode;
}

/**
 * Renders a list item with optional selection, supplementary content, and interaction.
 *
 * @param selected - Whether the item is displayed as selected
 * @param onClick - Action invoked when the interactive item is activated
 * @param title - Main content displayed in the item
 * @param pills - Supplementary content displayed alongside the title
 * @param meta - Secondary content displayed beneath the title
 */
export function ListItem({
  selected,
  onClick,
  title,
  pills,
  meta,
}: ListItemProps) {
  return (
    // eslint-disable-next-line jsx-a11y/no-static-element-interactions -- interactivity is conditional; role becomes button and a keydown handler is attached only when onClick is provided
    <div
      className={[listItem, selected ? listItemSelected : undefined]
        .filter(Boolean)
        .join(' ')}
      data-testid="list-item"
      onClick={onClick}
      role={onClick ? 'button' : undefined}
      // eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex -- only focusable when onClick is provided, where role becomes button
      tabIndex={onClick ? 0 : undefined}
      aria-pressed={onClick ? Boolean(selected) : undefined}
      onKeyDown={
        onClick
          ? (e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                onClick();
              }
            }
          : undefined
      }
    >
      <div className={listItemTitle}>
        {title}
        {pills}
      </div>
      {meta && <div className={listItemMeta}>{meta}</div>}
    </div>
  );
}
