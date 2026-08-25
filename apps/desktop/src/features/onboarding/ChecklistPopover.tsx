// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * The Getting-started flyout (spec 056, US2 T020; FR-011).
 *
 * A progress-ring trigger in the sidebar opens the {@link ChecklistSection} as
 * a NON-MODAL popover to the RIGHT of the sidebar: no backdrop, no focus trap,
 * no `aria-modal` — the rest of the app stays fully interactive while it is
 * open. Reuses `checklist.css` (research R10).
 *
 * This is now the ONLY presentation, in both sidebar widths — `labelled`
 * only controls whether the trigger shows its text and count alongside the
 * ring, or the bare ring when the sidebar is icon-collapsed.
 *
 * WHY NOT INLINE: the section used to render directly into the expanded
 * sidebar with nothing but a hairline `border-top`, on the sidebar's own
 * background. Expanding it read as "more sidebar" rather than as a distinct
 * panel. Floating it onto the raised popover surface (border + shadow, off the
 * sidebar's background) makes it legible as its own thing by construction,
 * and keeps the sidebar's vertical space for navigation.
 */

import { useId, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { clsx } from 'clsx';
import { m } from '@/lib/i18n';
import { useDismiss } from '@/hooks/useDismiss';
import { ChecklistSection } from './ChecklistSection';
import { useVisibleOnboardingState } from './store';
import { useCompletionChoreography } from './choreography';

/** Gap between the sidebar trigger and the portalled flyout, in px. */
const FLYOUT_GAP = 4;

const RING_SIZE = 24;
const RING_STROKE = 3;
const RING_RADIUS = (RING_SIZE - RING_STROKE) / 2;
const RING_CIRCUMFERENCE = 2 * Math.PI * RING_RADIUS;

export interface ChecklistPopoverProps {
  /** Show the trigger's text + count (expanded sidebar). Bare ring when false. */
  labelled?: boolean;
}

export function ChecklistPopover({
  labelled = false,
}: ChecklistPopoverProps = {}): React.ReactElement | null {
  const state = useVisibleOnboardingState();
  const choreo = useCompletionChoreography(state);
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);
  const gradientId = `onb-ring-grad-${useId().replaceAll(':', '')}`;
  const [anchorRect, setAnchorRect] = useState<DOMRect | null>(null);
  const panelRef = useRef<HTMLDivElement>(null);

  // The flyout is PORTALLED to <body> and positioned from the trigger's rect.
  //
  // `.pv-sidebar` sets `overflow: hidden`, so an absolutely-positioned panel
  // sitting beside the sidebar is scissored away entirely — it is in the DOM,
  // `aria-expanded` is true, and nothing is visible. (This also silently broke
  // the icon-collapsed popover, which has always lived in the same clipped
  // container.) Portalling escapes the clip without having to relax the
  // sidebar's overflow, which is what keeps long nav labels from spilling.
  useLayoutEffect(() => {
    if (!open) return undefined;
    const measure = () => {
      const el = wrapRef.current;
      if (el) setAnchorRect(el.getBoundingClientRect());
    };
    measure();
    window.addEventListener('resize', measure);
    return () => window.removeEventListener('resize', measure);
  }, [open]);

  // WCAG 1.4.13 dismissable; also the plain expectation for any popover. Escape
  // restores focus to the trigger — otherwise focus would be stranded on a
  // removed subtree. The panel is portalled, so it is NOT inside wrapRef; both
  // refs are listed or every click inside the flyout would dismiss it.
  useDismiss(
    [wrapRef, panelRef],
    (e) => {
      if (e instanceof KeyboardEvent) {
        e.stopPropagation();
        setOpen(false);
        wrapRef.current?.querySelector('button')?.focus();
        return;
      }
      setOpen(false);
    },
    open,
  );

  if (!state) return null;

  const { done, total } = state.progress;
  const fraction = total > 0 ? done / total : 0;
  const progressText = m.onboarding_section_progress({ done, total });

  return (
    <div className="pv-onb-ring-wrap" ref={wrapRef}>
      <button
        type="button"
        className={clsx(
          'pv-onb-ring',
          labelled && 'pv-onb-ring--labelled',
          choreo.pulseActive && 'pv-onb-ring--pulse',
        )}
        data-testid="onb-ring"
        aria-label={progressText}
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        <span
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={total}
          aria-valuenow={done}
          aria-label={progressText}
        >
          <svg width={RING_SIZE} height={RING_SIZE} aria-hidden>
            {/*
              Progress gradient: the donut's stroke runs from the deep accent
              into the bright accent as completion advances, so the arc reads as
              "warming up" rather than as a flat band. The id is instance-scoped
              because the expanded and icon-collapsed hosts can both be mounted
              in the same document, and duplicate SVG gradient ids would make
              one silently adopt the other's stops.
            */}
            <defs>
              <linearGradient
                id={gradientId}
                x1="0%"
                y1="100%"
                x2="100%"
                y2="0%"
              >
                <stop offset="0%" className="pv-onb-ring__grad-from" />
                <stop offset="100%" className="pv-onb-ring__grad-to" />
              </linearGradient>
            </defs>
            <circle
              className="pv-onb-ring__track"
              cx={RING_SIZE / 2}
              cy={RING_SIZE / 2}
              r={RING_RADIUS}
              fill="none"
              strokeWidth={RING_STROKE}
            />
            <circle
              className="pv-onb-ring__fill"
              cx={RING_SIZE / 2}
              cy={RING_SIZE / 2}
              r={RING_RADIUS}
              fill="none"
              stroke={`url(#${gradientId})`}
              strokeWidth={RING_STROKE}
              strokeLinecap="round"
              strokeDasharray={RING_CIRCUMFERENCE}
              strokeDashoffset={RING_CIRCUMFERENCE * (1 - fraction)}
              transform={`rotate(-90 ${RING_SIZE / 2} ${RING_SIZE / 2})`}
            />
          </svg>
        </span>
        {labelled && (
          <>
            <span className="pv-onb-ring__label">
              {m.onboarding_section_title()}
            </span>
            <span className="pv-onb-ring__count" aria-hidden>
              {done}/{total}
            </span>
          </>
        )}
      </button>
      {open &&
        anchorRect &&
        createPortal(
          <div
            ref={panelRef}
            className="pv-onb-popover"
            data-testid="onb-popover"
            role="region"
            aria-label={progressText}
            // eslint-disable-next-line no-restricted-syntax -- dynamic: the panel is portalled out of the clipping sidebar, so its position must be computed from the trigger's measured rect at open time; no static class can express it.
            style={{
              left: anchorRect.right + FLYOUT_GAP,
              // Bottom-aligned to the trigger via `bottom`, NOT `top` minus a
              // measured height: on first render the panel is not yet mounted,
              // so its offsetHeight is 0 and a top-based calculation drops the
              // panel off the bottom of the window. Anchoring the bottom edge
              // needs no measurement at all.
              bottom: window.innerHeight - anchorRect.bottom,
              // Never grow past the top of the window (still capped at 60vh by
              // the stylesheet, whichever is smaller).
              maxHeight: anchorRect.bottom - FLYOUT_GAP * 2,
            }}
          >
            <ChecklistSection idPrefix="onb-pop" />
          </div>,
          document.body,
        )}
    </div>
  );
}
