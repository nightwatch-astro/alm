// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * DetailDockPlacementControl — spec 054 T021 (US4), issue #1066.
 *
 * The ONE Auto/Bottom/Right control for detail-panel dock placement, rendered
 * through the shared `SegControl` (owner mandate: one parameterised component,
 * never a per-feature clone). "Right" (not "Side") is the user-facing wording;
 * "Auto" is the adaptive mode.
 *
 * This exists because the placement model is three-state but the UI that
 * preceded it was a two-state toggle: `useAdaptiveDock` treats `override ===
 * null` as "follow the automatic width rule", yet the old
 * `.pv-listpage__detail-pin` button only ever called `setOverride('side')` or
 * `setOverride('bottom')`. Once a user touched it, Auto was unreachable
 * without clearing localStorage (#1066). Mapping the third state onto a real
 * control is the fix.
 *
 * Icon-only (#1068): three text buttons ran ~175px wide, dominating a
 * ~390px narrow-dock detail bar. `SegControl`'s `icon` option renders each
 * option as an icon with `label` moved to `aria-label`/`title`, so the
 * accessible name (asserted by tests/e2e as "Auto"/"Bottom"/"Right") and a
 * hover tooltip are unchanged — only the visible glyph differs.
 */

import { Wand2, PanelBottom, PanelRight } from 'lucide-react';
import { dockPlacementControl } from './ListPageLayout.css';
import { SegControl } from '@/ui';
import type { DockPlacement } from '@/ui';
import { m } from '@/lib/i18n';

/** The user-facing three-way choice. `'adaptive'` is `override === null`. */
export type DetailDockMode = 'adaptive' | DockPlacement;

export interface DetailDockPlacementControlProps {
  /**
   * Current explicit pin, or `null` when following the automatic width rule
   * (i.e. `useAdaptiveDock().override`, NOT its resolved `placement` — the
   * resolved value can never express "Auto").
   */
  override: DockPlacement | null;
  /** Passed straight to `useAdaptiveDock().setOverride`; `null` clears the pin. */
  onChange: (value: DockPlacement | null) => void;
  className?: string;
}

export function DetailDockPlacementControl({
  override,
  onChange,
  className,
}: DetailDockPlacementControlProps) {
  return (
    <SegControl
      className={[dockPlacementControl, className].filter(Boolean).join(' ')}
      data-testid="dock-placement-control"
      aria-label={m.detail_dock_placement_aria()}
      options={[
        {
          value: 'adaptive',
          label: m.detail_dock_placement_auto(),
          icon: <Wand2 size={14} aria-hidden="true" />,
        },
        {
          value: 'bottom',
          label: m.detail_dock_placement_bottom(),
          icon: <PanelBottom size={14} aria-hidden="true" />,
        },
        {
          value: 'side',
          label: m.detail_dock_placement_right(),
          icon: <PanelRight size={14} aria-hidden="true" />,
        },
      ]}
      value={override ?? 'adaptive'}
      onChange={(value) =>
        onChange(value === 'adaptive' ? null : (value as DockPlacement))
      }
    />
  );
}
