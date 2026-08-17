// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { forwardRef } from 'react';
import type { ReactNode, HTMLAttributes } from 'react';
import * as kv from './KV.css';
import { uvars, vars } from '@/styles/themes.css';

export interface KVProps extends HTMLAttributes<HTMLDivElement> {
  label: string;
  value: ReactNode;
  provenance?: string;
  mono?: boolean;
}

export const KV = forwardRef<HTMLDivElement, KVProps>(function KV(
  { label, value, provenance, mono, className, ...rest },
  ref,
) {
  const cls = [kv.row, className].filter(Boolean).join(' ');
  return (
    <div ref={ref} className={cls} {...rest}>
      <span className={kv.label}>{label}</span>
      <span
        className={kv.value}
        // eslint-disable-next-line no-restricted-syntax -- dynamic: conditional mono override uses token values, no static equivalent
        style={
          mono
            ? { fontFamily: vars.fontMono, fontSize: uvars.textXs }
            : undefined
        }
      >
        {value}
        {provenance && <span className={kv.provenance}>{provenance}</span>}
      </span>
    </div>
  );
});
KV.displayName = 'KV';
