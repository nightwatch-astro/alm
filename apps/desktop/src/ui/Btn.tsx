// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { forwardRef } from 'react';
import type { ReactNode, ButtonHTMLAttributes } from 'react';
import { sizeVariants, variantStyles } from './Btn.css';

export type BtnVariant = 'primary' | 'danger' | 'destructive' | 'ghost';
export type BtnSize = 'xs' | 'sm' | 'md';
export interface BtnProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: BtnVariant;
  size?: BtnSize;
  children: ReactNode;
}

export const Btn = forwardRef<HTMLButtonElement, BtnProps>(function Btn(
  { variant, size = 'md', className, children, ...rest },
  ref,
) {
  const sizeKey = size === 'md' ? 'default' : size;
  const cls = [
    sizeVariants[sizeKey],
    variant ? variantStyles[variant] : undefined,
    className,
  ]
    .filter(Boolean)
    .join(' ');
  return (
    <button ref={ref} className={cls} data-variant={variant ?? "default"} {...rest}>
      {children}
    </button>
  );
});
Btn.displayName = 'Btn';
