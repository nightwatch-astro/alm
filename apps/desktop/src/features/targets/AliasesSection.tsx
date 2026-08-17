// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * AliasesSection — alias list + add-alias form for a target.
 *
 * Extracted from TargetDetail.tsx.
 */

import { X } from 'lucide-react';
import { Pill, Section, Banner } from '@/ui';
import { m } from '@/lib/i18n';
import { kindLabel } from './target-detail-format';

export interface AliasEntry {
  id: string;
  alias: string;
  kind: string;
}

export interface AliasesSectionProps {
  aliases: AliasEntry[];
  aliasInput: string;
  setAliasInput: (v: string) => void;
  aliasError: string | null;
  actionError: string | null;
  onAdd: () => void;
  onRemove: (id: string) => void;
}

export function AliasesSection({
  aliases,
  aliasInput,
  setAliasInput,
  aliasError,
  actionError,
  onAdd,
  onRemove,
}: AliasesSectionProps) {
  return (
    <Section title={m.common_aliases()} count={aliases.length}>
      <div className="pv-target-detail__alias-list">
        {aliases.map((a) => (
          <Pill key={a.id} variant={a.kind === 'user' ? 'accent' : 'ghost'}>
            <span title={m.targets_detail_alias_kind_title({ kind: a.kind })}>
              <span className="pv-target-detail__alias-kind">
                [{kindLabel(a.kind)}]
              </span>
              {a.alias}
            </span>
            {a.kind === 'user' && (
              <button
                aria-label={m.targets_detail_alias_remove_aria({
                  alias: a.alias,
                })}
                className="pv-target-detail__alias-remove"
                onClick={() => onRemove(a.id)}
              >
                <X size={14} aria-hidden="true" />
              </button>
            )}
          </Pill>
        ))}
        {aliases.length === 0 && (
          <span className="pv-target-detail__alias-empty">
            {m.targets_detail_no_aliases()}
          </span>
        )}
      </div>

      {/* Add user alias form */}
      <div className="pv-target-detail__alias-add-row">
        <input
          aria-label={m.targets_detail_alias_input_aria()}
          placeholder={m.targets_detail_alias_placeholder()}
          value={aliasInput}
          onChange={(e) => setAliasInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') void onAdd();
          }}
          className="pv-target-detail__text-input"
        />
        <button onClick={onAdd} className="pv-target-detail__action-btn">
          {m.common_add()}
        </button>
      </div>
      {aliasError && (
        <Banner variant="danger" className="pv-target-detail__banner">
          {aliasError}
        </Banner>
      )}
      {actionError && (
        <Banner variant="danger" className="pv-target-detail__banner">
          {actionError}
        </Banner>
      )}
    </Section>
  );
}
