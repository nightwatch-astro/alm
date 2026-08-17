// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { forwardRef } from 'react';
import type { ReactNode, HTMLAttributes } from 'react';
import * as box from './Box.css';

export interface BoxProps extends HTMLAttributes<HTMLDivElement> {
  title?: string;
  children: ReactNode;
}

export const Box = forwardRef<HTMLDivElement, BoxProps>(function Box(
  { title, className, children, ...rest },
  ref,
) {
  const cls = [box.root, className].filter(Boolean).join(' ');
  return (
    <div ref={ref} className={cls} {...rest}>
      {title && <div className={box.header}>{title}</div>}
      <div className={box.body}>{children}</div>
    </div>
  );
});
Box.displayName = 'Box';
