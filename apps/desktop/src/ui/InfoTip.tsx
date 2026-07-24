// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { Tooltip } from './Tooltip';
import { infoTip as infoTipCls } from './InfoTip.css';
import { m } from '@/lib/i18n';

export interface InfoTipProps {
  /**
   * Help text revealed on hover/focus.
   *
   * `string`, not `ReactNode`, because the tip is mirrored into `aria-label`
   * and only a string can be. A node-valued tip leaves the accessible name as
   * the bare "More information" prefix while the real text stays unreachable:
   * base-ui portals the popup and mounts it only once open, and the closed
   * trigger carries no `aria-describedby`. Rich tooltip content has no
   * accessible equivalent here — it needs visible text, not a wider type.
   */
  tip: string;
  /** Accessible label prefix; defaults to "More information". */
  label?: string;
  className?: string;
}

/**
 * Small ⓘ affordance that reveals help text on hover/focus — the de-vibe
 * replacement for always-on help prose under form rows (settings mock).
 * Token-only styling lives in components/primitives.css under `.pv-info-tip`,
 * alongside `.pv-lock`, with which it shares one `:focus-visible` rule.
 *
 * Uses the shared base-ui `Tooltip`; the tip text is also mirrored into
 * `aria-label` so screen readers get it without a hover. The trigger is
 * focusable (`tabIndex={0}`) so keyboard users can reveal it too.
 *
 * Distinct from `Lock`, which marks protected state rather than supplying help
 * text and therefore has a decorative mode this component must not gain. See
 * `docs/adr/0002-lock-and-infotip-stay-separate.md`.
 */
export function InfoTip({
  tip,
  label = m.infotip_more_information(),
  className,
}: InfoTipProps) {
  const cls = [infoTipCls, className].filter(Boolean).join(' ');
  return (
    <Tooltip
      content={tip}
      className={cls}
      data-testid="info-tip"
      role="note"
      tabIndex={0}
      aria-label={`${label}: ${tip}`}
    >
      {/* eslint-disable-next-line alm/no-user-string -- decorative icon glyph, not user prose */}
      {'i'}
    </Tooltip>
  );
}
