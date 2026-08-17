// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * TwoColDetailLayout / DetailLinkedGroup (#813) — the shared two-col-
 * properties + linked-entity detail recipe consumed by SessionDetail,
 * MasterDetail, and (via `DetailLinkedGroup`) SessionListPopover.
 */

import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { TwoColDetailLayout, DetailLinkedGroup } from './TwoColDetailLayout';

describe('TwoColDetailLayout', () => {
  it('renders colA/colB and omits the linked slot when absent', () => {
    render(<TwoColDetailLayout colA={<span>A</span>} colB={<span>B</span>} />);
    const wrapper = screen.getByTestId('two-col-detail');
    expect(wrapper.querySelectorAll('[data-testid="detail-col"]')).toHaveLength(
      2,
    );
    expect(wrapper.querySelector('[data-testid="detail-linked"]')).toBeNull();
  });

  it('omits the colB slot entirely when colB is null', () => {
    // An empty col still claims min-width: 340px in the flex row, so
    // rendering one for absent content reads as a gap, not as nothing.
    render(<TwoColDetailLayout colA={<span>A</span>} colB={null} />);
    const wrapper = screen.getByTestId('two-col-detail');
    expect(wrapper.querySelectorAll('[data-testid="detail-col"]')).toHaveLength(
      1,
    );
  });

  it('renders extraCols as full col slots, skipping null entries', () => {
    render(
      <TwoColDetailLayout
        colA={<span>A</span>}
        colB={<span>B</span>}
        extraCols={[<span key="c">C</span>, null, <span key="d">D</span>]}
      />,
    );
    const wrapper = screen.getByTestId('two-col-detail');
    // A, B, C, D — the null entry contributes nothing.
    expect(wrapper.querySelectorAll('[data-testid="detail-col"]')).toHaveLength(
      4,
    );
    expect(screen.getByText('C')).toBeInTheDocument();
    expect(screen.getByText('D')).toBeInTheDocument();
  });

  it('renders extraCols as col slots, never as the narrow linked slot', () => {
    // Guards the regression this API exists to prevent: linked is
    // flex: 0 0 auto; min-width: 160px and squeezes table-shaped content.
    render(
      <TwoColDetailLayout
        colA={<span>A</span>}
        colB={<span>B</span>}
        extraCols={[<span key="t">table</span>]}
      />,
    );
    expect(screen.queryByTestId('detail-linked')).toBeNull();
    // The table content sits inside a detail-col, not a detail-linked
    expect(
      screen.getByText('table').closest('[data-testid="detail-col"]'),
    ).toBeTruthy();
  });

  it('renders the linked slot with an optional modifier class', () => {
    render(
      <TwoColDetailLayout
        colA={<span>A</span>}
        colB={<span>B</span>}
        linked={<span>linked</span>}
        linkedClassName="pv-session-detail2__linked--stack"
      />,
    );
    const linked = screen.getByTestId('detail-linked');
    expect(linked).toBeInTheDocument();
    expect(linked).toHaveTextContent('linked');
    expect(linked).toHaveClass('pv-session-detail2__linked--stack');
  });
});

describe('DetailLinkedGroup', () => {
  it('renders the head label plus children when not empty', () => {
    render(
      <DetailLinkedGroup label="Used by">
        <span>content</span>
      </DetailLinkedGroup>,
    );
    expect(screen.getByTestId('detail-group-head')).toHaveTextContent(
      'Used by',
    );
    expect(screen.getByText('content')).toBeInTheDocument();
  });

  it('renders the muted emptyLabel instead of children when empty', () => {
    render(
      <DetailLinkedGroup label="Compatible" empty emptyLabel="None">
        <span>content</span>
      </DetailLinkedGroup>,
    );
    expect(screen.queryByText('content')).not.toBeInTheDocument();
    expect(screen.getByText('None')).toBeInTheDocument();
  });
});
