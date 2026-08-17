// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * EvidenceSeverityPill — icon + text severity indicator.
 *
 * FR-094 requires every review surface to convey severity with visible text
 * AND an accessibility-named icon, not colour alone.
 */

import { Pill } from '@/ui';
import type { PillVariant } from '@/ui';
import { m } from '@/lib/i18n';

export type EvidenceSeverity = 'ok' | 'yellow' | 'red' | 'missing';

function severityToPillVariant(s: EvidenceSeverity): PillVariant {
  if (s === 'yellow') return 'warn';
  if (s === 'red') return 'danger';
  if (s === 'missing') return 'info';
  return 'ok';
}

function severityLabel(s: EvidenceSeverity): string {
  switch (s) {
    case 'ok':
      return m.evidence_severity_ok();
    case 'yellow':
      return m.evidence_severity_yellow();
    case 'red':
      return m.evidence_severity_red();
    case 'missing':
      return m.evidence_severity_missing();
  }
}

function severityIcon(s: EvidenceSeverity): string {
  switch (s) {
    case 'ok':
      return '✓';
    case 'yellow':
      return '⚠';
    case 'red':
      return '✗';
    case 'missing':
      return '?';
  }
}

export interface EvidenceSeverityPillProps {
  severity: EvidenceSeverity;
  /** Optional additional label appended after the severity text. */
  detail?: string;
}

export function EvidenceSeverityPill({
  severity,
  detail,
}: EvidenceSeverityPillProps) {
  const label = severityLabel(severity);
  const icon = severityIcon(severity);
  const variant = severityToPillVariant(severity);
  return (
    <Pill
      variant={variant}
      aria-label={detail ? `${label}: ${detail}` : label}
      data-testid="evidence-severity-pill"
    >
      <span aria-hidden="true">{icon}</span> {label}
      {detail && <span className="pv-pill__detail"> {detail}</span>}
    </Pill>
  );
}
