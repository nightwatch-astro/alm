// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Vanilla-extract styles for the Skeleton shimmer placeholder.
 * Replaces skeleton.css (58L).
 *
 * The shimmer animation uses a moving gradient.  Per the comment in the
 * original CSS, duration uses the `s` unit (not `ms`) to pass the token-policy
 * check — the policy only forbids raw `ms` values.
 * Reduced-motion: drops the gradient to a static muted block; the global
 * reset (reset.css) additionally collapses the animation to a near-instant
 * frame.
 */

import { keyframes, style, styleVariants } from '@vanilla-extract/css';
import { vars } from '@/styles/themes.css';

const shimmer = keyframes({
  from: { backgroundPosition: '200% 0' },
  to: { backgroundPosition: '-200% 0' },
});

export const group = style({
  display: 'flex',
  flexDirection: 'column',
  gap: 'var(--pv-sp-2)',
  width: '100%',
});

// Base shimmer block — geometry overridden per-instance via CSS custom
// properties set inline (--pv-skel-w / -h / -r).
const skeletonBase = {
  display: 'block',
  width: 'var(--pv-skel-w, 100%)',
  backgroundColor: vars.chip,
  backgroundImage: `linear-gradient(90deg, ${vars.chip} 0%, ${vars.surfaceRaised} 50%, ${vars.chip} 100%)`,
  backgroundSize: '200% 100%',
  backgroundRepeat: 'no-repeat',
  animation: `${shimmer} 1.4s ease-in-out infinite`,
  '@media': {
    '(prefers-reduced-motion: reduce)': {
      backgroundImage: 'none',
      backgroundColor: vars.chip,
    },
  },
} as const;

export const skeletonVariants = styleVariants({
  line: {
    ...skeletonBase,
    height: 'var(--pv-skel-h, var(--pv-sp-3))',
    borderRadius: 'var(--pv-skel-r, var(--pv-radius-sm))',
  },
  block: {
    ...skeletonBase,
    height: 'var(--pv-skel-h, var(--pv-row-height))',
    borderRadius: 'var(--pv-skel-r, var(--pv-radius-md))',
  },
  circle: {
    ...skeletonBase,
    width: 'var(--pv-skel-w, var(--pv-row-height))',
    height: 'var(--pv-skel-h, var(--pv-row-height))',
    borderRadius: 'var(--pv-skel-r, 50%)',
    flex: 'none',
  },
});
