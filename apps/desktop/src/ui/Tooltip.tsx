// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import type { ComponentPropsWithoutRef, ReactNode } from 'react';
import { Tooltip as BaseTooltip } from '@base-ui-components/react/tooltip';
import './Tooltip.css';

export interface TooltipProps
  extends Omit<ComponentPropsWithoutRef<'span'>, 'content'> {
  /** Content revealed in the popup on hover/focus. */
  content: ReactNode;
  /** Trigger content (glyph, icon, or label). */
  children: ReactNode;
  /** Popup offset from the trigger, in px. */
  sideOffset?: number;
  /**
   * Force the popup open. Use when a DIFFERENT element owns the reveal — e.g.
   * a row whose focusable control is a sibling checkbox rather than the trigger
   * span itself. Uncontrolled (hover/focus on the trigger) when omitted.
   */
  open?: boolean;
  /** Controlled-open change handler; pairs with `open`. */
  onOpenChange?: (open: boolean) => void;
  /** Popup id, so another element can point `aria-describedby` at it. */
  popupId?: string;
}

/**
 * Single token-styled tooltip wrapper over base-ui. The trigger renders as a
 * `<span>` so it can wrap inline affordances (Lock glyph, InfoTip icon);
 * `className` and any extra span attributes (aria-label, tabIndex, data-*)
 * pass through to that trigger span. Popup styling lives in `.pv-tooltip`.
 *
 * ACCESSIBILITY: that trigger span is NOT focusable — base-ui adds no
 * `tabIndex` and a bare span has none. So a tooltip whose only reveal is this
 * trigger is pointer-only, which fails WCAG 1.4.13 (#1103). Callers must give
 * keyboard users a path: either pass `tabIndex={0}` through to the trigger, or
 * drive `open` from a real focusable control nearby and point that control's
 * `aria-describedby` at `popupId`.
 */
export function Tooltip({
  content,
  children,
  className,
  sideOffset = 4,
  open,
  onOpenChange,
  popupId,
  ...rest
}: TooltipProps) {
  return (
    <BaseTooltip.Provider>
      <BaseTooltip.Root open={open} onOpenChange={onOpenChange}>
        <BaseTooltip.Trigger className={className} render={<span {...rest} />}>
          {children}
        </BaseTooltip.Trigger>
        <BaseTooltip.Portal>
          <BaseTooltip.Positioner sideOffset={sideOffset}>
            <BaseTooltip.Popup
              id={popupId}
              className="pv-tooltip"
              data-testid="tooltip"
            >
              {content}
            </BaseTooltip.Popup>
          </BaseTooltip.Positioner>
        </BaseTooltip.Portal>
      </BaseTooltip.Root>
    </BaseTooltip.Provider>
  );
}
