// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { usePreference } from '@/data/preferences';
import { useNavigate } from '@tanstack/react-router';
import { useEffect, useState } from 'react';
import { checkFirstRunComplete } from '@/app/first-run';
import { m } from '@/lib/i18n';
import { SetupWizard } from './SetupWizard';
import { page } from '@/ui/page-layout.css';

export function SetupPage() {
  const [setupCompleted] = usePreference('setupCompleted');
  const [density] = usePreference('density');
  const navigate = useNavigate();
  const [checking, setChecking] = useState(!setupCompleted);

  useEffect(() => {
    if (setupCompleted) {
      void navigate({ to: '/inbox' });
      return;
    }

    // Single source of truth for the first-run gate (also used by the index
    // route) — handles mock mode, the backend round-trip, and the fallback to
    // the cached preference internally.
    let cancelled = false;
    void checkFirstRunComplete().then((complete) => {
      if (cancelled) return;
      if (complete) {
        void navigate({ to: '/inbox' });
      } else {
        setChecking(false);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [setupCompleted, navigate]);

  if (setupCompleted || checking) {
    return (
      <div className={`${page} pv-setup-page__loading`}>
        {m.common_loading()}
      </div>
    );
  }

  return (
    // density-* mirrors Shell.tsx's `.pv-frame` class: the wizard renders
    // outside the Shell, so it needs its own carrier for a live density
    // preview during setup (#505).
    <div className={`${page} density-${density}`} data-testid="setup-page">
      <SetupWizard />
    </div>
  );
}
