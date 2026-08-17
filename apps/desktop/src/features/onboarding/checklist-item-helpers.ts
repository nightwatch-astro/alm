// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// Pure item-state helpers shared by ChecklistSection, ChecklistGroup, and
// useChecklistGroups. Extracted to break the mutual import cycle created when
// ChecklistGroup was split from ChecklistSection (kyo7.104 refactor).

import type { OnboardingItemDto } from '@/bindings/index';

export function isChecklistGroupSettled(items: OnboardingItemDto[]): boolean {
  return items.length > 0 && items.every((item) => item.state !== 'unchecked');
}

export function completedChecklistItems(
  items: OnboardingItemDto[],
): OnboardingItemDto[] {
  return items.filter(
    (item) =>
      item.state === 'auto_checked' || item.state === 'manually_checked',
  );
}
