// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * Tests for InboxDetail (FR-010: per-file metadata table, FR-011: mixed
 * composition summary).
 *
 * Scope: ONLY InboxDetail.tsx. Uses fixtures; no IPC mocks needed because
 * InboxDetail never fetches — it renders the data it receives.
 *
 * InboxFileMetadata is the generated Specta type (camelCase), re-exported via
 * '@/bindings/index' (spec 041 US2/FR-010 — wired in T019):
 *   relativeFilePath, frameTypeEffective, imageTyp, filter, exposureS, binningX,
 *   binningY, gain (string), temperatureC, object, dateObs, instrume, telescop,
 *   naxis1, naxis2, stackCount, isMaster, overrideStale
 */

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import {
  fireEvent,
  render as rtlRender,
  screen,
  within,
} from '@testing-library/react';
import type React from 'react';
import { describe, expect, it, vi } from 'vitest';
import type {
  InboxFileMetadata_Serialize as InboxFileMetadata,
  InboxClassifyResponse_Serialize as InboxClassifyResponse,
  InboxItemSummary_Serialize as InboxItemSummary,
} from '@/bindings';

import { m } from '@/lib/i18n';
import { InboxDetail } from '../InboxDetail';

// InboxDetail uses the TanStack-Query-backed `useInboxReclassify` hook (spec 042),
// so every render must be wrapped in a QueryClientProvider.
function render(ui: React.ReactElement) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return rtlRender(
    <QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>,
  );
}

// ── Mock reclassify hook ─────────────────────────────────────────────────────
vi.mock('@/bindings/index', async (importOriginal) => {
  const mod = await importOriginal<typeof import('@/bindings/index')>();
  return {
    ...mod,
    commands: {
      ...mod.commands,
      inboxReclassify: vi.fn().mockResolvedValue({
        status: 'ok',
        data: {
          inboxItemId: 'item-001',
          remainingUnclassified: 0,
        },
      }),
    },
  };
});

// ── Fixtures ─────────────────────────────────────────────────────────────────

const sampleItem: InboxItemSummary = {
  inboxItemId: 'item-001',
  relativePath: '2025-10-10/NGC7000',
  fileCount: 17,
  lane: 'fits',
  format: 'fits',
  state: 'classified',
  contentSignature: 'sig-001',
  isMaster: false,
  masterFrameType: null,
  masterFilter: null,
  masterExposureS: null,
};

/** A detected calibration master file (#551 repro: masters never get a
 * per-file `inbox_file_metadata` row — `fileMetadata` is always empty for
 * them — so the required-attribute gate has no data to evaluate). */
const masterItem: InboxItemSummary = {
  inboxItemId: 'item-master-001',
  relativePath: 'masterDark.fit',
  fileCount: 1,
  lane: 'fits',
  format: 'fits',
  state: 'classified',
  contentSignature: 'sig-master-001',
  isMaster: true,
  masterFrameType: 'dark',
  masterFilter: null,
  masterExposureS: 300,
};

/** Classification with three frame types → classType "mixed" */
const mixedClassification: InboxClassifyResponse = {
  inboxItemId: 'item-001',
  type: 'mixed',
  frameType: null,
  contentSignature: 'sig-001',
  breakdown: [
    {
      kind: 'light',
      count: 12,
      destinationPreview: 'NGC7000/Ha/light/',
      sampleFiles: [],
    },
    {
      kind: 'dark',
      count: 4,
      destinationPreview: 'calib/dark/',
      sampleFiles: [],
    },
    {
      kind: 'flat',
      count: 1,
      destinationPreview: 'calib/flat/',
      sampleFiles: [],
    },
  ],
  unclassifiedFiles: [],
  sampleFiles: [],
  computedAt: '2025-10-10T22:00:00Z',
};

/** Classification with a single frame type */
const singleTypeClassification: InboxClassifyResponse = {
  inboxItemId: 'item-001',
  type: 'single_type',
  frameType: 'light',
  contentSignature: 'sig-001',
  breakdown: [
    {
      kind: 'light',
      count: 17,
      destinationPreview: 'NGC7000/Ha/light/',
      sampleFiles: [],
    },
  ],
  unclassifiedFiles: [],
  sampleFiles: [],
  computedAt: '2025-10-10T22:00:00Z',
};

/** Two-row fixture for the per-file metadata table (FR-010). */
const fileMetadataFixture: InboxFileMetadata[] = [
  {
    relativeFilePath: 'light_0001.fits',
    frameTypeEffective: 'light',
    imageTyp: 'LIGHT',
    filter: 'Ha',
    exposureS: 300,
    binningX: 1,
    binningY: 1,
    gain: '100',
    temperatureC: -10,
    object: 'NGC7000',
    dateObs: '2025-10-10T22:00:00Z',
    instrume: 'ASI2600MM',
    telescop: null,
    naxis1: 6248,
    naxis2: 4176,
    stackCount: null,
    isMaster: false,
    overrideStale: false,
  },
  {
    relativeFilePath: 'calib_dark_0001.fits',
    frameTypeEffective: 'dark',
    imageTyp: 'DARK',
    filter: null,
    exposureS: null,
    binningX: 1,
    binningY: 1,
    gain: '100',
    temperatureC: null,
    object: null,
    dateObs: null,
    instrume: null,
    telescop: null,
    naxis1: null,
    naxis2: null,
    stackCount: 30,
    isMaster: true,
    overrideStale: false,
  },
];

// ── FR-011: Mixed composition summary ────────────────────────────────────────

describe('InboxDetail — FR-010: file metadata popover trigger', () => {
  it('renders the popover trigger button when fileMetadata is provided', () => {
    render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={singleTypeClassification}
        fileMetadata={fileMetadataFixture}
      />,
    );
    expect(
      screen.getByTestId('inbox-files-popover-trigger'),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId('inbox-files-popover-trigger').textContent,
    ).toContain('File metadata (2)');
  });

  it('does NOT render the popover trigger when fileMetadata is absent', () => {
    render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={singleTypeClassification}
      />,
    );
    expect(
      screen.queryByTestId('inbox-files-popover-trigger'),
    ).not.toBeInTheDocument();
  });

  it('does NOT render the popover trigger when fileMetadata is an empty array', () => {
    render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={singleTypeClassification}
        fileMetadata={[]}
      />,
    );
    expect(
      screen.queryByTestId('inbox-files-popover-trigger'),
    ).not.toBeInTheDocument();
  });

  it('opens the popup with the metadata table when the trigger is clicked', () => {
    render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={singleTypeClassification}
        fileMetadata={fileMetadataFixture}
      />,
    );
    fireEvent.click(screen.getByTestId('inbox-files-popover-trigger'));
    // Popup should now be in the DOM (portaled to body).
    expect(screen.getByTestId('inbox-files-popup')).toBeInTheDocument();
    // File paths appear in the popup table.
    expect(screen.getByTitle('light_0001.fits')).toBeInTheDocument();
    expect(screen.getByTitle('calib_dark_0001.fits')).toBeInTheDocument();
  });

  it('popup contains the missing-attr badge for files that need it', () => {
    const withMissing: InboxFileMetadata[] = [
      {
        ...fileMetadataFixture[0],
        relativeFilePath: 'light_ok.fits',
        missingPathAttributes: [],
      },
      {
        ...fileMetadataFixture[0],
        relativeFilePath: 'light_nodate.fits',
        dateObs: null,
        missingPathAttributes: ['date'],
      },
    ];
    render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={singleTypeClassification}
        fileMetadata={withMissing}
      />,
    );
    fireEvent.click(screen.getByTestId('inbox-files-popover-trigger'));
    const badge = screen.getByTestId('inbox-missing-attr-light_nodate.fits');
    expect(badge).toHaveTextContent('needs date');
    expect(
      screen.queryByTestId('inbox-missing-attr-light_ok.fits'),
    ).not.toBeInTheDocument();
  });
});

// ── FR-032 (US9): missing path-load-bearing attribute gate ───────────────────

describe('InboxDetail — FR-032: missing-attribute banner', () => {
  const withMissing: InboxFileMetadata[] = [
    {
      ...fileMetadataFixture[0],
      relativeFilePath: 'light_ok.fits',
      missingPathAttributes: [],
    },
    {
      ...fileMetadataFixture[0],
      relativeFilePath: 'light_nodate.fits',
      dateObs: null,
      missingPathAttributes: ['date'],
    },
  ];

  it('shows a summary banner counting the blocked files', () => {
    render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={singleTypeClassification}
        fileMetadata={withMissing}
      />,
    );
    expect(screen.getByTestId('inbox-missing-attr-banner')).toHaveTextContent(
      '1 file',
    );
  });

  it('renders no banner when no file is missing attributes', () => {
    render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={singleTypeClassification}
        fileMetadata={fileMetadataFixture}
      />,
    );
    expect(
      screen.queryByTestId('inbox-missing-attr-banner'),
    ).not.toBeInTheDocument();
  });

  // #554: the banner used to be its own trailing `.pv-session-detail2__col`
  // (a separate full-width alert competing with the property tables). It now
  // lives inline inside the Files column, right below the popover trigger it
  // explains.
  it('renders inline in the Files column, not as a separate trailing column', () => {
    render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={singleTypeClassification}
        fileMetadata={withMissing}
      />,
    );
    const banner = screen.getByTestId('inbox-missing-attr-banner');
    const trigger = screen.getByTestId('inbox-files-popover-trigger');
    // Same `.pv-session-detail2__col` ancestor as the Files trigger — i.e.
    // the banner is NOT its own separate trailing column.
    const filesCol = trigger.closest('[data-testid="detail-col"]');
    expect(filesCol).not.toBeNull();
    expect(filesCol?.contains(banner)).toBe(true);
  });
});

// ── #551: item-detail gating parity with the "batch"/list view ───────────────
//
// Master items never get a per-file `inbox_file_metadata` row (they bypass
// classify()'s persist_file_metadata path — see
// crates/app/inbox/src/metadata.rs), so `fileMetadata` is always empty for
// them and the FR-032 missing-attribute gate has no data to evaluate. Before
// this fix that silently rendered as "No file metadata" with no caveat,
// which read as "nothing to worry about" even though `inbox.confirm` can
// still reject the file server-side (`inbox.missing_path_attributes`) —
// the exact item-view-vs-batch-view mismatch reported in #551.
describe('InboxDetail — #551: honest "unknown" state when no per-file metadata is available', () => {
  it('appends a caveat explaining the gate is unverified, instead of implying "all clear"', () => {
    render(
      <InboxDetail
        item={masterItem}
        rootAbsolutePath="/astro/inbox"
        classification={singleTypeClassification}
      />,
    );
    const empty = screen.getByText(/No file metadata/);
    expect(empty.textContent).toContain('Required-attribute status');
  });
});

// ── task #34: mixed-folder — banner in body, action in header ────────────────

/**
 * The header confirm action (task #34).
 *
 * Spec 058 T035 retired the mixed-folder banner these tests were paired with,
 * but the header confirm behaviour they also covered is very much live, so it
 * is kept here rather than deleted along with the banner. The fixtures move to
 * a single-type classification because that is now the only shape a confirmable
 * detection has.
 */
describe('InboxDetail — task #34: header confirm action', () => {
  it('renders the confirm button in the header and reports clicks', () => {
    const onConfirm = vi.fn();
    render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={singleTypeClassification}
        onConfirm={onConfirm}
        confirmLabel="Confirm to inventory"
      />,
    );
    const btn = screen.getByTestId('inbox-confirm-btn');
    expect(btn).toHaveTextContent('Confirm to inventory');
    fireEvent.click(btn);
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it('disables the header confirm button while busy', () => {
    render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={singleTypeClassification}
        onConfirm={vi.fn()}
        confirmLabel="Confirm to inventory"
        confirmBusy
      />,
    );
    expect(screen.getByTestId('inbox-confirm-btn')).toBeDisabled();
  });

  it('does not render the header confirm button when no callback is supplied', () => {
    render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={singleTypeClassification}
      />,
    );
    expect(screen.queryByTestId('inbox-confirm-btn')).not.toBeInTheDocument();
  });

  /**
   * Spec 058 T035: the retired banner must not come back by accident. A
   * multi-type folder now reports `unclassified` and renders no advisory of its
   * own — the folder is represented by the single-type items materialization
   * produced, each confirmable on its own.
   */
  it('renders no mixed banner for a multi-type classification', () => {
    render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={mixedClassification}
      />,
    );
    expect(screen.queryByTestId('inbox-mixed-alert')).not.toBeInTheDocument();
  });
});

// ── Compact layout: SessionDetail-style left-packed col ───────────────────────
//
// The body is a .pv-session-detail2 flex row.
// Col A: PropertyTable with detection facts + mixed-summary line + Files popover trigger.
// No breakdown table. No inline metadata col. No FileInspector in the row.

describe('InboxDetail — compact layout: detection col + popover trigger', () => {
  it('renders the pv-session-detail2 row wrapper', () => {
    const { container } = render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={singleTypeClassification}
        fileMetadata={fileMetadataFixture}
      />,
    );
    expect(screen.queryByTestId('two-col-detail')).not.toBeNull();
    // No old 3-zone wrappers.
    expect(container.querySelector('.pv-detailpanel__facts')).toBeNull();
    expect(container.querySelector('.pv-detailpanel__aux')).toBeNull();
    // No inline metadata col (it lives in the popover).
    expect(container.querySelector('.pv-inbox-detail__meta-col')).toBeNull();
  });

  // #553: DetailPanel's content-only mode renders `children` with no scroll
  // wrapper (that mode assumes a self-scrolling child like a virtualized
  // Table). InboxDetail's body isn't one — it wraps everything in
  // `.pv-inbox-detail__scroll`, the sole scroll region (see detail-panes.css
  // `:has()` rule pinning the header above it), so FILES/Needs-review content
  // taller than the docked panel's max-height scrolls instead of being
  // clipped by the ancestor `.pv-listpage__detail-body`'s `overflow:hidden`.
  it('#553: wraps the body in the sole scroll region, containing the session-detail2 row', () => {
    const { container } = render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={singleTypeClassification}
        fileMetadata={fileMetadataFixture}
      />,
    );
    const scroll = container.querySelector('.pv-inbox-detail__scroll');
    expect(scroll).not.toBeNull();
    expect(scroll?.contains(screen.queryByTestId('two-col-detail'))).toBe(true);
  });

  it('renders detection facts spread across multiple property columns', () => {
    render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={singleTypeClassification}
        fileMetadata={fileMetadataFixture}
      />,
    );
    // Left-packed multi-column body (Sessions convention): ≥2 columns.
    expect(
      screen
        .getByTestId('two-col-detail')
        .querySelectorAll('[data-testid="detail-col"]').length,
    ).toBeGreaterThanOrEqual(2);
    // The Files column carries a head label (scoped to the head element —
    // "Files" also appears as a PropertyTable row label).
    // The Files column has a head label 'Files' visible in the DOM.
    expect(screen.getByText('Files', { selector: 'div' })).toBeInTheDocument();
    // 'light' from frameType appears in the PropertyTable value.
    expect(screen.getAllByText(/light/).length).toBeGreaterThan(0);
  });

  it('files popover trigger renders inside a detail column', () => {
    render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={singleTypeClassification}
        fileMetadata={fileMetadataFixture}
      />,
    );
    const cols = [...document.querySelectorAll('[data-testid="detail-col"]')];
    expect(
      cols.some(
        (c) =>
          c.querySelector('[data-testid="inbox-files-popover-trigger"]') !=
          null,
      ),
    ).toBe(true);
  });

  it('no breakdown table is rendered in the detail body', () => {
    const { container } = render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={mixedClassification}
      />,
    );
    // The old frame-type breakdown table buttons are gone.
    expect(
      container.querySelector('[data-testid^="breakdown-filter-"]'),
    ).toBeNull();
    expect(screen.queryByText('Frame type breakdown')).not.toBeInTheDocument();
  });
});

// ── spec-030 Q16 (#620, #619): missing vs not-applicable per-row ────────────

describe('InboxDetail — missing-value semantics (Q16 / #620)', () => {
  it('detection facts (col A/B) render the unresolved chip for a missing applicable value, never a bare dash', () => {
    // singleTypeClassification is 'light' — filter/target/exposure are all
    // applicable to light, so a null repFile field must render the chip.
    const noFilter: InboxFileMetadata[] = [
      { ...fileMetadataFixture[0], filter: null },
    ];
    render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={singleTypeClassification}
        fileMetadata={noFilter}
      />,
    );
    expect(screen.getAllByTestId('unresolved-chip').length).toBeGreaterThan(0);
  });

  it('per-file metadata popup: filter/target cells are blank (not-applicable) on a dark row, never the unresolved chip', () => {
    render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={mixedClassification}
        fileMetadata={fileMetadataFixture}
      />,
    );
    fireEvent.click(screen.getByTestId('inbox-files-popover-trigger'));
    // calib_dark_0001.fits (frameTypeEffective: 'dark') has filter=null and
    // object=null — both not-applicable to dark (data-model.md matrix), so
    // those specific cells must render blank, never the unresolved chip.
    // Column order (metadataColumns): file, type, filter, exposure, binning,
    // gain, temp, object, date.
    const darkRow = screen.getByTitle('calib_dark_0001.fits').closest('tr');
    expect(darkRow).not.toBeNull();
    const cells = darkRow?.querySelectorAll('td') ?? [];
    const filterCell = cells[2];
    const objectCell = cells[7];
    expect(
      filterCell.querySelector('[data-testid="unresolved-chip"]'),
    ).toBeNull();
    expect(
      objectCell.querySelector('[data-testid="unresolved-chip"]'),
    ).toBeNull();
    // Exposure/temp/date ARE applicable to dark and are absent on this
    // fixture row — they DO get the unresolved chip (the contrast that
    // proves filter/object are genuinely not-applicable, not just "also
    // missing").
    const exposureCell = cells[3];
    expect(
      exposureCell.querySelector('[data-testid="unresolved-chip"]'),
    ).not.toBeNull();
  });

  it('per-file metadata popup: a missing-but-applicable value on a light row renders the unresolved chip', () => {
    const lightMissingGain: InboxFileMetadata[] = [
      { ...fileMetadataFixture[0], gain: null },
    ];
    render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={singleTypeClassification}
        fileMetadata={lightMissingGain}
      />,
    );
    fireEvent.click(screen.getByTestId('inbox-files-popover-trigger'));
    const row = screen.getByTitle('light_0001.fits').closest('tr');
    expect(row).not.toBeNull();
    expect(
      row?.querySelector('[data-testid="unresolved-chip"]'),
    ).not.toBeNull();
  });

  it('FileInspector: telescope is not-applicable (blank) for a dark file, never the unresolved chip', () => {
    render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={mixedClassification}
        fileMetadata={fileMetadataFixture}
      />,
    );
    fireEvent.click(screen.getByTestId('inbox-files-popover-trigger'));
    // Click the dark row to inspect it (telescop is null on that fixture row).
    fireEvent.click(screen.getByTitle('calib_dark_0001.fits'));
    const telescopeRow = screen.getByTestId('inspector-telescop');
    expect(
      telescopeRow.querySelector('[data-testid="unresolved-chip"]'),
    ).toBeNull();
  });

  it('FileInspector: instrument (always-applicable) missing on the dark file renders the unresolved chip', () => {
    render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={mixedClassification}
        fileMetadata={fileMetadataFixture}
      />,
    );
    fireEvent.click(screen.getByTestId('inbox-files-popover-trigger'));
    fireEvent.click(screen.getByTitle('calib_dark_0001.fits'));
    const instrumeRow = screen.getByTestId('inspector-instrume');
    expect(
      instrumeRow.querySelector('[data-testid="unresolved-chip"]'),
    ).not.toBeNull();
  });
});

// ── #653: Files property row must include unclassified files ────────────────

describe('InboxDetail — #653 Files count includes unclassified files', () => {
  it("sums breakdown + unclassifiedFiles, matching the list row's total fileCount", () => {
    const needsReviewClassification: InboxClassifyResponse = {
      inboxItemId: 'item-001',
      type: 'mixed',
      frameType: null,
      contentSignature: 'sig-001',
      breakdown: [
        {
          kind: 'flat',
          count: 4,
          destinationPreview: 'calib/flat/',
          sampleFiles: [],
        },
      ],
      unclassifiedFiles: ['unk_0001.fits', 'unk_0002.fits'],
      sampleFiles: [],
      computedAt: '2025-10-10T22:00:00Z',
    };
    render(
      <InboxDetail
        item={{ ...sampleItem, fileCount: 6 }}
        rootAbsolutePath="/astro/inbox"
        classification={needsReviewClassification}
      />,
    );
    const filesRow = screen
      .getByRole('rowheader', { name: m.inbox_col_files() })
      .closest('[role="row"]') as HTMLElement;
    // First cell is the value; the second is the (empty here) source badge cell.
    expect(within(filesRow).getAllByRole('cell')[0]).toHaveTextContent('6');
  });
});

// ── #789: exposure renders at a sensible precision, not raw float noise ──────

describe('InboxDetail — #789 exposure formatting', () => {
  it('rounds a noisy raw float exposure in the per-file metadata table', () => {
    const noisyFileMetadata: InboxFileMetadata[] = [
      {
        ...fileMetadataFixture[0],
        exposureS: 6.92447668013071,
      },
    ];
    render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={singleTypeClassification}
        fileMetadata={noisyFileMetadata}
      />,
    );
    fireEvent.click(screen.getByTestId('inbox-files-popover-trigger'));
    const popup = screen.getByTestId('inbox-files-popup');
    expect(within(popup).getByText('6.92 s')).toBeInTheDocument();
    expect(
      within(popup).queryByText('6.92447668013071 s'),
    ).not.toBeInTheDocument();
  });

  it('shows a whole-second exposure with no decimal', () => {
    const wholeSecondMetadata: InboxFileMetadata[] = [
      { ...fileMetadataFixture[0], exposureS: 300 },
    ];
    render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={singleTypeClassification}
        fileMetadata={wholeSecondMetadata}
      />,
    );
    fireEvent.click(screen.getByTestId('inbox-files-popover-trigger'));
    const popup = screen.getByTestId('inbox-files-popup');
    expect(within(popup).getByText('300 s')).toBeInTheDocument();
  });

  it('rounds the noisy exposure in the detection property table too', () => {
    const noisyFileMetadata: InboxFileMetadata[] = [
      {
        ...fileMetadataFixture[0],
        exposureS: 6.92447668013071,
      },
    ];
    render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={singleTypeClassification}
        fileMetadata={noisyFileMetadata}
      />,
    );
    const exposureRow = screen
      .getByRole('rowheader', { name: m.inbox_col_exposure() })
      .closest('[role="row"]') as HTMLElement;
    expect(within(exposureRow).getAllByRole('cell')[0]).toHaveTextContent(
      '6.92 s',
    );
  });
});

// ── #1114: partially-resolved needs-review item ──────────────────────────────
//
// A file whose frame type the user has just supplied but which is still
// blocked on a mandatory attribute. The backend downgrades the classification
// to "unclassified" (reclassify.rs, the #1086/#711-Instance-B gate), so the
// detail pane sees classType === 'unclassified' in BOTH the "no frame type"
// and the "frame type supplied, attribute missing" cases — the bug was that it
// rendered identical copy for the two, and unmounted the editing affordance in
// the second.

describe('InboxDetail — #1114: partially-resolved needs-review', () => {
  /** No frame type at all: the original, still-correct case. */
  const noFrameType: InboxClassifyResponse = {
    ...singleTypeClassification,
    type: 'unclassified',
    frameType: null,
    unclassifiedFiles: ['mystery_001.fits'],
  };

  /**
   * Frame type supplied (so the file has dropped out of `unclassifiedFiles`),
   * but exposure + gain are still absent for a dark frame.
   */
  const frameTypeSuppliedOnly: InboxClassifyResponse = {
    ...singleTypeClassification,
    type: 'unclassified',
    frameType: null,
    unclassifiedFiles: [],
  };

  const partiallyResolvedMetadata: InboxFileMetadata[] = [
    {
      ...fileMetadataFixture[0],
      relativeFilePath: 'mystery_001.fits',
      frameTypeEffective: 'dark',
      exposureS: null,
      gain: null,
      missingPathAttributes: [],
      missingMandatory: ['exposureS', 'gain'],
    },
  ];

  it('names the frame type when nothing has been supplied yet', () => {
    render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={noFrameType}
        fileMetadata={[
          {
            ...fileMetadataFixture[0],
            relativeFilePath: 'mystery_001.fits',
            frameTypeEffective: null,
            missingPathAttributes: [],
            missingMandatory: ['frameType'],
          },
        ]}
      />,
    );
    expect(screen.getByTestId('inbox-unclassified-alert')).toHaveTextContent(
      m.inbox_frame_types_required_title(),
    );
  });

  it('names the missing attribute, not the frame type, once the type is supplied', () => {
    render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={frameTypeSuppliedOnly}
        fileMetadata={partiallyResolvedMetadata}
      />,
    );
    const banner = screen.getByTestId('inbox-unclassified-alert');
    expect(banner).toHaveTextContent('Exposure s');
    expect(banner).toHaveTextContent('Gain');
    expect(banner).not.toHaveTextContent(m.inbox_frame_types_required_title());
  });

  it('keeps the needs-review editor mounted while an attribute is still missing', () => {
    render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={frameTypeSuppliedOnly}
        fileMetadata={partiallyResolvedMetadata}
      />,
    );
    // The section survives the file leaving `unclassifiedFiles` — this is the
    // affordance the user needs in order to supply the exposure.
    expect(screen.getByTestId('reclassify-select-all')).toBeInTheDocument();
    expect(screen.getByTitle('mystery_001.fits')).toBeInTheDocument();
  });

  it('unmounts the needs-review editor once nothing is missing', () => {
    render(
      <InboxDetail
        item={sampleItem}
        rootAbsolutePath="/astro/inbox"
        classification={singleTypeClassification}
        fileMetadata={[
          {
            ...fileMetadataFixture[0],
            missingPathAttributes: [],
            missingMandatory: [],
          },
        ]}
      />,
    );
    expect(
      screen.queryByTestId('reclassify-select-all'),
    ).not.toBeInTheDocument();
  });
});
