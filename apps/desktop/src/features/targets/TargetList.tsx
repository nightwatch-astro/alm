// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { useMemo, useRef, useState } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import type { TargetListItem, TargetObjectType } from '@/bindings/index';
import { ListSidebar } from '@/components';
import { Pill, SegControl } from '@/ui';
import { m } from '@/lib/i18n';
import { objectTypeLabel } from '@/components/TargetSearch/objectType';
import { virtualInner } from '@/ui/page-layout.css';
import { listItem, listItemSelected } from '@/components/ListItem.css';

/**
 * Estimated row height (px) for the virtualizer's initial measurement.
 *
 * Dense  — single-line: label + type pill ≈ 34px
 * Rich   — two-line: label row + meta row ≈ 54px
 *
 * NOTE: TargetListItem only carries effectiveLabel, primaryDesignation, and
 * objectType. Fields shown in the authoritative mock (CON, COORDS, MAG,
 * COVERAGE, best-season, sessions) live on the full-detail endpoint and are
 * NOT available on list rows. They are omitted here rather than fabricated.
 */
const ROW_ESTIMATE_DENSE = 34;
const ROW_ESTIMATE_RICH = 54;

type RowDensity = 'Dense' | 'Rich';

interface Props {
  targets: TargetListItem[];
  selected: string | null;
  onSelect: (id: string) => void;
}

function matchesSearch(t: TargetListItem, query: string): boolean {
  const q = query.toLowerCase();
  return (
    t.primaryDesignation.toLowerCase().includes(q) ||
    t.effectiveLabel.toLowerCase().includes(q)
  );
}

/** Formats the objectType string into a readable, localized label. */
function formatType(objectType: string): string {
  return objectTypeLabel(objectType as TargetObjectType);
}

export function TargetList({ targets, selected, onSelect }: Props) {
  const [search, setSearch] = useState('');
  const [density, setDensity] = useState<RowDensity>('Dense');
  const scrollRef = useRef<HTMLDivElement>(null);

  const filtered = useMemo(
    () =>
      search.trim()
        ? targets.filter((t) => matchesSearch(t, search.trim()))
        : targets,
    [targets, search],
  );

  const rowEstimate =
    density === 'Dense' ? ROW_ESTIMATE_DENSE : ROW_ESTIMATE_RICH;

  const virtualizer = useVirtualizer({
    count: filtered.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowEstimate,
    overscan: 8,
  });

  return (
    <ListSidebar
      scrollRef={scrollRef}
      virtualized
      placeholder={m.targets_page_search_placeholder()}
      searchValue={search}
      onSearchChange={setSearch}
      controls={
        <>
          <SegControl
            options={[
              { value: 'Dense', label: m.targets_legacy_density_dense() },
              { value: 'Rich', label: m.targets_legacy_density_rich() },
            ]}
            value={density}
            onChange={(v) => setDensity(v as RowDensity)}
            aria-label={m.targets_legacy_row_density_aria()}
          />
          <select defaultValue="name">
            <option value="name">{m.targets_legacy_sort_name()}</option>
          </select>
        </>
      }
      footer={m.common_item_count({ count: filtered.length })}
    >
      <div
        className={virtualInner}
        // eslint-disable-next-line no-restricted-syntax -- dynamic: virtualizer total height (getTotalSize)
        style={{
          height: `${virtualizer.getTotalSize()}px`,
          position: 'relative',
        }}
      >
        {virtualizer.getVirtualItems().map((virtualRow) => {
          const t = filtered[virtualRow.index];
          const isSelected = selected === t.id;
          const showAltDesig = t.effectiveLabel !== t.primaryDesignation;

          return (
            <div
              key={t.id}
              data-index={virtualRow.index}
              ref={virtualizer.measureElement}
              className={[listItem, isSelected ? listItemSelected : undefined]
                .filter(Boolean)
                .join(' ')}
              role="button"
              tabIndex={0}
              aria-label={m.targets_list_view_aria({ label: t.effectiveLabel })}
              onClick={() => onSelect(t.id)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  onSelect(t.id);
                }
              }}
              // eslint-disable-next-line no-restricted-syntax -- dynamic: virtualizer translateY offset per target row
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                right: 0,
                transform: `translateY(${virtualRow.start}px)`,
              }}
            >
              {density === 'Dense' ? (
                /* ── Dense: single-line ─────────────────────────────── */
                <div className="pv-target-row">
                  <span className="pv-target-row__label">
                    {t.effectiveLabel}
                  </span>
                  {showAltDesig && (
                    <span className="pv-target-row__desig">
                      ({t.primaryDesignation})
                    </span>
                  )}
                  <span className="pv-target-row__spacer" />
                  <Pill variant="ghost">{formatType(t.objectType)}</Pill>
                </div>
              ) : (
                /* ── Rich: two-line ──────────────────────────────────── */
                <div className="pv-target-row pv-target-row--rich">
                  <div className="pv-target-row__line1">
                    <span className="pv-target-row__label">
                      {t.effectiveLabel}
                    </span>
                    {showAltDesig && (
                      <span className="pv-target-row__desig">
                        {t.primaryDesignation}
                      </span>
                    )}
                    <span className="pv-target-row__spacer" />
                  </div>
                  <div className="pv-target-row__line2">
                    <span className="pv-target-row__type-label">
                      {formatType(t.objectType)}
                    </span>
                    {/* CON · COORDS · MAG · best-season · sessions omitted:
                        not present on TargetListItem (detail endpoint only) */}
                  </div>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </ListSidebar>
  );
}
