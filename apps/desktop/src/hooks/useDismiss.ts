// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Dismiss-on-outside-pointerdown / dismiss-on-Escape for popovers and menus.
 *
 * Listeners are document-level and only attached while `enabled`, so a closed
 * surface costs nothing. `pointerdown` (not `click`) fires before focus moves,
 * which is what keeps a dismiss from racing the trigger's own toggle.
 *
 * `refs` takes every element that counts as "inside". A portalled panel is not
 * a DOM descendant of its anchor, so both refs must be listed or each click in
 * the panel would dismiss it.
 *
 * `onDismiss` receives the originating event: a surface that stops Escape
 * propagation or restores focus to its trigger does so from the callback, since
 * those concerns differ per consumer. It is read through a ref, so registration
 * depends on `enabled` alone — an inline callback does not re-register the
 * listeners on every render.
 */

import { useEffect, useRef } from 'react';
import type { RefObject } from 'react';

export type DismissEvent = PointerEvent | KeyboardEvent;

export function useDismiss(
  refs: ReadonlyArray<RefObject<HTMLElement | null>>,
  onDismiss: (event: DismissEvent) => void,
  enabled: boolean,
): void {
  const latest = useRef({ refs, onDismiss });
  latest.current = { refs, onDismiss };

  useEffect(() => {
    if (!enabled) return undefined;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      latest.current.onDismiss(e);
    };
    const onPointerDown = (e: PointerEvent) => {
      const target = e.target as Node;
      if (latest.current.refs.some((ref) => ref.current?.contains(target))) {
        return;
      }
      latest.current.onDismiss(e);
    };
    document.addEventListener('keydown', onKeyDown);
    document.addEventListener('pointerdown', onPointerDown);
    return () => {
      document.removeEventListener('keydown', onKeyDown);
      document.removeEventListener('pointerdown', onPointerDown);
    };
  }, [enabled]);
}
