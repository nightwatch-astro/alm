// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * SiteLocationPicker tests (issue #491) — shared map picker for the
 * Observing Site step and the Settings site form. Leaflet requires a real
 * DOM+canvas, which jsdom doesn't provide, so the module is mocked at the
 * boundary with a fake L object that records calls the way the real Leaflet
 * API would be driven.
 *
 * Leaflet's LatLng normalizes longitudes and clamps latitudes silently rather
 * than throwing — the isValidLatLon guard in the component prevents us from
 * ever calling panTo with out-of-range values, which this suite regression-
 * guards against.
 */
import { render, screen, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';

// ── Mock construction ─────────────────────────────────────────────────────────
// `vi.hoisted` runs before any import, so MockMap is available in vi.mock().
const { MockMap, MockMarker, MockTileLayer } = vi.hoisted(() => {
  // Guard the same coordinate range that isValidLatLon() enforces. Leaflet
  // doesn't throw, but panTo with a lat of 200 would silently wrap — calling
  // it at all is the regression we're protecting against.
  function assertValidLatLon([lat, lon]: [number, number]): void {
    if (lat < -90 || lat > 90) {
      throw new Error(`panTo called with invalid latitude: ${lat}`);
    }
    if (lon < -180 || lon > 180) {
      throw new Error(`panTo called with invalid longitude: ${lon}`);
    }
  }

  class MockMarker {
    latLon: [number, number] | null = null;
    setLatLng(ll: [number, number]) {
      this.latLon = ll;
      return this;
    }
    addTo() {
      return this;
    }
  }

  class MockMap {
    static instances: MockMap[] = [];
    static shouldThrowOnConstruct = false;
    listeners: Record<string, Array<(...args: unknown[]) => void>> = {};
    panToCalls: Array<[number, number]> = [];
    removed = false;
    constructor(_container: unknown, _options: unknown) {
      if (MockMap.shouldThrowOnConstruct) {
        throw new Error('Leaflet map construction failed');
      }
      MockMap.instances.push(this);
    }
    on(event: string, cb: (...args: unknown[]) => void) {
      this.listeners[event] ??= [];
      this.listeners[event].push(cb);
    }
    remove() {
      this.removed = true;
    }
    panTo(ll: [number, number]) {
      assertValidLatLon(ll);
      this.panToCalls.push(ll);
    }
    emit(event: string, ...args: unknown[]) {
      for (const cb of this.listeners[event] ?? []) cb(...args);
    }
  }

  class MockTileLayer {
    static instances: MockTileLayer[] = [];
    listeners: Record<string, Array<(...args: unknown[]) => void>> = {};
    constructor() {
      MockTileLayer.instances.push(this);
    }
    on(event: string, cb: (...args: unknown[]) => void) {
      this.listeners[event] ??= [];
      this.listeners[event].push(cb);
      return this;
    }
    addTo() {
      return this;
    }
    emit(event: string, ...args: unknown[]) {
      for (const cb of this.listeners[event] ?? []) cb(...args);
    }
  }

  return { MockMap, MockMarker, MockTileLayer };
});

vi.mock('leaflet', () => {
  const marker = vi.fn(() => new MockMarker());
  const tileLayer = vi.fn(() => new MockTileLayer());
  const Icon = { Default: { mergeOptions: vi.fn() } };
  return {
    default: {
      map: vi.fn(
        (container: unknown, options: unknown) =>
          new MockMap(container, options),
      ),
      marker,
      tileLayer,
      Icon,
    },
    Icon,
  };
});
vi.mock('leaflet/dist/leaflet.css', () => ({}));

// Import AFTER mocks are registered.
import { SiteLocationPicker } from './SiteLocationPicker';

beforeEach(() => {
  MockMap.instances.length = 0;
  MockMap.shouldThrowOnConstruct = false;
  MockTileLayer.instances.length = 0;
});

describe('SiteLocationPicker', () => {
  it('reports coordinates when the map is clicked', () => {
    const onPick = vi.fn();
    render(
      <SiteLocationPicker
        latitudeDeg={null}
        longitudeDeg={null}
        onPick={onPick}
      />,
    );

    const map = MockMap.instances[0];
    expect(map).toBeDefined();
    act(() => {
      map.emit('click', { latlng: { lat: 51.5, lng: -0.12 } });
    });

    expect(onPick).toHaveBeenCalledWith(51.5, -0.12);
  });

  it('moves the pin when lat/lon props change (fields edited)', () => {
    const { rerender } = render(
      <SiteLocationPicker
        latitudeDeg={null}
        longitudeDeg={null}
        onPick={vi.fn()}
      />,
    );
    const map = MockMap.instances[0];

    act(() => {
      rerender(
        <SiteLocationPicker
          latitudeDeg={40.7}
          longitudeDeg={-74}
          onPick={vi.fn()}
        />,
      );
    });

    expect(map.panToCalls).toContainEqual([40.7, -74]);

    act(() => {
      rerender(
        <SiteLocationPicker
          latitudeDeg={41}
          longitudeDeg={-75}
          onPick={vi.fn()}
        />,
      );
    });

    expect(map.panToCalls).toContainEqual([41, -75]);
  });

  it('ignores an out-of-range latitude instead of crashing (regression)', () => {
    // The lat/lon fields accept out-of-range values mid-type — the component
    // must skip panTo/setLatLng rather than forwarding the invalid coord.
    const { rerender } = render(
      <SiteLocationPicker
        latitudeDeg={null}
        longitudeDeg={null}
        onPick={vi.fn()}
      />,
    );
    const map = MockMap.instances[0];

    expect(() => {
      act(() => {
        rerender(
          <SiteLocationPicker
            latitudeDeg={200}
            longitudeDeg={10}
            onPick={vi.fn()}
          />,
        );
      });
    }).not.toThrow();

    expect(map.panToCalls).toHaveLength(0);
    expect(screen.getByTestId('site-location-map')).toBeInTheDocument();
    expect(screen.queryByText(/map unavailable/i)).not.toBeInTheDocument();
  });

  it('degrades gracefully when the map fails to construct', () => {
    MockMap.shouldThrowOnConstruct = true;

    render(
      <SiteLocationPicker
        latitudeDeg={null}
        longitudeDeg={null}
        onPick={vi.fn()}
      />,
    );

    expect(screen.queryByTestId('site-location-map')).not.toBeInTheDocument();
    expect(screen.getByText(/map unavailable/i)).toBeInTheDocument();
  });

  it('degrades gracefully when tiles fail to load (offline / provider outage)', () => {
    // Remote observatory sites are often offline — three consecutive tileerror
    // events cross the threshold and trigger the unavailable notice.
    render(
      <SiteLocationPicker
        latitudeDeg={null}
        longitudeDeg={null}
        onPick={vi.fn()}
      />,
    );

    const tiles = MockTileLayer.instances[0];
    expect(tiles).toBeDefined();

    act(() => {
      tiles.emit('tileerror', {});
      tiles.emit('tileerror', {});
    });
    // Below threshold — map still visible.
    expect(screen.getByTestId('site-location-map')).toBeInTheDocument();
    expect(screen.queryByText(/map unavailable/i)).not.toBeInTheDocument();

    act(() => {
      tiles.emit('tileerror', {});
    });
    // At threshold — degrade to unavailable notice.
    expect(screen.queryByTestId('site-location-map')).not.toBeInTheDocument();
    expect(screen.getByText(/map unavailable/i)).toBeInTheDocument();
  });

  it('does not degrade when a tileload resets the error count', () => {
    render(
      <SiteLocationPicker
        latitudeDeg={null}
        longitudeDeg={null}
        onPick={vi.fn()}
      />,
    );

    const tiles = MockTileLayer.instances[0];

    act(() => {
      tiles.emit('tileerror', {});
      tiles.emit('tileerror', {});
      // A successful load resets the counter.
      tiles.emit('tileload', {});
      // Two more errors — still below threshold (count restarted at 0).
      tiles.emit('tileerror', {});
      tiles.emit('tileerror', {});
    });

    expect(screen.getByTestId('site-location-map')).toBeInTheDocument();
    expect(screen.queryByText(/map unavailable/i)).not.toBeInTheDocument();
  });
});
