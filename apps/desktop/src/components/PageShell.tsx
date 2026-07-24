// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import type { ReactNode } from 'react';
import { page } from '@/ui/page-layout.css';

export interface PageShellProps {
  children: ReactNode;
}

export function PageShell({ children }: PageShellProps) {
  return <div className={page}>{children}</div>;
}
