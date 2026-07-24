// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * ListPageLayout tests — spec 043 tasks #94 / #104.
 *
 * Covers the `detailPlacement` prop: the DEFAULT `'bottom'` dock keeps the
 * existing classes (Sessions/Calibration/Targets are unchanged), while `'side'`
 * switches the body + detail to the right-side-panel variant classes. Both keep
 * role=complementary, the detailLabel, and the close ✕ affordance.
 *
 * Task #104 adds coverage for `detailPlacement="side-and-bottom"`: the dual
 * layout renders both a side panel (from `detail`) and a bottom strip (from
 * `bottomDetail`), each with their own aria label and close affordance.
 */

import {
  render,
  screen,
  fireEvent,
  act,
  renderHook,
} from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ListPageLayout } from './ListPageLayout';
import { Modal } from './Modal';
import { Combobox } from '@base-ui-components/react/combobox';
import { resetPreferences } from '@/data/preferences';
import { useAdaptiveDock } from '@/ui/useAdaptiveDock';

/** Simulate a browser window resize for the adaptive-dock hook (jsdom
 * doesn't resize on its own — `window.innerWidth` must be forced). */
function resizeWindowTo(width: number) {
  Object.defineProperty(window, 'innerWidth', {
    writable: true,
    configurable: true,
    value: width,
  });
  act(() => {
    window.dispatchEvent(new Event('resize'));
  });
}

describe('ListPageLayout', () => {
  it('defaults to the bottom dock (no side variant classes)', () => {
    const { container } = render(
      <ListPageLayout topBar={<div>bar</div>} detail={<div>detail</div>}>
        <div>main</div>
      </ListPageLayout>,
    );
    const body = screen.getByTestId('listpage-body');
    expect(body).toBeInTheDocument();
    expect(body).not.toHaveAttribute('data-testid', 'listpage-body-side');
    const detail = screen.queryByTestId('listpage-detail');
    expect(detail).toBeInTheDocument();
    expect(detail).not.toHaveAttribute('data-testid', 'listpage-detail-side');
  });

  it('does not render the detail panel when detail is null', () => {
    const { container } = render(
      <ListPageLayout topBar={<div>bar</div>}>
        <div>main</div>
      </ListPageLayout>,
    );
    expect(screen.queryByTestId('listpage-detail')).toBeNull();
  });

  it('applies the side variant classes when detailPlacement="side"', () => {
    const { container } = render(
      <ListPageLayout
        topBar={<div>bar</div>}
        detail={<div>detail</div>}
        detailPlacement="side"
        detailLabel="Project details"
      >
        <div>main</div>
      </ListPageLayout>,
    );
    expect(screen.getByTestId('listpage-body-side')).toBeInTheDocument();
    expect(screen.queryByTestId('listpage-detail-side')).toBeInTheDocument();
  });

  it('keeps role=complementary + detailLabel + close affordance in the side variant', () => {
    const onClose = vi.fn();
    render(
      <ListPageLayout
        topBar={<div>bar</div>}
        detail={<div>detail</div>}
        detailPlacement="side"
        detailLabel="Project details"
        onCloseDetail={onClose}
      >
        <div>main</div>
      </ListPageLayout>,
    );
    const region = screen.getByRole('complementary', {
      name: 'Project details',
    });
    expect(region).toBeInTheDocument();
    const close = screen.getByRole('button', { name: 'Close details' });
    fireEvent.click(close);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  // ── Task #104: side-and-bottom dual layout ───────────────────────────────

  it('(#104) dual: renders side panel from detail and bottom strip from bottomDetail', () => {
    const { container } = render(
      <ListPageLayout
        topBar={<div>bar</div>}
        detail={<div>side content</div>}
        bottomDetail={<div>bottom content</div>}
        detailPlacement="side-and-bottom"
        detailLabel="Project details"
        bottomDetailLabel="Selected item details"
      >
        <div>main</div>
      </ListPageLayout>,
    );
    // Both regions are present.
    expect(
      screen.getByRole('complementary', { name: 'Project details' }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('complementary', { name: 'Selected item details' }),
    ).toBeInTheDocument();
    // Body carries the dual modifier class.
    expect(
      screen.queryByTestId('listpage-body-dual'),
    ).toBeInTheDocument();
    // Side panel and bottom strip use their own classes (not the old detail classes).
    expect(screen.queryByTestId('listpage-side')).toBeInTheDocument();
    expect(screen.queryByTestId('listpage-bottom')).toBeInTheDocument();
    expect(screen.getByText('side content')).toBeInTheDocument();
    expect(screen.getByText('bottom content')).toBeInTheDocument();
  });

  it('(#104) dual: does not render side panel when detail is null', () => {
    const { container } = render(
      <ListPageLayout
        topBar={<div>bar</div>}
        bottomDetail={<div>bottom content</div>}
        detailPlacement="side-and-bottom"
      >
        <div>main</div>
      </ListPageLayout>,
    );
    expect(screen.queryByTestId('listpage-side')).toBeNull();
    expect(screen.queryByTestId('listpage-bottom')).toBeInTheDocument();
  });

  it('(#104) dual: does not render bottom strip when bottomDetail is null', () => {
    const { container } = render(
      <ListPageLayout
        topBar={<div>bar</div>}
        detail={<div>side content</div>}
        detailPlacement="side-and-bottom"
        detailLabel="Project details"
      >
        <div>main</div>
      </ListPageLayout>,
    );
    expect(screen.queryByTestId('listpage-side')).toBeInTheDocument();
    expect(screen.queryByTestId('listpage-bottom')).toBeNull();
  });

  it('(#104) dual: close affordances call their respective callbacks', () => {
    const onCloseSide = vi.fn();
    const onCloseBottom = vi.fn();
    render(
      <ListPageLayout
        topBar={<div>bar</div>}
        detail={<div>side content</div>}
        bottomDetail={<div>bottom content</div>}
        detailPlacement="side-and-bottom"
        detailLabel="Project details"
        bottomDetailLabel="Selected item details"
        onCloseDetail={onCloseSide}
        onCloseBottomDetail={onCloseBottom}
      >
        <div>main</div>
      </ListPageLayout>,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Close details' }));
    expect(onCloseSide).toHaveBeenCalledTimes(1);
    fireEvent.click(
      screen.getByRole('button', { name: 'Close session details' }),
    );
    expect(onCloseBottom).toHaveBeenCalledTimes(1);
  });

  it('(#104) dual: backward-compatible — existing bottom/side paths unaffected', () => {
    // Sessions/Calibration/Targets use detailPlacement="bottom" (default) —
    // ensure they still get the original class names with no dual classes.
    const { container } = render(
      <ListPageLayout topBar={<div>bar</div>} detail={<div>detail</div>}>
        <div>main</div>
      </ListPageLayout>,
    );
    expect(screen.queryByTestId('listpage-body-dual')).toBeNull();
    expect(screen.queryByTestId('listpage-side')).toBeNull();
    expect(screen.queryByTestId('listpage-bottom')).toBeNull();
    // Original detail class still present.
    expect(screen.queryByTestId('listpage-detail')).toBeInTheDocument();
  });

  // ── Escape-to-close (#771) ────────────────────────────────────────────────

  it('closes the bottom-dock detail on Escape, even with focus on <body>', () => {
    const onClose = vi.fn();
    render(
      <ListPageLayout
        topBar={<div>bar</div>}
        detail={<div>detail</div>}
        onCloseDetail={onClose}
      >
        <div>main</div>
      </ListPageLayout>,
    );
    expect(document.activeElement).toBe(document.body);
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('closes the side-dock detail on Escape when focus is inside the panel', () => {
    const onClose = vi.fn();
    render(
      <ListPageLayout
        topBar={<div>bar</div>}
        detail={<button type="button">focus me</button>}
        detailPlacement="side"
        onCloseDetail={onClose}
      >
        <div>main</div>
      </ListPageLayout>,
    );
    screen.getByText('focus me').focus();
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('does not close the panel on Escape while a Base UI Modal is open above it (#906)', () => {
    const onCloseDetail = vi.fn();
    const onCloseModal = vi.fn();
    render(
      <>
        <ListPageLayout
          topBar={<div>bar</div>}
          detail={<div>detail</div>}
          onCloseDetail={onCloseDetail}
        >
          <div>main</div>
        </ListPageLayout>
        <Modal open onClose={onCloseModal} title="Overlay">
          <div>modal body</div>
        </Modal>
      </>,
    );
    // Base UI stamps the open popup with `data-open` + role="dialog" — the
    // same signal our overlay guard checks for.
    expect(
      document.querySelector('[data-open][role="dialog"]'),
    ).toBeInTheDocument();
    fireEvent.keyDown(document, { key: 'Escape' });
    // The panel must NOT also close — only the modal's own dismissal should
    // react to this Escape (asserted separately from Base UI's own listener,
    // which isn't under test here; the regression is that our listener used
    // to co-fire regardless).
    expect(onCloseDetail).not.toHaveBeenCalled();
  });

  it('does not close the panel on Escape while a Base UI Combobox is open above it (#906)', () => {
    const onCloseDetail = vi.fn();
    render(
      <>
        <ListPageLayout
          topBar={<div>bar</div>}
          detail={<div>detail</div>}
          onCloseDetail={onCloseDetail}
        >
          <div>main</div>
        </ListPageLayout>
        <Combobox.Root items={['Alpha', 'Beta']} open>
          <Combobox.Input aria-label="Search" />
          <Combobox.Portal keepMounted>
            <Combobox.Positioner>
              <Combobox.Popup>
                <Combobox.List>
                  {(item: string) => (
                    <Combobox.Item key={item} value={item}>
                      {item}
                    </Combobox.Item>
                  )}
                </Combobox.List>
              </Combobox.Popup>
            </Combobox.Positioner>
          </Combobox.Portal>
        </Combobox.Root>
      </>,
    );
    // Combobox splits the signal across two nodes: `role="listbox"` sits on
    // the List, `data-open` on its Positioner/Popup ancestor — neither node
    // carries both, which is exactly what our overlay guard must handle.
    const listbox = document.querySelector('[role="listbox"]');
    expect(listbox).toBeInTheDocument();
    expect(listbox).not.toHaveAttribute('data-open');
    expect(listbox?.closest('[data-open]')).not.toBeNull();
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onCloseDetail).not.toHaveBeenCalled();
  });

  it('does not call onCloseDetail on Escape when no detail is open', () => {
    const onClose = vi.fn();
    render(
      <ListPageLayout topBar={<div>bar</div>} onCloseDetail={onClose}>
        <div>main</div>
      </ListPageLayout>,
    );
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).not.toHaveBeenCalled();
  });

  it('does not close on Escape when an inner handler already consumed it', () => {
    const onClose = vi.fn();
    render(
      <ListPageLayout
        topBar={<div>bar</div>}
        detail={
          <input
            aria-label="inner"
            onKeyDown={(e) => {
              if (e.key === 'Escape') e.preventDefault();
            }}
          />
        }
        onCloseDetail={onClose}
      >
        <div>main</div>
      </ListPageLayout>,
    );
    fireEvent.keyDown(screen.getByLabelText('inner'), { key: 'Escape' });
    expect(onClose).not.toHaveBeenCalled();
  });

  it('ignores non-Escape keys', () => {
    const onClose = vi.fn();
    render(
      <ListPageLayout
        topBar={<div>bar</div>}
        detail={<div>detail</div>}
        onCloseDetail={onClose}
      >
        <div>main</div>
      </ListPageLayout>,
    );
    fireEvent.keyDown(document, { key: 'Enter' });
    expect(onClose).not.toHaveBeenCalled();
  });

  // ── Adaptive dock (spec 054 / #936) ──────────────────────────────────────

  describe('adaptive dock', () => {
    const originalInnerWidth = window.innerWidth;

    beforeEach(() => {
      // Clears the in-memory preference cache too — plain localStorage.clear()
      // would leave the module cache holding the previous test's dock pins.
      resetPreferences();
    });

    afterEach(() => {
      resizeWindowTo(originalInnerWidth);
    });

    it('docks to the bottom under the threshold and to the side at/above it', () => {
      resizeWindowTo(1024);
      const { container, rerender } = render(
        <ListPageLayout
          topBar={<div>bar</div>}
          detail={<div>detail</div>}
          dockId="adaptive-test-a"
        >
          <div>main</div>
        </ListPageLayout>,
      );
      // narrow: bottom dock (no side testid).
      expect(screen.queryByTestId('listpage-detail-side')).toBeNull();

      resizeWindowTo(1600);
      rerender(
        <ListPageLayout
          topBar={<div>bar</div>}
          detail={<div>detail</div>}
          dockId="adaptive-test-a"
        >
          <div>main</div>
        </ListPageLayout>,
      );
      expect(screen.queryByTestId('listpage-detail-side')).toBeInTheDocument();
    });

    it('honors adaptiveThreshold overrides per page', () => {
      resizeWindowTo(1450);
      render(
        <ListPageLayout
          topBar={<div>bar</div>}
          detail={<div>detail</div>}
          dockId="adaptive-test-threshold"
          adaptiveThreshold={1500}
        >
          <div>main</div>
        </ListPageLayout>,
      );
      // 1450 < 1500 → still bottom, even though it clears the generic default.
      expect(screen.queryByTestId('listpage-detail-side')).toBeNull();
    });

    it('pinning to side persists across remount (dockId-scoped preference)', () => {
      resizeWindowTo(1024);
      const { container, unmount } = render(
        <ListPageLayout
          topBar={<div>bar</div>}
          detail={<div>detail</div>}
          dockId="adaptive-test-pin"
        >
          <div>main</div>
        </ListPageLayout>,
      );
      fireEvent.click(screen.getByRole('radio', { name: 'Right' }));
      expect(screen.queryByTestId('listpage-detail-side')).toBeInTheDocument();
      unmount();

      const { container: container2 } = render(
        <ListPageLayout
          topBar={<div>bar</div>}
          detail={<div>detail</div>}
          dockId="adaptive-test-pin"
        >
          <div>main</div>
        </ListPageLayout>,
      );
      // Narrow window (1024), but the pin from the previous mount survives.
      expect(screen.queryByTestId('listpage-detail-side')).toBeInTheDocument();
    });

    // #1066: the placement model is three-state (side / bottom / null=auto),
    // but the UI that preceded this was a two-state toggle that only ever set
    // a concrete pin — so Auto was unreachable once touched, and the adaptive
    // width rule was permanently dead for that dockId.
    it('returning to Auto clears the pin and resumes the width rule', () => {
      resizeWindowTo(1024);
      const { container, unmount } = render(
        <ListPageLayout
          topBar={<div>bar</div>}
          detail={<div>detail</div>}
          dockId="adaptive-test-unpin"
        >
          <div>main</div>
        </ListPageLayout>,
      );
      // Pin to Right on a narrow window — placement defies the width rule.
      fireEvent.click(screen.getByRole('radio', { name: 'Right' }));
      expect(screen.queryByTestId('listpage-detail-side')).toBeInTheDocument();

      // Back to Auto: 1024 is below the threshold, so it must fall to bottom.
      fireEvent.click(screen.getByRole('radio', { name: 'Auto' }));
      expect(screen.queryByTestId('listpage-detail-side')).toBeNull();
      expect(screen.getByRole('radio', { name: 'Auto' })).toBeChecked();

      // And Auto must survive a remount — i.e. the persisted pin was actually
      // cleared, not just overridden in component state.
      unmount();
      const { container: container2 } = render(
        <ListPageLayout
          topBar={<div>bar</div>}
          detail={<div>detail</div>}
          dockId="adaptive-test-unpin"
        >
          <div>main</div>
        </ListPageLayout>,
      );
      expect(screen.queryByTestId('listpage-detail-side')).toBeNull();

      // Auto still follows the width rule upward, not just downward.
      resizeWindowTo(1600);
      expect(screen.queryByTestId('listpage-detail-side')).toBeInTheDocument();
    });

    it('renders a resize handle only in the side placement', () => {
      resizeWindowTo(1600);
      const { rerender, queryByTestId } = render(
        <ListPageLayout
          topBar={<div>bar</div>}
          detail={<div>detail</div>}
          dockId="adaptive-test-handle"
        >
          <div>main</div>
        </ListPageLayout>,
      );
      expect(queryByTestId('dock-resize-handle')).toBeInTheDocument();

      resizeWindowTo(1024);
      rerender(
        <ListPageLayout
          topBar={<div>bar</div>}
          detail={<div>detail</div>}
          dockId="adaptive-test-handle"
        >
          <div>main</div>
        </ListPageLayout>,
      );
      expect(queryByTestId('dock-resize-handle')).not.toBeInTheDocument();
    });

    it('keeps role=complementary + close affordance working in adaptive-side mode', () => {
      resizeWindowTo(1600);
      const onClose = vi.fn();
      render(
        <ListPageLayout
          topBar={<div>bar</div>}
          detail={<div>detail</div>}
          dockId="adaptive-test-close"
          detailLabel="Adaptive details"
          onCloseDetail={onClose}
        >
          <div>main</div>
        </ListPageLayout>,
      );
      const region = screen.getByRole('complementary', {
        name: 'Adaptive details',
      });
      expect(region).toBeInTheDocument();
      fireEvent.click(screen.getByRole('button', { name: 'Close details' }));
      expect(onClose).toHaveBeenCalledTimes(1);
    });
  });

  // ── useAdaptiveDock, directly (#1158) ────────────────────────────────────
  //
  // The hook drives all placement but was only ever covered indirectly, through
  // this layout. That is how #1066 slipped through: `setOverride(null)` — the
  // "return to Auto" path — had no call site at all, and no test caught it.
  //
  // Folded into this file rather than a new one on purpose: adding a
  // React-rendering test file previously tipped the suite into load-induced
  // timeouts in unrelated suites, so these live beside the layout tests that
  // already exercise the same hook.

  describe('useAdaptiveDock', () => {
    const originalInnerWidth = window.innerWidth;

    beforeEach(() => {
      resetPreferences();
    });

    afterEach(() => {
      resizeWindowTo(originalInnerWidth);
    });

    it('resolves placement from the threshold when unpinned', () => {
      resizeWindowTo(1024);
      const { result, rerender } = renderHook(() =>
        useAdaptiveDock({ dockId: 'hook-threshold' }),
      );
      expect(result.current.placement).toBe('bottom');
      expect(result.current.override).toBeNull();

      act(() => resizeWindowTo(1600));
      rerender();
      expect(result.current.placement).toBe('side');
    });

    it('a pin overrides the threshold, and setOverride(null) restores it (#1066)', () => {
      resizeWindowTo(1600);
      const { result } = renderHook(() =>
        useAdaptiveDock({ dockId: 'hook-override' }),
      );
      expect(result.current.placement).toBe('side');

      act(() => result.current.setOverride('bottom'));
      expect(result.current.placement).toBe('bottom');
      expect(result.current.override).toBe('bottom');

      // The regression: clearing the pin must resume the width rule, not
      // persist a value that reads back as a pin.
      act(() => result.current.setOverride(null));
      expect(result.current.override).toBeNull();
      expect(result.current.placement).toBe('side');
    });

    it('falls back to bottom when the window is too narrow for a side dock', () => {
      // sideAvailable = innerWidth >= minWidth * 2. Below that an explicit
      // 'side' pin must NOT win — the 1100x720 shell minimum stays workable.
      resizeWindowTo(600);
      const { result } = renderHook(() =>
        useAdaptiveDock({ dockId: 'hook-narrow', minWidth: 320 }),
      );
      act(() => result.current.setOverride('side'));
      expect(result.current.override).toBe('side');
      expect(result.current.placement).toBe('bottom');
    });

    it('clamps width to [minWidth, window * maxWidthFraction]', () => {
      resizeWindowTo(1600);
      const { result } = renderHook(() =>
        useAdaptiveDock({
          dockId: 'hook-clamp',
          minWidth: 320,
          maxWidthFraction: 0.5,
        }),
      );
      act(() => result.current.setWidth(50));
      expect(result.current.width).toBe(320);

      act(() => result.current.setWidth(5000));
      expect(result.current.width).toBe(800); // 1600 * 0.5

      act(() => result.current.setWidth(500));
      expect(result.current.width).toBe(500);
    });

    it('persists across remount, scoped per dockId', () => {
      resizeWindowTo(1600);
      const first = renderHook(() => useAdaptiveDock({ dockId: 'page-a' }));
      act(() => {
        first.result.current.setOverride('bottom');
        first.result.current.setWidth(500);
      });
      first.unmount();

      // Same id: restored.
      const restored = renderHook(() => useAdaptiveDock({ dockId: 'page-a' }));
      expect(restored.result.current.override).toBe('bottom');
      expect(restored.result.current.width).toBe(500);

      // Different id: untouched by page-a's pin.
      const other = renderHook(() => useAdaptiveDock({ dockId: 'page-b' }));
      expect(other.result.current.override).toBeNull();
      expect(other.result.current.placement).toBe('side');
    });
  });
});

/**
 * T023 (spec 054, #1257) — the two halves the remount tests deliberately
 * left uncovered: width set by DRAGGING the handle rather than calling
 * `setWidth`, and the value actually reaching storage rather than only the
 * in-memory cache.
 *
 * Why the second matters. `getPreferences()` returns a module-level
 * `cachedPreferences` when one exists, and `persistPreferences()` assigns
 * that cache BEFORE its `localStorage.setItem`, whose failure is swallowed
 * ("state is still in memory"). A remount therefore re-reads the CACHE, not
 * storage — so every existing remount assertion would still pass if the
 * write never landed at all. Asserting the serialized bytes is what closes
 * that gap; a real reload (the Layer-2 half of T023) is what proves the
 * cold read.
 */
describe('adaptive dock — drag + real persistence (T023)', () => {
  /** The real on-disk key from `data/preferences.ts`. Deliberately a
   * literal: it is not exported, and this test's whole point is to assert
   * the actual stored bytes rather than to re-derive them from the module
   * under test. */
  const STORAGE_KEY = 'alm-preferences';

  function storedDock(dockId: string) {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return undefined;
    return JSON.parse(raw)?.detailDock?.[dockId];
  }

  beforeEach(() => {
    resetPreferences();
    localStorage.clear();
  });

  afterEach(() => {
    resetPreferences();
    localStorage.clear();
  });

  it('a drag on the resize handle really resizes the side panel', () => {
    resizeWindowTo(1600);
    const { getByTestId, container } = render(
      <ListPageLayout
        topBar={<div>bar</div>}
        detail={<div>detail</div>}
        dockId="drag-page"
      >
        <div>main</div>
      </ListPageLayout>,
    );

    const handle = getByTestId('dock-resize-handle');
    // The side panel sits on the RIGHT edge, so dragging LEFT grows it:
    // delta = startX - currentX = 1000 - 900 = 100, on the 420px default.
    fireEvent.pointerDown(handle, { clientX: 1000 });
    fireEvent.pointerMove(window, { clientX: 900 });
    fireEvent.pointerUp(window);

    const styled = container.querySelector('[style*="--pv-side-detail-w"]');
    expect(styled?.getAttribute('style')).toContain('520px');
  });

  it('the dragged width reaches localStorage, not just the in-memory cache', () => {
    resizeWindowTo(1600);
    const { getByTestId } = render(
      <ListPageLayout
        topBar={<div>bar</div>}
        detail={<div>detail</div>}
        dockId="drag-persist"
      >
        <div>main</div>
      </ListPageLayout>,
    );

    fireEvent.pointerDown(getByTestId('dock-resize-handle'), { clientX: 1000 });
    fireEvent.pointerMove(window, { clientX: 940 });
    fireEvent.pointerUp(window);

    // Read the serialized bytes, NOT getPreferences() — going through the
    // module would consult the cache and assert nothing about persistence.
    expect(storedDock('drag-persist')?.width).toBe(480);
  });

  it('a drag is clamped to the window fraction before it is persisted', () => {
    resizeWindowTo(1600);
    const { getByTestId } = render(
      <ListPageLayout
        topBar={<div>bar</div>}
        detail={<div>detail</div>}
        dockId="drag-clamp"
      >
        <div>main</div>
      </ListPageLayout>,
    );

    // Drag far past the ceiling: maxWidthFraction 0.5 of 1600 = 800px.
    fireEvent.pointerDown(getByTestId('dock-resize-handle'), { clientX: 1000 });
    fireEvent.pointerMove(window, { clientX: 0 });
    fireEvent.pointerUp(window);

    // The CLAMPED value must be what persists — storing the raw drag target
    // would restore an over-wide dock on the next cold start.
    expect(storedDock('drag-clamp')?.width).toBe(800);
  });

  it('a pinned placement reaches localStorage too', () => {
    resizeWindowTo(1024);
    const { result } = renderHook(() =>
      useAdaptiveDock({ dockId: 'pin-persist' }),
    );
    expect(result.current.placement).toBe('bottom');

    act(() => {
      result.current.setOverride('side');
    });

    expect(storedDock('pin-persist')?.placement).toBe('side');
  });
});
