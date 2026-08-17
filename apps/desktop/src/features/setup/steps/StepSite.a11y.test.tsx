// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />

import { useState } from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import {
  DEFAULT_SITE_STEP_STATE,
  StepSite,
  type SiteStepState,
} from './StepSite';

vi.mock('@/shared/observing-sites/SiteLocationPicker', () => ({
  SiteLocationPicker: () => null,
}));

function ControlledSite({ initial }: { initial?: Partial<SiteStepState> }) {
  const [state, setState] = useState({
    ...DEFAULT_SITE_STEP_STATE,
    ...initial,
  });
  return <StepSite state={state} onChange={setState} />;
}

describe('StepSite screen-reader validation', () => {
  it('associates every invalid field with its stable localized error', () => {
    render(
      <ControlledSite
        initial={{
          latitudeDegText: '200',
          longitudeDegText: '999',
          elevationMText: 'high',
        }}
      />,
    );

    const name = screen.getByRole('textbox', { name: 'Name' });
    const latitude = screen.getByRole('textbox', { name: 'Latitude (°)' });
    const longitude = screen.getByRole('textbox', { name: 'Longitude (°)' });
    const elevation = screen.getByRole('textbox', {
      name: 'Elevation (m, optional)',
    });

    expect(name).toHaveAttribute('aria-invalid', 'true');
    expect(name).toHaveAttribute('aria-describedby', 'setup-site-name-error');
    expect(latitude).toHaveAttribute('aria-invalid', 'true');
    expect(latitude).toHaveAttribute(
      'aria-describedby',
      'setup-site-lat-error',
    );
    expect(longitude).toHaveAttribute('aria-invalid', 'true');
    expect(longitude).toHaveAttribute(
      'aria-describedby',
      'setup-site-lon-error',
    );
    expect(elevation).toHaveAttribute('aria-invalid', 'true');
    expect(elevation).toHaveAttribute(
      'aria-describedby',
      'setup-site-elevation-error',
    );

    expect(document.getElementById('setup-site-name-error')).toHaveTextContent(
      'Name is required.',
    );
    expect(document.getElementById('setup-site-lat-error')).toHaveTextContent(
      'Latitude must be a number between -90 and 90.',
    );
    expect(document.getElementById('setup-site-lon-error')).toHaveTextContent(
      'Longitude must be a number between -180 and 180.',
    );
    expect(
      document.getElementById('setup-site-elevation-error'),
    ).toHaveTextContent('Elevation must be a number (in metres).');
  });

  it('keeps focus on the edited field when validation appears', () => {
    render(
      <ControlledSite initial={{ name: 'Backyard', longitudeDegText: '10' }} />,
    );
    const latitude = screen.getByRole('textbox', { name: 'Latitude (°)' });

    latitude.focus();
    fireEvent.change(latitude, { target: { value: '200' } });

    expect(latitude).toHaveFocus();
    expect(latitude).toHaveAttribute('aria-invalid', 'true');
  });
});
