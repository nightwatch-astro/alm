// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import type { HTMLAttributes } from 'react';
import { lock as lockCls } from './Lock.css';
import { Tooltip } from './Tooltip';
import { m } from '@/lib/i18n';

export interface LockProps extends HTMLAttributes<HTMLSpanElement> {
  reason?: string;
  /**
   * Render as pure decoration: no tooltip, no role, no tab stop, hidden from
   * assistive tech. Only correct where the reason is already stated in text
   * nearby AND is identical for every instance — otherwise the padlock is the
   * sole carrier of that information and must stay reachable (see below).
   */
  decorative?: boolean;
}

/**
 * Padlock glyph whose tooltip explains why a row or category is protected.
 *
 * `role="note"` + `tabIndex={0}` mirror `InfoTip`, and both are load-bearing
 * (WCAG 1.4.13 / 2.1.1, same defect class as #1103):
 *
 * - The shared `Tooltip` renders its trigger as a bare `<span>` and base-ui
 *   adds no `tabIndex`, so without one the reason is reachable by pointer
 *   hover only — and the reason is not redundant prose. The cleanup-row hint
 *   ("requires explicit acknowledgement during plan review") states a
 *   consequence that appears nowhere else on screen; the adjacent pill says
 *   only "Protected".
 * - `aria-label` on a role-less `<span>` is not reliably exposed — naming
 *   applies to elements with a role that supports it — so the role is what
 *   makes the label reach assistive tech at all.
 *
 * Distinct from `InfoTip`, which supplies help text and therefore has no
 * decorative mode. See `docs/adr/0002-lock-and-infotip-stay-separate.md`.
 */
export function Lock({ reason, decorative, className, ...rest }: LockProps) {
  const label = reason ?? m.settings_cleanup_protection_protected();
  const cls = [lockCls, className].filter(Boolean).join(' ');

  // Decorative instances carry no information of their own, so giving each one
  // a role and a tab stop would add N identical announcements to the tab order
  // (the cleanup table's per-row hint is one static sentence, repeated on every
  // protected row). The rule is stated once above that table instead, and the
  // row's "Protected" pill still marks the state in text.
  if (decorative) {
    return (
      <span className={cls} aria-hidden="true" {...rest}>
        &#x1F512;
      </span>
    );
  }

  return (
    <Tooltip
      content={label}
      className={cls}
      role="note"
      tabIndex={0}
      aria-label={label}
      {...rest}
    >
      &#x1F512;
    </Tooltip>
  );
}
