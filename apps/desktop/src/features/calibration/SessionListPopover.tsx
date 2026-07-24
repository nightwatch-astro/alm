// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * SessionListPopover — compact anchored pop-out for a list of session names.
 *
 * Shows a count trigger ("3 ▾") that opens a base-ui Popover with a search
 * input and scrollable filtered list. Zero names → plain "None" (no trigger).
 * Portaled so it is never clipped by the detail panel's overflow.
 */

import { Popover } from '@base-ui-components/react/popover';
import { useId, useRef, useState } from 'react';
import { DetailLinkedGroup } from '@/components';
import { m } from '@/lib/i18n';
import {
  trigger,
  popup,
  search,
  list,
  item,
  empty,
} from './session-list-popover.css';

interface Props {
  label: string;
  names: string[];
}

export function SessionListPopover({ label, names }: Props) {
  const [query, setQuery] = useState('');
  const inputId = useId();
  const inputRef = useRef<HTMLInputElement>(null);

  const filtered = query.trim()
    ? names.filter((n) => n.toLowerCase().includes(query.toLowerCase()))
    : names;

  return (
    // #813: shared DetailLinkedGroup (head + muted-or-content) instead of
    // hand-rolling the `pv-session-detail2__head`/`__muted` classes here.
    <DetailLinkedGroup
      label={label}
      empty={names.length === 0}
      emptyLabel={m.common_none()}
    >
      <Popover.Root
        onOpenChange={(open) => {
          setQuery('');
          if (open) {
            // Focus the search input after the popup mounts.
            setTimeout(() => inputRef.current?.focus(), 0);
          }
        }}
      >
        <Popover.Trigger className={trigger}>{names.length} ▾</Popover.Trigger>
        <Popover.Portal>
          <Popover.Positioner side="bottom" align="start" sideOffset={4}>
            <Popover.Popup className={popup}>
              {}
              <label htmlFor={inputId} className="pv-visually-hidden">
                {m.calibration_session_popover_filter_label()}
              </label>
              <input
                ref={inputRef}
                id={inputId}
                className={search}
                placeholder={m.calibration_session_popover_filter_placeholder()}
                aria-label={m.calibration_session_popover_filter_label()}
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
              <ul className={list}>
                {filtered.length === 0 ? (
                  <li className={empty}>
                    {m.calibration_session_popover_no_matches()}
                  </li>
                ) : (
                  filtered.map((name) => (
                    <li key={name} className={item}>
                      {name}
                    </li>
                  ))
                )}
              </ul>
            </Popover.Popup>
          </Popover.Positioner>
        </Popover.Portal>
      </Popover.Root>
    </DetailLinkedGroup>
  );
}
