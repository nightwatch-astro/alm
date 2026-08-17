// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// Extracted from ChecklistSection.tsx (kyo7.104 refactor) to break the
// mutual import cycle introduced when ChecklistGroup was split out.

import { useState } from 'react';
import { Check, Lock, Search, X } from 'lucide-react';
import { clsx } from 'clsx';
import { m } from '@/lib/i18n';
import { Tooltip } from '@/ui';
import type { OnboardingItemDto, OnboardingPage } from '@/bindings/index';
import { setOnboardingItemState } from './store';
import { toggleFind, useActiveFindItem } from './FindSpotlight';
import {
  itemLabel,
  itemTooltip,
  prerequisiteReason,
  pageLabel,
} from './onboarding-labels';

export interface ChecklistItemRowProps {
  item: OnboardingItemDto;
  idPrefix: string;
  /** True while the row plays its completion choreography before it drops to
   * the completed area (T024). */
  completing: boolean;
  onJump: (page: OnboardingPage) => void;
}

/** One open (unchecked) item row: label, hover/focus tooltip (WCAG 1.4.13),
 * a manual check affordance for non-auto items, and — when the upstream
 * milestone is missing — a prerequisite reason plus a jump link (FR-010). */
export function ChecklistItemRow({
  item,
  idPrefix,
  completing,
  onJump,
}: ChecklistItemRowProps): React.ReactElement {
  const safeId = item.itemId.replaceAll('.', '_');

  const labelId = `${idPrefix}-lbl-${safeId}`;
  const blocked = item.prerequisite != null && !item.prerequisite.met;
  const label = itemLabel(item.itemId);
  const findActive = useActiveFindItem()?.itemId === item.itemId;

  // Tooltip (FR-008 / WCAG 1.4.13) is the shared `Tooltip` primitive.
  //
  // It was a bespoke reveal twice over and both attempts were wrong. First pure
  // CSS (`:hover, :focus-within`), which pinned the tooltip open after any click
  // inside the row and could not honour Escape at all. Then hand-rolled hover +
  // :focus-visible + Escape state here, which fixed the pinning but still felt
  // clunky — no delay, no positioning, no collision handling. base-ui owns all
  // of that, and it is what every other tooltip in the app already uses.
  //
  // #1103: the shared Tooltip's trigger is a bare, NON-focusable span, so
  // delegating to it made the text pointer-only — keyboard and screen-reader
  // users got nothing, failing 1.4.13 and T017's "hover AND focus".
  //
  // The reveal is therefore owned by the row's CHECKBOX, which is already in the
  // tab order: focusing it opens the popup, blurring or Escape closes it, and it
  // carries `aria-describedby` so assistive tech reads the explanation without
  // depending on the visual popup at all. Driving it from the checkbox rather
  // than making the label span focusable keeps the tab order at one stop per
  // row — a focusable label would have added a second stop per item.
  //
  // Fully controlled, never partly: passing `open={x || undefined}` flips the
  // popup between controlled and uncontrolled, and base-ui then keeps its own
  // internal state — which silently broke Escape. One boolean owns it, with
  // base-ui reporting its hover transitions through `onOpenChange`, so hover
  // and keyboard both flow through the same state.
  const [tipOpen, setTipOpen] = useState(false);
  const tooltipId = `${idPrefix}-tt-${safeId}`;

  const check = () =>
    void setOnboardingItemState(item.itemId, 'manually_checked');
  const dismiss = () => void setOnboardingItemState(item.itemId, 'dismissed');

  // Settling in place (T024): a checked, emphasised row that has not yet
  // dropped to the completed area. Non-interactive during the animation.
  if (completing) {
    return (
      <li
        className="pv-onb-checklist__item pv-onb-checklist__item--completing"
        data-testid="checklist-item-row"
        data-item-id={item.itemId}
        data-completing="true"
      >
        <Check size={14} aria-hidden className="pv-onb-checklist__check-icon" />
        <span className="pv-onb-checklist__item-main">
          <span
            className="pv-onb-checklist__item-label"
            data-testid="onb-checklist-item-label"
          >
            {label}
          </span>
        </span>
      </li>
    );
  }

  return (
    <li
      className={clsx(
        'pv-onb-checklist__item',
        blocked && 'pv-onb-checklist__item--blocked',
      )}
      data-testid="checklist-item-row"
      data-item-id={item.itemId}
      data-blocked={blocked ? 'true' : undefined}
      data-auto={item.hasAutoTick ? 'true' : undefined}
    >
      {item.hasAutoTick ? (
        <span className="pv-onb-checklist__auto-marker" aria-hidden />
      ) : (
        <button
          type="button"
          role="checkbox"
          aria-checked={false}
          aria-labelledby={labelId}
          aria-describedby={tooltipId}
          className="pv-onb-checklist__check"
          onClick={check}
          disabled={blocked}
          // #1103: this control owns the tooltip reveal for keyboard users.
          //
          // Reveals on ANY focus, deliberately not gated on `:focus-visible`.
          // Gating on it was tried first and is wrong here: `:focus-visible` does
          // not match focus moved programmatically after pointer input, which is
          // exactly how assistive tech and `element.focus()` arrive — so the
          // people this fix exists for were the ones it silently skipped. The
          // cost is that a pointer click also pops the tooltip briefly; that is
          // acceptable, since hovering to reach the checkbox already showed it.
          onFocus={() => setTipOpen(true)}
          onBlur={() => setTipOpen(false)}
          onKeyDown={(e) => {
            // 1.4.13 "dismissible". base-ui's own `useDismiss` handles Escape for
            // ITS trigger — but the reveal here is owned by this checkbox, which
            // is not that trigger, so Escape never reaches it. Verified by
            // removing this handler: the e2e Escape assertion fails.
            //
            // This is NOT a hand-rolled reveal: base-ui still owns the popup,
            // positioning, delays and hoverable safe-polygon. Only the dismiss
            // key is bridged from the control that owns the open state.
            if (e.key === 'Escape' && tipOpen) {
              e.stopPropagation();
              setTipOpen(false);
            }
          }}
        >
          <span className="pv-onb-checklist__checkbox" aria-hidden />
        </button>
      )}
      <span className="pv-onb-checklist__item-main">
        {/*
          Shared Tooltip (base-ui): portalled, positioned, with the app's
          standard open/close delays and dismissal. Replaces a bespoke
          CSS-plus-local-state reveal that felt clunky and had to hand-roll
          hover, keyboard focus and Escape itself.
        */}
        <Tooltip
          content={itemTooltip(item.itemId)}
          sideOffset={6}
          popupId={tooltipId}
          open={tipOpen}
          onOpenChange={setTipOpen}
        >
          <span
            id={labelId}
            className="pv-onb-checklist__item-label"
            data-testid="onb-checklist-item-label"
          >
            {label}
          </span>
        </Tooltip>
        {blocked && item.prerequisite && (
          <span className="pv-onb-checklist__prereq">
            <Lock
              size={10}
              aria-hidden
              className="pv-onb-checklist__prereq-icon"
            />
            <span className="pv-onb-checklist__prereq-reason">
              {prerequisiteReason(item.prerequisite.reasonKey)}
            </span>
            <button
              type="button"
              className="pv-onb-checklist__prereq-jump"
              onClick={() =>
                item.prerequisite && onJump(item.prerequisite.jumpPage)
              }
            >
              {pageLabel(item.prerequisite.jumpPage)}
            </button>
          </span>
        )}
      </span>
      <span className="pv-onb-checklist__actions">
        {!item.hasAutoTick && (
          <button
            type="button"
            className="pv-onb-checklist__dismiss"
            aria-label={m.onboarding_item_dismiss_label({ item: label })}
            onClick={dismiss}
          >
            <X size={13} aria-hidden />
          </button>
        )}
        <button
          type="button"
          className="pv-onb-checklist__find"
          aria-label={m.onboarding_find_label({ item: label })}
          aria-pressed={findActive}
          aria-describedby={item.hasAutoTick ? tooltipId : undefined}
          onFocus={item.hasAutoTick ? () => setTipOpen(true) : undefined}
          onBlur={item.hasAutoTick ? () => setTipOpen(false) : undefined}
          onClick={() => toggleFind(item)}
        >
          <Search size={13} aria-hidden />
        </button>
      </span>
    </li>
  );
}
