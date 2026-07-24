// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * ListPageLayout — standard list-page scaffold (spec 043, tasks #62/#73/#86/#89/#104).
 *
 * The shared layout system generalized from the Sessions page. Composition:
 *
 *   ┌─────────────────────────────────────────────┐
 *   │ PageTopBar (pinned, never scrolls)           │  ← topBar
 *   ├──────────────────────────────────────────────┤
 *   │ primary content (FULL WIDTH)                 │  ← children
 *   │ table / list, scrolls                        │
 *   ├──────────────────────────────────────────────┤  ← top-border separator
 *   │ detail panel (full width, docked BELOW)      │  ← detail
 *   │ AUTO-SIZES to content; scrolls if very tall  │
 *   └──────────────────────────────────────────────┘
 *
 * "Standardize on the Sessions layout" means a consistent PINNED TOP BAR +
 * consistent button styling/placement + a consistent body shape. The detail
 * opens as a horizontal BOTTOM SPLIT below the primary content — NOT a right
 * side-panel. Rationale (task #86): a side panel reflows the full-width table
 * (columns shift as the detail mounts/unmounts); docking the detail BELOW keeps
 * the table full-width and column layout stable. The bottom panel has ample
 * horizontal room, so its rail/cards read side-by-side cleanly (we do NOT force
 * the single-column rail-collapse the old narrow side-panel needed).
 *
 * The bottom panel is NON-resizable (task #89, revising #86): it AUTO-SIZES to
 * its content (`height: fit-content`) with a `max-height` cap so large content
 * scrolls WITHIN the panel rather than pushing the table off-screen. There is no
 * splitter, no drag, and no persisted height. A close (✕) affordance hides the
 * detail and returns the table to full height. The panel mounts only when
 * `detail` is non-null (so we never show an empty centered dashboard).
 *
 * `detailPlacement` (DEFAULT `'adaptive'`, spec 054 / #936) chooses the dock.
 * `'adaptive'` docks to the SIDE when the window is wide enough and falls
 * back to the BOTTOM strip when narrow — see `useAdaptiveDock` (src/ui). The
 * threshold, per-page pin, and side width are keyed by `dockId` (defaults to
 * `detailLabel`) and persisted across restarts; drag the `ResizeHandle` on
 * the side panel's left edge to resize (bounded, see `useAdaptiveDock`).
 * `'bottom'` pins the STATIC bottom dock (the original Sessions/Calibration
 * reference: a horizontal split BELOW the full-width primary content — NOT a
 * right side-panel; rationale in task #86 below). `'side'` pins a STATIC
 * full-height RIGHT side panel (fixed ~420px width, own scroll, keeps the
 * close ✕) BESIDE the full-width primary content — suited to detail that
 * reads as a tall narrow column (Projects). The side variant simply switches
 * `.pv-listpage__body` from a column to a row and pins the detail width; the
 * primary content stays full-width to the left of it.
 *
 * `'side-and-bottom'` (task #104) — dual variant: renders BOTH a right side
 * panel AND a bottom strip simultaneously. Uses the additive `sideDetail` and
 * `bottomDetail` props. The existing `detail` prop is used as the side panel
 * content in this mode; `bottomDetail` provides the bottom strip. Callers that
 * already pass `detailPlacement="side"` with a `detail` prop are unaffected.
 *
 * Pass the top bar either as a ready `topBar` node (e.g. `<PageTopBar .../>`)
 * or via the convenience `topBarProps` slots, which this component forwards to
 * an internal `PageTopBar`. ListPageLayout is itself the `.pv-page` root, so
 * it must be the page's outermost element (do not nest it inside PageShell).
 */

import { type CSSProperties, type ReactNode, useEffect } from 'react';
import { PageTopBar, type PageTopBarProps } from './PageTopBar';
import { DetailDockPlacementControl } from './DetailDockPlacementControl';
import { m } from '@/lib/i18n';
import { useAdaptiveDock, ResizeHandle } from '@/ui';
import * as lpl from './ListPageLayout.css';
import { page } from '@/ui/page-layout.css';

export interface ListPageLayoutProps {
  /** A ready top-bar node. Mutually exclusive with `topBarProps`. */
  topBar?: ReactNode;
  /** Convenience: build the PageTopBar from slot props instead of a node. */
  topBarProps?: PageTopBarProps;
  /** Primary full-width content (table / list). */
  children: ReactNode;
  /**
   * Detail content. For `'bottom'` and `'side'` placements this is the single
   * detail panel. For `'side-and-bottom'` this becomes the SIDE panel content;
   * pair it with `bottomDetail` for the bottom strip.
   * The panel is shown only when this is non-null.
   */
  detail?: ReactNode;
  /** Invoked when the panel's close affordance is used. Omit to hide it. */
  onCloseDetail?: () => void;
  /** Accessible label for the detail panel region. Default "Details". */
  detailLabel?: string;
  /**
   * Where the detail panel docks. DEFAULT `'adaptive'` (spec 054 / #936): side
   * when the window is wide enough, bottom when narrow — see the module
   * header. `'bottom'` / `'side'` pin the STATIC legacy placements. `'side-
   * and-bottom'` (task #104) renders BOTH a right side panel (from `detail`)
   * AND a bottom strip (from `bottomDetail`) simultaneously.
   */
  detailPlacement?: 'adaptive' | 'bottom' | 'side' | 'side-and-bottom';
  /**
   * Persistence scope for the adaptive dock's pinned placement + side width
   * (`'adaptive'` only). Defaults to `detailLabel`. Pass a stable per-page id
   * when `detailLabel` is localized/dynamic.
   */
  dockId?: string;
  /** Window width (px) at/above which the adaptive dock engages the side
   * placement. `'adaptive'` only. Default 1400 — see `useAdaptiveDock`. */
  adaptiveThreshold?: number;
  /**
   * Bottom strip content for the `'side-and-bottom'` dual layout (task #104).
   * Rendered only when `detailPlacement="side-and-bottom"`. Ignored for other
   * placements. The strip is shown only when this prop is non-null.
   */
  bottomDetail?: ReactNode;
  /** Invoked when the bottom strip's close affordance is used. Omit to hide it. */
  onCloseBottomDetail?: () => void;
  /** Accessible label for the bottom strip region. Default "Session details". */
  bottomDetailLabel?: string;
}

/**
 * True when a Base UI overlay (Dialog, Select, Combobox, Menu, …) is open
 * anywhere in the document. Base UI stamps the OPEN popup's outer node
 * (Positioner/Popup) with `data-open` (its documented styling-state hook),
 * but the ARIA overlay role doesn't always land on that same node: Dialog and
 * Menu put `role` and `data-open` together on Popup, while Select/Combobox
 * split them — `role="listbox"`/`"grid"` sits on an inner List that has no
 * `data-open` of its own, only a `data-open` ANCESTOR (Positioner/Popup).
 * Walking up from every overlay-role element to the nearest `data-open`
 * ancestor (`closest`, which also matches the element itself) covers both
 * shapes without hardcoding which library variant is in use (#906).
 */
function hasOpenOverlay(): boolean {
  const overlayRoles = document.querySelectorAll(
    '[role="dialog"], [role="alertdialog"], [role="listbox"], [role="grid"], [role="menu"]',
  );
  for (const el of overlayRoles) {
    if (el.closest('[data-open]')) return true;
  }
  return false;
}

export function ListPageLayout({
  topBar,
  topBarProps,
  children,
  detail,
  onCloseDetail,
  detailLabel = m.common_details(),
  detailPlacement = 'adaptive',
  dockId,
  adaptiveThreshold,
  bottomDetail,
  onCloseBottomDetail,
  bottomDetailLabel = m.list_page_layout_bottom_detail_label(),
}: ListPageLayoutProps) {
  const hasDetail = detail != null;
  const hasBottom = bottomDetail != null;

  // Always called (rules-of-hooks) — a no-op width/localStorage cost when the
  // page isn't in adaptive mode. dockId falls back to detailLabel so pages
  // that don't pass one still get a stable-enough persistence key.
  const adaptiveDock = useAdaptiveDock({
    dockId: dockId ?? detailLabel,
    threshold: adaptiveThreshold,
  });
  const resolvedPlacement =
    detailPlacement === 'adaptive' ? adaptiveDock.placement : detailPlacement;

  // Escape closes the open detail panel(s), matching the ✕ affordance (#771).
  // A `document`-level listener also catches the common case where nothing
  // inside the panel has focus (e.g. focus stayed on the row that opened it,
  // or on <body>). `stopPropagation()` does NOT stop a sibling listener
  // registered on the SAME target — Base UI's own Escape dismissal
  // (`useDismiss`) is also a `document`-level `keydown` listener, and by
  // default (`escapeKeyBubbles: false`) it calls `stopPropagation()` but
  // never `preventDefault()`, so neither mechanism reaches across to block
  // this listener. We therefore check explicitly for an open Base UI overlay
  // (Dialog/Select/Combobox/Menu — anything carrying Base UI's `data-open`
  // styling-hook attribute plus an overlay ARIA role) and skip closing the
  // panel while one is open, deferring to its own dismissal.
  useEffect(() => {
    if (!hasDetail && !hasBottom) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || event.defaultPrevented) return;
      if (hasOpenOverlay()) return;
      if (hasDetail) onCloseDetail?.();
      if (detailPlacement === 'side-and-bottom' && hasBottom) {
        onCloseBottomDetail?.();
      }
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [
    hasDetail,
    hasBottom,
    detailPlacement,
    onCloseDetail,
    onCloseBottomDetail,
  ]);

  // ── Dual side-and-bottom layout (task #104) ──────────────────────────────
  //
  // Structure: the bottom strip sits ONLY under the main content column, NOT
  // under the side panel. A wrapper column (.pv-listpage__main-col) groups
  // main + bottom so the side panel is flush to full height on the right.
  //
  //   .pv-listpage__body--dual (row)
  //     .pv-listpage__main-col (column, flex:1)
  //       .pv-listpage__main   (flex:1, scrolls)
  //       .pv-listpage__bottom (fit-content, max-height cap)
  //     .pv-listpage__side     (fixed 420px, own scroll)
  //
  if (detailPlacement === 'side-and-bottom') {
    return (
      <div className={page}>
        {topBar ?? (topBarProps && <PageTopBar {...topBarProps} />)}

        <div className={`${lpl.body} ${lpl.bodyDual}`} data-testid="listpage-body-dual">
          {/* Left column: main table + bottom strip stacked */}
          <div className={lpl.mainCol}>
            <div className={lpl.main}>{children}</div>

            {/* Bottom strip constrained to the content column */}
            {hasBottom && (
              <section
                className={lpl.bottomPanel}
                data-testid="listpage-bottom"
                role="complementary"
                aria-label={bottomDetailLabel}
              >
                {onCloseBottomDetail && (
                  <div className={lpl.panelBar}>
                    <button
                      type="button"
                      className={lpl.panelClose}
                      onClick={onCloseBottomDetail}
                      aria-label={m.cmp_listpage_close_session_details_aria()}
                    >
                      ✕
                    </button>
                  </div>
                )}
                <div className={lpl.panelBody}>{bottomDetail}</div>
              </section>
            )}
          </div>

          {/* Right: side detail panel, full height of the body */}
          {hasDetail && (
            <section
              className={lpl.sidePanel}
              data-testid="listpage-side"
              role="complementary"
              aria-label={detailLabel}
            >
              {onCloseDetail && (
                <div className={lpl.panelBar}>
                  <button
                    type="button"
                    className={lpl.panelClose}
                    onClick={onCloseDetail}
                    aria-label={m.inbox_close_details_aria()}
                  >
                    ✕
                  </button>
                </div>
              )}
              <div className={lpl.panelBody}>{detail}</div>
            </section>
          )}
        </div>
      </div>
    );
  }

  // ── Bottom / side / adaptive layouts ─────────────────────────────────────
  const isAdaptive = detailPlacement === 'adaptive';
  const isSide = resolvedPlacement === 'side';
  const bodyClass = isSide ? `${lpl.body} ${lpl.bodySide}` : lpl.body;
  const detailClass = isSide
    ? `${lpl.detail} ${lpl.detailSide} ${lpl.detailSidePanel}`
    : lpl.detail;
  // Adaptive side width is drag-resized (useAdaptiveDock); static 'side'
  // keeps the fixed --pv-side-detail-w CSS default (undefined = no override).
  const detailStyle =
    isAdaptive && isSide
      ? ({ '--pv-side-detail-w': `${adaptiveDock.width}px` } as CSSProperties)
      : undefined;

  return (
    <div className={page}>
      {topBar ?? (topBarProps && <PageTopBar {...topBarProps} />)}

      <div className={bodyClass} data-testid={isSide ? 'listpage-body-side' : 'listpage-body'}>
        <div className={lpl.main}>{children}</div>

        {hasDetail && (
          <section
            className={detailClass}
            data-testid={isSide ? 'listpage-detail-side' : 'listpage-detail'}
            // eslint-disable-next-line no-restricted-syntax -- dynamic: user-resizable side-panel width persisted per dockId, not a static design token
            style={detailStyle}
            role="complementary"
            aria-label={detailLabel}
          >
            {isAdaptive && isSide && (
              <ResizeHandle
                onPointerDown={adaptiveDock.onResizeStart}
                label={m.list_page_layout_dock_resize_aria()}
              />
            )}
            {(onCloseDetail || isAdaptive) && (
              <div className={lpl.detailBar}>
                {isAdaptive && (
                  // #1066: a three-state Auto/Bottom/Right control, not the
                  // old two-state pin button — `override === null` ("Auto") is
                  // a reachable choice again, so pinning is no longer a
                  // one-way door out of adaptive placement.
                  <DetailDockPlacementControl
                    override={adaptiveDock.override}
                    onChange={adaptiveDock.setOverride}
                  />
                )}
                {onCloseDetail && (
                  <button
                    type="button"
                    className={lpl.detailClose}
                    onClick={onCloseDetail}
                    aria-label={m.inbox_close_details_aria()}
                  >
                    ✕
                  </button>
                )}
              </div>
            )}
            <div className={lpl.detailBody}>{detail}</div>
          </section>
        )}
      </div>
    </div>
  );
}
