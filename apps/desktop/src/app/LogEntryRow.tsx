// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * LogEntryRow — single virtualized row in the log panel.
 *
 * Migrated to vanilla-extract (pilot/css-vanilla-extract branch).
 */

import { useCallback } from 'react';
import { m } from '@/lib/i18n';
import type { LogEntry } from '@/data/logStore';
import { formatTimeOfDay } from '@/lib/datetime';
import {
  buildEntityPath,
  type EntityNavigateFn,
  type AuditNavigateFn,
} from './log-panel-model';
import {
  event,
  eventLink,
  eventTime,
  levelVariants,
  eventSource,
  eventContext,
  eventMsg,
  eventLinkIndicator,
} from './logpanel.css';

export interface LogEntryRowProps {
  entry: LogEntry;
  onNavigateEntity: EntityNavigateFn;
  onNavigateAudit: AuditNavigateFn;
  /** Virtual-row positioning style (absolute + translateY). */
  style?: React.CSSProperties;
  /** Virtual-row index for the virtualizer's measure cache. */
  index?: number;
  /** Virtualizer measure callback ref. */
  measureRef?: (node: Element | null) => void;
}

export function LogEntryRow({
  entry,
  onNavigateEntity,
  onNavigateAudit,
  style,
  index,
  measureRef,
}: LogEntryRowProps) {
  const hasEntity = entry.entityType != null && entry.entityId != null;
  const hasEntityLink =
    hasEntity &&
    buildEntityPath(entry.entityType ?? '', entry.entityId ?? '') != null;
  const hasAuditLink = entry.requestId != null && !hasEntity;
  const contextLabel = hasEntity
    ? `${entry.entityType} · ${entry.entityId}`
    : hasAuditLink
      ? entry.requestId
      : null;

  const handleClick = useCallback(() => {
    if (hasEntityLink && entry.entityType && entry.entityId) {
      onNavigateEntity(entry.entityType, entry.entityId);
    } else if (hasAuditLink && entry.requestId) {
      onNavigateAudit(entry.requestId);
    }
  }, [entry, hasEntityLink, hasAuditLink, onNavigateEntity, onNavigateAudit]);

  const isClickable = hasEntityLink || hasAuditLink;

  return (
    // eslint-disable-next-line jsx-a11y/no-noninteractive-element-interactions
    <li
      ref={measureRef}
      data-testid="log-entry-row"
      data-index={index}
      // eslint-disable-next-line no-restricted-syntax
      style={style}
      className={`${event}${isClickable ? ` ${eventLink}` : ''}`}
      onClick={isClickable ? handleClick : undefined}
      role={isClickable ? 'button' : 'listitem'}
      // eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex
      tabIndex={isClickable ? 0 : undefined}
      onKeyDown={
        isClickable
          ? (e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                handleClick();
              }
            }
          : undefined
      }
      aria-label={
        isClickable
          ? m.log_entry_navigate_aria({
              level: entry.level,
              message: entry.message,
            })
          : undefined
      }
    >
      <span className={eventTime}>{formatTimeOfDay(entry.time)}</span>
      <span className={levelVariants[entry.level]} aria-label={entry.level}>
        {entry.level}
      </span>
      <span
        className={eventSource}
        data-testid="logpanel-event-source"
        data-source={entry.source}
      >
        {entry.source}
      </span>
      {contextLabel && (
        <span className={eventContext} title={contextLabel}>
          {contextLabel}
        </span>
      )}
      <span className={eventMsg}>{entry.message}</span>
      {hasEntityLink && (
        <span className={eventLinkIndicator} aria-hidden="true">
          →
        </span>
      )}
    </li>
  );
}
