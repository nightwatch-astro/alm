// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { forwardRef } from 'react';
import type { ReactNode, HTMLAttributes } from 'react';
import { variantStyles } from './Banner.css';

export type BannerVariant = 'warn' | 'danger' | 'info';
export interface BannerProps extends HTMLAttributes<HTMLDivElement> {
  variant?: BannerVariant;
  children: ReactNode;
}

export const Banner = forwardRef<HTMLDivElement, BannerProps>(function Banner(
  {
    variant = 'warn',
    className,
    children,
    role,
    'aria-live': ariaLive,
    ...rest
  },
  ref,
) {
  const cls = [variantStyles[variant], className].filter(Boolean).join(' ');
  const defaultRole =
    variant === 'danger' ? 'alert' : variant === 'warn' ? 'status' : undefined;
  return (
    <div
      ref={ref}
      className={cls}
      role={role ?? defaultRole}
      aria-live={ariaLive ?? (variant === 'warn' ? 'polite' : undefined)}
      {...rest}
    >
      {children}
    </div>
  );
});
Banner.displayName = 'Banner';
