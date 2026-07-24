// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import { forwardRef } from 'react';
import type { CSSProperties, HTMLAttributes } from 'react';
import { m } from '@/lib/i18n';
import { group, skeletonVariants } from './skeleton.css';

export type SkeletonVariant = 'line' | 'block' | 'circle';

export interface SkeletonProps
  extends Omit<HTMLAttributes<HTMLDivElement>, 'children'> {
  /**
   * Placeholder shape: `line` (thin text line), `block` (list row / card), or
   * `circle` (avatar / icon). Default `line`.
   */
  variant?: SkeletonVariant;
  /** Width — number is px, string is any CSS length. Falls back to the variant default. */
  width?: number | string;
  /** Height — number is px, string is any CSS length. Falls back to the variant default. */
  height?: number | string;
  /** Corner radius (CSS length or token). Overrides the variant default. */
  radius?: string;
  /** Repeated placeholder blocks — e.g. list rows or text lines. Default 1. */
  count?: number;
  /** Accessible loading label announced by assistive tech. Defaults to the
   * shared `common_loading` catalog message, evaluated per render so it follows
   * the active locale. */
  label?: string;
}

const len = (v: number | string | undefined): string | undefined =>
  v == null ? undefined : typeof v === 'number' ? `${v}px` : v;

/**
 * Token-styled shimmer placeholder shown while real content loads. Renders a
 * `role="status"` group (accessible loading semantic) wrapping `count`
 * decorative blocks. Per-instance geometry is passed as CSS custom properties
 * so shape/motion stay in skeleton.css.ts; the shimmer is disabled under
 * `prefers-reduced-motion`, leaving a plain muted block.
 */
export const Skeleton = forwardRef<HTMLDivElement, SkeletonProps>(
  function Skeleton(
    {
      variant = 'line',
      width,
      height,
      radius,
      count = 1,
      label = m.common_loading(),
      className,
      style,
      ...rest
    },
    ref,
  ) {
    const cssVars: CSSProperties = {
      ...style,
      ...(len(width) ? { ['--pv-skel-w' as string]: len(width) } : null),
      ...(len(height) ? { ['--pv-skel-h' as string]: len(height) } : null),
      ...(radius ? { ['--pv-skel-r' as string]: radius } : null),
    };
    return (
      <div
        ref={ref}
        className={[group, className].filter(Boolean).join(' ')}
        role="status"
        aria-busy="true"
        aria-label={label}
        data-testid="skeleton"
        // eslint-disable-next-line no-restricted-syntax -- dynamic: per-instance skeleton geometry passed as CSS custom properties
        style={cssVars}
        {...rest}
      >
        {Array.from({ length: Math.max(1, count) }, (_, i) => (
          <span
            key={i}
            className={skeletonVariants[variant]}
            aria-hidden="true"
          />
        ))}
      </div>
    );
  },
);
Skeleton.displayName = 'Skeleton';
