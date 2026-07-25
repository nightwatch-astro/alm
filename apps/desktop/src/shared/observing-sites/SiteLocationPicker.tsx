// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// Observing Site: interactive map picker (spec 044 journey-1, issue #491).
//
// Shared by the first-run wizard (StepSite) and the Settings site form
// (ObservingSites). Leaflet replaces maplibre-gl: same click-to-pick
// semantics, ~5× smaller chunk (~240 KB raw vs ~1.1 MB), no WebGL required.
//
// Top-level Leaflet import is intentional — React.lazy in StepSite and
// ObservingSites splits this whole module (and Leaflet) into a separate chunk
// that loads only when the map section mounts.
//
// The numeric lat/lon fields remain the source of truth — this component
// translates between them and the map: a click fires `onPick`, and a prop
// change from field editing moves the pin. If Leaflet fails to initialize
// (DOM unavailable, webview restriction) the component degrades to a text
// notice rather than crashing — the fields keep working either way.

import { useEffect, useRef, useState } from 'react';
import L from 'leaflet';
import 'leaflet/dist/leaflet.css';
import { m } from '@/lib/i18n';

// OpenStreetMap tiles — no API key required, same as the prior tile provider.
const TILE_URL = 'https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png';
const TILE_ATTRIBUTION =
  '© <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors';

// World view used before any coordinates are set.
const DEFAULT_CENTER: L.LatLngExpression = [20, 10]; // [lat, lon]
const DEFAULT_ZOOM = 1;
const PICKED_ZOOM = 6;

// The lat/lon fields accept out-of-range values while the user types
// (validation happens on blur/submit). Guard here so we never hand invalid
// coordinates to Leaflet, which silently wraps longitude but clamps latitude.
function isValidLatLon(lat: number, lon: number): boolean {
  return lat >= -90 && lat <= 90 && lon >= -180 && lon <= 180;
}

// Fix Leaflet's bundled icon paths, which break under Vite's content-hashed
// asset pipeline because Leaflet detects its path via `import.meta.url` only
// in its own module scope.
function fixLeafletIconPaths() {
  L.Icon.Default.mergeOptions({
    iconUrl: new URL('leaflet/dist/images/marker-icon.png', import.meta.url)
      .href,
    iconRetinaUrl: new URL(
      'leaflet/dist/images/marker-icon-2x.png',
      import.meta.url,
    ).href,
    shadowUrl: new URL('leaflet/dist/images/marker-shadow.png', import.meta.url)
      .href,
  });
}

// Called once per module load — Vite resolves the asset URLs at build time.
fixLeafletIconPaths();

export interface SiteLocationPickerProps {
  /** `null` when the field is blank or not a valid number — no pin shown. */
  latitudeDeg: number | null;
  longitudeDeg: number | null;
  /** Fired when the user clicks the map; caller updates the numeric fields. */
  onPick: (latitudeDeg: number, longitudeDeg: number) => void;
}

export function SiteLocationPicker({
  latitudeDeg,
  longitudeDeg,
  onPick,
}: SiteLocationPickerProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const mapRef = useRef<L.Map | null>(null);
  const markerRef = useRef<L.Marker | null>(null);
  const onPickRef = useRef(onPick);
  const [unavailable, setUnavailable] = useState(false);

  // Keep callback ref current without triggering map re-init effects.
  useEffect(() => {
    onPickRef.current = onPick;
  });

  // Freeze the initial view at mount time — coordinate changes are handled by
  // the recenter effect below, not by re-running this one.
  const [initialView] = useState<{ center: L.LatLngExpression; zoom: number }>(
    () => {
      const hasValid =
        latitudeDeg != null &&
        longitudeDeg != null &&
        isValidLatLon(latitudeDeg, longitudeDeg);
      return {
        center: hasValid ? [latitudeDeg, longitudeDeg] : DEFAULT_CENTER,
        zoom: hasValid ? PICKED_ZOOM : DEFAULT_ZOOM,
      };
    },
  );

  useEffect(() => {
    if (!containerRef.current) return;

    let map: L.Map;
    try {
      map = L.map(containerRef.current, {
        center: initialView.center,
        zoom: initialView.zoom,
        zoomControl: true,
      });
    } catch {
      // DOM not ready or webview restriction — never blocks the containing form.
      setUnavailable(true);
      return;
    }

    // Count consecutive tile failures. A single edge-tile miss is normal (OSM
    // tiles for low-zoom ocean areas are often missing); 3+ consecutive errors
    // indicate offline or provider outage — degrade to the unavailable notice
    // so the user knows to enter coordinates manually. Resets on any success.
    let tileErrorCount = 0;
    const TILE_ERROR_THRESHOLD = 3;
    const tiles = L.tileLayer(TILE_URL, { attribution: TILE_ATTRIBUTION });
    tiles.on('tileerror', () => {
      tileErrorCount += 1;
      if (tileErrorCount >= TILE_ERROR_THRESHOLD) {
        setUnavailable(true);
      }
    });
    tiles.on('tileload', () => {
      tileErrorCount = 0;
    });
    tiles.addTo(map);

    if (
      latitudeDeg != null &&
      longitudeDeg != null &&
      isValidLatLon(latitudeDeg, longitudeDeg)
    ) {
      markerRef.current = L.marker([latitudeDeg, longitudeDeg]).addTo(map);
    }

    map.on('click', (e: L.LeafletMouseEvent) => {
      onPickRef.current(e.latlng.lat, e.latlng.lng);
    });

    mapRef.current = map;

    return () => {
      map.remove();
      mapRef.current = null;
      markerRef.current = null;
    };
    // initialView is frozen at mount — no deps on latitudeDeg/longitudeDeg here.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initialView]);

  // Move/place the pin when the numeric fields change externally (user edits
  // the text inputs). Out-of-range values (e.g. latitude 200 mid-type) are
  // silently skipped — the form's own validation catches those on submit.
  useEffect(() => {
    const map = mapRef.current;
    if (!map) return;
    if (latitudeDeg == null || longitudeDeg == null) return;
    if (!isValidLatLon(latitudeDeg, longitudeDeg)) return;

    const ll: [number, number] = [latitudeDeg, longitudeDeg];
    try {
      if (markerRef.current) {
        markerRef.current.setLatLng(ll);
      } else {
        markerRef.current = L.marker(ll).addTo(map);
      }
      map.panTo(ll);
    } catch {
      // Belt-and-suspenders: never let a map call crash the containing form.
    }
  }, [latitudeDeg, longitudeDeg]);

  if (unavailable) {
    return (
      <p className="pv-step-site__map-unavailable">
        {m.setup_site_map_unavailable()}
      </p>
    );
  }

  return (
    <div
      ref={containerRef}
      className="pv-step-site__map"
      data-testid="site-location-map"
      role="application"
      aria-label={m.setup_site_map_label()}
    />
  );
}
