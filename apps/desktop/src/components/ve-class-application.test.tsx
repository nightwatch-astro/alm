// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Guards the vanilla-extract wave-1 migration against silently dropped classes:
 * each component must apply the style exported for its element, because the
 * legacy `pv-*` rules those elements used to match no longer exist.
 */

import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { DetailHeader } from './DetailHeader';
import { subtitle as detailSubtitle } from './DetailHeader.css';
import { TopActionBar } from './TopActionBar';
import {
  actionBarWrap,
  actionsWrap,
  spacerWrap,
} from './TopActionBar.css';
import { EmptyState } from '../ui/EmptyState';
import { empty } from '../ui/EmptyState.css';

describe('vanilla-extract class application', () => {
  it('applies the EmptyState root style', () => {
    render(<EmptyState title="Nothing here" description="Add a folder" />);

    expect(screen.getByText('Nothing here').parentElement).toHaveClass(empty);
  });

  it('keeps a caller-supplied className alongside the root style', () => {
    render(<EmptyState title="Nothing here" className="caller-class" />);

    const root = screen.getByText('Nothing here').parentElement;
    expect(root).toHaveClass(empty);
    expect(root).toHaveClass('caller-class');
  });

  it('applies the DetailHeader subtitle style', () => {
    render(<DetailHeader title="M31" subtitle="/very/long/archive/path" />);

    expect(screen.getByText('/very/long/archive/path')).toHaveClass(
      detailSubtitle,
    );
  });

  it('adds the wrap variant classes only when TopActionBar wraps', () => {
    const { container, unmount } = render(
      <TopActionBar title="Bar" right={<button type="button">Go</button>} />,
    );
    expect(container.querySelector(`.${actionBarWrap}`)).toBeNull();
    expect(container.querySelector(`.${spacerWrap}`)).toBeNull();
    expect(container.querySelector(`.${actionsWrap}`)).toBeNull();
    unmount();

    const wrapped = render(
      <TopActionBar
        title="Bar"
        wrap
        right={<button type="button">Go</button>}
      />,
    );
    expect(
      wrapped.container.querySelector(`.${actionBarWrap}`),
    ).not.toBeNull();
    expect(wrapped.container.querySelector(`.${spacerWrap}`)).not.toBeNull();
    expect(wrapped.container.querySelector(`.${actionsWrap}`)).not.toBeNull();
  });
});
