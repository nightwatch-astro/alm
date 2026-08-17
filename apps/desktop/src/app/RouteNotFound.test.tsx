// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />

import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { RouteNotFound } from './RouteNotFound';

// Link from @tanstack/react-router requires a router context; render it as a
// plain anchor so the component is testable without a full RouterProvider.
vi.mock('@tanstack/react-router', async (importOriginal) => {
  const actual =
    await importOriginal<typeof import('@tanstack/react-router')>();
  return {
    ...actual,
    Link: ({
      to,
      children,
      className,
    }: {
      to: string;
      children: React.ReactNode;
      className?: string;
    }) => (
      <a href={to} className={className}>
        {children}
      </a>
    ),
  };
});

describe('RouteNotFound', () => {
  it('renders with data-testid="route-not-found"', () => {
    render(<RouteNotFound />);
    expect(screen.getByTestId('route-not-found')).toBeInTheDocument();
  });

  it('shows "Page not found" text', () => {
    render(<RouteNotFound />);
    expect(screen.getByText(/page not found/i)).toBeInTheDocument();
  });
});
