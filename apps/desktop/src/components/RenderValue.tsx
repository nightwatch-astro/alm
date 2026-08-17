// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Shared missing-value renderer (spec-030 Q16 / #620, #619, FR-135–FR-138).
 *
 * Every displayed metadata field is one of three states, modeled — never
 * inferred at render time from `value` alone:
 *   - "real": data exists, including a real `0` / `false` / `""`.
 *   - "unresolved": the field applies to this entity but no data exists.
 *   - "not_applicable": the field does not apply to this entity (e.g. filter
 *     on a dark, set-temp on a flat) — determined by the caller from the
 *     data-model.md field-applicability matrix, never guessed here.
 *
 * `renderValue(value, { source, applicability })` is the single composite
 * entry point for inline contexts (table cells, meta lines) where the value
 * and its source pill render together in one cell. `PropertyTable` has a
 * separate value/source column layout that predates this module, so it
 * composes the same primitives (`valueState`, `UnresolvedChip`,
 * `SourceBadge`) directly instead of the composite wrapper — both paths
 * share one state-classification rule and one chip/badge implementation, so
 * there is still exactly one place absence semantics are decided.
 */

import type { ReactNode } from 'react';
import { Pill } from '@/ui';
import { m } from '@/lib/i18n';
import * as pt from './PropertyTable.css';

export type FieldApplicability = 'applicable' | 'not_applicable';
export type ValueSource = 'fits' | 'user' | 'inferred' | 'default';
export type ValueState = 'real' | 'unresolved' | 'not_applicable';

export interface RenderValueOptions {
  source?: ValueSource;
  applicability: FieldApplicability;
}

/** Blank marker for not-applicable fields (FR-137) — never a chip. */
export const NOT_APPLICABLE_DISPLAY = '—';

/**
 * Classify a field value into its Q16 state. `applicability` is the caller's
 * authoritative input (data-model.md matrix) — this function never derives
 * not-applicable from `value` being absent (FR-135).
 */
export function valueState(
  value: unknown,
  applicability: FieldApplicability,
): ValueState {
  if (applicability === 'not_applicable') return 'not_applicable';
  return value === null || value === undefined ? 'unresolved' : 'real';
}

const SOURCE_LABELS: Record<ValueSource, () => string> = {
  fits: m.cmp_source_fits,
  user: m.cmp_source_user,
  inferred: m.cmp_source_inferred,
  default: m.cmp_source_default,
};

/**
 * Provenance badge for a present value. Source pills couple to value
 * presence (FR-138) — callers MUST NOT render this for a missing/absent
 * value; `renderValue` enforces that automatically.
 */
export function SourceBadge({ source }: { source: ValueSource }) {
  return (
    <span
      className={
        source === 'fits'
          ? pt.sourceBadgeFits
          : source === 'user'
            ? pt.sourceBadgeUser
            : source === 'inferred'
              ? pt.sourceBadgeInferred
              : source === 'default'
                ? pt.sourceBadgeDefault
                : pt.sourceBadge
      }
    >
      {SOURCE_LABELS[source]()}
    </span>
  );
}

/**
 * Muted chip marking an applicable-but-unresolved field value. Distinct from
 * both a real value (which gets a source pill, never this chip) and a
 * not-applicable field (which renders blank, never this chip).
 */
export function UnresolvedChip() {
  return (
    <Pill variant="ghost" data-testid="unresolved-chip">
      {m.cmp_unresolved_chip()}
    </Pill>
  );
}

function defaultFormat(value: string | number | boolean): string {
  if (typeof value === 'boolean') return value ? m.common_yes() : m.common_no();
  return String(value);
}

/**
 * The value-only node for a field: the formatted value for "real", the
 * unresolved chip for "unresolved", or the blank marker for "not_applicable"
 * — no source pill. Used by contexts (PropertyTable's value column) that
 * render the source badge in a separate slot.
 */
export function renderValueOnly(
  value: string | number | boolean | null | undefined,
  { applicability }: Pick<RenderValueOptions, 'applicability'>,
  format: (value: string | number | boolean) => string = defaultFormat,
): ReactNode {
  const state = valueState(value, applicability);
  if (state === 'not_applicable') return NOT_APPLICABLE_DISPLAY;
  if (state === 'unresolved') return <UnresolvedChip />;
  return format(value as string | number | boolean);
}

/**
 * Single shared rendering path for a metadata field value (FR-137): the
 * value plus its source pill when real, the unresolved chip when missing
 * (never a source pill, never 0), or a blank marker when not-applicable
 * (never a chip). This is the composite renderer for inline contexts (table
 * cells, meta-line tokens) where value + source render together.
 */
export function renderValue(
  value: string | number | boolean | null | undefined,
  { source, applicability }: RenderValueOptions,
  format: (value: string | number | boolean) => string = defaultFormat,
): ReactNode {
  const state = valueState(value, applicability);
  if (state === 'not_applicable') return NOT_APPLICABLE_DISPLAY;
  if (state === 'unresolved') return <UnresolvedChip />;
  return (
    <>
      {format(value as string | number | boolean)}
      {source && <SourceBadge source={source} />}
    </>
  );
}
