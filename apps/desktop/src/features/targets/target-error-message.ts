// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Maps a target-operation `ContractError.code` to a user-readable, localized
 * message.
 *
 * Extracted from `TargetDetail.tsx` so the mapping can be unit-tested directly
 * (see `target-identity.test.ts`) instead of via a hand-copied mirror that
 * silently drifts from production.
 */

import { m } from '@/lib/i18n';
import type { ContractError } from '@/lib/errors';
import { ERROR_MESSAGES } from '@/lib/error-messages';
import type { ErrorCode } from '@/bindings/index';

/**
 * Map a target `ContractError.code` to a user-readable, localized message.
 * Target-specific wording wins for codes this switch names; any other known
 * code falls through to the shared errors.ts catalog before `fallback`, so
 * consolidation doesn't regress coverage for codes this switch never named.
 */
export function errorMessage(err: ContractError, fallback: string): string {
  switch (err.code) {
    case 'alias.blank':
      return m.targets_detail_alias_blank();
    case 'alias.not_found':
      return m.targets_detail_alias_not_found();
    case 'alias.not_removable':
      return m.targets_detail_alias_not_removable();
    case 'target.not_found':
      return m.targets_detail_target_not_found();
    case 'target.invalid_id':
      return m.targets_detail_invalid_target_id();
    case 'note.content_too_large':
      return m.err_note_content_too_large();
    default: {
      const resolve = ERROR_MESSAGES[err.code as ErrorCode] as
        | (() => string)
        | undefined;
      return resolve ? resolve() : fallback;
    }
  }
}
