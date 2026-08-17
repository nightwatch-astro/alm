// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * SessionListPopover — migrated onto the shared `DetailLinkedGroup` leaf
 * (#813) instead of hand-rolling the `pv-session-detail2__head`/`__muted`
 * classes. Proves the head label and empty/non-empty rendering still hold.
 */

import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { SessionListPopover } from './SessionListPopover';

describe('SessionListPopover', () => {
  it('renders the muted "None" placeholder when there are no names', () => {
    render(<SessionListPopover label="Used by" names={[]} />);
    expect(screen.getByTestId('detail-group-head')).toHaveTextContent(
      'Used by',
    );
    expect(screen.getByText('None')).toBeInTheDocument();
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });

  it('renders a count trigger popover when names are present', () => {
    render(
      <SessionListPopover
        label="Compatible"
        names={['a · b · c', 'd · e · f']}
      />,
    );
    expect(screen.getByText('Compatible')).toBeInTheDocument();
    expect(screen.getByText('2 ▾')).toBeInTheDocument();
    expect(screen.queryByText('None')).not.toBeInTheDocument();
  });
});
