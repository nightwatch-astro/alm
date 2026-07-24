// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * ProjectsTable tests — spec 043 (tasks #73/#43/#105).
 *
 * The full-width dense sortable table that replaced the narrow ProjectsList
 * sidebar. Covers the same row behaviors the old list test asserted, adapted to
 * the table surface:
 *   1. Renders project names.
 *   2. Empty state when no projects.
 *   3. Loading state.
 *   4. onSelect fires with the project id on row click.
 *   5. Channel-drift badge renders for drifting projects.
 *   6. Rich columns: Tool, State (now a dot+text tag, not a pill), Sources, Updated.
 *   7. Selected row carries the selected CSS class.
 *   8. Clicking a sortable header calls onSort with the column.
 *   9. (#105) State column renders a StatusTag (dot + label, no pill).
 */

import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { ProjectsTable, DEFAULT_PROJECT_SORT } from './ProjectsTable';
import type { ProjectSummaryDto } from '@/bindings/index';

const mockProjects: ProjectSummaryDto[] = [
  {
    id: 'proj-001',
    name: 'NGC 7000 Narrowband',
    tool: 'PixInsight',
    lifecycle: 'processing',
    path: 'projects/NGC7000_NB',
    channelDrift: false,
    sourceCount: 3,
    createdAt: '2026-06-01T00:00:00Z',
    updatedAt: '2026-06-10T00:00:00Z',
  },
  {
    id: 'proj-002',
    name: 'M31 LRGB',
    tool: 'PixInsight',
    lifecycle: 'ready',
    path: 'projects/M31_LRGB',
    channelDrift: true,
    sourceCount: 2,
    createdAt: '2026-06-02T00:00:00Z',
    updatedAt: '2026-06-09T00:00:00Z',
  },
];

function renderTable(
  overrides: Partial<React.ComponentProps<typeof ProjectsTable>> = {},
) {
  return render(
    <ProjectsTable
      projects={mockProjects}
      selectedId={undefined}
      onSelect={vi.fn()}
      sort={DEFAULT_PROJECT_SORT}
      onSort={vi.fn()}
      {...overrides}
    />,
  );
}

describe('ProjectsTable', () => {
  it('renders project names', () => {
    renderTable();
    expect(screen.getByText('NGC 7000 Narrowband')).toBeInTheDocument();
    expect(screen.getByText('M31 LRGB')).toBeInTheDocument();
  });

  it('shows empty state when no projects', () => {
    renderTable({ projects: [] });
    expect(screen.getByText(/no projects yet/i)).toBeInTheDocument();
  });

  it('shows filtered-empty state when a filter yields no projects', () => {
    renderTable({ projects: [], isFiltered: true });
    expect(
      screen.getByText(/no projects match the current filters/i),
    ).toBeInTheDocument();
  });

  it('shows loading state when loading and no projects', () => {
    renderTable({ projects: [], loading: true });
    // Loading now renders a skeleton (role="status") instead of text.
    expect(screen.getByTestId('skeleton')).toBeInTheDocument();
  });

  it('calls onSelect with the project id on row click', () => {
    const onSelect = vi.fn();
    renderTable({ onSelect });
    fireEvent.click(screen.getByText('NGC 7000 Narrowband'));
    expect(onSelect).toHaveBeenCalledWith('proj-001');
  });

  it('shows the channel drift badge for projects with drift', () => {
    renderTable();
    // M31 LRGB has channelDrift: true → shows the ⚠ channels badge.
    expect(screen.getByTitle('Channel drift detected')).toBeInTheDocument();
  });

  it('renders rich columns: tool, state tag, sources and updated date', () => {
    renderTable();
    // Tool column present on every row.
    expect(screen.getAllByText('PixInsight').length).toBe(2);
    // State tags (dot + label — task #105, replaces filled pill).
    expect(screen.getByText('Processing')).toBeInTheDocument();
    expect(screen.getByText('Ready')).toBeInTheDocument();
    // Source count.
    expect(screen.getByText('3')).toBeInTheDocument();
    // Updated date formatted as "yyyy-MM-dd HH:mm" (local).
    expect(
      screen.getAllByText(/2026-06-1[09] \d{2}:\d{2}/).length,
    ).toBeGreaterThan(0);
  });

  it('(#105) state column uses StatusTag: dot+text, not a filled pill', () => {
    const { container } = renderTable();
    // StatusTag renders dot+text — verify via data-component attribute.
    const tags = container.querySelectorAll('[data-component="status-tag"]');
    expect(tags.length).toBeGreaterThan(0);
    // Each tag contains a dot span.
    tags.forEach((tag) => {
      expect(tag.querySelector('[data-component="status-tag-dot"]')).toBeInTheDocument();
    });
  });

  it('marks the selected row with the selected CSS class', () => {
    const { container } = renderTable({ selectedId: 'proj-001' });
    const selected = container.querySelectorAll(
      '.pv-projects-table__row--selected',
    );
    expect(selected).toHaveLength(1);
    expect(selected[0]).toHaveTextContent('NGC 7000 Narrowband');
  });

  it('calls onSort with the column when a sortable header is clicked', () => {
    const onSort = vi.fn();
    renderTable({ onSort });
    fireEvent.click(screen.getByRole('button', { name: 'Sort by Name' }));
    expect(onSort).toHaveBeenCalledWith('name');
  });

  it('(#720 SC-001) a blocked row carries the real blocked reason as title + aria-label, not a bare icon', () => {
    const blockedProjects: ProjectSummaryDto[] = [
      {
        id: 'proj-blocked',
        name: 'Cave Nebula attempt',
        tool: 'Siril',
        lifecycle: 'blocked',
        path: 'projects/Cave_attempt',
        blockedReasonKind: 'calibration_unmatched',
        blockedReasonNote: 'Missing calibration masters for SII filter',
        channelDrift: false,
        sourceCount: 1,
        createdAt: '2026-06-01T00:00:00Z',
        updatedAt: '2026-06-10T00:00:00Z',
      },
    ];
    renderTable({ projects: blockedProjects });
    const icon = screen.getByRole('img', {
      name: 'Blocked: Calibration set missing',
    });
    expect(icon).toBeInTheDocument();
    expect(icon.closest('span[title]')).toHaveAttribute(
      'title',
      'Calibration set missing',
    );
  });
});
