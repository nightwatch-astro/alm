// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * PropertyTable — key-value property display supporting both read-only and
 * editable modes. Used for session details, inbox review, and equipment.
 *
 * Value rendering goes through the shared missing-value renderer (spec-030
 * Q16 / FR-135–FR-138, `@/components/RenderValue`): a real value (including
 * a real 0/false/"") renders plain, a missing-but-applicable value renders
 * the muted unresolved chip, and a not-applicable value renders blank — the
 * source badge only ever appears next to a real value (FR-138). Each
 * `PropertyDef` carries an explicit `applicability` marker (default
 * `'applicable'`) rather than overloading `value === null`, so a caller that
 * knows a field genuinely doesn't apply to the entity (per the
 * data-model.md field-applicability matrix) states that, instead of the
 * renderer guessing from data absence (FR-135).
 *
 * Uses @base-ui-components/react/checkbox for confirm toggles.
 */

import { Checkbox } from '@base-ui-components/react/checkbox';
import { Select } from '@base-ui-components/react/select';
import { Tooltip } from '@/ui';
import { m } from '@/lib/i18n';
import * as pt from './PropertyTable.css';
import { vars } from '@/styles/themes.css';
import {
  renderValueOnly,
  SourceBadge,
  valueState,
  type FieldApplicability,
  type ValueSource,
} from './RenderValue';
import { selectBase } from '@/styles/select.css';

export interface PropertyDef {
  key: string;
  label: string;
  value: string | number | boolean | null;
  editable?: boolean;
  source?: ValueSource;
  /**
   * Whether this field applies to the entity being shown (Q16 / FR-135).
   * Defaults to `'applicable'` — a `null` value then renders the unresolved
   * chip, not a silent blank. Set `'not_applicable'` when the
   * data-model.md field-applicability matrix says the field doesn't apply
   * to this entity/frame-type (e.g. filter on a dark).
   */
  applicability?: FieldApplicability;
  /**
   * Optional explanation revealed on hover/focus of the VALUE (view mode
   * only), via the shared token-styled `ui/Tooltip`. Mirrored into the
   * trigger's `aria-label` together with the value (the InfoTip pattern) so
   * screen readers get it without a hover.
   */
  tooltip?: string;
  /**
   * Render the value in the monospace stack (spec 055 FR-005) — e.g. RA/Dec
   * coordinate values, where fixed digit width matters for scanability.
   */
  mono?: boolean;
  confirmed?: boolean;
  onConfirmToggle?: () => void;
  onChange?: (newValue: string) => void;
  type?: 'text' | 'number' | 'select' | 'boolean';
  options?: { value: string; label: string }[];
}

export interface PropertyTableProps {
  properties: PropertyDef[];
  mode: 'view' | 'edit';
  showSource?: boolean;
  showConfirm?: boolean;
}

function PropertyValueEditor({ prop }: { prop: PropertyDef }) {
  const { type = 'text', value, onChange, options } = prop;

  if (type === 'select' && options) {
    return (
      <Select.Root
        value={value !== null ? String(value) : ''}
        onValueChange={(v: string | null) => {
          if (v !== null) onChange?.(v);
        }}
      >
        <Select.Trigger className={selectBase} aria-label={prop.label}>
          <Select.Value />
          <Select.Icon className="pv-select__icon" />
        </Select.Trigger>
        <Select.Portal>
          <Select.Positioner>
            <Select.Popup className="pv-select__popup">
              {options.map((opt) => (
                <Select.Item
                  key={opt.value}
                  value={opt.value}
                  className="pv-select__item"
                >
                  <Select.ItemText>{opt.label}</Select.ItemText>
                </Select.Item>
              ))}
            </Select.Popup>
          </Select.Positioner>
        </Select.Portal>
      </Select.Root>
    );
  }

  if (type === 'boolean') {
    return (
      <Checkbox.Root
        className="pv-checkbox"
        checked={value === true}
        onCheckedChange={(checked) => onChange?.(String(checked))}
        aria-label={prop.label}
      >
        <Checkbox.Indicator className="pv-checkbox__indicator">
          &#x2713;
        </Checkbox.Indicator>
      </Checkbox.Root>
    );
  }

  return (
    <input
      type={type === 'number' ? 'number' : 'text'}
      className="pv-input pv-input--sm"
      value={value !== null && value !== undefined ? String(value) : ''}
      onChange={(e) => onChange?.(e.target.value)}
      aria-label={prop.label}
    />
  );
}

export function PropertyTable({
  properties,
  mode,
  showSource = false,
  showConfirm = false,
}: PropertyTableProps) {
  return (
    <div
      className={pt.root}
      data-testid="property-table"
      role="table"
      aria-label={m.cmp_property_table_aria()}
    >
      {/* Header row */}
      <div className={pt.header} role="row">
        <span className={pt.cellLabel} role="columnheader">
          {m.cmp_property_table_col_property()}
        </span>
        <span
          className={pt.cellValue}
          data-testid="property-table-cell-value"
          role="columnheader"
        >
          {m.cmp_property_table_col_value()}
        </span>
        {showSource && (
          <span className={pt.cellSource} role="columnheader">
            {m.projects_wizard_col_source()}
          </span>
        )}
        {showConfirm && (
          <span className={pt.cellConfirm} role="columnheader">
            {m.cmp_property_table_col_confirmed()}
          </span>
        )}
      </div>

      {/* Data rows */}
      {properties.map((prop) => {
        const isEditing = mode === 'edit' && prop.editable;
        const applicability = prop.applicability ?? 'applicable';
        const state = valueState(prop.value, applicability);

        return (
          <div key={prop.key} className={pt.row} role="row">
            <span className={pt.cellLabel} role="rowheader">
              {prop.label}
            </span>

            <span
              className={pt.cellValue}
              // mono: inline font-family override — still token-driven via vars.fontMono
              style={
                prop.mono
                  ? { fontFamily: vars.fontMono, fontSize: 'var(--pv-text-xs)' }
                  : undefined
              }
              data-testid="property-table-cell-value"
              role="cell"
            >
              {isEditing ? (
                <PropertyValueEditor prop={prop} />
              ) : prop.tooltip ? (
                <Tooltip
                  content={prop.tooltip}
                  // `tabIndex` makes the tooltip reachable without a pointer
                  // (the shared Tooltip trigger is a bare <span>); `role` is
                  // what actually exposes `aria-label` — naming is ignored on
                  // a role-less <span>, so without it assistive tech hears the
                  // value and never the tooltip. Same pairing as `ui/Lock`.
                  role="note"
                  tabIndex={0}
                  data-testid={`proptable-tooltip-${prop.key}`}
                  aria-label={
                    prop.value != null
                      ? `${String(prop.value)} — ${prop.tooltip}`
                      : prop.tooltip
                  }
                >
                  {renderValueOnly(prop.value, { applicability })}
                </Tooltip>
              ) : (
                renderValueOnly(prop.value, { applicability })
              )}
            </span>

            {showSource && (
              <span className={pt.cellSource} role="cell">
                {/* Source pills couple to value presence (FR-138) — never
                    shown for an unresolved or not-applicable field. */}
                {state === 'real' && prop.source && (
                  <SourceBadge source={prop.source} />
                )}
              </span>
            )}

            {showConfirm && (
              <span className={pt.cellConfirm} role="cell">
                {prop.onConfirmToggle !== undefined && (
                  <Checkbox.Root
                    className="pv-checkbox"
                    checked={prop.confirmed ?? false}
                    onCheckedChange={() => prop.onConfirmToggle?.()}
                    aria-label={m.proptable_confirm_aria({ label: prop.label })}
                  >
                    <Checkbox.Indicator className="pv-checkbox__indicator">
                      &#x2713;
                    </Checkbox.Indicator>
                  </Checkbox.Root>
                )}
              </span>
            )}
          </div>
        );
      })}
    </div>
  );
}
