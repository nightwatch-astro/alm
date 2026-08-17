// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * TableStateGate tests — each branch of the loading/error/empty/data gate.
 *
 * The `loading && isEmpty` skeleton gate is the subtle one: a refetch that
 * still has rows on screen must keep the table visible rather than replace it
 * with a skeleton.
 */

import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { TableStateGate } from './TableStateGate';

const EMPTY = <div>no records</div>;
const FILTERED_EMPTY = <div>no match</div>;
const ERROR_EMPTY = <div>load failed</div>;
const TABLE = <table aria-label="rows" />;

describe('TableStateGate', () => {
  it('renders the skeleton while loading with no data yet', () => {
    render(
      <TableStateGate loading isEmpty empty={EMPTY} skeletonLabel="Loading">
        {TABLE}
      </TableStateGate>,
    );
    expect(screen.getByLabelText('Loading')).toBeInTheDocument();
    expect(screen.queryByLabelText('rows')).not.toBeInTheDocument();
  });

  it('keeps the table visible during a refetch that already has data', () => {
    render(
      <TableStateGate
        loading
        isEmpty={false}
        empty={EMPTY}
        skeletonLabel="Loading"
      >
        {TABLE}
      </TableStateGate>,
    );
    expect(screen.getByLabelText('rows')).toBeInTheDocument();
    expect(screen.queryByLabelText('Loading')).not.toBeInTheDocument();
  });

  it('renders errorEmpty when the load failed', () => {
    render(
      <TableStateGate
        loading={false}
        error="boom"
        isEmpty
        empty={EMPTY}
        errorEmpty={ERROR_EMPTY}
      >
        {TABLE}
      </TableStateGate>,
    );
    expect(screen.getByText('load failed')).toBeInTheDocument();
  });

  it('falls back to the error text when errorEmpty is omitted', () => {
    render(
      <TableStateGate loading={false} error="boom" isEmpty empty={EMPTY}>
        {TABLE}
      </TableStateGate>,
    );
    expect(screen.getByText('boom')).toBeInTheDocument();
  });

  it('renders filteredEmpty for a filter miss', () => {
    render(
      <TableStateGate
        loading={false}
        isEmpty
        isFilteredEmpty
        empty={EMPTY}
        filteredEmpty={FILTERED_EMPTY}
      >
        {TABLE}
      </TableStateGate>,
    );
    expect(screen.getByText('no match')).toBeInTheDocument();
    expect(screen.queryByText('no records')).not.toBeInTheDocument();
  });

  it('renders empty for a filter miss when filteredEmpty is omitted', () => {
    render(
      <TableStateGate loading={false} isEmpty isFilteredEmpty empty={EMPTY}>
        {TABLE}
      </TableStateGate>,
    );
    expect(screen.getByText('no records')).toBeInTheDocument();
  });

  it('renders empty for a truly empty list', () => {
    render(
      <TableStateGate
        loading={false}
        isEmpty
        empty={EMPTY}
        filteredEmpty={FILTERED_EMPTY}
      >
        {TABLE}
      </TableStateGate>,
    );
    expect(screen.getByText('no records')).toBeInTheDocument();
    expect(screen.queryByText('no match')).not.toBeInTheDocument();
  });

  it('renders children when data is present', () => {
    render(
      <TableStateGate loading={false} isEmpty={false} empty={EMPTY}>
        {TABLE}
      </TableStateGate>,
    );
    expect(screen.getByLabelText('rows')).toBeInTheDocument();
  });

  it('wraps content and applies data-testid when wrapperClassName is set', () => {
    render(
      <TableStateGate
        loading={false}
        isEmpty={false}
        empty={EMPTY}
        wrapperClassName="pv-calib-table__status"
        data-testid="gate"
      >
        {TABLE}
      </TableStateGate>,
    );
    const wrapper = screen.getByTestId('gate');
    expect(wrapper).toHaveClass('pv-calib-table__status');
    expect(wrapper).toContainElement(screen.getByLabelText('rows'));
  });

  it('renders no wrapper element without wrapperClassName', () => {
    const { container } = render(
      <TableStateGate loading={false} isEmpty={false} empty={EMPTY}>
        {TABLE}
      </TableStateGate>,
    );
    expect(container.firstChild).toBe(screen.getByLabelText('rows'));
  });
});
